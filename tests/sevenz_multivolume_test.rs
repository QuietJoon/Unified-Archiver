//! A numerically split 7z set reads as one archive (OI-0080-004).
//!
//! # The fact this whole feature rests on
//!
//! A 7z volume set is **a plain byte split of one archive**. Concatenating
//! `x.7z.001`, `x.7z.002` and the rest reproduces `x.7z` byte for byte. No part
//! carries a header, a footer, or a volume number, and only part 1 begins with
//! the 7z magic — every later part is raw payload. The format specification has
//! no volume concept at all, and 7-Zip itself handles `.001` sets through a
//! *signature-less* pseudo-format whose reader is a bare concatenating adapter.
//!
//! So this needed no format knowledge: the backend reads the members as one
//! source through `VolumeChain`, and the 7z parser never learns there was a
//! split. `split_set_lists_exactly_like_the_unsplit_archive` is the test that
//! pins that claim, by comparing against the same content written unsplit.
//!
//! # Why this is not the RAR problem
//!
//! RAR volumes are self-describing: each carries `MHD_VOLUME` and a volume
//! number, which is why multi-volume RAR needed the listing walk taught about
//! continuation headers (ticgit 3b4d15). None of that applies here, and the
//! difference has one user-visible consequence worth pinning — see
//! `an_incomplete_set_is_refused_by_name_not_as_corruption`.
//!
//! Fixtures come from `scripts/generate-7z-fixtures.sh`, run by hand and
//! committed; never from the build.

// Every test in this file is about the 7z backend, so the whole file is
// gated rather than each test: gating them individually left the imports
// and fixture constants dangling in the minimal profile.
#![cfg(feature = "sevenzip")]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::{Path, PathBuf};

use unified_archive::{Archive, ArchiveFormat, Support};

const PARTS: [&str; 4] = [
    "test_split.7z.001",
    "test_split.7z.002",
    "test_split.7z.003",
    "test_split.7z.004",
];

/// The same content written as a single archive, for differential comparison.
const WHOLE: &str = "test_split_whole.7z";

/// Copy the named fixtures into a fresh directory; return it and the first.
fn stage(names: &[&str]) -> (PathBuf, PathBuf) {
    let dir = common::temp_test_dir();
    for name in names {
        fs::copy(common::fixture(name), dir.join(name))
            .unwrap_or_else(|e| panic!("staging {name}: {e}"));
    }
    (dir.clone(), dir.join(names[0]))
}

fn subdir(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(&path).expect("create destination");
    path
}

/// The capability matrix must match what the tests below demonstrate.
#[test]
fn sevenz_multipart_read_is_full() {
    let caps = ArchiveFormat::SevenZip.capabilities();
    assert_eq!(
        caps.multipart_read,
        Support::Full,
        "a complete split set lists and extracts, so this is not None"
    );
    assert_eq!(
        caps.multipart_write,
        Support::None,
        "reading a split set and writing one are different jobs"
    );
    assert!(ArchiveFormat::SevenZip.supports_multipart());
}

/// The differential test, and the one that would catch a reassembly bug: the
/// split set must be indistinguishable from the same content unsplit.
#[test]
fn split_set_lists_exactly_like_the_unsplit_archive() {
    let (_dir, first) = stage(&PARTS);
    let split = Archive::open(&first).expect("open volume 1 of a complete set");
    let whole = Archive::open(common::fixture(WHOLE)).expect("open the unsplit archive");

    let mut split_entries: Vec<(String, Option<u64>)> = split
        .list_files()
        .expect("list the split set")
        .iter()
        .map(|e| (e.path.clone(), e.size))
        .collect();
    let mut whole_entries: Vec<(String, Option<u64>)> = whole
        .list_files()
        .expect("list the unsplit archive")
        .iter()
        .map(|e| (e.path.clone(), e.size))
        .collect();
    split_entries.sort();
    whole_entries.sort();

    assert_eq!(
        split_entries, whole_entries,
        "a split set is a byte split of the same archive, so the listing must be identical"
    );
    assert!(
        split_entries.len() >= 2,
        "the fixture must hold more than one entry or it proves little: {split_entries:?}"
    );
}

/// Extraction across the volume boundary, byte for byte against the unsplit
/// archive. The payload is deliberately larger than one volume, so a
/// reassembly that dropped or duplicated a boundary byte fails here.
#[test]
fn split_set_extracts_the_same_bytes_as_the_unsplit_archive() {
    let (dir, first) = stage(&PARTS);
    let split = Archive::open(&first).expect("open volume 1");
    let whole = Archive::open(common::fixture(WHOLE)).expect("open unsplit");

    let split_dest = subdir(&dir, "split");
    split
        .extract_all(common::default_extraction_options(split_dest.clone()))
        .expect("extract the split set");

    let whole_dest = subdir(&dir, "whole");
    whole
        .extract_all(common::default_extraction_options(whole_dest.clone()))
        .expect("extract the unsplit archive");

    for entry in split.list_files().expect("list") {
        let from_split = fs::read(split_dest.join(&entry.path))
            .unwrap_or_else(|e| panic!("read {} from the split extraction: {e}", entry.path));
        let from_whole = fs::read(whole_dest.join(&entry.path))
            .unwrap_or_else(|e| panic!("read {} from the unsplit extraction: {e}", entry.path));
        assert_eq!(
            from_split, from_whole,
            "{} differs between the split and unsplit extractions",
            entry.path
        );
    }

    // The payload must actually span volumes, or this test is vacuous.
    let payload = fs::metadata(split_dest.join("split_payload.bin"))
        .expect("stat the payload")
        .len();
    let volume = fs::metadata(common::fixture(PARTS[0]))
        .expect("stat volume 1")
        .len();
    assert!(
        payload > volume,
        "the payload ({payload}) must exceed one volume ({volume}) or nothing was reassembled"
    );

    common::cleanup(&dir);
}

#[cfg(feature = "integrity")]
/// Integrity validation sees one archive, not four files.
#[test]
fn split_set_validates_as_one_archive() {
    let (dir, first) = stage(&PARTS);
    let archive = Archive::open(&first).expect("open volume 1");
    let report = archive
        .validate_integrity()
        .expect("validate the split set");
    assert!(report.total_entries >= 2);
    assert_eq!(report.validated, report.total_entries);
    assert!(
        report.failed.is_empty(),
        "a complete set must validate clean: {:?}",
        report.failed
    );
    common::cleanup(&dir);
}

/// **The behaviour that has to be deliberate rather than emergent.**
///
/// A byte-split set carries nothing that marks a boundary, so a missing middle
/// volume cannot be noticed while reading — the later parts simply land at the
/// wrong offsets and the archive decodes as garbage. Left alone, that surfaces
/// as "Invalid 7z" and sends the caller looking for a damaged file instead of a
/// missing one. So completeness is judged from the *names* before any bytes are
/// read, and the refusal has to say which volume is absent.
#[test]
fn an_incomplete_set_is_refused_by_name_not_as_corruption() {
    // Stage volumes 1, 2 and 4 — a hole at 3, which is the case that would
    // otherwise decode as corruption rather than as an absence.
    let (dir, first) = stage(&[PARTS[0], PARTS[1], PARTS[3]]);

    let opened = Archive::open(&first).expect("opening volume 1 still succeeds; it is a real 7z");
    let err = opened
        .list_files()
        .expect_err("a set with a hole must not list");

    let message = err.to_string();
    assert!(
        message.contains("incomplete"),
        "the error must name the absence rather than blame the bytes: {message}"
    );
    assert!(
        !message.contains("Invalid 7z"),
        "a missing volume must not be reported as a corrupt archive: {message}"
    );

    common::cleanup(&dir);
}

/// A lone `.001` with no siblings is just a file with an odd name, and must
/// still open as an ordinary archive rather than being treated as a broken set.
#[test]
fn a_single_volume_named_001_is_not_a_set() {
    let dir = common::temp_test_dir();
    let solo = dir.join("solo.7z.001");
    fs::copy(common::fixture(WHOLE), &solo).expect("stage a lone .001");

    let archive = Archive::open(&solo).expect("a lone .001 is an ordinary archive");
    let entries = archive.list_files().expect("list it");
    assert!(
        entries.len() >= 2,
        "the lone file must read as the whole archive it is: {entries:?}"
    );

    common::cleanup(&dir);
}

/// An ordinary `.7z` must be unaffected — the split support added a code path
/// every archive now goes through, so the unsplit case is the regression risk.
#[test]
fn an_ordinary_archive_still_reads() {
    let archive = Archive::open(common::fixture("test.7z")).expect("open an ordinary 7z");
    let entries = archive.list_files().expect("list");
    assert!(!entries.is_empty());
}
