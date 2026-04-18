//! Integration tests for UnRAR CRC32 extraction
//!
//! Validates that CRC32 checksums are correctly extracted from RAR and RAR5 archives

#![cfg(feature = "rar-support")]

use unified_archive::ffi::wrapper::UnrarArchive;

#[test]
#[serial_test::file_serial(rar)]
fn test_rar4_crc32_extraction() {
    let archive =
        UnrarArchive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1, "Expected 1 file in archive");

    let entry = &entries[0];
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18), "File size should be 18 bytes");
    assert_eq!(entry.crc32, Some(0x054607BC), "CRC32 mismatch!");
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_crc32_extraction() {
    let archive =
        UnrarArchive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1, "Expected 1 file in archive");

    let entry = &entries[0];
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18), "File size should be 18 bytes");
    assert_eq!(entry.crc32, Some(0x054607BC), "CRC32 mismatch for RAR5!");
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar_vs_rar5_same_crc32() {
    let rar4 = UnrarArchive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
    let rar5 =
        UnrarArchive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let rar4_entries = rar4.list_files().unwrap();
    let rar5_entries = rar5.list_files().unwrap();

    assert_eq!(
        rar4_entries[0].crc32, rar5_entries[0].crc32,
        "RAR and RAR5 should produce identical CRC32 for same file"
    );
}

#[test]
#[serial_test::file_serial(rar)]
fn test_metadata_completeness() {
    let archive = UnrarArchive::open("tests/fixtures/test.rar").unwrap();
    let entries = archive.list_files().unwrap();
    let entry = &entries[0];

    // Verify all required metadata is present
    assert!(entry.size.is_some(), "Size should be present");
    assert!(
        entry.compressed_size.is_some(),
        "Compressed size should be present"
    );
    assert!(entry.crc32.is_some(), "CRC32 should be present");
    assert!(
        entry.modified.is_some(),
        "Modification time should be present"
    );

    println!("Entry metadata:");
    println!("  Path: {}", entry.path);
    println!("  Size: {} bytes", entry.size.unwrap());
    println!("  Compressed: {} bytes", entry.compressed_size.unwrap());
    println!("  CRC32: 0x{:08X}", entry.crc32.unwrap());
    println!("  Modified: {:?}", entry.modified.unwrap());
}
