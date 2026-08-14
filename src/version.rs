//! Build identity: crate version plus the git SHA embedded by `build.rs`.

/// Display wordmark: the pipe pun. The binary, crate, and config keep
/// the plain `spacecat` name.
pub const WORDMARK: &str = "space | cat";

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git SHA the binary was built from (`unknown` when built without git
/// metadata, e.g. from a source tarball without `SPACECAT_GIT_SHA` set).
pub const GIT_SHA: &str = env!("SPACECAT_GIT_SHA");

/// `<version> (<sha>)`, e.g. `0.3.0 (1a2b3c4d5e6f)`.
pub const VERSION_STRING: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("SPACECAT_GIT_SHA"),
    ")"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_populated() {
        assert!(!VERSION.is_empty());
        assert!(!GIT_SHA.is_empty());
        assert!(VERSION_STRING.starts_with(VERSION));
        assert!(VERSION_STRING.contains(GIT_SHA));
    }
}
