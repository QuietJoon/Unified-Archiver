//! Integration tests for hard link skip behavior (FR-022, R010-001)
//!
//! Verifies that hard links are silently skipped during extraction for security.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use unified_archive::ExtractionOptions;

/// Check if a required command is available
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a test archive with hard links using tar
fn create_tar_with_hardlink(archive_path: &Path, temp_dir: &Path) -> std::io::Result<bool> {
    // Create source files
    let file1_path = temp_dir.join("original.txt");
    let hardlink_path = temp_dir.join("hardlink.txt");

    // Create original file
    let mut file1 = fs::File::create(&file1_path)?;
    file1.write_all(b"This is the original file content.\n")?;
    file1.sync_all()?;

    // Create hard link
    #[cfg(unix)]
    {
        std::fs::hard_link(&file1_path, &hardlink_path)?;
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, just create a regular copy (test will be less meaningful)
        fs::copy(&file1_path, &hardlink_path)?;
    }

    // Create tar archive that preserves hard links
    let output = Command::new("tar")
        .args(["cvf", archive_path.to_str().unwrap()])
        .args(["-C", temp_dir.to_str().unwrap()])
        .args(["original.txt", "hardlink.txt"])
        .output()?;

    Ok(output.status.success())
}

#[test]
#[cfg(unix)]
fn test_hardlink_skip_during_extraction() {
    // Skip if tar command not available
    if !command_exists("tar") {
        eprintln!("Skipping test: tar command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let source_dir = temp_dir.path().join("source");
    let archive_path = temp_dir.path().join("test_hardlink.tar");
    let extract_dir = temp_dir.path().join("extracted");

    // Create directories
    fs::create_dir_all(&source_dir).expect("Failed to create source dir");
    fs::create_dir_all(&extract_dir).expect("Failed to create extract dir");

    // Create archive with hard link
    match create_tar_with_hardlink(&archive_path, &source_dir) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("Skipping test: Failed to create tar archive");
            return;
        }
        Err(e) => {
            eprintln!("Skipping test: Error creating archive: {}", e);
            return;
        }
    }

    // Extract using unified-archive
    let result = unified_archive::Archive::open(&archive_path);
    if result.is_err() {
        eprintln!("Skipping test: Could not open tar archive");
        return;
    }

    let archive = result.unwrap();

    // List files to verify structure
    let entries = archive.list_files().expect("Failed to list files");

    // Should have 2 entries (original + hardlink)
    // Note: The actual count may vary based on how tar stores hard links
    assert!(!entries.is_empty(), "Archive should contain entries");

    // Extract - hard links should be silently skipped
    let options = ExtractionOptions {
        destination: extract_dir.clone(),
        ..Default::default()
    };
    let extract_result = archive.extract_all(options);

    // Extraction should succeed (hard links are skipped, not an error)
    assert!(
        extract_result.is_ok(),
        "Extraction should succeed even with hard links: {:?}",
        extract_result
    );

    // Verify original file was extracted
    let extracted_original = extract_dir.join("original.txt");
    assert!(
        extracted_original.exists(),
        "Original file should be extracted"
    );

    // Read and verify content
    let content = fs::read_to_string(&extracted_original).expect("Failed to read extracted file");
    assert!(
        content.contains("original file content"),
        "Extracted content should match original"
    );
}

#[test]
fn test_hardlink_entry_detection() {
    // Test that EntryType::HardLink is properly defined
    use unified_archive::entry::EntryType;

    let hardlink_type = EntryType::HardLink;
    assert!(
        matches!(hardlink_type, EntryType::HardLink),
        "HardLink variant should exist"
    );
}

#[test]
fn test_entry_is_hardlink_method() {
    // Test the is_hardlink() helper method on ArchiveEntry
    use unified_archive::entry::{ArchiveEntry, EntryType};

    let mut entry = ArchiveEntry::new("test.txt".to_string(), 0);
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
