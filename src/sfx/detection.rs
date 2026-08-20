// SFX detection implementation
//
// Three-stage pipeline: exe validation → signature scan → heuristic screening
// of the candidate offset (stage 3 does NOT fully validate the embedded
// archive; it only rejects obviously-implausible candidates).

use super::result::SfxDetectionResult;
use super::signatures;
use super::stub_types::StubType;
use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

// Size policy lives in `super::limits` so detection,
// extraction, and staging share a single source of truth.
use super::limits::{HEADER_SIZE, MAX_SCAN_SIZE, PROBE_TAIL};

/// Detect if a file is a self-extracting archive
///
/// Implements 3-stage detection pipeline:
/// 1. Stage 1: Classify executable format (PE/ELF/Mach-O/Script)
/// 2. Stage 2: Scan for archive signatures in first 1MB
/// 3. Stage 3: Heuristic screening of the candidate offset (cheap
///    plausibility check; full archive validation happens later when the
///    caller opens the payload via [`crate::Archive::open_sfx`] or
///    [`crate::Archive::open_at_offset`]).
///
/// # Arguments
/// * `path` - Path to the file to check
///
/// # Returns
/// * `Ok(SfxDetectionResult)` with detection results
/// * `Err(ArchiveError)` for I/O errors only — opening, statting, or
///   reading `path`. The embedded archive is never opened here, so a
///   corrupt payload cannot fail this call; it surfaces when the caller
///   opens the payload via [`crate::Archive::open_sfx`] or
///   [`crate::Archive::open_at_offset`].
pub fn detect_sfx<P: AsRef<Path>>(path: P) -> Result<SfxDetectionResult> {
    let path = path.as_ref();

    // Stage 1 & 2: Read file once (up to 1MB) for both executable validation and signature scanning
    // This optimization reduces file I/O by 50% compared to separate reads
    let file = File::open(path).map_err(|e| ArchiveError::io("open", path, e))?;

    // Get file length for validation
    let meta = file
        .metadata()
        .map_err(|e| ArchiveError::io("metadata", path, e))?;
    // R0001-0002 (completes OI-0081-001): snapshot the identity of the
    // inode *this* handle refers to — `File::metadata` is an fstat on the
    // open descriptor, so it names exactly the bytes detection is about to
    // read. Staging used to re-`stat` the pathname after this function
    // returned and its handle was closed, which let a replacement dropped
    // in that window become the trusted baseline.
    let source_identity = crate::archive::read_file_identity(&meta);
    let file_len = meta.len();

    // Clamp the capacity hint as `u64` before casting to `usize` so a
    // huge file on a narrow target can't truncate before the `min` clamp
    // is applied (R0069-0077).
    //
    // R0079-0043: over-read PROBE_TAIL bytes past the scan window so the
    // Stage-3 structural probes still have header bytes for a signature
    // found just inside the 1 MiB boundary. The signature scan itself
    // stays capped at MAX_SCAN_SIZE below.
    let read_limit = (MAX_SCAN_SIZE + PROBE_TAIL) as u64;
    let scan_capacity = file_len.min(read_limit) as usize;
    let mut scan_buffer = Vec::with_capacity(scan_capacity);
    let _bytes_read = file
        .take(read_limit)
        .read_to_end(&mut scan_buffer)
        .map_err(|e| ArchiveError::io("read", path, e))?;

    // Stage 1: Executable format classification using first 4KB of buffer.
    //
    // `StubType::detect()` returns `StubType::Unknown` for unrecognized
    // executables instead of erroring. Unknown stubs are rejected before
    // Stage 2 signature scanning; only recognized native/script stubs proceed
    // to archive-signature checks.
    let header_size = HEADER_SIZE.min(scan_buffer.len());
    let header = &scan_buffer[..header_size];

    let stub_type = StubType::detect(header);

    // R0070-0076: a real SFX must carry a recognised executable stub
    // *before* the embedded archive. Reject `Unknown` (no
    // PE/ELF/Mach-O/script header) up front — before paying the
    // Stage-2 signature scan — instead of letting arbitrary binaries
    // with incidental archive magic slip through as `is_sfx=true`.
    if stub_type == StubType::Unknown {
        return Ok(SfxDetectionResult::not_sfx());
    }

    // Stage 2: Archive signature scanning (using same buffer already
    // read). The scan window excludes the PROBE_TAIL over-read so the
    // documented 1 MiB detection bound is unchanged.
    let scan_window = &scan_buffer[..scan_buffer.len().min(MAX_SCAN_SIZE)];
    let signatures_found = signatures::scan_for_signatures(scan_window);

    if signatures_found.is_empty() {
        // No archive signature found - return not_sfx (not an error)
        return Ok(SfxDetectionResult::not_sfx());
    }

    // Stage 3: heuristic screening of each candidate signature.
    // Returns `probable()` on a passing screen — full archive
    // validation is the caller's responsibility (e.g. via
    // `Archive::open_sfx`). Renamed from "validation" to "heuristic
    // screening" so the function name no longer over-promises
    // (R0069-0075).
    //
    // R0079-0031: candidates are screened in offset order, but a
    // passing strong-magic candidate (multi-byte signature with a
    // structural probe: ZIP/7z/RAR/xz/bzip2) wins over the 2-byte gzip
    // signature regardless of position — a genuine embedded gzip
    // resource in the stub region passes the gzip probe by
    // construction and would otherwise shadow the real payload behind
    // it. Earliest offset remains the tiebreak within a strength
    // class.
    let mut weak_match: Option<(usize, crate::ArchiveFormat)> = None;
    for &(offset, format) in &signatures_found {
        // R0070-0075: drop the unconditional 100-byte tail gate. The
        // per-format probe below knows exactly how many bytes its
        // header needs; cutting valid candidates short of that just
        // because the archive happens to be small produces false
        // negatives for legitimate gzip/zip-stream SFXs. Each format
        // arm now enforces its own minimum length; if the buffer is
        // shorter than required, the arm returns `false` and we
        // continue to the next candidate.

        // Format-specific header probe at the detected offset.
        // Raw-compression signatures (gzip/bzip2/xz) used to fall into a
        // `>= 100 bytes` catch-all which misreported ordinary executables
        // carrying incidental magic bytes as probable SFX archives. The
        // ZIP and raw-compression arms now inspect header bytes, scaled to
        // the amount of data the sfx header scanner actually loaded; the
        // RAR, RAR5 and 7z arms remain remaining-length gates that read no
        // header bytes.
        let archive_data = &scan_buffer[offset..];
        // R0079-0043: gate per-format minimums on the bytes remaining in
        // the *file* (`file_len - offset`), not on the scan-buffer
        // remainder — the buffer truncates at the scan window and was
        // rejecting valid payloads just inside the 1 MiB boundary. The
        // PROBE_TAIL over-read guarantees the buffer covers every
        // byte-inspecting per-format minimum (all ≤ PROBE_TAIL) for any
        // in-window offset; the `archive_data.len()` clamp only guards a
        // file that shrank between `metadata()` and the read above,
        // keeping the probes' indexing in-bounds.
        let avail = file_len
            .saturating_sub(offset as u64)
            .min(archive_data.len() as u64);
        let valid = match format {
            crate::ArchiveFormat::Zip => {
                // ZIP local file header layout (PKZIP APPNOTE 4.3.7):
                //   bytes  0..4  signature 'PK\x03\x04'
                //   bytes  4..6  version-needed-to-extract
                //   bytes  6..8  general-purpose flags
                //   bytes  8..10 compression method
                //   bytes 14..18 CRC-32
                //   bytes 18..22 compressed size
                //   bytes 22..26 uncompressed size
                //   bytes 26..28 file-name length
                //   bytes 28..30 extra-field length
                //
                // This is a cheap screen, not full validation; the payload
                // is parsed for real when the caller opens it. A plausible
                // local header has a recognised compression method
                // (R0075-0070), a file-name length within a sane bound
                // (4 KiB covers every real filename and rejects the random
                // 0xFFFF a 16-bit field defaults to in noise), and its
                // complete variable-length portion (30 + name_len +
                // extra_len) must fit in the bytes remaining in the file
                // (R0081-0019 — a truncated header cannot be a real archive).
                // The extra-field length is deliberately NOT capped: Zip64,
                // Unicode-path, and NTFS extras legitimately exceed 4 KiB,
                // and the fits-in-file bound already rejects an over-long
                // declaration (R0081-0018).
                let signature_ok = avail >= 30
                    && archive_data[0] == 0x50
                    && archive_data[1] == 0x4B
                    && archive_data[2] == 0x03
                    && archive_data[3] == 0x04;
                if signature_ok {
                    let compression = u16::from_le_bytes([archive_data[8], archive_data[9]]);
                    let name_len =
                        u16::from_le_bytes([archive_data[26], archive_data[27]]) as usize;
                    let extra_len =
                        u16::from_le_bytes([archive_data[28], archive_data[29]]) as usize;
                    // Known-canonical methods: 0 (store), 8 (deflate),
                    // 9 (deflate64), 12 (bzip2), 14 (LZMA), 93 (zstd),
                    // 95 (xz), 99 (AES-WinZip wrapper). Anything else
                    // is almost certainly random bytes coinciding
                    // with the signature.
                    let method_ok = matches!(compression, 0 | 8 | 9 | 12 | 14 | 93 | 95 | 99);
                    let remaining = file_len.saturating_sub(offset as u64);
                    let header_len = 30usize
                        .checked_add(name_len)
                        .and_then(|len| len.checked_add(extra_len));
                    let len_ok =
                        name_len <= 4096 && header_len.is_some_and(|len| (len as u64) <= remaining);
                    method_ok && len_ok
                } else {
                    false
                }
            }
            crate::ArchiveFormat::Rar => {
                // RAR4: Rar!\x1a\x07\x00 + archive header
                avail >= 20
            }
            crate::ArchiveFormat::Rar5 => {
                // RAR5: Rar!\x1a\x07\x01\x00 + archive header
                avail >= 20
            }
            crate::ArchiveFormat::SevenZip => {
                // 7z: 6-byte signature + 2 version bytes + header size fields
                avail >= 32
            }
            crate::ArchiveFormat::Gzip => {
                // gzip member: ID1 ID2 (\x1f\x8b), CM=08 (deflate), FLG, MTIME(4),
                // XFL, OS. Require the compression method byte to be 8; reserved
                // flag bits (0xE0) must be clear. This rules out arbitrary
                // 2-byte `\x1f\x8b` occurrences in executables.
                avail >= 10 && archive_data[2] == 0x08 && (archive_data[3] & 0xE0) == 0
            }
            crate::ArchiveFormat::Bzip2 => {
                // bzip2: "BZh" + block-size digit ('1'..='9') followed by
                // one of two 48-bit markers: the compressed-block magic
                // 0x314159265359 (a non-empty stream) or the end-of-stream
                // magic 0x177245385090 (a valid *empty* stream, e.g. an
                // SFX stub carrying `BZhN` + EOS). Accept either so empty
                // members are not missed (R0080-0085).
                const BLOCK_MAGIC: [u8; 6] = [0x31, 0x41, 0x59, 0x26, 0x53, 0x59];
                const EOS_MAGIC: [u8; 6] = [0x17, 0x72, 0x45, 0x38, 0x50, 0x90];
                avail >= 10
                    && matches!(archive_data[3], b'1'..=b'9')
                    && (archive_data[4..10] == BLOCK_MAGIC || archive_data[4..10] == EOS_MAGIC)
            }
            crate::ArchiveFormat::Xz => {
                // xz stream header: 6-byte magic + stream flags (2 bytes) +
                // CRC32 (4 bytes) = 12 bytes minimum. Stream flags reserved
                // byte must be 0; check byte must match xz's CRC gate shape
                // (we only enforce the reserved-zero invariant here so the
                // probe stays cheap).
                avail >= 12 && archive_data[6] == 0x00
            }
            _ => {
                // Unknown / unsupported format: require at least 100 bytes of
                // data and nothing else. Prefer to reject probable matches
                // over false positives; confirmed SFX flows open the archive
                // below and don't reach this branch.
                avail >= 100
            }
        };

        if valid {
            if format == crate::ArchiveFormat::Gzip {
                // Weak 2-byte signature: remember the earliest passing
                // candidate but keep looking for a strong-magic one
                // behind it (R0079-0031).
                if weak_match.is_none() {
                    weak_match = Some((offset, format));
                }
                continue;
            }
            // Signature-only match: Probable, since we screened the
            // signature structurally but did not open/validate the
            // embedded archive (I3 — the former 0.9 confidence float is
            // replaced by SfxConfidence::Probable plus evidence).
            let evidence = vec![
                format!("executable stub: {}", stub_type.description()),
                format!(
                    "{:?} signature passed the Stage-3 structural probe at offset {}",
                    format, offset
                ),
            ];
            // R0001-0002: carry the detection-open identity so the staging
            // copy binds to the file detection actually inspected.
            return Ok(
                SfxDetectionResult::probable(stub_type, format, offset as u64, evidence)
                    .with_source_identity(source_identity),
            );
        }
    }

    if let Some((offset, format)) = weak_match {
        let evidence = vec![
            format!("executable stub: {}", stub_type.description()),
            format!(
                "{:?} 2-byte signature passed its probe at offset {} \
                 (weak match; no strong-magic candidate found)",
                format, offset
            ),
        ];
        // R0001-0002: same binding for the weak-match arm.
        return Ok(
            SfxDetectionResult::probable(stub_type, format, offset as u64, evidence)
                .with_source_identity(source_identity),
        );
    }

    Ok(SfxDetectionResult::not_sfx())
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
        assert_eq!(result.stub_type, Some(StubType::ScriptInterpreter));
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    }

    #[test]
    fn test_detect_sfx_carries_detection_open_identity() {
        // R0001-0002: a probable result is bound to the identity of the
        // handle detection actually read, so the staging copy can refuse a
        // file swapped in after detection returned. Results with nothing to
        // stage (not_sfx) carry no identity.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 200]);
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 200]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        let meta = std::fs::metadata(temp.path()).unwrap();
        assert_eq!(
            result.source_identity(),
            Some(crate::archive::read_file_identity(&meta)),
            "probable results must carry the detection open's identity"
        );

        let mut plain = NamedTempFile::new().unwrap();
        plain.write_all(b"plain text, no executable stub").unwrap();
        plain.flush().unwrap();
        assert!(
            detect_sfx(plain.path())
                .unwrap()
                .source_identity()
                .is_none(),
            "not_sfx results have no payload to bind an identity to"
        );
    }

    #[test]
    fn test_detect_sfx_zip_large_extra_field_passes() {
        // R0081-0018: a specification-valid ZIP local header whose extra
        // field exceeds 4 KiB (Zip64/Unicode-path/NTFS extras routinely do)
        // must still screen as a probable SFX, provided its full declared
        // extent fits in the file.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 100]); // stub padding

        let name_len: usize = 8;
        let extra_len: usize = 5000; // > 4 KiB
        let mut header = vec![0u8; 30];
        header[0..4].copy_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // PK\x03\x04
        header[8..10].copy_from_slice(&8u16.to_le_bytes()); // deflate (decodable)
        header[26..28].copy_from_slice(&(name_len as u16).to_le_bytes());
        header[28..30].copy_from_slice(&(extra_len as u16).to_le_bytes());
        data.extend_from_slice(&header);
        // The complete variable-length portion is present in the file.
        data.extend_from_slice(&vec![b'a'; name_len]);
        data.extend_from_slice(&vec![0u8; extra_len]);

        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    }

    #[test]
    fn test_detect_sfx_zip_truncated_header_not_probable() {
        // R0081-0019: a local header whose declared extent (30 + name_len +
        // extra_len) exceeds the bytes remaining in the file is truncated
        // and cannot be a real archive, so it must not screen as probable.
        // name_len stays within the sanity cap so the fits-in-file bound is
        // the only reason for rejection.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 100]); // stub padding

        let mut header = vec![0u8; 30];
        header[0..4].copy_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        header[8..10].copy_from_slice(&8u16.to_le_bytes()); // deflate
        header[26..28].copy_from_slice(&8u16.to_le_bytes()); // name_len within cap
        header[28..30].copy_from_slice(&40000u16.to_le_bytes()); // extra_len
        data.extend_from_slice(&header);
        // Only a few trailing bytes follow — nowhere near 30 + 8 + 40000.
        data.extend_from_slice(&[0u8; 16]);

        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx);
    }

    #[test]
    fn test_detect_sfx_signature_too_close_to_end() {
        // R0070-0075: the unconditional 100-byte trailing-bytes gate
        // was removed in favour of per-format probes. The ZIP probe
        // requires at least 30 bytes of archive data (local file
        // header minimum). Keep the test scenario but shrink the
        // payload so the per-format gate still rejects this
        // truncated input — without falling back to the gone
        // 100-byte heuristic.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend_from_slice(&[0u8; 50]);
        data.extend_from_slice(b"PK\x03\x04"); // ZIP signature (4 bytes)
        data.extend_from_slice(&[0u8; 10]); // total archive_data = 14 bytes < 30
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx);
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
    fn test_detect_sfx_bzip2_empty_stream() {
        // R0080-0085: a valid *empty* bzip2 member begins with the
        // end-of-stream marker (0x177245385090) right after `BZhN`, with
        // no compressed-block magic. The Stage-3 probe must still
        // classify it as an embedded bzip2 payload.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 200]); // stub padding
        data.extend_from_slice(b"BZh9"); // header + block-size digit
        data.extend_from_slice(&[0x17, 0x72, 0x45, 0x38, 0x50, 0x90]); // EOS marker
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // stream CRC
        data.extend(vec![0u8; 100]); // trailing bytes
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx, "empty bzip2 stream should be detected");
        assert_eq!(result.archive_format, Some(ArchiveFormat::Bzip2));
    }

    #[test]
    fn test_detect_sfx_xz_payload() {
        // Stage-3 probe, Xz arm: 6-byte magic + stream flags with the
        // reserved first flag byte zero, ≥12 bytes available.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 200]); // stub padding
        data.extend_from_slice(b"\xFD7zXZ\x00"); // xz magic
        data.extend_from_slice(&[0x00, 0x01]); // stream flags: reserved=0, check=CRC32
        data.extend_from_slice(&[0x69, 0x22, 0xDE, 0x36]); // flags CRC32
        data.extend(vec![0u8; 100]); // trailing bytes
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx, "embedded xz stream should be detected");
        assert_eq!(result.archive_format, Some(ArchiveFormat::Xz));
    }

    #[test]
    fn test_detect_sfx_xz_reserved_flag_rejected() {
        // Same layout but the reserved stream-flag byte is non-zero: the
        // probe must reject the candidate instead of reporting Probable.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 200]);
        data.extend_from_slice(b"\xFD7zXZ\x00");
        data.extend_from_slice(&[0xFF, 0x01]); // reserved byte set — invalid
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        data.extend(vec![0u8; 100]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(
            !result.is_sfx,
            "xz candidate with reserved stream-flag bits set must be rejected"
        );
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
        assert_eq!(HEADER_SIZE, 4096, "HEADER_SIZE should be 4KB");
        // The largest byte-inspecting per-format probe minimum (7z, 32
        // bytes) must fit inside the over-read tail (R0079-0043).
        const { assert!(PROBE_TAIL >= 32, "PROBE_TAIL must cover the probe minimums") };
    }

    #[test]
    fn test_detect_sfx_unknown_stub_with_zip_payload_is_rejected() {
        // R0070-0076: a payload at non-zero offset behind an
        // unrecognised header is no longer reported as an SFX. The
        // previous behaviour misclassified arbitrary binaries with
        // incidental archive magic; real SFXs always carry a
        // recognised executable stub.
        let mut temp = NamedTempFile::new().unwrap();
        let mut data = vec![0xAB, 0xCD, 0xEF, 0x99];
        data.extend(vec![0u8; 500]);
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 200]);
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(
            !result.is_sfx,
            "unknown-stub payload should not be classified as SFX (R0070-0076)"
        );
    }

    #[test]
    fn test_detect_sfx_unknown_stub_without_payload_stays_not_sfx() {
        // Regression guard: unrecognized header + no archive signature
        // must still be not_sfx (don't false-positive on random binaries).
        let mut temp = NamedTempFile::new().unwrap();
        let data = vec![0xAB, 0xCD, 0xEF, 0x99, 0x11, 0x22, 0x33, 0x44];
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(
            !result.is_sfx,
            "unknown header without archive signature must not be SFX"
        );
    }

    #[test]
    fn test_detect_sfx_iterates_candidates() {
        // Per AD 0015: detection iterates all candidate signatures
        // and returns the first one that validates (earliest offset).
        let mut temp = NamedTempFile::new().unwrap();
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
        // ZIP is the earliest candidate that passes validation
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
    }

    // ============================================================
    // R0079-0010: PE/ELF stub classification from header fields
    // ============================================================

    #[test]
    fn test_detect_sfx_pe_stub_with_zip_payload() {
        // R0079-0010: PE stubs are classified from header fields only.
        // goblin's whole-file parse failed on the 4 KiB prefix (import
        // data lives beyond it in any real stub), which turned every
        // PE SFX into not_sfx.
        let mut data = vec![0u8; 0x48];
        data[0] = b'M';
        data[1] = b'Z';
        data[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        data[0x40..0x44].copy_from_slice(b"PE\0\0");
        // Pad past the 4 KiB header window like a real stub's sections
        data.resize(8192, 0);
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 200]);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx, "PE stub with ZIP payload must detect as SFX");
        assert_eq!(result.stub_type, Some(StubType::WindowsPE));
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
        assert_eq!(result.data_offset, Some(8192));
    }

    #[test]
    fn test_detect_sfx_elf_stub_with_zip_payload() {
        // R0079-0010: ELF classification stays inside the 4 KiB prefix —
        // goblin chased e_shoff past it and failed for any real ELF stub.
        // R0001-0066: e_ident alone is no longer sufficient. The fixed
        // header fields that live inside the probe window are validated
        // too, so a synthetic stub must populate them or it now classifies
        // as `StubType::Unknown` (and detection fails closed to not_sfx).
        let mut data = vec![0u8; 64];
        data[..4].copy_from_slice(b"\x7fELF");
        data[4] = 2; // ELFCLASS64
        data[5] = 1; // ELFDATA2LSB
        data[6] = 1; // EI_VERSION
        data[0x10..0x12].copy_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
        data[0x14..0x18].copy_from_slice(&1u32.to_le_bytes()); // e_version = EV_CURRENT
        data[0x34..0x36].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize (ELFCLASS64)
        data.resize(8192, 0);
        data.extend_from_slice(b"PK\x03\x04");
        data.extend(vec![0u8; 200]);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(
            result.is_sfx,
            "ELF stub with ZIP payload must detect as SFX"
        );
        assert_eq!(result.stub_type, Some(StubType::LinuxELF));
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
        assert_eq!(result.data_offset, Some(8192));
    }

    // ============================================================
    // R0079-0031: strong-magic candidates win over the 2-byte gzip
    // signature
    // ============================================================

    #[test]
    fn test_detect_sfx_prefers_strong_magic_over_embedded_gzip() {
        // An embedded gzip resource ahead of the real payload passes
        // the gzip probe by construction; it must not shadow the
        // strong-magic ZIP signature behind it.
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 100]);
        let gzip_offset = data.len();
        // Plausible gzip member header: ID1 ID2, CM=8, FLG=0, MTIME, XFL, OS
        data.extend_from_slice(b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\x03");
        data.extend(vec![0u8; 100]);
        let zip_offset = data.len();
        let mut zip_header = [0u8; 30];
        zip_header[..4].copy_from_slice(b"PK\x03\x04");
        zip_header[8] = 8; // deflate
        data.extend_from_slice(&zip_header);
        data.extend(vec![0u8; 200]);
        assert!(gzip_offset < zip_offset);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(
            result.archive_format,
            Some(ArchiveFormat::Zip),
            "strong-magic ZIP must win over the earlier gzip candidate"
        );
        assert_eq!(result.data_offset, Some(zip_offset as u64));
    }

    #[test]
    fn test_detect_sfx_gzip_only_payload_still_detected() {
        // Guard for R0079-0031: with no strong-magic candidate, a
        // passing gzip candidate is still reported.
        let mut data = b"#!/bin/sh\n".to_vec();
        data.extend(vec![0u8; 50]);
        let gzip_offset = data.len();
        data.extend_from_slice(b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\x03");
        data.extend(vec![0u8; 100]);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Gzip));
        assert_eq!(result.data_offset, Some(gzip_offset as u64));
    }

    // ============================================================
    // R0079-0043: per-format gates run against the file length, with
    // a PROBE_TAIL over-read past the scan window
    // ============================================================

    #[test]
    fn test_detect_sfx_seven_zip_just_inside_scan_boundary() {
        // 7z gate needs 32 bytes; only 10 remain inside the scan
        // window, but the file has plenty after the offset. Gating on
        // the truncated buffer used to reject this.
        let sig_offset = MAX_SCAN_SIZE - 10;
        let mut data = b"#!/bin/sh\n".to_vec();
        data.resize(sig_offset, 0);
        data.extend_from_slice(b"7z\xbc\xaf\x27\x1c");
        data.extend(vec![0u8; 256]);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(
            result.is_sfx,
            "signature inside the scan window with payload beyond it must detect"
        );
        assert_eq!(result.archive_format, Some(ArchiveFormat::SevenZip));
        assert_eq!(result.data_offset, Some(sig_offset as u64));
    }

    #[test]
    fn test_detect_sfx_zip_probe_reads_past_scan_boundary() {
        // The ZIP structural probe inspects bytes 0..30 after the
        // signature; here bytes 6..30 lie past MAX_SCAN_SIZE and are
        // only available through the PROBE_TAIL over-read.
        let sig_offset = MAX_SCAN_SIZE - 6;
        let mut data = b"#!/bin/sh\n".to_vec();
        data.resize(sig_offset, 0);
        let mut zip_header = [0u8; 30];
        zip_header[..4].copy_from_slice(b"PK\x03\x04");
        zip_header[8] = 8; // deflate
        data.extend_from_slice(&zip_header);
        data.extend(vec![0u8; 1024]);

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(result.is_sfx);
        assert_eq!(result.archive_format, Some(ArchiveFormat::Zip));
        assert_eq!(result.data_offset, Some(sig_offset as u64));
    }

    #[test]
    fn test_detect_sfx_boundary_signature_short_file_still_rejected() {
        // The file itself ends 10 bytes after the 7z signature — the
        // file-length gate (not the scan-window gate) must reject it.
        let sig_offset = MAX_SCAN_SIZE - 10;
        let mut data = b"#!/bin/sh\n".to_vec();
        data.resize(sig_offset, 0);
        data.extend_from_slice(b"7z\xbc\xaf\x27\x1c");
        data.extend(vec![0u8; 4]); // 10 bytes total after offset, < 32

        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&data).unwrap();
        temp.flush().unwrap();

        let result = detect_sfx(temp.path()).unwrap();
        assert!(!result.is_sfx, "too little file data after the offset");
    }
}
