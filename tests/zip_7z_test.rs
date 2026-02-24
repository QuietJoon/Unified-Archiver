//! ZIP and 7z format integration tests
//!
//! Tests the libarchive backend for ZIP and 7z formats

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use unified_archive::{Archive, ArchiveFormat, EntryType, ExtractionOptions};

// Mutex to serialize tests that modify the working directory
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Helper to create a temporary extraction directory
fn temp_dir() -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let temp = std::env::temp_dir().join(format!("7zip_test_{}", timestamp));

    // Clean up if exists
    if temp.exists() {
        fs::remove_dir_all(&temp).ok();
    }

    fs::create_dir_all(&temp).expect("Failed to create temp directory");
    temp
}

/// Helper to clean up temporary directory
fn cleanup_temp(path: &PathBuf) {
    fs::remove_dir_all(path).ok();
}

#[test]
fn test_zip_format_detection() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[test]
fn test_7z_format_detection() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
}

#[test]
fn test_zip_list_files() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].entry_type, EntryType::File);
    assert_eq!(entries[0].size, Some(18));
}

#[test]
fn test_7z_list_files() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].entry_type, EntryType::File);
    assert_eq!(entries[0].size, Some(18));
}

#[test]
fn test_zip_extract_all() {
    let _lock = TEST_LOCK.lock().unwrap();
    let temp = temp_dir();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_all(options)
        .expect("Failed to extract ZIP archive");

    // Verify extracted file
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists(), "Extracted file should exist");

    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    cleanup_temp(&temp);
}

#[test]
fn test_7z_extract_all() {
    let _lock = TEST_LOCK.lock().unwrap();
    let temp = temp_dir();

    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_all(options)
        .expect("Failed to extract 7z archive");

    // Verify extracted file
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists(), "Extracted file should exist");

    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    cleanup_temp(&temp);
}

#[test]
fn test_zip_extract_file() {
    let _lock = TEST_LOCK.lock().unwrap();
    let temp = temp_dir();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    archive
        .extract_file("test_file.txt", options)
        .expect("Failed to extract file from ZIP");

    // Verify extracted file
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists());

    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    cleanup_temp(&temp);
}

#[test]
fn test_zip_extract_to_memory() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let content = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");

    let content_str = String::from_utf8(content).expect("Failed to convert to UTF-8");
    assert_eq!(content_str, "Hello, RAR World!\n");
}

#[test]
fn test_7z_extract_to_memory() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    let content = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");

    let content_str = String::from_utf8(content).expect("Failed to convert to UTF-8");
    assert_eq!(content_str, "Hello, RAR World!\n");
}

#[test]
fn test_zip_entry_count() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let count = archive.entry_count().expect("Failed to get entry count");

    assert_eq!(count, 1);
}

#[test]
fn test_zip_find_entry() {
    let _lock = TEST_LOCK.lock().unwrap();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");

    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
}
