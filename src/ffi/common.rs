//! Common utilities shared across FFI wrappers

use std::path::PathBuf;

/// RAII guard for temporary directories
/// Ensures cleanup on drop, even if an error occurs
pub(crate) struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Normalize archive entry path by converting backslashes to forward slashes
#[inline]
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
