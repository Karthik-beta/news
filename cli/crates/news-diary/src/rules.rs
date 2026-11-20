//! The per-source rule tables that turn calendar event summaries into event keys (Bureau §3.3, "map
//! event summaries to event keys with a per-source rule table").
//!
//! Only events a built Stringer can capture are mapped: a calendar entry nobody can ever capture
//! would sit `expected` and go `late` for nothing. Events the table does not know are skipped.
//! Reference periods are read from the summary where the source states one (BEA); for BLS, whose
//! summaries name no period, the reference month is the month before the release day, the pattern
//! every one of these releases follows (CPI for October is published in November).

use chrono::{Datelike, Months, NaiveDate};

use news_connectors::CalendarEntry;
use news_ledger::eventkey;

use crate::ics::IcsEvent;

/// `YYYY-MM` of the month before `day`.
fn previous_month(day: NaiveDate) -> Option<String> {
    let m = day.checked_sub_months(Months::new(1))?;
    Some(format!("{:04}-{:02}", m.year(), m.month()))
}

/// `(1st|2nd|3rd|4th) Quarter YYYY` → `YYYY-Qn`.
fn quarter_of(summary: &str) -> Option<String> {
    let words: Vec<&str> = summary
        .split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .collect();
    let i = words.iter().position(|w| *w == "Quarter")?;
    let q = match *words.get(i.checked_sub(1)?)? {
        "1st" => 1,
        "2nd" => 2,
        "3rd" => 3,
        "4th" => 4,
        _ => return None,
    };
    // "3rd Quarter 2026" or "4th Quarter and Year 2026": the first year after "Quarter".
    let year: i32 = words[i + 1..]
        .iter()
        .find_map(|w| w.parse().ok().filter(|y| (1900..2200).contains(y)))?;
    Some(format!("{year:04}-Q{q}"))
}

/// The `(event key, title)` pairs a calendar event stands for, for a calendar of `source_id`.
pub fn keys_for(source_id: &str, summary: &str, day: NaiveDate) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match source_id {
        "bls" => {
            let families: &[&str] = match summary {
                "Consumer Price Index" => &["cpi"],
                "Employment Situation" => &["payrolls", "unemployment"],
                _ => &[],
            };
            if let Some(period) = previous_month(day) {
                for f in families {
                    if let Some(k) = eventkey::macro_release("us", f, &period) {
                        out.push((k, format!("BLS {summary} ({f})")));
                    }
                }
            }
        }
        "bea" => {
            // The advance estimate is the release; the second and third are revisions of the
            // same period and share its event key, so they are not separate calendar events.
            if summary.contains("Advance Estimate")
                && let Some(q) = quarter_of(summary)
                && let Some(k) = eventkey::macro_release("us", "gdp", &q)
            {
                out.push((k, format!("BEA {summary}")));
            }
        }
        _ => {}
    }
    out
}

/// Entries for the events of one ICS file.
pub fn entries_from_ics(source_id: &str, events: &[IcsEvent]) -> Vec<CalendarEntry> {
    let mut out: Vec<CalendarEntry> = Vec::new();
    for e in events {
        let (day, at) = e.start.resolve();
        for (event_key, title) in keys_for(source_id, &e.summary, day) {
            // One key, one event: the earliest date wins (a file may list a release twice).
            if let Some(prev) = out.iter_mut().find(|p| p.event_key == event_key) {
                if day < prev.expected_day {
                    prev.expected_day = day;
                    prev.expected_at = at;
                }
                continue;
            }
            out.push(CalendarEntry {
                source_id: source_id.into(),
                event_key,
                title,
                expected_day: day,
                expected_at: at,
                window_days: 0,
                precision: if at.is_some() { "exact" } else { "day" }.into(),
                origin: "ics".into(),
            });
        }
    }
    out.sort_by(|a, b| (a.expected_day, &a.event_key).cmp(&(b.expected_day, &b.event_key)));
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn bls_reference_month_is_the_month_before_the_release() {
        let k = keys_for("bls", "Consumer Price Index", d(2026, 12, 10));
        assert_eq!(k[0].0, "macro:us.cpi:2026-11");
        // A January release is for December of the year before.
        let k = keys_for("bls", "Employment Situation", d(2027, 1, 8));
        assert_eq!(
            k.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["macro:us.payrolls:2026-12", "macro:us.unemployment:2026-12"]
        );
        assert!(keys_for("bls", "Real Earnings", d(2026, 12, 10)).is_empty());
    }

    #[test]
    fn bea_reads_the_quarter_from_the_summary_and_skips_revisions() {
        let adv = "GDP (Advance Estimate), 3rd Quarter 2026";
        assert_eq!(
            keys_for("bea", adv, d(2026, 10, 29))[0].0,
            "macro:us.gdp:2026-Q3"
        );
        let old = "Gross Domestic Product, 4th Quarter and Year 2024 (Advance Estimate)";
        assert_eq!(
            keys_for("bea", old, d(2025, 1, 30))[0].0,
            "macro:us.gdp:2024-Q4"
        );
        let q4 = "GDP (Advance Estimate), 4th Quarter and Year 2026";
        assert_eq!(
            keys_for("bea", q4, d(2027, 1, 28))[0].0,
            "macro:us.gdp:2026-Q4"
        );
        assert!(
            keys_for(
                "bea",
                "GDP (Second Estimate) and Corporate Profits, 3rd Quarter 2026",
                d(2026, 11, 25)
            )
            .is_empty()
        );
        assert!(
            keys_for(
                "bea",
                "Personal Income and Outlays, October 2026",
                d(2026, 11, 25)
            )
            .is_empty()
        );
    }
}
