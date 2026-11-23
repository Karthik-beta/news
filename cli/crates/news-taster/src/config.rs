//! Taster's thresholds: `data/seeds/taster-v1.toml`, compiled in and checked on load.

use serde::Deserialize;

const SEED: &str = include_str!("../../../../data/seeds/taster-v1.toml");

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub version: String,
    /// Cosine at which a candidate with a shared entity joins a story.
    pub join: f64,
    /// Lower bound of the grey zone.
    pub grey: f64,
    /// Cosine to a published page that makes a candidate an update.
    pub update: f64,
    pub simhash_distance: u32,
    pub simhash_days: i32,
    pub exact_days: i32,
    pub window_hours: i32,
    pub page_days: i32,
}

/// Taster config v1.
pub fn v1() -> Result<Thresholds, String> {
    let t: Thresholds = toml::from_str(SEED).map_err(|e| format!("taster-v1.toml: {e}"))?;
    if !(0.0 < t.grey && t.grey < t.join && t.join <= t.update && t.update <= 1.0) {
        return Err("taster-v1.toml: need 0 < grey < join <= update <= 1".into());
    }
    let windows = [t.simhash_days, t.exact_days, t.window_hours, t.page_days];
    if t.simhash_distance > 64 || windows.iter().any(|d| *d <= 0) {
        return Err("taster-v1.toml: windows must be positive and the distance at most 64".into());
    }
    Ok(t)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_design_thresholds_are_the_seed() {
        let t = v1().unwrap();
        assert_eq!((t.grey, t.join, t.update), (0.72, 0.82, 0.85));
        assert_eq!((t.simhash_distance, t.simhash_days), (3, 7));
    }
}
