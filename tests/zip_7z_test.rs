//! ZIP- and 7z-specific extraction paths (single-file, to-memory, find_entry).

#[path = "common/mod.rs"]
mod common;

use std::fs;
use unified_archive::Archive;

#[cfg(feature = "sevenzip")]
#[test]
fn test_7z_list_files() {
    use unified_archive::{ArchiveFormat, EntryType};
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].entry_type, EntryType::File);
    assert_eq!(entries[0].size, Some(18));
}

#[test]
fn test_zip_extract_file() {
    let temp = common::temp_test_dir();

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let options = common::default_extraction_options(temp.clone());

    archive
        .extract_file("test_file.txt", options)
        .expect("Failed to extract file from ZIP");

    // Verify extracted file
    let extracted_file = temp.join("test_file.txt");
    assert!(extracted_file.exists());

    let content = fs::read_to_string(&extracted_file).expect("Failed to read extracted file");
    assert_eq!(content, "Hello, RAR World!\n");

    common::cleanup(&temp);
}

#[test]
fn test_zip_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let content = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");

    let content_str = String::from_utf8(content).expect("Failed to convert to UTF-8");
    assert_eq!(content_str, "Hello, RAR World!\n");
}

#[cfg(feature = "sevenzip")]
#[test]
fn test_7z_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    let content = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");

    let content_str = String::from_utf8(content).expect("Failed to convert to UTF-8");
    assert_eq!(content_str, "Hello, RAR World!\n");
}

#[test]
fn test_zip_find_entry() {
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");

    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
}
