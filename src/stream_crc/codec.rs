//! Codec layer: per-format header and trailer parsing.
//!
//! One child module per standalone compressed format — [`gzip`],
//! [`bzip2`], [`xz`]. Each child owns exactly one entry point, opens the
//! path itself, validates the container's fixed header before trusting a
//! checksum found elsewhere in the file, and returns a
//! [`super::digest::StreamChecksum`]. No child knows about the others,
//! and none of them performs format *detection* — that is
//! [`super::detect`]'s job, and it dispatches into these entry points by
//! magic bytes.
//!
//! This module itself holds only the one rule every child shares:
//! [`framing_read_error`], which decides how a failed fixed-size read is
//! classified.

pub mod bzip2;
pub mod gzip;
pub mod xz;

use crate::error::ArchiveError;
use crate::format::ArchiveFormat;
use std::io::ErrorKind;
use std::path::Path;

/// Classify a failed fixed-size *framing* read — a magic, header, or
/// trailer whose length the format mandates.
///
/// # Truncation is a `Format` error at every layer
///
/// A read that ends with [`ErrorKind::UnexpectedEof`] means the stream
/// stopped inside a structure the format requires, i.e. the *data* is
/// truncated. That is a property of the bytes, not of the machine, and
/// no retry can turn it into a success, so it is reported as
/// [`ArchiveError::Format`] — carrying `format` when the container has
/// already been established and `None` when it has not.
///
/// Every other [`ErrorKind`] is an operational failure (permission,
/// device, storage) and keeps its [`ArchiveError::Io`] classification,
/// its `operation` label, and its original `io::Error` source so
/// retryable failures are never reported as unsupported data.
///
/// Applying this one rule in the codec layer *and* in the auto-detect
/// probe is what makes a truncated file report the same variant no
/// matter which layer noticed it (see the 2026-08-21 amendment to
/// `docs/records/AD-0010-unified-stream-checksum-extraction.md`).
pub(super) fn framing_read_error(
    format: Option<ArchiveFormat>,
    operation: &'static str,
    path: &Path,
    what: &str,
    error: std::io::Error,
) -> ArchiveError {
    if error.kind() == ErrorKind::UnexpectedEof {
        ArchiveError::format(format, format!("{what}: {error}"))
    } else {
        ArchiveError::io(operation, path.to_path_buf(), error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_maps_to_format_with_the_supplied_hint() {
        let err = framing_read_error(
            Some(ArchiveFormat::Xz),
            "read_header",
            Path::new("/nonexistent/truncated.xz"),
            "XZ stream too short for its 12-byte stream header",
            std::io::Error::new(ErrorKind::UnexpectedEof, "failed to fill whole buffer"),
        );
        match err {
            ArchiveError::Format { format, message } => {
                assert_eq!(format, Some(ArchiveFormat::Xz));
                assert!(message.contains("too short"), "message was: {message}");
            }
            other => panic!("expected Format for a truncated read, got: {other:?}"),
        }
    }

    #[test]
    fn non_eof_failures_keep_their_io_classification_and_source() {
        let err = framing_read_error(
            Some(ArchiveFormat::Xz),
            "read_header",
            Path::new("/nonexistent/unreadable.xz"),
            "XZ stream too short for its 12-byte stream header",
            std::io::Error::from(ErrorKind::PermissionDenied),
        );
        match err {
            ArchiveError::Io {
                operation, source, ..
            } => {
                assert_eq!(operation, "read_header");
                assert_eq!(source.kind(), ErrorKind::PermissionDenied);
            }
            other => panic!("expected Io for a non-EOF read failure, got: {other:?}"),
        }
    }
}
