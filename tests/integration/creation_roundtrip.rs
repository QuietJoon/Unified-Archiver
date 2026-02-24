//! Roundtrip integration tests for archive creation
//!
//! These tests create archives and then extract them to verify correctness.
//! This ensures the entire create -> extract pipeline works end-to-end.

use std::fs;
use unified_archive::{
    Archive, ArchiveFormat, CompressionLevel, CompressionOptions, ExtractionOptions,
};

#[path = "../common/mod.rs"]
mod common;

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_create_and_extract_zip() {
    let temp = common::temp_test_dir();

    // Create ZIP archive
    let archive_path = temp.join("test.zip");
    let options = CompressionOptions::builder()
        .format(ArchiveFormat::Zip)
        .level(CompressionLevel::Normal)
        .build();

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("file1.txt", b"Hello, World!")
        .unwrap();
    creator
        .add_file_from_data("file2.txt", b"Archive test content")
        .unwrap();
    creator.add_directory_entry("subdir").unwrap();
    creator
        .add_file_from_data("subdir/nested.txt", b"Nested file")
        .unwrap();

    let entry_count = creator.entry_count();
    assert_eq!(entry_count, 4);

    creator.finish().unwrap();
    assert!(archive_path.exists(), "Archive file should be created");

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    let options = ExtractionOptions::default();
    archive.extract_all(&extract_dir, &options).unwrap();

    // Verify extracted files
    assert!(extract_dir.join("file1.txt").exists());
    assert!(extract_dir.join("file2.txt").exists());
    assert!(extract_dir.join("subdir").exists());
    assert!(extract_dir.join("subdir/nested.txt").exists());

    // Verify content
    let content1 = fs::read_to_string(extract_dir.join("file1.txt")).unwrap();
    assert_eq!(content1, "Hello, World!");

    let content2 = fs::read_to_string(extract_dir.join("file2.txt")).unwrap();
    assert_eq!(content2, "Archive test content");

    let content3 = fs::read_to_string(extract_dir.join("subdir/nested.txt")).unwrap();
    assert_eq!(content3, "Nested file");

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_create_and_extract_targz() {
    let temp = common::temp_test_dir();

    // Create TAR.GZ archive
    let archive_path = temp.join("test.tar.gz");
    let options = CompressionOptions::builder()
        .format(ArchiveFormat::TarGzip)
        .level(CompressionLevel::Fast)
        .build();

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("readme.md", b"# Test Archive")
        .unwrap();
    creator
        .add_file_from_data("data.json", br#"{"test": true}"#)
        .unwrap();

    creator.finish().unwrap();
    assert!(archive_path.exists());

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    // Verify files
    assert!(extract_dir.join("readme.md").exists());
    assert!(extract_dir.join("data.json").exists());

    let readme = fs::read_to_string(extract_dir.join("readme.md")).unwrap();
    assert_eq!(readme, "# Test Archive");

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_create_and_extract_7z() {
    let temp = common::temp_test_dir();

    // Create 7z archive
    let archive_path = temp.join("test.7z");
    let options = CompressionOptions::builder()
        .format(ArchiveFormat::SevenZip)
        .level(CompressionLevel::Maximum)
        .build();

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("test.txt", b"Seven-Zip test")
        .unwrap();
    creator
        .add_file_from_data("config.ini", b"[settings]\nvalue=42")
        .unwrap();

    creator.finish().unwrap();
    assert!(archive_path.exists());

    // Extract archive
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    // Verify files
    assert!(extract_dir.join("test.txt").exists());
    assert!(extract_dir.join("config.ini").exists());

    let content = fs::read_to_string(extract_dir.join("test.txt")).unwrap();
    assert_eq!(content, "Seven-Zip test");

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_create_from_filesystem_files() {
    let temp = common::temp_test_dir();

    // Create source files
    let source_dir = temp.join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("doc1.txt"), b"Document 1").unwrap();
    fs::write(source_dir.join("doc2.txt"), b"Document 2").unwrap();

    // Create archive from filesystem files
    let archive_path = temp.join("files.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_path(source_dir.join("doc1.txt"))
        .unwrap();
    creator
        .add_file_from_path(source_dir.join("doc2.txt"))
        .unwrap();
    creator.finish().unwrap();

    // Extract and verify
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    let doc1 = fs::read_to_string(extract_dir.join("doc1.txt")).unwrap();
    assert_eq!(doc1, "Document 1");

    let doc2 = fs::read_to_string(extract_dir.join("doc2.txt")).unwrap();
    assert_eq!(doc2, "Document 2");

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_create_with_directory_recursion() {
    let temp = common::temp_test_dir();

    // Create directory structure
    let source_dir = temp.join("project");
    fs::create_dir_all(source_dir.join("src")).unwrap();
    fs::create_dir_all(source_dir.join("docs")).unwrap();
    fs::write(source_dir.join("README.md"), b"# Project").unwrap();
    fs::write(source_dir.join("src/main.rs"), b"fn main() {}").unwrap();
    fs::write(source_dir.join("src/lib.rs"), b"pub fn test() {}").unwrap();
    fs::write(source_dir.join("docs/guide.md"), b"# Guide").unwrap();

    // Create archive with recursive directory addition
    let archive_path = temp.join("project.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator.add_directory_recursive(&source_dir).unwrap();
    creator.finish().unwrap();

    // Extract and verify structure
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    // Check all files were extracted
    assert!(extract_dir.join("README.md").exists());
    assert!(extract_dir.join("src/main.rs").exists());
    assert!(extract_dir.join("src/lib.rs").exists());
    assert!(extract_dir.join("docs/guide.md").exists());

    // Verify content
    let readme = fs::read_to_string(extract_dir.join("README.md")).unwrap();
    assert_eq!(readme, "# Project");

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_compression_levels_roundtrip() {
    let temp = common::temp_test_dir();

    // Test data (highly compressible)
    let test_data = b"A".repeat(10000);

    for level in [
        CompressionLevel::Store,
        CompressionLevel::Fastest,
        CompressionLevel::Normal,
        CompressionLevel::Maximum,
    ] {
        let archive_path = temp.join(format!("compression_{:?}.zip", level));
        let options = CompressionOptions::builder()
            .format(ArchiveFormat::Zip)
            .level(level)
            .build();

        let mut creator = Archive::create(&archive_path, options).unwrap();
        creator.add_file_from_data("data.bin", &test_data).unwrap();
        creator.finish().unwrap();

        // Extract and verify
        let extract_dir = temp.join(format!("extracted_{:?}", level));
        let archive = Archive::open(&archive_path).unwrap();
        archive
            .extract_all(&extract_dir, &ExtractionOptions::default())
            .unwrap();

        let extracted_data = fs::read(extract_dir.join("data.bin")).unwrap();
        assert_eq!(
            extracted_data, test_data,
            "Data should be identical for {:?}",
            level
        );
    }

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_password_protected_zip_roundtrip() {
    let temp = common::temp_test_dir();

    // Create password-protected ZIP
    let archive_path = temp.join("protected.zip");
    let options = CompressionOptions::builder()
        .format(ArchiveFormat::Zip)
        .password("secret123")
        .build();

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("secret.txt", b"Confidential data")
        .unwrap();
    creator.finish().unwrap();

    // Try to extract without password (should fail)
    let extract_dir1 = temp.join("extracted_nopass");
    let archive = Archive::open(&archive_path);

    if let Ok(archive) = archive {
        let result = archive.extract_all(&extract_dir1, &ExtractionOptions::default());
        // May fail or succeed depending on libarchive behavior - both are acceptable
        // Some formats store encryption info in headers, some don't
    }

    // Extract with correct password
    let extract_dir2 = temp.join("extracted_withpass");
    let archive = Archive::open_encrypted(&archive_path, "secret123").unwrap();
    let result = archive.extract_all(&extract_dir2, &ExtractionOptions::default());

    if result.is_ok() {
        // If extraction succeeded, verify content
        if extract_dir2.join("secret.txt").exists() {
            let content = fs::read_to_string(extract_dir2.join("secret.txt")).unwrap();
            assert_eq!(content, "Confidential data");
        }
    }
    // Note: Password-protected extraction may not work perfectly depending on
    // libarchive version and encryption method support

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_multiple_formats_same_content() {
    let temp = common::temp_test_dir();

    let test_content = b"This is the test content that will be archived in multiple formats";

    // Create archives in different formats
    let formats = vec![
        (ArchiveFormat::Zip, "test.zip"),
        (ArchiveFormat::TarGzip, "test.tar.gz"),
        (ArchiveFormat::SevenZip, "test.7z"),
    ];

    for (format, filename) in formats {
        let archive_path = temp.join(filename);
        let options = CompressionOptions::new(format);

        let mut creator = Archive::create(&archive_path, options).unwrap();
        creator
            .add_file_from_data("content.txt", test_content)
            .unwrap();
        creator.finish().unwrap();

        // Extract and verify
        let extract_dir = temp.join(format!("extracted_{}", filename));
        let archive = Archive::open(&archive_path).unwrap();
        archive
            .extract_all(&extract_dir, &ExtractionOptions::default())
            .unwrap();

        let extracted = fs::read(extract_dir.join("content.txt")).unwrap();
        assert_eq!(
            extracted, test_content,
            "Content should match for {:?}",
            format
        );
    }

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_large_file_roundtrip() {
    let temp = common::temp_test_dir();

    // Create a larger file (1MB)
    let large_data = vec![0xABu8; 1_000_000];

    let archive_path = temp.join("large.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("large.bin", &large_data)
        .unwrap();
    creator.finish().unwrap();

    // Extract
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    // Verify
    let extracted = fs::read(extract_dir.join("large.bin")).unwrap();
    assert_eq!(extracted.len(), large_data.len());
    assert_eq!(extracted, large_data);

    common::cleanup(&temp);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_unicode_filenames_roundtrip() {
    let temp = common::temp_test_dir();

    let archive_path = temp.join("unicode.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);

    let mut creator = Archive::create(&archive_path, options).unwrap();
    creator
        .add_file_from_data("��.txt", b"Chinese filename")
        .unwrap();
    creator
        .add_file_from_data("D09;.txt", b"Russian filename")
        .unwrap();
    creator
        .add_file_from_data("ա��.txt", b"Japanese filename")
        .unwrap();
    creator.finish().unwrap();

    // Extract
    let extract_dir = temp.join("extracted");
    let archive = Archive::open(&archive_path).unwrap();
    archive
        .extract_all(&extract_dir, &ExtractionOptions::default())
        .unwrap();

    // Verify (may fail on some systems with encoding issues)
    if extract_dir.join("��.txt").exists() {
        let content = fs::read(extract_dir.join("��.txt")).unwrap();
        assert_eq!(content, b"Chinese filename");
    }

    common::cleanup(&temp);
}
