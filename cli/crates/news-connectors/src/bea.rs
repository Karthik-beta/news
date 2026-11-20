//! U.S. Bureau of Economic Analysis API (NIPA tables). Needs a free key (`BEA_API_KEY`, added by the
//! Runner, never in a URL here). **Shipped disabled** with synthetic fixtures until Karthik records
//! real ones (`tests/fixtures/README.md`).
//!
//! `DataValue` is text and may carry thousands separators (`"23,486,000"`): kept as published.
//! `CL_UNIT` is the unit and `UNIT_MULT` the multiplier exponent, both stored as stated (`scale`
//! reads `UNIT_MULT=6`; nothing is multiplied). `UTCProductionTime` is the vintage.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::stat::{StatSeries, StatSource};
use crate::util::{period_from_label, shape, value_err};
use crate::{
    CandidateDraft, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "bea";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://apps.bea.gov/api/data/";
/// Real GDP, percent change from the preceding period, seasonally adjusted at annual rates.
const HEADLINE: &str = "A191RL";

pub const SERIES: &[StatSeries] = &[StatSeries {
    id: "NIPA.T10101",
    country: "us",
    family: "gdp",
    spec: "T10101",
    entity: ("iso3166", "US", "United States"),
}];

pub struct Bea;

fn url(s: &StatSeries, years: &str) -> String {
    format!(
        "{BASE}?method=GetData&datasetname=NIPA&TableName={}&Frequency=Q&Year={years}&ResultFormat=JSON",
        s.spec
    )
}

fn req(u: String) -> Request {
    Request::json(u).with_secret("UserID", "BEA_API_KEY")
}

#[derive(Deserialize)]
struct Doc {
    #[serde(rename = "BEAAPI")]
    api: Api,
}

#[derive(Deserialize)]
struct Api {
    #[serde(rename = "Results")]
    results: Results,
}

#[derive(Deserialize)]
struct Results {
    #[serde(rename = "UTCProductionTime")]
    produced: String,
    #[serde(rename = "Data")]
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "TableName")]
    table: String,
    #[serde(rename = "SeriesCode")]
    series_code: String,
    #[serde(rename = "LineNumber")]
    line: String,
    #[serde(rename = "LineDescription")]
    description: String,
    #[serde(rename = "TimePeriod")]
    time: String,
    #[serde(rename = "CL_UNIT")]
    unit: String,
    #[serde(rename = "UNIT_MULT")]
    unit_mult: String,
    #[serde(rename = "DataValue")]
    value: String,
}

/// `2025Q2` → (`2025-Q2` for the event key, first and last day).
fn quarter(label: &str) -> Option<(String, chrono::NaiveDate, chrono::NaiveDate)> {
    let (y, q) = label.split_once('Q')?;
    let key = format!("{y}-Q{q}");
    let (s, e) = period_from_label(&key)?;
    Some((key, s, e))
}

impl StatSource for Bea {
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
        req(url(s, "LAST5"))
    }
    fn history(&self, s: &StatSeries) -> Vec<Request> {
        vec![req(url(s, "LAST10"))]
    }
    fn page(&self, s: &StatSeries) -> String {
        format!(
            "https://apps.bea.gov/iTable/?reqid=19&step=3&isuri=1&categories=survey&nipa_table_list={}",
            s.spec
        )
    }

    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError> {
        let snap = crate::util::find(snaps, &url(s, "LAST10"))?;
        let doc: Doc =
            serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.BEAAPI.Results", e))?;
        let vintage: DateTime<Utc> =
            NaiveDateTime::parse_from_str(&doc.api.results.produced, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|t| t.and_utc())
                .map_err(|e| shape("$.BEAAPI.Results.UTCProductionTime", e))?;
        let mut out = Parsed::default();
        let mut flagged = std::collections::BTreeSet::new();
        let mut latest: Option<(String, String)> = None;
        for (i, r) in doc.api.results.data.iter().enumerate() {
            let path = format!("$.BEAAPI.Results.Data[{i}].DataValue");
            let series_key = format!("{ID}:{}:{}", r.table, r.series_code);
            let locator = serde_json::json!({
                "kind": "json", "path": path, "table": r.table, "series_code": r.series_code,
                "line": r.line, "time_period": r.time,
            });
            if is_missing_marker(&r.value) {
                out.missing.push(Missing {
                    series_key,
                    period_label: r.time.clone(),
                    value_text: r.value.clone(),
                    locator,
                });
                continue;
            }
            if news_ledger::units::classify(&r.unit).is_none() && flagged.insert(series_key.clone())
            {
                out.needs_human.push(NeedsHuman {
                    reason: format!("unit {:?} is not in the unit vocabulary", r.unit),
                    series_key: series_key.clone(),
                    locator: locator.clone(),
                });
            }
            let (key_period, start, end) = quarter(&r.time)
                .ok_or_else(|| value_err(&path, format!("{:?} is not a quarter label", r.time)))?;
            let point = Draft {
                context: vec![],
                series_key,
                entity: Some((s.entity.0.into(), s.entity.1.into())),
                concept: r.series_code.clone(),
                label: r.description.clone(),
                value_text: r.value.clone(),
                unit: r.unit.clone(),
                currency: None,
                scale: (r.unit_mult != "0").then(|| format!("UNIT_MULT={}", r.unit_mult)),
                decimals: None,
                period_label: r.time.clone(),
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
            if r.series_code == HEADLINE && latest.as_ref().is_none_or(|(p, _)| key_period > *p) {
                latest = Some((key_period, point.dp_id.clone()));
            }
            out.points.push(point);
        }
        if let Some((period, dp)) = latest {
            out.candidates.push(CandidateDraft {
                event_key: eventkey::macro_release(s.country, s.family, &period),
                url: item.url.clone(),
                title: format!("Gross domestic product, {period}"),
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
