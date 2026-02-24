//! Example demonstrating archive creation with the unified-archive library
//!
//! This example creates various archive formats and demonstrates:
//! - Creating archives with different formats (ZIP, 7z, TAR, TAR.GZ)
//! - Adding files from data (in-memory)
//! - Adding files from filesystem paths
//! - Setting compression levels
//! - Password-protecting archives

use unified_archive::{Archive, ArchiveFormat, CompressionLevel, CompressionOptions, Result};

fn main() -> Result<()> {
    println!("unified-archive creation examples");
    println!("==================================\n");

    // Example 1: Create a simple ZIP archive
    example_create_zip()?;

    // Example 2: Create a TAR.GZ archive
    example_create_tar_gz()?;

    // Example 3: Create a password-protected archive
    example_create_encrypted()?;

    // Example 4: Create archive with maximum compression
    example_create_maximum_compression()?;

    println!("\nAll examples completed successfully!");
    Ok(())
}

/// Create a simple ZIP archive with files from data
fn example_create_zip() -> Result<()> {
    println!("1. Creating simple ZIP archive...");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("/Volumes/Temp/claude/example.zip", options)?;

    // Add files from in-memory data
    archive.add_file_from_data("readme.txt", b"Hello from unified-archive!\n")?;
    archive.add_file_from_data("config.json", br#"{"version": "1.0"}"#)?;
    archive.add_file_from_data(
        "data.txt",
        b"This is some sample data\nthat spans multiple lines\n",
    )?;

    // Finalize the archive
    archive.finish()?;

    println!("   ✓ Created /Volumes/Temp/claude/example.zip with 3 files\n");
    Ok(())
}

/// Create a TAR.GZ archive (compressed tarball)
fn example_create_tar_gz() -> Result<()> {
    println!("2. Creating TAR.GZ archive...");

    let options = CompressionOptions {
        format: ArchiveFormat::TarGzip,
        level: CompressionLevel::Fast,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("/Volumes/Temp/claude/example.tar.gz", options)?;

    archive.add_file_from_data(
        "document.md",
        b"# Documentation\n\nThis is a markdown document.\n",
    )?;
    archive.add_file_from_data("script.sh", b"#!/bin/bash\necho 'Hello, World!'\n")?;

    archive.finish()?;

    println!("   ✓ Created /Volumes/Temp/claude/example.tar.gz with 2 files\n");
    Ok(())
}

/// Create a password-protected archive
fn example_create_encrypted() -> Result<()> {
    println!("3. Creating password-protected archive...");

    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: Some("secret123".to_string()),
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("/Volumes/Temp/claude/encrypted.zip", options)?;

    archive.add_file_from_data("secret.txt", b"This is confidential data\n")?;
    archive.add_file_from_data("passwords.txt", b"admin:hunter2\nuser:password123\n")?;

    archive.finish()?;

    println!("   ✓ Created /Volumes/Temp/claude/encrypted.zip (password: secret123)\n");
    Ok(())
}

/// Create archive with maximum compression
fn example_create_maximum_compression() -> Result<()> {
    println!("4. Creating archive with maximum compression...");

    let options = CompressionOptions {
        format: ArchiveFormat::SevenZip,
        level: CompressionLevel::Ultra,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("/Volumes/Temp/claude/compressed.7z", options)?;

    // Add some repetitive data that compresses well
    let repetitive_data = "This is repetitive data. ".repeat(100);
    archive.add_file_from_data("repetitive.txt", repetitive_data.as_bytes())?;

    archive.add_file_from_data(
        "info.txt",
        b"This archive was created with maximum compression\n",
    )?;

    archive.finish()?;

    println!("   ✓ Created /Volumes/Temp/claude/compressed.7z with ultra compression\n");
    Ok(())
}
