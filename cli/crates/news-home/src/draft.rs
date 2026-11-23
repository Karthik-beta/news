//! The draft curation the curator returns (contract `home-draft.v1`) and the `curation.v1` body built
//! from it (ADR 0005). Slots point at page IDs and never carry page text.

use crate::check::{Failure, RuleResult};
use chrono::{DateTime, SecondsFormat, Utc};
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// What the curator writes: the model, the slots and a one-line rationale per page. The rationale
/// stays in PostgreSQL; `curation.v1` refuses it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub curator_model: String,
    pub slots: Slots,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slots {
    pub lead: Slot,
    pub side: Vec<Slot>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub category: String,
    pub pages: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub page_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_id: Option<String>,
    /// The lead only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub still_leading: Option<bool>,
}

impl Draft {
    /// Every slot with its JSON Pointer, in reading order: lead, side stories, then sections.
    pub fn placed(&self) -> Vec<(&Slot, String)> {
        let mut out = vec![(&self.slots.lead, "/slots/lead".to_owned())];
        for (i, s) in self.slots.side.iter().enumerate() {
            out.push((s, format!("/slots/side/{i}")));
        }
        for (i, sec) in self.slots.sections.iter().enumerate() {
            for (j, s) in sec.pages.iter().enumerate() {
                out.push((s, format!("/slots/sections/{i}/pages/{j}")));
            }
        }
        out
    }

    pub fn figure_ids(&self) -> Vec<String> {
        self.placed()
            .iter()
            .filter_map(|(s, _)| s.figure_id.clone())
            .collect()
    }

    pub fn page_ids(&self) -> Vec<String> {
        self.placed()
            .iter()
            .map(|(s, _)| s.page_id.clone())
            .collect()
    }
}

/// An RFC 3339 UTC timestamp with milliseconds and a `Z`, the shape `common.schema.json` accepts.
pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The `curation.v1` body (`PUT /v1/home/curation`). The same draft, times and results always give
/// the same bytes: object keys are sorted, so the preview's hash and the idempotency key are stable.
pub fn body(
    d: &Draft,
    created_at: DateTime<Utc>,
    approved_at: DateTime<Utc>,
    results: &[RuleResult],
) -> Value {
    json!({
        "schema": "curation.v1",
        "lang": "en",
        "created_at": ts(created_at),
        "approved_at": ts(approved_at),
        "curator_model": d.curator_model,
        "slots": d.slots,
        "checks": {"passed": true, "results": results},
    })
}

/// The cloud contracts, embedded: the write API validates a curation against the same file.
const CURATION: &str = include_str!("../../../../contracts/cloud/curation.v1.schema.json");
const COMMON: &str = include_str!("../../../../contracts/cloud/common.schema.json");
const CLOUD: &str = "https://news.karthik.nz/contracts/cloud/";

struct Retriever;

impl jsonschema::Retrieve for Retriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        match uri.as_str().strip_prefix(CLOUD) {
            Some("common.schema.json") => Ok(serde_json::from_str(COMMON)?),
            _ => Err(format!("not a cloud contract: {}", uri.as_str()).into()),
        }
    }
}

fn validator() -> Option<&'static Validator> {
    static V: OnceLock<Option<Validator>> = OnceLock::new();
    V.get_or_init(|| {
        let schema: Value = serde_json::from_str(CURATION).ok()?;
        jsonschema::draft202012::options()
            .with_retriever(Retriever)
            .build(&schema)
            .ok()
    })
    .as_ref()
}

/// Failures of `body` against `curation.v1` (slot counts, ID shapes, timestamps), at most 50 like the
/// API's `details`.
pub fn schema_failures(body: &Value) -> Vec<Failure> {
    let Some(v) = validator() else {
        return vec![Failure::new(
            "schema.invalid",
            "the curation.v1 contract does not compile",
            None,
        )];
    };
    v.iter_errors(body)
        .take(50)
        .map(|e| {
            let path = e.instance_path().to_string();
            Failure::new(
                "schema.invalid",
                e.to_string(),
                Some(path).filter(|p| !p.is_empty()),
            )
        })
        .collect()
}
