//! Tests for password-protected archive handling (Phase 2.6)
//!
//! Verifies password detection, correct password extraction, and error handling
//! for encrypted archives across different formats.

#[path = "common/mod.rs"]
mod common;

// Every test in this file is `rar-support`-gated, so under
// `--no-default-features` the file compiles to nothing and each of these
// would dangle. That profile is a release-gate lane (AD-0070) run under
// `-D warnings`, where a dangling import is a build failure rather than
// noise — hence the gates rather than an `#[allow]`.
#[cfg(feature = "rar-support")]
use std::fs;
#[cfg(feature = "rar-support")]
use std::path::PathBuf;
#[cfg(feature = "rar-support")]
use unified_archive::{Archive, ArchiveError};

/// Helper to get test fixtures directory
#[cfg(feature = "rar-support")]
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_detect_encrypted_rar_archive() {
    // Test that is_encrypted() correctly detects encrypted RAR archives
    // Note: Using test_encrypted_data.rar (data encrypted, headers not encrypted)
    let archive = Archive::open(fixtures_dir().join("test_encrypted_data.rar"))
        .expect("Failed to open archive");

    let is_encrypted = archive.is_encrypted().expect("Failed to check encryption");
    assert!(
        is_encrypted,
        "Expected is_encrypted() to return true for encrypted RAR archive"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_encrypted_rar_with_correct_password() {
    // Test opening and extracting encrypted RAR with correct password
    // Note: Using test_encrypted_data.rar (data encrypted, headers not encrypted)
    let archive =
        Archive::open_encrypted(fixtures_dir().join("test_encrypted_data.rar"), "test123")
            .expect("Failed to open encrypted archive");

    // List files should work
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty(), "Expected entries in encrypted archive");

    // Check that entries are marked as encrypted
    let has_encrypted = entries.iter().any(|e| e.is_encrypted);
    assert!(has_encrypted, "Expected entries to be marked as encrypted");

    // Extract should work
    let temp_dest = std::env::temp_dir().join("encrypted_rar_test");
    fs::create_dir_all(&temp_dest).ok();

    let options = common::default_extraction_options(temp_dest.clone());

    let result = archive.extract_all(options);
    fs::remove_dir_all(&temp_dest).ok();

    assert!(
        result.is_ok(),
        "Expected successful extraction with correct password: {:?}",
        result
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_encrypted_rar_with_wrong_password() {
    // A wrong password is not rejected at open time: password validation is
    // deferred to extraction by design (AD 0014). The contract asserted here is
    // that opening still succeeds and that extraction then fails.
    let archive = Archive::open_encrypted(
        fixtures_dir().join("test_encrypted_data.rar"),
        "wrongpassword",
    )
    .expect("open_encrypted defers password validation and must succeed (AD 0014)");

    let temp_dest = std::env::temp_dir().join("wrong_password_test");
    fs::create_dir_all(&temp_dest).ok();

    let options = common::default_extraction_options(temp_dest.clone());

    let result = archive.extract_all(options);
    fs::remove_dir_all(&temp_dest).ok();

    // Extraction with the wrong password must fail.
    let err = result.expect_err("extraction must fail with a wrong password");
    match err {
        ArchiveError::Password { message } => {
            assert!(
                message.contains("password") || message.contains("Password"),
                "Expected password error message, got: {}",
                message
            );
        }
        other => {
            // Format or corruption errors are also acceptable (different backends)
            println!("Got error (acceptable): {:?}", other);
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_encrypted_rar_without_password() {
    // Opening a RAR with non-encrypted headers succeeds even without a password;
    // password validation is deferred to extraction (AD 0014).
    let archive = Archive::open(fixtures_dir().join("test_encrypted_data.rar"))
        .expect("opening an encrypted RAR without a password must succeed for listing");

    let temp_dest = std::env::temp_dir().join("no_password_test");
    fs::create_dir_all(&temp_dest).ok();

    let options = common::default_extraction_options(temp_dest.clone());

    let result = archive.extract_all(options);
    fs::remove_dir_all(&temp_dest).ok();

    // Extraction without a password must fail.
    let err = result.expect_err("extraction without a password must fail");
    match err {
        ArchiveError::Password { message } => {
            assert!(
                message.contains("password")
                    || message.contains("Password")
                    || message.contains("required"),
                "Expected password required message, got: {}",
                message
            );
        }
        other => {
            println!("Got error (may be acceptable): {:?}", other);
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_password_does_not_leak_in_errors() {
    // Verify that passwords are not exposed in error messages.
    let archive = Archive::open_encrypted(
        fixtures_dir().join("test_encrypted_data.rar"),
        "secretpassword123",
    )
    .expect("open_encrypted defers password validation and must succeed (AD 0014)");

    // Force an error by trying to extract a non-existent file.
    let temp_dest = std::env::temp_dir().join("password_leak_test");
    fs::create_dir_all(&temp_dest).ok();

    let options = common::default_extraction_options(temp_dest.clone());

    let outcome = archive.extract_file("nonexistent.txt", options);
    fs::remove_dir_all(&temp_dest).ok();

    // Extracting a non-existent entry must fail, and the password must never
    // appear in the resulting error message.
    let err = outcome.expect_err("extracting a non-existent entry must fail");
    let error_string = format!("{:?}", err);
    assert!(
        !error_string.contains("secretpassword123"),
        "Password leaked in error message: {}",
        error_string
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_encrypted_metadata_available_without_password() {
    // File list and metadata are accessible without a password for RAR archives
    // with non-encrypted headers.
    let archive = Archive::open(fixtures_dir().join("test_encrypted_data.rar"))
        .expect("opening an encrypted RAR with non-encrypted headers must succeed");

    // RAR allows listing files without a password.
    let entries = archive
        .list_files()
        .expect("listing an encrypted RAR with non-encrypted headers must succeed");

    assert!(
        !entries.is_empty(),
        "Should be able to list encrypted archive files"
    );

    // The encryption flag must be set on the encrypted entries.
    let has_encrypted = entries.iter().any(|e| e.is_encrypted);
    assert!(
        has_encrypted,
        "Should detect that archive contains encrypted files"
    );
}
