//! Format compatibility integration tests
//!
//! Verifies that the unified API works consistently across different archive formats

use unified_archive::{Archive, ArchiveFormat, EntryType, ValidationReport};

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_format_detection() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    assert_eq!(archive.format(), ArchiveFormat::Rar5);
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_list_files() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].entry_type, EntryType::File);
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_entry_count() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let count = archive.entry_count().expect("Failed to get entry count");

    assert_eq!(count, 1);
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_find_entry() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // Find existing entry
    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");

    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));

    // Try to find non-existent entry
    let not_found = archive
        .find_entry("nonexistent.txt")
        .expect("Failed to search");

    assert!(not_found.is_none());
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_validate_integrity() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    assert_eq!(report.total_entries, 1);
    assert_eq!(report.validated, 1);
    assert!(report.failed.is_empty());
}

#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_metadata_consistency() {
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);

    let entry = &entries[0];

    // Verify all metadata fields are consistent
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.entry_type, EntryType::File);
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));
    assert!(entry.compressed_size.is_some());
    assert!(entry.modified.is_some());
}

#[test]
fn test_unsupported_format_error() {
    // Try to open a non-archive file
    let result = Archive::open("Cargo.toml");

    assert!(result.is_err());
    // The error should indicate unknown/unsupported format
}

#[test]
fn test_nonexistent_file_error() {
    let result = Archive::open("tests/fixtures/nonexistent.rar");

    assert!(result.is_err());
    // The error should be an I/O error
}

/// Test that ValidationReport has the correct structure
#[test]
fn test_validation_report_structure() {
    let report = ValidationReport {
        total_entries: 10,
        validated: 8,
        failed: vec!["file1.txt".to_string(), "file2.txt".to_string()],
    };

    assert_eq!(report.total_entries, 10);
    assert_eq!(report.validated, 8);
    assert_eq!(report.failed.len(), 2);
    assert_eq!(report.failed[0], "file1.txt");
}

/// Test that the same API works for both RAR and RAR5
#[test]
#[serial_test::file_serial(rar)]
fn test_api_consistency_rar_and_rar5() {
    // Note: Due to UnRAR's sequential iteration, we need to reopen archives
    // between operations for reliable results

    // Test list_files consistency
    {
        let rar_archive =
            Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
        let rar5_archive =
            Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

        let rar_entries = rar_archive.list_files().expect("Failed to list RAR files");
        let rar5_entries = rar5_archive
            .list_files()
            .expect("Failed to list RAR5 files");

        // Both should have the same file
        assert_eq!(rar_entries.len(), rar5_entries.len());
        assert_eq!(rar_entries[0].path, rar5_entries[0].path);
        assert_eq!(rar_entries[0].size, rar5_entries[0].size);
        assert_eq!(rar_entries[0].crc32, rar5_entries[0].crc32);
    }

    // Test entry_count consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        assert_eq!(
            rar_archive.entry_count().unwrap(),
            rar5_archive.entry_count().unwrap()
        );
    }

    // Test find_entry consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        let rar_found = rar_archive.find_entry("test_file.txt").unwrap();
        let rar5_found = rar5_archive.find_entry("test_file.txt").unwrap();
        assert!(rar_found.is_some());
        assert!(rar5_found.is_some());
    }

    // Test validation consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        let rar_report = rar_archive.validate_integrity().unwrap();
        let rar5_report = rar5_archive.validate_integrity().unwrap();
        assert_eq!(rar_report.validated, rar5_report.validated);
    }
}

// ── TAR format tests ──

#[test]
fn test_tar_format_detection() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    assert_eq!(archive.format(), ArchiveFormat::Tar);
}

#[test]
fn test_tar_list_files() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let entries = archive.list_files().expect("Failed to list files");

    assert!(!entries.is_empty());
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[test]
fn test_tar_entry_count() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let count = archive.entry_count().expect("Failed to get entry count");
    assert!(count >= 1);
}

#[test]
fn test_tar_find_entry() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
}

#[test]
fn test_tar_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.GZ format tests ──

#[test]
fn test_tar_gz_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

#[test]
fn test_tar_gz_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[test]
fn test_tar_gz_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.BZ2 format tests ──

#[test]
fn test_tar_bz2_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
}

#[test]
fn test_tar_bz2_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[test]
fn test_tar_bz2_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.XZ format tests ──

#[test]
fn test_tar_xz_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    assert_eq!(archive.format(), ArchiveFormat::TarXz);
}

#[test]
fn test_tar_xz_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[test]
fn test_tar_xz_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── GZIP format tests ──
// Note: Plain .gz files (not .tar.gz) require archive_read_support_format_raw()
// which is not yet bound. libarchive treats these as compression filters, not
// archive formats. These tests document the expected behavior once format_raw
// support is added.

#[test]
#[ignore = "Plain GZIP requires format_raw support in libarchive bindings"]
fn test_gzip_format_detection() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    assert_eq!(archive.format(), ArchiveFormat::Gzip);
}

#[test]
#[ignore = "Plain GZIP requires format_raw support in libarchive bindings"]
fn test_gzip_list_files() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    let entries = archive.list_files().expect("Failed to list files");
    // GZIP wraps a single file; libarchive may report it as one entry
    assert!(!entries.is_empty());
}

#[test]
#[ignore = "Plain GZIP requires format_raw support in libarchive bindings"]
fn test_gzip_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract GZIP to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── BZIP2 format tests ──

#[test]
#[ignore = "Plain BZIP2 requires format_raw support in libarchive bindings"]
fn test_bzip2_format_detection() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    assert_eq!(archive.format(), ArchiveFormat::Bzip2);
}

#[test]
#[ignore = "Plain BZIP2 requires format_raw support in libarchive bindings"]
fn test_bzip2_list_files() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
}

#[test]
#[ignore = "Plain BZIP2 requires format_raw support in libarchive bindings"]
fn test_bzip2_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract BZIP2 to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── XZ format tests ──

#[test]
#[ignore = "Plain XZ requires format_raw support in libarchive bindings"]
fn test_xz_format_detection() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    assert_eq!(archive.format(), ArchiveFormat::Xz);
}

#[test]
#[ignore = "Plain XZ requires format_raw support in libarchive bindings"]
fn test_xz_list_files() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
}

#[test]
#[ignore = "Plain XZ requires format_raw support in libarchive bindings"]
fn test_xz_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract XZ to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── Cross-format consistency tests ──

/// All libarchive-backed formats with test_file.txt should produce
/// identical extracted content
#[test]
fn test_cross_format_content_consistency() {
    let tar_formats = vec![
        ("tests/fixtures/test.tar", "TAR"),
        ("tests/fixtures/test.tar.gz", "TAR.GZ"),
        ("tests/fixtures/test.tar.bz2", "TAR.BZ2"),
        ("tests/fixtures/test.tar.xz", "TAR.XZ"),
    ];

    let expected = b"Hello, RAR World!\n";

    for (path, label) in &tar_formats {
        let archive =
            Archive::open(path).unwrap_or_else(|e| panic!("Failed to open {}: {}", label, e));
        let data = archive
            .extract_to_memory("test_file.txt")
            .unwrap_or_else(|e| panic!("Failed to extract from {}: {}", label, e));
        assert_eq!(data, expected, "Content mismatch for {} format", label);
    }
}

/// All single-stream formats should produce identical decompressed content
/// Currently ignored: plain .gz/.bz2/.xz require archive_read_support_format_raw()
#[test]
#[ignore = "Plain compressed streams require format_raw support in libarchive bindings"]
fn test_single_stream_content_consistency() {
    let stream_formats = vec![
        "tests/fixtures/test.gz",
        "tests/fixtures/test.bz2",
        "tests/fixtures/test.xz",
    ];

    let expected = b"Hello, RAR World!\n";

    let mut results = Vec::new();
    for path in &stream_formats {
        let archive =
            Archive::open(path).unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));
        let entries = archive
            .list_files()
            .unwrap_or_else(|e| panic!("Failed to list {}: {}", path, e));
        assert!(!entries.is_empty(), "No entries for {}", path);
        let data = archive
            .extract_to_memory(&entries[0].path)
            .unwrap_or_else(|e| panic!("Failed to extract {}: {}", path, e));
        assert_eq!(data, expected, "Content mismatch for {}", path);
        results.push(data);
    }

    // All should be identical
    for (i, data) in results.iter().enumerate().skip(1) {
        assert_eq!(data, &results[0], "Stream format {} differs from first", i);
    }
}
