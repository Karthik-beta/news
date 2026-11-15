//! Shared types for the News pipeline. The data model is designed in Phase 5
//! (see the Technical Design, "Data model"); this crate only holds the version for now.

/// Version of the `news` toolchain, recorded on every page as the renderer/CLI version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
