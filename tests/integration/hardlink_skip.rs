//! Integration tests for hard link skip behavior (FR-022, R010-001)
//!
//! Verifies that hard links are silently skipped during extraction for security.

#[cfg_attr(not(feature = "libarchive"), allow(unused_imports))]
use super::common;

#[cfg_attr(not(feature = "libarchive"), allow(unused_imports))]
use std::fs;

#[cfg(feature = "libarchive")]
#[test]
#[cfg(unix)]
fn test_hardlink_skip_during_extraction() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test_hardlink.tar");
    let extract_dir = temp_dir.path().join("extracted");

    fs::create_dir_all(&extract_dir).expect("Failed to create extract dir");

    // Shared fixture builder: `regular.txt` + `hardlink.txt` hard-linked
    // to it. OI-0056-010: infallible and host-independent, so neither the
    // fixture nor the open below can turn this lane into a silent pass.
    common::build_tar_with_hardlink(&archive_path);

    let archive =
        unified_archive::Archive::open(&archive_path).expect("hardlink tar fixture must open");

    // The writer emits exactly the two members, so the count is pinned
    // rather than merely non-empty.
    let entries = archive.list_files().expect("Failed to list files");
    assert_eq!(
        entries.len(),
        2,
        "fixture must list regular.txt + hardlink.txt, got {:?}",
        entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );

    // Extract - hard links should be silently skipped
    let options = common::default_extraction_options(extract_dir.clone());
    let extract_result = archive.extract_all(options);

    // Extraction should succeed (hard links are skipped, not an error)
    assert!(
        extract_result.is_ok(),
        "Extraction should succeed even with hard links: {:?}",
        extract_result
    );

    // Verify original file was extracted
    let extracted_original = extract_dir.join("regular.txt");
    assert!(
        extracted_original.exists(),
        "Original file should be extracted"
    );

    // Read and verify content
    let content = fs::read_to_string(&extracted_original).expect("Failed to read extracted file");
    assert!(
        content.contains("hello from a regular file"),
        "Extracted content should match original"
    );
}

#[cfg(feature = "libarchive")]
#[test]
#[cfg(unix)]
fn test_hardlink_entry_detection() {
    // R0079-0046: the previous version only matched the EntryType
    // variant against itself. This pins the actual detection path: a
    // TAR hard link must surface from list_files() as
    // EntryType::HardLink with its link target populated.
    use unified_archive::entry::EntryType;

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test_hardlink.tar");

    common::build_tar_with_hardlink(&archive_path);

    let archive = unified_archive::Archive::open(&archive_path).expect("Failed to open archive");
    let entries = archive.list_files().expect("Failed to list files");

    let hardlink_entry = entries
        .iter()
        .find(|e| e.path.ends_with("hardlink.txt"))
        .expect("hardlink.txt entry should be listed");
    assert_eq!(
        hardlink_entry.entry_type,
        EntryType::HardLink,
        "hardlink.txt should be detected as a hard link"
    );
    assert!(
        hardlink_entry.is_hardlink(),
        "is_hardlink() should be true for the detected entry"
    );
    assert!(
        hardlink_entry
            .link_target
            .as_deref()
            .is_some_and(|t| t.ends_with("regular.txt")),
        "hard link target should point at regular.txt, got {:?}",
        hardlink_entry.link_target
    );
}

#[test]
fn test_entry_is_hardlink_method() {
    // Test the is_hardlink() helper method on ArchiveEntry
    use unified_archive::entry::{ArchiveEntry, EntryType};

    let mut entry = ArchiveEntry::file("test.txt", 0).build();
    entry.entry_type = EntryType::HardLink;

    assert!(
        entry.is_hardlink(),
        "is_hardlink() should return true for HardLink type"
    );

    entry.entry_type = EntryType::File;
    assert!(
        !entry.is_hardlink(),
        "is_hardlink() should return false for File type"
    );
}
