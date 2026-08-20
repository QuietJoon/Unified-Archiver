//! GZIP header validation and trailer parsing (RFC 1952).

use super::framing_read_error;
use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::stream_crc::digest::{CheckType, StreamChecksum};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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
/// # Errors
///
/// A stream too short to hold the framing this function reads is an
/// [`ArchiveError::Format`], not an [`ArchiveError::Io`]; other read
/// failures keep their `Io` classification and source.
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
            Some(ArchiveFormat::Gzip),
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
    file.read_exact(&mut header).map_err(|e| {
        framing_read_error(
            None,
            "read_magic",
            path.as_ref(),
            "GZIP stream too short for its 4-byte header",
            e,
        )
    })?;
    if header[0] != 0x1F || header[1] != 0x8B {
        return Err(ArchiveError::format(
            None,
            "Not a valid GZIP file (bad magic)",
        ));
    }
    if header[2] != 0x08 || (header[3] & 0xE0) != 0 {
        return Err(ArchiveError::format(
            Some(ArchiveFormat::Gzip),
            "GZIP header has an unsupported compression method or reserved flags set",
        ));
    }

    // Seek to last 8 bytes (GZIP trailer)
    file.seek(SeekFrom::End(-8))
        .map_err(|e| ArchiveError::io("seek", path.as_ref().to_path_buf(), e))?;

    let mut trailer = [0u8; 8];
    file.read_exact(&mut trailer).map_err(|e| {
        framing_read_error(
            Some(ArchiveFormat::Gzip),
            "read_trailer",
            path.as_ref(),
            "GZIP stream too short for its 8-byte trailer",
            e,
        )
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_crc::test_support::write_fixture;

    /// A gzip stream shorter than its own header+trailer minimum is
    /// truncated *data*, so it is a `Format` error — the same variant
    /// the auto-detect probe reports for a truncated stream.
    #[test]
    fn truncated_gzip_is_a_format_error() {
        let path = write_fixture(
            "ua_stream_crc_gzip_truncated.gz",
            &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
        let err = extract_gzip_stream_crc(&path).expect_err("truncated gzip must fail");
        match err {
            ArchiveError::Format { format, message } => {
                assert_eq!(format, Some(ArchiveFormat::Gzip));
                assert!(message.contains("too short"), "message was: {message}");
            }
            other => panic!("expected Format for a truncated gzip, got: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
