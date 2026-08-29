//! Example demonstrating archive creation with the unified-archive library
//!
//! This example creates various archive formats and demonstrates:
//! - Creating archives with different formats (ZIP, 7z, TAR, TAR.GZ)
//! - Adding files from data (in-memory)
//! - Adding files from filesystem paths
//! - Setting compression levels
//! - Encrypted-archive creation is NOT supported (see MADR-0027) —
//!   use the demo below to see how the API rejects such requests.

use std::path::Path;
use unified_archive::{
    Archive, ArchiveError, CompressionLevel, CompressionOptions, Result, WritableFormat,
};

fn main() -> Result<()> {
    println!("unified-archive creation examples");
    println!("==================================\n");

    let temp_dir =
        tempfile::tempdir().map_err(|e| ArchiveError::io("create_tempdir", Path::new(""), e))?;
    let base = temp_dir.path();
    println!("Using scratch dir: {}\n", base.display());

    // Example 1: Create a simple ZIP archive
    example_create_zip(base)?;

    // Example 2: Create a TAR.GZ archive
    example_create_tar_gz(base)?;

    // Example 3: Demonstrate that encrypted creation is rejected
    example_encrypted_creation_rejected(base)?;

    // Example 4: Create archive with maximum compression
    example_create_maximum_compression(base)?;

    println!("\nAll examples completed successfully!");
    Ok(())
}

/// Create a simple ZIP archive with files from data
fn example_create_zip(base: &Path) -> Result<()> {
    println!("1. Creating simple ZIP archive...");

    // `CompressionOptions` is `#[non_exhaustive]`, so a struct literal is not
    // available outside the crate. `for_writable` gives the same defaults
    // (Normal level, no password, no split, no progress) and rules out a
    // read-only format at construction.
    let options = CompressionOptions::for_writable(WritableFormat::ZIP);

    let path = base.join("example.zip");
    let mut archive = Archive::create(&path, options)?;

    // Add files from in-memory data
    archive.add_file_from_data("readme.txt", b"Hello from unified-archive!\n")?;
    archive.add_file_from_data("config.json", br#"{"version": "1.0"}"#)?;
    archive.add_file_from_data(
        "data.txt",
        b"This is some sample data\nthat spans multiple lines\n",
    )?;

    // Finalize the archive
    archive.finish()?;

    println!("   ✓ Created {} with 3 files\n", path.display());
    Ok(())
}

/// Create a TAR.GZ archive (compressed tarball)
fn example_create_tar_gz(base: &Path) -> Result<()> {
    println!("2. Creating TAR.GZ archive...");

    let mut options = CompressionOptions::for_writable(WritableFormat::TAR_GZIP);
    // The fields stay `pub` under `#[non_exhaustive]`; only the literal is gone.
    options.level = CompressionLevel::Fast;

    let path = base.join("example.tar.gz");
    let mut archive = Archive::create(&path, options)?;

    archive.add_file_from_data(
        "document.md",
        b"# Documentation\n\nThis is a markdown document.\n",
    )?;
    archive.add_file_from_data("script.sh", b"#!/bin/bash\necho 'Hello, World!'\n")?;

    archive.finish()?;

    println!("   ✓ Created {} with 2 files\n", path.display());
    Ok(())
}

/// Demonstrate that encrypted-archive creation is rejected (MADR-0027).
///
/// `Archive::create` with a password set returns `ArchiveError::OperationBlocked`
/// for every supported format. Encrypted reads are still fully supported via
/// `Archive::open_encrypted`.
fn example_encrypted_creation_rejected(base: &Path) -> Result<()> {
    println!("3. Encrypted-archive creation is rejected (MADR-0027)...");

    let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
    options.password = Some("secret123".into());

    let path = base.join("encrypted.zip");
    match Archive::create(&path, options) {
        Ok(_) => {
            println!("   ✗ Unexpected: creation succeeded (should have been blocked)\n");
        }
        Err(ArchiveError::OperationBlocked { operation, reason }) => {
            println!(
                "   ✓ Creation correctly rejected [{}]: {}\n",
                operation, reason
            );
        }
        Err(err) => {
            println!("   ✓ Creation rejected with: {}\n", err);
        }
    }

    Ok(())
}

/// Create archive with maximum compression
fn example_create_maximum_compression(base: &Path) -> Result<()> {
    println!("4. Creating archive with maximum compression...");

    let mut options = CompressionOptions::for_writable(WritableFormat::SEVEN_ZIP);
    options.level = CompressionLevel::Ultra;

    let path = base.join("compressed.7z");
    let mut archive = Archive::create(&path, options)?;

    // Add some repetitive data that compresses well
    let repetitive_data = "This is repetitive data. ".repeat(100);
    archive.add_file_from_data("repetitive.txt", repetitive_data.as_bytes())?;

    archive.add_file_from_data(
        "info.txt",
        b"This archive was created with maximum compression\n",
    )?;

    archive.finish()?;

    println!("   ✓ Created {} with ultra compression\n", path.display());
    Ok(())
}
