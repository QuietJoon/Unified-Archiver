//! Archive Modification Integration Tests
//!
//! Tests for archive modification functionality (add, remove, replace)

use unified_archive::{Archive, ArchiveFormat, CompressionLevel, CompressionOptions};

#[test]
#[ignore = "ZIP modification via libarchive has known issues - use 7z/tar for modification"]
fn test_add_files_to_archive() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let test_path = tmp_dir.path().join("test_modify_add.zip");

    // Create initial archive
    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("original.txt", b"original content")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("new_file.txt", b"new content").unwrap();
    archive.commit_changes().unwrap();

    // Verify
    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.path == "original.txt"));
    assert!(entries.iter().any(|e| e.path == "new_file.txt"));
    // tmp_dir cleanup is automatic on drop
}

#[test]
#[ignore = "ZIP modification via libarchive has known issues - use 7z/tar for modification"]
fn test_remove_files_from_archive() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let test_path = tmp_dir.path().join("test_modify_remove.zip");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("file1.txt", b"content1")
        .unwrap();
    archive
        .add_file_from_data("file2.txt", b"content2")
        .unwrap();
    archive
        .add_file_from_data("file3.txt", b"content3")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("file2.txt").unwrap();
    archive.commit_changes().unwrap();

    // Verify
    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.path == "file1.txt"));
    assert!(entries.iter().any(|e| e.path == "file3.txt"));
    assert!(!entries.iter().any(|e| e.path == "file2.txt"));
}

#[test]
#[ignore = "ZIP modification via libarchive has known issues - use 7z/tar for modification"]
fn test_replace_files_in_archive() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let test_path = tmp_dir.path().join("test_modify_replace.zip");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("config.txt", b"old config")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.replace_entry("config.txt", b"new config").unwrap();
    archive.commit_changes().unwrap();

    // Verify
    let archive = Archive::open(&test_path).unwrap();
    let data = archive.extract_to_memory("config.txt").unwrap();
    assert_eq!(data, b"new config");
}

#[test]
#[ignore = "ZIP modification via libarchive has known issues - use 7z/tar for modification"]
fn test_combined_operations() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let test_path = tmp_dir.path().join("test_modify_combined.zip");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("keep.txt", b"keep this")
        .unwrap();
    archive
        .add_file_from_data("remove.txt", b"remove this")
        .unwrap();
    archive
        .add_file_from_data("replace.txt", b"old content")
        .unwrap();
    archive.finish().unwrap();

    // Now perform multiple operations
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("remove.txt").unwrap();
    archive
        .replace_entry("replace.txt", b"new content")
        .unwrap();
    archive.add_entry("added.txt", b"added content").unwrap();
    archive.commit_changes().unwrap();

    // Verify all operations
    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().any(|e| e.path == "keep.txt"));
    assert!(entries.iter().any(|e| e.path == "replace.txt"));
    assert!(entries.iter().any(|e| e.path == "added.txt"));
    assert!(!entries.iter().any(|e| e.path == "remove.txt"));

    // Verify replaced content
    let data = archive.extract_to_memory("replace.txt").unwrap();
    assert_eq!(data, b"new content");
}

#[test]
fn test_open_nonexistent_archive() {
    let result = Archive::modify("/nonexistent/path/to/archive.zip");
    assert!(result.is_err());
}
