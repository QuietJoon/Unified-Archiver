//! XZ stream-header validation and check-type mapping.

use super::framing_read_error;
use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::stream_crc::digest::{CheckType, StreamChecksum};
use std::fs::File;
use std::io::Read;
use std::path::Path;

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
/// gzip/bzip2 last-member caveats.
///
/// # Errors
///
/// A stream too short to hold the 12-byte Stream Header is an
/// [`ArchiveError::Format`], not an [`ArchiveError::Io`]; other read
/// failures keep their `Io` classification and source.
///
/// Reference: XZ file format specification 1.2.1
pub fn extract_xz_stream_check(path: impl AsRef<Path>) -> Result<StreamChecksum> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| ArchiveError::io("open", path.as_ref().to_path_buf(), e))?;

    // Read XZ stream header (12 bytes minimum)
    let mut header = [0u8; 12];
    file.read_exact(&mut header).map_err(|e| {
        framing_read_error(
            None,
            "read_header",
            path.as_ref(),
            "XZ stream too short for its 12-byte stream header",
            e,
        )
    })?;

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
            Some(ArchiveFormat::Xz),
            "Invalid XZ stream flags (reserved bits set)".to_string(),
        ));
    }
    let stored_flags_crc = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    let computed_flags_crc = crc32fast::hash(&header[6..8]);
    if stored_flags_crc != computed_flags_crc {
        return Err(ArchiveError::format(
            Some(ArchiveFormat::Xz),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_crc::test_support::write_fixture;

    /// A stream that ends inside the 12-byte XZ Stream Header is
    /// truncated *data*: `Format`, not `Io`. This is the gap the
    /// 2026-08-21 AD-0010 amendment closes — the six-byte auto-detect
    /// probe accepts a file this long, so an 8-byte `.xz` used to
    /// report `Format` when it was 5 bytes and `Io("read_header")`
    /// when it was 8.
    #[test]
    fn truncated_xz_header_is_a_format_error() {
        let path = write_fixture(
            "ua_stream_crc_xz_truncated_header.xz",
            b"\xFD7zXZ\x00\x00\x04",
        );
        let err = extract_xz_stream_check(&path).expect_err("truncated xz must fail");
        match err {
            ArchiveError::Format { format, message } => {
                assert_eq!(format, None);
                assert!(message.contains("too short"), "message was: {message}");
            }
            other => panic!("expected Format for a truncated XZ header, got: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
