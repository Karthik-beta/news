//! World Bank Indicators API v2 (JSON). No key.
//!
//! The response is `[page meta, rows]`. The API's `unit` field is empty for the indicators followed
//! here, so the unit is the parenthetical the World Bank puts in the indicator name (`GDP (current
//! US$)` gives `current US$`), kept verbatim. The vintage is the response's `lastupdated` date.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::value::RawValue;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::stat::{StatSeries, StatSource};
use crate::util::{day_vintage, period_from_label, shape, value_err};
use crate::{
    CandidateDraft, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "worldbank";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.worldbank.org/v2/";
const HISTORY: u32 = 5;

pub const SERIES: &[StatSeries] = &[
    StatSeries {
        id: "GDP.USA",
        country: "us",
        family: "gdp_wb",
        spec: "country/USA/indicator/NY.GDP.MKTP.CD",
        entity: ("iso3166", "US", "United States"),
    },
    StatSeries {
        id: "GDP.IND",
        country: "in",
        family: "gdp_wb",
        spec: "country/IND/indicator/NY.GDP.MKTP.CD",
        entity: ("iso3166", "IN", "India"),
    },
];

pub struct WorldBank;

fn url(s: &StatSeries, mrv: u32) -> String {
    format!("{BASE}{}?format=json&mrv={mrv}", s.spec)
}

#[derive(Deserialize)]
struct Meta {
    #[serde(default)]
    lastupdated: Option<String>,
}

#[derive(Deserialize)]
struct Row<'a> {
    indicator: Named,
    countryiso3code: String,
    date: String,
    #[serde(borrow)]
    value: &'a RawValue,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    obs_status: String,
    #[serde(default)]
    decimal: Option<i16>,
}

#[derive(Deserialize)]
struct Named {
    id: String,
    value: String,
}

/// `GDP (current US$)` → `current US$`.
fn unit_of(indicator: &str) -> Option<&str> {
    let close = indicator.rfind(')')?;
    let open = indicator[..close].rfind('(')?;
    Some(&indicator[open + 1..close])
}

impl StatSource for WorldBank {
    fn id(&self) -> &'static str {
        ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }
    fn catalogue(&self) -> &'static [StatSeries] {
        SERIES
    }
    fn probe(&self, s: &StatSeries) -> Request {
        Request::json(url(s, 1))
    }
    fn history(&self, s: &StatSeries) -> Vec<Request> {
        vec![Request::json(url(s, HISTORY))]
    }
    fn page(&self, s: &StatSeries) -> String {
        let ind = s.spec.rsplit('/').next().unwrap_or("");
        format!("https://data.worldbank.org/indicator/{ind}")
    }

    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError> {
        let snap = crate::util::find(snaps, &url(s, HISTORY))?;
        let (meta, rows): (Meta, Vec<Row>) = serde_json::from_slice(&snap.bytes)
            .map_err(|e| shape("$", format!("expected [meta, rows]: {e}")))?;
        let vintage = meta
            .lastupdated
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .map(day_vintage)
            .ok_or_else(|| shape("$[0].lastupdated", "missing or not a date"))?;
        let mut out = Parsed::default();
        let mut latest: Option<(String, String)> = None;
        for (i, r) in rows.iter().enumerate() {
            let path = format!("$[1][{i}].value");
            let series_key = format!("{ID}:{}:{}", r.indicator.id, r.countryiso3code);
            let locator = serde_json::json!({
                "kind": "json", "path": path, "indicator": r.indicator.id,
                "country": r.countryiso3code, "date": r.date, "obs_status": r.obs_status,
            });
            let text = r.value.get().trim().to_owned();
            if text == "null" || is_missing_marker(&text) {
                out.missing.push(Missing {
                    series_key,
                    period_label: r.date.clone(),
                    value_text: text,
                    locator,
                });
                continue;
            }
            let unit = if r.unit.is_empty() {
                unit_of(&r.indicator.value).unwrap_or_default().to_owned()
            } else {
                r.unit.clone()
            };
            if news_ledger::units::classify(&unit).is_none() {
                out.needs_human.push(NeedsHuman {
                    reason: format!("unit {unit:?} is not in the unit vocabulary"),
                    series_key: series_key.clone(),
                    locator: locator.clone(),
                });
            }
            let (start, end) = period_from_label(&r.date)
                .ok_or_else(|| value_err(&path, "date is not a calendar year"))?;
            let point = Draft {
                context: vec![],
                series_key,
                entity: Some((s.entity.0.into(), s.entity.1.into())),
                concept: r.indicator.id.clone(),
                label: r.indicator.value.clone(),
                value_text: text,
                unit,
                currency: None,
                scale: None,
                decimals: r.decimal,
                period_label: r.date.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: None,
                vintage,
                snapshot_sha: snap.sha256,
                locator,
                parser: parser.to_owned(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if latest.as_ref().is_none_or(|(l, _)| r.date > *l) {
                latest = Some((r.date.clone(), point.dp_id.clone()));
            }
            out.points.push(point);
        }
        if let Some((period, dp)) = latest {
            out.candidates.push(CandidateDraft {
                event_key: eventkey::macro_release(s.country, s.family, &period),
                url: item.url.clone(),
                title: format!("{} ({}), {period}", s.id, s.entity.2),
                published_at: item.published_at,
                rule_key: format!("family:{}", s.family),
                dp_ids: vec![dp],
                entity: Some((s.entity.0.into(), s.entity.1.into())),
            });
        }
        out.entities.push(EntityRef {
            scheme: s.entity.0.into(),
            value: s.entity.1.into(),
            kind: "country".into(),
            name: s.entity.2.into(),
        });
        Ok(out)
    }
}
