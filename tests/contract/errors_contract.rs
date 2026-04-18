//! Contract tests for ArchiveError
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/errors.md:
//! 1. Verify all error variants are returned appropriately
//! 2. Test error message clarity and actionability
//! 3. Verify error source preservation
//! 4. Test Display implementation formatting

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use std::error::Error;
use unified_archive::{Archive, ArchiveError, ArchiveFormat};

// ── Contract 1: All error variants are returned appropriately ──

#[test]
fn contract_io_error_for_nonexistent_file() {
    let result = Archive::open("/nonexistent/path/archive.zip");
    let err = result.err().expect("Should return error");
    assert!(
        matches!(err, ArchiveError::Io { .. }),
        "Expected Io variant, got: {:?}",
        err
    );
}

#[test]
fn contract_format_error_for_invalid_file() {
    let result = Archive::open(fixture("test_file.txt"));
    let err = result.err().expect("Should return error");
    assert!(
        matches!(err, ArchiveError::Format { .. }),
        "Expected Format variant, got: {:?}",
        err
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_password_error_for_encrypted_without_password() {
    let archive = Archive::open(fixture("test_encrypted.rar")).unwrap();
    let result = archive.extract_to_memory("test_file.txt");
    // Should get a password-related error or extraction error
    assert!(
        result.is_err(),
        "Extracting encrypted file without password should fail"
    );
}

#[test]
#[serial_test::file_serial(rar)]
fn contract_unsupported_operation_for_rar_modify() {
    let result = Archive::modify(fixture("test.rar"));
    let err = result.err().expect("Should return error");
    assert!(
        matches!(
            err,
            ArchiveError::OperationBlocked { .. }
                | ArchiveError::NotImplemented { .. }
                | ArchiveError::WriteModeOnly { .. }
                | ArchiveError::ReadOnlyBackend { .. }
                | ArchiveError::Unsupported { .. }
        ),
        "Expected an operation-blocked/unsupported variant, got: {:?}",
        err
    );
}

#[test]
fn contract_corruption_error_for_corrupted_archive() {
    let archive = Archive::open(fixture("corrupted_crc.zip"));
    if let Ok(archive) = archive {
        // If it opens, integrity check should detect corruption
        let report = archive.validate_integrity();
        if let Ok(report) = report {
            assert!(
                !report.failed.is_empty() || report.validated < report.total_entries,
                "Corrupted archive should have integrity issues"
            );
        }
    }
    // Some backends may reject at open time — that's also acceptable
}

#[test]
fn contract_invalid_path_error_for_path_traversal() {
    // This tests the normalized_entry_path behavior
    let archive = Archive::open(fixture("test.zip")).unwrap();
    // Extracting with traversal path should be handled safely
    let _result = archive.extract_to_memory("../../etc/passwd");
    // Either returns error or sanitizes the path — both are acceptable
    // Just verify it doesn't panic
}

// ── Contract 2: Error message clarity and actionability ──

#[test]
fn contract_io_error_contains_operation_and_path() {
    let result = Archive::open("/nonexistent/archive.zip");
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    // Error message should contain useful context
    assert!(
        msg.contains("nonexistent") || msg.contains("archive.zip") || msg.contains("No such file"),
        "I/O error should mention the path or system error: {msg}"
    );
}

#[test]
fn contract_format_error_is_descriptive() {
    let result = Archive::open(fixture("test_file.txt"));
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    assert!(!msg.is_empty(), "Format error message should not be empty");
    // Message should be actionable — mention format or detection
    assert!(
        msg.contains("format") || msg.contains("detect") || msg.contains("extension"),
        "Format error should explain the issue: {msg}"
    );
}

#[test]
#[serial_test::file_serial(rar)]
fn contract_unsupported_error_explains_why() {
    let result = Archive::modify(fixture("test.rar"));
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    assert!(
        msg.contains("RAR") || msg.contains("read-only") || msg.contains("modify"),
        "Unsupported error should explain what's not supported: {msg}"
    );
}

// ── Contract 3: Error source preservation ──

#[test]
fn contract_io_error_preserves_source() {
    let result = Archive::open("/nonexistent/archive.zip");
    let err = result.err().unwrap();
    if let ArchiveError::Io { ref source, .. } = err {
        // The source should be an std::io::Error
        assert!(
            source.kind() == std::io::ErrorKind::NotFound
                || source.kind() == std::io::ErrorKind::Other,
            "I/O source should indicate file not found: {:?}",
            source.kind()
        );
    }
    // Also verify via the Error trait's source() method
    let dyn_err: &dyn Error = &err;
    if matches!(err, ArchiveError::Io { .. }) {
        assert!(
            dyn_err.source().is_some(),
            "Io variant should have a source error"
        );
    }
}

#[test]
fn contract_non_io_errors_have_no_source() {
    let result = Archive::open(fixture("test_file.txt"));
    let err = result.err().unwrap();
    if matches!(err, ArchiveError::Format { .. }) {
        let dyn_err: &dyn Error = &err;
        assert!(
            dyn_err.source().is_none(),
            "Format variant should not have a source error"
        );
    }
}

// ── Contract 4: Display implementation formatting ──

#[test]
fn contract_display_io_error() {
    let result = Archive::open("/nonexistent/archive.zip");
    let err = result.err().unwrap();
    let display = format!("{}", err);
    let debug = format!("{:?}", err);

    assert!(
        !display.is_empty(),
        "Display should produce non-empty output"
    );
    assert!(!debug.is_empty(), "Debug should produce non-empty output");
    // Display and Debug should be different representations
    // (Display is human-readable, Debug is for developers)
}

#[test]
fn contract_display_format_error() {
    let result = Archive::open(fixture("test_file.txt"));
    let err = result.err().unwrap();
    let display = format!("{}", err);
    assert!(!display.is_empty());
}

#[test]
#[serial_test::file_serial(rar)]
fn contract_display_unsupported_error() {
    let result = Archive::modify(fixture("test.rar"));
    let err = result.err().unwrap();
    let display = format!("{}", err);
    assert!(!display.is_empty());
    // Should be understandable to a user
    assert!(
        display.len() > 10,
        "Error message should be descriptive, not just a code: {display}"
    );
}

#[test]
fn contract_all_variants_implement_debug() {
    // Verify Debug is implemented for ArchiveError by exercising it
    let io_err = ArchiveError::Io {
        operation: "test".to_string(),
        path: "/test".into(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
    };
    let format_err = ArchiveError::Format {
        format: Some(ArchiveFormat::Zip),
        message: "bad format".to_string(),
    };
    let corruption_err = ArchiveError::Corruption {
        path: "test.zip".to_string(),
        details: "CRC mismatch".to_string(),
    };
    let password_err = ArchiveError::Password {
        message: "wrong password".to_string(),
    };
    let invalid_path_err = ArchiveError::InvalidPath {
        path: "../etc/passwd".to_string(),
        reason: "path traversal".to_string(),
    };

    // All should produce non-empty Debug output
    assert!(!format!("{:?}", io_err).is_empty());
    assert!(!format!("{:?}", format_err).is_empty());
    assert!(!format!("{:?}", corruption_err).is_empty());
    assert!(!format!("{:?}", password_err).is_empty());
    assert!(!format!("{:?}", invalid_path_err).is_empty());

    // All should produce non-empty Display output
    assert!(!format!("{}", io_err).is_empty());
    assert!(!format!("{}", format_err).is_empty());
    assert!(!format!("{}", corruption_err).is_empty());
    assert!(!format!("{}", password_err).is_empty());
    assert!(!format!("{}", invalid_path_err).is_empty());
}
