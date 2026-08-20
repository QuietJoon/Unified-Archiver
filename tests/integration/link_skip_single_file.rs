//! Integration tests for FR-022 link-skip policy on single-file extraction
//! (R0064-0009..0012 + R0064-0076..0079).
//!
//! The multi-entry extract path logs symlink/hard-link entries as
//! `ArchiveWarning::SkippedSymlink`/`SkippedHardLink` and continues. The
//! single-entry path (`Archive::extract_file`) has no warnings channel, so
//! when the caller targets a link by name it returns
//! `ArchiveError::OperationBlocked` instead of silently materializing or
//! silently succeeding with no file.

use super::common;

use std::fs;
use std::path::Path;
use std::process::Command;

use unified_archive::{Archive, ArchiveError};

/// Build a ZIP with a symlink entry via the `zip --symlinks` CLI. Writing a
/// real ZIP symlink from pure Rust is fragile because many zip writers mask
/// file-type bits out of `unix_permissions`; the CLI is the authoritative
/// reference for the S_IFLNK-on-external-attributes layout the ZIP reader reads.
fn build_zip_with_symlink(archive_path: &Path, staging: &Path) -> bool {
    if !common::command_exists("zip") || !common::command_exists("ln") {
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

    let status = Command::new("zip")
        .args([
            "--symlinks",
            archive_path.to_str().unwrap(),
            "regular.txt",
            "link.txt",
        ])
        .current_dir(staging)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn assert_link_blocked(err: ArchiveError, entry_hint: &str) {
    match err {
        ArchiveError::OperationBlocked { operation, reason } => {
            assert_eq!(operation, "extract_file", "operation tag");
            assert!(
                reason.contains(entry_hint),
                "reason should mention entry path '{}', got: {}",
                entry_hint,
                reason
            );
            assert!(
                reason.contains("FR-022"),
                "reason should cite FR-022 policy, got: {}",
                reason
            );
        }
        other => panic!("expected OperationBlocked, got {:?}", other),
    }
}

#[test]
#[cfg(unix)]
fn single_file_extract_rejects_zip_symlink() {
    // Every ZIP routes through the sole `zip`-crate backend (DCR-009).
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging_zip");
    let archive = tmp.join("symlink_zip.zip");
    let out_dir = tmp.join("out");
    if !build_zip_with_symlink(&archive, &staging) {
        eprintln!("skipping: zip --symlinks not available");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open zip");
    let err = opened
        .extract_file("link.txt", common::default_extraction_options(&out_dir))
        .expect_err("symlink must be rejected");
    assert_link_blocked(err, "link.txt");
    assert!(
        !out_dir.join("link.txt").exists(),
        "no file should be written"
    );

    // Regular entries still extract fine.
    opened
        .extract_file("regular.txt", common::default_extraction_options(&out_dir))
        .expect("regular extraction");
    assert!(out_dir.join("regular.txt").exists());

    common::cleanup(&tmp);
}

#[test]
#[cfg(unix)]
fn single_file_extract_rejects_zip_symlink_zip_backend() {
    // The `zip`-crate backend is reached for encrypted ZIPs. Calling the
    // backend directly exercises the same extract_file_with_options path
    // that production uses for `open_encrypted`.
    use unified_archive::ffi::zip_wrapper::ZipArchive as ZipBackend;

    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging_zip");
    let archive = tmp.join("symlink_zip.zip");
    let out_dir = tmp.join("out");
    if !build_zip_with_symlink(&archive, &staging) {
        eprintln!("skipping: zip --symlinks not available");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let backend = ZipBackend::open(&archive).expect("open zip via zip-crate backend");
    let err = backend
        .extract_file_with_options("link.txt", &out_dir, true, false)
        .expect_err("symlink must be rejected");
    assert_link_blocked(err, "link.txt");
    assert!(!out_dir.join("link.txt").exists());

    common::cleanup(&tmp);
}

#[test]
#[cfg(unix)]
fn single_file_extract_rejects_tar_symlink_libarchive() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("symlink.tar");
    let out_dir = tmp.join("out");

    if !common::build_tar_with_symlink(&archive, &staging) {
        eprintln!("skipping: failed to build tar fixture");
        common::cleanup(&tmp);
        return;
    }

    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    let err = opened
        .extract_file("link.txt", common::default_extraction_options(&out_dir))
        .expect_err("symlink must be rejected");
    assert_link_blocked(err, "link.txt");
    assert!(!out_dir.join("link.txt").exists());

    opened
        .extract_file("regular.txt", common::default_extraction_options(&out_dir))
        .expect("regular extraction");
    assert!(out_dir.join("regular.txt").exists());

    common::cleanup(&tmp);
}

#[test]
#[cfg(unix)]
fn single_file_extract_rejects_tar_hardlink_libarchive() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("hardlink.tar");
    let out_dir = tmp.join("out");

    if !common::build_tar_with_hardlink(&archive, &staging) {
        eprintln!("skipping: failed to build tar hardlink fixture");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    // Whichever member tar emitted as the hard-link reference is the one
    // that must be blocked. tar on macOS writes the second occurrence as a
    // hardlink record, so try "hardlink.txt" first and fall through to the
    // other name.
    let err =
        match opened.extract_file("hardlink.txt", common::default_extraction_options(&out_dir)) {
            Err(e) => e,
            Ok(()) => opened
                .extract_file("regular.txt", common::default_extraction_options(&out_dir))
                .expect_err("hardlink must be rejected from one of the two names"),
        };
    assert!(
        matches!(&err, ArchiveError::OperationBlocked { reason, .. } if reason.contains("FR-022")),
        "expected FR-022 OperationBlocked, got {:?}",
        err
    );

    common::cleanup(&tmp);
}
