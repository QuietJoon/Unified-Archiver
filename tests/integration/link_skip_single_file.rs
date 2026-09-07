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

use unified_archive::{Archive, ArchiveError};

/// Build a ZIP carrying `regular.txt` and a `link.txt -> regular.txt`
/// symlink entry, then prove the fixture really is symlink-flagged.
///
/// OI-0056-010: this used to shell out to `zip --symlinks` and return
/// `false` when the CLI was missing, so both consuming lanes reported
/// success on a host without the CLI having asserted nothing. The builder
/// is now in-process (`ZipWriter::add_symlink`), and the S_IFLNK worry that
/// motivated the CLI is settled by assertion instead of avoidance: if the
/// writer ever stops setting the file-type bits, the fixture check below
/// fails loudly rather than the lane skipping.
fn build_zip_with_symlink(archive_path: &Path) {
    common::build_zip_with_symlink(archive_path);

    let probe = Archive::open(archive_path).expect("fixture zip must open");
    let listed = probe.list_files().expect("fixture zip must list");
    assert!(
        listed.iter().any(|e| e.is_symlink()),
        "the zip writer stopped emitting an S_IFLNK-flagged entry, so this lane would test \
         nothing: {:?}",
        listed
            .iter()
            .map(|e| (&e.path, e.entry_type))
            .collect::<Vec<_>>()
    );
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
    let archive = tmp.join("symlink_zip.zip");
    let out_dir = tmp.join("out");
    build_zip_with_symlink(&archive);
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

#[cfg(feature = "zip-read")]
#[cfg(feature = "create")]
#[test]
#[cfg(unix)]
fn single_file_extract_rejects_zip_symlink_zip_backend() {
    // The `zip`-crate backend is reached for encrypted ZIPs. Calling the
    // backend directly exercises the same extract_file_with_options path
    // that production uses for `open_encrypted`.
    #[cfg(feature = "zip-read")]
    use unified_archive::ffi::zip_wrapper::ZipArchive as ZipBackend;

    let tmp = common::temp_test_dir();
    let archive = tmp.join("symlink_zip.zip");
    let out_dir = tmp.join("out");
    build_zip_with_symlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let backend = ZipBackend::open(&archive).expect("open zip via zip-crate backend");
    let err = backend
        .extract_file_with_options("link.txt", &out_dir, true, false)
        .expect_err("symlink must be rejected");
    assert_link_blocked(err, "link.txt");
    assert!(!out_dir.join("link.txt").exists());

    common::cleanup(&tmp);
}

#[cfg(feature = "libarchive")]
#[test]
#[cfg(unix)]
fn single_file_extract_rejects_tar_symlink_libarchive() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("symlink.tar");
    let out_dir = tmp.join("out");

    common::build_tar_with_symlink(&archive);

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

#[cfg(feature = "libarchive")]
#[test]
#[cfg(unix)]
fn single_file_extract_rejects_tar_hardlink_libarchive() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("hardlink.tar");
    let out_dir = tmp.join("out");

    common::build_tar_with_hardlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    // The in-process ustar writer always emits `hardlink.txt` as the link
    // record, so the name that must be blocked is fixed. The old
    // CLI-built fixture left it up to whichever tar the host had, which is
    // why this used to accept a rejection from either name.
    let err = opened
        .extract_file("hardlink.txt", common::default_extraction_options(&out_dir))
        .expect_err("hardlink.txt must be rejected");
    assert!(
        matches!(&err, ArchiveError::OperationBlocked { reason, .. } if reason.contains("FR-022")),
        "expected FR-022 OperationBlocked, got {:?}",
        err
    );
    // The regular member is unaffected by the link policy.
    opened
        .extract_file("regular.txt", common::default_extraction_options(&out_dir))
        .expect("regular extraction");

    common::cleanup(&tmp);
}
