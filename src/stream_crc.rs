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
//!
//! # Internal layout
//!
//! The implementation is split along the three concerns it used to mix
//! in one file (see the 2026-08-21 amendment to
//! `docs/records/AD-0010-unified-stream-checksum-extraction.md`):
//!
//! - `digest` — the value layer: [`StreamChecksum`], [`CheckType`], the
//!   type-gated accessors, and the I/O-free primitives that recover a
//!   digest from a byte or bit window.
//! - `codec` — the framing layer: one child per format
//!   ([`extract_gzip_stream_crc`], [`extract_bzip2_stream_crc`],
//!   [`extract_xz_stream_check`]), plus the single rule that classifies
//!   a failed fixed-size read.
//! - `detect` — the dispatch layer: [`extract_stream_checksum`], which
//!   knows only which magic bytes belong to which codec.
//!
//! All three are private modules; every public item keeps the exact
//! `stream_crc::…` path it had before the split, so the split is not
//! observable from outside the crate.

mod codec;
mod detect;
mod digest;

#[cfg(test)]
mod test_support;

pub use codec::bzip2::extract_bzip2_stream_crc;
pub use codec::gzip::extract_gzip_stream_crc;
pub use codec::xz::extract_xz_stream_check;
pub use detect::extract_stream_checksum;
pub use digest::{CheckType, StreamChecksum};
