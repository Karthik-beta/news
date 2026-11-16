//! Small helpers shared by the Stringers.

use chrono::{DateTime, NaiveDate, Utc};

use crate::{ParseError, ParseErrorKind, Snapshot};

/// First and last day of a calendar month. Date arithmetic for the period a source names, not a
/// conversion: the source's own label stays in `period_label`.
pub fn month_bounds(year: i32, month: u32) -> Option<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    Some((start, next.pred_opt()?))
}

/// Months `first..=last` of a year, as one period.
pub fn months_bounds(year: i32, first: u32, last: u32) -> Option<(NaiveDate, NaiveDate)> {
    Some((month_bounds(year, first)?.0, month_bounds(year, last)?.1))
}

pub fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Midnight UTC of a date: the vintage for sources that publish only a date (SEC `filed`, FRED
/// `realtime_start`). Recorded as such in ADR 0018.
pub fn day_vintage(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

pub fn shape(locator: impl Into<String>, e: impl std::fmt::Display) -> ParseError {
    ParseError::new(ParseErrorKind::Shape, locator, e.to_string())
}

pub fn value_err(locator: impl Into<String>, e: impl std::fmt::Display) -> ParseError {
    ParseError::new(ParseErrorKind::Value, locator, e.to_string())
}

/// The one snapshot whose URL contains `needle`.
pub fn find<'a>(snaps: &'a [Snapshot], needle: &str) -> Result<&'a Snapshot, ParseError> {
    snaps.iter().find(|s| s.url.contains(needle)).ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::MissingSnapshot,
            needle,
            "no snapshot for this request",
        )
    })
}

/// `"a b"` with `%20`-style escaping for the few characters our query values can hold.
pub fn q(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_handle_december_and_leap_years() {
        let (s, e) = month_bounds(2024, 2).unwrap();
        assert_eq!((s.to_string(), e.to_string()), ("2024-02-01".into(), "2024-02-29".into()));
        let (_, e) = month_bounds(2026, 12).unwrap();
        assert_eq!(e.to_string(), "2026-12-31");
        assert!(month_bounds(2026, 13).is_none());
    }
}
