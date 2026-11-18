//! The Fed's FOMC calendar page (`monetarypolicy/fomccalendars.htm`).
//!
//! The page has one panel per year (`<h4><a …>2027 FOMC Meetings</a></h4>`) of rows with a month
//! cell (`<strong>January</strong>`, or `Apr/May`) and a date cell (`27-28`, `17-18*`, `30-1`). The
//! statement comes out on the last day: `policy:fed:<that date>`, the same key the `cb-fed`
//! Stringer gives the statement. A `*` marks a meeting with projections; it does not change the date.

use chrono::NaiveDate;

use news_connectors::CalendarEntry;
use news_ledger::eventkey;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FomcError(pub String);

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn month_number(name: &str) -> Option<u32> {
    let n = name.trim();
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(n) || m[..3].eq_ignore_ascii_case(n))
        .and_then(|i| u32::try_from(i + 1).ok())
}

/// The text after the first `>` and before the next `<` at or after `from`, and the position after it.
fn cell(html: &str, from: usize) -> Option<(String, usize)> {
    let open = from + html[from..].find('>')? + 1;
    // The month cell wraps its text in <strong>.
    let mut start = open;
    if html[start..].trim_start().starts_with("<strong>") {
        start = start + html[start..].find("<strong>")? + "<strong>".len();
    }
    let end = start + html[start..].find('<')?;
    Some((html[start..end].trim().to_owned(), end))
}

/// Every meeting on the page: `(first day, last day)`.
pub fn meetings(html: &str) -> Result<Vec<(NaiveDate, NaiveDate)>, FomcError> {
    let mut out = Vec::new();
    let mut year: Option<i32> = None;
    let mut month: Option<String> = None;
    let mut pos = 0;
    // Walk the three markers in document order.
    loop {
        let heading = html[pos..].find("FOMC Meetings</a>").map(|i| pos + i);
        let m = html[pos..].find("fomc-meeting__month").map(|i| pos + i);
        let dt = html[pos..].find("fomc-meeting__date").map(|i| pos + i);
        let Some(next) = [heading, m, dt].into_iter().flatten().min() else {
            break;
        };
        if Some(next) == heading {
            let y = html
                .get(next.saturating_sub(5)..next)
                .and_then(|s| s.trim().parse::<i32>().ok())
                .ok_or_else(|| FomcError("a panel heading has no year".into()))?;
            year = Some(y);
            month = None;
            pos = next + "FOMC Meetings</a>".len();
        } else if Some(next) == m {
            let (text, end) =
                cell(html, next).ok_or_else(|| FomcError("a month cell is cut off".into()))?;
            month = Some(text);
            pos = end;
        } else {
            let (text, end) =
                cell(html, next).ok_or_else(|| FomcError("a date cell is cut off".into()))?;
            pos = end;
            let (Some(y), Some(mo)) = (year, month.as_deref()) else {
                return Err(FomcError(format!("date {text:?} before any year or month")));
            };
            let text = text.trim_end_matches('*').trim();
            let (a, b) = text.split_once('-').unwrap_or((text, text));
            let (a, b): (u32, u32) = (
                a.trim()
                    .parse()
                    .map_err(|_| FomcError(format!("date {text:?} is not day-day")))?,
                b.trim()
                    .parse()
                    .map_err(|_| FomcError(format!("date {text:?} is not day-day")))?,
            );
            let (m1, m2) = mo.split_once('/').unwrap_or((mo, mo));
            let (m1, m2) = (
                month_number(m1)
                    .ok_or_else(|| FomcError(format!("month {mo:?} is not a month")))?,
                month_number(m2)
                    .ok_or_else(|| FomcError(format!("month {mo:?} is not a month")))?,
            );
            let start = NaiveDate::from_ymd_opt(y, m1, a)
                .ok_or_else(|| FomcError(format!("{y}-{m1}-{a} is not a date")))?;
            let end = NaiveDate::from_ymd_opt(y, m2, b)
                .ok_or_else(|| FomcError(format!("{y}-{m2}-{b} is not a date")))?;
            if end < start {
                return Err(FomcError(format!("meeting {start} ends before it starts")));
            }
            out.push((start, end));
        }
    }
    if out.is_empty() {
        return Err(FomcError(
            "no FOMC meetings found: the page has changed shape".into(),
        ));
    }
    Ok(out)
}

/// Calendar entries (`source_id` is `cb-fed`) for meetings that end on or after `from`.
pub fn entries(html: &str, from: NaiveDate) -> Result<Vec<CalendarEntry>, FomcError> {
    Ok(meetings(html)?
        .into_iter()
        .filter(|(_, end)| *end >= from)
        .filter_map(|(start, end)| {
            Some(CalendarEntry {
                source_id: "cb-fed".into(),
                event_key: eventkey::policy("fed", end)?,
                title: format!("FOMC meeting, {start} to {end}"),
                expected_day: end,
                expected_at: None,
                window_days: 0,
                precision: "day".into(),
                origin: "html".into(),
            })
        })
        .collect())
}
