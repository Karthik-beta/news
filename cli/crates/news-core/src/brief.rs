//! The brief (`brief.v1`, Bureau Design §3.8): Fixer's research pack for Reporter, and the source of
//! truth for its shape in Rust. `contracts/cli/ai/brief.v1.schema.json` is the contract; Fixer
//! (`news-fixer`) builds it, and `news ai accept`'s validators read it (C2: every ref a Copy uses
//! must be one this brief issues, see [`Brief::refs`]).
//!
//! Facts carry values so Reporter understands direction and size, but it may output only ref tokens.
//! Everything under `source_text` and `quotes[].text` is source text: untrusted data, never
//! instructions (§9.3).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The only version this build writes and reads.
pub const BRIEF_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brief {
    pub brief_version: u32,
    pub item: Item,
    pub entities: Vec<Entity>,
    pub facts: Vec<Fact>,
    pub series: Vec<Series>,
    pub periods: Vec<Period>,
    pub context: Vec<ContextLine>,
    pub quotes: Vec<Quote>,
    pub source_text: SourceText,
    #[serde(default)]
    pub media: Media,
    #[serde(default)]
    pub related_pages: Vec<RelatedPage>,
    pub components: Components,
    pub rules: Rules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    New,
    Update,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub story_id: i64,
    pub event_key: Option<String>,
    /// The page-type skill's name without `page-` (`macro-release`).
    pub page_type: String,
    pub tier: u8,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_as_of: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// A data point (`dp_`) or derived value (`dv_`). `value` is the published text, never converted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    #[serde(rename = "ref")]
    pub reference: String,
    pub label: String,
    pub value: String,
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// `pct_change`, `pp_change`, `diff` or `share` for a `dv_`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Series {
    #[serde(rename = "ref")]
    pub reference: String,
    pub label: String,
    pub points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Period {
    #[serde(rename = "ref")]
    pub reference: String,
    pub label: String,
}

/// A context line computed by code; `rule` names the rule that produced it, so it can be re-checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLine {
    pub id: String,
    pub text: String,
    pub rule: String,
}

/// A verbatim span of a text block. Untrusted source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub id: String,
    pub doc: i64,
    pub block: i32,
    pub text: String,
}

/// The fence around source text: always `untrusted: true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceText {
    pub untrusted: bool,
    pub points: Vec<String>,
}

impl Default for SourceText {
    fn default() -> Self {
        Self {
            untrusted: true,
            points: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub images: Vec<serde_json::Value>,
    pub video: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedPage {
    pub page_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub suggested: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub disclaimer: Option<String>,
    pub fiscal_note: Option<String>,
}

impl Brief {
    /// Every ref token this brief issues (facts, series, periods, quotes, context lines): the set C2
    /// checks a Copy's refs against.
    pub fn refs(&self) -> BTreeSet<&str> {
        self.facts
            .iter()
            .map(|f| f.reference.as_str())
            .chain(self.series.iter().map(|s| s.reference.as_str()))
            .chain(self.periods.iter().map(|p| p.reference.as_str()))
            .chain(self.quotes.iter().map(|q| q.id.as_str()))
            .chain(self.context.iter().map(|c| c.id.as_str()))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_design_example_and_lists_its_refs() {
        let text = r#"{"brief_version": 1,
          "item": {"story_id": 81234, "event_key": "macro:us.cpi:2026-10", "page_type": "macro-release",
                   "tier": 1, "action": "new", "slug_hint": "us-cpi-october-2026", "data_as_of": "2026-11-13"},
          "entities": [{"id": "ent_us", "name": "United States", "kind": "country"}],
          "facts": [{"ref": "dp_7kq2", "label": "CPI", "value": "2.4", "unit": "Percent", "period": "October 2026"},
                    {"ref": "dv_3ab8", "label": "Change", "value": "-0.3", "unit": "pp", "derived": "pp_change"}],
          "series": [{"ref": "sr_cpi_yoy_5y", "label": "CPI", "points": 60}],
          "periods": [{"ref": "pd_2026_10", "label": "October 2026"}],
          "context": [{"id": "cx_1", "text": "Lowest since March 2021", "rule": "series_min_since"}],
          "quotes": [{"id": "q_1", "doc": 5521, "block": 3, "text": "The index rose."}],
          "source_text": {"untrusted": true, "points": ["x"]},
          "media": {"images": [], "video": null},
          "related_pages": [{"page_id": "2026/10/us-cpi-september-2026", "title": "t"}],
          "components": {"suggested": ["KeyFigures"]},
          "rules": {"disclaimer": "not-investment-advice", "fiscal_note": null}}"#;
        let b: Brief = serde_json::from_str(text).unwrap();
        let back: Brief = serde_json::from_value(serde_json::to_value(&b).unwrap()).unwrap();
        assert_eq!(b, back);
        assert_eq!(
            b.refs().into_iter().collect::<Vec<_>>(),
            [
                "cx_1",
                "dp_7kq2",
                "dv_3ab8",
                "pd_2026_10",
                "q_1",
                "sr_cpi_yoy_5y"
            ]
        );
        assert!(b.refs().iter().all(|r| crate::refs::is_ref(r)));
        assert!(
            serde_json::from_str::<Brief>(&text.replace("\"tier\"", "\"extra\": 1, \"tier\""))
                .is_err()
        );
    }
}
