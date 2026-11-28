//! BLS Public Data API v2 (U.S. Bureau of Labor Statistics).
//!
//! * Discovery: BLS has no change feed and its ICS calendar refuses automated clients, so discovery
//!   pulls each tracked series (one request each) and emits an item when the row marked
//!   `latest: "true"` names a period newer than the cursor.
//! * Fetch: the same series request. The free key (`BLS_API_KEY`) is added by the Runner.
//! * Parse: every row, value verbatim; `-` with footnote `X` is a missing observation; footnotes
//!   (`P` = preliminary) travel in the locator. BLS publishes no vintages, so the vintage is the
//!   snapshot's fetch time and the Ledger keeps a new vintage only when a value changed.
//!
//! BLS responses carry no units or titles without the catalogue option, so [`SERIES`] holds the series
//! metadata as BLS states it on each series page. A tracked series that is not in it gets no data
//! points and a `needs_human` entry: its unit is never guessed.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::util::{find, month_bounds, months_bounds, shape, value_err};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, EntityRef, ItemRef, NeedsHuman, ParseError,
    ParseErrorKind, Parsed, Request, Snapshot,
};

pub const ID: &str = "bls";
pub const SOURCE_ID: &str = "bls";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.bls.gov/publicAPI/v2/timeseries/data/";

/// Series metadata as BLS publishes it (series title, unit and adjustment from the series page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlsSeries {
    pub id: &'static str,
    pub title: &'static str,
    pub unit: &'static str,
    pub seasonal_adj: &'static str,
    /// Event-key series family (`macro:us.<family>:<period>`).
    pub family: &'static str,
}

pub const SERIES: &[BlsSeries] = &[
    BlsSeries {
        id: "CUUR0000SA0",
        title: "All items in U.S. city average, all urban consumers, not seasonally adjusted",
        unit: "Index 1982-1984=100",
        seasonal_adj: "NSA",
        family: "cpi",
    },
    BlsSeries {
        id: "CUSR0000SA0",
        title: "All items in U.S. city average, all urban consumers, seasonally adjusted",
        unit: "Index 1982-1984=100",
        seasonal_adj: "SA",
        family: "cpi",
    },
    BlsSeries {
        id: "CES0000000001",
        title: "All employees, thousands, total nonfarm, seasonally adjusted",
        unit: "All employees, thousands",
        seasonal_adj: "SA",
        family: "payrolls",
    },
    BlsSeries {
        id: "LNS14000000",
        title: "Unemployment rate, 16 years and over, seasonally adjusted",
        unit: "Percent",
        seasonal_adj: "SA",
        family: "unemployment",
    },
];

pub fn series(id: &str) -> Option<&'static BlsSeries> {
    SERIES.iter().find(|s| s.id == id)
}

pub struct Bls;

#[derive(Deserialize)]
struct Response {
    status: String,
    #[serde(default)]
    message: Vec<String>,
    #[serde(rename = "Results")]
    results: Option<Results>,
}

#[derive(Deserialize)]
struct Results {
    #[serde(default)]
    series: Vec<Series>,
}

#[derive(Deserialize)]
struct Series {
    #[serde(rename = "seriesID")]
    series_id: String,
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    year: String,
    period: String,
    #[serde(rename = "periodName")]
    period_name: String,
    #[serde(default)]
    latest: Option<String>,
    value: String,
    #[serde(default)]
    footnotes: Vec<Footnote>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
struct Footnote {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn read(snap: &Snapshot) -> Result<Vec<Series>, ParseError> {
    let r: Response = serde_json::from_slice(&snap.bytes).map_err(|e| shape("$", e))?;
    if r.status != "REQUEST_SUCCEEDED" {
        return Err(ParseError::new(
            ParseErrorKind::Upstream,
            "$.status",
            format!("{}: {}", r.status, r.message.join("; ")),
        ));
    }
    // A success with no `Results` at all is a changed shape, not an empty answer.
    let results = r
        .results
        .ok_or_else(|| shape("$.Results", "REQUEST_SUCCEEDED without Results"))?;
    Ok(results.series)
}

/// `(start, end, period_type)` for a BLS period code in a year.
fn period_bounds(year: i32, code: &str) -> Option<(chrono::NaiveDate, chrono::NaiveDate)> {
    let n: u32 = code.get(1..)?.parse().ok()?;
    match code.as_bytes().first()? {
        b'M' if (1..=12).contains(&n) => month_bounds(year, n),
        b'M' if n == 13 => months_bounds(year, 1, 12),
        b'Q' if (1..=4).contains(&n) => months_bounds(year, 3 * n - 2, 3 * n),
        b'Q' if n == 5 => months_bounds(year, 1, 12),
        b'S' if (1..=2).contains(&n) => months_bounds(year, 6 * n - 5, 6 * n),
        b'S' if n == 3 => months_bounds(year, 1, 12),
        b'A' if n == 1 => months_bounds(year, 1, 12),
        _ => None,
    }
}

/// `2026-10` for a monthly period, `2026-Q3` for a quarter, `2026` otherwise: the event key's
/// reference period.
fn reference_period(year: &str, code: &str) -> String {
    match (
        code.as_bytes().first(),
        code.get(1..).and_then(|n| n.parse::<u32>().ok()),
    ) {
        (Some(b'M'), Some(n @ 1..=12)) => format!("{year}-{n:02}"),
        (Some(b'Q'), Some(n @ 1..=4)) => format!("{year}-Q{n}"),
        _ => year.to_owned(),
    }
}

fn cursor_map(cursor: Option<&str>) -> BTreeMap<String, String> {
    cursor
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

/// Sortable period token: `2026-M10`.
fn period_token(year: &str, period: &str) -> String {
    format!("{year}-{period}")
}

impl Connector for Bls {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        cx.bls_series
            .iter()
            .map(|id| {
                Request::json(format!("{BASE}{id}")).with_secret("registrationkey", "BLS_API_KEY")
            })
            .collect()
    }

    fn parse_discovery(
        &self,
        _cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen = cursor_map(cursor);
        let mut refs = Vec::new();
        for snap in snaps {
            for s in read(snap)? {
                let Some(row) = s
                    .data
                    .iter()
                    .find(|r| r.latest.as_deref() == Some("true"))
                    .or_else(|| s.data.first())
                else {
                    continue;
                };
                let token = period_token(&row.year, &row.period);
                if seen.get(&s.series_id).is_some_and(|c| *c >= token) {
                    continue;
                }
                let meta = series(&s.series_id);
                refs.push(ItemRef {
                    source_id: SOURCE_ID.into(),
                    kind: "bls.series".into(),
                    key: s.series_id.clone(),
                    url: format!("https://data.bls.gov/timeseries/{}", s.series_id),
                    event_key_hint: meta.and_then(|m| {
                        eventkey::macro_release(
                            "us",
                            m.family,
                            &reference_period(&row.year, &row.period),
                        )
                    }),
                    published_at: None,
                    title: Some(format!(
                        "{}, {} {}",
                        meta.map_or(s.series_id.as_str(), |m| m.title),
                        row.period_name,
                        row.year
                    )),
                    meta: json!({"year": row.year, "period": row.period}),
                });
                seen.insert(s.series_id.clone(), token);
            }
        }
        Ok(Discovery {
            refs,
            calendar: Vec::new(),
            next_cursor: serde_json::to_string(&seen).ok(),
        })
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        vec![
            Request::json(format!("{BASE}{}", item.key))
                .with_secret("registrationkey", "BLS_API_KEY"),
        ]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &format!("/timeseries/data/{}", item.key))?;
        let mut out = Parsed::default();
        let us = ("iso3166".to_owned(), "US".to_owned());
        for (si, s) in read(snap)?.into_iter().enumerate() {
            let series_key = format!("bls:{}", s.series_id);
            let Some(meta) = series(&s.series_id) else {
                out.needs_human.push(NeedsHuman {
                    reason: format!(
                        "BLS series {} is not in the series catalogue: unit unknown",
                        s.series_id
                    ),
                    series_key,
                    locator: json!({"kind": "json", "path": format!("$.Results.series[{si}]")}),
                });
                continue;
            };
            let mut item_points = Vec::new();
            let mut seen = std::collections::BTreeMap::new();
            for (ri, row) in s.data.iter().enumerate() {
                let path = format!("$.Results.series[{si}].data[{ri}].value");
                let footnotes: Vec<Footnote> = row
                    .footnotes
                    .iter()
                    .filter(|f| f.code.is_some() || f.text.is_some())
                    .cloned()
                    .collect();
                let mut locator = json!({"kind": "json", "path": path, "series": s.series_id});
                if !footnotes.is_empty() {
                    locator["footnotes"] = json!(footnotes);
                }
                let period_label = format!("{} {}", row.period_name, row.year);
                // One period listed twice: identical rows collapse, differing values cannot both be
                // right and share a dp_id, so the parse fails rather than keep one silently.
                match seen.insert((row.year.as_str(), row.period.as_str()), row.value.as_str()) {
                    Some(prev) if prev == row.value => continue,
                    Some(prev) => {
                        return Err(value_err(
                            &path,
                            format!(
                                "conflicting observations for {} {}: {prev:?} and {:?}",
                                row.period_name, row.year, row.value
                            ),
                        ));
                    }
                    None => {}
                }
                if is_missing_marker(&row.value) {
                    out.missing.push(Missing {
                        series_key: series_key.clone(),
                        period_label,
                        value_text: row.value.clone(),
                        locator,
                    });
                    continue;
                }
                let year: i32 = row
                    .year
                    .parse()
                    .map_err(|e| value_err(format!("$.Results.series[{si}].data[{ri}].year"), e))?;
                let (start, end) = period_bounds(year, &row.period).ok_or_else(|| {
                    value_err(
                        format!("$.Results.series[{si}].data[{ri}].period"),
                        format!("unknown BLS period code {:?}", row.period),
                    )
                })?;
                let point = Draft {
                    context: vec![],
                    series_key: series_key.clone(),
                    entity: Some(us.clone()),
                    concept: s.series_id.clone(),
                    label: meta.title.into(),
                    value_text: row.value.clone(),
                    unit: meta.unit.into(),
                    currency: None,
                    scale: None,
                    decimals: None,
                    period_label,
                    period_start: Some(start),
                    period_end: Some(end),
                    period_type: PeriodType::Duration,
                    fiscal_year: None,
                    fiscal_period: None,
                    seasonal_adj: Some(meta.seasonal_adj.into()),
                    vintage: snap.fetched_at,
                    snapshot_sha: snap.sha256,
                    locator,
                    parser: self.parser(),
                }
                .build()
                .map_err(|e| value_err(&path, e))?;
                if row.latest.as_deref() == Some("true") {
                    item_points.push(point.dp_id.clone());
                }
                out.points.push(point);
            }
            let latest = s.data.iter().find(|r| r.latest.as_deref() == Some("true"));
            // No candidate when the newest period has no published figure: nothing to write about.
            if let Some(row) = latest.filter(|_| !item_points.is_empty()) {
                out.candidates.push(CandidateDraft {
                    event_key: eventkey::macro_release(
                        "us",
                        meta.family,
                        &reference_period(&row.year, &row.period),
                    ),
                    url: item.url.clone(),
                    title: format!("{}, {} {}", meta.title, row.period_name, row.year),
                    published_at: None,
                    rule_key: format!("family:{}", meta.family),
                    dp_ids: item_points,
                    entity: Some(us.clone()),
                });
            }
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
