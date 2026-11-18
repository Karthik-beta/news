//! The write-API key guard (Bureau Design §9.3): only `news publish` and `news home publish` (Press)
//! read `NEWS_WRITE_API_KEY`, and the service's background pools never do.
//!
//! [`write_api_key`] is the only place in the workspace that names the variable. Every call is counted
//! ([`write_api_key_reads`]), so the sentinel test that runs the background pools can assert the count
//! stayed at zero. A process that runs no publishing step can call [`forbid_write_api_key`], after which
//! every read fails with `gated`.

use crate::error::NewsError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const WRITE_API_KEY_VAR: &str = "NEWS_WRITE_API_KEY";

static FORBIDDEN: AtomicBool = AtomicBool::new(false);
static READS: AtomicUsize = AtomicUsize::new(0);

/// From now on, this process may not read the write-API key. Cannot be undone.
pub fn forbid_write_api_key() {
    FORBIDDEN.store(true, Ordering::SeqCst);
}

pub fn write_api_key_forbidden() -> bool {
    FORBIDDEN.load(Ordering::SeqCst)
}

/// How many times this process asked for the key (allowed or not).
pub fn write_api_key_reads() -> usize {
    READS.load(Ordering::SeqCst)
}

/// The write-API key. Only Press's G-tier commands may call this.
pub fn write_api_key() -> Result<String, NewsError> {
    read_with(|name| std::env::var(name).ok())
}

fn read_with(lookup: impl Fn(&str) -> Option<String>) -> Result<String, NewsError> {
    READS.fetch_add(1, Ordering::SeqCst);
    if write_api_key_forbidden() {
        return Err(NewsError::gated(
            "this process may not read the write-API key (it runs no publishing step)",
        ));
    }
    lookup(WRITE_API_KEY_VAR)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| NewsError::precondition(format!("{WRITE_API_KEY_VAR} is not set")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // One test owns the process-wide flag, so the order of its steps is fixed.
    #[test]
    fn reads_count_and_the_guard_blocks_after_forbid() {
        let sentinel = "sentinel-write-key";
        let before = write_api_key_reads();
        assert_eq!(read_with(|_| Some(sentinel.into())).unwrap(), sentinel);
        assert_eq!(
            read_with(|_| None).unwrap_err().code,
            crate::ErrorCode::Precondition
        );
        assert_eq!(write_api_key_reads(), before + 2);

        forbid_write_api_key();
        let err = read_with(|_| Some(sentinel.into())).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::Gated);
        assert!(!err.message.contains(sentinel));
        assert_eq!(write_api_key_reads(), before + 3);
    }
}
