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

/// Lower-case hex of a SHA-256, for cursors.
pub fn sha_hex(sha: &[u8; 32]) -> String {
    use std::fmt::Write;
    sha.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The leading `YYYY-MM-DD` of an ISO timestamp.
pub fn date_prefix(s: &str) -> Option<NaiveDate> {
    s.get(..10).and_then(date)
}

/// A period label as a calendar period: `2026-10`, `2026-Q3`, `2026` or `2026-10-16` to its first and
/// last day. Date arithmetic for the period the source names; the source's label stays verbatim.
pub fn period_from_label(label: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (y, rest) = label
        .split_once('-')
        .map_or((label, None), |(y, r)| (y, Some(r)));
    let year: i32 = y.parse().ok().filter(|_| y.len() == 4)?;
    match rest {
        None => months_bounds(year, 1, 12),
        Some(r) if r.len() == 2 && r.starts_with('Q') => {
            let q: u32 = r[1..].parse().ok().filter(|q| (1..=4).contains(q))?;
            months_bounds(year, (q - 1) * 3 + 1, q * 3)
        }
        Some(r) if r.len() == 2 => month_bounds(year, r.parse().ok()?),
        Some(_) => {
            let d = date(label)?;
            Some((d, d))
        }
    }
}

pub fn shape(locator: impl Into<String>, e: impl std::fmt::Display) -> ParseError {
    ParseError::new(ParseErrorKind::Shape, locator, e.to_string())
}

pub fn value_err(locator: impl Into<String>, e: impl std::fmt::Display) -> ParseError {
    ParseError::new(ParseErrorKind::Value, locator, e.to_string())
}

/// The one snapshot whose URL contains `needle`.
pub fn find<'a>(snaps: &'a [Snapshot], needle: &str) -> Result<&'a Snapshot, ParseError> {
    snaps
        .iter()
        .find(|s| s.url.contains(needle))
        .ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::MissingSnapshot,
                needle,
                "no snapshot for this request",
            )
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_handle_december_and_leap_years() {
        let (s, e) = month_bounds(2024, 2).unwrap();
        assert_eq!(
            (s.to_string(), e.to_string()),
            ("2024-02-01".into(), "2024-02-29".into())
        );
        let (_, e) = month_bounds(2026, 12).unwrap();
        assert_eq!(e.to_string(), "2026-12-31");
        assert!(month_bounds(2026, 13).is_none());
    }
}
