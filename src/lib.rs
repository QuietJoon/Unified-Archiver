//! # unified-archive
//!
//! Unified, ergonomic Rust library for archive inspection, extraction, and creation.
//!
//! This library provides a consistent, high-level API for working with multiple archive formats
//! (RAR, RAR5, ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST, TAR.LZ4, TAR.LZMA, GZIP, BZIP2,
//! XZ, ZST, LZ4, LZMA, ISO) without requiring format-specific code. Write once, work with any
//! format.
//!
//! ## Features
//!
//! - ✨ **Unified Interface** - Same API for all formats
//! - 🔍 **Automatic Format Detection** - Magic byte and extension-based detection
//! - 🌊 **libarchive-backed Streaming** - True streaming for TAR family, ISO,
//!   and standalone Gzip/Bzip2/Xz; native ZIP/7z/RAR backends buffer the
//!   selected entry into memory before exposing a `Read` adapter, so memory
//!   usage tracks per-entry size for those formats. See [`Archive::extract_to_stream`]
//!   and the per-backend notes in `docs/USER_MANUAL.md` for details.
//! - 🚀 **High Performance** - SIMD CRC32 on supported targets
//! - 🔐 **Encrypted Read Support** - Encrypted archives (RAR, RAR5, ZIP, 7z)
//! - ✅ **Integrity Validation** - CRC32 verification
//! - 📊 **Rich Metadata** - Sizes, timestamps, CRC32, permissions
//! - 🛡️ **SFX Detection and Opening** - Identify and open self-extracting archives across platforms
//! - 🏗️ **Archive Creation** - Create ZIP, 7z, TAR archives with compression
//!
//! ## Quick Examples
//!
//! ### List Archive Contents
//!
//! ```no_run
//! use unified_archive::Archive;
//!
//! let archive = Archive::open("document.zip")?;
//!
//! for entry in archive.list_files()? {
//!     println!("{}: {} bytes (CRC32: {:08X?})",
//!         entry.path,
//!         entry.size.unwrap_or(0),
//!         entry.crc32
//!     );
//! }
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ### Extract All Files
//!
//! ```no_run
//! use unified_archive::{Archive, ExtractionOptions};
//! use std::path::PathBuf;
//!
//! let archive = Archive::open("backup.7z")?;
//!
//! let options = ExtractionOptions {
//!     destination: PathBuf::from("./output"),
//!     preserve_permissions: true,
//!     verify_crc32: true,
//!     ..Default::default()
//! };
//!
//! let result = archive.extract_all(options)?;
//! for warning in &result.warnings {
//!     eprintln!("warning: {warning}");
//! }
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ### Extract to Memory
//!
//! ```no_run
//! use unified_archive::Archive;
//!
//! let archive = Archive::open("data.rar")?;
//! let data = archive.extract_to_memory("config.json")?;
//! let text = String::from_utf8(data)?;
//! println!("Config: {}", text);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ### Streaming Extraction
//!
//! Propagate read errors instead of treating them as EOF — corruption
//! and I/O failures surface through `Err`. Pass a [`StreamBound`] to
//! choose the output cap: [`StreamBound::DeclaredSize`] (the safe
//! default) and [`StreamBound::Cap`] report over-production as an
//! `Err`, while [`StreamBound::Unbounded`] forwards bytes verbatim and
//! trusts the source.
//!
//! ```no_run
//! use unified_archive::{Archive, StreamBound};
//! use std::io::Read;
//!
//! let archive = Archive::open("large.zip")?;
//! let mut stream = archive.extract_to_stream("huge_file.bin", StreamBound::DeclaredSize)?;
//!
//! let mut buffer = [0u8; 8192];
//! loop {
//!     let n = stream.read(&mut buffer)?;
//!     if n == 0 { break; }
//!     // Process chunk — note that actual streaming behavior depends on backend
//!     // (libarchive truly streams; native backends may buffer)
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ### Password-Protected Archives
//!
//! ```no_run
//! use unified_archive::Archive;
//!
//! // Check if password is required
//! let archive = Archive::open("secret.rar")?;
//! if archive.is_encrypted()? {
//!     println!("Password required");
//! }
//!
//! // Open with password
//! let archive = Archive::open_encrypted("secret.rar", "mypassword")?;
//! let entries = archive.list_files()?;
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ### Create Archives
//!
//! ```no_run
//! use unified_archive::{Archive, ArchiveFormat, CompressionOptions, CompressionLevel};
//!
//! let options = CompressionOptions {
//!     format: ArchiveFormat::Zip,
//!     level: CompressionLevel::Normal,
//!     ..Default::default()
//! };
//!
//! let mut creator = Archive::create("backup.zip", options)?;
//! creator.add_file_from_data("readme.txt", b"Hello, world!")?;
//! creator.add_directory_recursive("./src")?;
//! creator.finish()?;
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ### Detect Self-Extracting Archives (SFX)
//!
//! ```no_run
//! use unified_archive::Archive;
//!
//! let result = Archive::detect_sfx("installer.exe")?;
//! // `payload_coordinates()` yields the three fields together, or `None` if
//! // this is not an SFX — prefer it over unwrapping the accessors one by one.
//! if let Some((format, offset, stub)) = result.payload_coordinates() {
//!     println!("Found {format:?} archive at offset {offset}");
//!     println!("Stub type: {stub:?}");
//! }
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ## Supported Formats
//!
//! | Format    | Open | Extract | Create via `Archive::create` | Encrypted Read | SFX Detection |
//! |-----------|------|---------|------------------------------|----------------|---------------|
//! | **RAR**   | ✅   | ✅      | ❌                           | ✅             | ✅            |
//! | **RAR5**  | ✅   | ✅      | ❌                           | ✅             | ✅            |
//! | **ZIP**   | ✅   | ✅      | ✅                           | ✅             | ✅            |
//! | **7z**    | ✅   | ✅      | ✅                           | ✅†            | ✅            |
//! | **TAR**   | ✅   | ✅      | ✅                           | ❌             | ❌            |
//! | **TAR.GZ**| ✅   | ✅      | ✅                           | ❌             | ❌            |
//! | **TAR.BZ2**| ✅  | ✅      | ✅                           | ❌             | ❌            |
//! | **TAR.XZ**| ✅   | ✅      | ✅                           | ❌             | ❌            |
//! | **TAR.ZST**| ✅  | ✅      | ✅‡                          | ❌             | ❌            |
//! | **TAR.LZ4**| ✅  | ✅      | ✅‡                          | ❌             | ❌            |
//! | **TAR.LZMA**| ✅ | ✅      | ✅‡                          | ❌             | ❌            |
//! | **GZIP**  | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **BZIP2** | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **XZ**    | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **ZST**   | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **LZ4**   | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **LZMA**  | ✅   | ✅      | ❌                           | ❌             | ❌            |
//! | **ISO**   | ✅   | ✅      | ❌                           | ❌             | ❌            |
//!
//! *Optional Windows-only RAR creation exists via `external::RarCreator` behind the `external-rar-create` feature.*
//! *† 7z encryption: read-only via `open_encrypted()`; encrypted creation is not supported by `Archive::create()`.*
//! *‡ TAR.ZST / TAR.LZ4 / TAR.LZMA creation needs a libarchive built with the matching zstd / lz4 / lzma write filter; a missing filter fails at writer construction rather than degrading.*
//! Standalone `.gz`/`.bz2`/`.xz` files are supported for read/extract via libarchive's
//! raw-format binding (MADR-0019). Creation of standalone compressed files is out of scope
//! per AD 0018; use the TAR compound variants to produce compressed archives.
//! ZIP encryption is read-only (MADR-0027): `open_encrypted()` reads
//! AES/ZipCrypto archives, but `Archive::create()` deliberately rejects password-based
//! ZIP creation with `OperationBlocked`.
//!
//! ## Performance
//!
//! - **CRC32**: SIMD-accelerated checksum validation
//! - **Memory**: <100MB for multi-GB archives with libarchive-backed formats (streaming architecture);
//!   native ZIP (`zip` crate), 7z (SevenZ), and RAR (UnRAR) backends buffer individual entries in memory
//!   during streaming extraction
//! - **Detection**: <100ms for SFX detection (first 1MB scan)
//!
//! ## Architecture
//!
//! This library uses four backend engines:
//! - **zip**: ZIP read/extract/create — the sole ZIP backend for both
//!   encrypted and unencrypted archives (AD 0007 collapse)
//! - **sevenz-rust2**: 7z read/extract (native Rust)
//! - **libarchive**: TAR family, single-file compressed formats, ISO, and archive creation
//! - **UnRAR**: RAR/RAR5 with full CRC32 support (statically linked)
//!
//! Format detection is automatic using magic byte signatures.
//!
//! ## See Also
//!
//! - [Archive] - Main entry point for archive operations
//! - [ArchiveEntry] - File metadata within archives
//! - [ExtractionOptions] - Configure extraction behavior
//! - [ProgressCallback] - Monitor extraction progress

// Core modules
pub mod archive;
pub(crate) mod backend; // D1: ReadBackend trait scaffold (R0068-0029)
pub mod entry;
pub mod error;
pub mod format;
pub mod options;
pub mod password;
pub mod security; // Security utilities
pub mod sfx;
pub mod stream_crc; // Stream-level CRC32 extraction
pub mod streaming; // Phase 2.4: Streaming extraction // Phase 7: SFX detection

// Operation modules. AD 0062 A.1: visibility tightened to
// `pub(crate)` — the curated facade re-exports below are the only
// public entry points, so backend renames and module reorganisations
// stay non-breaking.
pub(crate) mod creation;
pub(crate) mod extraction;
/// Shared inode-identity primitive for the by-name revalidation guards
/// (DCR-007 / OI-0081-001 / R0080-0061).
pub(crate) mod fs_identity;
pub(crate) mod inspection;
pub(crate) mod modification;

#[cfg(test)]
pub(crate) mod test_utils;

// FFI layer (AD 0062 A.1).
//
// Marked `#[doc(hidden)]` rather than `pub(crate)` so internal
// integration tests under `tests/` (which link against the
// non-test build of the library) can still reach backend types
// they exercise directly — `tests/unrar_crc32_test.rs`,
// `tests/integration/directory_entry_single_file.rs`, and
// `tests/integration/link_skip_single_file.rs` deliberately probe
// backend-specific behaviour that the facade abstracts away. Doc
// generation skips the module so it stays out of the published API
// surface; new callers should rely on the facade re-exports.
#[doc(hidden)]
pub mod ffi;

// External tool integrations
#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub mod external;

// Public API re-exports
pub use archive::Archive;

/// D2 typed-handle split (R0068-0027 / AD 0053). Behind the
/// `v2-api` cargo feature in v0.3; v0.4 will enable it by default.
///
/// External callers opt in by enabling `v2-api`:
///
/// ```toml
/// unified-archive = { version = "0.3", features = ["v2-api"] }
/// ```
///
/// And import the typed handles via `unified_archive::v2`:
///
/// ```ignore
/// // requires --features v2-api
/// use unified_archive::v2::{ReadArchive, WriteArchive, ModifyArchive};
/// ```
#[cfg(feature = "v2-api")]
pub mod v2 {
    pub use crate::archive::mode_split::{ModifyArchive, ReadArchive, WriteArchive};
}
pub use entry::{ArchiveEntry, ArchiveEntryBuilder, EntryType, FileAttributes};
pub use error::{ArchiveError, Operation, Result};
pub use format::{ArchiveFormat, FormatCapabilities, Support};
pub use inspection::{MultipartLayout, ValidationReport};
pub use modification::ModificationOptions; // Phase 6: Archive modification
pub use options::{
    CompressionLevel, CompressionOptions, EntryFilter, ExtractionOptions,
    LibarchiveCompressionOptions, ProgressCallback, SevenZCompressionOptions, SfxStagingProgress,
    ZipCompressionOptions, entry_filter_from_fn,
};
pub use password::Password;
// Narrow the public surface to what callers reasonably
// audit. The raw policy fragments (`check_archive_ratio`,
// `check_extraction_safe`, `check_single_entry_safe`,
// `check_single_entry_safe_with_archive`, `sanitize_entry_path`,
// `verify_crc32`) accept caller-constructed
// entry slices and paths, so exposing them invited callers to drive
// the policy without the invariants the facade normally establishes.
// They are demoted to `pub(crate)` and the few that remain useful as
// public API are kept here.
pub use security::{Cap, CompressionRatio, ExtractionLimits, ExtractionLimitsBuilder};
pub use sfx::{SfxConfidence, SfxDetectionResult, StubType}; // Phase 7: SFX detection
pub use stream_crc::{
    CheckType, StreamChecksum, extract_bzip2_stream_crc, extract_gzip_stream_crc,
    extract_stream_checksum, extract_xz_stream_check,
};
pub use streaming::{StreamBound, StreamingExtractor}; // Phase 2.4 // Security utilities
