//! Integration tests for SFX detection (SC-016)
//!
//! Verifies heuristic-based SFX detection for common archive formats.
//!
//! Test Strategy:
//! - Create synthetic SFX test cases (shell script + ZIP/RAR/7z)
//! - Verify detection succeeds for all SFX files
//! - Verify correct stub type, format, and offset detection

use std::io::Write;
use tempfile::NamedTempFile;
use unified_archive::{Archive, ArchiveFormat, StubType};

#[test]
fn test_shell_script_zip_sfx() {
    // Create a synthetic shell script SFX with embedded ZIP
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub
    let stub = b"#!/bin/sh\n\
                 echo 'Self-extracting archive'\n\
                 echo 'Extracting...'\n\
                 # Archive data follows\n";

    // Padding to make it more realistic (align to 512 bytes)
    let padding_size = 512 - (stub.len() % 512);
    let padding = vec![0u8; padding_size];

    // ZIP signature (PK\x03\x04) + minimal ZIP header
    let zip_header = b"PK\x03\x04\x14\x00\x00\x00\x08\x00";
    let zip_data = [0u8; 100]; // Additional ZIP data

    // Write complete SFX
    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_header).unwrap();
    temp.write_all(&zip_data).unwrap();
    temp.flush().unwrap();

    // Detect SFX
    let result = Archive::detect_sfx(temp.path()).unwrap();

    // Verify detection
    assert!(result.is_sfx, "Should detect shell script SFX");
    assert_eq!(result.stub_type, Some(StubType::ScriptInterpreter));
    assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    assert!(result.data_offset.is_some());
    assert!(
        result.confidence >= 0.9,
        "Confidence should be >= 0.9, got {}",
        result.confidence
    );
}

#[test]
fn test_shell_script_rar_sfx() {
    // Create a synthetic shell script SFX with embedded RAR
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub (makeself-style)
    let stub = b"#!/bin/sh\n\
                 # This is a makeself-style self-extracting archive\n\
                 echo 'Extracting archive...'\n";

    // Padding
    let padding = vec![0u8; 400];

    // RAR 5.0 signature
    let rar_signature = b"Rar!\x1a\x07\x01\x00";
    let rar_data = [0u8; 100];

    // Write complete SFX
    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(rar_signature).unwrap();
    temp.write_all(&rar_data).unwrap();
    temp.flush().unwrap();

    // Detect SFX
    let result = Archive::detect_sfx(temp.path()).unwrap();

    // Verify detection
    assert!(result.is_sfx, "Should detect RAR5 SFX");
    assert_eq!(result.stub_type, Some(StubType::ScriptInterpreter));
    assert_eq!(result.archive_format, Some(ArchiveFormat::Rar5));
    assert!(result.confidence >= 0.9);
}

#[test]
fn test_shell_script_7z_sfx() {
    // Create a synthetic shell script SFX with embedded 7z
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub
    let stub = b"#!/bin/bash\n\
                 ARCHIVE_OFFSET=512\n\
                 tail -c +$ARCHIVE_OFFSET $0 | 7z x -si\n\
                 exit 0\n";

    // Padding to offset 512
    let padding_size = 512 - stub.len();
    let padding = vec![0u8; padding_size];

    // 7z signature
    let sevenz_signature = b"7z\xbc\xaf\x27\x1c";
    let sevenz_data = [0u8; 100];

    // Write complete SFX
    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(sevenz_signature).unwrap();
    temp.write_all(&sevenz_data).unwrap();
    temp.flush().unwrap();

    // Detect SFX
    let result = Archive::detect_sfx(temp.path()).unwrap();

    // Verify detection
    assert!(result.is_sfx, "Should detect 7z SFX");
    assert_eq!(result.stub_type, Some(StubType::ScriptInterpreter));
    assert_eq!(result.archive_format, Some(ArchiveFormat::SevenZip));
    assert_eq!(result.data_offset, Some(512));
    assert!(result.confidence >= 0.9);
}

#[test]
fn test_multiple_signatures_iterates_candidates() {
    // Test that when multiple archive signatures are found, all candidates are
    // tried (per AD 0015) and the first one that validates is returned.
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub
    let stub = b"#!/bin/sh\necho test\n";
    let padding = vec![0u8; 100];

    // First signature: ZIP at offset ~130
    let zip_sig = b"PK\x03\x04";
    let zip_data = [0u8; 50];

    // Second signature: RAR at offset ~190
    let rar_sig = b"Rar!\x1a\x07\x00";
    let rar_data = [0u8; 50];

    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_sig).unwrap();
    temp.write_all(&zip_data).unwrap();
    temp.write_all(rar_sig).unwrap();
    temp.write_all(&rar_data).unwrap();
    temp.flush().unwrap();

    // Detect SFX — detection iterates all candidates and returns
    // the first one that passes validation (earliest offset wins
    // when no candidate fully validates via backend parse).
    let result = Archive::detect_sfx(temp.path()).unwrap();

    assert!(result.is_sfx);
    // ZIP is the first candidate to be tried (smallest offset)
    assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
}

#[test]
fn test_sfx_summary_message() {
    // Test the human-readable summary
    let mut temp = NamedTempFile::new().unwrap();

    let stub = b"#!/bin/sh\necho test\n";
    let padding = vec![0u8; 100];
    let zip_sig = b"PK\x03\x04";
    let zip_data = [0u8; 100];

    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_sig).unwrap();
    temp.write_all(&zip_data).unwrap();
    temp.flush().unwrap();

    let result = Archive::detect_sfx(temp.path()).unwrap();
    let summary = result.summary();

    assert!(summary.contains("SFX detected"));
    assert!(summary.contains("Script interpreter"));
    assert!(summary.contains("Zip"));
}

#[test]
fn test_extract_stub() {
    // Test stub extraction for security analysis
    let mut temp = NamedTempFile::new().unwrap();

    let stub = b"#!/bin/sh\necho 'Stub code'\n";
    let padding = vec![0u8; 100];
    let zip_sig = b"PK\x03\x04";
    let zip_data = [0u8; 100];

    temp.write_all(stub).unwrap();
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_sig).unwrap();
    temp.write_all(&zip_data).unwrap();
    temp.flush().unwrap();

    // Detect and extract stub
    let result = Archive::detect_sfx(temp.path()).unwrap();
    let stub_data = Archive::extract_stub(temp.path(), &result).unwrap();

    // Verify stub size
    let expected_stub_size = stub.len() + padding.len();
    assert_eq!(stub_data.len(), expected_stub_size);

    // Verify stub starts with shebang
    assert_eq!(&stub_data[0..2], b"#!");
}

#[test]
fn test_open_at_offset_with_real_zip() {
    // Build a real SFX-style file: shell-script stub + contents of test.zip.
    // After concatenation, open_at_offset should hand back a working Archive.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");
    let zip_bytes = std::fs::read(&fixture).expect("read test.zip");

    let mut temp = NamedTempFile::new().unwrap();
    let stub = b"#!/bin/sh\necho 'stub'\n";
    temp.write_all(stub).unwrap();
    let padding = vec![0u8; 500];
    temp.write_all(&padding).unwrap();
    let offset = (stub.len() + padding.len()) as u64;
    temp.write_all(&zip_bytes).unwrap();
    temp.flush().unwrap();

    let archive = Archive::open_at_offset(temp.path(), offset)
        .expect("open_at_offset should succeed for real SFX payload");
    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
}

#[test]
fn test_open_at_offset_zero_matches_plain_open() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");

    let archive = Archive::open_at_offset(&fixture, 0).expect("open_at_offset(0)");
    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
}

#[test]
fn test_open_at_offset_past_eof_errors() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");
    let len = std::fs::metadata(&fixture).unwrap().len();

    let result = Archive::open_at_offset(&fixture, len);
    assert!(result.is_err(), "should reject offset==len");
}

#[test]
fn test_open_sfx_with_real_zip_payload() {
    // End-to-end: detect_sfx → open_sfx returns a working archive.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");
    let zip_bytes = std::fs::read(&fixture).expect("read test.zip");

    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(b"#!/bin/sh\necho 'sfx stub'\n# payload follows\n")
        .unwrap();
    // Pad so the detector sees a realistic stub size
    let padding = vec![0u8; 500];
    temp.write_all(&padding).unwrap();
    temp.write_all(&zip_bytes).unwrap();
    temp.flush().unwrap();

    let archive = Archive::open_sfx(temp.path())
        .expect("open_sfx should succeed once open_at_offset is wired");
    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
}

#[cfg(target_os = "linux")]
#[test]
fn test_elf_binary_detection() {
    // Note: This test requires a real ELF binary
    // We'll test with /bin/ls as a non-SFX executable
    let result = Archive::detect_sfx("/bin/ls").unwrap();

    // /bin/ls is an ELF but not an SFX
    assert!(!result.is_sfx, "/bin/ls should not be detected as SFX");
}

#[cfg(target_os = "macos")]
#[test]
fn test_macho_binary_detection() {
    // Note: This test requires a real Mach-O binary
    // We'll test with /bin/ls as a non-SFX executable
    let result = Archive::detect_sfx("/bin/ls").unwrap();

    // /bin/ls is Mach-O but not an SFX
    assert!(!result.is_sfx, "/bin/ls should not be detected as SFX");
}
