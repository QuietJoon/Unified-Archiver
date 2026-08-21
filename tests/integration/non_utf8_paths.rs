//! Tests for AD 0064 (non-UTF-8 path policy — Option A).
//!
//! Validates that backends round-trip raw filesystem path bytes
//! losslessly. Unix-only: Windows path-name testing is deferred to
//! the OI-0065-001 follow-up.

#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use unified_archive::{Archive, CompressionOptions, WritableFormat};

fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

/// Probe whether the temp filesystem accepts non-UTF-8 byte sequences
/// in filenames. Linux ext4/xfs/btrfs do; macOS APFS/HFS+ enforce
/// UTF-8 normalization and reject `\xff\xfe` with `EILSEQ`. Memoized
/// so the probe runs once across the four tests in this module.
fn fs_supports_non_utf8_names() -> bool {
    use std::sync::OnceLock;
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let mut probe_name = std::ffi::OsString::from("");
        probe_name.push(OsStr::from_bytes(b"unified-archive-probe-\xff\xfe.tmp"));
        let probe_path = temp_dir().join(&probe_name);
        let supported = std::fs::File::create(&probe_path).is_ok();
        let _ = std::fs::remove_file(&probe_path);
        supported
    })
}

/// Fail loudly when the temp filesystem rejects the fixtures these tests
/// need.
///
/// OI-0056-010: this used to `eprintln!` and return, so on macOS (APFS/HFS+
/// reject `\xff\xfe` with `EILSEQ`, R0077-0097) all four lanes reported
/// success having created nothing — indistinguishable from a covered run.
/// The host dependency is now declared two ways instead: the lanes are
/// `#[ignore]`d on macOS, where the filesystem genuinely cannot hold the
/// fixture, and on every other host this macro turns a rejecting filesystem
/// into a failure rather than a pass.
///
/// To run them on macOS anyway (they are expected to fail — that is the
/// point, the fixture cannot exist there):
///
/// ```text
/// TMPDIR=/Volumes/Temp/claude cargo test --test integration_tests --all-features \
///     -- --ignored --test-threads=4 integration::non_utf8_paths
/// ```
macro_rules! require_non_utf8_fs {
    () => {
        assert!(
            fs_supports_non_utf8_names(),
            "the temp filesystem at {:?} rejects non-UTF-8 filenames (Illegal byte sequence), so \
             this lane cannot build its fixture. On macOS that is expected and the lane is \
             `#[ignore]`d; anywhere else it is a real environment problem — point TMPDIR at a \
             filesystem that stores raw bytes (ext4/xfs/btrfs/tmpfs).",
            temp_dir()
        );
    };
}

#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS APFS/HFS+ reject non-UTF-8 filenames (EILSEQ), so the fixture cannot exist \
              here; run on Linux, or force it with `TMPDIR=/Volumes/Temp/claude cargo test \
              --test integration_tests --all-features -- --ignored --test-threads=4 \
              integration::non_utf8_paths`"
)]
fn libarchive_path_roundtrips_through_non_utf8_archive_name() {
    require_non_utf8_fs!();
    // The archive's filesystem path itself contains \xff\xfe — invalid
    // UTF-8 on Unix. Before AD 0064 the libarchive wrapper stored the
    // path as `String` via `to_string_lossy()` so a downstream reopen
    // (list_files, extract_to_stream) used the U+FFFD-substituted form
    // and missed the actual inode. After AD 0064 the path is `PathBuf`
    // and CString conversion at the FFI boundary uses
    // `OsStrExt::as_bytes` to preserve raw bytes.

    let mut archive_name = std::ffi::OsString::from("");
    archive_name.push(OsStr::from_bytes(b"weird-\xff\xfe.tar"));
    let archive_path = temp_dir().join(&archive_name);
    cleanup(&archive_path);

    let opts = CompressionOptions::for_writable(WritableFormat::TAR);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    a.add_file_from_data("hello.txt", b"hello").unwrap();
    a.finish().unwrap();

    // Reopen and list — without AD 0064 this would fail because
    // libarchive's CString carried the U+FFFD'd bytes and the kernel
    // returned ENOENT.
    let opened = Archive::open(&archive_path).unwrap();
    let entries = opened.list_files().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "list_files() should return one entry from non-UTF-8-named archive"
    );
    assert_eq!(entries[0].path, "hello.txt");

    // Archive::path() preserves the PathBuf bytes (this layer was
    // already correct, but assert it for paranoia).
    assert_eq!(
        opened.path().as_os_str().as_bytes(),
        archive_path.as_os_str().as_bytes(),
        "Archive::path() must round-trip original PathBuf bytes"
    );

    cleanup(&archive_path);
}

#[cfg(feature = "rar-support")]
#[test]
fn unrar_archive_path_preserves_pathbuf_through_open() {
    // libunrar's open API (`RAROpenArchiveEx`) requires UTF-8 file
    // names internally — true non-UTF-8 archive filenames cannot be
    // opened on Unix without a wide-char path that libunrar's Unix
    // build does not support (tracked as a libunrar wire-format
    // limitation, not unified-archive's). What AD 0064 fixes for
    // UnRAR is the `path: String` -> `PathBuf` field migration so the
    // wrapper no longer pre-truncates a Path passed in via lossy
    // conversion. This test asserts that an UnRAR archive opened
    // via a UTF-8 path round-trips the PathBuf bytes through
    // `Archive::path()` exactly.

    // OI-0056-010: `tests/fixtures/test.rar` is a tracked fixture, so an
    // `if !exists { return }` here made a broken checkout look like a pass.
    let src = PathBuf::from("tests/fixtures/test.rar");
    assert!(
        src.exists(),
        "tracked RAR fixture {} is missing; this lane cannot run without it",
        src.display()
    );

    let opened = Archive::open(&src).unwrap();
    assert_eq!(
        opened.path().as_os_str().as_bytes(),
        src.as_os_str().as_bytes(),
        "Archive::path() must return the same PathBuf bytes the caller passed in"
    );
    let entries = opened.list_files().unwrap();
    assert!(!entries.is_empty(), "list_files() returns entries");
}

#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS APFS/HFS+ reject non-UTF-8 filenames (EILSEQ), so the fixture cannot exist \
              here; run on Linux, or force it with `TMPDIR=/Volumes/Temp/claude cargo test \
              --test integration_tests --all-features -- --ignored --test-threads=4 \
              integration::non_utf8_paths`"
)]
fn add_file_from_path_rejects_non_utf8_filename_loudly() {
    require_non_utf8_fs!();
    // AD 0064 / R0075-0007: silent to_string_lossy at this boundary is
    // rejected so a non-UTF-8 source filename doesn't get truncated to
    // a different archive entry name. Callers route through
    // add_file_from_path_as for explicit naming.

    let dir = temp_dir().join("add_non_utf8_filename");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut weird_filename = std::ffi::OsString::from("");
    weird_filename.push(OsStr::from_bytes(b"file-\xff\xfe.txt"));
    let src_file = dir.join(&weird_filename);
    std::fs::write(&src_file, b"hello").unwrap();

    let archive_path = dir.join("out.zip");
    let opts = unified_archive::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    let result = a.add_file_from_path(&src_file);

    assert!(
        result.is_err(),
        "add_file_from_path with non-UTF-8 filename must be rejected"
    );
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(
        err_str.contains("UTF-8") || err_str.contains("utf-8"),
        "Error must mention UTF-8 constraint, got: {err_str}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS APFS/HFS+ reject non-UTF-8 filenames (EILSEQ), so the fixture cannot exist \
              here; run on Linux, or force it with `TMPDIR=/Volumes/Temp/claude cargo test \
              --test integration_tests --all-features -- --ignored --test-threads=4 \
              integration::non_utf8_paths`"
)]
fn add_file_from_path_as_accepts_non_utf8_source_with_explicit_name() {
    require_non_utf8_fs!();
    // The escape hatch: callers with a non-UTF-8 source can still add
    // it under an explicit UTF-8 archive entry name.

    let dir = temp_dir().join("add_non_utf8_as");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut weird_filename = std::ffi::OsString::from("");
    weird_filename.push(OsStr::from_bytes(b"file-\xff\xfe.txt"));
    let src_file = dir.join(&weird_filename);
    std::fs::write(&src_file, b"hello").unwrap();

    let archive_path = dir.join("out.zip");
    let opts = unified_archive::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    a.add_file_from_path_as(&src_file, "explicit_name.txt")
        .unwrap();
    a.finish().unwrap();

    let opened = Archive::open(&archive_path).unwrap();
    let entries = opened.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "explicit_name.txt");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS APFS/HFS+ reject non-UTF-8 filenames (EILSEQ), so the fixture cannot exist \
              here; run on Linux, or force it with `TMPDIR=/Volumes/Temp/claude cargo test \
              --test integration_tests --all-features -- --ignored --test-threads=4 \
              integration::non_utf8_paths`"
)]
fn libarchive_extract_to_memory_works_through_non_utf8_archive_name() {
    require_non_utf8_fs!();
    let mut archive_name = std::ffi::OsString::from("");
    archive_name.push(OsStr::from_bytes(b"extract-\xa0\xa1.tar"));
    let archive_path = temp_dir().join(&archive_name);
    cleanup(&archive_path);

    let opts = CompressionOptions::for_writable(WritableFormat::TAR);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    a.add_file_from_data("payload.bin", b"abc123").unwrap();
    a.finish().unwrap();

    let opened = Archive::open(&archive_path).unwrap();
    let bytes = opened.extract_to_memory("payload.bin").unwrap();
    assert_eq!(bytes, b"abc123");

    cleanup(&archive_path);
}
