//! Full-jitter exponential backoff (§5.4): `sleep = random(0, min(cap, base · 2^attempt))`,
//! base 30 s, cap 1 h.

use std::time::Duration;

pub const BASE: Duration = Duration::from_secs(30);
pub const CAP: Duration = Duration::from_secs(3600);

/// The upper bound of the jitter window after `attempt` attempts.
pub fn ceiling(attempt: i16) -> Duration {
    let exp = u32::try_from(attempt.max(0)).unwrap_or(0).min(20);
    BASE.saturating_mul(1u32 << exp).min(CAP)
}

/// A backoff for `attempt`, drawn uniformly from `[0, ceiling(attempt)]` with `rand01` in `[0, 1)`.
pub fn full_jitter_with(attempt: i16, rand01: f64) -> Duration {
    ceiling(attempt).mul_f64(rand01.clamp(0.0, 1.0))
}

pub fn full_jitter(attempt: i16) -> Duration {
    full_jitter_with(attempt, fastrand::f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_doubles_then_caps_at_an_hour() {
        assert_eq!(ceiling(0), Duration::from_secs(30));
        assert_eq!(ceiling(1), Duration::from_secs(60));
        assert_eq!(ceiling(5), Duration::from_secs(960));
        assert_eq!(ceiling(7), CAP);
        assert_eq!(ceiling(i16::MAX), CAP);
        assert_eq!(ceiling(-3), BASE);
    }

    #[test]
    fn jitter_stays_inside_the_window() {
        for attempt in 0..12 {
            for _ in 0..200 {
                assert!(full_jitter(attempt) <= ceiling(attempt));
            }
            assert_eq!(full_jitter_with(attempt, 0.0), Duration::ZERO);
            assert_eq!(full_jitter_with(attempt, 1.0), ceiling(attempt));
        }
    }
}
