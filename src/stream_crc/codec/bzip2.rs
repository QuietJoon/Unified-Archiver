//! BZIP2 header validation and end-of-stream marker parsing.

use super::framing_read_error;
use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::stream_crc::digest::{
    CheckType, StreamChecksum, find_bzip2_eos_crc_bitwise, find_pattern_last,
};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Extract stream-level checksum from a BZIP2 file
///
/// BZIP2 format has a stream CRC32 in the end-of-stream marker:
/// - Magic: 0x177245385090 (6 bytes, BCD of sqrt(pi))
/// - Stream CRC32: 32 bits
///
/// The stream CRC is computed by combining all block CRCs:
/// `stream_crc = (stream_crc << 1) | (stream_crc >> 31); stream_crc ^= block_crc;`
///
/// The EOS marker is bit-packed: bzip2 emits blocks back-to-back with
/// no byte-alignment padding, so the 48-bit magic (and the CRC after
/// it) usually starts mid-byte. Extraction tries a byte-aligned match
/// first, then falls back to scanning the file tail at every bit
/// offset (R0079-0009).
///
/// # Errors
///
/// A stream too short to hold the framing this function reads is an
/// [`ArchiveError::Format`], not an [`ArchiveError::Io`]; other read
/// failures keep their `Io` classification and source.
pub fn extract_bzip2_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Validate the BZIP2 header: "BZh" + block-size digit '1'..='9'. The
    // digit gate rejects a bare "BZh" prefix that is not a real stream
    // header (R0080-0080).
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).map_err(|e| {
        framing_read_error(
            None,
            "read_magic",
            path.as_ref(),
            "BZIP2 stream too short for its 4-byte header",
            e,
        )
    })?;
    if magic[0] != b'B' || magic[1] != b'Z' || magic[2] != b'h' {
        return Err(ArchiveError::format(
            None,
            "Not a valid BZIP2 file (bad magic)",
        ));
    }
    if !matches!(magic[3], b'1'..=b'9') {
        return Err(ArchiveError::format(
            Some(ArchiveFormat::Bzip2),
            "BZIP2 header has an invalid block-size digit",
        ));
    }

    // Read the last 1KB of the file to find EOS marker
    // The EOS marker (6 bytes) + CRC32 (4 bytes) = 10 bytes minimum,
    // but we read more to handle padding and alignment
    let file_len = file
        .metadata()
        .map_err(|e| ArchiveError::io("stat", path.as_ref().to_path_buf(), e))?
        .len();

    let read_size = (file_len.min(1024)) as usize;
    if read_size < 10 {
        return Err(ArchiveError::format(
            None,
            "BZIP2 file too small to contain EOS marker",
        ));
    }

    file.seek(SeekFrom::End(-(read_size as i64)))
        .map_err(|e| ArchiveError::io("seek", path.as_ref().to_path_buf(), e))?;

    let mut tail = vec![0u8; read_size];
    file.read_exact(&mut tail).map_err(|e| {
        framing_read_error(
            Some(ArchiveFormat::Bzip2),
            "read",
            path.as_ref(),
            "BZIP2 stream ended inside its trailing block",
            e,
        )
    })?;

    // Look for EOS magic: 0x177245385090 (6 bytes)
    let eos_magic = [0x17u8, 0x72, 0x45, 0x38, 0x50, 0x90];

    // Fast path: the EOS marker happens to land byte-aligned (~1 in 8
    // files). Search backward in the tail for the last occurrence.
    if let Some(pos) = find_pattern_last(&tail, &eos_magic) {
        // CRC32 is the 4 bytes after the magic
        if pos + 10 <= tail.len() {
            let crc_bytes = &tail[pos + 6..pos + 10];
            let crc32 =
                u32::from_be_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);

            return Ok(StreamChecksum {
                crc32: Some(crc32),
                crc64: None,
                uncompressed_size: None, // BZIP2 doesn't store uncompressed size
                check_type: CheckType::Crc32,
            });
        }
    }

    // Slow path: blocks are bit-packed, so the marker usually starts
    // at a non-zero bit offset within a byte (R0079-0009).
    if let Some(crc32) = find_bzip2_eos_crc_bitwise(&tail) {
        return Ok(StreamChecksum {
            crc32: Some(crc32),
            crc64: None,
            uncompressed_size: None, // BZIP2 doesn't store uncompressed size
            check_type: CheckType::Crc32,
        });
    }

    Err(ArchiveError::format(
        None,
        "Could not find BZIP2 end-of-stream marker".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_crc::test_support::write_fixture;

    /// Tiny single-block bzip2 streams compressed from seeded random
    /// input. Each expected stream CRC was cross-checked against an
    /// independent CRC-32/BZIP2 of the uncompressed input (for a
    /// single block, stream CRC == block CRC).
    ///
    /// EOS magic starts at bit offset 640 (byte-aligned).
    const BZ2_EOS_BYTE_ALIGNED: &[u8] = &[
        0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x01, 0xC6, 0x1E, 0x94, 0x00,
        0x00, 0x0B, 0x3D, 0x7F, 0x84, 0x00, 0x82, 0x00, 0x58, 0x00, 0x00, 0x88, 0x00, 0x40, 0xC6,
        0x00, 0x40, 0x09, 0x20, 0x20, 0x02, 0x80, 0x00, 0x00, 0x80, 0x81, 0x10, 0x00, 0x24, 0x40,
        0x20, 0x00, 0x22, 0x82, 0x64, 0xC9, 0x84, 0xD3, 0xD4, 0xDA, 0x4F, 0x35, 0x42, 0x9A, 0x34,
        0x01, 0xA0, 0x00, 0x0E, 0xF2, 0x9E, 0x53, 0x38, 0xEC, 0x8F, 0x16, 0x4E, 0xBE, 0x24, 0x2F,
        0x54, 0x18, 0x80, 0x0A, 0x3F, 0x17, 0x72, 0x45, 0x38, 0x50, 0x90, 0x01, 0xC6, 0x1E, 0x94,
    ];
    const BZ2_EOS_BYTE_ALIGNED_CRC: u32 = 0x01C6_1E94;

    /// EOS magic starts at bit offset 423 (= 7 mod 8).
    const BZ2_EOS_BIT_OFFSET_7: &[u8] = &[
        0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x85, 0xD1, 0x9B, 0xE6, 0x00,
        0x00, 0x00, 0xD9, 0x47, 0x80, 0x01, 0x40, 0x00, 0x40, 0x04, 0x00, 0x08, 0x10, 0x00, 0x10,
        0x80, 0x00, 0x02, 0x01, 0x00, 0x20, 0x22, 0x20, 0x00, 0x22, 0x04, 0xC2, 0x66, 0x42, 0x01,
        0xA0, 0x01, 0x95, 0x7F, 0xA1, 0x7A, 0x48, 0x3C, 0x2E, 0xE4, 0x8A, 0x70, 0xA1, 0x21, 0x0B,
        0xA3, 0x37, 0xCC,
    ];
    const BZ2_EOS_BIT_OFFSET_7_CRC: u32 = 0x85D1_9BE6;

    /// EOS magic starts at bit offset 358 (= 6 mod 8).
    const BZ2_EOS_BIT_OFFSET_6: &[u8] = &[
        0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x30, 0x54, 0xBB, 0x77, 0x00,
        0x00, 0x01, 0x33, 0x68, 0x10, 0x00, 0x00, 0x09, 0x01, 0x00, 0x00, 0x04, 0x40, 0x00, 0x00,
        0x02, 0x00, 0x40, 0x20, 0x00, 0x22, 0x06, 0x9A, 0x7A, 0x10, 0xC0, 0x8E, 0x5A, 0x78, 0x10,
        0x5D, 0xC9, 0x14, 0xE1, 0x42, 0x40, 0xC1, 0x52, 0xED, 0xDC,
    ];
    const BZ2_EOS_BIT_OFFSET_6_CRC: u32 = 0x3054_BB77;

    #[test]
    fn test_bzip2_stream_crc_byte_aligned_eos() {
        let path = write_fixture("ua_stream_crc_bz2_aligned.bz2", BZ2_EOS_BYTE_ALIGNED);
        let checksum = extract_bzip2_stream_crc(&path).expect("byte-aligned EOS");
        assert_eq!(checksum.check_type, CheckType::Crc32);
        assert_eq!(checksum.crc32, Some(BZ2_EOS_BYTE_ALIGNED_CRC));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_bzip2_stream_crc_non_byte_aligned_eos() {
        // R0079-0009: the EOS marker is bit-packed; these fixtures
        // place it at bit offsets 7 and 6 within a byte, where the old
        // byte-aligned-only search reported "Could not find BZIP2
        // end-of-stream marker".
        for (name, bytes, expected) in [
            (
                "ua_stream_crc_bz2_bit7.bz2",
                BZ2_EOS_BIT_OFFSET_7,
                BZ2_EOS_BIT_OFFSET_7_CRC,
            ),
            (
                "ua_stream_crc_bz2_bit6.bz2",
                BZ2_EOS_BIT_OFFSET_6,
                BZ2_EOS_BIT_OFFSET_6_CRC,
            ),
        ] {
            let path = write_fixture(name, bytes);
            let checksum = extract_bzip2_stream_crc(&path).expect("non-byte-aligned EOS");
            assert_eq!(checksum.check_type, CheckType::Crc32, "fixture {name}");
            assert_eq!(checksum.crc32, Some(expected), "fixture {name}");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn test_bzip2_stream_crc_multi_stream_returns_last_eos() {
        // Concatenated bzip2 streams are valid input; the helper
        // reports the CRC after the *last* EOS marker — here the
        // second stream's, with both markers non-byte-aligned.
        let mut concat = BZ2_EOS_BIT_OFFSET_7.to_vec();
        concat.extend_from_slice(BZ2_EOS_BIT_OFFSET_6);
        let path = write_fixture("ua_stream_crc_bz2_concat.bz2", &concat);
        let checksum = extract_bzip2_stream_crc(&path).expect("multi-stream bzip2");
        assert_eq!(checksum.crc32, Some(BZ2_EOS_BIT_OFFSET_6_CRC));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_bzip2_stream_crc_multi_block_cli() {
        // R0079-0009: multi-block coverage. 300 KB of seeded LCG bytes
        // is incompressible, so `bzip2 -1` (100 KB blocks) emits four
        // blocks and the EOS lands at an arbitrary bit offset.
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("ua_stream_crc_mb_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let raw = dir.join("mb.bin");
        let compressed = dir.join("mb.bin.bz2");

        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut data = Vec::with_capacity(300_000);
        for _ in 0..300_000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            data.push((state >> 33) as u8);
        }
        std::fs::write(&raw, &data).expect("write raw data");

        let status = match Command::new("bzip2").args(["-1", "-f"]).arg(&raw).status() {
            Ok(status) => status,
            Err(e) => {
                eprintln!("skipping multi-block bzip2 test: bzip2 CLI unavailable ({e})");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        assert!(status.success(), "bzip2 -1 failed");

        let checksum = extract_bzip2_stream_crc(&compressed).expect("multi-block bzip2");
        assert_eq!(checksum.check_type, CheckType::Crc32);
        let crc32 = checksum.crc32.expect("stream CRC present");

        // bzip2's own integrity check prints the combined (stream) CRC
        // at high verbosity — use it as an independent oracle for the
        // exact value when available.
        if let Ok(out) = Command::new("bzip2")
            .arg("-tvvvv")
            .arg(&compressed)
            .output()
        {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if let Some(rest) = stderr.split("combined CRCs: stored = 0x").nth(1)
                && let Some(hex) = rest.split(',').next()
                && let Ok(expected) = u32::from_str_radix(hex.trim(), 16)
            {
                assert_eq!(
                    crc32, expected,
                    "stream CRC must match bzip2's own combined CRC"
                );
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stream that ends inside the 4-byte bzip2 header is truncated
    /// *data*: `Format`, not `Io`. Before the 2026-08-21 AD-0010
    /// amendment this path reported `Io("read_magic")` while the
    /// auto-detect probe reported `Format` for the same shortfall.
    #[test]
    fn truncated_bzip2_header_is_a_format_error() {
        let path = write_fixture("ua_stream_crc_bz2_truncated_header.bz2", b"BZh");
        let err = extract_bzip2_stream_crc(&path).expect_err("truncated bzip2 must fail");
        match err {
            ArchiveError::Format { format, message } => {
                assert_eq!(format, None);
                assert!(message.contains("too short"), "message was: {message}");
            }
            other => panic!("expected Format for a truncated bzip2 header, got: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
