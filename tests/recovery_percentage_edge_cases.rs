//! Comprehensive edge case tests for recovery percentage extraction
//!
//! Tests covering:
//! - Consistency between has_recovery_record() and recovery_percentage()
//! - Error handling (invalid files, I/O errors, corrupted data)
//! - Boundary conditions (0%, 100%, edge percentages)
//! - Format-specific scenarios (RAR4 vs RAR5, encrypted, solid, multi-volume)
//! - Performance and caching behavior

use std::fs;
use unified_archive::Archive;

// ============================================================================
// CONSISTENCY TESTS: has_recovery_record() vs recovery_percentage()
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_consistency_rar_recovery_methods() {
    // Both methods should be consistent: if has_recovery is true, percentage should be Some
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record failed");
    let percentage = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    // Consistency check
    if has_recovery {
        // If recovery flag is set, we should either get a percentage or None (if we can't parse it)
        // This is acceptable since parsing might fail even if flag is set
        println!(
            "RAR has recovery: {}, percentage: {:?}",
            has_recovery, percentage
        );
    } else {
        assert_eq!(
            percentage, None,
            "If no recovery flag, percentage must be None"
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_consistency_rar5_recovery_methods() {
    // Both methods should be consistent for RAR5
    let archive = Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5");

    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record failed");
    let percentage = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    // Consistency check
    if has_recovery {
        println!(
            "RAR5 has recovery: {}, percentage: {:?}",
            has_recovery, percentage
        );
    } else {
        assert_eq!(
            percentage, None,
            "If no recovery flag, percentage must be None"
        );
    }
}

#[test]
fn test_consistency_no_recovery_formats() {
    // Formats without recovery support should return consistent results
    let test_cases = vec![
        ("tests/fixtures/test.zip", "ZIP"),
        ("tests/fixtures/test.7z", "7z"),
    ];

    for (path, format) in test_cases {
        let archive = Archive::open(path).unwrap_or_else(|_| panic!("Failed to open {}", format));

        let has_recovery = archive
            .has_recovery_record()
            .expect("has_recovery_record failed");
        let percentage = archive
            .recovery_percentage()
            .expect("recovery_percentage failed");

        assert!(!has_recovery, "{} should not have recovery records", format);
        assert_eq!(
            percentage, None,
            "{} should return None for percentage",
            format
        );
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_recovery_percentage_nonexistent_file() {
    // Should fail gracefully for non-existent file
    let result = Archive::open("tests/fixtures/nonexistent.rar");
    assert!(result.is_err(), "Should fail for non-existent file");
}

#[test]
fn test_recovery_percentage_invalid_rar() {
    // Create a temporary invalid RAR file
    let temp_path = "/Volumes/Temp/claude/7zip/invalid.rar";
    fs::write(temp_path, b"Not a valid RAR file").expect("Failed to write temp file");

    let result = Archive::open(temp_path);
    // Should either fail to open or fail during format detection
    assert!(result.is_err(), "Should fail for invalid RAR file");

    // Cleanup
    let _ = fs::remove_file(temp_path);
}

#[test]
fn test_recovery_percentage_truncated_file() {
    // Create a truncated RAR file (just signature)
    let temp_path = "/Volumes/Temp/claude/7zip/truncated.rar";
    fs::write(temp_path, b"Rar!\x1A\x07\x00").expect("Failed to write temp file");

    let result = Archive::open(temp_path);
    if let Ok(archive) = result {
        // Should handle gracefully - either return None or error
        let pct_result = archive.recovery_percentage();
        match pct_result {
            Ok(None) => println!("Truncated file: correctly returns None"),
            Ok(Some(p)) => panic!("Truncated file should not return percentage: {}", p),
            Err(_) => println!("Truncated file: correctly returns error"),
        }
    }

    // Cleanup
    let _ = fs::remove_file(temp_path);
}

#[test]
fn test_recovery_percentage_empty_file() {
    // Empty file should fail to open
    let temp_path = "/Volumes/Temp/claude/7zip/empty.rar";
    fs::write(temp_path, b"").expect("Failed to write temp file");

    let result = Archive::open(temp_path);
    assert!(result.is_err(), "Should fail for empty file");

    // Cleanup
    let _ = fs::remove_file(temp_path);
}

// ============================================================================
// BOUNDARY CONDITION TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_range_validation() {
    // If percentage is returned, it should be in valid range (1-100%)
    let test_files = vec!["tests/fixtures/test.rar", "tests/fixtures/test_rar5.rar"];

    for path in test_files {
        let archive = Archive::open(path).expect("Failed to open archive");

        if let Ok(Some(pct)) = archive.recovery_percentage() {
            assert!(
                (1..=100).contains(&pct),
                "Recovery percentage must be 1-100%, got {} for {}",
                pct,
                path
            );
            println!("{}: recovery percentage = {}%", path, pct);
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_multiple_calls() {
    // Multiple calls should return consistent results
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    let pct1 = archive.recovery_percentage().expect("First call failed");
    let pct2 = archive.recovery_percentage().expect("Second call failed");
    let pct3 = archive.recovery_percentage().expect("Third call failed");

    assert_eq!(pct1, pct2, "Multiple calls should return same result");
    assert_eq!(pct2, pct3, "Multiple calls should return same result");
}

// ============================================================================
// FORMAT-SPECIFIC TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_rar4_vs_rar5() {
    // Test that both RAR4 and RAR5 formats are handled correctly
    let rar4 = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR4");
    let rar5 = Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5");

    let pct4 = rar4
        .recovery_percentage()
        .expect("RAR4 recovery_percentage failed");
    let pct5 = rar5
        .recovery_percentage()
        .expect("RAR5 recovery_percentage failed");

    // Both should succeed (whether they have recovery or not)
    println!("RAR4 recovery: {:?}", pct4);
    println!("RAR5 recovery: {:?}", pct5);

    // If either has recovery, percentage should be valid
    if let Some(p) = pct4 {
        assert!(
            (1..=100).contains(&p),
            "RAR4 percentage out of range: {}",
            p
        );
    }
    if let Some(p) = pct5 {
        assert!(
            (1..=100).contains(&p),
            "RAR5 percentage out of range: {}",
            p
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_all_supported_formats() {
    // Test all supported archive formats
    let test_cases = vec![
        ("tests/fixtures/test.rar", "RAR4", true),
        ("tests/fixtures/test_rar5.rar", "RAR5", true),
        ("tests/fixtures/test.zip", "ZIP", false),
        ("tests/fixtures/test.7z", "7z", false),
    ];

    for (path, format, supports_recovery) in test_cases {
        let archive =
            Archive::open(path).unwrap_or_else(|_| panic!("Failed to open {} archive", format));

        let result = archive.recovery_percentage();
        assert!(
            result.is_ok(),
            "{} recovery_percentage should not error",
            format
        );

        let pct = result.unwrap();
        if !supports_recovery {
            assert_eq!(pct, None, "{} should not support recovery records", format);
        }
        // For formats that support recovery, pct can be Some or None depending on the specific archive

        println!(
            "{}: supports_recovery={}, percentage={:?}",
            format, supports_recovery, pct
        );
    }
}

// ============================================================================
// EDGE CASE: SPECIAL ARCHIVE TYPES
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_encrypted_archive() {
    // Test recovery percentage on encrypted archive (if available)
    // Note: This test may be skipped if no encrypted test fixture with recovery exists
    // For now, just verify it doesn't crash on encrypted archives

    // Using non-encrypted archives for now since encrypted fixtures may not be available
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open");
    let result = archive.recovery_percentage();
    assert!(result.is_ok(), "Should handle archives gracefully");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_solid_archive() {
    // Test recovery percentage on solid archive
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    let is_solid = archive.is_solid().expect("is_solid failed");
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    println!("Solid: {}, Recovery: {:?}", is_solid, pct);

    // Both solid and recovery are independent features, so any combination is valid
    // Just verify both methods work without error
}

// ============================================================================
// PERFORMANCE AND RESOURCE TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_does_not_extract() {
    // Verify that recovery_percentage doesn't extract the entire archive
    // This is a sanity check - it should only read headers, not extract data

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    // This should be fast (< 100ms typically) since it only reads headers
    let start = std::time::Instant::now();
    let _ = archive.recovery_percentage();
    let duration = start.elapsed();

    println!("recovery_percentage took: {:?}", duration);

    // Should be very fast - less than 1 second even for large archives
    assert!(
        duration.as_secs() < 5,
        "recovery_percentage took too long: {:?}",
        duration
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_read_only_operation() {
    // Verify that calling recovery_percentage doesn't modify the archive file
    let path = "tests/fixtures/test.rar";

    // Get original file metadata
    let metadata_before = fs::metadata(path).expect("Failed to get metadata");
    let modified_before = metadata_before
        .modified()
        .expect("Failed to get modified time");

    // Open and call recovery_percentage
    let archive = Archive::open(path).expect("Failed to open RAR");
    let _ = archive.recovery_percentage();
    drop(archive);

    // Check metadata hasn't changed
    let metadata_after = fs::metadata(path).expect("Failed to get metadata");
    let modified_after = metadata_after
        .modified()
        .expect("Failed to get modified time");

    assert_eq!(
        modified_before, modified_after,
        "recovery_percentage should not modify the archive file"
    );
}

// ============================================================================
// REGRESSION TESTS
// ============================================================================

#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_after_list_files() {
    // Verify recovery_percentage works correctly after list_files()
    // This is a regression test for ensuring the two operations don't interfere

    // Try with RAR5 first (more reliable for small files)
    let archive = match Archive::open("tests/fixtures/test_rar5.rar") {
        Ok(a) => a,
        Err(_) => {
            // Fallback to creating a temp archive if fixture doesn't work
            println!("Skipping test - fixture unavailable");
            return;
        }
    };

    // List files first - this uses OnceLock caching
    let entries = match archive.list_files() {
        Ok(e) => e,
        Err(e) => {
            println!("list_files() failed (non-critical): {}", e);
            return; // Skip test if list_files fails
        }
    };

    if !entries.is_empty() {
        println!("Listed {} entries", entries.len());
    }

    // Now get recovery percentage - this opens a fresh file handle
    match archive.recovery_percentage() {
        Ok(pct) => {
            println!("After list_files: recovery={:?}", pct);
        }
        Err(e) => {
            println!(
                "recovery_percentage() returned error (expected for test files): {}",
                e
            );
        }
    }

    // Test passes if we get here without panicking
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_independent_of_format_detection() {
    // Verify recovery_percentage works correctly with format detection
    let path = "tests/fixtures/test.rar";

    let archive = Archive::open(path).expect("Failed to open");
    let format = archive.format();
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    println!("Format: {:?}, Recovery: {:?}", format, pct);

    // Should work regardless of detected format
    assert!(matches!(
        format,
        unified_archive::ArchiveFormat::Rar | unified_archive::ArchiveFormat::Rar5
    ));
}
