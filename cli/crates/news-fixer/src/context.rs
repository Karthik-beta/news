//! Context lines computed by code (Bureau §3.8): "lowest since …", "record in the series", "first cut
//! since …". Each names the rule that produced it, and [`recheck`] runs the rules again over the same
//! series, so a line can be verified rather than trusted.
//!
//! A series is one series' observations oldest first, as of the story's data; its last point is the
//! story's latest. Values compare as exact decimals; points without a number or a period end are left
//! out. "Since" claims are only about the observations we hold, and the texts say so where it matters.

use crate::inputs::Point;
use news_ledger::decimal::Dec;
use std::cmp::Ordering;

pub const SERIES_MIN_SINCE: &str = "series_min_since";
pub const SERIES_MAX_SINCE: &str = "series_max_since";
pub const SERIES_RECORD_LOW: &str = "series_record_low";
pub const SERIES_RECORD_HIGH: &str = "series_record_high";
pub const FIRST_CUT_SINCE: &str = "first_cut_since";
pub const FIRST_HIKE_SINCE: &str = "first_hike_since";

/// A "since" must reach back at least this far to be news.
const MIN_GAP_DAYS: i64 = 365;
/// A record needs at least this much history behind it.
const MIN_RECORD_DAYS: i64 = 730;

/// One line a rule produced. `since` is the earlier observation the line names, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Found<'a> {
    pub rule: &'static str,
    pub text: String,
    pub since: Option<&'a Point>,
}

fn cmp(a: Dec, b: Dec) -> Option<Ordering> {
    Some(a.checked_sub(b)?.m.cmp(&0))
}

/// The numeric observations with a period end, oldest first.
fn usable<'a>(series: &[&'a Point]) -> Vec<(&'a Point, Dec)> {
    series
        .iter()
        .filter(|p| p.period_end.is_some())
        .filter_map(|p| Some((*p, p.dec()?)))
        .collect()
}

fn days(a: &Point, b: &Point) -> i64 {
    match (a.period_end, b.period_end) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// Every line the rules find for `series`. `policy` turns on the cut and hike rules (policy rates).
pub fn lines<'a>(series: &[&'a Point], policy: bool) -> Vec<Found<'a>> {
    let s = usable(series);
    let Some(&(last, lv)) = s.last() else {
        return Vec::new();
    };
    let earlier = &s[..s.len() - 1];
    let mut out = Vec::new();
    for (want, since_rule, record_rule, word) in [
        (
            Ordering::Greater,
            SERIES_MIN_SINCE,
            SERIES_RECORD_LOW,
            "lowest",
        ),
        (
            Ordering::Less,
            SERIES_MAX_SINCE,
            SERIES_RECORD_HIGH,
            "highest",
        ),
    ] {
        // The most recent earlier observation at or beyond the latest value (≤ for a low, ≥ for a high).
        let mut beaten = true;
        let mut since = None;
        let mut skipped = 0;
        for &(p, v) in earlier.iter().rev() {
            match cmp(v, lv) {
                Some(o) if o == want => skipped += 1,
                Some(_) => {
                    since = Some(p);
                    break;
                }
                None => {
                    beaten = false;
                    break;
                }
            }
        }
        if !beaten {
            continue;
        }
        match since {
            // "Since" needs observations in between that it beat, and a year or more.
            Some(p) if skipped > 0 && days(p, last) >= MIN_GAP_DAYS => out.push(Found {
                rule: since_rule,
                text: format!("{}: {word} since {}", last.label, p.period_label),
                since: Some(p),
            }),
            None if earlier
                .first()
                .is_some_and(|(first, _)| days(first, last) >= MIN_RECORD_DAYS) =>
            {
                let first = earlier[0].0;
                out.push(Found {
                    rule: record_rule,
                    text: format!(
                        "{}: {word} of every observation held, which begin in {}",
                        last.label, first.period_label
                    ),
                    since: Some(first),
                });
            }
            _ => {}
        }
    }
    if policy && s.len() >= 3 {
        let step = |i: usize| cmp(s[i].1, s[i - 1].1);
        for (dir, rule, word) in [
            (Ordering::Less, FIRST_CUT_SINCE, "cut"),
            (Ordering::Greater, FIRST_HIKE_SINCE, "rise"),
        ] {
            if step(s.len() - 1) != Some(dir) {
                continue;
            }
            // The previous move the same way, with at least one other observation in between.
            if let Some(k) = (1..s.len() - 1).rev().find(|&i| step(i) == Some(dir))
                && k + 2 < s.len()
            {
                out.push(Found {
                    rule,
                    text: format!("{}: first {word} since {}", last.label, s[k].0.period_label),
                    since: Some(s[k].0),
                });
            }
        }
    }
    out
}

/// Whether the rule `rule` still produces exactly `text` over `series`.
pub fn recheck(rule: &str, text: &str, series: &[&Point], policy: bool) -> bool {
    lines(series, policy)
        .iter()
        .any(|f| f.rule == rule && f.text == text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};

    fn obs(values: &[&str]) -> Vec<Point> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let year = 2015 + i32::try_from(i).unwrap();
                Point {
                    dp_id: format!("dp_{i}"),
                    series_key: "s".into(),
                    label: "Rate".into(),
                    value_text: (*v).into(),
                    unit: "Percent".into(),
                    currency: None,
                    scale: None,
                    seasonal_adj: None,
                    period_label: format!("Y{i}"),
                    period_start: NaiveDate::from_ymd_opt(year, 1, 1),
                    period_end: NaiveDate::from_ymd_opt(year, 12, 31),
                    period_type: news_ledger::PeriodType::Duration,
                    fiscal_year: None,
                    fiscal_period: None,
                    vintage: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                }
            })
            .collect()
    }

    fn rules(values: &[&str], policy: bool) -> Vec<(String, String)> {
        let pts = obs(values);
        let refs: Vec<&Point> = pts.iter().collect();
        lines(&refs, policy)
            .into_iter()
            .map(|f| (f.rule.to_owned(), f.text))
            .collect()
    }

    #[test]
    fn lowest_since_names_the_last_observation_at_or_below() {
        assert_eq!(
            rules(&["2.0", "3.0", "2.5", "2.9", "2.1"], false),
            [(SERIES_MIN_SINCE.into(), "Rate: lowest since Y0".into())]
        );
        // Equal counts as "since"; a gap under a year is no news.
        assert_eq!(
            rules(&["2.1", "3.0", "2.1"], false)[0].1,
            "Rate: lowest since Y0"
        );
        assert!(
            rules(&["3", "2.5"], false).is_empty(),
            "one step down from the only earlier value"
        );
    }

    #[test]
    fn a_record_needs_two_years_behind_it() {
        assert_eq!(
            rules(&["2", "3", "4", "5"], false),
            [(
                SERIES_RECORD_HIGH.into(),
                "Rate: highest of every observation held, which begin in Y0".into()
            )]
        );
        assert!(rules(&["2", "3"], false).is_empty());
    }

    #[test]
    fn first_cut_since_skips_back_to_back_cuts() {
        let found = rules(&["5", "4", "4.5", "4.75", "4.25"], true);
        assert!(found.contains(&(FIRST_CUT_SINCE.into(), "Rate: first cut since Y1".into())));
        let back_to_back = rules(&["5", "4.5", "4"], true);
        assert!(back_to_back.iter().all(|(r, _)| r != FIRST_CUT_SINCE));
        assert!(
            rules(&["5", "4", "4.5", "4.75", "4.25"], false)
                .iter()
                .all(|(r, _)| r != FIRST_CUT_SINCE),
            "cuts are only for policy pages"
        );
    }

    #[test]
    fn exact_decimals_and_non_numbers() {
        // 2.10 equals 2.1 exactly; a non-number is left out, not read as zero.
        assert_eq!(
            rules(&["2.1", "x", "3", "2.10"], false)[0].1,
            "Rate: lowest since Y0"
        );
    }

    #[test]
    fn recheck_accepts_only_what_the_rules_say() {
        let pts = obs(&["2.0", "3.0", "2.5", "2.9", "2.1"]);
        let refs: Vec<&Point> = pts.iter().collect();
        assert!(recheck(
            SERIES_MIN_SINCE,
            "Rate: lowest since Y0",
            &refs,
            false
        ));
        assert!(!recheck(
            SERIES_MIN_SINCE,
            "Rate: lowest since Y2",
            &refs,
            false
        ));
        assert!(!recheck(
            SERIES_RECORD_LOW,
            "Rate: lowest since Y0",
            &refs,
            false
        ));
    }
}
