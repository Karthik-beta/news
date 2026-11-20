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

/// A secret Karthik enters in Board and the service reads from Windows Credential Manager when a step
/// needs it (ADR 0007 decision 11). Nothing secret crosses the Hotline or is stored in PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Secret {
    /// The Credential Manager target is `news/<name>`.
    pub name: &'static str,
    pub label: &'static str,
    /// Who reads it (Technical Design, "Configuration and security").
    pub used_by: &'static str,
}

/// The secrets Board offers.
pub const SECRETS: &[Secret] = &[
    Secret {
        name: "write-api-key",
        label: "Write API key",
        used_by: "Press, inside an authorised publish step",
    },
    Secret {
        name: "cloudflare-analytics-token",
        label: "Cloudflare analytics token (read-only)",
        used_by: "Ticker's analytics pull",
    },
    Secret {
        name: "aws-access-key-id",
        label: "AWS access key id (read-only)",
        used_by: "Board's cloud views",
    },
    Secret {
        name: "aws-secret-access-key",
        label: "AWS secret access key (read-only)",
        used_by: "Board's cloud views",
    },
    Secret {
        name: "github-token",
        label: "GitHub fine-grained token",
        used_by: "Board's development views",
    },
];

/// The Credential Manager target for a secret name.
pub fn target(name: &str) -> String {
    format!("news/{name}")
}

/// Reads a Credential Manager entry by target; the `news` binary installs `news_win::cred_read` at
/// start. Without one (tests, other platforms) only the environment is used.
pub type CredentialReader = fn(&str) -> Option<String>;

static READER: std::sync::OnceLock<CredentialReader> = std::sync::OnceLock::new();

pub fn set_credential_reader(f: CredentialReader) {
    let _ = READER.set(f);
}

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
    // Credential Manager first (where Board puts it); the variable stays for tests and local scripts.
    read_with(|name| {
        READER
            .get()
            .and_then(|read| read(&target("write-api-key")))
            .or_else(|| std::env::var(name).ok())
    })
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

    #[test]
    fn the_registry_has_unique_names_and_targets() {
        let mut names: Vec<_> = SECRETS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SECRETS.len());
        assert_eq!(target("write-api-key"), "news/write-api-key");
    }

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
