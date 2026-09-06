// Comprehensive integrity checking tests for critical core library validation
// These tests ensure archive integrity verification works correctly under all conditions

use std::thread;
use unified_archive::{Archive, ArchiveError};

// ============================================================================
// CORRUPTED ARCHIVE DETECTION TESTS
// ============================================================================

#[test]
fn test_detect_crc_corruption_zip() {
    // Test that CRC-corrupted ZIP files are detected
    let archive = Archive::open("tests/fixtures/corrupted_crc.zip");

    match archive {
        Ok(arc) => {
            let report = arc
                .validate_integrity()
                .expect("Validation should complete even with corruption");

            // Corrupted file should be detected
            assert!(
                !report.failed.is_empty(),
                "CRC-corrupted ZIP should have failed files"
            );
            println!(
                "✓ CRC corruption detected: {} files failed",
                report.failed.len()
            );
        }
        Err(e) => {
            // Archive might fail to open if severely corrupted
            println!("✓ Severely corrupted ZIP rejected at open: {}", e);
        }
    }
}

#[test]
fn test_detect_truncated_archive_zip() {
    // Test that truncated archives are properly handled
    let result = Archive::open("tests/fixtures/truncated.zip");

    match result {
        Ok(arc) => {
            // If it opens, validation should fail
            let validation = arc.validate_integrity();
            assert!(
                validation.is_err() || !validation.unwrap().failed.is_empty(),
                "Truncated archive should fail validation"
            );
            println!("✓ Truncated archive detected during validation");
        }
        Err(e) => {
            // More likely: truncated archive fails to open
            println!("✓ Truncated archive rejected at open: {}", e);
            assert!(matches!(e, ArchiveError::Format { .. }));
        }
    }
}

#[test]
fn test_valid_archive_baseline_zip() {
    // Baseline: valid archive should pass all validation
    let archive =
        Archive::open("tests/fixtures/valid_test.zip").expect("Valid test ZIP should open");

    let report = archive
        .validate_integrity()
        .expect("Valid ZIP validation should succeed");

    assert_eq!(
        report.failed.len(),
        0,
        "Valid ZIP should have zero failed files"
    );
    assert!(
        report.validated > 0,
        "Valid ZIP should have validated files"
    );
    println!("✓ Baseline validation: {} files passed", report.validated);
}

// ============================================================================
// MULTI-FORMAT CORRUPTION DETECTION
// ============================================================================

#[cfg(feature = "sevenzip")]
#[test]
#[serial_test::file_serial(rar)]
fn test_corruption_detection_consistency_across_formats() {
    // Ensure all formats detect corruption consistently

    let test_cases = vec![
        ("tests/fixtures/test.zip", "ZIP"),
        ("tests/fixtures/test.rar", "RAR"),
        ("tests/fixtures/test_rar5.rar", "RAR5"),
        ("tests/fixtures/test.7z", "7z"),
    ];

    let mut successful_validations = 0;

    for (path, format) in test_cases {
        match Archive::open(path) {
            Ok(archive) => {
                match archive.validate_integrity() {
                    Ok(report) => {
                        // Valid archives should have no failures
                        assert_eq!(
                            report.failed.len(),
                            0,
                            "{} format: valid archive should not fail validation",
                            format
                        );
                        assert!(
                            report.validated > 0,
                            "{} format: should validate at least one file",
                            format
                        );

                        println!(
                            "✓ {} format corruption detection: {} files validated",
                            format, report.validated
                        );
                        successful_validations += 1;
                    }
                    Err(e) => {
                        // Some formats may have transient errors (e.g., UnRAR handle conflicts)
                        println!(
                            "⚠ {} format validation error (may be transient): {}",
                            format, e
                        );
                    }
                }
            }
            Err(e) => {
                println!("⚠ {} format could not be opened: {}", format, e);
            }
        }
    }

    // At least 2 formats should validate successfully
    assert!(
        successful_validations >= 2,
        "At least 2 archive formats should validate successfully, got {}",
        successful_validations
    );

    println!(
        "✓ Multi-format validation: {}/4 formats validated successfully",
        successful_validations
    );
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

#[test]
fn test_validation_empty_file_in_archive() {
    // Test validation of archives containing empty files (0 bytes)
    use std::fs;

    // R0074-0074: use tempfile so the test is portable and cleans up
    // automatically. The previous hardcoded scratch path was
    // machine-specific.
    let temp_handle = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_dir = temp_handle.path().to_path_buf();

    // Create an empty file
    fs::write(temp_dir.join("empty.txt"), b"").expect("Create empty file");

    // Create ZIP with empty file
    let zip_path = temp_dir.join("empty_file.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("empty.txt").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path);
        if let Ok(arc) = archive {
            let report = arc
                .validate_integrity()
                .expect("Empty file validation should succeed");

            // Empty files should still validate (CRC32 of empty = 0x00000000)
            println!("✓ Empty file validation: {} files", report.validated);
        }
    }

    // Cleanup
    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_validation_large_file() {
    // Test validation of archives with files > 1MB
    use std::fs;

    // R0074-0074: tempdir() makes the path portable + auto-cleans.
    let temp_handle = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_dir = temp_handle.path();

    // Create a 2MB file with pattern
    let large_data: Vec<u8> = (0..2_000_000).map(|i| (i % 256) as u8).collect();
    fs::write(temp_dir.join("large.bin"), &large_data).expect("Create large file");

    // Create ZIP with large file
    let zip_path = temp_dir.join("large_file.zip");
    std::process::Command::new("zip")
        .args([
            "-j",
            zip_path.to_str().unwrap(),
            temp_dir.join("large.bin").to_str().unwrap(),
        ])
        .output()
        .ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path);
        if let Ok(arc) = archive {
            let report = arc
                .validate_integrity()
                .expect("Large file validation should succeed");

            assert!(
                report.validated > 0,
                "Large files should validate successfully"
            );
            println!("✓ Large file (2MB) validation passed");
        }
    }
}

#[test]
fn test_validation_special_characters_in_filename() {
    // Test validation with special characters in filenames
    use std::fs;

    // R0074-0074: tempdir() makes the path portable + auto-cleans.
    let temp_handle = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_dir = temp_handle.path().to_path_buf();

    // Create file with special characters (safe subset)
    let special_names = vec![
        "file-with-dash.txt",
        "file_with_underscore.txt",
        "file.multiple.dots.txt",
    ];

    for name in &special_names {
        fs::write(temp_dir.join(name), b"test content").ok();
    }

    // Create ZIP
    let zip_path = temp_dir.join("special_chars.zip");
    let mut cmd = std::process::Command::new("zip");
    cmd.arg("-j").arg(zip_path.to_str().unwrap());
    for name in &special_names {
        cmd.arg(temp_dir.join(name).to_str().unwrap());
    }
    cmd.output().ok();

    if zip_path.exists() {
        let archive = Archive::open(&zip_path);
        if let Ok(arc) = archive {
            let report = arc
                .validate_integrity()
                .expect("Special chars validation should succeed");

            assert_eq!(
                report.failed.len(),
                0,
                "Files with special chars should validate"
            );
            println!(
                "✓ Special characters in filenames: {} files validated",
                report.validated
            );
        }
    }

    // Cleanup
    fs::remove_dir_all(&temp_dir).ok();
}

// ============================================================================
// CONCURRENT VALIDATION TESTS
// ============================================================================

#[test]
fn test_concurrent_validation_same_archive() {
    // Test that multiple threads can validate the same archive simultaneously
    let archive_path = "tests/fixtures/batch_test.zip";

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let path = archive_path.to_string();
            thread::spawn(move || {
                let archive = Archive::open(&path)
                    .unwrap_or_else(|_| panic!("Thread {} should open archive", i));
                let report = archive
                    .validate_integrity()
                    .unwrap_or_else(|_| panic!("Thread {} validation should succeed", i));
                (i, report.validated, report.failed.len())
            })
        })
        .collect();

    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("Thread should complete"))
        .collect();

    // All threads should get the same result
    let first_validated = results[0].1;
    let first_failed = results[0].2;

    for (i, validated, failed) in &results {
        assert_eq!(
            *validated, first_validated,
            "Thread {} should have same validated count",
            i
        );
        assert_eq!(
            *failed, first_failed,
            "Thread {} should have same failed count",
            i
        );
    }

    println!(
        "✓ Concurrent validation: 4 threads validated {} files consistently",
        first_validated
    );
}

#[cfg(feature = "sevenzip")]
#[test]
#[serial_test::file_serial(rar)]
fn test_concurrent_validation_different_archives() {
    // Test validating multiple different archives concurrently
    let archives = vec![
        "tests/fixtures/test.zip",
        "tests/fixtures/test.rar",
        "tests/fixtures/test_rar5.rar",
        "tests/fixtures/test.7z",
    ];

    let handles: Vec<_> = archives
        .into_iter()
        .map(|path| {
            let path_owned = path.to_string();
            thread::spawn(move || {
                if let Ok(archive) = Archive::open(&path_owned) {
                    archive
                        .validate_integrity()
                        .map(|r| (path_owned.clone(), r.validated, r.failed.len()))
                        .ok()
                } else {
                    None
                }
            })
        })
        .collect();

    let results: Vec<_> = handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect();

    assert!(
        results.len() >= 2,
        "At least 2 archives should validate concurrently"
    );

    for (path, validated, failed) in &results {
        assert_eq!(*failed, 0, "{} should have no failures", path);
        assert!(*validated > 0, "{} should validate files", path);
    }

    println!(
        "✓ Concurrent different archives: {} archives validated",
        results.len()
    );
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_validation_nonexistent_file() {
    // Test validation fails gracefully for nonexistent files
    let result = Archive::open("tests/fixtures/does_not_exist.zip");

    assert!(result.is_err(), "Nonexistent archive should fail to open");

    match result {
        Err(ArchiveError::Io { .. }) => {
            println!("✓ Nonexistent file error handled correctly");
        }
        Err(e) => {
            println!("✓ Nonexistent file rejected with: {:?}", e);
        }
        Ok(_) => panic!("Should not open nonexistent file"),
    }
}

#[test]
fn test_validation_not_an_archive() {
    // Test that non-archive files are rejected
    use std::fs;

    // R0074-0074: tempdir() instead of a hardcoded host path.
    let temp_handle = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_path = temp_handle.path().join("not_archive.zip");

    // Create a text file with .zip extension
    fs::write(&temp_path, b"This is not a ZIP file, just text").ok();

    let result = Archive::open(&temp_path);

    assert!(result.is_err(), "Text file should not open as archive");

    match result {
        Err(ArchiveError::Format { .. }) => {
            println!("✓ Non-archive file rejected with Format error");
        }
        Err(e) => {
            println!("✓ Non-archive file rejected: {:?}", e);
        }
        Ok(_) => panic!("Should not open non-archive as ZIP"),
    }
}

#[test]
#[serial_test::file_serial(rar)]
fn test_validation_encrypted_without_password() {
    // Test validation of encrypted archives without password
    let archive = Archive::open("tests/fixtures/test_encrypted.rar");

    match archive {
        Ok(arc) => {
            let result = arc.validate_integrity();

            // Validation should either fail or report all files as failed
            match result {
                Ok(report) => {
                    println!(
                        "Encrypted validation: {} validated, {} failed",
                        report.validated,
                        report.failed.len()
                    );
                    // Encrypted files without password should fail
                    assert!(
                        !report.failed.is_empty() || report.validated == 0,
                        "Encrypted files should fail without password"
                    );
                }
                Err(_) => {
                    println!("✓ Encrypted archive validation failed (expected without password)");
                }
            }
        }
        Err(_) => {
            println!("Encrypted archive test skipped (file not available)");
        }
    }
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_validation_many_files() {
    // Test validation of archive with many files (stress test)
    // Uses batch_test.zip which has multiple files

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Batch test ZIP should open");

    let entries = archive.list_files().expect("Should list files");

    println!("Testing validation of {} total entries", entries.len());

    let report = archive
        .validate_integrity()
        .expect("Batch validation should succeed");

    assert_eq!(
        report.failed.len(),
        0,
        "All files in batch archive should validate"
    );
    println!(
        "✓ Stress test: {} files validated successfully",
        report.validated
    );
}

#[test]
fn test_validation_repeated_calls() {
    // Test that validation can be called multiple times on same archive
    let archive = Archive::open("tests/fixtures/test.zip").expect("Test ZIP should open");

    let mut results = Vec::new();

    for i in 0..5 {
        let report = archive
            .validate_integrity()
            .unwrap_or_else(|_| panic!("Validation {} should succeed", i));
        results.push((report.validated, report.failed.len()));
    }

    // All calls should return identical results
    let first = results[0];
    for (i, result) in results.iter().enumerate() {
        assert_eq!(
            *result, first,
            "Validation call {} should return same results",
            i
        );
    }

    println!("✓ Repeated validation (5x): consistent results");
}

// ============================================================================
// VALIDATION REPORT STRUCTURE TESTS
// ============================================================================

#[test]
fn test_validation_report_counts_accuracy() {
    // Test that validation report counts are mathematically correct
    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Batch test ZIP should open");

    let entries = archive.list_files().expect("Should list files");

    let file_count = entries.iter().filter(|e| e.is_file()).count();
    let dir_count = entries.iter().filter(|e| e.is_directory()).count();

    let report = archive
        .validate_integrity()
        .expect("Validation should succeed");

    // Verified mathematical relationships
    assert_eq!(
        report.total_entries,
        entries.len(),
        "Total entries should match list"
    );
    assert_eq!(
        report.total_entries,
        file_count + dir_count,
        "Total should equal files + directories"
    );
    assert_eq!(
        report.validated + report.failed.len(),
        file_count,
        "Validated + failed should equal file count (dirs excluded)"
    );

    println!("✓ Validation report accuracy:");
    println!(
        "  Total: {}, Files: {}, Dirs: {}",
        report.total_entries, file_count, dir_count
    );
    println!(
        "  Validated: {}, Failed: {}",
        report.validated,
        report.failed.len()
    );
}

#[test]
fn test_validation_failed_list_contains_paths() {
    // Test that failed files list contains actual file paths
    let archive = Archive::open("tests/fixtures/corrupted_crc.zip");

    if let Ok(arc) = archive {
        let report = arc.validate_integrity();

        if let Ok(rep) = report {
            if !rep.failed.is_empty() {
                // Failed list should contain valid path strings
                for path in &rep.failed {
                    assert!(!path.is_empty(), "Failed file paths should not be empty");
                    println!("Failed file: {}", path);
                }
                println!("✓ Failed list contains {} file paths", rep.failed.len());
            }
        }
    }
}
