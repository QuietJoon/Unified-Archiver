//! Test large ZIP file handling to verify mmap size guard behavior
//!
//! This test validates the consistency of size guards across piz operations.

use std::path::{Path, PathBuf};
use unified_archive::Archive;

const LARGE_ZIP_PATH: &str = "/Volumes/Temp/claude/7zip/large_test/large_test.zip";
const EXTRACT_PATH: &str = "/Volumes/Temp/claude/7zip/large_test/extracted";

/// Test that documents current behavior with large (>100MB) ZIP files
#[test]
fn test_large_zip_behavior() {
    let zip_path = Path::new(LARGE_ZIP_PATH);

    // Skip if test file doesn't exist
    if !zip_path.exists() {
        eprintln!("Skipping: Large test ZIP not found at {}", LARGE_ZIP_PATH);
        return;
    }

    let file_size = std::fs::metadata(zip_path).unwrap().len();
    println!(
        "Testing ZIP: {} ({} bytes / {} MB)",
        LARGE_ZIP_PATH,
        file_size,
        file_size / 1024 / 1024
    );

    // Test 1: Open archive (should succeed - no mmap yet)
    println!("\n=== Test 1: Open archive ===");
    let archive = Archive::open(zip_path).expect("Should open archive");
    println!("Format: {:?}", archive.format());

    // Test 2: List files (has 100MB guard - should fail for >100MB)
    println!("\n=== Test 2: List files (has size guard) ===");
    let list_result = archive.list_files();
    match &list_result {
        Ok(entries) => println!(
            "UNEXPECTED: Listed {} entries (guard should have rejected)",
            entries.len()
        ),
        Err(e) => println!("EXPECTED: {}", e),
    }

    // Test 3: Extract all (no size guard - should succeed)
    println!("\n=== Test 3: Extract all (no size guard) ===");
    std::fs::create_dir_all(EXTRACT_PATH).ok();

    // Clean up any previous extraction
    if Path::new(EXTRACT_PATH).exists() {
        for entry in std::fs::read_dir(EXTRACT_PATH).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).ok();
        }
    }

    // Re-open to avoid cache
    let archive2 = Archive::open(zip_path).expect("Should open archive");
    let options = unified_archive::ExtractionOptions {
        destination: PathBuf::from(EXTRACT_PATH),
        overwrite: true,
        ..Default::default()
    };

    let extract_result = archive2.extract_all(options);
    match &extract_result {
        Ok(_) => {
            println!("OK: Extraction completed");
            for entry in std::fs::read_dir(EXTRACT_PATH).unwrap() {
                let entry = entry.unwrap();
                println!(
                    "  Extracted: {} ({} bytes)",
                    entry.file_name().to_string_lossy(),
                    entry.metadata().unwrap().len()
                );
            }
        }
        Err(e) => println!("FAILED: {}", e),
    }

    // Document the inconsistency
    if list_result.is_err() && extract_result.is_ok() {
        println!("\n=== INCONSISTENCY DETECTED ===");
        println!("list_files() rejected due to size guard, but extract_all() succeeded");
        println!("This is the R003-03 issue");
    }
}

/// Test to verify the current DEFAULT_MAX_MMAP_SIZE value
#[test]
fn test_mmap_size_constant() {
    // Verify the constant exists and has expected value
    use unified_archive::security::DEFAULT_MAX_MMAP_SIZE;

    println!(
        "DEFAULT_MAX_MMAP_SIZE = {} bytes ({} MB)",
        DEFAULT_MAX_MMAP_SIZE,
        DEFAULT_MAX_MMAP_SIZE / 1024 / 1024
    );

    // Platform-specific limits
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        DEFAULT_MAX_MMAP_SIZE,
        4 * 1024 * 1024 * 1024,
        "Expected 4GB limit on 64-bit"
    );

    #[cfg(target_pointer_width = "32")]
    assert_eq!(
        DEFAULT_MAX_MMAP_SIZE,
        100 * 1024 * 1024,
        "Expected 100MB limit on 32-bit"
    );
}
