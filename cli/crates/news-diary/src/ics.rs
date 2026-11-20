//! A small iCalendar reader (RFC 5545): the `VEVENT`s of a file, with line unfolding, text escapes
//! and the three `DTSTART` forms the release calendars use.
//!
//! Times: a UTC stamp (`…Z`) is exact. A local time with `TZID=US-Eastern` or `America/New_York` is
//! turned into UTC with the US daylight-saving rule (second Sunday of March to first Sunday of
//! November, 02:00 local). Any other zone is kept as a day only, never guessed. `expected_day` is
//! the day the source's own clock shows.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Utc, Weekday};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsEvent {
    pub uid: String,
    pub summary: String,
    pub start: Start,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// `DTSTART;VALUE=DATE:20250103`
    Date(NaiveDate),
    /// `DTSTART:20261125T133000Z`
    Utc(DateTime<Utc>),
    /// `DTSTART;TZID=US-Eastern:20250103T100000`
    Local { at: NaiveDateTime, tzid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct IcsError {
    pub line: usize,
    pub message: String,
}

impl Start {
    /// The day the source shows, and the exact instant when it can be known.
    pub fn resolve(&self) -> (NaiveDate, Option<DateTime<Utc>>) {
        match self {
            Start::Date(d) => (*d, None),
            Start::Utc(t) => (t.date_naive(), Some(*t)),
            Start::Local { at, tzid } => match tzid.as_str() {
                "US-Eastern" | "America/New_York" => {
                    let off = if us_dst(*at) { 4 } else { 5 };
                    (at.date(), Some((*at + Duration::hours(off)).and_utc()))
                }
                _ => (at.date(), None),
            },
        }
    }
}

/// The `n`th (1-based) given weekday of a month.
fn nth_weekday(year: i32, month: u32, wd: Weekday, n: u32) -> Option<NaiveDate> {
    NaiveDate::from_weekday_of_month_opt(year, month, wd, u8::try_from(n).ok()?)
}

/// US daylight time at a local wall-clock time.
fn us_dst(t: NaiveDateTime) -> bool {
    let y = t.year();
    let (Some(start), Some(end)) = (
        nth_weekday(y, 3, Weekday::Sun, 2),
        nth_weekday(y, 11, Weekday::Sun, 1),
    ) else {
        return false;
    };
    let two = chrono::NaiveTime::from_hms_opt(2, 0, 0).unwrap_or_default();
    t >= start.and_time(two) && t < end.and_time(two)
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_start(params: &str, value: &str, line: usize) -> Result<Start, IcsError> {
    let err = |m: &str| IcsError {
        line,
        message: format!("{m}: {value:?}"),
    };
    let tzid = params
        .split(';')
        .find_map(|p| p.strip_prefix("TZID="))
        .map(str::to_owned);
    if params.split(';').any(|p| p == "VALUE=DATE") || (value.len() == 8 && !value.contains('T')) {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .map(Start::Date)
            .map_err(|_| err("not a date"));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        return NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S")
            .map(|t| Start::Utc(t.and_utc()))
            .map_err(|_| err("not a UTC date-time"));
    }
    let at = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S")
        .map_err(|_| err("not a date-time"))?;
    Ok(match tzid {
        Some(tzid) => Start::Local { at, tzid },
        // A floating time has no zone: the day is known, the instant is not.
        None => Start::Date(at.date()),
    })
}

/// Every `VEVENT` with a `SUMMARY` and a `DTSTART`.
pub fn parse(bytes: &[u8]) -> Result<Vec<IcsEvent>, IcsError> {
    let text = std::str::from_utf8(bytes).map_err(|e| IcsError {
        line: 0,
        message: e.to_string(),
    })?;
    // Unfold: a line starting with a space or tab continues the previous one.
    let mut lines: Vec<(usize, String)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        match raw.strip_prefix([' ', '\t']) {
            Some(cont) if !lines.is_empty() => {
                if let Some(last) = lines.last_mut() {
                    last.1.push_str(cont);
                }
            }
            _ => lines.push((i + 1, raw.trim_end().to_owned())),
        }
    }
    if !lines.iter().any(|(_, l)| l == "BEGIN:VCALENDAR") {
        return Err(IcsError {
            line: 1,
            message: "not an iCalendar file (no BEGIN:VCALENDAR)".into(),
        });
    }
    let mut out = Vec::new();
    let mut cur: Option<(String, Option<String>, Option<Start>)> = None;
    for (n, l) in lines {
        match l.as_str() {
            "BEGIN:VEVENT" => cur = Some((String::new(), None, None)),
            "END:VEVENT" => {
                if let Some((uid, Some(summary), Some(start))) = cur.take() {
                    out.push(IcsEvent {
                        uid,
                        summary,
                        start,
                    });
                }
            }
            _ => {
                let Some(ev) = cur.as_mut() else { continue };
                let Some((head, value)) = l.split_once(':') else {
                    continue;
                };
                let (name, params) = head.split_once(';').unwrap_or((head, ""));
                match name {
                    "UID" => ev.0 = value.to_owned(),
                    "SUMMARY" => {
                        ev.1 = Some(
                            unescape(value)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" "),
                        )
                    }
                    "DTSTART" => ev.2 = Some(parse_start(params, value, n)?),
                    _ => {}
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn eastern_time_follows_the_us_dst_rule() {
        let est = Start::Local {
            at: NaiveDate::from_ymd_opt(2026, 12, 11)
                .unwrap()
                .and_hms_opt(8, 30, 0)
                .unwrap(),
            tzid: "US-Eastern".into(),
        };
        assert_eq!(
            est.resolve().1.unwrap().to_rfc3339(),
            "2026-12-11T13:30:00+00:00"
        );
        let edt = Start::Local {
            at: NaiveDate::from_ymd_opt(2026, 7, 14)
                .unwrap()
                .and_hms_opt(8, 30, 0)
                .unwrap(),
            tzid: "America/New_York".into(),
        };
        assert_eq!(
            edt.resolve().1.unwrap().to_rfc3339(),
            "2026-07-14T12:30:00+00:00"
        );
        // The switch days: 8 March 2026 (02:00) and 1 November 2026 (02:00).
        let d = |m, d, h| {
            NaiveDate::from_ymd_opt(2026, m, d)
                .unwrap()
                .and_hms_opt(h, 0, 0)
                .unwrap()
        };
        assert!(!us_dst(d(3, 8, 1)) && us_dst(d(3, 8, 3)));
        assert!(us_dst(d(11, 1, 1)) && !us_dst(d(11, 1, 3)));
    }

    #[test]
    fn an_unknown_zone_keeps_only_the_day() {
        let s = Start::Local {
            at: NaiveDate::from_ymd_opt(2026, 7, 14)
                .unwrap()
                .and_hms_opt(8, 30, 0)
                .unwrap(),
            tzid: "Mars/Olympus".into(),
        };
        let (day, at) = s.resolve();
        assert_eq!(day.to_string(), "2026-07-14");
        assert!(at.is_none());
    }

    #[test]
    fn folding_escapes_and_the_three_forms() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART;VALUE=DATE:20261201\r\nSUMMARY:One\\, two\r\n  three\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE-TIME:20261125T133000Z\r\nSUMMARY:B\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let ev = parse(ics.as_bytes()).unwrap();
        assert_eq!(ev[0].summary, "One, two three");
        assert_eq!(
            ev[0].start,
            Start::Date(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap())
        );
        assert_eq!(
            ev[1].start.resolve().1.unwrap().to_rfc3339(),
            "2026-11-25T13:30:00+00:00"
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse(b"hello").is_err());
        let bad =
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:2026\nSUMMARY:x\nEND:VEVENT\nEND:VCALENDAR\n";
        assert_eq!(parse(bad.as_bytes()).unwrap_err().line, 3);
    }
}
