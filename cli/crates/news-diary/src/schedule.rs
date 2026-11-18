//! The pre-warm and capture schedule (Bureau §3.3, "Algorithm"), as pure functions of the event and a
//! clock value, so it can be tested on a virtual clock.
//!
//! * **Pre-warm** runs at the start of `expected_day − 1` (UTC): fetch history, resolve entities,
//!   build the static parts of the brief.
//! * **Capture** starts at `expected_at + 2 minutes` (or at the start of the expected day when only
//!   the day is known; a `window` event starts [`window_days`](news_connectors::CalendarEntry) days
//!   earlier). It retries every 5 minutes for 2 hours, then hourly for a day, then the event is
//!   marked `late`.

use chrono::{DateTime, Duration, NaiveDate, Utc};

pub const CAPTURE_DELAY_MIN: i64 = 2;
pub const FAST_RETRY_MIN: i64 = 5;
pub const FAST_RETRY_FOR_H: i64 = 2;
pub const SLOW_RETRY_FOR_H: i64 = 24;

/// What the schedule needs to know about an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub expected_day: NaiveDate,
    pub expected_at: Option<DateTime<Utc>>,
    pub window_days: u16,
}

fn day_start(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc()
}

/// When the pre-warm job runs.
pub fn prewarm_at(t: &Timing) -> DateTime<Utc> {
    day_start(t.expected_day - Duration::days(1 + i64::from(t.window_days)))
}

/// When the first capture attempt runs.
pub fn capture_start(t: &Timing) -> DateTime<Utc> {
    match (t.expected_at, t.window_days) {
        (Some(at), 0) => at + Duration::minutes(CAPTURE_DELAY_MIN),
        (_, w) => day_start(t.expected_day - Duration::days(i64::from(w))),
    }
}

/// When the capture of an event is given up and the event marked `late`: the last retry plus the
/// hourly day, counted from the end of the window for a window event.
pub fn late_after(t: &Timing) -> DateTime<Utc> {
    let start = if t.window_days == 0 {
        capture_start(t)
    } else {
        day_start(t.expected_day + Duration::days(i64::from(t.window_days) + 1))
    };
    start + Duration::hours(FAST_RETRY_FOR_H + SLOW_RETRY_FOR_H)
}

/// The next step of the capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Run the next attempt at this time.
    At(DateTime<Utc>),
    /// Every attempt is spent: mark the event `late`.
    Late,
}

/// The time of attempt number `attempt` (0-based), or `Late` once the schedule is exhausted.
/// Attempts 0..=24 are 5 minutes apart (the first 2 hours, both ends included); then one every hour
/// until 26 hours after the start.
pub fn attempt(start: DateTime<Utc>, attempt: u32) -> Next {
    let fast = u32::try_from(FAST_RETRY_FOR_H * 60 / FAST_RETRY_MIN).unwrap_or(24);
    let slow = u32::try_from(SLOW_RETRY_FOR_H).unwrap_or(24);
    if attempt <= fast {
        Next::At(start + Duration::minutes(FAST_RETRY_MIN * i64::from(attempt)))
    } else if attempt <= fast + slow {
        Next::At(start + Duration::hours(FAST_RETRY_FOR_H + i64::from(attempt - fast)))
    } else {
        Next::Late
    }
}

/// Every attempt time of the schedule, in order (49 of them), then the `Late` marking time.
pub fn timeline(start: DateTime<Utc>) -> (Vec<DateTime<Utc>>, DateTime<Utc>) {
    let mut at = Vec::new();
    let mut n = 0;
    while let Next::At(t) = attempt(start, n) {
        at.push(t);
        n += 1;
    }
    let late = at.last().copied().unwrap_or(start);
    (at, late)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(day: u32, h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 12, day, h, m, 0).unwrap()
    }

    fn exact() -> Timing {
        Timing {
            expected_day: NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            expected_at: Some(t(10, 13, 30)),
            window_days: 0,
        }
    }

    #[test]
    fn prewarm_is_the_day_before_and_capture_two_minutes_after_release() {
        assert_eq!(prewarm_at(&exact()), t(9, 0, 0));
        assert_eq!(capture_start(&exact()), t(10, 13, 32));
    }

    #[test]
    fn capture_retries_every_five_minutes_for_two_hours_then_hourly_then_late() {
        let start = capture_start(&exact());
        let (at, _) = timeline(start);
        assert_eq!(at[0], start);
        assert_eq!(at[1], start + Duration::minutes(5));
        assert_eq!(
            at[24],
            start + Duration::hours(2),
            "25th attempt: exactly 2 h in"
        );
        assert_eq!(at[25], start + Duration::hours(3), "then hourly");
        assert_eq!(*at.last().unwrap(), start + Duration::hours(26));
        assert_eq!(at.len(), 49);
        // Gaps: 24 five-minute gaps, 24 one-hour gaps (the 2 h mark is the last fast attempt, and
        // the hourly phase continues from it).
        let gaps: Vec<i64> = at.windows(2).map(|w| (w[1] - w[0]).num_minutes()).collect();
        assert_eq!(gaps.iter().filter(|g| **g == 5).count(), 24);
        assert_eq!(gaps.iter().filter(|g| **g == 60).count(), 24);
        assert_eq!(attempt(start, 49), Next::Late);
        assert_eq!(late_after(&exact()), start + Duration::hours(26));
    }

    #[test]
    fn a_day_only_event_starts_at_midnight_and_a_window_event_at_the_window_start() {
        let day = Timing {
            expected_day: NaiveDate::from_ymd_opt(2026, 12, 9).unwrap(),
            expected_at: None,
            window_days: 0,
        };
        assert_eq!(capture_start(&day), t(9, 0, 0));
        assert_eq!(prewarm_at(&day), t(8, 0, 0));
        let win = Timing {
            expected_day: NaiveDate::from_ymd_opt(2027, 1, 20).unwrap(),
            expected_at: None,
            window_days: 7,
        };
        assert_eq!(
            capture_start(&win),
            Utc.with_ymd_and_hms(2027, 1, 13, 0, 0, 0).unwrap()
        );
        assert_eq!(
            prewarm_at(&win),
            Utc.with_ymd_and_hms(2027, 1, 12, 0, 0, 0).unwrap()
        );
        // Late only after the window has closed, plus the capture tail.
        assert_eq!(
            late_after(&win),
            Utc.with_ymd_and_hms(2027, 1, 29, 2, 0, 0).unwrap()
        );
    }
}
