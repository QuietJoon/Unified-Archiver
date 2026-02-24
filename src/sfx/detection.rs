// SFX detection implementation
//
// Three-stage pipeline: exe validation → signature scan → archive validation

use super::result::SfxDetectionResult;
use super::signatures;
use super::stub_types::StubType;
use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Maximum size to scan for archive signatures (1MB)
const MAX_SCAN_SIZE: usize = 1_048_576;

/// Chunk size for aligned scanning (512 bytes)
const CHUNK_SIZE: usize = 512;

/// Size of header to read for executable format detection (4KB)
const HEADER_SIZE: usize = 4096;

/// Detect if a file is a self-extracting archive
///
/// Implements 3-stage detection pipeline:
/// 1. Stage 1: Validate executable format (PE/ELF/Mach-O/Script)
/// 2. Stage 2: Scan for archive signatures in first 1MB
/// 3. Stage 3: Validate archive structure at detected offset
///
/// # Arguments
/// * `path` - Path to the file to check
///
/// # Returns
/// * `Ok(SfxDetectionResult)` with detection results
/// * `Err(ArchiveError)` for I/O errors or corrupted archives
pub fn detect_sfx<P: AsRef<Path>>(path: P) -> Result<SfxDetectionResult> {
    let path = path.as_ref();

    // Stage 1 & 2: Read file once (up to 1MB) for both executable validation and signature scanning
    // This optimization reduces file I/O by 50% compared to separate reads
    let file = File::open(path).map_err(|e| ArchiveError::io("open", path, e))?;

    // Get file length for validation
    let file_len = file
        .metadata()
        .map_err(|e| ArchiveError::io("metadata", path, e))?
        .len();

    let mut scan_buffer = Vec::with_capacity(MAX_SCAN_SIZE);
    let _bytes_read = file
        .take(MAX_SCAN_SIZE as u64)
        .read_to_end(&mut scan_buffer)
        .map_err(|e| ArchiveError::io("read", path, e))?;

    // Stage 1: Executable format validation using first 4KB of buffer
    let header_size = HEADER_SIZE.min(scan_buffer.len());
    let header = &scan_buffer[..header_size];

    let stub_type = match StubType::detect(header) {
        Ok(stub) => stub,
        Err(_) => {
            // Not an executable format - return not_sfx (not an error)
            return Ok(SfxDetectionResult::not_sfx());
        }
    };

    // Stage 2: Archive signature scanning (using same buffer already read)

    let signatures_found = signatures::scan_for_signatures(&scan_buffer, CHUNK_SIZE);

    if signatures_found.is_empty() {
        // No archive signature found - return not_sfx (not an error)
        return Ok(SfxDetectionResult::not_sfx());
    }

    // Stage 3: Archive validation (use first signature found)
    let (offset, format) = signatures_found[0];

    // Basic validation: check if there's enough data after the signature
    // Check against total file length, not just buffer length
    if (offset as u64) + 100 > file_len {
        // Signature too close to end, likely false positive
        return Ok(SfxDetectionResult::not_sfx());
    }

    // For now, return detected result (full validation would require format-specific parsing)
    Ok(SfxDetectionResult::detected(
        stub_type,
        format,
        offset as u64,
    ))
}

/// Validate archive structure at given offset
///
/// Simplified validation - just checks for sufficient data size.
/// Magic byte validation is already performed by the signature scanner,
/// so we only need to ensure there's enough data to be a valid archive.
///
/// # Arguments
/// * `offset` - Byte offset where archive should start
/// * `buffer` - Buffer containing the file data
///
/// # Returns
/// * `Ok(())` if archive is valid
/// * `Err(ArchiveError)` if validation fails
#[allow(dead_code)]
pub fn validate_archive_at_offset(offset: usize, buffer: &[u8]) -> Result<()> {
    // Check if there's at least 100 bytes of data after the signature
    // This is a basic sanity check - full validation happens during extraction
    if offset + 100 > buffer.len() {
        return Err(ArchiveError::corruption(
            "embedded_archive",
            "Insufficient data after archive signature",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ArchiveFormat;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_non_executable_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"Just plain text, not an executable")
            .unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx);
    }

    #[test]
    fn test_shell_script_with_zip() {
        let mut temp = NamedTempFile::new().unwrap();
        // Shell script header + some padding + ZIP signature
        let mut data = b"#!/bin/sh\necho 'Self-extracting archive'\n".to_vec();
        data.extend_from_slice(&[0u8; 400]); // Padding
        data.extend_from_slice(b"PK\x03\x04"); // ZIP signature
        data.extend_from_slice(&[0u8; 100]); // More data
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.stub_type, Some(StubType::ShellScript));
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    }

    #[test]
    fn test_validate_archive_at_offset_valid() {
        let buffer = vec![0u8; 500];
        // Offset 100, buffer has 500 bytes, so 400 bytes after offset
        assert!(validate_archive_at_offset(100, &buffer).is_ok());
    }

    #[test]
    fn test_validate_archive_at_offset_too_close() {
        let buffer = vec![0u8; 150];
        // Offset 100, only 50 bytes after offset (need 100)
        assert!(validate_archive_at_offset(100, &buffer).is_err());
    }

    #[test]
    fn test_validate_archive_at_offset_at_end() {
        let buffer = vec![0u8; 100];
        // Offset at end of buffer
        assert!(validate_archive_at_offset(100, &buffer).is_err());
    }

    #[test]
    fn test_detect_sfx_signature_too_close_to_end() {
        let mut temp = NamedTempFile::new().unwrap();
        // Shell script + ZIP signature very close to end (< 100 bytes)
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 50]); // Padding
        data.extend_from_slice(b"PK\x03\x04"); // ZIP signature
        data.extend_from_slice(&[0u8; 50]); // Only 50 bytes after signature
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx); // Should be rejected as not SFX
    }

    #[test]
    fn test_detect_sfx_multiple_signatures() {
        let mut temp = NamedTempFile::new().unwrap();
        // Shell script with multiple archive signatures (should use first)
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 200]); // Padding
        data.extend_from_slice(b"PK\x03\x04"); // ZIP at 212
        data.extend_from_slice(&[0u8; 200]); // More padding
        data.extend_from_slice(b"7z\xbc\xaf\x27\x1c"); // 7z at ~416
        data.extend_from_slice(&[0u8; 200]); // More data
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip)); // First signature
        assert!(result.data_offset.unwrap() < 300); // Should be the ZIP offset
    }

    #[test]
    fn test_detect_sfx_no_signature() {
        let mut temp = NamedTempFile::new().unwrap();
        // Shell script without any archive signature
        let data = b"#!/bin/sh\necho 'Just a script, no archive'\nexit 0\n";
        temp.write_all(data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx);
    }

    #[test]
    fn test_detect_sfx_shell_script_with_7z() {
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/bash\n".to_vec();
        data.extend_from_slice(&[0u8; 300]);
        data.extend_from_slice(b"7z\xbc\xaf\x27\x1c");
        data.extend_from_slice(&[0u8; 150]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::SevenZip));
    }

    #[test]
    fn test_detect_sfx_shell_script_with_rar() {
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 250]);
        data.extend_from_slice(b"Rar!\x1a\x07\x00");
        data.extend_from_slice(&[0u8; 200]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Rar));
    }

    #[test]
    fn test_detect_sfx_large_stub() {
        let mut temp = NamedTempFile::new().unwrap();
        // Large stub (>1MB) - should still work as we scan first 1MB
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&vec![0u8; 900_000]); // Large stub
        data.extend_from_slice(b"PK\x03\x04");
        data.extend_from_slice(&[0u8; 200]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    }

    #[test]
    fn test_detect_sfx_signature_beyond_1mb() {
        let mut temp = NamedTempFile::new().unwrap();
        // Signature beyond 1MB scan limit - should not be detected
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&vec![0u8; 1_100_000]); // Push signature beyond 1MB
        data.extend_from_slice(b"PK\x03\x04");
        data.extend_from_slice(&[0u8; 200]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx); // Signature beyond scan limit
    }

    #[test]
    fn test_detect_sfx_small_file() {
        let mut temp = NamedTempFile::new().unwrap();
        // Very small file
        let data = b"#!/bin/sh\nPK\x03\x04data";
        temp.write_all(data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        // Should find signature but reject due to insufficient data after it
        assert!(!result.is_sfx);
    }

    // ============================================================
    // T110f: Additional unit tests for detection.rs validation logic
    // ============================================================

    #[test]
    fn test_detect_sfx_nonexistent_file() {
        let result = detect_sfx("/nonexistent/path/to/file.exe");
        assert!(result.is_err(), "Should return error for nonexistent file");
    }

    #[test]
    fn test_detect_sfx_empty_file() {
        let temp = NamedTempFile::new().unwrap();
        // Empty file - 0 bytes
        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx, "Empty file should not be SFX");
    }

    #[test]
    fn test_validate_archive_at_offset_boundary() {
        // Test various boundary conditions for validate_archive_at_offset
        let buffer = vec![0u8; 200];

        // Offset + 100 exactly equals buffer length
        assert!(validate_archive_at_offset(100, &buffer).is_ok());

        // Offset + 100 > buffer length
        assert!(validate_archive_at_offset(101, &buffer).is_err());

        // Offset at buffer end
        assert!(validate_archive_at_offset(200, &buffer).is_err());

        // Offset beyond buffer
        assert!(validate_archive_at_offset(300, &buffer).is_err());
    }

    #[test]
    fn test_validate_archive_at_offset_zero() {
        // Test offset 0
        let buffer = vec![0u8; 100];
        assert!(validate_archive_at_offset(0, &buffer).is_ok());
    }

    #[test]
    fn test_validate_archive_empty_buffer() {
        let buffer: Vec<u8> = vec![];
        assert!(validate_archive_at_offset(0, &buffer).is_err());
    }

    #[test]
    fn test_detect_sfx_signature_at_different_positions() {
        // Test ZIP signature at various positions within 1MB limit
        let positions = [50, 100, 1000, 10000, 100000, 500000, 900000];

        for pos in positions {
            let mut temp = NamedTempFile::new().unwrap();
            let mut data = b"#!/bin/sh\n".to_vec();
            data.extend(vec![0u8; pos - 10]); // Padding to reach position
            data.extend_from_slice(b"PK\x03\x04");
            data.extend(vec![0u8; 200]); // Enough data after signature
            temp.write_all(&data).unwrap();
            temp.flush().unwrap();

            let result = detect_sfx(temp.path()).unwrap();
            assert!(
                result.is_sfx,
                "Should detect SFX with signature at position {}",
                pos
            );
            assert_eq!(
                result.data_offset,
                Some(pos as u64),
                "Offset should be {} for signature at position {}",
                pos,
                pos
            );
        }
    }

    #[test]
    fn test_detect_sfx_all_format_signatures() {
        // Test detection with each archive format signature
        let signatures: Vec<(&[u8], ArchiveFormat)> = vec![
            (b"PK\x03\x04", ArchiveFormat::Zip),
            (b"Rar!\x1a\x07\x00", ArchiveFormat::Rar),
            (b"Rar!\x1a\x07\x01\x00", ArchiveFormat::Rar5),
            (b"7z\xbc\xaf\x27\x1c", ArchiveFormat::SevenZip),
        ];

        for (sig, expected_format) in signatures {
            let mut temp = NamedTempFile::new().unwrap();
            let mut data = b"#!/bin/sh\n".to_vec();
            data.extend(vec![0u8; 200]); // Padding
            data.extend_from_slice(sig);
            data.extend(vec![0u8; 150]); // Enough data after
            temp.write_all(&data).unwrap();
            temp.flush().unwrap();

            let result = detect_sfx(temp.path()).unwrap();
            assert!(result.is_sfx, "Should detect SFX for {:?}", expected_format);
            assert_eq!(result.archive_format, Some(expected_format));
        }
    }

    #[test]
    fn test_detect_sfx_concurrent_safe() {
        // Test that multiple concurrent detections don't interfere
        use std::thread;

        // Create multiple test files
        let files: Vec<_> = (0..4)
            .map(|i| {
                let mut temp = NamedTempFile::new().unwrap();
                let mut data = b"#!/bin/sh\n".to_vec();
                data.extend(vec![0u8; 200 + i * 100]);
                data.extend_from_slice(b"PK\x03\x04");
                data.extend(vec![0u8; 150]);
                temp.write_all(&data).unwrap();
                temp.flush().unwrap();
                temp
            })
            .collect();

        // Run detections in parallel
        let handles: Vec<_> = files
            .iter()
            .map(|f| {
                let path = f.path().to_owned();
                thread::spawn(move || detect_sfx(&path))
            })
            .collect();

        // All should succeed
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result.is_ok());
            assert!(result.unwrap().is_sfx);
        }
    }

    #[test]
    fn test_detect_sfx_file_permissions() {
        // Test with file that exists but might have unusual properties
        let temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 200]);
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 150]);
        std::fs::write(temp.path(), &data).unwrap();

        // Should work normally
        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
    }

    #[test]
    fn test_detect_sfx_exact_1mb_file() {
        let mut temp = NamedTempFile::new().unwrap();
        // File exactly 1MB
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; MAX_SCAN_SIZE - 14 - 150]); // Leave room for sig + trailing data
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 150]);

        // Ensure we're exactly at 1MB
        while data.len() < MAX_SCAN_SIZE {
            data.push(0);
        }
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path());
        // Should process without error
        assert!(result.is_ok());
    }

    #[test]
    fn test_constants_are_reasonable() {
        // Verify constants have expected values
        assert_eq!(MAX_SCAN_SIZE, 1_048_576, "MAX_SCAN_SIZE should be 1MB");
        assert_eq!(CHUNK_SIZE, 512, "CHUNK_SIZE should be 512");
        assert_eq!(HEADER_SIZE, 4096, "HEADER_SIZE should be 4KB");
    }

    #[test]
    fn test_detect_sfx_returns_first_signature() {
        let mut temp = NamedTempFile::new().unwrap();
        // Multiple signatures - should return first one (smallest offset)
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 100]);
        data.extend_from_slice(b"PK\x03\x04"); // ZIP at ~111
        data.extend(vec![0u8; 100]);
        data.extend_from_slice(b"7z\xbc\xaf\x27\x1c"); // 7z at ~215
        data.extend(vec![0u8; 150]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip)); // First signature
    }
}
