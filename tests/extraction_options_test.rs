//! Tests for extraction options behavior

mod common;

use std::fs;
use unified_archive::{Archive, ExtractionLimits, ExtractionOptions};

#[test]
fn test_extract_file_overwrite_false_blocks_existing_zip() {
    let temp = common::temp_test_dir();
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let target = temp.join("test_file.txt");
    fs::write(&target, b"existing").expect("Failed to seed target file");

    let options = ExtractionOptions {
        destination: temp.clone(),
        overwrite: false,
        ..Default::default()
    };

    let result = archive.extract_file("test_file.txt", options);
    assert!(result.is_err(), "Expected overwrite=false to fail");

    common::cleanup(&temp);
}

#[test]
fn test_extract_file_respects_limits_zip() {
    let temp = common::temp_test_dir();
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let limits = ExtractionLimits {
        max_file_size: 1,
        ..Default::default()
    };

    let options = ExtractionOptions {
        destination: temp.clone(),
        limits,
        ..Default::default()
    };

    let result = archive.extract_file("test_file.txt", options);
    assert!(result.is_err(), "Expected file size limit to fail");
    assert!(
        !temp.join("test_file.txt").exists(),
        "File should not be created"
    );

    common::cleanup(&temp);
}

#[test]
fn test_extract_files_respects_limits_zip() {
    let temp = common::temp_test_dir();
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let limits = ExtractionLimits {
        max_file_size: 1,
        ..Default::default()
    };

    let options = ExtractionOptions {
        destination: temp.clone(),
        limits,
        ..Default::default()
    };

    let result = archive.extract_files(&["test_file.txt"], options);
    assert!(result.is_err(), "Expected file size limit to fail");
    assert!(
        !temp.join("test_file.txt").exists(),
        "File should not be created"
    );

    common::cleanup(&temp);
}
