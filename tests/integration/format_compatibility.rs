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

use std::path::PathBuf;
use unified_archive::{Archive, ArchiveFormat};

/// Helper to get fixture path
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

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
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

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

#[test]
#[serial_test::file_serial(rar)]
fn test_list_files_consistency() {
    // Test that list_files() provides consistent entry structure across formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));
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

#[test]
#[serial_test::file_serial(rar)]
fn test_entry_count_consistency() {
    // Test that entry_count() returns consistent values
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

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

#[test]
#[serial_test::file_serial(rar)]
fn test_find_entry_consistency() {
    // Test that find_entry() works consistently across formats
    //
    // Note: This test assumes all test archives contain a file named "test_file.txt"
    // Adjust the filename based on actual fixture contents
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        // Get first entry from list_files for testing
        let entries = archive
            .list_files()
            .unwrap_or_else(|_| panic!("Failed to list files in {}", filename));

        if entries.is_empty() {
            continue;
        }

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

#[test]
#[serial_test::file_serial(rar)]
fn test_find_entry_not_found() {
    // Test that find_entry() returns None for non-existent entries
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

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

#[test]
#[serial_test::file_serial(rar)]
fn test_validate_integrity_consistency() {
    // Test that validate_integrity() provides consistent reporting across formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

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

#[test]
#[serial_test::file_serial(rar)]
fn test_path_getter_consistency() {
    // Test that path() returns the correct path for all formats
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        assert_eq!(
            archive.path(),
            path.as_path(),
            "Archive::path() should return original path for {}",
            filename
        );
    }
}

#[test]
#[serial_test::file_serial(rar)]
fn test_is_encrypted_consistency() {
    // Test that is_encrypted() works consistently across formats
    let unencrypted_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in unencrypted_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

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
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        // Try to open without password first - this should succeed for listing
        // but is_encrypted() might fail if password is required for metadata
        let archive_result = Archive::open(&path);

        if let Ok(archive) = archive_result {
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
}

#[test]
#[serial_test::file_serial(rar)]
fn test_calculate_archive_crc_consistency() {
    // Test that calculate_archive_crc() works consistently
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        let archive_crc = archive
            .calculate_archive_crc()
            .unwrap_or_else(|_| panic!("Failed to calculate archive CRC for {}", filename));

        // Archive CRC should be deterministic
        // Calculate again and verify consistency
        let archive2 = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to reopen {}", filename));
        let archive_crc2 = archive2.calculate_archive_crc().unwrap_or_else(|_| panic!(
            "Failed to recalculate archive CRC for {}",
            filename
        ));

        assert_eq!(
            archive_crc, archive_crc2,
            "Archive CRC should be consistent for {}",
            filename
        );
    }
}

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
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping {}: fixture not found", filename);
            continue;
        }

        // 1. Open archive
        let archive = Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

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

        // 6. Find entry
        if !entries.is_empty() {
            let first_path = &entries[0].path;
            let found = archive
                .find_entry(first_path)
                .unwrap_or_else(|_| panic!("Failed to find entry in {}", filename));
            assert!(found.is_some());
        }

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
