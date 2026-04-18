//! Stream-level CRC32 extraction for compression formats
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

use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Stream-level checksum information
#[derive(Debug, Clone)]
pub struct StreamChecksum {
    /// CRC32 value from the stream (if available)
    pub crc32: Option<u32>,

    /// CRC64 value from the stream (if available, XZ only)
    pub crc64: Option<u64>,

    /// Uncompressed size from the stream metadata
    pub uncompressed_size: Option<u64>,

    /// Type of checksum found
    pub check_type: CheckType,
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
/// Reference: RFC 1952
pub fn extract_gzip_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Validate GZIP header magic (0x1F 0x8B)
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)
        .map_err(|e| ArchiveError::io("read_magic", path.as_ref().to_path_buf(), e))?;
    if magic != [0x1F, 0x8B] {
        return Err(ArchiveError::format(
            None,
            "Not a valid GZIP file (bad magic)",
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
pub fn extract_bzip2_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Validate BZIP2 header magic ("BZh")
    let mut magic = [0u8; 3];
    file.read_exact(&mut magic)
        .map_err(|e| ArchiveError::io("read_magic", path.as_ref().to_path_buf(), e))?;
    if magic[0] != b'B' || magic[1] != b'Z' || magic[2] != b'h' {
        return Err(ArchiveError::format(
            None,
            "Not a valid BZIP2 file (bad magic)",
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

    // Search backward in the tail for the last EOS marker
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

    // Stream Flags are at bytes 6-7
    // Byte 7 contains the Check type in bits 0-3
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
/// Detection prefers magic bytes over file extension to avoid mislabeled files
/// being handed to the wrong parser.
pub fn extract_stream_checksum(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let path_ref = path.as_ref();

    // Prefer magic-byte detection for robustness (handles renamed/mislabeled files)
    let mut file =
        File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref.to_path_buf(), e))?;

    let mut magic = [0u8; 6];
    file.read_exact(&mut magic)
        .map_err(|e| ArchiveError::io("read_magic", path_ref.to_path_buf(), e))?;
    drop(file);

    // GZIP: 0x1F 0x8B
    if magic[0] == 0x1F && magic[1] == 0x8B {
        return extract_gzip_stream_crc(path);
    }

    // BZIP2: "BZh"
    if magic[0] == b'B' && magic[1] == b'Z' {
        return extract_bzip2_stream_crc(path);
    }

    // XZ: 0xFD 7 z X Z 0x00
    if magic == *b"\xFD7zXZ\x00" {
        return extract_xz_stream_check(path);
    }

    // Fall back to extension for files whose magic might not match standard patterns
    let extension = path_ref.extension().and_then(|s| s.to_str()).unwrap_or("");
    match extension {
        "gz" | "gzip" => extract_gzip_stream_crc(path),
        "bz2" | "bzip2" => extract_bzip2_stream_crc(path),
        "xz" => extract_xz_stream_check(path),
        _ => Err(ArchiveError::format(
            None,
            format!("Unknown format with magic: {:02X?}", &magic[0..2]),
        )),
    }
}

/// Helper function to find the last occurrence of a byte pattern in data
fn find_pattern_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
