//! Time as a dependency (ADR 0008 §2): Chief, Rota and the Lineup read `now` from a [`Clock`], so tests
//! run them on a [`ManualClock`] instead of waiting.

use chrono::{DateTime, Duration, Utc};
use std::sync::{Arc, Mutex};

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
}

/// The wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that moves only when told to. Clones share the same time.
#[derive(Debug, Clone)]
pub struct ManualClock(Arc<Mutex<DateTime<Utc>>>);

impl ManualClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(at)))
    }

    pub fn set(&self, at: DateTime<Utc>) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = at;
    }

    pub fn advance(&self, by: Duration) {
        let mut t = self.0.lock().unwrap_or_else(|p| p.into_inner());
        *t += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_told_and_clones_share_it() {
        let t0 = DateTime::<Utc>::UNIX_EPOCH;
        let c = ManualClock::new(t0);
        let d = c.clone();
        c.advance(Duration::minutes(5));
        assert_eq!(d.now(), t0 + Duration::minutes(5));
        d.set(t0);
        assert_eq!(c.now(), t0);
    }
}
