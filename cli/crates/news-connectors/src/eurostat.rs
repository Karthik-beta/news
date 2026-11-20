//! Eurostat dissemination API (JSON-stat 2.0). No key.
//!
//! Discovery is the shared hash probe (`lastTimePeriod=1`). A response's `updated` stamp is the
//! vintage: it changes when Eurostat revises the dataset. Unit and seasonal adjustment are the
//! labels of the `unit` and `s_adj` dimensions; the period label is Eurostat's time code (`2025-10`,
//! `2025-Q3`, `2024`).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::value::RawValue;

use news_ledger::{Draft, PeriodType, eventkey};

use crate::stat::{StatSeries, StatSource};
use crate::util::{period_from_label, shape, value_err};
use crate::{
    CandidateDraft, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "eurostat";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://ec.europa.eu/eurostat/api/dissemination/statistics/1.0/data/";
const HISTORY: u32 = 12;

pub const SERIES: &[StatSeries] = &[StatSeries {
    id: "HICP.EA",
    country: "ea",
    family: "hicp",
    spec: "prc_hicp_manr?geo=EA&coicop=CP00",
    entity: ("eurostat-geo", "EA", "Euro area"),
}];

pub struct Eurostat;

fn url(s: &StatSeries, last: u32) -> String {
    format!("{BASE}{}&lastTimePeriod={last}&lang=EN", s.spec)
}

#[derive(Deserialize)]
struct Doc<'a> {
    label: String,
    #[serde(default)]
    updated: Option<String>,
    id: Vec<String>,
    size: Vec<usize>,
    dimension: BTreeMap<String, Dim>,
    #[serde(borrow)]
    value: BTreeMap<String, &'a RawValue>,
    #[serde(default)]
    status: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Dim {
    category: Category,
}

#[derive(Deserialize)]
struct Category {
    index: BTreeMap<String, usize>,
    #[serde(default)]
    label: BTreeMap<String, String>,
}

impl Dim {
    /// Codes in position order.
    fn codes(&self) -> Vec<&str> {
        let mut v: Vec<(&usize, &String)> =
            self.category.index.iter().map(|(k, i)| (i, k)).collect();
        v.sort();
        v.into_iter().map(|(_, k)| k.as_str()).collect()
    }
    fn label(&self, code: &str) -> Option<&str> {
        self.category.label.get(code).map(String::as_str)
    }
}

impl StatSource for Eurostat {
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
        let dataset = s.spec.split('?').next().unwrap_or(s.spec);
        format!("https://ec.europa.eu/eurostat/databrowser/view/{dataset}/default/table")
    }

    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError> {
        let snap = crate::util::find(snaps, &url(s, HISTORY))?;
        let doc: Doc = serde_json::from_slice(&snap.bytes).map_err(|e| shape("$", e))?;
        if doc.id.len() != doc.size.len() {
            return Err(shape("$.size", "id and size differ in length"));
        }
        let dataset = s.spec.split('?').next().unwrap_or(s.spec);
        let vintage: DateTime<Utc> = doc
            .updated
            .as_deref()
            .and_then(|u| DateTime::parse_from_str(u, "%Y-%m-%dT%H:%M:%S%z").ok())
            .map(|d| d.with_timezone(&Utc))
            .ok_or_else(|| shape("$.updated", "missing or not a timestamp"))?;
        let dims: Vec<&Dim> = doc
            .id
            .iter()
            .map(|id| {
                doc.dimension
                    .get(id)
                    .ok_or_else(|| shape(format!("$.dimension.{id}"), "missing"))
            })
            .collect::<Result<_, _>>()?;
        let time_pos = doc
            .id
            .iter()
            .position(|i| i == "time")
            .ok_or_else(|| shape("$.id", "no time dimension"))?;
        let codes: Vec<Vec<&str>> = dims.iter().map(|d| d.codes()).collect();

        let mut out = Parsed::default();
        let mut by_pos: Vec<(usize, &String, &&RawValue)> = Vec::new();
        for (k, v) in &doc.value {
            let p: usize = k.parse().map_err(|e| shape(format!("$.value.{k}"), e))?;
            by_pos.push((p, k, v));
        }
        by_pos.sort_by_key(|(p, ..)| *p);
        let mut latest: Option<(String, String)> = None;
        let mut flagged = std::collections::BTreeSet::new();
        for (p, k, raw) in by_pos {
            // Row-major decode: the last dimension varies fastest.
            let mut rest = p;
            let mut coord = vec![0usize; doc.size.len()];
            for i in (0..doc.size.len()).rev() {
                let n = doc.size[i].max(1);
                coord[i] = rest % n;
                rest /= n;
            }
            let path = format!("$.value.{k}");
            let code = |i: usize| -> Result<&str, ParseError> {
                codes[i]
                    .get(coord[i])
                    .copied()
                    .ok_or_else(|| shape(&path, "position outside the dimension"))
            };
            let period_label = code(time_pos)?.to_owned();
            let mut key_parts = Vec::new();
            let (mut unit, mut seasonal) = (String::new(), None);
            for (i, id) in doc.id.iter().enumerate() {
                let c = code(i)?;
                if i != time_pos {
                    key_parts.push(c.to_owned());
                }
                if id == "unit" {
                    unit = dims[i].label(c).unwrap_or(c).to_owned();
                }
                if id == "s_adj" {
                    seasonal = dims[i].label(c).map(str::to_owned);
                }
            }
            let series_key = format!("{ID}:{dataset}:{}", key_parts.join("."));
            if news_ledger::units::classify(&unit).is_none() && flagged.insert(series_key.clone()) {
                out.needs_human.push(NeedsHuman {
                    reason: format!("unit {unit:?} is not in the unit vocabulary"),
                    series_key: series_key.clone(),
                    locator: serde_json::json!({"kind": "json", "path": path}),
                });
            }
            let (start, end) = period_from_label(&period_label)
                .ok_or_else(|| value_err(&path, "time code is not a calendar period"))?;
            let mut locator = serde_json::json!({
                "kind": "json", "path": path, "dataset": dataset, "period": period_label,
            });
            if let Some(st) = doc.status.get(k) {
                locator["status"] = st.clone().into();
            }
            let point = Draft {
                context: vec![],
                series_key: series_key.clone(),
                entity: Some((s.entity.0.into(), s.entity.1.into())),
                concept: s.id.into(),
                label: doc.label.clone(),
                value_text: raw.get().trim().to_owned(),
                unit,
                currency: None,
                scale: None,
                decimals: None,
                period_label: period_label.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: seasonal,
                vintage,
                snapshot_sha: snap.sha256,
                locator,
                parser: parser.to_owned(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if latest.as_ref().is_none_or(|(l, _)| period_label > *l) {
                latest = Some((period_label, point.dp_id.clone()));
            }
            out.points.push(point);
        }
        if let Some((period, dp)) = latest {
            out.candidates.push(CandidateDraft {
                event_key: eventkey::macro_release(s.country, s.family, &period),
                url: item.url.clone(),
                title: format!("{}, {period}", doc.label),
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
