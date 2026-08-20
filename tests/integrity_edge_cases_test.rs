// Additional edge case tests for integrity validation
// Critical scenarios that could break in production
//
// Fixture-building tests shell out to the `zip` CLI. They skip loudly
// (eprintln + return) when the CLI is missing and otherwise assert
// every step, so a broken prerequisite cannot silently no-op the
// assertions (R0079-0032).

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::Command;
use unified_archive::Archive;

/// Run the `zip` CLI with `args`, panicking on spawn failure or
/// non-zero exit so fixture-creation problems fail the test instead
/// of skipping its assertions.
fn run_zip(args: &[&str], current_dir: Option<&Path>) {
    let mut cmd = Command::new("zip");
    cmd.args(args);
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    let output = cmd.output().expect("run zip CLI");
    assert!(
        output.status.success(),
        "zip CLI should create the archive: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// FILE TYPE TESTS
// ============================================================================

#[test]
fn test_validation_binary_files() {
    // Test validation of archives containing binary data
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create binary file with various byte patterns
    let binary_data: Vec<u8> = (0..=255).cycle().take(1024).collect();
    fs::write(temp_dir.join("binary.bin"), &binary_data).expect("Create binary file");

    // Create ZIP
    let zip_path = temp_dir.join("binary.zip");
    run_zip(
        &[
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("binary.bin").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_path).expect("Open binary ZIP");
    let report = archive.validate_integrity().expect("Validate binary ZIP");

    assert_eq!(report.failed.len(), 0, "Binary files should validate");
    println!("✓ Binary file validation: {} files", report.validated);

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_text_files_various_encodings() {
    // Test text files with different line endings and content
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Unix line endings
    fs::write(temp_dir.join("unix.txt"), b"Line 1\nLine 2\nLine 3\n").expect("write unix.txt");
    // Windows line endings
    fs::write(
        temp_dir.join("windows.txt"),
        b"Line 1\r\nLine 2\r\nLine 3\r\n",
    )
    .expect("write windows.txt");
    // Mixed content
    fs::write(
        temp_dir.join("mixed.txt"),
        b"Text\n\x00Binary\xFF\nMore text\n",
    )
    .expect("write mixed.txt");

    let zip_path = temp_dir.join("text.zip");
    run_zip(
        &[
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("unix.txt").to_str().unwrap(),
            temp_dir.join("windows.txt").to_str().unwrap(),
            temp_dir.join("mixed.txt").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_path).expect("Open text ZIP");
    let report = archive.validate_integrity().expect("Validate text ZIP");

    assert_eq!(report.failed.len(), 0, "Text files should validate");
    assert_eq!(report.validated, 3, "Should validate 3 text files");
    println!("✓ Text file validation: {} files", report.validated);

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_mixed_file_types() {
    // Test archive with mixed binary and text files
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Text file
    fs::write(temp_dir.join("readme.txt"), b"This is a text file\n").expect("write readme.txt");
    // Binary file
    let binary: Vec<u8> = (0..256).map(|i| i as u8).collect();
    fs::write(temp_dir.join("data.bin"), &binary).expect("write data.bin");
    // Empty file
    fs::write(temp_dir.join("empty.dat"), b"").expect("write empty.dat");

    let zip_path = temp_dir.join("mixed.zip");
    run_zip(
        &[
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("readme.txt").to_str().unwrap(),
            temp_dir.join("data.bin").to_str().unwrap(),
            temp_dir.join("empty.dat").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_path).expect("Open mixed ZIP");
    let report = archive.validate_integrity().expect("Validate mixed ZIP");

    assert_eq!(report.failed.len(), 0, "Mixed files should all validate");
    assert_eq!(report.validated, 3, "Should validate all 3 files");
    println!("✓ Mixed file types validation: {} files", report.validated);

    common::cleanup(&temp_dir);
}

// ============================================================================
// DIRECTORY STRUCTURE TESTS
// ============================================================================

#[test]
fn test_validation_nested_directories() {
    // Test validation with deeply nested directory structures
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create nested structure: dir1/dir2/dir3/file.txt
    let nested_path = temp_dir.join("dir1/dir2/dir3");
    fs::create_dir_all(&nested_path).expect("create nested dirs");
    fs::write(nested_path.join("deep_file.txt"), b"Deeply nested file\n")
        .expect("write deep_file.txt");

    // Create file at root level too
    fs::write(temp_dir.join("root_file.txt"), b"Root level file\n").expect("write root_file.txt");

    let zip_path = temp_dir.join("nested.zip");
    run_zip(
        &[
            "-r",
            zip_path.to_str().unwrap(),
            temp_dir.join("dir1").to_str().unwrap(),
            temp_dir.join("root_file.txt").to_str().unwrap(),
        ],
        Some(&temp_dir),
    );

    let archive = Archive::open(&zip_path).expect("Open nested ZIP");
    let report = archive.validate_integrity().expect("Validate nested ZIP");

    assert_eq!(report.failed.len(), 0, "Nested files should validate");
    assert!(
        report.validated >= 2,
        "Should validate files in nested structure"
    );
    println!("✓ Nested directory validation: {} files", report.validated);

    common::cleanup(&temp_dir);
}

// ============================================================================
// FILENAME TESTS
// ============================================================================

#[test]
fn test_validation_long_filename() {
    // Test validation with very long filenames (near system limits)
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create file with long name (200 characters, below 255 limit)
    let long_name = "a".repeat(200) + ".txt";
    fs::write(temp_dir.join(&long_name), b"File with long name\n").expect("write long-named file");

    let zip_path = temp_dir.join("longname.zip");
    run_zip(
        &[
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join(&long_name).to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_path).expect("Open long filename ZIP");
    let report = archive
        .validate_integrity()
        .expect("Validate long filename ZIP");

    assert_eq!(report.failed.len(), 0, "Long filename should validate");
    println!("✓ Long filename (200 chars) validation passed");

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_unicode_filename() {
    // Test validation with Unicode/UTF-8 filenames
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create files with various Unicode characters
    let unicode_names = vec![
        "文件.txt",          // Chinese
        "файл.txt",          // Cyrillic
        "αρχείο.txt",        // Greek
        "ファイル.txt",      // Japanese
        "emoji_😀_test.txt", // Emoji
    ];

    for name in &unicode_names {
        fs::write(temp_dir.join(name), b"Unicode filename test\n").expect("write unicode file");
    }

    let zip_path = temp_dir.join("unicode.zip");
    let mut args = vec!["-j", zip_path.to_str().unwrap()];
    let unicode_paths: Vec<_> = unicode_names
        .iter()
        .map(|name| temp_dir.join(name))
        .collect();
    args.extend(unicode_paths.iter().map(|p| p.to_str().unwrap()));
    run_zip(&args, None);

    let archive = Archive::open(&zip_path).expect("Open Unicode ZIP");
    let report = archive.validate_integrity().expect("Validate Unicode ZIP");

    assert_eq!(report.failed.len(), 0, "Unicode filenames should validate");
    println!("✓ Unicode filenames validation: {} files", report.validated);

    common::cleanup(&temp_dir);
}

// ============================================================================
// CORRUPTION PERSISTENCE TESTS
// ============================================================================

#[test]
fn test_validation_repeated_on_corrupted() {
    // Test that repeated validation calls on corrupted archive remain consistent
    let archive =
        Archive::open("tests/fixtures/corrupted_crc.zip").expect("Open corrupted-CRC fixture");

    let mut results = Vec::new();

    // Validate 3 times
    for i in 0..3 {
        let report = archive
            .validate_integrity()
            .unwrap_or_else(|e| panic!("Validation {} should complete: {}", i, e));
        println!(
            "Validation {}: {} validated, {} failed",
            i,
            report.validated,
            report.failed.len()
        );
        results.push((report.validated, report.failed.clone()));
    }

    // Results should be consistent across calls
    let first_failed_count = results[0].1.len();
    assert!(
        first_failed_count > 0,
        "Corrupted-CRC fixture should report failed entries"
    );
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

// ============================================================================
// MEMORY STRESS TESTS
// ============================================================================

#[test]
fn test_validation_large_file_stress() {
    // Test validation of archive with larger file (5MB)
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create 5MB file
    let large_data: Vec<u8> = (0..5_000_000).map(|i| (i % 256) as u8).collect();
    fs::write(temp_dir.join("large_5mb.bin"), &large_data).expect("write 5MB file");

    let zip_path = temp_dir.join("large_5mb.zip");
    run_zip(
        &[
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("large_5mb.bin").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_path).expect("Open large ZIP");
    let report = archive.validate_integrity().expect("Validate large ZIP");

    assert_eq!(report.failed.len(), 0, "Large file (5MB) should validate");
    println!("✓ Large file (5MB) stress test passed");

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_many_small_files_stress() {
    // Test validation with many small files (stress file count)
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    // Create 50 small files
    for i in 0..50 {
        fs::write(
            temp_dir.join(format!("file_{:03}.txt", i)),
            format!("Content of file {}\n", i).as_bytes(),
        )
        .expect("write small file");
    }

    let zip_path = temp_dir.join("many_files.zip");
    let mut args = vec!["-j".to_string(), zip_path.to_str().unwrap().to_string()];
    for i in 0..50 {
        args.push(
            temp_dir
                .join(format!("file_{:03}.txt", i))
                .to_str()
                .unwrap()
                .to_string(),
        );
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_zip(&arg_refs, None);

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

    common::cleanup(&temp_dir);
}

// ============================================================================
// SPECIFIC FORMAT EDGE CASES
// ============================================================================

#[test]
fn test_validation_zip_compression_methods() {
    // Test ZIP archives with different compression methods (store, deflate)
    if !common::command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }
    let temp_dir = common::temp_test_dir();

    fs::write(temp_dir.join("test.txt"), b"Test data for compression\n").expect("write test.txt");

    // Create ZIP with no compression (store)
    let zip_store = temp_dir.join("stored.zip");
    run_zip(
        &[
            "-0", // No compression
            "-j",
            zip_store.to_str().unwrap(),
            temp_dir.join("test.txt").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_store).expect("Open stored ZIP");
    let report = archive.validate_integrity().expect("Validate stored ZIP");
    assert_eq!(report.failed.len(), 0, "Stored ZIP should validate");
    println!("✓ ZIP with STORE compression validated");

    // Create ZIP with maximum compression (deflate)
    let zip_deflate = temp_dir.join("deflated.zip");
    run_zip(
        &[
            "-9", // Maximum compression
            "-j",
            zip_deflate.to_str().unwrap(),
            temp_dir.join("test.txt").to_str().unwrap(),
        ],
        None,
    );

    let archive = Archive::open(&zip_deflate).expect("Open deflated ZIP");
    let report = archive.validate_integrity().expect("Validate deflated ZIP");
    assert_eq!(report.failed.len(), 0, "Deflated ZIP should validate");
    println!("✓ ZIP with DEFLATE (max) compression validated");

    common::cleanup(&temp_dir);
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
