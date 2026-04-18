# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-04-19

Post-release hardening pass. No public API break; every change is either a
tightened internal invariant, a docs-alignment fix, or a reviewer-driven
safety gate.

### Added

- **Archive-internal path validation at the write-side facade.** `Archive::add_file_from_data`, `add_file_from_path_as`, `add_directory`, `add_entry`, `add_directory_entry`, and `remove_entry` now reject empty, NUL-containing, traversal (`..`), and absolute names up front with `ArchiveError::InvalidPath` instead of forwarding them to the backend and letting the extractor silently rewrite them on read-back (AD 0044, closes R0063-0001..0006).
- **Non-UTF-8 password rejection at the FFI boundary.** `CompressionOptions::password_as_str` now returns `Result<&str, ArchiveError::InvalidArgument>`; password-capable backends propagate the error instead of silently dropping or corrupting non-UTF-8 bytes (AD 0042).
- **SFX payload size ceiling.** `Archive::open_sfx` now caps embedded payload extraction at 16 GiB (AD 0040) to bound tempfile growth on hostile or malformed SFX binaries.

### Changed

- **Hermetic UnRAR build.** `build.rs` now stages UnRAR sources into `OUT_DIR` rather than compiling from the source tree, eliminating cross-crate-build working-tree contamination (AD 0039).
- **Dropped hardcoded `/Volumes/Temp/claude` preference from the library.** Tempfile selection now uses the system defaults exclusively; scratch-path preference is a test-harness concern, not a library concern (AD 0041).
- **Inspection contract asserts semantic stability, not pointer identity.** `contract_list_files_caching_same_pointer` → `contract_list_files_caching_stable`: repeated `list_files()` calls must return the same entries (same path / size / crc32), not necessarily the same backing slice.
- **SFX stage-3 rustdoc narrowed** and the misleading "CD signature" test-assertion message refreshed (Review 0062 closure).
- **Examples migrated to `tempfile::tempdir()`** and dropped stale SFX prose (Review 0062).
- **Doc alignment** across `Limitations.md`, `README.md`, `docs/GETTING_STARTED.md`, `docs/README.md`, `src/lib.rs`, and `src/inspection.rs` to match the shipped v0.1.0 capability surface (split-volume is RAR-only end-to-end, encrypted ZIP is read-only, `Archive::create()` rejects passwords, standalone `.gz/.bz2/.xz` are read-only).

### Fixed

- **DEF-001 closure recorded in the impact report.** The tempfile-backed `open_at_offset()` / `open_sfx()` implementation that shipped in v0.1.0 is now reflected in `docs/project/implementation-impact-report.md` so future readers don't re-open the gap.

### Decision records

- AD 0039 — Hermetic UnRAR build + staged artifact purge
- AD 0040 — SFX payload size ceiling (16 GiB)
- AD 0041 — Remove hardcoded temp path from library
- AD 0042 — `password_as_str` returns `Result` on non-UTF-8
- AD 0043 — Reject R0062-0007 (tempdir-security concern already resolved)
- AD 0044 — Validate archive-internal paths at the creation/modification facade boundary
- AD 0045 — Reject R0063 low-cluster doc-drift sweep; already covered by OI-0057-009 + post-v0.1.0 banners

## [0.1.0] - 2026-04-18

### Pre-release gap closure (2026-04-18)

Four targeted gaps closed before tagging v0.1.0:

- **`Archive::open_at_offset(path, offset)`** is now fully implemented for every backend (ZIP via an `OffsetReader` adapter; 7z / TAR / ISO / RAR via a tempfile slice). Closes DEF-001 and unblocks `Archive::open_sfx` end-to-end.
- **Options-aware memory/stream extraction**: `Archive::extract_to_memory_with_options` and `Archive::extract_to_stream_with_options` let callers pass `ExtractionLimits` through to non-libarchive backends (Piz, ZipReader, SevenZ, UnRAR). Closes OI-0057-005.
- **ZIP modification metadata preservation**: `commit_changes` now preserves the archive-level EOCD comment and each entry's compression method (Stored vs Deflated) when rewriting a ZIP source. Closes DEF-005 (quick-wins scope); remaining ZIP-specific caveats (ZIP64 >4 GiB, encrypted re-encryption, crash-recovery journaling) are documented in `Limitations.md`.
- **SevenZ source-side streaming**: investigated and deferred — sevenz-rust2 0.19.4 exposes no owned entry-level `Read`, so the buffered `extract_to_stream` adapter stays. Recorded as AD 0035; OI-0057-007 closes as partial with the SevenZ row deferred to DEF-004 pending upstream support.

### Documentation polish (2026-04-18)

- Added a public [user manual](./docs/USER_MANUAL.md) for `v0.1.0`
- Aligned `README.md`, `docs/README.md`, `docs/API_REFERENCE.md`, `docs/GETTING_STARTED.md`, and `Limitations.md` around the same capability story
- Corrected outdated claims around encrypted creation, standalone `.gz/.bz2/.xz` support, SFX opening, and split-volume support

### Added

#### Core Functionality
- **Archive Operations**
  - `Archive::open()` - Open archives with automatic format detection
  - `Archive::open_encrypted()` - Open password-protected archives
  - `Archive::list_files()` - List all files with rich metadata
  - `Archive::find_entry()` - Find specific files by path
  - `Archive::is_encrypted()` - Check for password protection
  - `Archive::validate_integrity()` - Validate CRC32 checksums
  - **NEW** `Archive::create()` - Create archives in ZIP, 7z, and TAR variants with configurable compression
  - **NEW** `Archive::modify()` - Modify existing archives (add/remove/replace files)
  - **NEW** `Archive::detect_sfx()` - Detect self-extracting archives across platforms
  - **NEW** `Archive::open_sfx()` / `Archive::open_at_offset()` - Open embedded archive payloads discovered inside self-extracting binaries

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

- **ZIP**
  - Full read/extract support
  - Native ZIP creation
  - Encrypted reads via the `zip` crate backend

- **7z**
  - Full read/extract support
  - Native 7z inspection/extraction
  - 7z creation through the libarchive-backed creation path

- **TAR family / ISO / raw compressed formats**
  - TAR, TAR.GZ, TAR.BZ2, TAR.XZ, and ISO support via libarchive
  - Standalone `.gz`, `.bz2`, and `.xz` read/extract support via libarchive `format_raw`
  - TAR-family creation through libarchive

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
  - Avoids full-file loads on libarchive-backed streaming paths; some native backends still buffer entries

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
  - `detect_sfx.rs` - SFX detection and embedded payload opening

- **Documentation**
  - Comprehensive README with usage examples
  - API reference documentation
  - Inline code documentation
  - Known limitations documented

- **Testing**
  - Comprehensive test suite covering unit, integration, contract, and doc tests
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
- Added a consolidated public user manual

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
- Piz (ZIP read), zip crate (encrypted ZIP read + ZIP creation), SevenZ (7z read), libarchive (TAR family creation/read, ISO, standalone `.gz`/`.bz2`/`.xz` read), UnRAR (RAR/RAR5)
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
- `Archive::create()` rejects password-based encrypted creation for every format
- Split-volume support is limited to RAR / RAR5
- Progress callbacks during creation report per-entry events with `total = None` (file count not pre-computed)
- RAR format is read-only through the main `Archive` facade
- Windows support is not release-verified yet
- Some workflows intentionally use temporary files (`open_at_offset`, UnRAR `extract_to_memory`)
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
