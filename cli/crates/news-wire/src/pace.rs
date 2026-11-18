//! Pacing: the clock the Runner sleeps on, per-host token buckets and `Retry-After`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Time as the Runner sees it. Tests use [`VirtualClock`], so pacing is checked without waiting.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    fn sleep(&self, d: Duration) -> BoxFut<'_, ()>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
    fn sleep(&self, d: Duration) -> BoxFut<'_, ()> {
        Box::pin(tokio::time::sleep(d))
    }
}

/// A clock that only moves when slept on; records every sleep.
pub struct VirtualClock {
    now: Mutex<DateTime<Utc>>,
    pub sleeps: Mutex<Vec<Duration>>,
}

impl VirtualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(start),
            sleeps: Mutex::new(Vec::new()),
        }
    }

    pub fn advance(&self, d: Duration) {
        if let Ok(mut n) = self.now.lock() {
            *n += chrono::Duration::from_std(d).unwrap_or_default();
        }
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> DateTime<Utc> {
        self.now.lock().map(|n| *n).unwrap_or_default()
    }
    fn sleep(&self, d: Duration) -> BoxFut<'_, ()> {
        if let Ok(mut s) = self.sleeps.lock() {
            s.push(d);
        }
        self.advance(d);
        Box::pin(async {})
    }
}

/// A token bucket of capacity 1 refilled at `rate` tokens per second: requests are spaced `1/rate`
/// apart. After a 429/503 the rate is halved for an hour (Bureau §3.2, "Failure modes").
#[derive(Debug, Clone)]
pub struct TokenBucket {
    rate: f64,
    tokens: f64,
    last: DateTime<Utc>,
    penalty_until: Option<DateTime<Utc>>,
}

impl TokenBucket {
    pub fn new(rate_per_sec: f64, now: DateTime<Utc>) -> Self {
        Self {
            rate: rate_per_sec.max(0.001),
            tokens: 1.0,
            last: now,
            penalty_until: None,
        }
    }

    fn effective_rate(&self, now: DateTime<Utc>) -> f64 {
        match self.penalty_until {
            Some(t) if now < t => self.rate / 2.0,
            _ => self.rate,
        }
    }

    /// Take a token now (`Duration::ZERO`) or say how long to wait before trying again.
    pub fn reserve(&mut self, now: DateTime<Utc>) -> Duration {
        let rate = self.effective_rate(now);
        let elapsed = (now - self.last).num_microseconds().unwrap_or(0).max(0) as f64 / 1e6;
        self.tokens = (self.tokens + elapsed * rate).min(1.0);
        self.last = now;
        if self.tokens >= 1.0 - 1e-9 {
            self.tokens -= 1.0;
            Duration::ZERO
        } else {
            Duration::from_secs_f64((1.0 - self.tokens) / rate)
        }
    }

    /// Halve the rate for one hour.
    pub fn penalise(&mut self, now: DateTime<Utc>) {
        self.penalty_until = Some(now + chrono::Duration::hours(1));
    }
}

/// `Retry-After` as delay-seconds or an HTTP-date (RFC 9110 §10.2.3), relative to `now`. A date in
/// the past means "now"; anything unparseable is `None`.
pub fn retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let v = value.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at: DateTime<Utc> = httpdate::parse_http_date(v).ok()?.into();
    Some((at - now).to_std().unwrap_or(Duration::ZERO))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap()
    }

    #[test]
    fn the_bucket_paces_on_a_virtual_clock() {
        let clock = VirtualClock::new(t0());
        let mut b = TokenBucket::new(4.0, clock.now());
        // 9 requests at 4/s: the first goes at once, the next 8 are 250 ms apart.
        for _ in 0..9 {
            loop {
                let wait = b.reserve(clock.now());
                if wait.is_zero() {
                    break;
                }
                drop(clock.sleep(wait));
            }
        }
        let elapsed = (clock.now() - t0()).num_milliseconds();
        assert!((1995..=2005).contains(&elapsed), "{elapsed} ms");
        // Idle time refills at most one token: no burst after a pause.
        clock.advance(Duration::from_secs(60));
        assert!(b.reserve(clock.now()).is_zero());
        assert!(!b.reserve(clock.now()).is_zero());
    }

    #[test]
    fn a_penalty_halves_the_rate_for_an_hour() {
        let mut b = TokenBucket::new(2.0, t0());
        assert!(b.reserve(t0()).is_zero());
        b.penalise(t0());
        let wait = b.reserve(t0());
        assert_eq!(wait.as_millis(), 1000);
        let later = t0() + chrono::Duration::hours(2);
        assert!(b.reserve(later).is_zero());
        assert_eq!(b.reserve(later).as_millis(), 500);
    }

    #[test]
    fn retry_after_in_both_forms() {
        assert_eq!(retry_after("120", t0()), Some(Duration::from_secs(120)));
        assert_eq!(
            retry_after("Mon, 16 Nov 2026 08:05:00 GMT", t0()),
            Some(Duration::from_secs(300))
        );
        assert_eq!(
            retry_after("Mon, 16 Nov 2026 07:00:00 GMT", t0()),
            Some(Duration::ZERO)
        );
        assert_eq!(retry_after("soon", t0()), None);
    }
}
