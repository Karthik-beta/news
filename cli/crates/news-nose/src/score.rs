//! Nose: signals, the score S and the Pitch (Bureau §4.2–§4.3). Pure: the same input, time and
//! config always give the same score, so re-scoring changes nothing.

use crate::config::{ScorerConfig, Shares};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// What Nose needs to know about one story, written by Taster (`stories.score_input`). Every number
/// in it comes from source data or our own records, filled by code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryInput {
    /// Key into the event weights (`data/seeds/event_weights.toml`).
    pub event_type: String,
    /// The page type and the inputs its magnitude formula needs.
    pub page: Page,
    /// Up to three; the first is the primary category.
    pub categories: Vec<String>,
    /// At least one; the first is the primary region.
    pub regions: Vec<String>,
    /// `None` when the main entity is unresolved.
    pub main_entity: Option<Entity>,
    /// Other linked entities.
    #[serde(default)]
    pub entities: Vec<Entity>,
    /// The best member's source tier (Contacts), 1–4.
    pub source_tier: u8,
    /// When the best member was published.
    pub published_at: DateTime<Utc>,
    /// Resolvable data points.
    pub data_points: u32,
    /// A series of at least 12 points exists.
    pub series: bool,
    pub novelty: Novelty,
    /// The source's `redistribution` flag.
    #[serde(default = "yes")]
    pub redistribution: bool,
    /// The parser reported warnings.
    #[serde(default)]
    pub parse_warnings: bool,
    #[serde(default)]
    pub action: Action,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    /// Roster's `entities.prominence`: 1 flagship, 2 major, 3 other.
    #[serde(default = "other")]
    pub prominence: u8,
}

fn other() -> u8 {
    3
}

/// A new page, or an AI update to an existing one (Taster step 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    New,
    Update,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::New => "new",
            Action::Update => "update",
        }
    }
}

/// The page type, carrying the source data its magnitude formula M reads (§4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Page {
    /// Observations in time order, latest last; `per_year` 12 for monthly, 4 for quarterly.
    Macro {
        values: Vec<f64>,
        per_year: u32,
    },
    /// Signed rate change in basis points; `statement_change` is the share of the statement's text
    /// that changed (0–1); `holds_before` counts consecutive holds before this decision; `reversal`
    /// is a move opposite to the last one.
    Policy {
        change_bp: i32,
        #[serde(default)]
        statement_change: f64,
        #[serde(default)]
        holds_before: u32,
        #[serde(default)]
        reversal: bool,
    },
    /// Year-on-year revenue growth this quarter and in earlier quarters (latest last); `flip` is a
    /// loss↔profit flip or a restatement.
    Earnings {
        growth: f64,
        past_growth: Vec<f64>,
        #[serde(default)]
        flip: bool,
    },
    /// The headline forecast's revision against the edition before, and earlier editions' revisions.
    Report {
        revision: Option<f64>,
        #[serde(default)]
        past_revisions: Vec<f64>,
    },
    Science {
        #[serde(default)]
        open_dataset: bool,
        /// The peer-reviewed version of a tracked preprint.
        #[serde(default)]
        peer_reviewed_preprint: bool,
    },
    News {
        primary_sources: u32,
    },
}

impl Page {
    pub fn type_name(&self) -> &'static str {
        match self {
            Page::Macro { .. } => "macro",
            Page::Policy { .. } => "policy",
            Page::Earnings { .. } => "earnings",
            Page::Report { .. } => "report",
            Page::Science { .. } => "science",
            Page::News { .. } => "news",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Novelty {
    /// A new event key.
    NewEvent,
    /// A regular series with a page last period.
    RegularSeries,
    /// A follow-up of a covered story.
    FollowUp,
    /// Unstructured: the highest cosine to a page of the last 30 days that shares an entity.
    Similar { max_cos: f64 },
}

/// The ten signals (§4.2), each in [0, 1].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Components {
    #[serde(rename = "I")]
    pub i: f64,
    #[serde(rename = "E")]
    pub e: f64,
    #[serde(rename = "P")]
    pub p: f64,
    #[serde(rename = "T")]
    pub t: f64,
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
    #[serde(rename = "Q")]
    pub q: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub scorer_version: String,
    pub components: Components,
    /// S, 0–100, rounded to 2 decimals (the stored `numeric(5,2)`).
    pub total: f64,
    /// The tier S earns on its own (§4.4); `None` stays queued.
    pub tier: Option<u8>,
    /// One line per component.
    pub pitch: String,
}

pub const TIER1_FLOOR: f64 = 70.0;
pub const TIER1_PROMINENT_FLOOR: f64 = 60.0;
pub const TIER2_FLOOR: f64 = 40.0;

/// Tier by S (§4.4): 1 if S ≥ 70, or the main entity has prominence 1 and S ≥ 60; 2 if S ≥ 40.
pub fn tier(s: f64, main_prominence: Option<u8>) -> Option<u8> {
    if s >= TIER1_FLOOR || (main_prominence == Some(1) && s >= TIER1_PROMINENT_FLOOR) {
        Some(1)
    } else if s >= TIER2_FLOOR {
        Some(2)
    } else {
        None
    }
}

fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

/// Population standard deviation.
fn sd(x: &[f64]) -> f64 {
    let m = mean(x);
    (x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

fn surprise(z: f64) -> f64 {
    1.0 - (-z / 2.0).exp()
}

const NOT_COMPUTABLE: f64 = 0.5;

/// Magnitude / surprise M and its Pitch text (§4.2, "Magnitude / surprise (M) by page type").
fn magnitude(page: &Page) -> (f64, String) {
    match page {
        Page::Macro { values, per_year } => {
            let window = if *per_year <= 4 { 20 } else { 60 };
            let w = &values[values.len().saturating_sub(window)..];
            let diffs: Vec<f64> = w.windows(2).map(|p| p[1] - p[0]).collect();
            let Some(last) = diffs.last().copied().filter(|_| diffs.len() >= 2) else {
                return (
                    NOT_COMPUTABLE,
                    "fewer than 3 observations, not computable".into(),
                );
            };
            let sigma = sd(&diffs);
            let z = if sigma > 0.0 {
                last.abs() / sigma
            } else if last == 0.0 {
                0.0
            } else {
                return (
                    NOT_COMPUTABLE,
                    "changes have no spread, not computable".into(),
                );
            };
            let mut m = surprise(z);
            let mut text = format!(
                "change is {z:.2} σ of changes over {} observations",
                w.len()
            );
            let span = 10 * *per_year as usize;
            if let Some((x_t, before)) = values.split_last()
                && span > 0
                && before.len() >= span
            {
                let past = &before[before.len() - span..];
                let high = past.iter().all(|v| x_t > v);
                let low = past.iter().all(|v| x_t < v);
                if high || low {
                    m = m.max(0.8);
                    text.push_str(if high {
                        "; 10-year high"
                    } else {
                        "; 10-year low"
                    });
                }
            }
            (m, text)
        }
        Page::Policy {
            change_bp,
            statement_change,
            holds_before,
            reversal,
        } => {
            let (base, what) = match change_bp.unsigned_abs() {
                50.. => (1.0, format!("move of {change_bp} bp")),
                1.. => (0.8, format!("move of {change_bp} bp")),
                0 if *statement_change > 0.20 => (
                    0.4,
                    format!("hold, statement {:.0}% changed", statement_change * 100.0),
                ),
                0 => (0.3, "hold".to_owned()),
            };
            if *change_bp != 0 && (*holds_before >= 6 || *reversal) {
                let why = if *reversal {
                    "change of direction"
                } else {
                    "first move after 6+ holds"
                };
                ((base + 0.2_f64).min(1.0), format!("{what}, {why} (+0.2)"))
            } else {
                (base, what)
            }
        }
        Page::Earnings {
            growth,
            past_growth,
            flip,
        } => {
            let past = &past_growth[past_growth.len().saturating_sub(12)..];
            let sigma = if past.len() >= 2 { sd(past) } else { 0.0 };
            let (mut m, mut text) = if sigma > 0.0 {
                let z = (growth - mean(past)).abs() / sigma;
                (
                    surprise(z),
                    format!(
                        "revenue growth is {z:.2} σ from its last {} quarters",
                        past.len()
                    ),
                )
            } else {
                (
                    NOT_COMPUTABLE,
                    "growth history too short, not computable".into(),
                )
            };
            if *flip {
                m = m.max(0.7);
                text.push_str("; loss/profit flip or restatement");
            }
            (m, text)
        }
        Page::Report {
            revision,
            past_revisions,
        } => {
            let sigma = if past_revisions.len() >= 2 {
                sd(past_revisions)
            } else {
                0.0
            };
            match revision {
                Some(r) if sigma > 0.0 => {
                    let z = (r - mean(past_revisions)).abs() / sigma;
                    (
                        surprise(z),
                        format!(
                            "headline revision is {z:.2} σ of {} past revisions",
                            past_revisions.len()
                        ),
                    )
                }
                _ => (NOT_COMPUTABLE, "revision not computable".into()),
            }
        }
        Page::Science {
            open_dataset,
            peer_reviewed_preprint,
        } => {
            let mut m = 0.3;
            let mut text = "science base 0.3".to_owned();
            if *open_dataset {
                m += 0.2;
                text.push_str(", open dataset +0.2");
            }
            if *peer_reviewed_preprint {
                m += 0.2;
                text.push_str(", peer-reviewed version of a tracked preprint +0.2");
            }
            (m, text)
        }
        Page::News { primary_sources } => {
            let n = f64::from(*primary_sources);
            (
                ((1.0 + n).log2() / 9_f64.log2()).min(1.0),
                format!("{primary_sources} distinct primary sources"),
            )
        }
    }
}

fn prominence_weight(p: u8) -> f64 {
    match p {
        1 => 1.0,
        2 => 0.6,
        3 => 0.3,
        _ => 0.0,
    }
}

/// Coverage gap of one bucket: `max(0, (target − actual) / target)`; a bucket with no target has none.
fn bucket_gap(target: Option<f64>, actual: Option<f64>) -> f64 {
    match target {
        Some(t) if t > 0.0 => ((t - actual.unwrap_or(0.0)) / t).max(0.0),
        _ => 0.0,
    }
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Score one story at `as_of` against `coverage` (actual shares over the last 7 days).
pub fn score(
    input: &StoryInput,
    as_of: DateTime<Utc>,
    coverage: &Shares,
    cfg: &ScorerConfig,
) -> Result<Score, String> {
    let primary_category = input
        .categories
        .first()
        .ok_or("the story has no category")?;
    let primary_region = input.regions.first().ok_or("the story has no region")?;
    if input.categories.len() > 3 {
        return Err("a story carries at most three categories".into());
    }
    let page_type = input.page.type_name();

    // Importance.
    let e = *cfg
        .event_weights
        .get(&input.event_type)
        .ok_or_else(|| format!("unknown event type {:?}", input.event_type))?;
    let p = input
        .main_entity
        .iter()
        .chain(&input.entities)
        .map(|x| prominence_weight(x.prominence))
        .fold(0.0, f64::max);
    let t = match input.source_tier {
        1 => 1.0,
        2 => 0.9,
        3 => 0.6,
        4 => 0.3,
        n => return Err(format!("source tier {n} is not 1–4")),
    };
    let i = 0.5 * e + 0.3 * p + 0.2 * t;

    let (m, m_text) = magnitude(&input.page);

    let h = *cfg
        .half_life_hours
        .get(page_type)
        .ok_or_else(|| format!("no half-life for {page_type}"))?;
    let age_h = ((as_of - input.published_at).num_seconds().max(0)) as f64 / 3600.0;
    let f = 2_f64.powf(-age_h / h);

    let n_points = f64::from(input.data_points);
    let s_series = if input.series { 1.0 } else { 0.0 };
    let d = ((1.0 + n_points).ln() / 51_f64.ln()).min(1.0) * (0.7 + 0.3 * s_series);

    let (n, n_text) = match &input.novelty {
        Novelty::NewEvent => (1.0, "new event key".to_owned()),
        Novelty::RegularSeries => (0.9, "regular series with a page last period".to_owned()),
        Novelty::FollowUp => (0.3, "follow-up of a covered story".to_owned()),
        Novelty::Similar { max_cos } => (
            (1.0 - max_cos).clamp(0.0, 1.0),
            format!("closest recent page with a shared entity at cos {max_cos:.2}"),
        ),
    };

    let targets = &cfg.coverage_targets;
    let gaps = [
        (
            primary_category.as_str(),
            bucket_gap(
                targets.category.get(primary_category).copied(),
                coverage.category.get(primary_category).copied(),
            ),
        ),
        (
            primary_region.as_str(),
            bucket_gap(
                targets.region.get(primary_region).copied(),
                coverage.region.get(primary_region).copied(),
            ),
        ),
        (
            page_type,
            bucket_gap(
                targets.page_type.get(page_type).copied(),
                coverage.page_type.get(page_type).copied(),
            ),
        ),
    ];
    let g = gaps.iter().map(|(_, v)| v).sum::<f64>() / 3.0;

    let mut q = 1.0;
    let mut q_why = Vec::new();
    if !input.redistribution {
        q *= 0.9;
        q_why.push("no redistribution ×0.9");
    }
    if input.parse_warnings {
        q *= 0.9;
        q_why.push("parse warnings ×0.9");
    }
    if input.main_entity.is_none() {
        q *= 0.8;
        q_why.push("main entity unresolved ×0.8");
    }

    let w = &cfg.weights;
    let total = round2(100.0 * q * (w.i * i + w.m * m + w.f * f + w.d * d + w.n * n + w.g * g));
    let tier = tier(total, input.main_entity.as_ref().map(|x| x.prominence));

    let mut pitch = String::new();
    let _ = writeln!(
        pitch,
        "S {total:.2} = 100 × Q {q:.3} × ({:.2}·I + {:.2}·M + {:.2}·F + {:.2}·D + {:.2}·N + {:.2}·G) [{}]; {}",
        w.i,
        w.m,
        w.f,
        w.d,
        w.n,
        w.g,
        cfg.version,
        match tier {
            Some(t) => format!("tier {t}"),
            None => "below the tier 2 floor, stays queued".into(),
        }
    );
    let _ = writeln!(
        pitch,
        "I {i:.3} importance: event {e:.3} ({}), prominence {p:.3}, source tier {} → {t:.3}",
        input.event_type, input.source_tier
    );
    let _ = writeln!(pitch, "M {m:.3} magnitude ({page_type}): {m_text}");
    let _ = writeln!(
        pitch,
        "F {f:.3} freshness: {age_h:.1} h old, half-life {h} h"
    );
    let _ = writeln!(
        pitch,
        "D {d:.3} data: {} data points, {}",
        input.data_points,
        if input.series {
            "a series of 12+ points"
        } else {
            "no series"
        }
    );
    let _ = writeln!(pitch, "N {n:.3} novelty: {n_text}");
    let _ = writeln!(
        pitch,
        "G {g:.3} coverage gap: {}",
        gaps.iter()
            .map(|(k, v)| format!("{k} {v:.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = write!(
        pitch,
        "Q {q:.3} quality: {}",
        if q_why.is_empty() {
            "no penalties".to_owned()
        } else {
            q_why.join(", ")
        }
    );

    Ok(Score {
        scorer_version: cfg.version.clone(),
        components: Components {
            i,
            e,
            p,
            t,
            m,
            f,
            d,
            n,
            g,
            q,
        },
        total,
        tier,
        pitch,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn m(page: Page) -> f64 {
        magnitude(&page).0
    }

    fn policy(change_bp: i32, statement_change: f64, holds_before: u32, reversal: bool) -> Page {
        Page::Policy {
            change_bp,
            statement_change,
            holds_before,
            reversal,
        }
    }

    #[test]
    fn policy_magnitude_follows_the_table() {
        assert_eq!(m(policy(-50, 0.0, 0, false)), 1.0);
        assert_eq!(m(policy(25, 0.0, 0, false)), 0.8);
        assert_eq!(m(policy(0, 0.25, 0, false)), 0.4);
        assert_eq!(m(policy(0, 0.20, 9, true)), 0.3, "a hold gets no bonus");
        assert!((m(policy(25, 0.0, 6, false)) - 1.0).abs() < 1e-12);
        assert!((m(policy(-25, 0.0, 0, true)) - 1.0).abs() < 1e-12);
        assert_eq!(m(policy(75, 0.0, 6, false)), 1.0, "capped at 1");
    }

    #[test]
    fn macro_magnitude_uses_the_series_and_the_ten_year_rule() {
        // 121 monthly values: alternating 100 and 101, then a new 10-year high.
        let mut values: Vec<f64> = (0..120).map(|k| 100.0 + f64::from(k % 2)).collect();
        values.push(101.5);
        let (got, text) = magnitude(&Page::Macro {
            values: values.clone(),
            per_year: 12,
        });
        assert!(got >= 0.8 && text.contains("10-year high"), "{got} {text}");
        // Without ten years of history the override cannot apply.
        let (got, text) = magnitude(&Page::Macro {
            values: values[60..].to_vec(),
            per_year: 12,
        });
        assert!(!text.contains("10-year") && got < 0.8, "{got} {text}");
        assert_eq!(
            m(Page::Macro {
                values: vec![1.0, 2.0],
                per_year: 4
            }),
            NOT_COMPUTABLE
        );
    }

    #[test]
    fn other_magnitudes() {
        assert_eq!(m(Page::News { primary_sources: 8 }), 1.0);
        let two = m(Page::News { primary_sources: 2 });
        assert!((two - 3_f64.log2() / 9_f64.log2()).abs() < 1e-12);
        let science = m(Page::Science {
            open_dataset: true,
            peer_reviewed_preprint: true,
        });
        assert!((science - 0.7).abs() < 1e-12);
        assert_eq!(
            m(Page::Report {
                revision: None,
                past_revisions: vec![0.1, 0.2]
            }),
            NOT_COMPUTABLE
        );
        let flip = m(Page::Earnings {
            growth: 5.0,
            past_growth: vec![4.0, 6.0, 4.0, 6.0],
            flip: true,
        });
        assert_eq!(flip, 0.7);
    }

    #[test]
    fn tiers() {
        assert_eq!(tier(70.0, Some(3)), Some(1));
        assert_eq!(tier(60.0, Some(1)), Some(1));
        assert_eq!(tier(69.99, Some(2)), Some(2));
        assert_eq!(tier(40.0, None), Some(2));
        assert_eq!(tier(39.99, Some(1)), None);
    }

    #[test]
    fn quality_penalties_multiply_and_bad_input_is_refused() {
        let cfg = crate::config::v1().unwrap();
        let as_of: DateTime<Utc> = "2026-11-13T18:00:00Z".parse().unwrap();
        let mut input = StoryInput {
            event_type: "news_story".into(),
            page: Page::News { primary_sources: 3 },
            categories: vec!["tech".into()],
            regions: vec!["europe".into()],
            main_entity: None,
            entities: vec![],
            source_tier: 2,
            published_at: as_of,
            data_points: 0,
            series: false,
            novelty: Novelty::Similar { max_cos: 0.4 },
            redistribution: false,
            parse_warnings: true,
            action: Action::New,
        };
        let s = score(&input, as_of, &Shares::default(), &cfg).unwrap();
        assert!((s.components.q - 0.9 * 0.9 * 0.8).abs() < 1e-12);
        assert!((s.components.n - 0.6).abs() < 1e-12);
        assert_eq!(s.components.f, 1.0);
        assert_eq!(s.components.g, 1.0, "nothing published: every gap is full");
        input.event_type = "rumour".into();
        assert!(score(&input, as_of, &Shares::default(), &cfg).is_err());
        input.event_type = "news_story".into();
        input.source_tier = 5;
        assert!(score(&input, as_of, &Shares::default(), &cfg).is_err());
    }
}
