//! Scorer config v1 and its seed data (`data/seeds/`, Bureau §4.2–§4.3). The seeds are compiled in, so
//! a score's `scorer_version` always names exactly the weights, event weights and targets that made it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SCORER: &str = include_str!("../../../../data/seeds/scorer-v1.toml");
const EVENT_WEIGHTS: &str = include_str!("../../../../data/seeds/event_weights.toml");
const COVERAGE_TARGETS: &str = include_str!("../../../../data/seeds/coverage_targets.toml");

/// The six component weights of S (§4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    #[serde(rename = "I")]
    pub i: f64,
    #[serde(rename = "M")]
    pub m: f64,
    #[serde(rename = "F")]
    pub f: f64,
    #[serde(rename = "D")]
    pub d: f64,
    #[serde(rename = "N")]
    pub n: f64,
    #[serde(rename = "G")]
    pub g: f64,
}

/// Coverage targets: shares of a 7-day window, by bucket.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shares {
    pub category: BTreeMap<String, f64>,
    pub region: BTreeMap<String, f64>,
    pub page_type: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScorerConfig {
    pub version: String,
    pub weights: Weights,
    pub half_life_hours: BTreeMap<String, f64>,
    pub event_weights: BTreeMap<String, f64>,
    pub coverage_targets: Shares,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScorerFile {
    version: String,
    weights: Weights,
    half_life_hours: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventFile {
    event: Vec<EventWeight>,
}

/// One row of `data/seeds/event_weights.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventWeight {
    #[serde(rename = "type")]
    pub event_type: String,
    pub weight: f64,
    pub note: String,
}

/// The event weight rows, in file order.
pub fn event_weight_rows() -> Result<Vec<EventWeight>, String> {
    let f: EventFile =
        toml::from_str(EVENT_WEIGHTS).map_err(|e| format!("event_weights.toml: {e}"))?;
    Ok(f.event)
}

/// Scorer config v1, checked: weights sum to 1, every value in [0, 1], every half-life positive.
pub fn v1() -> Result<ScorerConfig, String> {
    let s: ScorerFile = toml::from_str(SCORER).map_err(|e| format!("scorer-v1.toml: {e}"))?;
    let coverage_targets: Shares =
        toml::from_str(COVERAGE_TARGETS).map_err(|e| format!("coverage_targets.toml: {e}"))?;
    let mut event_weights = BTreeMap::new();
    for r in event_weight_rows()? {
        if !(0.0..=1.0).contains(&r.weight) {
            return Err(format!("event weight {} is outside [0, 1]", r.event_type));
        }
        if event_weights
            .insert(r.event_type.clone(), r.weight)
            .is_some()
        {
            return Err(format!("event type {} is listed twice", r.event_type));
        }
    }
    let w = &s.weights;
    let sum = w.i + w.m + w.f + w.d + w.n + w.g;
    if (sum - 1.0).abs() > 1e-9 {
        return Err(format!("scorer weights sum to {sum}, not 1"));
    }
    if s.half_life_hours.values().any(|h| *h <= 0.0) {
        return Err("a half-life is not positive".into());
    }
    for (name, table) in [
        ("category", &coverage_targets.category),
        ("region", &coverage_targets.region),
        ("page_type", &coverage_targets.page_type),
    ] {
        let total: f64 = table.values().sum();
        if (total - 1.0).abs() > 1e-9 || table.values().any(|v| !(0.0..=1.0).contains(v)) {
            return Err(format!("coverage targets for {name} do not sum to 1"));
        }
    }
    Ok(ScorerConfig {
        version: s.version,
        weights: s.weights,
        half_life_hours: s.half_life_hours,
        event_weights,
        coverage_targets,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn v1_loads_and_has_every_page_type_half_life() {
        let c = v1().unwrap();
        assert_eq!(c.version, "v1");
        for t in ["news", "macro", "policy", "earnings", "science", "report"] {
            assert!(c.half_life_hours.contains_key(t), "{t}");
            assert!(c.coverage_targets.page_type.contains_key(t), "{t}");
        }
    }

    /// Migration 0004 seeded `event_weights` with the design's starting values; the seed file is the
    /// same table, so the two must not drift.
    #[test]
    fn event_weight_seed_matches_migration_0004() {
        let sql = include_str!("../../../migrations/0004_taster_nose.sql");
        let rows = event_weight_rows().unwrap();
        assert_eq!(rows.len(), 11);
        for r in rows {
            let line = sql
                .lines()
                .find(|l| l.trim_start().starts_with(&format!("('{}',", r.event_type)))
                .unwrap_or_else(|| panic!("{} is not in 0004", r.event_type));
            let weight: f64 = line.split(',').nth(1).unwrap().trim().parse().unwrap();
            assert_eq!(weight, r.weight, "{}", r.event_type);
        }
    }
}
