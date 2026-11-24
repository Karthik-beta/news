//! Shared core of the `news` CLI (Bureau Design §6.2): the JSON envelope, stable error codes and their
//! exit codes, output shaping (`--limit`, `--fields`, `--cursor`), the job contract every subsystem
//! handler implements, permission tiers, the write-API key guard and the brief type ([`brief`]).
//!
//! Nothing here touches the database or the network, and only `clock::SystemClock` reads the time, so
//! all of it is unit-tested.

pub mod brief;
pub mod clock;
pub mod envelope;
pub mod error;
pub mod job;
pub mod refs;
pub mod secrets;
pub mod shape;
pub mod tier;

pub use envelope::Envelope;
pub use error::{ErrorCode, NewsError};
pub use shape::Shape;

/// Version of the `news` toolchain, recorded on every page as the renderer/CLI version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result type of every command handler.
pub type NewsResult<T> = Result<T, NewsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn version_is_plain_major_minor_patch() {
        // Pages record this string as the renderer/CLI version, so it must stay a bare x.y.z.
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "expected MAJOR.MINOR.PATCH, got {VERSION:?}"
        );
        for part in parts {
            assert!(
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()),
                "non-numeric component {part:?} in {VERSION:?}"
            );
        }
    }

    #[test]
    fn version_matches_the_crate_manifest() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
