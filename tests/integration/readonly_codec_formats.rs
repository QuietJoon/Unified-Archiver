//! Integration tests for the read-only ZST / LZ4 / LZMA codec formats
//! (OI-0078-001).
//!
//! These formats already had enum variants, capability entries, extension
//! routing, and magic detection, but no end-to-end open/list/extract
//! coverage. Each family test covers both surfaces:
//!
//! - the standalone codec file (`test.txt.<codec>`): per MADR-0019 the raw
//!   single-stream input is exposed as one pseudo-entry named after the
//!   archive's file stem (`test.txt.zst` → `test.txt`), and
//!   `extract_to_memory` yields the decompressed payload;
//! - the compressed tar (`test.tar.<codec>`): `list_files()` shows the tar
//!   member and `extract_all` materializes it with exact content.
//!
//! Fixtures are committed under `tests/fixtures/` (see the README there for
//! the exact generation commands), so these tests need no codec CLI at
//! runtime.

// ZST / LZ4 / LZMA are all read through libarchive, so the whole file
// needs that backend (AD 0058 format features).
#![cfg(feature = "libarchive")]

use super::common;

use std::fs;
use unified_archive::{Archive, ArchiveFormat, EntryType};

/// The known payload compressed into every OI-0078-001 fixture:
/// `"unified-archive codec fixture\n"` repeated four times (120 bytes).
fn payload() -> Vec<u8> {
    b"unified-archive codec fixture\n".repeat(4)
}

/// Standalone codec file: open, verify format, list the single MADR-0019
/// pseudo-entry (named after the archive stem), and extract it to memory.
fn assert_standalone_codec(fixture_name: &str, expected_format: ArchiveFormat) {
    let path = common::fixture(fixture_name);
    let archive =
        Archive::open(&path).unwrap_or_else(|e| panic!("Failed to open {}: {}", fixture_name, e));
    assert_eq!(
        archive.format(),
        expected_format,
        "Format detection mismatch for {}",
        fixture_name
    );

    let entries = archive
        .list_files()
        .unwrap_or_else(|e| panic!("Failed to list {}: {}", fixture_name, e));
    assert_eq!(
        entries.len(),
        1,
        "{} should expose exactly one pseudo-entry",
        fixture_name
    );
    assert_eq!(
        entries[0].path, "test.txt",
        "MADR-0019 pseudo-entry should be named after the archive stem for {}",
        fixture_name
    );

    let data = archive
        .extract_to_memory(&entries[0].path)
        .unwrap_or_else(|e| panic!("Failed to extract {} to memory: {}", fixture_name, e));
    assert_eq!(
        data,
        payload(),
        "Decompressed payload mismatch for {}",
        fixture_name
    );
}

/// Compressed tar: open, verify format, list the tar member, and
/// extract_all into a tempdir, verifying the member's exact content.
fn assert_tar_codec(fixture_name: &str, expected_format: ArchiveFormat) {
    let path = common::fixture(fixture_name);
    let archive =
        Archive::open(&path).unwrap_or_else(|e| panic!("Failed to open {}: {}", fixture_name, e));
    assert_eq!(
        archive.format(),
        expected_format,
        "Format detection mismatch for {}",
        fixture_name
    );

    let entries = archive
        .list_files()
        .unwrap_or_else(|e| panic!("Failed to list {}: {}", fixture_name, e));
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(
        file_entries.len(),
        1,
        "{} should contain exactly one file member",
        fixture_name
    );
    assert_eq!(
        file_entries[0].path, "codec_member.txt",
        "Unexpected tar member path in {}",
        fixture_name
    );

    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let options = common::default_extraction_options(temp.path().to_path_buf());
    archive
        .extract_all(options)
        .unwrap_or_else(|e| panic!("Failed to extract_all {}: {}", fixture_name, e));

    let extracted = temp.path().join("codec_member.txt");
    assert!(
        extracted.exists(),
        "extract_all of {} should produce codec_member.txt",
        fixture_name
    );
    let data = fs::read(&extracted).expect("Failed to read extracted member");
    assert_eq!(
        data,
        payload(),
        "Extracted member content mismatch for {}",
        fixture_name
    );
}

#[test]
fn test_zst_standalone_and_tar() {
    assert_standalone_codec("test.txt.zst", ArchiveFormat::Zst);
    assert_tar_codec("test.tar.zst", ArchiveFormat::TarZst);
}

#[test]
fn test_lz4_standalone_and_tar() {
    assert_standalone_codec("test.txt.lz4", ArchiveFormat::Lz4);
    assert_tar_codec("test.tar.lz4", ArchiveFormat::TarLz4);
}

#[test]
fn test_lzma_standalone_and_tar() {
    assert_standalone_codec("test.txt.lzma", ArchiveFormat::Lzma);
    assert_tar_codec("test.tar.lzma", ArchiveFormat::TarLzma);
}
