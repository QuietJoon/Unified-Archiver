// Additional edge case tests for integrity validation
// Critical scenarios that could break in production

use std::fs;
use std::path::PathBuf;
use unified_archive::Archive;

// ============================================================================
// FILE TYPE TESTS
// ============================================================================

#[test]
fn test_validation_binary_files() {
    // Test validation of archives containing binary data
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/binary_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create binary file with various byte patterns
    let binary_data: Vec<u8> = (0..=255).cycle().take(1024).collect();
    fs::write(temp_dir.join("binary.bin"), &binary_data).expect("Create binary file");

    // Create ZIP
    let zip_path = temp_dir.join("binary.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("binary.bin").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open binary ZIP");
        let report = archive.validate_integrity().expect("Validate binary ZIP");

        assert_eq!(report.failed.len(), 0, "Binary files should validate");
        println!("✓ Binary file validation: {} files", report.validated);
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_text_files_various_encodings() {
    // Test text files with different line endings and content
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/text_test");
    fs::create_dir_all(&temp_dir).ok();

    // Unix line endings
    fs::write(temp_dir.join("unix.txt"), b"Line 1\nLine 2\nLine 3\n").ok();
    // Windows line endings
    fs::write(
        temp_dir.join("windows.txt"),
        b"Line 1\r\nLine 2\r\nLine 3\r\n",
    )
    .ok();
    // Mixed content
    fs::write(
        temp_dir.join("mixed.txt"),
        b"Text\n\x00Binary\xFF\nMore text\n",
    )
    .ok();

    let zip_path = temp_dir.join("text.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("unix.txt").to_str().unwrap(),
            temp_dir.join("windows.txt").to_str().unwrap(),
            temp_dir.join("mixed.txt").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open text ZIP");
        let report = archive.validate_integrity().expect("Validate text ZIP");

        assert_eq!(report.failed.len(), 0, "Text files should validate");
        assert_eq!(report.validated, 3, "Should validate 3 text files");
        println!("✓ Text file validation: {} files", report.validated);
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_mixed_file_types() {
    // Test archive with mixed binary and text files
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/mixed_test");
    fs::create_dir_all(&temp_dir).ok();

    // Text file
    fs::write(temp_dir.join("readme.txt"), b"This is a text file\n").ok();
    // Binary file
    let binary: Vec<u8> = (0..256).map(|i| i as u8).collect();
    fs::write(temp_dir.join("data.bin"), &binary).ok();
    // Empty file
    fs::write(temp_dir.join("empty.dat"), b"").ok();

    let zip_path = temp_dir.join("mixed.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("readme.txt").to_str().unwrap(),
            temp_dir.join("data.bin").to_str().unwrap(),
            temp_dir.join("empty.dat").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open mixed ZIP");
        let report = archive.validate_integrity().expect("Validate mixed ZIP");

        assert_eq!(report.failed.len(), 0, "Mixed files should all validate");
        assert_eq!(report.validated, 3, "Should validate all 3 files");
        println!("✓ Mixed file types validation: {} files", report.validated);
    }

    fs::remove_dir_all(&temp_dir).ok();
}

// ============================================================================
// DIRECTORY STRUCTURE TESTS
// ============================================================================

#[test]
fn test_validation_nested_directories() {
    // Test validation with deeply nested directory structures
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/nested_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create nested structure: dir1/dir2/dir3/file.txt
    let nested_path = temp_dir.join("dir1/dir2/dir3");
    fs::create_dir_all(&nested_path).ok();
    fs::write(nested_path.join("deep_file.txt"), b"Deeply nested file\n").ok();

    // Create file at root level too
    fs::write(temp_dir.join("root_file.txt"), b"Root level file\n").ok();

    let zip_path = temp_dir.join("nested.zip");
    std::process::Command::new("zip")
        .args([
            "-r",
            zip_path.to_str().unwrap(),
            temp_dir.join("dir1").to_str().unwrap(),
            temp_dir.join("root_file.txt").to_str().unwrap(),
        ])
        .current_dir(&temp_dir)
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open nested ZIP");
        let report = archive.validate_integrity().expect("Validate nested ZIP");

        assert_eq!(report.failed.len(), 0, "Nested files should validate");
        assert!(
            report.validated >= 2,
            "Should validate files in nested structure"
        );
        println!("✓ Nested directory validation: {} files", report.validated);
    }

    fs::remove_dir_all(&temp_dir).ok();
}

// ============================================================================
// FILENAME TESTS
// ============================================================================

#[test]
fn test_validation_long_filename() {
    // Test validation with very long filenames (near system limits)
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/longname_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create file with long name (200 characters, below 255 limit)
    let long_name = "a".repeat(200) + ".txt";
    fs::write(temp_dir.join(&long_name), b"File with long name\n").ok();

    let zip_path = temp_dir.join("longname.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join(&long_name).to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open long filename ZIP");
        let report = archive
            .validate_integrity()
            .expect("Validate long filename ZIP");

        assert_eq!(report.failed.len(), 0, "Long filename should validate");
        println!("✓ Long filename (200 chars) validation passed");
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_unicode_filename() {
    // Test validation with Unicode/UTF-8 filenames
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/unicode_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create files with various Unicode characters
    let unicode_names = vec![
        "文件.txt",          // Chinese
        "файл.txt",          // Cyrillic
        "αρχείο.txt",        // Greek
        "ファイル.txt",      // Japanese
        "emoji_😀_test.txt", // Emoji
    ];

    for name in &unicode_names {
        fs::write(temp_dir.join(name), b"Unicode filename test\n").ok();
    }

    let zip_path = temp_dir.join("unicode.zip");
    let mut cmd = std::process::Command::new("zip");
    cmd.arg("-j").arg(zip_path.to_str().unwrap());
    for name in &unicode_names {
        cmd.arg(temp_dir.join(name).to_str().unwrap());
    }
    cmd.output().ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open Unicode ZIP");
        let report = archive.validate_integrity().expect("Validate Unicode ZIP");

        assert_eq!(report.failed.len(), 0, "Unicode filenames should validate");
        println!("✓ Unicode filenames validation: {} files", report.validated);
    }

    fs::remove_dir_all(&temp_dir).ok();
}

// ============================================================================
// CORRUPTION PERSISTENCE TESTS
// ============================================================================

#[test]
fn test_validation_repeated_on_corrupted() {
    // Test that repeated validation calls on corrupted archive remain consistent
    let archive = Archive::open("tests/fixtures/corrupted_crc.zip");

    if let Ok(arc) = archive {
        let mut results = Vec::new();

        // Validate 3 times
        for i in 0..3 {
            match arc.validate_integrity() {
                Ok(report) => {
                    results.push((report.validated, report.failed.clone()));
                    println!(
                        "Validation {}: {} validated, {} failed",
                        i,
                        report.validated,
                        report.failed.len()
                    );
                }
                Err(e) => {
                    println!("Validation {}: error {}", i, e);
                }
            }
        }

        if results.len() >= 2 {
            // Results should be consistent across calls
            let first_failed_count = results[0].1.len();
            for (i, (_, failed)) in results.iter().enumerate() {
                assert_eq!(
                    failed.len(),
                    first_failed_count,
                    "Validation {} should have same failed count",
                    i
                );
            }
            println!(
                "✓ Corrupted archive validation consistency: {} calls",
                results.len()
            );
        }
    }
}

// ============================================================================
// MEMORY STRESS TESTS
// ============================================================================

#[test]
fn test_validation_large_file_stress() {
    // Test validation of archive with larger file (5MB)
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/large_stress_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create 5MB file
    let large_data: Vec<u8> = (0..5_000_000).map(|i| (i % 256) as u8).collect();
    fs::write(temp_dir.join("large_5mb.bin"), &large_data).ok();

    let zip_path = temp_dir.join("large_5mb.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("large_5mb.bin").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open large ZIP");
        let report = archive.validate_integrity().expect("Validate large ZIP");

        assert_eq!(report.failed.len(), 0, "Large file (5MB) should validate");
        println!("✓ Large file (5MB) stress test passed");
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_many_small_files_stress() {
    // Test validation with many small files (stress file count)
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/many_files_test");
    fs::create_dir_all(&temp_dir).ok();

    // Create 50 small files
    for i in 0..50 {
        fs::write(
            temp_dir.join(format!("file_{:03}.txt", i)),
            format!("Content of file {}\n", i).as_bytes(),
        )
        .ok();
    }

    let zip_path = temp_dir.join("many_files.zip");
    let mut cmd = std::process::Command::new("zip");
    cmd.arg("-j").arg(zip_path.to_str().unwrap());
    for i in 0..50 {
        cmd.arg(
            temp_dir
                .join(format!("file_{:03}.txt", i))
                .to_str()
                .unwrap(),
        );
    }
    cmd.output().ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path).expect("Open many files ZIP");
        let report = archive
            .validate_integrity()
            .expect("Validate many files ZIP");

        assert_eq!(report.failed.len(), 0, "All 50 files should validate");
        assert_eq!(report.validated, 50, "Should validate exactly 50 files");
        println!(
            "✓ Many files stress test: {} files validated",
            report.validated
        );
    }

    fs::remove_dir_all(&temp_dir).ok();
}

// ============================================================================
// SPECIFIC FORMAT EDGE CASES
// ============================================================================

#[test]
fn test_validation_zip_compression_methods() {
    // Test ZIP archives with different compression methods (store, deflate)
    let temp_dir = PathBuf::from("/Volumes/Temp/claude/7zip/compression_test");
    fs::create_dir_all(&temp_dir).ok();

    fs::write(temp_dir.join("test.txt"), b"Test data for compression\n").ok();

    // Create ZIP with no compression (store)
    let zip_store = temp_dir.join("stored.zip");
    std::process::Command::new("zip")
        .args([
            "-0", // No compression
            "-j",
            zip_store.to_str().unwrap(),
            temp_dir.join("test.txt").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_store.exists() {
        let archive = Archive::open(&zip_store).expect("Open stored ZIP");
        let report = archive.validate_integrity().expect("Validate stored ZIP");
        assert_eq!(report.failed.len(), 0, "Stored ZIP should validate");
        println!("✓ ZIP with STORE compression validated");
    }

    // Create ZIP with maximum compression (deflate)
    let zip_deflate = temp_dir.join("deflated.zip");
    std::process::Command::new("zip")
        .args([
            "-9", // Maximum compression
            "-j",
            zip_deflate.to_str().unwrap(),
            temp_dir.join("test.txt").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_deflate.exists() {
        let archive = Archive::open(&zip_deflate).expect("Open deflated ZIP");
        let report = archive.validate_integrity().expect("Validate deflated ZIP");
        assert_eq!(report.failed.len(), 0, "Deflated ZIP should validate");
        println!("✓ ZIP with DEFLATE (max) compression validated");
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_error_recovery() {
    // Test that validation errors don't leave archive in bad state
    let archive = Archive::open("tests/fixtures/batch_test.zip").expect("Open test ZIP");

    // First validation
    let report1 = archive.validate_integrity().expect("First validation");

    // Try to cause an error by validating nonexistent archive
    let _ = Archive::open("nonexistent.zip");

    // Second validation on original archive should still work
    let report2 = archive.validate_integrity().expect("Second validation");

    assert_eq!(
        report1.validated, report2.validated,
        "Validation results should be consistent"
    );
    assert_eq!(
        report1.failed, report2.failed,
        "Failed lists should be identical"
    );

    println!("✓ Validation error recovery: results consistent");
}
