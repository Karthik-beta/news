//! FRED / ALFRED (Federal Reserve Bank of St. Louis).
//!
//! * Discovery: `series/vintagedates` per tracked series (the dates its data changed) since the
//!   cursor, and `release/dates` with future dates for Diary's calendar.
//! * Fetch: `series` (title, units, frequency, seasonal adjustment as FRED states them) and
//!   `series/observations` over the full real-time window, so every vintage of each recent
//!   observation arrives with its own `realtime_start`.
//! * Parse: one data point per (observation date, vintage); `"."` is a missing observation. The
//!   vintage is `realtime_start`, so a revision is a new point that supersedes the old one.
//!
//! The free key (`FRED_API_KEY`) is added by the Runner; URLs here never carry it.

use std::collections::BTreeMap;

use chrono::{Datelike, Months, NaiveDate};
use serde::Deserialize;
use serde_json::json;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::util::{date, day_vintage, find, month_bounds, months_bounds, shape, value_err};
use crate::{
    CalendarEntry, CandidateDraft, Connector, Cx, Discovery, EntityRef, ItemRef, NeedsHuman,
    ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "fred";
pub const SOURCE_ID: &str = "fred";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.stlouisfed.org/fred/";
/// Start of FRED's real-time axis: asking from here returns every vintage.
const REALTIME_ALL: &str = "realtime_start=1776-07-04&realtime_end=9999-12-31";
/// How far back observations are fetched before the vintage being captured.
const HISTORY_YEARS: u32 = 2;

/// Tracked series: the event-key family and the FRED release that publishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FredSeries {
    pub id: &'static str,
    pub family: &'static str,
    pub release_id: u32,
    /// Months between the reference period and its release (CPI for October comes out in November).
    pub release_lag_months: u32,
}

pub const SERIES: &[FredSeries] = &[
    FredSeries {
        id: "CPIAUCSL",
        family: "cpi",
        release_id: 10,
        release_lag_months: 1,
    },
    FredSeries {
        id: "PAYEMS",
        family: "payrolls",
        release_id: 50,
        release_lag_months: 1,
    },
    FredSeries {
        id: "UNRATE",
        family: "unemployment",
        release_id: 50,
        release_lag_months: 1,
    },
];

pub fn series(id: &str) -> Option<&'static FredSeries> {
    SERIES.iter().find(|s| s.id == id)
}

pub struct Fred;

#[derive(Deserialize)]
struct VintageDates {
    vintage_dates: Vec<String>,
}

#[derive(Deserialize)]
struct ReleaseDates {
    release_dates: Vec<ReleaseDate>,
}

#[derive(Deserialize)]
struct ReleaseDate {
    release_id: u32,
    date: String,
}

#[derive(Deserialize)]
struct SeriesMeta {
    seriess: Vec<SeriesInfo>,
}

#[derive(Deserialize)]
struct SeriesInfo {
    id: String,
    title: String,
    units: String,
    frequency_short: String,
    seasonal_adjustment_short: String,
}

#[derive(Deserialize)]
struct Observations {
    observations: Vec<Obs>,
}

#[derive(Deserialize)]
struct Obs {
    realtime_start: String,
    realtime_end: String,
    date: String,
    value: String,
}

fn cursor_map(cursor: Option<&str>) -> BTreeMap<String, String> {
    cursor
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

/// The period an observation dated `d` covers, by FRED's frequency code. FRED dates a period by its
/// first day; weekly and daily observations are single dates.
fn period_bounds(d: NaiveDate, freq: &str) -> Option<(NaiveDate, NaiveDate)> {
    match freq {
        "M" => month_bounds(d.year(), d.month()),
        "Q" => months_bounds(d.year(), d.month(), d.month() + 2),
        "SA" => months_bounds(d.year(), d.month(), d.month() + 5),
        "A" => months_bounds(d.year(), 1, 12),
        _ => Some((d, d)),
    }
}

fn reference_period(d: NaiveDate, freq: &str) -> String {
    match freq {
        "M" => format!("{:04}-{:02}", d.year(), d.month()),
        "Q" => format!("{:04}-Q{}", d.year(), (d.month() - 1) / 3 + 1),
        "A" => format!("{:04}", d.year()),
        _ => d.to_string(),
    }
}

impl Connector for Fred {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, cursor: Option<&str>) -> Vec<Request> {
        let seen = cursor_map(cursor);
        let default_since = cx.today - chrono::Duration::days(31);
        let mut out: Vec<Request> = cx
            .fred_series
            .iter()
            .map(|id| {
                let since = seen.get(id).and_then(|d| date(d)).unwrap_or(default_since);
                Request::json(format!(
                    "{BASE}series/vintagedates?series_id={id}&realtime_start={since}&file_type=json"
                ))
                .with_secret("api_key", "FRED_API_KEY")
            })
            .collect();
        let mut releases: Vec<u32> = cx
            .fred_series
            .iter()
            .filter_map(|id| series(id).map(|s| s.release_id))
            .collect();
        releases.sort_unstable();
        releases.dedup();
        out.extend(releases.into_iter().map(|r| {
            Request::json(format!(
                "{BASE}release/dates?release_id={r}&realtime_start={}&realtime_end=9999-12-31&include_release_dates_with_no_data=true&sort_order=asc&file_type=json",
                cx.today
            ))
            .with_secret("api_key", "FRED_API_KEY")
        }));
        out
    }

    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen = cursor_map(cursor);
        let mut out = Discovery::default();
        for id in &cx.fred_series {
            let Some(snap) = snaps.iter().find(|s| {
                s.url.contains("series/vintagedates") && s.url.contains(&format!("series_id={id}&"))
            }) else {
                continue; // 304 or not requested: nothing new
            };
            let v: VintageDates =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.vintage_dates", e))?;
            let last = seen.get(id).cloned().unwrap_or_default();
            let newest = v.vintage_dates.iter().filter(|d| **d > last).max().cloned();
            if let Some(vintage) = newest {
                out.refs.push(ItemRef {
                    source_id: SOURCE_ID.into(),
                    kind: "fred.series".into(),
                    key: id.clone(),
                    url: format!("https://fred.stlouisfed.org/series/{id}"),
                    event_key_hint: None,
                    published_at: date(&vintage).map(day_vintage),
                    title: None,
                    meta: json!({"vintage": vintage}),
                });
                seen.insert(id.clone(), vintage);
            }
        }
        for snap in snaps.iter().filter(|s| s.url.contains("release/dates")) {
            let r: ReleaseDates =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.release_dates", e))?;
            for (i, rd) in r.release_dates.iter().enumerate() {
                let day = date(&rd.date)
                    .ok_or_else(|| value_err(format!("$.release_dates[{i}].date"), "not a date"))?;
                if day < cx.today {
                    continue;
                }
                for s in SERIES.iter().filter(|s| {
                    s.release_id == rd.release_id && cx.fred_series.iter().any(|t| t == s.id)
                }) {
                    let Some(reference) = day.checked_sub_months(Months::new(s.release_lag_months))
                    else {
                        continue;
                    };
                    let Some(key) =
                        eventkey::macro_release("us", s.family, &reference_period(reference, "M"))
                    else {
                        continue;
                    };
                    out.calendar.push(CalendarEntry {
                        source_id: SOURCE_ID.into(),
                        event_key: key,
                        title: format!("FRED release {} ({})", rd.release_id, s.id),
                        expected_day: day,
                        expected_at: None,
                        precision: "day".into(),
                        origin: "api".into(),
                    });
                }
            }
        }
        out.calendar.sort_by(|a, b| a.event_key.cmp(&b.event_key));
        out.calendar.dedup_by(|a, b| a.event_key == b.event_key);
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        let id = &item.key;
        let since = item
            .meta
            .get("vintage")
            .and_then(|v| v.as_str())
            .and_then(date)
            .and_then(|d| d.checked_sub_months(Months::new(12 * HISTORY_YEARS)))
            .map(|d| format!("&observation_start={d}"))
            .unwrap_or_default();
        vec![
            Request::json(format!("{BASE}series?series_id={id}&file_type=json"))
                .with_secret("api_key", "FRED_API_KEY"),
            Request::json(format!(
                "{BASE}series/observations?series_id={id}&{REALTIME_ALL}{since}&file_type=json"
            ))
            .with_secret("api_key", "FRED_API_KEY"),
        ]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let id = &item.key;
        let meta_snap = find(snaps, &format!("fred/series?series_id={id}&"))?;
        let obs_snap = find(snaps, &format!("series/observations?series_id={id}&"))?;
        let meta: SeriesMeta =
            serde_json::from_slice(&meta_snap.bytes).map_err(|e| shape("$.seriess", e))?;
        let info = meta
            .seriess
            .into_iter()
            .find(|s| s.id == *id)
            .ok_or_else(|| shape("$.seriess", format!("series {id} not in the response")))?;
        let obs: Observations =
            serde_json::from_slice(&obs_snap.bytes).map_err(|e| shape("$.observations", e))?;
        let series_key = format!("fred:{id}");
        let us = ("iso3166".to_owned(), "US".to_owned());
        let mut out = Parsed::default();
        if news_ledger::units::classify(&info.units).is_none() {
            out.needs_human.push(NeedsHuman {
                reason: format!("unit {:?} is not in the unit vocabulary", info.units),
                series_key: series_key.clone(),
                locator: json!({"kind": "json", "path": "$.seriess[0].units", "series": id}),
            });
        }
        let vintage = item
            .meta
            .get("vintage")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let mut latest: Option<NaiveDate> = None;
        let mut item_points = Vec::new();
        for (i, o) in obs.observations.iter().enumerate() {
            let path = format!("$.observations[{i}].value");
            let locator = json!({
                "kind": "json", "path": path, "series": id,
                "realtime_start": o.realtime_start, "realtime_end": o.realtime_end,
            });
            if is_missing_marker(&o.value) {
                out.missing.push(Missing {
                    series_key: series_key.clone(),
                    period_label: o.date.clone(),
                    value_text: o.value.clone(),
                    locator,
                });
                continue;
            }
            let d = date(&o.date)
                .ok_or_else(|| value_err(format!("$.observations[{i}].date"), "not a date"))?;
            let rs = date(&o.realtime_start).ok_or_else(|| {
                value_err(format!("$.observations[{i}].realtime_start"), "not a date")
            })?;
            let (start, end) = period_bounds(d, &info.frequency_short).ok_or_else(|| {
                value_err(format!("$.observations[{i}].date"), "period out of range")
            })?;
            let point = Draft {
                context: vec![],
                series_key: series_key.clone(),
                entity: Some(us.clone()),
                concept: id.clone(),
                label: info.title.clone(),
                value_text: o.value.clone(),
                unit: info.units.clone(),
                currency: None,
                scale: None,
                decimals: None,
                period_label: o.date.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: Some(info.seasonal_adjustment_short.clone()),
                vintage: day_vintage(rs),
                snapshot_sha: obs_snap.sha256,
                locator,
                parser: self.parser(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if o.realtime_start == vintage {
                item_points.push(point.dp_id.clone());
                latest = latest.max(Some(d));
            }
            out.points.push(point);
        }
        if let (Some(s), Some(d)) = (series(id), latest) {
            out.candidates.push(CandidateDraft {
                event_key: eventkey::macro_release(
                    "us",
                    s.family,
                    &reference_period(d, &info.frequency_short),
                ),
                url: item.url.clone(),
                title: format!("{}, {}", info.title, d),
                published_at: item.published_at,
                rule_key: format!("family:{}", s.family),
                dp_ids: item_points,
                entity: Some(us.clone()),
            });
        }
        out.entities.push(EntityRef {
            scheme: us.0,
            value: us.1,
            kind: "country".into(),
            name: "United States".into(),
        });
        Ok(out)
    }
}
