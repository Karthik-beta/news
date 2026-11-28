//! Bank of Canada Valet API (JSON). No key, no contact e-mail.
//!
//! `observations/<series>/json?recent=N` returns `seriesDetail` (label, `metadata.unit`) and the last N
//! observations, newest first, each `{"d": "<date>", "<series>": {"v": "<value>"}}`. The value is a
//! string and is kept exactly as published ("2.50", not 2.5). `B114039` is the Bank's own change-dated
//! series of the target for the overnight rate (its description names the daily `V39079`): one
//! observation per date a new target took effect, like the ECB's `FM.MRR`. The response states no
//! update time, so the vintage is the fetch time.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use news_ledger::decimal::is_missing_marker;
use news_ledger::{Draft, Missing, PeriodType};

use crate::stat::{StatSeries, StatSource};
use crate::util::{period_from_label, shape, value_err};
use crate::{
    CandidateDraft, EntityRef, ItemRef, NeedsHuman, ParseError, Parsed, Request, Snapshot,
};

pub const ID: &str = "boc-valet";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://www.bankofcanada.ca/valet/observations/";
const HISTORY: u32 = 12;
/// The bank as the `cb-boc` Stringer records it (`centralbank.rs`), so both candidates share the entity.
const BANK: (&str, &str, &str) = ("cb", "boc", "Bank of Canada");

pub const SERIES: &[StatSeries] = &[StatSeries {
    id: "B114039",
    country: "ca",
    family: "policy_rate",
    spec: "B114039",
    entity: ("iso3166", "CA", "Canada"),
}];

pub struct BocValet;

fn url(s: &StatSeries, recent: u32) -> String {
    format!("{BASE}{}/json?recent={recent}", s.spec)
}

#[derive(Deserialize)]
struct Body {
    #[serde(rename = "seriesDetail")]
    detail: BTreeMap<String, Detail>,
    observations: Vec<BTreeMap<String, Value>>,
}

#[derive(Deserialize)]
struct Detail {
    label: String,
    #[serde(default)]
    metadata: Meta,
}

#[derive(Deserialize, Default)]
struct Meta {
    #[serde(default)]
    unit: String,
}

impl StatSource for BocValet {
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
    fn page(&self, _s: &StatSeries) -> String {
        // The Bank's page for the policy rate, with its table of changes (the only series followed).
        "https://www.bankofcanada.ca/rates/interest-rates/key-interest-rate/".into()
    }

    fn parse_series(
        &self,
        s: &StatSeries,
        item: &ItemRef,
        snaps: &[Snapshot],
        parser: &str,
    ) -> Result<Parsed, ParseError> {
        let snap = crate::util::find(snaps, &url(s, HISTORY))?;
        let body: Body = serde_json::from_slice(&snap.bytes)
            .map_err(|e| shape("$", format!("expected seriesDetail and observations: {e}")))?;
        let detail = body.detail.get(s.spec).ok_or_else(|| {
            shape(
                format!("$.seriesDetail.{}", s.spec),
                "series not in response",
            )
        })?;
        let unit = detail.metadata.unit.clone();
        let series_key = format!("{ID}:{}", s.spec);
        let mut out = Parsed::default();
        if news_ledger::units::classify(&unit).is_none() {
            out.needs_human.push(NeedsHuman {
                reason: format!("unit {unit:?} is not in the unit vocabulary"),
                series_key: series_key.clone(),
                locator: serde_json::json!({
                    "kind": "json", "path": format!("$.seriesDetail.{}.metadata.unit", s.spec),
                    "unit": unit,
                }),
            });
        }
        let mut latest: Option<(String, String)> = None;
        for (i, obs) in body.observations.iter().enumerate() {
            let path = format!("$.observations[{i}].{}.v", s.spec);
            let date = obs
                .get("d")
                .and_then(Value::as_str)
                .ok_or_else(|| shape(format!("$.observations[{i}].d"), "no date"))?
                .to_owned();
            let locator = serde_json::json!({
                "kind": "json", "path": path, "series": s.spec, "date": date,
            });
            // A missing observation: the series key absent, no `v`, null or a missing marker.
            let text = match obs.get(s.spec).and_then(|o| o.get("v")) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(v)) => v.trim().to_owned(),
                Some(other) => return Err(shape(&path, format!("value is not a string: {other}"))),
            };
            if is_missing_marker(&text) {
                out.missing.push(Missing {
                    series_key: series_key.clone(),
                    period_label: date,
                    value_text: text,
                    locator,
                });
                continue;
            }
            let (start, end) = period_from_label(&date)
                .filter(|(a, b)| a == b)
                .ok_or_else(|| value_err(&path, format!("date {date:?} is not a day")))?;
            let point = Draft {
                context: vec![],
                series_key: series_key.clone(),
                entity: Some((s.entity.0.into(), s.entity.1.into())),
                concept: s.spec.into(),
                label: detail.label.clone(),
                value_text: text,
                unit: unit.clone(),
                currency: None,
                scale: None,
                decimals: None,
                period_label: date.clone(),
                period_start: Some(start),
                period_end: Some(end),
                period_type: PeriodType::Duration,
                fiscal_year: None,
                fiscal_period: None,
                seasonal_adj: None,
                vintage: snap.fetched_at,
                snapshot_sha: snap.sha256,
                locator,
                parser: parser.to_owned(),
            }
            .build()
            .map_err(|e| value_err(&path, e))?;
            if latest.as_ref().is_none_or(|(l, _)| date > *l) {
                latest = Some((date, point.dp_id.clone()));
            }
            out.points.push(point);
        }
        if let Some((period, dp)) = latest {
            // No event key: the observation is dated the day the new target took effect, not the decision
            // day, so it would open a second story for one decision. Like an 8-K, it carries the bank as its
            // entity and Taster groups it with the `cb-boc` statement's `policy:boc:<decision_date>` story.
            out.candidates.push(CandidateDraft {
                event_key: None,
                url: item.url.clone(),
                title: format!("{}, {period}", s.id),
                published_at: item.published_at,
                rule_key: format!("family:{}", s.family),
                dp_ids: vec![dp],
                entity: Some((BANK.0.into(), BANK.1.into())),
            });
            out.entities.push(EntityRef {
                scheme: BANK.0.into(),
                value: BANK.1.into(),
                kind: "central_bank".into(),
                name: BANK.2.into(),
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
