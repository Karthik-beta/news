//! Bureau Design §4.6, the Desk test fixture: the five scores, the four picks with their planned
//! tiers, and the target-10 case (entity limit, Tier 1 quota of 2). Plus idempotency and the golden
//! `news plan` output.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{DateTime, Utc};
use news_nose::config::{self, Shares};
use news_nose::desk::{self, Candidate};
use news_nose::score::{self, Action, Score, StoryInput};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    as_of: DateTime<Utc>,
    coverage: Shares,
    stories: Vec<Story>,
}

#[derive(Deserialize)]
struct Story {
    id: i64,
    name: String,
    input: StoryInput,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/worked-example.json")).unwrap()
}

fn scored() -> Vec<(i64, String, StoryInput, Score)> {
    let f = fixture();
    let cfg = config::v1().unwrap();
    f.stories
        .into_iter()
        .map(|s| {
            let sc = score::score(&s.input, f.as_of, &f.coverage, &cfg).unwrap();
            (s.id, s.name, s.input, sc)
        })
        .collect()
}

fn candidates() -> Vec<Candidate> {
    scored()
        .iter()
        .map(|(id, _, input, s)| Candidate::new(*id, input, s))
        .collect()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn the_five_scores_match_the_worked_example() {
    // (I, M, F, D, N, G, S, tier) from the §4.6 table; components as printed (3 decimals, within 0.001), S to 1.
    let table = [
        (0.950, 0.528, 0.841, 0.945, 0.9, 0.033, 75.2, 1),
        (0.975, 0.283, 0.697, 1.000, 1.0, 0.211, 72.5, 1),
        (1.000, 0.300, 0.917, 0.652, 1.0, 0.167, 71.2, 1),
        (0.850, 0.221, 0.749, 1.000, 1.0, 0.040, 66.6, 1),
        (0.450, 0.300, 0.749, 0.346, 1.0, 0.433, 50.3, 2),
    ];
    let scored = scored();
    for ((id, name, _, s), (i, m, f, d, n, g, total, tier)) in scored.iter().zip(table) {
        let c = s.components;
        for (sym, got, want) in [
            ("I", c.i, i),
            ("M", c.m, m),
            ("F", c.f, f),
            ("D", c.d, d),
            ("N", c.n, n),
            ("G", c.g, g),
        ] {
            assert!(
                close(got, want, 0.001),
                "{name} (story {id}): {sym} is {got}, the table says {want}"
            );
        }
        assert_eq!(c.q, 1.0, "{name}");
        assert_eq!(
            (s.total * 10.0).round() / 10.0,
            total,
            "{name}: S {}",
            s.total
        );
        assert_eq!(s.tier, Some(tier), "{name}");
    }
    // The sixth story, US PPI, is S = 70.0 in the selection example.
    assert_eq!((scored[5].3.total * 10.0).round() / 10.0, 70.0);
}

#[test]
fn a_target_of_4_picks_cpi_imf_apple_and_the_preprint() {
    let plan = desk::select(&candidates(), 4);
    assert_eq!(
        (
            plan.limits.category_cap,
            plan.limits.tier1_quota,
            plan.limits.update_cap
        ),
        (2, 1, 0)
    );
    let picks: Vec<(i64, u8)> = plan.picks.iter().map(|p| (p.story_id, p.tier)).collect();
    // CPI (tier 1, uses the quota), IMF WEO and Apple (tier 1 earned, planned 2), the preprint.
    assert_eq!(picks, [(1, 1), (2, 2), (4, 2), (5, 2)]);
    // The table's adjusted values were worked from S rounded to 1 decimal; ours use S to 2.
    let adjusted: Vec<f64> = plan.picks.iter().map(|p| p.adjusted).collect();
    for (got, want) in adjusted.iter().zip([75.2, 66.7, 61.3, 46.3]) {
        assert!(close(*got, want, 0.1), "{adjusted:?}");
    }
    assert!(
        plan.picks[1].reason.contains("planned tier 2"),
        "{:?}",
        plan.picks[1]
    );
    // RBI and US PPI are blocked by the Economy cap and stay queued.
    let queued: Vec<(i64, &str)> = plan
        .queued
        .iter()
        .map(|h| (h.story_id, h.reason.as_str()))
        .collect();
    assert_eq!(
        queued,
        [
            (3, "category cap: economy has 2 of 2"),
            (6, "category cap: economy has 2 of 2")
        ]
    );
    assert_eq!(plan.below_floor, 0);
}

/// (story id, S, main-entity prominence), categories, region, page type, main entity, action.
fn extra(
    (story_id, s, prominence): (i64, f64, u8),
    categories: &[&str],
    region: &str,
    page_type: &str,
    entity: &str,
    action: Action,
) -> Candidate {
    Candidate {
        story_id,
        s,
        tier: score::tier(s, Some(prominence)),
        categories: categories.iter().map(|c| (*c).to_owned()).collect(),
        regions: vec![region.to_owned()],
        page_type: page_type.to_owned(),
        main_entity: Some(entity.to_owned()),
        action,
    }
}

#[test]
fn a_target_of_10_applies_the_entity_limit_and_a_tier_1_quota_of_2() {
    use Action::{New, Update};
    let mut cands = candidates();
    #[rustfmt::skip]
    cands.extend([
        extra((7, 64.0, 1), &["finance", "tech"], "north-america", "news", "401", New), // Apple 8-K
        extra((8, 62.0, 1), &["tech"], "europe", "news", "401", New), // Apple, EU ruling
        extra((9, 73.0, 1), &["economy"], "europe", "policy", "601", New), // ECB decision
        extra((10, 55.0, 2), &["economy"], "australia", "macro", "701", New), // Australia CPI
        extra((11, 35.0, 3), &["tech"], "china", "news", "801", New), // below the floor
        extra((12, 62.0, 1), &["economy"], "north-america", "macro", "101", Update), // US GDP flip
        extra((13, 58.0, 2), &["finance"], "europe", "earnings", "901", Update),
    ]);
    let plan = desk::select(&cands, 10);
    assert_eq!(
        (
            plan.limits.category_cap,
            plan.limits.entity_limit,
            plan.limits.tier1_quota,
            plan.limits.update_cap
        ),
        (4, 2, 2, 1)
    );
    let picks: Vec<(i64, u8, Action)> = plan
        .picks
        .iter()
        .map(|p| (p.story_id, p.tier, p.action))
        .collect();
    // Worked by hand (ADR 0012): CPI 75.17 and ECB 73 × 0.92 take the two Tier 1 places; IMF, Apple
    // and RBI earned tier 1 and are planned tier 2; Economy then has 4 (the cap); the preprint; the
    // Apple EU ruling (Apple's second story). One update fits the 10 % cap, after the quota is used.
    assert_eq!(
        picks,
        [
            (1, 1, New),
            (9, 1, New),
            (2, 2, New),
            (4, 2, New),
            (3, 2, New),
            (5, 2, New),
            (8, 2, New),
            (12, 2, Update),
        ]
    );
    let queued: Vec<(i64, &str)> = plan
        .queued
        .iter()
        .map(|h| (h.story_id, h.reason.as_str()))
        .collect();
    assert_eq!(
        queued,
        [
            (6, "category cap: economy has 4 of 4"),
            (7, "entity limit: 401 has 2 of 2"),
            (10, "category cap: economy has 4 of 4"),
            (13, "update cap: AI updates take at most 1 of a 10-page run"),
        ]
    );
    assert_eq!(plan.below_floor, 1);
    assert_eq!(plan.picks.iter().filter(|p| p.tier == 1).count(), 2);
}

#[test]
fn re_running_scoring_and_selection_changes_nothing() {
    assert_eq!(scored(), scored());
    let c = candidates();
    assert_eq!(desk::select(&c, 4), desk::select(&c, 4));
}

#[test]
fn every_score_has_a_version_components_and_a_pitch() {
    for (id, name, _, s) in scored() {
        assert_eq!(s.scorer_version, "v1", "{name}");
        let comp = serde_json::to_value(s.components).unwrap();
        for k in ["I", "E", "P", "T", "M", "F", "D", "N", "G", "Q"] {
            let v = comp[k].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&v), "story {id} {k} = {v}");
        }
        let lines: Vec<&str> = s.pitch.lines().collect();
        assert_eq!(lines.len(), 8, "{}", s.pitch);
        for (line, sym) in lines[1..].iter().zip(["I", "M", "F", "D", "N", "G", "Q"]) {
            assert!(line.starts_with(&format!("{sym} ")), "{line}");
        }
    }
}

/// The golden `news plan --run` output for the worked example (the Postgres test checks the command
/// prints the same). Set `NEWS_UPDATE_GOLDEN=1` to rewrite it, then read the diff.
#[test]
fn plan_output_matches_the_golden_file() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/plan-worked-example.json"
    );
    let got = desk::summary(
        "00000000-0000-4000-8000-000000000046",
        &desk::select(&candidates(), 4),
    );
    if std::env::var_os("NEWS_UPDATE_GOLDEN").is_some() {
        std::fs::write(path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(got, want);
}
