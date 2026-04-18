//! Example demonstrating archive modification with the unified-archive library
//!
//! This example shows how to modify existing archives:
//! - Adding new files to an existing archive
//! - Removing files from an archive
//! - Replacing existing files with updated content
//! - Committing changes back to the archive

use std::path::Path;
use unified_archive::{
    Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions, Result,
};

fn main() -> Result<()> {
    println!("unified-archive modification examples");
    println!("======================================\n");

    let temp_dir =
        tempfile::tempdir().map_err(|e| ArchiveError::io("create_tempdir", Path::new(""), e))?;
    let sample = temp_dir.path().join("sample.zip");
    println!("Using sample archive: {}\n", sample.display());

    // First, create a sample archive to modify
    create_sample_archive(&sample)?;

    // Example 1: Add files to existing archive
    example_add_files(&sample)?;

    // Example 2: Remove files from archive
    example_remove_files(&sample)?;

    // Example 3: Replace files in archive
    example_replace_files(&sample)?;

    println!("\nAll modification examples completed successfully!");
    Ok(())
}

/// Create a sample archive for demonstration
fn create_sample_archive(path: &Path) -> Result<()> {
    println!("Creating sample archive for modification...");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create(path, options)?;

    archive.add_file_from_data("file1.txt", b"Original content 1\n")?;
    archive.add_file_from_data("file2.txt", b"Original content 2\n")?;
    archive.add_file_from_data("file3.txt", b"Original content 3\n")?;

    archive.finish()?;

    println!("   ✓ Created sample.zip with 3 files\n");
    Ok(())
}

/// Example 1: Add new files to an existing archive
fn example_add_files(path: &Path) -> Result<()> {
    println!("1. Adding files to existing archive...");

    let mut archive = Archive::modify(path)?;

    // Add new files
    archive.add_entry("new_file1.txt", b"This is a new file\n")?;
    archive.add_entry("new_file2.txt", b"Another new file\n")?;

    // Commit changes
    archive.commit_changes()?;

    // Verify
    let archive = Archive::open(path)?;
    let entries = archive.list_files()?;
    println!("   ✓ Archive now has {} files (was 3)", entries.len());
    for entry in entries {
        println!("     - {}", entry.path);
    }
    println!();

    Ok(())
}

/// Example 2: Remove files from an archive
fn example_remove_files(path: &Path) -> Result<()> {
    println!("2. Removing files from archive...");

    let mut archive = Archive::modify(path)?;

    // Remove files
    archive.remove_entry("file2.txt")?;
    archive.remove_entry("new_file2.txt")?;

    // Commit changes
    archive.commit_changes()?;

    // Verify
    let archive = Archive::open(path)?;
    let entries = archive.list_files()?;
    println!("   ✓ Archive now has {} files (removed 2)", entries.len());
    for entry in entries {
        println!("     - {}", entry.path);
    }
    println!();

    Ok(())
}

/// Example 3: Replace files in an archive
fn example_replace_files(path: &Path) -> Result<()> {
    println!("3. Replacing files in archive...");

    let mut archive = Archive::modify(path)?;

    // Replace existing file with updated content
    archive.replace_entry("file1.txt", b"UPDATED content - version 2.0\n")?;
    archive.replace_entry("new_file1.txt", b"This file was also updated\n")?;

    // Commit changes
    archive.commit_changes()?;

    // Verify by extracting
    let archive = Archive::open(path)?;
    let data = archive.extract_to_memory("file1.txt")?;
    let content = String::from_utf8_lossy(&data);

    println!("   ✓ Replaced file content:");
    println!("     Content: {}", content.trim());
    println!();

    Ok(())
}
