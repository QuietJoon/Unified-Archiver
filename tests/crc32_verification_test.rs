//! Tests for CRC32 verification during extraction (Phase 2.5)
//!
//! Verifies that CRC32 verification is enabled and properly configured in both backends.
//!
//! ## CRC32 Verification Implementation
//!
//! Both UnRAR SDK and libarchive perform **automatic CRC32 verification** during extraction:
//!
//! - **UnRAR**: Automatically verifies CRC32 during RARProcessFile/RARProcessFileW.
//!   Returns ERAR_BAD_DATA (code 12) if CRC32 check fails.
//!   Mapped to ArchiveError::Corruption in wrapper.rs.
//!
//! - **libarchive**: Automatically verifies checksums during archive_read_data_block.
//!   Returns ARCHIVE_FAILED with error message containing "checksum" or "CRC" on failure.
//!   Detected and mapped to ArchiveError::Corruption in libarchive_wrapper.rs.
//!
//! The `verify_crc32` flag in ExtractionOptions documents this behavior but cannot
//! disable it, as both backend libraries provide no option to skip CRC32 verification.

// Only the two `rar-support`-gated lanes touch the filesystem directly; the
// third reads through the facade. Gated so the minimal profile compiles
// warning-free (AD-0070).
#[cfg(feature = "rar-support")]
use std::fs;
use std::path::PathBuf;
use unified_archive::{Archive, ArchiveError, ExtractionOptions};

/// Helper to get test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_crc32_error_mapping_corrupted_zip() {
    // R0079-0046: the earlier UnRAR/libarchive "mapping" tests here were
    // comment-only and could not fail. This exercises the mapping end to
    // end: corrupted_crc.zip stores a CRC32 that does not match its
    // data, so extraction with `verify_crc32: true` must surface
    // ArchiveError::Corruption (mapped via security::verify_crc32 in the
    // ZIP backend).
    let archive = Archive::open(fixtures_dir().join("corrupted_crc.zip"))
        .expect("corrupted-CRC fixture should open (central directory is intact)");

    let temp_dir = tempfile::tempdir().expect("create tempdir");
    let options = ExtractionOptions::new(temp_dir.path())
        .verify_crc32(true)
        .overwrite(true);

    let err = archive
        .extract_all(options)
        .expect_err("CRC32 mismatch must fail extraction");
    match err {
        ArchiveError::Corruption { path, details } => {
            assert!(
                path.contains("corrupt_test.txt"),
                "Corruption should name the corrupt entry, got: {}",
                path
            );
            assert!(
                details.contains("CRC32 mismatch"),
                "Corruption details should report the CRC32 mismatch, got: {}",
                details
            );
        }
        other => panic!("Expected ArchiveError::Corruption, got: {:?}", other),
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_crc32_verification_disabled() {
    // Note: Both UnRAR and libarchive perform CRC32 verification automatically
    // and don't provide a way to disable it. The verify_crc32 flag is for
    // documentation purposes and future backends that might support optional verification.

    let archive =
        Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open test archive");

    let temp_dest = std::env::temp_dir().join("crc32_disabled_test");
    fs::create_dir_all(&temp_dest).ok();

    // Disabled, but backends will still verify.
    let options = ExtractionOptions::new(&temp_dest).verify_crc32(false);

    let result = archive.extract_all(options);
    fs::remove_dir_all(&temp_dest).ok();

    // Should succeed because archive is valid
    assert!(
        result.is_ok(),
        "Expected successful extraction: {:?}",
        result
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_metadata_crc32_present() {
    // Verify that CRC32 values are present in metadata for formats that support it
    let archive =
        Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open test archive");

    let entries = archive.list_files().expect("Failed to list files");

    // RAR archives should have CRC32 in metadata
    for entry in entries {
        if entry.entry_type == unified_archive::EntryType::File {
            assert!(
                entry.crc32.is_some(),
                "Expected CRC32 for file entry: {}",
                entry.path
            );
        }
    }
}
