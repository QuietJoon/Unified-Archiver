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
#[cfg_attr(not(feature = "sevenzip"), allow(unused_imports))]
use super::common::command_exists;

use std::fs;
use std::path::Path;
#[cfg_attr(not(feature = "sevenzip"), allow(unused_imports))]
use std::process::Command;

use unified_archive::Archive;

/// Build a ZIP containing both a directory entry and a regular file.
///
/// `ZipWriter::add_directory` writes the trailing-slash name with the
/// MS-DOS directory attribute bit set — the same shape the `zip` CLI
/// produced and the one the `zip` crate recognizes as `is_dir()`.
///
/// OI-0056-010: this used to shell out to `zip -r` and return `false` when
/// the CLI was missing, so both consuming lanes reported success having
/// asserted nothing. The fixture check below proves the directory entry is
/// actually classified as one, so a writer regression fails rather than
/// skips.
fn build_zip_with_dir(archive_path: &Path) {
    common::build_zip_with_dir(archive_path);

    let probe = Archive::open(archive_path).expect("fixture zip must open");
    let listed = probe.list_files().expect("fixture zip must list");
    assert!(
        listed.iter().any(|e| e.is_directory()),
        "the zip writer stopped flagging `subdir/` as a directory, so these lanes would test \
         nothing: {:?}",
        listed
            .iter()
            .map(|e| (&e.path, e.entry_type))
            .collect::<Vec<_>>()
    );
}

#[cfg(feature = "sevenzip")]
/// Directory-carrying 7z via the CLI (`7zz`/`7z`).
///
/// OI-0056-010: the 7z writer is a library dependency rather than a
/// dev-dependency, so this fixture genuinely cannot be built in-process
/// from an integration test. A missing CLI is therefore a hard failure
/// with the install hint, not a silent skip.
fn build_7z_with_dir(archive_path: &Path, staging: &Path) {
    let cli = if command_exists("7zz") {
        "7zz"
    } else if command_exists("7z") {
        "7z"
    } else {
        panic!(
            "this lane needs the 7-Zip CLI to build a directory-carrying 7z (the 7z writer is a \
             library dependency, not a dev-dependency, so the fixture cannot be built \
             in-process). Install it with `brew install sevenzip` (provides `7zz`) or \
             `apt install p7zip-full` (provides `7z`)."
        )
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
        .status()
        .expect("spawn the 7-Zip CLI");
    assert!(status.success(), "{cli} failed to build the 7z fixture");
}

#[test]
fn single_file_extract_creates_directory_zip() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("dir.zip");
    let out_dir = tmp.join("out");
    build_zip_with_dir(&archive);
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
    let archive = tmp.join("dir_zipbe.zip");
    let out_dir = tmp.join("out");
    build_zip_with_dir(&archive);
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

#[cfg(feature = "sevenzip")]
#[test]
fn single_file_extract_creates_directory_sevenz() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("dir.7z");
    let out_dir = tmp.join("out");
    build_7z_with_dir(&archive, &staging);
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
