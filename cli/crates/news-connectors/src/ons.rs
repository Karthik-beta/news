//! ONS beta API (`api.beta.ons.gov.uk/v1`). Open Government Licence v3.0. No key.
//!
//! **Beta**: ONS has said the legacy endpoints are being retired and the beta API may change, so
//! every shape failure here is `upstream_changed` and the source's health goes amber, not red.
//! Discovery lists the dataset (`latest_version.id`); a new version number is a new item. Fetch
//! reads that version's observations for one aggregate and geography. Period labels are ONS's own
//! (`Jul-24`); the unit is `unit_of_measure`. ONS states no publication time per observation, so
//! the vintage is the fetch time (ADR 0018).
//!
//! Followed: `cpih01` (CPIH, UK, all items). ONS froze this dataset after January 2026 and points
//! users to its time series pages, so it stays as the worked example until a live dataset replaces it.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType, eventkey};

use crate::util::{find, month_bounds, sha_hex, shape, value_err};
use crate::{
    CandidateDraft, Connector, Cx, Discovery, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed,
    Request, Snapshot,
};

pub const ID: &str = "ons";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://api.beta.ons.gov.uk/v1/datasets/";

/// A followed ONS dataset.
pub struct Dataset {
    pub id: &'static str,
    pub edition: &'static str,
    /// Query that selects one series (`geography` and `aggregate` codes).
    pub query: &'static str,
    pub family: &'static str,
}

pub const DATASETS: &[Dataset] = &[Dataset {
    id: "cpih01",
    edition: "time-series",
    query: "geography=K02000001&aggregate=CP00&time=*",
    family: "cpih",
}];

pub struct Ons;

#[derive(Deserialize)]
struct DatasetDoc {
    id: String,
    title: String,
    links: Links,
}

#[derive(Deserialize)]
struct Links {
    latest_version: Version,
}

#[derive(Deserialize)]
struct Version {
    id: String,
}

#[derive(Deserialize)]
struct Observations {
    unit_of_measure: String,
    #[serde(default)]
    observations: Option<Vec<Obs>>,
}

#[derive(Deserialize)]
struct Obs {
    dimensions: BTreeMap<String, Dim>,
    observation: String,
}

#[derive(Deserialize)]
struct Dim {
    id: String,
}

fn dataset(id: &str) -> Option<&'static Dataset> {
    DATASETS.iter().find(|d| d.id == id)
}

/// `Jul-24` → first and last day of July 2024.
fn month_label(label: &str) -> Option<(NaiveDate, NaiveDate)> {
    let d = NaiveDate::parse_from_str(&format!("01-{label}"), "%d-%b-%y").ok()?;
    use chrono::Datelike;
    month_bounds(d.year(), d.month())
}

impl Connector for Ons {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        DATASETS
            .iter()
            .filter(|d| cx.tracked(ID).iter().any(|t| t == d.id))
            .map(|d| Request::json(format!("{BASE}{}", d.id)))
            .collect()
    }

    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen: BTreeMap<String, String> = cursor
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or_default();
        let mut out = Discovery::default();
        for d in DATASETS
            .iter()
            .filter(|d| cx.tracked(ID).iter().any(|t| t == d.id))
        {
            let Some(snap) = snaps.iter().find(|s| s.url == format!("{BASE}{}", d.id)) else {
                continue;
            };
            let doc: DatasetDoc = serde_json::from_slice(&snap.bytes)
                .map_err(|e| shape("$.links.latest_version", e))?;
            let version = doc.links.latest_version.id;
            if seen.get(d.id) == Some(&version) {
                continue;
            }
            seen.insert(d.id.to_owned(), version.clone());
            out.refs.push(ItemRef {
                source_id: ID.into(),
                kind: "ons.dataset".into(),
                key: doc.id.clone(),
                url: format!("https://www.ons.gov.uk/datasets/{}", doc.id),
                event_key_hint: None,
                published_at: Some(snap.fetched_at),
                title: Some(doc.title),
                meta: json!({"version": version, "listing_sha": sha_hex(&snap.sha256)}),
            });
        }
        out.next_cursor = serde_json::to_string(&seen).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        let (Some(d), Some(v)) = (
            dataset(&item.key),
            item.meta.get("version").and_then(|v| v.as_str()),
        ) else {
            return vec![];
        };
        vec![Request::json(format!(
            "{BASE}{}/editions/{}/versions/{v}/observations?{}",
            d.id, d.edition, d.query
        ))]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let d = dataset(&item.key)
            .ok_or_else(|| shape("$.key", format!("{} is not followed", item.key)))?;
        let snap = find(snaps, &format!("{BASE}{}/editions/", d.id))?;
        let doc: Observations =
            serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.observations", e))?;
        let rows = doc.observations.unwrap_or_default();
        let series_key = format!("{ID}:{}:{}", d.id, d.query.replace(['&', '='], "."));
        let uk = ("iso3166".to_owned(), "GB".to_owned());
        let mut out = Parsed::default();
        if news_ledger::units::classify(&doc.unit_of_measure).is_none() {
            out.needs_human.push(NeedsHuman {
                reason: format!(
                    "unit {:?} is not in the unit vocabulary",
                    doc.unit_of_measure
                ),
                series_key: series_key.clone(),
                locator: json!({"kind": "json", "path": "$.unit_of_measure"}),
            });
        }
        let mut latest: Option<(NaiveDate, String, String)> = None;
        for (i, o) in rows.iter().enumerate() {
            let path = format!("$.observations[{i}].observation");
            let time = o
                .dimensions
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("time"))
                .map(|(_, v)| v.id.clone())
                .ok_or_else(|| shape(&path, "no Time dimension"))?;
            let locator = json!({"kind": "json", "path": path, "dataset": d.id, "time": time});
            if is_missing_marker(&o.observation) || o.observation.is_empty() {
                out.missing.push(Missing {
                    series_key: series_key.clone(),
                    period_label: time,
                    value_text: o.observation.clone(),
                    locator,
                });
                continue;
            }
            let (start, end) = month_label(&time)
                .ok_or_else(|| value_err(&path, format!("{time:?} is not a month label")))?;
            let point = Draft {
                context: vec![],
                series_key: series_key.clone(),
                entity: Some(uk.clone()),
                concept: d.id.into(),
                label: item.title.clone().unwrap_or_else(|| d.id.into()),
                value_text: o.observation.clone(),
                unit: doc.unit_of_measure.clone(),
                currency: None,
                scale: None,
                decimals: None,
                period_label: time.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: None,
                vintage: snap.fetched_at,
                snapshot_sha: snap.sha256,
                locator,
                parser: self.parser(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if latest.as_ref().is_none_or(|(s, ..)| start > *s) {
                latest = Some((start, time, point.dp_id.clone()));
            }
            out.points.push(point);
        }
        if let Some((start, _, dp)) = latest {
            use chrono::Datelike;
            out.candidates.push(CandidateDraft {
                event_key: eventkey::macro_release(
                    "gb",
                    d.family,
                    &format!("{:04}-{:02}", start.year(), start.month()),
                ),
                url: item.url.clone(),
                title: item.title.clone().unwrap_or_default(),
                published_at: item.published_at,
                rule_key: format!("family:{}", d.family),
                dp_ids: vec![dp],
                entity: Some(uk.clone()),
            });
        }
        out.entities.push(EntityRef {
            scheme: uk.0,
            value: uk.1,
            kind: "country".into(),
            name: "United Kingdom".into(),
        });
        Ok(out)
    }
}
