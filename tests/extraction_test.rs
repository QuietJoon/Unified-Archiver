//! Extraction integration tests
//!
//! Tests the extraction API for RAR/RAR5 archives

mod common;

use std::fs;
use unified_archive::{Archive, ExtractionOptions};

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_all_rar5() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_all(options)
        .expect("Failed to extract archive");

    // Verify extracted file exists
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists(), "Extracted file should exist");

    // Verify file content
    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    common::cleanup(&temp);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_single_file_rar5() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_file("test_file.txt", options)
        .expect("Failed to extract file");

    // Verify extracted file exists
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists(), "Extracted file should exist");

    // Verify file content
    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    common::cleanup(&temp);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_nonexistent_file() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    let result = archive.extract_file("nonexistent.txt", options);

    assert!(
        result.is_err(),
        "Should fail when extracting nonexistent file"
    );

    common::cleanup(&temp);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_to_nonexistent_directory() {
    let temp = common::temp_test_dir().join("nested/path/that/does/not/exist");

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    // Should create directory and extract
    archive
        .extract_all(options)
        .expect("Failed to extract to new directory");

    // Verify file was extracted
    let extracted_file = temp.join("test_file.txt");
    assert!(
        extracted_file.exists(),
        "Extracted file should exist in nested path"
    );

    // Cleanup parent temp directory
    common::cleanup(&common::temp_test_dir());
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_rar5_alternate() {
    let temp = common::temp_test_dir();

    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_all(options)
        .expect("Failed to extract RAR5 archive");

    // Verify extracted file
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists());

    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    common::cleanup(&temp);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let content = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");

    // Verify content
    let content_str = String::from_utf8(content).expect("Failed to convert to UTF-8");
    assert_eq!(content_str, "Hello, RAR World!\n");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_to_memory_nonexistent() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let result = archive.extract_to_memory("nonexistent.txt");

    assert!(
        result.is_err(),
        "Should fail when extracting nonexistent file to memory"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_filtered_txt_files() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    // Extract only .txt files
    archive
        .extract_filtered(|entry| entry.path.ends_with(".txt"), options)
        .expect("Failed to extract filtered files");

    // Verify txt file was extracted
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists(), "TXT file should be extracted");

    common::cleanup(&temp);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_filtered_no_matches() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    // Try to extract .pdf files (none exist)
    archive
        .extract_filtered(|entry| entry.path.ends_with(".pdf"), options)
        .expect("Should succeed even with no matches");

    // Verify no files were extracted
    let entries = fs::read_dir(&temp).expect("Failed to read temp dir");
    assert_eq!(entries.count(), 0, "No files should be extracted");

    common::cleanup(&temp);
}
