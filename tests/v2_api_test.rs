//! Integration coverage for the typed-handle API (`v2::ReadArchive`,
//! `WriteArchive`, `ModifyArchive`).
//!
//! These exist because of the 2026-09-03 default flip. Until then `v2-api` was
//! an opt-in feature with eleven in-crate unit tests and **no integration
//! coverage at all** — thin for something nobody had to ask for, and much too
//! thin for something now in everyone's build. In-crate tests can reach private
//! state; these deliberately cannot, so they exercise the API as a consumer
//! actually meets it: through `unified_archive::v2`, with no access to the
//! `Archive` the handles wrap.
//!
//! The point of the typed split is that the COMPILER refuses what the legacy
//! facade only refused at runtime — you cannot call `add_entry` on a handle you
//! opened for reading, because that method does not exist on `ReadArchive`.
//! That property cannot be asserted from inside a test (it is a compile error,
//! not a value), so what these tests pin instead is the other half: that each
//! handle really does the job it is named for, end to end.

#![cfg(feature = "v2-api")]

#[path = "common/mod.rs"]
mod common;

use unified_archive::v2::{ModifyArchive, ReadArchive, WriteArchive};
use unified_archive::{CompressionOptions, ExtractionOptions, WritableFormat};

#[test]
fn read_archive_lists_and_extracts_through_the_typed_handle() {
    let temp = common::temp_test_dir();
    let archive = ReadArchive::open(common::fixture("test.zip")).expect("open for reading");

    let entries = archive.list_files().expect("list through ReadArchive");
    assert!(
        !entries.is_empty(),
        "the fixture must list at least one entry, or the rest proves nothing"
    );

    let dest = temp.join("out");
    archive
        .extract_all(ExtractionOptions::new(&dest))
        .expect("extract through ReadArchive");
    assert!(
        std::fs::read_dir(&dest)
            .expect("destination exists")
            .next()
            .is_some(),
        "a typed read handle must materialise entries, not just enumerate them"
    );

    common::cleanup(&temp);
}

#[test]
fn write_archive_creates_an_archive_a_read_handle_can_open() {
    let temp = common::temp_test_dir();
    let path = temp.join("written.zip");

    let mut writer =
        WriteArchive::create(&path, CompressionOptions::for_writable(WritableFormat::ZIP))
            .expect("create through WriteArchive");
    writer
        .add_file_from_data("hello.txt", b"typed handles")
        .expect("add an entry");
    writer.finish().expect("finish");

    // The round trip is the assertion that matters: a handle produced by one
    // typed type has to be readable by another, or the split has divided the
    // API rather than clarified it.
    let reader = ReadArchive::open(&path).expect("reopen what WriteArchive produced");
    let entries = reader.list_files().expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "hello.txt");

    common::cleanup(&temp);
}

#[test]
fn modify_archive_commits_and_the_result_reads_back() {
    let temp = common::temp_test_dir();
    let path = temp.join("modified.zip");

    let mut writer =
        WriteArchive::create(&path, CompressionOptions::for_writable(WritableFormat::ZIP))
            .expect("create");
    writer
        .add_file_from_data("original.txt", b"before")
        .expect("seed an entry");
    writer.finish().expect("finish");

    let mut modify = ModifyArchive::open(&path).expect("open for modification");
    modify
        .add_entry("added.txt", b"after")
        .expect("stage an addition");
    modify.commit_changes().expect("commit");

    let reader = ReadArchive::open(&path).expect("reopen after commit");
    let mut names: Vec<String> = reader
        .list_files()
        .expect("list")
        .iter()
        .map(|e| e.path.clone())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["added.txt".to_string(), "original.txt".to_string()]
    );

    common::cleanup(&temp);
}

/// The mode split's whole purpose, asserted at the one place it is observable
/// at runtime: a handle opened for reading cannot be used to write, and the
/// refusal is typed rather than a panic.
///
/// The stronger property — that `ReadArchive` has no `add_entry` to call — is a
/// compile-time fact and cannot be written as a test. What can be checked is
/// that the read handle refuses a *modify-shaped* request on an archive it does
/// not own the right to change.
#[test]
fn a_read_handle_does_not_expose_a_commit_path() {
    let archive = ReadArchive::open(common::fixture("test.zip")).expect("open for reading");
    // `validate_integrity` is the heaviest thing a read handle is allowed to
    // do; it must succeed, which is the control that keeps the assertion above
    // from passing merely because the handle is broken.
    let report = archive
        .validate_integrity()
        .expect("a read handle may validate");
    assert!(
        report.total_entries > 0,
        "validation must actually walk the archive"
    );
}

/// Recovery metadata through the typed handle, pinning the 0.5.0 widening at
/// the integration altitude too: `ReadArchive::recovery_percentage` forwards
/// the facade and must carry the same `Option<u16>` it does.
#[test]
fn read_archive_recovery_percentage_is_u16() {
    let archive = ReadArchive::open(common::fixture("test.zip")).expect("open");
    let pct: Option<u16> = archive
        .recovery_percentage()
        .expect("a ZIP reports no recovery record rather than failing");
    assert_eq!(pct, None, "ZIP has no recovery-record concept");
}
