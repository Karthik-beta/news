//! Candidates from parsed items: the licence gate, then rule-based classification.

use news_connectors::{CandidateDraft, Parsed};
use news_contacts::{LicenceMissing, licence_gate};
use serde::Serialize;

use crate::rules;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub source_id: String,
    pub draft: CandidateDraft,
    pub page_type: Option<String>,
    /// `rule` when a rule matched; `rule-unsure` when none did (A3 decides later, from a closed set).
    pub page_type_by: &'static str,
    pub categories: Vec<String>,
    pub region: Option<String>,
}

/// Classify the parse's candidates. A source with no licence emits none (`licence_missing`).
pub fn prepare(
    source_id: &str,
    licence: Option<&str>,
    parsed: &Parsed,
) -> Result<Vec<Candidate>, LicenceMissing> {
    licence_gate(source_id, licence)?;
    Ok(parsed
        .candidates
        .iter()
        .map(|d| match rules::classify(source_id, &d.rule_key) {
            Some(r) => Candidate {
                source_id: source_id.to_owned(),
                draft: d.clone(),
                page_type: Some(r.page_type.to_owned()),
                page_type_by: "rule",
                categories: r.categories.iter().map(|c| (*c).to_owned()).collect(),
                region: Some(r.region.to_owned()),
            },
            None => Candidate {
                source_id: source_id.to_owned(),
                draft: d.clone(),
                page_type: None,
                page_type_by: "rule-unsure",
                categories: Vec::new(),
                region: None,
            },
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn parsed(rule_key: &str) -> Parsed {
        Parsed {
            candidates: vec![CandidateDraft {
                event_key: Some("macro:us.cpi:2026-10".into()),
                url: "https://data.bls.gov/timeseries/CUUR0000SA0".into(),
                title: "CPI".into(),
                published_at: None,
                rule_key: rule_key.into(),
                dp_ids: vec![],
                entity: None,
            }],
            ..Parsed::default()
        }
    }

    #[test]
    fn rules_classify_and_unknowns_stay_unsure() {
        let c = prepare("bls", Some("public-domain"), &parsed("family:cpi")).unwrap();
        assert_eq!(c[0].page_type.as_deref(), Some("macro"));
        assert_eq!(c[0].categories, vec!["economy", "inflation"]);
        assert_eq!(c[0].page_type_by, "rule");
        let u = prepare("bls", Some("public-domain"), &parsed("family:ppi")).unwrap();
        assert_eq!(
            (u[0].page_type.as_deref(), u[0].page_type_by),
            (None, "rule-unsure")
        );
    }

    #[test]
    fn no_licence_no_candidates() {
        let err = prepare("bls", None, &parsed("family:cpi")).unwrap_err();
        assert!(err.to_string().starts_with("licence_missing"));
    }
}
