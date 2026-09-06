//! Integration tests for SFX false positive prevention (SC-018)
//!
//! Verifies that the library achieves zero false positives by correctly
//! rejecting non-SFX files (regular executables, plain archives, text files).
//!
//! Test Strategy:
//! - Test regular executables without embedded archives
//! - Test plain archive files (should return not_sfx, not error)
//! - Test text files and random data
//! - Test archives with coincidental signature patterns

// These pin what must NOT be detected as an SFX, which still requires the
// detection pipeline to exist (AD 0058 format features).
#![cfg(feature = "sfx")]

use std::io::Write;
use tempfile::NamedTempFile;
use unified_archive::{Archive, SfxConfidence};

#[test]
fn test_plain_text_not_sfx() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(b"Just plain text, not an executable or archive\n")
        .unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(!result.is_sfx(), "Plain text should not be detected as SFX");
    assert_eq!(result.confidence(), SfxConfidence::NotSfx);
}

#[test]
fn test_shell_script_without_archive_not_sfx() {
    // Shell script with shebang but no embedded archive
    let mut temp = NamedTempFile::new().unwrap();
    let script = b"#!/bin/bash\n\
                   echo 'Hello World'\n\
                   ls -la\n\
                   exit 0\n";
    temp.write_all(script).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(
        !result.is_sfx(),
        "Shell script without archive should not be SFX"
    );
}

#[test]
fn test_plain_zip_archive_not_sfx() {
    // Create a plain ZIP file (starts with PK\x03\x04)
    let mut temp = NamedTempFile::new().unwrap();
    let zip_signature = b"PK\x03\x04";
    let zip_header = [0x14, 0x00, 0x00, 0x00, 0x08, 0x00];
    let zip_data = [0u8; 100];

    temp.write_all(zip_signature).unwrap();
    temp.write_all(&zip_header).unwrap();
    temp.write_all(&zip_data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    // Plain ZIP is not an SFX because it's not an executable format
    assert!(
        !result.is_sfx(),
        "Plain ZIP archive should not be detected as SFX"
    );
}

#[test]
fn test_plain_rar_archive_not_sfx() {
    // Create a plain RAR file
    let mut temp = NamedTempFile::new().unwrap();
    let rar_signature = b"Rar!\x1a\x07\x00";
    let rar_data = [0u8; 100];

    temp.write_all(rar_signature).unwrap();
    temp.write_all(&rar_data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    // Plain RAR is not an SFX because it's not an executable format
    assert!(
        !result.is_sfx(),
        "Plain RAR archive should not be detected as SFX"
    );
}

#[test]
fn test_executable_with_fake_signature_at_end_not_sfx() {
    // R0070-0075: the unconditional 100-byte trailing-bytes gate was
    // removed in favour of per-format probes. ZIP requires at least
    // 30 bytes of archive data (local file header minimum). Use a
    // payload smaller than that so the per-format probe still
    // rejects this truncated input.
    let mut temp = NamedTempFile::new().unwrap();

    let stub = b"#!/bin/sh\necho test\n";
    let padding = vec![0u8; 400];
    let zip_sig = b"PK\x03\x04";
    // Total archive_data after the signature = 4 (sig) + 10 = 14 < 30
    let small_data = [0u8; 10];

    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_sig).unwrap();
    temp.write_all(&small_data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(
        !result.is_sfx(),
        "Signature too close to EOF should be rejected as false positive"
    );
}

#[test]
fn test_random_binary_data_not_sfx() {
    let mut temp = NamedTempFile::new().unwrap();
    // Random binary data that might contain coincidental patterns
    let random_data: Vec<u8> = (0..1000).map(|i| (i * 137 % 256) as u8).collect();
    temp.write_all(&random_data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(
        !result.is_sfx(),
        "Random binary data should not be detected as SFX"
    );
}

#[test]
fn test_partial_signature_not_sfx() {
    // File with partial/incomplete archive signature
    let mut temp = NamedTempFile::new().unwrap();
    let stub = b"#!/bin/sh\necho test\n";
    let padding = vec![0u8; 400];
    // Only "PK\x03" without the full ZIP signature
    let partial_sig = b"PK\x03";
    let data = [0u8; 200];

    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(partial_sig).unwrap();
    temp.write_all(&data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(
        !result.is_sfx(),
        "Partial signature should not trigger SFX detection"
    );
}

#[test]
fn test_string_pk_in_text_not_sfx() {
    // Text file containing "PK" as part of normal content
    let mut temp = NamedTempFile::new().unwrap();
    let text = b"#!/bin/sh\n\
                 # This script uses PKCS#11 for cryptography\n\
                 echo 'PK test'\n\
                 # PKI infrastructure notes\n";
    temp.write_all(text).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    // Should not detect because "PK" in text is not followed by \x03\x04
    assert!(!result.is_sfx(), "Text containing 'PK' should not be SFX");
}

#[test]
fn test_empty_file_not_sfx() {
    let temp = NamedTempFile::new().unwrap();
    // Empty file

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(!result.is_sfx(), "Empty file should not be detected as SFX");
}

#[test]
fn test_very_small_file_not_sfx() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(b"#!").unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(!result.is_sfx(), "Very small file should not be SFX");
}

#[test]
fn test_html_with_zip_in_comment_not_sfx() {
    // HTML file with "PK\x03\x04" in a comment or data
    let mut temp = NamedTempFile::new().unwrap();
    let html = b"<!DOCTYPE html>\n\
                 <html>\n\
                 <body>\n\
                 <!-- Discussing ZIP format: PK\x03\x04 is the signature -->\n\
                 <p>File formats</p>\n\
                 </body>\n\
                 </html>\n";
    temp.write_all(html).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    // HTML is not an executable format, so even with signature, not SFX
    assert!(
        !result.is_sfx(),
        "HTML with archive signature should not be SFX"
    );
}

#[test]
fn test_json_file_not_sfx() {
    let mut temp = NamedTempFile::new().unwrap();
    let json = br#"
    {
        "name": "test",
        "format": "ZIP",
        "signature": "PK\u0003\u0004"
    }
    "#;
    temp.write_all(json).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(!result.is_sfx(), "JSON file should not be detected as SFX");
}

#[test]
fn test_pdf_file_not_sfx() {
    let mut temp = NamedTempFile::new().unwrap();
    // PDF header
    let pdf_header = b"%PDF-1.4\n";
    let pdf_content = b"This is not really a PDF but starts like one\n";
    temp.write_all(pdf_header).unwrap();
    temp.write_all(pdf_content).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(!result.is_sfx(), "PDF file should not be detected as SFX");
}

#[test]
fn test_executable_with_no_archive_signature() {
    // Shell script executable with no archive embedded
    let mut temp = NamedTempFile::new().unwrap();
    let script = b"#!/usr/bin/env python3\n\
                   print('Hello from Python')\n\
                   import zipfile\n\
                   # This mentions zipfile but doesn't embed one\n";
    temp.write_all(script).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    assert!(
        !result.is_sfx(),
        "Executable without embedded archive should not be SFX"
    );
}

#[test]
fn test_archive_in_string_literal() {
    // Executable containing archive signature in a string literal
    let mut temp = NamedTempFile::new().unwrap();
    let script = b"#!/bin/sh\n\
                   # ZIP_SIGNATURE=\"PK\\x03\\x04\"\n\
                   echo 'Testing ZIP signature detection'\n";
    temp.write_all(script).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    // Escaped bytes in comment are not actual binary signature
    assert!(
        !result.is_sfx(),
        "Signature in comment should not trigger detection"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_common_linux_binaries_not_sfx() {
    // Test common Linux system binaries
    let binaries = ["/bin/ls", "/bin/cat", "/bin/echo", "/usr/bin/env"];

    for binary in &binaries {
        if std::path::Path::new(binary).exists() {
            let result = Archive::detect_sfx(binary);
            assert!(result.is_ok(), "Should handle {} without error", binary);
            assert!(!result.unwrap().is_sfx(), "{} should not be SFX", binary);
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_common_macos_binaries_not_sfx() {
    // Test common macOS system binaries
    let binaries = ["/bin/ls", "/bin/cat", "/bin/echo", "/usr/bin/env"];

    for binary in &binaries {
        if std::path::Path::new(binary).exists() {
            let result = Archive::detect_sfx(binary);
            assert!(result.is_ok(), "Should handle {} without error", binary);
            assert!(!result.unwrap().is_sfx(), "{} should not be SFX", binary);
        }
    }
}
