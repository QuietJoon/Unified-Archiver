//! # unified-archive
//!
//! Unified, ergonomic Rust library for archive inspection, extraction, and creation.
//!
//! This library provides a consistent, high-level API for working with multiple archive formats
//! (RAR, RAR5, ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZIP, BZIP2, XZ, ISO) without requiring
//! format-specific code. Write once, work with any format.
//!
//! ## Features
//!
//! - ✨ **Unified Interface** - Same API for all formats
//! - 🔍 **Automatic Format Detection** - Magic byte and extension-based detection
//! - 🚀 **High Performance** - SIMD CRC32, streaming architecture
//! - 💾 **Memory Efficient** - <100MB memory for multi-GB archives (for libarchive-backed formats; native ZIP/7z/RAR backends buffer entries during streaming)
//! - 🔐 **Password Support** - Encrypted archives (RAR, RAR5, ZIP, 7z)
//! - ✅ **Integrity Validation** - CRC32 verification
//! - 📊 **Rich Metadata** - Sizes, timestamps, CRC32, permissions
//! - 🛡️ **SFX Detection** - Identify self-extracting archives across platforms
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
//! archive.extract_all(options)?;
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
//! ```no_run
//! use unified_archive::Archive;
//! use std::io::Read;
//!
//! let archive = Archive::open("large.zip")?;
//! let mut stream = archive.extract_to_stream("huge_file.bin")?;
//!
//! let mut buffer = [0u8; 8192];
//! while let Ok(n) = stream.read(&mut buffer) {
//!     if n == 0 { break; }
//!     // Process chunk — note that actual streaming behavior depends on backend
//!     // (libarchive truly streams; native backends may buffer)
//! }
//! # Ok::<(), unified_archive::ArchiveError>(())
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
//!     password: Some("secret".to_string()),
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
//! if result.is_sfx {
//!     println!("Found {:?} archive at offset {}",
//!         result.archive_format.unwrap(),
//!         result.data_offset.unwrap());
//!     println!("Stub type: {:?}", result.stub_type);
//! }
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```
//!
//! ## Supported Formats
//!
//! | Format    | Read | Extract | Create | CRC32 | Encryption | SFX Detection |
//! |-----------|------|---------|--------|-------|------------|---------------|
//! | **RAR**   | ✅   | ✅      | 🪟*    | ✅    | ✅         | ✅            |
//! | **RAR5**  | ✅   | ✅      | 🪟*    | ✅    | ✅         | ✅            |
//! | **ZIP**   | ✅   | ✅      | ✅     | ✅    | ✅         | ✅            |
//! | **7z**    | ✅   | ✅      | ✅     | ✅    | ✅†        | ✅            |
//! | **TAR**   | ✅   | ✅      | ✅     | ✅    | ❌         | ❌            |
//! | **TAR.GZ**| ✅   | ✅      | ✅     | ✅    | ❌         | ❌            |
//! | **TAR.BZ2**| ✅  | ✅      | ✅     | ✅    | ❌         | ❌            |
//! | **TAR.XZ**| ✅   | ✅      | ✅     | ✅    | ❌         | ❌            |
//! | **GZIP**  | ✅‡  | ✅‡     | ❌     | ✅    | ❌         | ❌            |
//! | **BZIP2** | ✅‡  | ✅‡     | ❌     | ✅    | ❌         | ❌            |
//! | **XZ**    | ✅‡  | ✅‡     | ❌     | ✅    | ❌         | ❌            |
//! | **ISO**   | ✅   | ✅      | ❌     | ⏳    | ❌         | ❌            |
//!
//! *🪟 RAR creation requires WinRAR on Windows with `external-rar-create` feature flag*
//! *† 7z encryption: read-only via `open_encrypted()`*
//! *‡ Standalone `.gz`/`.bz2`/`.xz` files are not yet supported. These formats currently
//! only work as part of TAR compound formats (`.tar.gz`, `.tar.bz2`, `.tar.xz`).*
//!
//! ## Performance
//!
//! - **CRC32**: SIMD-accelerated checksum validation
//! - **Memory**: <100MB for multi-GB archives with libarchive-backed formats (streaming architecture);
//!   native ZIP (Piz), 7z (SevenZ), and RAR (UnRAR) backends buffer individual entries in memory
//!   during streaming extraction
//! - **Detection**: <100ms for SFX detection (first 1MB scan)
//!
//! ## Architecture
//!
//! This library uses four backend engines:
//! - **piz**: ZIP read/extract (memory-mapped, default for unencrypted ZIP)
//! - **zip**: ZIP read/extract for encrypted archives, ZIP creation
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
pub mod entry;
pub mod error;
pub mod format;
pub mod options;
pub mod security; // Security utilities
pub mod sfx;
pub mod stream_crc; // Stream-level CRC32 extraction
pub mod streaming; // Phase 2.4: Streaming extraction // Phase 7: SFX detection

// Operation modules
pub mod creation;
pub mod extraction;
pub mod inspection;
pub mod modification;

#[cfg(test)]
pub(crate) mod test_utils;

// FFI layer (public for testing)
pub mod ffi;

// External tool integrations
#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub mod external;

// Public API re-exports
pub use archive::Archive;
pub use entry::{ArchiveEntry, EntryType, FileAttributes};
pub use error::{ArchiveError, Result};
pub use format::ArchiveFormat;
pub use inspection::ValidationReport;
pub use modification::ModificationOptions; // Phase 6: Archive modification
pub use options::{
    CompressionLevel, CompressionOptions, EntryFilter, ExtractionOptions, ProgressCallback,
};
pub use security::{
    ExtractionLimits, check_archive_ratio, check_extraction_safe, sanitize_entry_path,
    validate_entry_path, verify_crc32,
};
pub use sfx::{SfxDetectionResult, StubType}; // Phase 7: SFX detection
pub use stream_crc::{
    CheckType, StreamChecksum, extract_bzip2_stream_crc, extract_gzip_stream_crc,
    extract_stream_checksum, extract_xz_stream_check,
};
pub use streaming::StreamingExtractor; // Phase 2.4 // Security utilities
