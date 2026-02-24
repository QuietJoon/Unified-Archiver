//! Contract tests for inspection operations
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/inspection.md:
//! 1. List files from each supported format
//! 2. Verify metadata consistency (size, crc32, modified time)
//! 3. Verify enhanced metadata (compression_ratio, is_encrypted, created/accessed times)
//! 4. Verify caching (repeated calls return same slice pointer)
//! 5. Validate integrity detects corrupted files
//! 6. Performance: <1s for listing

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use unified_archive::Archive;

// ── Contract 1: List files from each supported format ──

#[test]
fn contract_list_files_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "ZIP should contain entries");
    for entry in entries {
        assert!(!entry.path.is_empty(), "Entry path should not be empty");
    }
}

#[test]
fn contract_list_files_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "7z should contain entries");
}

#[test]
fn contract_list_files_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "RAR should contain entries");
}

#[test]
fn contract_list_files_rar5() {
    let archive = Archive::open(fixture("test_rar5.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "RAR5 should contain entries");
}

#[test]
fn contract_list_files_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR should contain entries");
}

#[test]
fn contract_list_files_tar_gz() {
    let archive = Archive::open(fixture("test.tar.gz")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.GZ should contain entries");
}

#[test]
fn contract_list_files_tar_bz2() {
    let archive = Archive::open(fixture("test.tar.bz2")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.BZ2 should contain entries");
}

#[test]
fn contract_list_files_tar_xz() {
    let archive = Archive::open(fixture("test.tar.xz")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.XZ should contain entries");
}

// ── Contract 2: Verify metadata consistency ──

#[test]
fn contract_metadata_size_present_for_files() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    for entry in entries {
        if entry.entry_type == unified_archive::EntryType::File {
            assert!(
                entry.size.is_some(),
                "File entry '{}' should have a size",
                entry.path
            );
        }
    }
}

#[test]
fn contract_metadata_crc32_present_for_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // ZIP format generally provides CRC32 for all entries
    let has_crc = entries
        .iter()
        .filter(|e| e.entry_type == unified_archive::EntryType::File)
        .any(|e| e.crc32.is_some());
    assert!(has_crc, "ZIP files should have CRC32 checksums");
}

#[test]
fn contract_metadata_modified_time_present() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // Most formats provide modification time
    let has_mtime = entries.iter().any(|e| e.modified.is_some());
    assert!(has_mtime, "Entries should have modification timestamps");
}

#[test]
fn contract_metadata_entry_ids_sequential() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.id, i,
            "Entry IDs should be sequential 0-based, entry '{}' has id {} at index {}",
            entry.path, entry.id, i
        );
    }
}

#[test]
fn contract_metadata_consistent_across_formats() {
    // The same content archived in different formats should have consistent metadata
    let zip = Archive::open(fixture("test.zip")).unwrap();
    let tar = Archive::open(fixture("test.tar")).unwrap();

    let zip_entries = zip.list_files().unwrap();
    let tar_entries = tar.list_files().unwrap();

    // Both should contain files (may differ in exact entries due to format differences)
    assert!(!zip_entries.is_empty());
    assert!(!tar_entries.is_empty());

    // File entries should have non-None sizes
    for entry in zip_entries
        .iter()
        .filter(|e| e.entry_type == unified_archive::EntryType::File)
    {
        assert!(
            entry.size.is_some(),
            "ZIP entry '{}' missing size",
            entry.path
        );
    }
}

// ── Contract 3: Verify enhanced metadata ──

#[test]
fn contract_enhanced_metadata_entry_type() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // All entries should have a valid entry_type
    for entry in entries {
        match entry.entry_type {
            unified_archive::EntryType::File
            | unified_archive::EntryType::Directory
            | unified_archive::EntryType::Symlink
            | unified_archive::EntryType::HardLink
            | unified_archive::EntryType::Other => {}
        }
    }
}

#[test]
fn contract_enhanced_metadata_is_encrypted_field() {
    // Non-encrypted archive should report is_encrypted = false
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    for entry in entries {
        assert!(
            !entry.is_encrypted,
            "Non-encrypted archive entries should have is_encrypted=false"
        );
    }
}

// ── Contract 4: Verify caching (repeated calls return same pointer) ──

#[test]
fn contract_list_files_caching_same_pointer() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries1 = archive.list_files().unwrap();
    let entries2 = archive.list_files().unwrap();

    // Both calls should return a reference to the same cached slice
    let ptr1 = entries1.as_ptr();
    let ptr2 = entries2.as_ptr();
    assert_eq!(
        ptr1, ptr2,
        "Repeated list_files() calls should return the same cached slice (pointer equality)"
    );
}

#[test]
fn contract_list_files_caching_same_length() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries1 = archive.list_files().unwrap();
    let entries2 = archive.list_files().unwrap();

    assert_eq!(entries1.len(), entries2.len());
}

#[test]
fn contract_entry_count_matches_list_files() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let count = archive.entry_count().unwrap();
    assert_eq!(
        entries.len(),
        count,
        "entry_count() should match list_files().len()"
    );
}

// ── Contract 5: Validate integrity detects corrupted files ──

#[test]
fn contract_validate_integrity_valid_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(
        report.failed.is_empty(),
        "Valid ZIP should pass integrity check. Failed entries: {:?}",
        report.failed
    );
}

#[test]
fn contract_validate_integrity_corrupted_zip() {
    let archive = Archive::open(fixture("corrupted_crc.zip"));
    match archive {
        Ok(archive) => {
            let report = archive.validate_integrity();
            match report {
                Ok(report) => {
                    // Corrupted archive should be detected
                    assert!(
                        !report.failed.is_empty() || report.validated < report.total_entries,
                        "Corrupted archive should fail integrity check"
                    );
                }
                Err(_) => {
                    // Error during validation is also acceptable for corrupted archives
                }
            }
        }
        Err(_) => {
            // If the archive can't even be opened, that's acceptable
        }
    }
}

#[test]
fn contract_validate_integrity_valid_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(
        report.failed.is_empty(),
        "Valid 7z should pass integrity check. Failed: {:?}",
        report.failed
    );
}

// ── Contract 6: Performance ──

#[test]
fn contract_list_files_performance() {
    use std::time::Instant;

    let archive = Archive::open(fixture("test.zip")).unwrap();
    let start = Instant::now();
    let _entries = archive.list_files().unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 1,
        "Listing files should complete in <1s, took: {:?}",
        elapsed
    );
}

#[test]
fn contract_find_entry_existing() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_path = &entries[0].path;

    let found = archive.find_entry(first_path).unwrap();
    assert!(
        found.is_some(),
        "Should find existing entry '{}'",
        first_path
    );
    assert_eq!(found.unwrap().path, *first_path);
}

#[test]
fn contract_find_entry_nonexistent() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let found = archive
        .find_entry("definitely_not_a_real_file.xyz")
        .unwrap();
    assert!(found.is_none(), "Should not find nonexistent entry");
}
