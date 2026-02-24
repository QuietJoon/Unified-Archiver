//! Integration test for unified Archive API
//!
//! Tests the high-level Archive interface with RAR/RAR5 backend

use unified_archive::{Archive, ArchiveFormat, EntryType};

#[test]
fn test_unified_api_open_rar4() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // Note: Modern RAR (7.12+) creates RAR5 format by default
    // This test validates format detection, not RAR4 specifically
    assert_eq!(archive.format(), ArchiveFormat::Rar5);

    // Verify path
    assert!(archive.path().to_string_lossy().ends_with("test.rar"));
}

#[test]
fn test_unified_api_open_rar5() {
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    // Verify format detection
    assert_eq!(archive.format(), ArchiveFormat::Rar5);
}

#[test]
fn test_unified_api_list_files_rar4() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);

    let entry = &entries[0];
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.entry_type, EntryType::File);
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));
}

#[test]
fn test_unified_api_list_files_rar5() {
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);

    let entry = &entries[0];
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.entry_type, EntryType::File);
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));
}

#[test]
fn test_unified_api_entry_count() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let count = archive.entry_count().expect("Failed to get entry count");

    assert_eq!(count, 1);
}

#[test]
fn test_unified_api_unsupported_format() {
    // Try to open a non-existent ZIP file (should fail with unsupported format)
    let result = Archive::open("tests/fixtures/nonexistent.zip");

    // Should fail because either file doesn't exist or format is not supported yet
    assert!(result.is_err());
}
