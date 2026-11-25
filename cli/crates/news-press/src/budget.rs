//! The index budget (Bureau Design §3.12, Edition step 11): code only, never in the background.
//!
//! Take published `noindex` pages, leave out any flagged `templated` (they stay `noindex` until Reporter
//! upgrades them, D10), rank them by `index_priority = score + 10·[Tier 1] + uniqueness` and promote
//! until the day's cap (`publish.index_per_day`, `NEWS_INDEX_PER_DAY`) is used up. Uniqueness is the
//! number of the page's series (`sr_` refs, the charts' data) that no indexed page has; it is counted
//! again after each pick, so two pages with the same new series do not both get the bonus.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

/// A published, not yet indexed page.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub page_id: String,
    pub tier: Option<i16>,
    /// The latest score `S` of the page's story (0–100); 0 without one.
    pub score: f64,
    pub flags: Vec<String>,
    /// The `sr_` refs of the page's current version.
    pub series: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub page_id: String,
    pub priority: f64,
    pub uniqueness: usize,
}

fn uniqueness(c: &Candidate, indexed: &HashSet<String>) -> usize {
    c.series
        .iter()
        .collect::<HashSet<_>>()
        .iter()
        .filter(|s| !indexed.contains(**s))
        .count()
}

/// `S + 10·[Tier 1] + uniqueness`.
pub fn index_priority(score: f64, tier: Option<i16>, uniqueness: usize) -> f64 {
    score + if tier == Some(1) { 10.0 } else { 0.0 } + uniqueness as f64
}

/// Up to `room` pages to promote, best first. `indexed` is the series the indexed pages already carry.
/// Ties go to the older page, then the smaller page id, so a re-run picks the same pages.
pub fn select(mut pool: Vec<Candidate>, mut indexed: HashSet<String>, room: usize) -> Vec<Pick> {
    pool.retain(|c| !c.flags.iter().any(|f| f == "templated"));
    let mut picks = Vec::new();
    while picks.len() < room && !pool.is_empty() {
        let scored: Vec<(f64, usize)> = pool
            .iter()
            .map(|c| {
                let u = uniqueness(c, &indexed);
                (index_priority(c.score, c.tier, u), u)
            })
            .collect();
        let best = (0..pool.len()).min_by(|&a, &b| {
            scored[b]
                .0
                .total_cmp(&scored[a].0)
                .then(pool[a].published_at.cmp(&pool[b].published_at))
                .then(pool[a].page_id.cmp(&pool[b].page_id))
        });
        let Some(i) = best else { break };
        let c = pool.swap_remove(i);
        indexed.extend(c.series.iter().cloned());
        picks.push(Pick {
            page_id: c.page_id,
            priority: scored[i].0,
            uniqueness: scored[i].1,
        });
    }
    picks
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn page(id: &str, tier: i16, score: f64, series: &[&str]) -> Candidate {
        Candidate {
            page_id: id.into(),
            tier: Some(tier),
            score,
            flags: vec![],
            series: series.iter().map(|s| (*s).to_owned()).collect(),
            published_at: None,
        }
    }

    fn ids(p: &[Pick]) -> Vec<&str> {
        p.iter().map(|p| p.page_id.as_str()).collect()
    }

    #[test]
    fn the_priority_formula() {
        assert_eq!(index_priority(50.0, Some(1), 3), 63.0);
        assert_eq!(index_priority(50.0, Some(2), 3), 53.0);
        assert_eq!(index_priority(50.0, None, 0), 50.0);
    }

    #[test]
    fn tier_1_goes_first_at_equal_scores() {
        let pool = vec![page("b-t2", 2, 70.0, &[]), page("a-t1", 1, 70.0, &[])];
        assert_eq!(ids(&select(pool, HashSet::new(), 2)), ["a-t1", "b-t2"]);
    }

    #[test]
    fn the_cap_limits_the_picks_and_zero_room_picks_none() {
        let pool: Vec<_> = (0..5)
            .map(|i| page(&format!("p{i}"), 2, 50.0 + f64::from(i), &[]))
            .collect();
        let picks = select(pool.clone(), HashSet::new(), 2);
        assert_eq!(ids(&picks), ["p4", "p3"]);
        assert!(select(pool, HashSet::new(), 0).is_empty());
    }

    #[test]
    fn templated_pages_are_never_picked() {
        let mut t = page("templated", 1, 99.0, &["sr_a"]);
        t.flags = vec!["templated".into()];
        let picks = select(vec![t, page("plain", 2, 10.0, &[])], HashSet::new(), 5);
        assert_eq!(ids(&picks), ["plain"]);
    }

    #[test]
    fn uniqueness_counts_series_no_indexed_page_has_and_is_recounted_after_each_pick() {
        let indexed: HashSet<String> = ["sr_old".to_owned()].into();
        // a: 2 new series (+2); b: the same 2 (+2 until a is picked, then 0); c: only an indexed one (0).
        let pool = vec![
            page("a", 2, 50.0, &["sr_x", "sr_y"]),
            page("b", 2, 50.0, &["sr_x", "sr_y"]),
            page("c", 2, 51.0, &["sr_old"]),
        ];
        let picks = select(pool, indexed, 3);
        assert_eq!(ids(&picks), ["a", "c", "b"]);
        assert_eq!(picks[0].uniqueness, 2);
        assert_eq!(picks[0].priority, 52.0);
        assert_eq!(picks[2].uniqueness, 0);
    }

    #[test]
    fn ties_go_to_the_older_page_then_the_smaller_id() {
        let t = |h: i64| Some(DateTime::from_timestamp(h * 3600, 0).unwrap());
        let mut a = page("z", 2, 50.0, &[]);
        a.published_at = t(1);
        let mut b = page("a", 2, 50.0, &[]);
        b.published_at = t(2);
        let c = page("b", 2, 50.0, &[]);
        // `None` sorts before any time, so c (unknown time) first, then z (older), then a.
        assert_eq!(
            ids(&select(vec![b, a, c], HashSet::new(), 3)),
            ["b", "z", "a"]
        );
    }
}
