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
}
