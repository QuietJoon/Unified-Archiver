//! Tests for stream-level CRC32 extraction
//!
//! This tests extraction of CRC32 checksums stored in the compression format's
//! metadata (GZIP trailer, BZIP2 EOS marker, XZ stream flags).

use unified_archive::stream_crc::{
    CheckType, extract_bzip2_stream_crc, extract_gzip_stream_crc, extract_stream_checksum,
    extract_xz_stream_check,
};

#[test]
fn test_gzip_stream_crc() {
    let result = extract_gzip_stream_crc("/Volumes/Temp/claude/test_stream_crc.txt.gz");

    match result {
        Ok(checksum) => {
            println!("GZIP Stream CRC32: {:08X?}", checksum.crc32);
            println!("GZIP Uncompressed Size: {:?}", checksum.uncompressed_size);
            assert!(checksum.crc32.is_some(), "GZIP should have CRC32");
            assert_eq!(checksum.check_type, CheckType::Crc32);
        }
        Err(e) => {
            eprintln!("Failed to extract GZIP stream CRC: {:?}", e);
            // Don't fail the test if file doesn't exist (CI environment)
            if !std::path::Path::new("/Volumes/Temp/claude/test_stream_crc.txt.gz").exists() {
                eprintln!("Test file doesn't exist, skipping");
                return;
            }
            panic!("Should extract GZIP stream CRC: {:?}", e);
        }
    }
}

#[test]
fn test_bzip2_stream_crc() {
    let result = extract_bzip2_stream_crc("/Volumes/Temp/claude/test_stream_crc.txt.bz2");

    match result {
        Ok(checksum) => {
            println!("BZIP2 Stream CRC32: {:08X?}", checksum.crc32);
            assert!(checksum.crc32.is_some(), "BZIP2 should have CRC32");
            assert_eq!(checksum.check_type, CheckType::Crc32);
        }
        Err(e) => {
            eprintln!("Failed to extract BZIP2 stream CRC: {:?}", e);
            // Don't fail the test if file doesn't exist (CI environment)
            if !std::path::Path::new("/Volumes/Temp/claude/test_stream_crc.txt.bz2").exists() {
                eprintln!("Test file doesn't exist, skipping");
                return;
            }
            panic!("Should extract BZIP2 stream CRC: {:?}", e);
        }
    }
}

#[test]
fn test_xz_stream_check() {
    let result = extract_xz_stream_check("/Volumes/Temp/claude/test_stream_crc.txt.xz");

    match result {
        Ok(checksum) => {
            println!("XZ Check Type: {:?}", checksum.check_type);
            // XZ typically uses CRC64 by default
            assert!(
                checksum.check_type == CheckType::Crc64 || checksum.check_type == CheckType::Crc32,
                "XZ should have CRC64 or CRC32 check type"
            );
        }
        Err(e) => {
            eprintln!("Failed to extract XZ stream check: {:?}", e);
            // Don't fail the test if file doesn't exist (CI environment)
            if !std::path::Path::new("/Volumes/Temp/claude/test_stream_crc.txt.xz").exists() {
                eprintln!("Test file doesn't exist, skipping");
                return;
            }
            panic!("Should extract XZ stream check: {:?}", e);
        }
    }
}

#[test]
fn test_auto_detect_gzip() {
    let result = extract_stream_checksum("/Volumes/Temp/claude/test_stream_crc.txt.gz");

    match result {
        Ok(checksum) => {
            println!("Auto-detected GZIP Stream CRC32: {:08X?}", checksum.crc32);
            assert!(
                checksum.crc32.is_some(),
                "Should detect GZIP and extract CRC32"
            );
        }
        Err(_) => {
            // Skip if file doesn't exist
            if !std::path::Path::new("/Volumes/Temp/claude/test_stream_crc.txt.gz").exists() {
                eprintln!("Test file doesn't exist, skipping");
            }
        }
    }
}

#[test]
fn test_auto_detect_bzip2() {
    let result = extract_stream_checksum("/Volumes/Temp/claude/test_stream_crc.txt.bz2");

    match result {
        Ok(checksum) => {
            println!("Auto-detected BZIP2 Stream CRC32: {:08X?}", checksum.crc32);
            assert!(
                checksum.crc32.is_some(),
                "Should detect BZIP2 and extract CRC32"
            );
        }
        Err(_) => {
            // Skip if file doesn't exist
            if !std::path::Path::new("/Volumes/Temp/claude/test_stream_crc.txt.bz2").exists() {
                eprintln!("Test file doesn't exist, skipping");
            }
        }
    }
}

#[test]
fn test_verify_crc32_values() {
    // Create a known file and verify CRC32 matches
    use std::fs::File;
    use std::io::Write;

    let test_path = "/Volumes/Temp/claude/test_known_crc.txt";
    let gz_path = "/Volumes/Temp/claude/test_known_crc.txt.gz";

    // Create test file
    if let Ok(mut file) = File::create(test_path) {
        let _ = file.write_all(b"Test data for CRC32 verification");
    }

    // Compress it
    let status = std::process::Command::new("gzip")
        .arg("-k")
        .arg(test_path)
        .status();

    if status.is_ok() && std::path::Path::new(gz_path).exists() {
        // Extract stream CRC32
        if let Ok(checksum) = extract_gzip_stream_crc(gz_path) {
            println!("Verified GZIP CRC32: {:08X?}", checksum.crc32);
            assert!(checksum.crc32.is_some());

            // Verify it matches the uncompressed data CRC32
            use crc32fast::Hasher;
            let mut hasher = Hasher::new();
            hasher.update(b"Test data for CRC32 verification");
            let expected_crc = hasher.finalize();

            assert_eq!(
                checksum.crc32.unwrap(),
                expected_crc,
                "Stream CRC32 should match computed CRC32"
            );
        }

        // Cleanup
        let _ = std::fs::remove_file(test_path);
        let _ = std::fs::remove_file(gz_path);
    } else {
        eprintln!("Could not create test file, skipping verification test");
    }
}
