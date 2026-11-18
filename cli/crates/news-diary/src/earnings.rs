//! Derived earnings windows (Bureau §3.3): no reliable free earnings-date source exists, so the
//! expected window is the same fiscal period last year, plus or minus [`WINDOW_DAYS`], from the
//! company's own filing history. Inside the window the SEC 8-K Atom feed is polled every
//! [`POLL_MINUTES`] minutes for that CIK.
//!
//! The company's own fiscal year and period are used as filed: nothing here maps a fiscal period to
//! a calendar quarter. "Same period next year" is the next fiscal year number with the same period
//! code, filed one calendar year after the last one.

use std::collections::BTreeMap;

use chrono::{DateTime, Months, NaiveDate, Utc};

use news_connectors::CalendarEntry;
use news_ledger::eventkey;

pub const WINDOW_DAYS: u16 = 7;
pub const POLL_MINUTES: u32 = 10;

/// One report the company filed: its own fiscal year and period, and the day it was filed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filing {
    pub cik: String,
    pub fiscal_year: i16,
    pub fiscal_period: String,
    pub filed: NaiveDate,
}

/// Windows for the next report of each fiscal period in `history`, still open on `today`
/// (`expected_day + window ≥ today`) and starting within a year. A period filed twice (an
/// amendment) is dated by its first filing.
pub fn derive(history: &[Filing], today: NaiveDate) -> Vec<CalendarEntry> {
    let mut first: BTreeMap<(String, i16, String), NaiveDate> = BTreeMap::new();
    for f in history {
        let k = (f.cik.clone(), f.fiscal_year, f.fiscal_period.clone());
        first
            .entry(k)
            .and_modify(|d| *d = (*d).min(f.filed))
            .or_insert(f.filed);
    }
    let mut out = Vec::new();
    for ((cik, fy, fp), filed) in first {
        let Some(expected) = filed.checked_add_months(Months::new(12)) else {
            continue;
        };
        let end = expected + chrono::Duration::days(i64::from(WINDOW_DAYS));
        if end < today || expected > today + chrono::Duration::days(366) {
            continue;
        }
        let Some(key) = eventkey::earnings(&cik, fy + 1, &fp) else {
            continue;
        };
        out.push(CalendarEntry {
            source_id: "sec-edgar".into(),
            event_key: key,
            title: format!(
                "Earnings {}{} (expected, from last year's filing on {filed})",
                fy + 1,
                fp
            ),
            expected_day: expected,
            expected_at: None,
            window_days: WINDOW_DAYS,
            precision: "window".into(),
            origin: "derived".into(),
        });
    }
    out.sort_by(|a, b| (a.expected_day, &a.event_key).cmp(&(b.expected_day, &b.event_key)));
    out
}

/// The 10-digit CIK inside an `earnings:<cik>:…` event key.
pub fn cik_of(event_key: &str) -> Option<&str> {
    let mut p = event_key.splitn(3, ':');
    (p.next()? == "earnings").then_some(())?;
    p.next()
}

/// CIKs whose window is open at `now` and whose event is still `expected`: the ones to poll for 8-Ks
/// in this ten-minute round. `events` is `(event_key, expected_day, window_days)`.
pub fn due_for_polling(events: &[(String, NaiveDate, u16)], now: DateTime<Utc>) -> Vec<String> {
    let day = now.date_naive();
    let mut out: Vec<String> = events
        .iter()
        .filter(|(_, d, w)| {
            let w = chrono::Duration::days(i64::from(*w));
            day >= *d - w && day <= *d + w
        })
        .filter_map(|(k, ..)| cik_of(k).map(str::to_owned))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn f(fy: i16, fp: &str, filed: NaiveDate) -> Filing {
        Filing {
            cik: "320193".into(),
            fiscal_year: fy,
            fiscal_period: fp.into(),
            filed,
        }
    }

    #[test]
    fn next_years_window_uses_the_companys_own_fiscal_labels() {
        // Apple's fiscal 2026 Q4 ends in September: filed 30 Oct 2026. The next Q4 is fiscal 2027.
        let h = [f(2026, "Q4", d(2026, 10, 30)), f(2026, "Q1", d(2026, 2, 1))];
        let e = derive(&h, d(2026, 11, 18));
        assert_eq!(e.len(), 2);
        let q4 = e.iter().find(|e| e.event_key.ends_with("2027Q4")).unwrap();
        assert_eq!(q4.event_key, "earnings:0000320193:2027Q4");
        assert_eq!(q4.expected_day, d(2027, 10, 30));
        assert_eq!(
            (q4.window_days, q4.precision.as_str(), q4.origin.as_str()),
            (7, "window", "derived")
        );
        assert!(q4.expected_at.is_none());
        assert_eq!(
            e[0].event_key, "earnings:0000320193:2027Q1",
            "sorted by expected day"
        );
    }

    #[test]
    fn windows_that_have_closed_or_are_far_off_are_not_derived() {
        let h = [f(2025, "Q1", d(2025, 11, 1)), f(2020, "FY", d(2021, 1, 1))];
        // Window for 2026Q1 is 1–8 Nov 2026: closed by 18 Nov. The 2020 history is years old.
        assert!(derive(&h, d(2026, 11, 18)).is_empty());
        // 8 days before the window opens it is not derived for tomorrow's horizon either way.
        let e = derive(&[f(2025, "Q1", d(2025, 11, 20))], d(2026, 11, 18));
        assert_eq!(e.len(), 1, "still open or upcoming");
    }

    #[test]
    fn an_amendment_does_not_move_the_date() {
        let h = [
            f(2026, "FY", d(2026, 11, 2)),
            f(2026, "FY", d(2026, 12, 20)),
        ];
        let e = derive(&h, d(2026, 12, 21));
        assert_eq!(e[0].expected_day, d(2027, 11, 2));
    }

    #[test]
    fn leap_day_filings_clamp_to_the_end_of_february() {
        let e = derive(&[f(2024, "Q2", d(2024, 2, 29))], d(2025, 2, 20));
        assert_eq!(e[0].expected_day, d(2025, 2, 28));
    }

    #[test]
    fn polling_runs_only_inside_the_window() {
        let ev = vec![(
            "earnings:0000320193:2027Q4".to_owned(),
            d(2027, 10, 30),
            7u16,
        )];
        let at = |m, day| Utc.with_ymd_and_hms(2027, m, day, 12, 0, 0).unwrap();
        assert!(due_for_polling(&ev, at(10, 22)).is_empty());
        assert_eq!(due_for_polling(&ev, at(10, 23)), ["0000320193"]);
        assert_eq!(due_for_polling(&ev, at(11, 6)), ["0000320193"]);
        assert!(due_for_polling(&ev, at(11, 7)).is_empty());
    }
}
