//! Common test utilities and helpers
//!
//! Provides shared infrastructure for integration and unit tests. The
//! `default_compression_options` and `default_extraction_options` helpers
//! exist so individual tests do not have to hand-roll the full
//! `CompressionOptions`/`ExtractionOptions` literal every time — override
//! only the fields that differ from the library's own defaults.

pub mod config;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use unified_archive::{ArchiveFormat, CompressionOptions, ExtractionOptions};

/// Create unique temporary directory for testing
///
/// Uses configured temp directory (see .unified-archive.toml for development).
/// Falls back to system temp dir if configured path is not available. The
/// path mixes pid + nanos + a process-wide atomic counter so two tests
/// scheduled within the same nanosecond cannot collide on the directory
/// name (the prior pid+nanos scheme flaked under `cargo test`'s parallel
/// scheduler).
pub fn temp_test_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("test_{}_{}_{}", std::process::id(), timestamp, counter);

    // Use test config to get temp directory
    let base = config::temp_dir().join("7zip");
    let path = if base.exists() || fs::create_dir_all(&base).is_ok() {
        base.join(&suffix)
    } else {
        // Fallback to system temp
        std::env::temp_dir().join(format!("unified-archive-{}", suffix))
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

/// Default `CompressionOptions` for tests — Normal level, no encryption, no
/// splitting, no progress callback. Equivalent to `CompressionOptions::new`.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn default_compression_options(format: ArchiveFormat) -> CompressionOptions {
    CompressionOptions::new(format)
}

/// Default `ExtractionOptions` for tests — library defaults with the given
/// destination. Override additional fields with the struct-update syntax:
/// `ExtractionOptions { overwrite: true, ..default_extraction_options(dest) }`.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn default_extraction_options(dest: impl Into<PathBuf>) -> ExtractionOptions {
    ExtractionOptions {
        destination: dest.into(),
        ..Default::default()
    }
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

/// Check whether the given executable is available on PATH. Tests use this
/// to skip gracefully when a fixture-building CLI (tar, zip, 7z…) is
/// missing from the environment.
///
/// R0001-0086: this used to launch `which`, which Windows does not
/// normally provide — so every tool-backed lane reported a graceful skip
/// there even when the tool was installed and usable. Resolve `PATH` in
/// Rust instead (honouring `PATHEXT` on Windows) so discovery behaves the
/// same on every host and needs no child process at all.
#[allow(dead_code)]
pub fn command_exists(cmd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }

    // A name that already carries a separator (`./tool`, `/usr/bin/tar`)
    // is not a PATH lookup — probe it directly, as the shell would.
    let explicit = Path::new(cmd);
    if explicit.components().count() > 1 {
        return is_executable_file(explicit);
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    let extensions = executable_extensions();
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if is_executable_file(&dir.join(cmd)) {
            return true;
        }
        for ext in &extensions {
            if is_executable_file(&dir.join(format!("{cmd}{ext}"))) {
                return true;
            }
        }
    }

    false
}

/// Extensions Windows appends to a bare command name when resolving it
/// through `PATH` (R0001-0086). Values are normalized to carry a leading
/// dot so callers can concatenate them onto the command name.
#[cfg(windows)]
#[allow(dead_code)]
fn executable_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .map(str::trim)
        .filter(|ext| !ext.is_empty())
        .map(|ext| {
            if ext.starts_with('.') {
                ext.to_string()
            } else {
                format!(".{ext}")
            }
        })
        .collect()
}

/// Unix resolves the command name verbatim — nothing to append.
#[cfg(not(windows))]
#[allow(dead_code)]
fn executable_extensions() -> Vec<String> {
    Vec::new()
}

/// A `PATH` candidate counts only when it is a regular file carrying at
/// least one execute bit (R0001-0086).
#[cfg(unix)]
#[allow(dead_code)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Windows has no execute bit — `PATHEXT` already decides what is
/// runnable, so a regular file at a candidate name is enough.
#[cfg(not(unix))]
#[allow(dead_code)]
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

/// Build a TAR archive at `archive_path` containing `regular.txt` and a
/// `link.txt -> regular.txt` symlink, using the system `tar` CLI. Returns
/// false when the environment cannot produce the fixture (non-Unix host or
/// missing `tar`/`ln`). Tests that require the fixture skip on `false`.
#[allow(dead_code)]
pub fn build_tar_with_symlink(archive_path: &Path, staging: &Path) -> bool {
    if !command_exists("tar") || !command_exists("ln") {
        return false;
    }

    fs::create_dir_all(staging).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello from a regular file\n").expect("write regular");

    let link_path = staging.join("link.txt");
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&link_path);
        std::os::unix::fs::symlink("regular.txt", &link_path).expect("symlink");
    }
    #[cfg(not(unix))]
    {
        let _ = link_path;
        return false;
    }

    let status = Command::new("tar")
        .args([
            "cf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "regular.txt",
            "link.txt",
        ])
        .status();

    status.map(|s| s.success()).unwrap_or(false)
}

/// Build a TAR archive at `archive_path` containing `regular.txt` and a
/// `hardlink.txt` hard-linked to it. Returns false on non-Unix or when
/// `tar`/`ln` are missing from PATH.
#[allow(dead_code)]
pub fn build_tar_with_hardlink(archive_path: &Path, staging: &Path) -> bool {
    if !command_exists("tar") || !command_exists("ln") {
        return false;
    }

    fs::create_dir_all(staging).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello from a regular file\n").expect("write regular");

    let hardlink_path = staging.join("hardlink.txt");
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&hardlink_path);
        fs::hard_link(staging.join("regular.txt"), &hardlink_path).expect("hardlink");
    }
    #[cfg(not(unix))]
    {
        let _ = hardlink_path;
        return false;
    }

    let status = Command::new("tar")
        .args([
            "cf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "regular.txt",
            "hardlink.txt",
        ])
        .status();

    status.map(|s| s.success()).unwrap_or(false)
}

/// Build a single-entry ZIP carrying a 0x5455 ExtendedTimestamp extra
/// field with the supplied modification, access, and creation times.
/// Shared between OI-0065-002 round-trip tests on both the integration
/// suite and the standalone modification-options binary.
///
/// The 0x5455 wire format encodes each timestamp as a signed 32-bit
/// Unix-seconds value, so callers must stay below 2038-01-19T03:14:07Z.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn build_zip_with_extended_timestamp(
    path: &Path,
    mtime: SystemTime,
    atime: SystemTime,
    btime: SystemTime,
) {
    use std::fs::File;
    use std::io::Write as _;
    use zip::CompressionMethod;
    use zip::write::{FullFileOptions, ZipWriter};

    let secs = |t: SystemTime| -> i32 { t.duration_since(UNIX_EPOCH).unwrap().as_secs() as i32 };

    let mut payload = Vec::with_capacity(13);
    // Flags byte: bit0=mtime, bit1=atime, bit2=ctime — set all three.
    payload.push(0b0000_0111);
    payload.extend_from_slice(&secs(mtime).to_le_bytes());
    payload.extend_from_slice(&secs(atime).to_le_bytes());
    payload.extend_from_slice(&secs(btime).to_le_bytes());

    let file = File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    let mut opts = FullFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    opts.add_extra_data(0x5455, payload.into_boxed_slice(), false)
        .unwrap();
    writer.start_file("ts.txt", opts).unwrap();
    writer.write_all(b"hello world").unwrap();
    writer.finish().unwrap();
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

    /// R0001-0086: the PATH walk must reject names that are not on PATH
    /// on every host, without depending on a `which`/`where` binary.
    #[test]
    fn test_command_exists_rejects_unknown_and_empty_names() {
        assert!(!command_exists(""), "empty name is never a command");
        assert!(
            !command_exists("unified-archive-definitely-not-installed-xyzzy"),
            "a name that is not on PATH must not resolve"
        );
    }

    /// R0001-0086: a name carrying a separator is probed directly, and only
    /// a regular file with an execute bit counts. The probes use system
    /// paths with fixed modes on every supported Unix host, so the oracle
    /// does not depend on the scratch filesystem honouring `chmod`.
    #[cfg(unix)]
    #[test]
    fn test_command_exists_honours_explicit_paths() {
        assert!(
            command_exists("/bin/sh"),
            "/bin/sh (0755) must resolve through the explicit-path probe"
        );
        assert!(
            !command_exists("/etc/hosts"),
            "/etc/hosts (0644) is a regular file with no execute bit"
        );
        assert!(
            !command_exists("/etc"),
            "a directory must never resolve as a command"
        );
        assert!(
            !command_exists("/nonexistent/path/to/tool"),
            "a missing explicit path must not resolve"
        );
    }
}
