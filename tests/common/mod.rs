//! Common test utilities and helpers
//!
//! Provides shared infrastructure for integration and unit tests

pub mod config;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Create unique temporary directory for testing
///
/// Uses configured temp directory (see .unified-archive.toml for development).
/// Falls back to system temp dir if configured path is not available.
pub fn temp_test_dir() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Use test config to get temp directory
    let base = config::temp_dir().join("7zip");
    let path = if base.exists() || fs::create_dir_all(&base).is_ok() {
        base.join(format!("test_{}_{}", std::process::id(), timestamp))
    } else {
        // Fallback to system temp
        std::env::temp_dir().join(format!("unified-archive-test-{}", timestamp))
    };

    fs::create_dir_all(&path).expect("Failed to create temp directory");
    path
}

/// Cleanup temporary directory
///
/// Silently ignores errors (directory may already be removed)
pub fn cleanup(path: &PathBuf) {
    fs::remove_dir_all(path).ok();
}

/// Get fixture path for test files
///
/// Returns absolute path to tests/fixtures/{name}
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_unique() {
        let dir1 = temp_test_dir();
        let dir2 = temp_test_dir();

        assert_ne!(dir1, dir2, "Each temp dir should be unique");
        assert!(dir1.exists(), "Temp dir should be created");
        assert!(dir2.exists(), "Temp dir should be created");

        cleanup(&dir1);
        cleanup(&dir2);
    }

    #[test]
    fn test_fixture_path() {
        let path = fixture("test.rar");
        assert!(path.to_string_lossy().contains("tests/fixtures/test.rar"));
    }
}
