//! Stream-level CRC32 extraction for compression formats.
//!
//! This module provides functionality to extract the stream-level CRC32 checksum
//! from single-file compression formats (GZIP, BZIP2, XZ). This is different from
//! per-file CRC32 - it's the checksum stored in the compressed stream's metadata.
//!
//! # Stream CRC32 vs File CRC32
//!
//! - **Stream CRC32**: Checksum of the uncompressed data stored in the format's trailer/footer
//! - **File CRC32**: Checksum computed over individual files in archives
//!
//! Stream CRC32 is metadata from the compression format itself and doesn't change
//! when you modify wrapper/header metadata (comments, timestamps).
//!
//! # Relationship to [`crate::Archive`] inspection
//!
//! These helpers are *file-level* utilities — they inspect a path on
//! disk directly, without going through the [`crate::Archive`] handle
//! or the inspection module. Most callers using a unified archive
//! shouldn't need them: per-entry CRC32 verification lives on
//! [`crate::Archive::validate_integrity`] /
//! [`crate::ArchiveEntry::crc32`]; archive-level digests live on
//! [`crate::Archive::calculate_archive_crc`] /
//! [`crate::Archive::calculate_content_multiset_digest_and_size`].
//!
//! Use the helpers in this module only when you need the *raw stream
//! checksum* of a standalone Gzip/Bzip2/Xz file *without* opening it
//! as an `Archive` — e.g. dedup-by-stream-crc, integrity scans of
//! plain compressed log files, or read-only checks against the
//! format's trailer when the archive listing is uninteresting.

use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Stream-level checksum information.
///
/// **Field-vs-`check_type` invariants.** The struct
/// tolerates contradictory states (`check_type == Crc32` with no
/// `crc32`, etc.) on its public fields. The accessor helpers below
/// (`crc32_value`, `crc64_value`) gate on `check_type` so callers
/// don't have to pattern-match the option-bag manually:
///
/// | `check_type` | populated fields                |
/// |--------------|---------------------------------|
/// | `Crc32`      | `crc32` (and usually `uncompressed_size`)        |
/// | `Crc64`      | `crc64`                                          |
/// | `None`       | neither `crc32` nor `crc64`; `uncompressed_size` may still be set if the format records it elsewhere |
/// | `Sha256`     | (reserved — no current backend emits this)       |
/// | `Unknown`    | nothing reliable; do not act on the option fields |
#[derive(Debug, Clone)]
pub struct StreamChecksum {
    /// CRC32 value from the stream (if available)
    pub crc32: Option<u32>,

    /// CRC64 value from the stream (if available, XZ only)
    pub crc64: Option<u64>,

    /// Uncompressed size from the stream metadata.
    ///
    /// **Gzip caveat.** For gzip this is the trailer `ISIZE`, i.e. the
    /// uncompressed size *modulo 2^32* (RFC 1952) — exact only for
    /// payloads below 4 GiB. A member of 4 GiB or larger reports its
    /// size mod 2^32, not the true size. (Bzip2 and Xz do not record an
    /// uncompressed size and leave this `None`.)
    pub uncompressed_size: Option<u64>,

    /// Type of checksum found
    pub check_type: CheckType,
}

impl StreamChecksum {
    /// Return the CRC32 value only when [`Self::check_type`] is
    /// [`CheckType::Crc32`].
    ///
    /// Use this in preference to inspecting `crc32` directly so a
    /// future variant change (XZ block with CRC32 marked but the
    /// option fields not populated) doesn't silently report a stale
    /// or contradictory value.
    pub fn crc32_value(&self) -> Option<u32> {
        match self.check_type {
            CheckType::Crc32 => self.crc32,
            _ => None,
        }
    }

    /// Return the CRC64 value only when [`Self::check_type`] is
    /// [`CheckType::Crc64`].
    pub fn crc64_value(&self) -> Option<u64> {
        match self.check_type {
            CheckType::Crc64 => self.crc64,
            _ => None,
        }
    }
}

/// Type of checksum in the stream
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckType {
    /// No checksum present
    None,
    /// CRC32 checksum
    Crc32,
    /// CRC64 checksum
    Crc64,
    /// SHA-256 checksum
    Sha256,
    /// Unknown or unsupported checksum type
    Unknown,
}

/// Extract stream-level checksum from a GZIP file
///
/// GZIP format has an 8-byte trailer at the end:
/// - Bytes 0-3: CRC32 of uncompressed data (little-endian)
/// - Bytes 4-7: Size of uncompressed data mod 2^32 (little-endian)
///
/// # Single-member assumption
///
/// RFC 1952 defines a gzip file as a *sequence* of members, each
/// carrying its own trailer (multi-member files come from `pigz`,
/// `bgzip`, or plain `cat a.gz b.gz`). This function reads only the
/// trailer at the end of the file, so on a multi-member input the
/// returned `crc32` and `uncompressed_size` describe the **last
/// member only**, not the concatenated payload (R0079-0030). Treat
/// the value as covering the whole file only for single-member
/// inputs — dedup or integrity flows keyed on this checksum compare
/// suffix-only data for multi-member files.
///
/// Reference: RFC 1952
pub fn extract_gzip_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Pre-flight length check: GZIP needs at least 2-byte header + 8-byte
    // trailer = 10 bytes. Surfacing a structured format error here
    // beats letting the seek/read fail with a low-level I/O error
    // when the file is too short (R0069-0078).
    let file_len = file
        .metadata()
        .map_err(|e| ArchiveError::io("stat", path.as_ref().to_path_buf(), e))?
        .len();
    if file_len < 10 {
        return Err(ArchiveError::format(
            Some(crate::format::ArchiveFormat::Gzip),
            format!(
                "GZIP file too short for header+trailer ({} bytes)",
                file_len
            ),
        ));
    }

    // Validate the fixed GZIP header shape: ID1 ID2 (0x1F 0x8B), CM == 8
    // (deflate), and the reserved FLG bits (0xE0) clear. The length
    // pre-flight above guarantees the 4 header bytes exist. This cheap
    // gate rejects arbitrary files whose first two bytes coincide with
    // the magic before we trust the 8-byte trailer (R0080-0078).
    let mut header = [0u8; 4];
    file.read_exact(&mut header)
        .map_err(|e| ArchiveError::io("read_magic", path.as_ref().to_path_buf(), e))?;
    if header[0] != 0x1F || header[1] != 0x8B {
        return Err(ArchiveError::format(
            None,
            "Not a valid GZIP file (bad magic)",
        ));
    }
    if header[2] != 0x08 || (header[3] & 0xE0) != 0 {
        return Err(ArchiveError::format(
            Some(crate::format::ArchiveFormat::Gzip),
            "GZIP header has an unsupported compression method or reserved flags set",
        ));
    }

    // Seek to last 8 bytes (GZIP trailer)
    file.seek(SeekFrom::End(-8))
        .map_err(|e| ArchiveError::io("seek", path.as_ref().to_path_buf(), e))?;

    let mut trailer = [0u8; 8];
    file.read_exact(&mut trailer)
        .map_err(|e| ArchiveError::io("read_trailer", path.as_ref().to_path_buf(), e))?;

    // Extract CRC32 (bytes 0-3, little-endian)
    let crc32 = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);

    // Extract ISIZE (bytes 4-7, little-endian) - uncompressed size mod 2^32
    let isize = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);

    Ok(StreamChecksum {
        crc32: Some(crc32),
        crc64: None,
        uncompressed_size: Some(isize as u64),
        check_type: CheckType::Crc32,
    })
}

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
pub fn extract_bzip2_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Validate the BZIP2 header: "BZh" + block-size digit '1'..='9'. The
    // digit gate rejects a bare "BZh" prefix that is not a real stream
    // header (R0080-0080).
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| ArchiveError::io("read_magic", path.as_ref().to_path_buf(), e))?;
    if magic[0] != b'B' || magic[1] != b'Z' || magic[2] != b'h' {
        return Err(ArchiveError::format(
            None,
            "Not a valid BZIP2 file (bad magic)",
        ));
    }
    if !matches!(magic[3], b'1'..=b'9') {
        return Err(ArchiveError::format(
            Some(crate::format::ArchiveFormat::Bzip2),
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
    file.read_exact(&mut tail)
        .map_err(|e| ArchiveError::io("read", path.as_ref().to_path_buf(), e))?;

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

/// Extract stream-level checksum from an XZ file
///
/// XZ format can use different check types:
/// - None (0x00)
/// - CRC32 (0x01)
/// - CRC64 (0x04)
/// - SHA-256 (0x0A)
///
/// The check type is specified in the Stream Flags.
///
/// # Single-stream assumption
///
/// An `.xz` file may be a *concatenation* of independent streams (e.g.
/// `xz --block-list` split output or `cat a.xz b.xz`), each with its
/// own Stream Header and check type. This function reads only the first
/// Stream Header, so the returned [`CheckType`] describes the **first
/// stream only**; a multi-stream file whose streams use different check
/// types is not represented here (R0080-0083). This mirrors the
/// gzip/bzip2 last-member caveats above.
///
/// Reference: XZ file format specification 1.2.1
pub fn extract_xz_stream_check(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Read XZ stream header (12 bytes minimum)
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|e| ArchiveError::io("read_header", path.as_ref().to_path_buf(), e))?;

    // Check XZ magic: 0xFD, '7', 'z', 'X', 'Z', 0x00
    if &header[0..6] != b"\xFD7zXZ\x00" {
        return Err(ArchiveError::format(
            None,
            "Invalid XZ file header".to_string(),
        ));
    }

    // Stream Flags occupy bytes 6-7: byte 6 is reserved and must be
    // zero, and only the low nibble of byte 7 (the Check ID) is defined
    // — the high nibble is reserved. Bytes 8-11 hold the CRC32 of the
    // two Stream Flags bytes. Reject a header that fails any of these
    // invariants rather than trusting a fabricated check type
    // (R0080-0082).
    if header[6] != 0x00 || (header[7] & 0xF0) != 0 {
        return Err(ArchiveError::format(
            Some(crate::format::ArchiveFormat::Xz),
            "Invalid XZ stream flags (reserved bits set)".to_string(),
        ));
    }
    let stored_flags_crc = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    let computed_flags_crc = crc32fast::hash(&header[6..8]);
    if stored_flags_crc != computed_flags_crc {
        return Err(ArchiveError::format(
            Some(crate::format::ArchiveFormat::Xz),
            "XZ stream flags CRC32 mismatch".to_string(),
        ));
    }

    // Byte 7 contains the Check type in bits 0-3.
    let check_type_id = header[7] & 0x0F;
    let check_type = match check_type_id {
        0x00 => CheckType::None,
        0x01 => CheckType::Crc32,
        0x04 => CheckType::Crc64,
        0x0A => CheckType::Sha256,
        _ => CheckType::Unknown,
    };

    // For now, we only extract the check type from the header
    // Actually reading the check value would require parsing the entire stream
    // to find the Block/Index checksums

    Ok(StreamChecksum {
        crc32: None, // Would need full stream parsing to extract
        crc64: None,
        uncompressed_size: None,
        check_type,
    })
}

/// Auto-detect format and extract stream checksum
///
/// Detection is by magic bytes only — the file extension is never
/// consulted, so renamed or mislabeled files are routed to the right
/// parser by content, and files whose magic matches no supported
/// format are rejected with the unknown-magic error regardless of
/// extension (R0079-0040).
pub fn extract_stream_checksum(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let path_ref = path.as_ref();

    // Prefer magic-byte detection for robustness (handles renamed/mislabeled files)
    let mut file =
        File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref.to_path_buf(), e))?;

    // Tiny files surface as an unsupported-stream format error rather
    // than a low-level read error that doesn't tell callers what's
    // missing (R0069-0079). R0001-0075: only a genuinely short stream
    // (`UnexpectedEof`) is a format problem — a permission, device or
    // storage failure keeps its `Io` classification and its original
    // `io::Error` source, so retryable/operational failures are not
    // reported as unsupported data.
    let mut magic = [0u8; 6];
    if let Err(e) = file.read_exact(&mut magic) {
        return Err(if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ArchiveError::format(
                None,
                format!("Stream too short for any supported format detection: {}", e),
            )
        } else {
            ArchiveError::io("read_magic", path_ref.to_path_buf(), e)
        });
    }
    drop(file);

    // GZIP: 0x1F 0x8B
    if magic[0] == 0x1F && magic[1] == 0x8B {
        return extract_gzip_stream_crc(path);
    }

    // BZIP2: "BZh" + block-size digit '1'..='9'. The full prefix plus a
    // valid level digit keeps a `BZ` followed by a non-`h` byte, or a
    // bare `BZh` with no level, out of the bzip parser (R0069-0080,
    // R0080-0080).
    if &magic[..3] == b"BZh" && matches!(magic[3], b'1'..=b'9') {
        return extract_bzip2_stream_crc(path);
    }

    // XZ: 0xFD 7 z X Z 0x00
    if magic == *b"\xFD7zXZ\x00" {
        return extract_xz_stream_check(path);
    }

    Err(ArchiveError::format(
        None,
        format!("Unknown format with magic: {:02X?}", &magic[0..2]),
    ))
}

/// Helper function to find the last occurrence of a byte pattern in data
fn find_pattern_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// Bit-level search for the bzip2 end-of-stream marker, returning the
/// 32-bit stream CRC stored immediately after it.
///
/// bzip2 packs blocks back-to-back with no byte-alignment padding, so
/// the 48-bit EOS magic (`0x177245385090`) can begin at any bit
/// offset. Scans the buffer with a rolling 48-bit window (MSB-first,
/// matching bzip2's bit order) and keeps the *last* match whose
/// trailing 32 CRC bits still fit in the buffer, mirroring the
/// byte-aligned fast path's last-occurrence semantics (R0079-0009).
fn find_bzip2_eos_crc_bitwise(tail: &[u8]) -> Option<u32> {
    const EOS_MAGIC: u64 = 0x1772_4538_5090;
    const WINDOW_MASK: u64 = (1 << 48) - 1;

    let bit_at = |pos: usize| (tail[pos / 8] >> (7 - (pos % 8))) & 1;

    let total_bits = tail.len() * 8;
    let mut window = 0u64;
    let mut last_match = None;
    for pos in 0..total_bits {
        window = ((window << 1) | u64::from(bit_at(pos))) & WINDOW_MASK;
        // `pos` is the window's final bit; the candidate match starts
        // 47 bits earlier and needs 32 CRC bits after it.
        if pos + 1 >= 48 {
            let start = pos + 1 - 48;
            if window == EOS_MAGIC && start + 80 <= total_bits {
                last_match = Some(start);
            }
        }
    }

    let crc_start = last_match? + 48;
    let mut crc32 = 0u32;
    for pos in crc_start..crc_start + 32 {
        crc32 = (crc32 << 1) | u32::from(bit_at(pos));
    }
    Some(crc32)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn write_fixture(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, bytes).expect("write stream_crc test fixture");
        path
    }

    #[test]
    fn test_find_pattern_last() {
        let data = b"hello world test";
        assert_eq!(find_pattern_last(data, b"world"), Some(6));
        assert_eq!(find_pattern_last(data, b"test"), Some(12));
        assert_eq!(find_pattern_last(data, b"notfound"), None);

        // find_pattern_last returns last occurrence, not first
        let data = b"abcXYZdefXYZghi";
        assert_eq!(find_pattern_last(data, b"XYZ"), Some(9));
    }

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

    #[test]
    fn test_extract_stream_checksum_ignores_extension_for_unknown_magic() {
        // R0079-0040: the extension fallback could never succeed (the
        // per-format helpers re-validate the same magic), so
        // unknown-magic files now get the informative unknown-magic
        // error even when carrying a compression extension.
        let path = write_fixture("ua_stream_crc_unknown_magic.gz", b"NOTGZIPDATA0");
        let err = extract_stream_checksum(&path).expect_err("unknown magic must fail");
        assert!(
            err.to_string().contains("Unknown format with magic"),
            "expected unknown-magic error, got: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// R0001-0075: a stream genuinely shorter than the six-byte probe
    /// stays the unsupported-stream `Format` error (R0069-0079).
    #[test]
    fn test_extract_stream_checksum_short_stream_is_format_error() {
        let path = write_fixture("ua_stream_crc_short_probe.gz", b"\x1F\x8B\x08");
        let err = extract_stream_checksum(&path).expect_err("short stream must fail");
        match err {
            ArchiveError::Format { message, .. } => assert!(
                message.contains("Stream too short"),
                "expected short-stream message, got: {message}"
            ),
            other => panic!("expected Format for a short stream, got: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    /// R0001-0075: any *other* `io::ErrorKind` on the probe keeps its
    /// `Io` classification and its original source instead of being
    /// reported as unsupported data.
    #[cfg(unix)]
    #[test]
    fn test_extract_stream_checksum_preserves_non_eof_io_error() {
        // Opening a directory succeeds on Unix while reading it fails
        // with `EISDIR`. Probe that first so the assertion below can
        // never pass vacuously on a filesystem that behaves otherwise.
        let dir = std::env::temp_dir().join("ua_stream_crc_non_eof_probe");
        std::fs::create_dir_all(&dir).expect("create stream_crc probe dir");
        let probe_kind = {
            let mut handle = File::open(&dir).expect("open probe dir");
            let mut buf = [0u8; 6];
            handle.read_exact(&mut buf).err().map(|e| e.kind())
        };

        if probe_kind.is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof) {
            let err = extract_stream_checksum(&dir).expect_err("directory probe must fail");
            match err {
                ArchiveError::Io {
                    operation, source, ..
                } => {
                    assert_eq!(operation, "read_magic");
                    assert_ne!(source.kind(), std::io::ErrorKind::UnexpectedEof);
                }
                other => panic!("expected Io for a non-EOF probe failure, got: {other:?}"),
            }
        }

        let _ = std::fs::remove_dir(&dir);
    }
}
