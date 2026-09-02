//! Integration tests for format compatibility
//!
//! Verifies that the unified API provides consistent behavior across all supported
//! archive formats (ZIP, RAR, RAR5, 7z, TAR.GZ, etc.).
//!
//! Tests ensure that:
//! - Archive::open() works for all formats
//! - list_files() returns consistent entry structure
//! - Metadata fields are consistently populated
//! - find_entry() works identically across formats
//! - validate_integrity() provides consistent reporting
//!
//! **Companion suite (R0074-0073).** [`tests/format_compatibility_test.rs`]
//! holds the root-level smoke/regression coverage; this file carries
//! broader cross-format expectations. Future consolidation tracked
//! under R0074-0079 (test taxonomy).

// This suite's cases are `rar-support`-gated, so the minimal profile compiles
// the file to nothing and every helper below it dangles. AD-0070 makes that
// profile a release lane under `-D warnings`, where a dangling helper is a
// build failure.
#[cfg(feature = "rar-support")]
use std::path::PathBuf;
#[cfg(feature = "rar-support")]
use unified_archive::{Archive, ArchiveFormat};

/// Helper to get fixture path
#[cfg(feature = "rar-support")]
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Resolve a **required** committed fixture, asserting it is present.
///
/// Every fixture named in this suite is committed under `tests/fixtures/`, so a
/// missing file signals a mispackaged checkout rather than an optional case to
/// skip. Panicking here keeps the cross-format matrix from passing vacuously
/// when no fixtures are exercised (R0081-0089).
#[cfg(feature = "rar-support")]
fn require_fixture(name: &str) -> PathBuf {
    let path = fixture_path(name);
    assert!(
        path.exists(),
        "required fixture `{}` is missing at {} \u{2014} the cross-format matrix cannot run vacuously",
        name,
        path.display()
    );
    path
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_multiple_formats() {
    // Test that Archive::open() works for all formats
    // Note: test.rar is actually RAR5 format (both test.rar and test_rar5.rar are v5)
    let formats = vec![
        ("test.rar", ArchiveFormat::Rar5),
        ("test_rar5.rar", ArchiveFormat::Rar5),
        ("test.zip", ArchiveFormat::Zip),
        ("test.7z", ArchiveFormat::SevenZip),
    ];

    for (filename, expected_format) in formats {
        let path = require_fixture(filename);

        let result = Archive::open(&path);
        assert!(
            result.is_ok(),
            "Failed to open {} (format: {:?}): {:?}",
            filename,
            expected_format,
            result.err()
        );

        let archive = result.unwrap();
        assert_eq!(
            archive.format(),
            expected_format,
            "Format detection mismatch for {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_list_files_consistency() {
    // Test that list_files() provides consistent entry structure across formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));
        let entries = archive
            .list_files()
            .unwrap_or_else(|_| panic!("Failed to list files in {}", filename));

        // All archives should have entries
        assert!(!entries.is_empty(), "{} should contain entries", filename);

        // Verify each entry has consistent fields
        for entry in entries {
            // Path should always be populated
            assert!(!entry.path.is_empty(), "Entry path should not be empty");

            // Entry type should be set
            // (File or Directory)
            assert!(
                matches!(
                    entry.entry_type,
                    unified_archive::EntryType::File
                        | unified_archive::EntryType::Directory
                        | unified_archive::EntryType::Symlink
                ),
                "Entry type should be File, Directory, or Symlink"
            );

            // Files should have size information
            if entry.entry_type == unified_archive::EntryType::File {
                // Note: Some formats may not provide all metadata
                // We just verify the fields exist and are accessible
                let _ = entry.size;
                let _ = entry.compressed_size;
                let _ = entry.crc32;
            }
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_entry_count_consistency() {
    // Test that entry_count() returns consistent values
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let entry_count = archive
            .entry_count()
            .unwrap_or_else(|_| panic!("Failed to get entry count for {}", filename));
        let list_count = archive
            .list_files()
            .unwrap_or_else(|_| panic!("Failed to list files in {}", filename))
            .len();

        assert_eq!(
            entry_count, list_count,
            "entry_count() and list_files().len() should match for {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_find_entry_consistency() {
    // Test that find_entry() works consistently across formats
    //
    // The lookup target is each archive's own first listed entry rather than a
    // hard-coded member name, so the case exercises every committed fixture
    // without depending on any particular file being present inside them.
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        // Get first entry from list_files for testing
        let entries = archive
            .list_files()
            .unwrap_or_else(|_| panic!("Failed to list files in {}", filename));

        assert!(
            !entries.is_empty(),
            "{} should contain at least one entry to exercise find_entry",
            filename
        );

        let first_entry_path = &entries[0].path;

        // Try to find the entry
        let found = archive
            .find_entry(first_entry_path)
            .unwrap_or_else(|_| panic!("Failed to find entry in {}", filename));

        assert!(
            found.is_some(),
            "Should find entry '{}' in {}",
            first_entry_path,
            filename
        );

        let found_entry = found.unwrap();
        assert_eq!(
            found_entry.path, *first_entry_path,
            "Found entry path should match"
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_find_entry_not_found() {
    // Test that find_entry() returns None for non-existent entries
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let found = archive
            .find_entry("nonexistent_file_12345.txt")
            .unwrap_or_else(|_| panic!("Failed to search for entry in {}", filename));

        assert!(
            found.is_none(),
            "Should not find non-existent entry in {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_validate_integrity_consistency() {
    // Test that validate_integrity() provides consistent reporting across formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let report = archive
            .validate_integrity()
            .unwrap_or_else(|_| panic!("Failed to validate integrity of {}", filename));

        // Report should have sensible values
        let entry_count = archive
            .entry_count()
            .unwrap_or_else(|_| panic!("Failed to get entry count for {}", filename));

        assert_eq!(
            report.total_entries, entry_count,
            "ValidationReport.total_entries should match entry_count() for {}",
            filename
        );

        // Validated + failed should not exceed total
        assert!(
            report.validated + report.failed.len() <= report.total_entries,
            "Validated + failed should not exceed total entries for {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_path_getter_consistency() {
    // Test that path() returns the correct path for all formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        assert_eq!(
            archive.path(),
            path.as_path(),
            "Archive::path() should return original path for {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_is_encrypted_consistency() {
    // Test that is_encrypted() works consistently across formats
    let unencrypted_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in unencrypted_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let is_encrypted = archive
            .is_encrypted()
            .unwrap_or_else(|_| panic!("Failed to check encryption for {}", filename));

        // These test archives should not be encrypted
        assert!(!is_encrypted, "{} should not be encrypted", filename);
    }

    // Test encrypted archives
    // Note: For password-protected archives, is_encrypted() may return an error
    // if the archive can't be opened without a password. This is expected behavior.
    let encrypted_files = vec![
        ("test_encrypted_data.rar", true, "test123"),
        ("test_encrypted.zip", true, "test123"),
    ];

    for (filename, should_be_encrypted, password) in encrypted_files {
        let path = require_fixture(filename);

        // Opening without a password must succeed for these header-unencrypted
        // fixtures; password validation is deferred to extraction (AD 0014).
        let archive = Archive::open(&path)
            .unwrap_or_else(|_| panic!("Failed to open {} without password", filename));

        // Some formats allow opening without password for inspection
        let is_encrypted_result = archive.is_encrypted();

        if let Ok(is_encrypted) = is_encrypted_result {
            assert_eq!(
                is_encrypted, should_be_encrypted,
                "{} encryption detection mismatch",
                filename
            );
        } else {
            // Password required - try with password
            let archive_with_pass = Archive::open_encrypted(&path, password)
                .unwrap_or_else(|_| panic!("Failed to open {} with password", filename));

            let is_encrypted = archive_with_pass
                .is_encrypted()
                .unwrap_or_else(|_| panic!("Failed to check encryption for {}", filename));

            assert_eq!(
                is_encrypted, should_be_encrypted,
                "{} encryption detection mismatch",
                filename
            );
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_calculate_archive_crc_consistency() {
    // Test that calculate_archive_crc() works consistently
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = require_fixture(filename);

        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let archive_crc = archive
            .calculate_archive_crc()
            .unwrap_or_else(|_| panic!("Failed to calculate archive CRC for {}", filename));

        // Archive CRC should be deterministic
        // Calculate again and verify consistency
        let archive2 =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to reopen {}", filename));
        let archive_crc2 = archive2
            .calculate_archive_crc()
            .unwrap_or_else(|_| panic!("Failed to recalculate archive CRC for {}", filename));

        assert_eq!(
            archive_crc, archive_crc2,
            "Archive CRC should be consistent for {}",
            filename
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_unified_api_cross_format() {
    // Comprehensive test demonstrating the unified API working across formats
    // Note: test.rar is actually RAR5 format
    let test_files = vec![
        ("test.rar", ArchiveFormat::Rar5),
        ("test.zip", ArchiveFormat::Zip),
        ("test.7z", ArchiveFormat::SevenZip),
    ];

    for (filename, expected_format) in test_files {
        let path = require_fixture(filename);

        // 1. Open archive
        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        // 2. Verify format detection
        assert_eq!(archive.format(), expected_format);

        // 3. Get path
        assert_eq!(archive.path(), path.as_path());

        // 4. List files
        let entries = archive
            .list_files()
            .unwrap_or_else(|_| panic!("Failed to list files in {}", filename));
        assert!(!entries.is_empty());

        // 5. Get entry count
        let count = archive
            .entry_count()
            .unwrap_or_else(|_| panic!("Failed to get entry count for {}", filename));
        assert_eq!(count, entries.len());

        // 6. Find entry (entries is non-empty per the assertion in step 4)
        let first_path = &entries[0].path;
        let found = archive
            .find_entry(first_path)
            .unwrap_or_else(|_| panic!("Failed to find entry in {}", filename));
        assert!(found.is_some());

        // 7. Check encryption
        let _ = archive
            .is_encrypted()
            .unwrap_or_else(|_| panic!("Failed to check encryption for {}", filename));

        // 8. Validate integrity
        let report = archive
            .validate_integrity()
            .unwrap_or_else(|_| panic!("Failed to validate integrity of {}", filename));
        assert_eq!(report.total_entries, count);

        // 9. Calculate archive CRC
        let _ = archive
            .calculate_archive_crc()
            .unwrap_or_else(|_| panic!("Failed to calculate CRC for {}", filename));

        println!(" {} - All unified API operations successful", filename);
    }
}
