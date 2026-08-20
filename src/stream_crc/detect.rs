//! Detection layer: pick the codec for a standalone compressed stream.
//!
//! The only thing this module knows is which magic bytes belong to which
//! child of [`super::codec`]. It never parses a header or a trailer
//! itself — it reads a six-byte probe, closes its handle, and delegates
//! to the codec entry point, which reopens the path and re-validates the
//! magic it was chosen on.

use super::codec::bzip2::extract_bzip2_stream_crc;
use super::codec::framing_read_error;
use super::codec::gzip::extract_gzip_stream_crc;
use super::codec::xz::extract_xz_stream_check;
use super::digest::StreamChecksum;
use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Auto-detect format and extract stream checksum
///
/// Detection is by magic bytes only — the file extension is never
/// consulted, so renamed or mislabeled files are routed to the right
/// parser by content, and files whose magic matches no supported
/// format are rejected with the unknown-magic error regardless of
/// extension (R0079-0040).
///
/// # Errors
///
/// A stream too short for the six-byte probe is an
/// [`ArchiveError::Format`], and so is a stream that is long enough to
/// be routed but too short for the chosen codec's own framing — the
/// classification does not depend on which layer noticed the
/// truncation. Other read failures keep their [`ArchiveError::Io`]
/// classification and source.
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
    file.read_exact(&mut magic).map_err(|e| {
        framing_read_error(
            None,
            "read_magic",
            path_ref,
            "Stream too short for any supported format detection",
            e,
        )
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_crc::test_support::write_fixture;

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

    /// The decision the 2026-08-21 AD-0010 amendment records: one
    /// truncated file yields the same error variant whether the probe
    /// or the codec catches the shortfall. This `.xz` stub is long
    /// enough to pass the six-byte probe and short enough to die inside
    /// the 12-byte Stream Header, which is exactly the window where the
    /// two layers used to disagree.
    #[test]
    fn truncation_classification_agrees_across_layers() {
        let stub = b"\xFD7zXZ\x00\x00\x04";
        let path = write_fixture("ua_stream_crc_layer_agreement.xz", stub);

        let via_detect = extract_stream_checksum(&path).expect_err("truncated xz must fail");
        let via_codec = extract_xz_stream_check(&path).expect_err("truncated xz must fail");

        for (layer, err) in [("detect", via_detect), ("codec", via_codec)] {
            match err {
                ArchiveError::Format { .. } => {}
                other => panic!("expected Format from the {layer} layer, got: {other:?}"),
            }
        }

        let _ = std::fs::remove_file(&path);
    }
}
