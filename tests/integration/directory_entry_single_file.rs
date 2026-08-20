//! Integration tests for single-file extraction of directory entries
//! (R0064-0013/0014/0015 origin, R0064-0080/0081/0082 coverage).
//!
//! Contract history: the single-entry path briefly materialized
//! directory entries as `create_dir_all`, but R0070-0027 (facade) and
//! R0076-0054 via OI-0076-002 (backends, shared
//! `validate_single_entry` gate) settled on rejection — single-entry
//! APIs commit to regular-file payloads, so requesting a directory
//! entry fails loudly and leaves the destination untouched. Directory
//! entries are materialized by the multi-entry paths
//! (`extract_all` / `extract_files`), which branch on
//! `is_directory`/`is_dir` and call `create_dir_all`.

use super::common;
use super::common::command_exists;

use std::fs;
use std::path::Path;
use std::process::Command;

use unified_archive::Archive;

/// Build a ZIP containing both a directory entry and a regular file using
/// the `zip` CLI. The CLI writes trailing-slash directory entries with the
/// MS-DOS directory attribute bit set — the canonical format the `zip`
/// crate recognizes as `is_dir()`.
fn build_zip_with_dir(archive_path: &Path, staging: &Path) -> bool {
    if !command_exists("zip") {
        return false;
    }
    fs::create_dir_all(staging.join("subdir")).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello\n").expect("write regular");

    let status = Command::new("zip")
        .args([
            "-r",
            archive_path.to_str().unwrap(),
            "regular.txt",
            "subdir",
        ])
        .current_dir(staging)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn build_7z_with_dir(archive_path: &Path, staging: &Path) -> bool {
    let cli = if command_exists("7zz") {
        "7zz"
    } else if command_exists("7z") {
        "7z"
    } else {
        return false;
    };
    fs::create_dir_all(staging.join("subdir")).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello\n").expect("write regular");

    let status = Command::new(cli)
        .args([
            "a",
            "-y",
            archive_path.to_str().unwrap(),
            "regular.txt",
            "subdir",
        ])
        .current_dir(staging)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

#[test]
fn single_file_extract_creates_directory_zip() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("dir.zip");
    let out_dir = tmp.join("out");
    if !build_zip_with_dir(&archive, &staging) {
        eprintln!("skipping: zip CLI not available");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open zip");

    // Directory entries in zip are named with a trailing slash.
    let entries = opened.list_files().expect("list");
    let dir_entry = entries
        .iter()
        .find(|e| e.is_directory())
        .expect("archive must contain a directory entry")
        .path
        .clone();

    // R0070-0027: `extract_file` now rejects directory entries with a
    // structured `OperationBlocked` error. Callers that want to
    // recreate a directory entry must go through `extract_some` /
    // `extract_files` instead. This test now asserts the rejection
    // contract; the previous "materialise as a real directory" path
    // was inconsistent with the memory/stream APIs (which already
    // refused directory entries) and produced cross-backend
    // divergence.
    let err = opened
        .extract_file(&dir_entry, common::default_extraction_options(&out_dir))
        .expect_err("extract_file on a directory entry must fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("not a regular file"),
        "expected directory rejection, got: {msg}"
    );
    let _ = out_dir; // not materialised

    common::cleanup(&tmp);
}

#[test]
fn single_file_extract_creates_directory_zip_backend() {
    use unified_archive::ffi::zip_wrapper::ZipArchive as ZipBackend;

    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("dir_zipbe.zip");
    let out_dir = tmp.join("out");
    if !build_zip_with_dir(&archive, &staging) {
        eprintln!("skipping: zip CLI not available");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let backend = ZipBackend::open(&archive).expect("open via zip-crate backend");
    // R0076-0054 (via OI-0076-002) supersedes the R0064-0013-era
    // directory-materialize pin: the backend single-entry path now
    // routes through the shared `validate_single_entry` gate, which
    // rejects non-regular entries. Use extract_all/extract_files to
    // materialize directories. zip-crate directory names carry a
    // trailing slash.
    let err = backend
        .extract_file_with_options("subdir/", &out_dir, true, false)
        .expect_err("directory entry via single-entry backend API must fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("not a regular file"),
        "expected directory rejection, got: {msg}"
    );
    assert!(
        !out_dir.join("subdir").exists(),
        "subdir must not be materialized by the rejected single-entry call"
    );

    common::cleanup(&tmp);
}

#[test]
fn single_file_extract_creates_directory_sevenz() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("dir.7z");
    let out_dir = tmp.join("out");
    if !build_7z_with_dir(&archive, &staging) {
        eprintln!("skipping: 7z/7zz CLI not available");
        common::cleanup(&tmp);
        return;
    }
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open 7z");
    let entries = opened.list_files().expect("list");
    let dir_entry = entries
        .iter()
        .find(|e| e.is_directory())
        .expect("archive must contain a directory entry")
        .path
        .clone();

    // R0070-0027: `extract_file` now rejects directory entries.
    let err = opened
        .extract_file(&dir_entry, common::default_extraction_options(&out_dir))
        .expect_err("extract_file on a directory entry must fail");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("not a regular file"),
        "expected directory rejection, got: {msg}"
    );

    common::cleanup(&tmp);
}
