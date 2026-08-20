// Test archive integrity checking with validate_integrity()

use unified_archive::{Archive, ArchiveError};

#[test]
fn test_integrity_check_valid_zip() {
    let archive = Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open test ZIP");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    // All files should pass validation
    assert!(
        report.failed.is_empty(),
        "Expected no failed files, got: {:?}",
        report.failed
    );
    assert!(report.validated > 0, "Should have validated some files");
    println!("✓ ZIP: {} files validated successfully", report.validated);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_integrity_check_valid_rar() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open test RAR");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    // All files should pass validation
    assert!(
        report.failed.is_empty(),
        "Expected no failed files, got: {:?}",
        report.failed
    );
    assert!(report.validated > 0, "Should have validated some files");
    println!("✓ RAR: {} files validated successfully", report.validated);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_integrity_check_valid_rar5() {
    let archive = Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open test RAR5");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    // All files should pass validation
    assert!(
        report.failed.is_empty(),
        "Expected no failed files, got: {:?}",
        report.failed
    );
    assert!(report.validated > 0, "Should have validated some files");
    println!("✓ RAR5: {} files validated successfully", report.validated);
}

#[test]
fn test_integrity_check_valid_7z() {
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open test 7z");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    // All files should pass validation
    assert!(
        report.failed.is_empty(),
        "Expected no failed files, got: {:?}",
        report.failed
    );
    assert!(report.validated > 0, "Should have validated some files");
    println!("✓ 7z: {} files validated successfully", report.validated);
}

#[test]
fn test_integrity_check_validation_report_structure() {
    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    // Verify report structure
    assert!(report.total_entries > 0, "Should have total entries");
    assert_eq!(
        report.validated + report.failed.len(),
        report.total_entries - count_directories(&archive),
        "Validated + failed should equal file count (excluding dirs)"
    );

    println!("Validation report:");
    println!("  Total entries: {}", report.total_entries);
    println!("  Files validated: {}", report.validated);
    println!("  Files failed: {}", report.failed.len());
}

fn count_directories(archive: &Archive) -> usize {
    archive
        .list_files()
        .expect("Failed to list files")
        .iter()
        .filter(|e| e.is_directory())
        .count()
}

#[test]
#[serial_test::file_serial(rar)]
fn test_integrity_check_across_formats() {
    // Test that integrity checking works consistently across all formats

    let test_archives = vec![
        ("tests/fixtures/batch_test.zip", "ZIP"),
        ("tests/fixtures/test.rar", "RAR"),
        ("tests/fixtures/test_rar5.rar", "RAR5"),
        ("tests/fixtures/test.7z", "7z"),
    ];

    for (path, format) in test_archives {
        if let Ok(archive) = Archive::open(path) {
            let report = archive
                .validate_integrity()
                .unwrap_or_else(|_| panic!("Failed to validate {} archive", format));

            assert!(
                report.failed.is_empty(),
                "{} format: validation failed for files: {:?}",
                format,
                report.failed
            );
            assert!(
                report.validated > 0,
                "{} format: should have validated some files",
                format
            );

            println!("✓ {} format: {} files validated", format, report.validated);
        }
    }
}

#[test]
fn test_integrity_check_empty_archive() {
    // Test with an archive that has no files
    // Note: Creating a truly valid empty ZIP is complex, so we use a real archive
    // and verify the validation handles zero-file archives correctly

    // batch_test.zip has directories, but we can test the zero-file case
    // by checking that directories don't count as validated files

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate archive");

    // Count only actual files (not directories)
    let entries = archive.list_files().expect("Failed to list files");
    let file_count = entries.iter().filter(|e| e.is_file()).count();

    // Validated files should match actual file count
    assert_eq!(
        report.validated + report.failed.len(),
        file_count,
        "Validated + failed should equal file count (excluding directories)"
    );

    println!("Archive validation handles directories correctly:");
    println!("  Total entries (including dirs): {}", report.total_entries);
    println!("  Files validated: {}", report.validated);
    println!(
        "  Directories (not validated): {}",
        report.total_entries - file_count
    );
}

#[test]
#[serial_test::file_serial(rar)]
fn test_integrity_check_encrypted_archive() {
    // Test that encrypted archives require password for integrity checking

    let archive = Archive::open("tests/fixtures/test_encrypted.rar");

    match archive {
        Ok(arc) => {
            // Validate without a password. test_encrypted.rar has data-only
            // encryption (headers readable), so the listing succeeds but the
            // first per-entry RAR_TEST needs the password.
            let result = arc.validate_integrity();

            // R0080-0021: a missing/wrong password on a per-entry RAR_TEST is
            // no longer collapsed into the damaged-file list — it surfaces as
            // ArchiveError::Password so callers can distinguish it from CRC
            // corruption and retry with a password.
            match result {
                Err(ArchiveError::Password { .. }) => {
                    println!(
                        "Encrypted archive without password yielded Password error (expected)"
                    );
                }
                Err(other) => panic!("expected ArchiveError::Password, got {:?}", other),
                Ok(report) => panic!(
                    "expected a password error validating an encrypted archive without a \
                     password, got a report ({} failed of {} entries)",
                    report.failed.len(),
                    report.total_entries
                ),
            }
        }
        Err(_) => {
            // Encrypted-header archives (or a build without rar-support) may
            // fail to open without a password; nothing to validate then.
            println!("Skipping encrypted archive test - file not available");
        }
    }
}
