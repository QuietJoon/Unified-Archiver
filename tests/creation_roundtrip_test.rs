//! Roundtrip integration tests for archive creation
//!
//! These tests create archives and then extract them to verify correctness.

use std::fs;
use unified_archive::{Archive, ArchiveFormat, CompressionLevel, CompressionOptions};

#[path = "common/mod.rs"]
mod common;

// Note: ZIP creation now uses the native Rust `zip` crate instead of libarchive,
// fixing the known central directory issues. The piz crate handles extraction,
// providing reliable ZIP support for both creation and extraction.
#[test]
fn test_create_and_extract_zip() {
    let temp = common::temp_test_dir();

    // Create ZIP archive
    let archive_path = temp.join("test.zip");
    let mut options = CompressionOptions::new(ArchiveFormat::Zip);
    options.level = CompressionLevel::Normal;

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("file1.txt", b"Hello, World!")
        .unwrap();
    creator
        .add_file_from_data("file2.txt", b"Archive test content")
        .unwrap();
    creator.finish().unwrap();

    assert!(archive_path.exists(), "Archive file should be created");

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(common::default_extraction_options(extract_dir.clone()))
        .unwrap();

    // Verify extracted files
    assert!(extract_dir.join("file1.txt").exists());
    assert!(extract_dir.join("file2.txt").exists());

    // Verify content
    let content1 = fs::read_to_string(extract_dir.join("file1.txt")).unwrap();
    assert_eq!(content1, "Hello, World!");

    common::cleanup(&temp);
}

#[test]
fn test_create_and_extract_targz() {
    let temp = common::temp_test_dir();

    // Create TAR.GZ archive
    let archive_path = temp.join("test.tar.gz");
    let mut options = CompressionOptions::new(ArchiveFormat::TarGzip);
    options.level = CompressionLevel::Fast;

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("readme.md", b"# Test Archive")
        .unwrap();
    creator.finish().unwrap();

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(common::default_extraction_options(extract_dir.clone()))
        .unwrap();

    // Verify files
    assert!(extract_dir.join("readme.md").exists());
    let readme = fs::read_to_string(extract_dir.join("readme.md")).unwrap();
    assert_eq!(readme, "# Test Archive");

    common::cleanup(&temp);
}

#[test]
fn test_create_and_extract_7z() {
    let temp = common::temp_test_dir();

    // Create 7z archive
    let archive_path = temp.join("test.7z");
    let options = common::default_compression_options(ArchiveFormat::SevenZip);

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("test.txt", b"Seven-Zip test")
        .unwrap();
    creator.finish().unwrap();

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(common::default_extraction_options(extract_dir.clone()))
        .unwrap();

    // Verify files
    assert!(extract_dir.join("test.txt").exists());
    let content = fs::read_to_string(extract_dir.join("test.txt")).unwrap();
    assert_eq!(content, "Seven-Zip test");

    common::cleanup(&temp);
}
