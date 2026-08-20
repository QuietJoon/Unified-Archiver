// Archive format signatures (magic bytes)
//
// Defines magic byte patterns for detecting embedded archives in SFX files

use std::sync::OnceLock;

use crate::format::ArchiveFormat;

/// Archive format signature definition.
///
/// Crate-internal: this primitive describes the table the SFX signature
/// scanner walks. It is not part of the public API surface — high-level
/// SFX detection results are exposed via [`SfxDetectionResult`] in the
/// parent module.
#[derive(Debug, Clone)]
pub(crate) struct Signature {
    /// Magic bytes to search for (at any offset)
    pub bytes: &'static [u8],
    /// Archive format this signature indicates
    pub format: ArchiveFormat,
}

/// Known archive format signatures
pub(crate) const SIGNATURES: &[Signature] = &[
    // ZIP format (local file header)
    Signature {
        bytes: b"PK\x03\x04",
        format: ArchiveFormat::Zip,
    },
    // RAR 4.x format
    Signature {
        bytes: b"Rar!\x1a\x07\x00",
        format: ArchiveFormat::Rar,
    },
    // RAR 5.x format
    Signature {
        bytes: b"Rar!\x1a\x07\x01\x00",
        format: ArchiveFormat::Rar5,
    },
    // 7-Zip format
    Signature {
        bytes: b"7z\xbc\xaf\x27\x1c",
        format: ArchiveFormat::SevenZip,
    },
    // gzip format
    Signature {
        bytes: b"\x1f\x8b",
        format: ArchiveFormat::Gzip,
    },
    // bzip2 format
    Signature {
        bytes: b"BZh",
        format: ArchiveFormat::Bzip2,
    },
    // XZ format
    Signature {
        bytes: b"\xfd7zXZ\x00",
        format: ArchiveFormat::Xz,
    },
];

/// Pre-computed first-byte dispatch table for the signature scan.
/// Built once on first use, shared across all calls.
fn sig_dispatch_table() -> &'static [Vec<usize>; 256] {
    static TABLE: OnceLock<[Vec<usize>; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
        for (i, sig) in SIGNATURES.iter().enumerate() {
            table[sig.bytes[0] as usize].push(i);
        }
        table
    })
}

/// Scan buffer for archive signatures.
///
/// Returns a vector of `(offset, format)` tuples for every signature that
/// matches inside `buffer`, sorted by offset. The scanner walks each byte
/// once using a first-byte dispatch table, so callers do not need to pass a
/// chunk size.
///
/// Crate-internal — public SFX detection goes through
/// [`crate::sfx::detect_sfx`].
pub(crate) fn scan_for_signatures(buffer: &[u8]) -> Vec<(usize, ArchiveFormat)> {
    let mut found = Vec::new();

    let table = sig_dispatch_table();

    // Single pass through buffer
    for offset in 0..buffer.len() {
        let first_byte = buffer[offset] as usize;
        for &sig_idx in &table[first_byte] {
            let sig = &SIGNATURES[sig_idx];
            if offset + sig.bytes.len() <= buffer.len()
                && &buffer[offset..offset + sig.bytes.len()] == sig.bytes
            {
                found.push((offset, sig.format));
            }
        }
    }

    // Sort by offset (return earliest match first)
    found.sort_by_key(|(offset, _)| *offset);
    found
}

/// Find first archive signature in buffer.
///
/// Returns the earliest offset and format found, or `None` if no signature
/// was detected. Crate-internal helper for the SFX detection pipeline.
/// Currently only used from this module's tests; kept `#[cfg(test)]` so a
/// future internal caller can revive it without producing a dead-code
/// warning in the meantime.
#[cfg(test)]
pub(crate) fn find_first_signature(buffer: &[u8]) -> Option<(usize, ArchiveFormat)> {
    scan_for_signatures(buffer).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_signature() {
        let data = b"Some data PK\x03\x04 more data";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 10); // Offset where "PK" starts
        assert_eq!(results[0].1, ArchiveFormat::Zip);
    }

    #[test]
    fn test_rar5_signature() {
        let data = b"Rar!\x1a\x07\x01\x00test";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ArchiveFormat::Rar5);
    }

    #[test]
    fn test_7z_signature() {
        let data = b"7z\xbc\xaf\x27\x1c";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ArchiveFormat::SevenZip);
    }

    #[test]
    fn test_no_signature() {
        let data = b"No archive signature here";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_signature_at_any_offset() {
        // Signature at various offsets (not just chunk boundaries)
        for offset in [100, 440, 512, 513, 1000] {
            let mut data = vec![0u8; offset + 100];
            data[offset..offset + 4].copy_from_slice(b"PK\x03\x04");

            let results = scan_for_signatures(&data);
            assert_eq!(
                results.len(),
                1,
                "Should find signature at offset {}",
                offset
            );
            assert_eq!(
                results[0].0, offset,
                "Should detect signature at exact offset {}",
                offset
            );
        }
    }

    #[test]
    fn test_find_first_signature() {
        // Test with multiple signatures - should return earliest
        let mut data = vec![0u8; 1000];
        data[500..504].copy_from_slice(b"PK\x03\x04"); // ZIP at 500
        data[700..707].copy_from_slice(b"Rar!\x1a\x07\x00"); // RAR at 700

        let result = find_first_signature(&data);
        assert!(result.is_some());
        let (offset, format) = result.unwrap();
        assert_eq!(offset, 500);
        assert_eq!(format, ArchiveFormat::Zip);
    }

    #[test]
    fn test_find_first_signature_none() {
        let data = b"No signatures here";
        let result = find_first_signature(data);
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_signatures() {
        // Test finding multiple signatures
        let mut data = vec![0u8; 2000];
        data[100..104].copy_from_slice(b"PK\x03\x04"); // ZIP
        data[500..506].copy_from_slice(b"7z\xbc\xaf\x27\x1c"); // 7z
        data[1000..1007].copy_from_slice(b"Rar!\x1a\x07\x00"); // RAR

        let results = scan_for_signatures(&data);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 100); // Sorted by offset
        assert_eq!(results[1].0, 500);
        assert_eq!(results[2].0, 1000);
    }

    #[test]
    fn test_tar_removed_from_sfx_signatures() {
        // TAR was removed from SFX SIGNATURES — ustar at any offset should not match
        let mut data = vec![0u8; 500];
        data[257..262].copy_from_slice(b"ustar");

        let results = scan_for_signatures(&data);
        assert_eq!(results.len(), 0, "TAR should not be in SFX signatures");
    }

    #[test]
    fn test_gzip_signature() {
        let data = b"\x1f\x8b\x08\x00test";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ArchiveFormat::Gzip);
    }

    #[test]
    fn test_bzip2_signature() {
        let data = b"BZh91AYtest";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ArchiveFormat::Bzip2);
    }

    #[test]
    fn test_xz_signature() {
        let data = b"\xfd7zXZ\x00test";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, ArchiveFormat::Xz);
    }

    // ============================================================
    // T110c: Additional edge case tests for signatures.rs
    // ============================================================

    #[test]
    fn test_empty_buffer() {
        let data: &[u8] = b"";
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 0, "Empty buffer should have no signatures");
    }

    #[test]
    fn test_buffer_shorter_than_signature() {
        // Buffer shorter than shortest signature (gzip is 2 bytes)
        let data: &[u8] = b"\x1f"; // Only 1 byte of gzip signature
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 0, "Partial signature should not match");
    }

    #[test]
    fn test_malformed_signature_bytes() {
        // Almost-but-not-quite matching signatures
        let data = b"PK\x03\x05"; // Wrong 4th byte (should be \x04)
        let results = scan_for_signatures(data);
        assert_eq!(results.len(), 0, "Malformed ZIP signature should not match");

        let data2 = b"Rar!\x1a\x08\x00"; // Wrong 6th byte (should be \x07)
        let results2 = scan_for_signatures(data2);
        assert_eq!(
            results2.len(),
            0,
            "Malformed RAR signature should not match"
        );
    }

    #[test]
    fn test_signature_at_boundary_offset() {
        // Test signature exactly at buffer boundary
        let mut data = vec![0u8; 512];
        data[508..512].copy_from_slice(b"PK\x03\x04"); // At offset 508, length 4 = ends at 512

        let results = scan_for_signatures(&data);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 508);
        assert_eq!(results[0].1, ArchiveFormat::Zip);
    }

    #[test]
    fn test_signature_spanning_boundary_cutoff() {
        // Signature would start at end but not complete
        let mut data = vec![0u8; 512];
        data[510..512].copy_from_slice(b"PK"); // Only first 2 bytes of 4-byte signature

        let results = scan_for_signatures(&data);
        assert_eq!(
            results.len(),
            0,
            "Partial signature at end should not match"
        );
    }

    #[test]
    fn test_multiple_same_signatures() {
        // Multiple occurrences of the same format signature
        let mut data = vec![0u8; 1000];
        data[100..104].copy_from_slice(b"PK\x03\x04"); // ZIP #1
        data[500..504].copy_from_slice(b"PK\x03\x04"); // ZIP #2
        data[800..804].copy_from_slice(b"PK\x03\x04"); // ZIP #3

        let results = scan_for_signatures(&data);
        assert_eq!(results.len(), 3, "Should find all 3 ZIP signatures");
        assert_eq!(results[0].0, 100);
        assert_eq!(results[1].0, 500);
        assert_eq!(results[2].0, 800);
    }

    #[test]
    fn test_overlapping_signature_patterns() {
        // RAR5 signature contains RAR4 as prefix, ensure proper detection
        let data = b"Rar!\x1a\x07\x01\x00more_data"; // RAR5

        let results = scan_for_signatures(data);
        // Both RAR and RAR5 might match at offset 0
        // RAR4: "Rar!\x1a\x07\x00" - should NOT match (different 7th byte)
        // RAR5: "Rar!\x1a\x07\x01\x00" - should match
        assert!(results.iter().any(|(_, f)| *f == ArchiveFormat::Rar5));
    }

    #[test]
    fn test_tar_removed_no_matches_anywhere() {
        // TAR removed from SFX SIGNATURES — ustar at any offset should not match
        let mut data = vec![0u8; 500];
        data[0..5].copy_from_slice(b"ustar");
        data[100..105].copy_from_slice(b"ustar");
        data[257..262].copy_from_slice(b"ustar");

        let results = scan_for_signatures(&data);
        assert_eq!(results.len(), 0, "TAR should not be in SFX signatures");
    }

    #[test]
    fn test_scan_is_deterministic_across_calls() {
        // Successive calls on the same buffer must yield identical results.
        let mut data = vec![0u8; 2000];
        data[123..127].copy_from_slice(b"PK\x03\x04");
        data[777..783].copy_from_slice(b"7z\xbc\xaf\x27\x1c");

        let first = scan_for_signatures(&data);
        let second = scan_for_signatures(&data);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn test_all_signature_formats_individually() {
        // Ensure each signature format can be detected in isolation
        let test_cases: Vec<(&[u8], ArchiveFormat)> = vec![
            (b"PK\x03\x04", ArchiveFormat::Zip),
            (b"Rar!\x1a\x07\x00", ArchiveFormat::Rar),
            (b"Rar!\x1a\x07\x01\x00", ArchiveFormat::Rar5),
            (b"7z\xbc\xaf\x27\x1c", ArchiveFormat::SevenZip),
            (b"\x1f\x8b", ArchiveFormat::Gzip),
            (b"BZh", ArchiveFormat::Bzip2),
            (b"\xfd7zXZ\x00", ArchiveFormat::Xz),
        ];

        for (signature, expected_format) in test_cases {
            let results = scan_for_signatures(signature);
            assert!(
                !results.is_empty(),
                "Should detect {:?} signature",
                expected_format
            );
            assert_eq!(results[0].1, expected_format);
        }
    }

    #[test]
    fn test_find_first_with_empty_buffer() {
        let data: &[u8] = b"";
        let result = find_first_signature(data);
        assert!(result.is_none(), "Empty buffer should return None");
    }

    #[test]
    fn test_signature_struct_fields() {
        // Verify Signature struct is properly constructed
        let sig = &SIGNATURES[0]; // ZIP local file header
        assert!(!sig.bytes.is_empty());

        // TAR signature was removed from SFX SIGNATURES (not a real-world SFX format)
        let tar_sig = SIGNATURES.iter().find(|s| s.format == ArchiveFormat::Tar);
        assert!(tar_sig.is_none());
    }
}
