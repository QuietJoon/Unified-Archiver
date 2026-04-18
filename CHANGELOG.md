# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-18

### Pre-release gap closure (2026-04-18)

Four targeted gaps closed before tagging v0.1.0:

- **`Archive::open_at_offset(path, offset)`** is now fully implemented for every backend (ZIP via an `OffsetReader` adapter; 7z / TAR / ISO / RAR via a tempfile slice). Closes DEF-001 and unblocks `Archive::open_sfx` end-to-end.
- **Options-aware memory/stream extraction**: `Archive::extract_to_memory_with_options` and `Archive::extract_to_stream_with_options` let callers pass `ExtractionLimits` through to non-libarchive backends (Piz, ZipReader, SevenZ, UnRAR). Closes OI-0057-005.
- **ZIP modification metadata preservation**: `commit_changes` now preserves the archive-level EOCD comment and each entry's compression method (Stored vs Deflated) when rewriting a ZIP source. Closes DEF-005 (quick-wins scope); remaining ZIP-specific caveats (ZIP64 >4 GiB, encrypted re-encryption, crash-recovery journaling) are documented in `Limitations.md §4`.
- **SevenZ source-side streaming**: investigated and deferred — sevenz-rust2 0.19.4 exposes no owned entry-level `Read`, so the buffered `extract_to_stream` adapter stays. Recorded as AD 0035; OI-0057-007 closes as partial with the SevenZ row deferred to DEF-004 pending upstream support.

### Added

#### Core Functionality
- **Archive Operations**
  - `Archive::open()` - Open archives with automatic format detection
  - `Archive::open_encrypted()` - Open password-protected archives
  - `Archive::list_files()` - List all files with rich metadata
  - `Archive::find_entry()` - Find specific files by path
  - `Archive::is_encrypted()` - Check for password protection
  - `Archive::validate_integrity()` - Validate CRC32 checksums
  - **NEW** `Archive::create()` - Create archives in ZIP, 7z, TAR variants with compression and encryption
  - **NEW** `Archive::modify()` - Modify existing archives (add/remove/replace files)
  - **NEW** `Archive::detect_sfx()` - Detect self-extracting archives across platforms

#### Extraction Features
- **Multiple Extraction Methods**
  - `extract_all()` - Extract all files with options
  - `extract_file()` - Extract single file by path
  - `extract_filtered()` - Extract files matching predicate with parallel execution
  - `extract_to_memory()` - Extract directly to memory
  - `extract_to_stream()` - Stream-based extraction for large files

- **Progress Tracking**
  - `ProgressCallback` trait for monitoring extraction
  - Cancellable extraction via `ControlFlow::Break`
  - Rate-limited callbacks to avoid overhead

- **Parallel Extraction**
  - Automatic parallelization for 4+ files using rayon
  - Thread-safe archive handles per worker
  - Efficient work stealing for balanced CPU utilization

#### Format Support
- **RAR/RAR5** (via UnRAR SDK)
  - Full read/extract support
  - Password-protected archives
  - Multi-part archives (automatic)
  - Direct CRC32 access from metadata
  - Encrypted header detection

- **ZIP, 7z, TAR** (via libarchive)
  - Full read/extract support
  - All TAR compression variants (GZ, BZ2, XZ)
  - Computed CRC32 checksums
  - Automatic format detection

#### Metadata Access
- **Rich Entry Metadata**
  - File sizes (uncompressed and compressed)
  - CRC32 checksums (all formats)
  - Timestamps (modified, created, accessed)
  - File permissions (Unix mode bits)
  - Encryption status
  - Compression ratios
  - Entry types (File, Directory, Symlink, Other)

#### Performance Features
- **Memory Efficiency**
  - Streaming extraction <100MB memory for GB+ files
  - Chunk-based reading with configurable buffer sizes
  - No full-file loads unless explicitly requested

- **SIMD Acceleration**
  - CRC32 computation using `crc32fast` (~300MB/s)
  - Optimized for modern CPU architectures

- **Thread Safety**
  - Parallel extraction with per-thread archive handles
  - Unique timestamp-based temporary directories
  - No global state or race conditions in callers; UnRAR's internal global state is mediated by an internal `UNRAR_LOCK` mutex so concurrent callers on different RAR archives are safe. Per-archive extraction still runs sequentially due to the SDK's iterator shape.

#### Error Handling
- **Comprehensive Error Types**
  - `ArchiveError::Io` - Missing files
  - `ArchiveError::Unsupported` - Unknown formats
  - `ArchiveError::Password` - Password issues
  - `ArchiveError::Corruption` - CRC32 failures
  - `ArchiveError::Io` - I/O errors with context
  - `ArchiveError::Format` - Format-specific errors

- **Automatic CRC32 Verification**
  - Enabled by default during extraction
  - Detects corruption immediately
  - Clear error messages with file paths

#### Developer Experience
- **Examples**
  - `inspect_archive.rs` - Archive inspection demo
  - `extract_archive.rs` - Extraction with progress
  - `streaming_extract.rs` - Memory-efficient extraction
  - `test_extract.rs` - Simple extraction example

- **Documentation**
  - Comprehensive README with usage examples
  - API reference documentation
  - Inline code documentation
  - Known limitations documented

- **Testing**
  - Comprehensive test suite covering all features (400+ tests including unit, integration, contract, and doc tests)
  - Performance benchmarks
  - Property-based tests with proptest
  - Multi-format test fixtures

### Fixed

#### Thread Safety Issues
- Fixed parallel test failures caused by `chdir()` usage
- Implemented absolute path handling for extraction
- Added nanosecond timestamps to temporary directories
- Eliminated race conditions in cleanup operations

#### CRC32 Computation
- Implemented CRC32 computation for ZIP/7z/TAR formats
- Fixed libarchive backend to read and hash file data during listing
- Ensured consistent CRC32 availability across all formats

#### Documentation
- Updated README to reflect current implementation status
- Corrected format support table
- Added missing error handling examples
- Documented all limitations and workarounds

### Changed

#### Project Naming
- Renamed from "7zip-RBinding" to "unified-archive"
- Updated to reflect unified interface philosophy
- Consistent branding across documentation

#### API Design
- `ExtractionOptions` uses builder pattern with `..Default::default()`
- Progress callbacks use `std::ops::ControlFlow` for cancellation
- Consistent `Result<T>` return types across all methods

#### Backend Architecture
- Piz (ZIP), zip crate (encrypted ZIP), SevenZ (7z), libarchive (TAR family — `.tar`, `.tar.gz`, `.tar.bz2`, `.tar.xz` — and ISO; standalone `.gz`/`.bz2`/`.xz` are out of scope, see AD 0018), UnRAR (RAR/RAR5)
- Automatic backend selection based on format detection
- Thread-local archive handles for parallel operations

### Performance

- **CRC32**: ~300MB/s using SIMD-accelerated hashing
- **Parallel Extraction**: Linear scaling up to available CPU cores
- **Memory Usage**: <100MB for multi-GB files with streaming (Note: This bound currently applies to libarchive-backed formats (TAR family). ZIP, 7z, and RAR backends buffer full entries in memory during streaming extraction.)
- **Format Detection**: O(1) single header read

### Technical Details

#### Dependencies
- `crc32fast` 1.4 - SIMD-accelerated CRC32
- `rayon` 1.8 - Parallel extraction
- `secstr` 0.5 - Secure password storage
- `once_cell` 1.20 - Entry caching
- `libarchive` (system) - Multi-format support
- `UnRAR SDK` (bundled) - RAR/RAR5 support

#### Platform Support
- ✅ macOS (tested on Darwin 24.6.0)
- ✅ Linux (Ubuntu/Debian, Fedora/RHEL)
- ⏳ Windows (untested, may need adjustments)

#### Rust Version
- Minimum: Rust 1.85+ (stable)
- Edition: 2024

### Security

- Passwords are stored as `Option<SecStr>` (via the `secstr` crate). Password bytes are zeroed on drop; decoding to `&str` happens only at the FFI boundary.
- No password leakage in error messages or logs
- Secure temporary file creation with unique names
- Permission preservation for extracted files

### Known Limitations

See [Limitations.md](./Limitations.md) for complete details.

**Key Limitations in v0.1.0:**
- Multi-part archive creation deferred to v0.2.0 (reading supported)
- Progress callbacks during creation report per-entry events with `total = None` (file count not pre-computed)
- RAR format is read-only (no creation support due to proprietary format)
- UnRAR iterator exhaustion requires reopening archive
- DOS timestamp conversion approximation for old files
- Temporary file usage for `extract_to_memory()`
- SFX detection has substantial unit and integration test coverage including synthetic PE/ELF/Mach-O/script stubs, signature scanning, and false-positive tests

### Migration Notes

This is the initial release (v0.1.0), no migration needed.

Future breaking changes will follow semantic versioning:
- Major version (2.0.0) for breaking API changes
- Minor version (0.2.0) for new features
- Patch version (0.1.1) for bug fixes

---

## [Unreleased]

### Planned for v0.2.0
- Multi-part archive creation support (T058)
- Additional inline documentation examples (T112)
- Contract tests for API stability (T113-T115)
- Performance benchmarks for all operations (T116-T120, T127)
- Improved streaming extraction (true RAR streaming via callbacks)
- Windows platform comprehensive testing and fixes

### Under Consideration
- ISO format creation (currently read-only)
- Additional archive formats (ARJ, LZH, CAB)
- Archive comment support on non-ZIP formats (ZIP-level comment round-trips in v0.1.0)
- Extended attributes preservation
- Sparse file support

---

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md with release date
3. Run full test suite: `cargo test --release`
4. Tag release: `git tag -a v0.1.0 -m "Release v0.1.0"`
5. Push tag: `git push origin v0.1.0`
6. Publish to crates.io: `cargo publish`

---

**Legend:**
- Added: New features
- Changed: Changes in existing functionality
- Deprecated: Soon-to-be removed features
- Removed: Removed features
- Fixed: Bug fixes
- Security: Security improvements
