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
        overwrite: false,
        ..common::default_extraction_options(temp.clone())
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
        limits,
        ..common::default_extraction_options(temp.clone())
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
        limits,
        ..common::default_extraction_options(temp.clone())
    };

    let result = archive.extract_files(&["test_file.txt"], options);
    assert!(result.is_err(), "Expected file size limit to fail");
    assert!(
        !temp.join("test_file.txt").exists(),
        "File should not be created"
    );

    common::cleanup(&temp);
}

#[test]
fn test_extract_to_memory_with_options_rejects_tight_limit() {
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let options = ExtractionOptions {
        limits: ExtractionLimits {
            max_file_size: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = archive.extract_to_memory_with_options("test_file.txt", &options);
    assert!(
        result.is_err(),
        "Expected max_file_size=1 to reject 18-byte entry"
    );
}

#[test]
fn test_extract_to_memory_with_options_accepts_loose_limit() {
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let options = ExtractionOptions {
        limits: ExtractionLimits {
            max_file_size: 1024,
            ..Default::default()
        },
        ..Default::default()
    };

    let data = archive
        .extract_to_memory_with_options("test_file.txt", &options)
        .expect("Expected extraction with generous limit to succeed");
    assert_eq!(data.len(), 18, "test_file.txt is 18 bytes");
}

#[test]
fn test_extract_to_stream_with_options_rejects_tight_limit() {
    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let options = ExtractionOptions {
        limits: ExtractionLimits {
            max_file_size: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = archive.extract_to_stream_with_options("test_file.txt", &options);
    assert!(
        result.is_err(),
        "Expected max_file_size=1 to reject 18-byte entry"
    );
}

#[test]
fn test_extract_to_stream_with_options_accepts_loose_limit() {
    use std::io::Read;

    let archive = Archive::open(common::fixture("test.zip")).expect("Failed to open ZIP");

    let options = ExtractionOptions {
        limits: ExtractionLimits {
            max_file_size: 1024,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut stream = archive
        .extract_to_stream_with_options("test_file.txt", &options)
        .expect("Expected stream with generous limit to succeed");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(buf.len(), 18, "test_file.txt is 18 bytes");
}
