//! Shared test utilities

use std::path::PathBuf;

/// Get path to a test fixture file
#[cfg(test)]
pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}
