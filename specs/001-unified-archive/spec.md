# Feature Specification: unified-archive

**Feature Branch**: `001-unified-archive`
**Created**: 2025-10-30
**Updated**: 2025-11-11 (Added SFX detection feature)
**Status**: Implemented (with tracked gaps)
**Input**: User description: "unified-archive project. This is a unified interface library inspired by 7zip-JBinding's design philosophy. Provides format-agnostic API for archive operations (inspection, extraction, creation, modification, SFX detection) across multiple formats with cross-platform compatibility (Windows, macOS, Linux) and zero JVM dependency."

> **Post-v0.1.0 reality check (2026-04-18):** This spec was written during planning and has not been line-edited to match the shipped 0.1.0 crate. Canonical references for current behavior are the source tree, `docs/API_REFERENCE.md`, and the `docs/architecture/decisions/*.md` records. Known deltas still readable below: (1) `Archive::open_at_offset()` and `Archive::open_sfx()` are **shipped**, not deferred; (2) passwords are `Option<SecStr>` (`secstr` is integrated, not deferred); (3) the error enum no longer has an `UnsupportedOperation` variant — use `WriteModeOnly` / `ReadOnlyBackend` / `NotImplemented` / `OperationBlocked` per `src/error.rs`; (4) extraction entry points return `Result<ResultWithWarnings<()>>` per AD 0010; (5) standalone `.gz` / `.bz2` / `.xz` read and extract are supported per AD 0019 (only *creation* of standalone streams is excluded, per AD 0018).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Archive Inspection (Priority: P1, MVP)

A Rust developer needs to inspect archive contents (list files, check metadata, validate integrity) **using a shared API surface with documented format caveats** across all supported archive formats (7z, RAR, RAR5, ZIP, TAR variants, ISO) without extracting the entire archive. **Note**: Standalone GZIP, BZIP2, and XZ are not supported; only TAR compound formats (.tar.gz, .tar.bz2, .tar.xz) per AD 0018. This includes retrieving file modification dates, file sizes, and format-dependent optional fields (compressed size, CRC32) for each file.

**Why this priority**: Inspection is the foundational operation that enables all other archive workflows. Developers must first understand what's inside an archive before deciding whether to extract, which files to extract, or validating archive integrity. This is the lightweight, essential first step that makes the library immediately useful for archive analysis, validation, and management tasks. **The unified interface eliminates format-specific code, allowing developers to write once and support all formats.**

**Independent Test**: Can be fully tested by listing archive contents across different formats (7z, RAR, RAR5, ZIP, TAR.GZ), retrieving file metadata (modification date, size, CRC32) using identical API calls, and comparing against known archive structures. Delivers standalone value as a format-agnostic archive browser/validator without requiring extraction or creation capabilities.

**Acceptance Scenarios**:

1. **Given** archives in different formats (ZIP, 7z, RAR, RAR5, TAR.GZ), **When** developer uses the same inspection API for all formats, **Then** all files with their paths and uncompressed sizes are returned in a consistent structure; compressed sizes, modification dates, and CRC32 checksums are format-dependent and may be `None` for some backends
2. **Given** a password-protected archive (any format), **When** developer inspects the file list without password, **Then** filenames and metadata (size, date, CRC32) are accessible but content remains protected, with consistent behavior across formats. **Caveat**: Header-encrypted archives (e.g., RAR with header encryption) may not expose metadata without the correct password.
3. **Given** an archive (any format), **When** developer checks integrity using CRC32 validation, **Then** any corruption is detected and reported with specific file details using the same error handling pattern
4. **Given** a multi-part RAR archive, **When** inspecting contents, **Then** all parts are recognized and the complete file list with accurate metadata is returned through the unified interface. **Note**: Multi-part reading is currently supported for RAR only (FR-019).
5. **Given** archives with thousands of files (various formats), **When** listing contents, **Then** results can be filtered by path patterns or file attributes while preserving metadata accuracy, using caller-side filter APIs applied to the returned entry list

---

### User Story 2 - Archive Extraction (Priority: P2, MVP)

A Rust developer needs to extract files from compressed archives **using a shared API surface with documented format caveats** (ZIP, 7z, RAR, RAR5, TAR.GZ, TAR.BZ2, TAR.XZ, ISO) without depending on external system tools or JVM runtime. The same extraction API shape handles format detection, decompression, and file extraction across all formats, though backend-specific behavior differences exist. **Note**: Standalone GZIP, BZIP2, and XZ files are not supported through `Archive::open()`; only TAR compound formats are in scope (AD 0018).

**Why this priority**: After inspecting archive contents, extraction is the second essential operation. Together with inspection (P1), these two features form the complete "read" capability for archives, enabling developers to both understand and access archive contents. This MVP combination delivers immediate practical value for archive consumption workflows. **The unified interface means developers write one extraction code path that automatically handles all formats, just like 7zip-JBinding achieved.**

**Independent Test**: Can be fully tested by creating archives of various formats (7z, RAR, RAR5, ZIP, TAR.GZ, TAR.BZ2, TAR.XZ), extracting them programmatically using identical API calls, and verifying extracted file content matches the original files across all formats. Note: BZIP2 and XZ are only supported as TAR compound formats (see AD 0018). This delivers standalone utility for format-agnostic archive extraction use cases.

**Acceptance Scenarios**:

1. **Given** archives in different formats (ZIP, 7z, RAR, RAR5, TAR.GZ), **When** developer uses the same extract API with archive path and destination directory, **Then** all files are extracted to the destination with correct filenames, directory structure, and content, regardless of format
2. **Given** password-protected archives in different formats (7z, ZIP, RAR), **When** developer provides the correct password using the same API pattern, **Then** files are successfully decrypted and extracted consistently across formats
3. **Given** multi-layer compressed archives (TAR.GZ, TAR.BZ2, TAR.XZ), **When** developer extracts using the unified API, **Then** the multi-layer compression is handled transparently with format auto-detection
4. **Given** corrupted or invalid archives of various formats, **When** developer attempts extraction, **Then** clear format-consistent errors are returned indicating the archive is invalid or corrupted
5. **Given** archives containing 1000+ files in various formats, **When** extraction is initiated, **Then** progress can be monitored throughout the extraction process using the same callback interface across all formats; actual progress cadence and granularity are backend-dependent

---

### User Story 3 - Archive Creation (Priority: P3)

A Rust developer needs to create compressed archives from files and directories **using a unified interface that supports multiple formats** (ZIP, 7z, TAR.GZ, TAR.BZ2, TAR.XZ, etc.) with consistent API calls regardless of the chosen output format. Developers should specify the desired format once and use the same creation methods for all formats.

**Why this priority**: Creation enables "write" capability, complementing the "read" operations (inspection and extraction). While valuable for complete archive management workflows and backup solutions, many applications only need to read existing archives, making creation a high-value but non-MVP feature. **The unified interface ensures format selection doesn't require different code paths, matching 7zip-JBinding's design.**

**Independent Test**: Can be tested by creating archives in various formats (7z, ZIP, TAR.GZ) using identical API calls with only format parameter differing, then verifying the archives can be extracted by standard tools (7zip, WinRAR) and match the original content. Delivers value as a format-agnostic compression utility.

**Acceptance Scenarios**:

1. **Given** a directory containing files, **When** developer creates archives in different formats (ZIP, 7z, TAR.GZ) using the same creation API, **Then** all files and subdirectories are included with relative paths preserved, with format-appropriate compression
2. **Given** multiple files from different locations, **When** developer adds them to archives in various formats with maximum compression setting, **Then** archives are created with optimal compression ratios appropriate to each format
3. **Given** sensitive files, **When** developer creates password-protected archives, **Then** creation-time encryption is currently ZIP-only (AES-256 via ZipWriter); 7z creation encryption is future work. Resulting archives cannot be extracted without the correct password.
4. **Given** a large dataset (10GB+), **When** creating archives in various formats, **Then** memory usage remains bounded for libarchive-backed formats; native backends (e.g., ZipWriter) may buffer entries and have higher memory usage. Progress monitoring during creation is supported per-entry with `total=None` (AD 0021 / OI-025-003 resolved).
5. **Given** files already compressed (JPEG, MP4), **When** adding to archives in any format, **Then** compression level can be adjusted uniformly across formats to avoid unnecessary processing overhead

---

### User Story 4 - Archive Modification (Priority: P4)

A Rust developer needs to update existing archives **using a unified interface** by adding, removing, or replacing files via copy-on-write full rewrite through `commit_changes()`, with consistent API behavior across modifiable formats (ZIP, 7z). **Note**: TAR modification is not supported. `commit_changes()` preserves per-entry metadata (timestamps, Unix permissions) when `preserve_metadata` is enabled (default), and respects `ModificationOptions.compression` for compression level and password.

**Why this priority**: Modification is an advanced feature useful for incremental backups and archive maintenance. Most applications only need extract/create, making this a nice-to-have enhancement. **The unified interface ensures modification operations use consistent patterns regardless of archive format, where supported.**

**Independent Test**: Can be tested by adding files to, removing entries from, and replacing files in existing archives of various formats using identical API calls, verifying changes are persisted correctly across formats. Delivers value as a format-agnostic archive maintenance utility.

**Acceptance Scenarios**:

1. **Given** existing archives in different modifiable formats (ZIP, 7z), **When** developer adds new files using the same API, **Then** files are added via full archive rewrite (copy-on-write semantics through `commit_changes()`), with format-appropriate behavior
2. **Given** archives with outdated files in various formats, **When** developer removes specific entries using the unified API, **Then** those files are deleted via full archive rewrite and archive size is reduced accordingly
3. **Given** archives in different formats, **When** developer replaces a file with an updated version using the same API pattern, **Then** all entries are rewritten; logical replacement semantics are preserved (the replaced entry uses new content)
4. **Given** large archives in various formats, **When** performing modifications, **Then** temporary disk space scales with archive size (copy-on-write rewrite requires space proportional to the full archive)

---

### User Story 5 - Self-Extracting Archive (SFX) Detection (Priority: P5)

A Rust developer needs to identify whether a file is a self-extracting archive (SFX) **using a unified interface** that detects SFX markers across different platforms (Windows PE executables, Linux ELF executables, macOS Mach-O executables, and ScriptInterpreter stubs) and archive formats (ZIP SFX, RAR SFX, 7z SFX).

**Why this priority**: SFX detection is an advanced inspection capability that enables security scanning, automated archive processing, and intelligent file classification. Many users distribute archives as SFX executables for ease of use, and applications need to identify and handle these correctly. While not essential for basic archive operations, SFX detection adds significant value for security-conscious applications, malware analysis tools, and enterprise file management systems.

**Independent Test**: Can be tested by creating SFX archives using various tools (7-Zip SFX, WinRAR SFX, Unix ScriptInterpreter-based SFX) on different platforms, then detecting SFX files and reporting embedded archive metadata (format, offset, confidence). The detection API should return both the SFX status and the embedded archive format. Direct extraction from detected SFX offsets is deferred. **Current status**: All SFX test archives are synthetic (hand-crafted stubs); official-tool SFX testing is planned. This delivers standalone value for security scanning and intelligent file classification without requiring other archive operations.

**Acceptance Scenarios**:

1. **Given** a Windows executable created as ZIP SFX, **When** developer checks if file is SFX, **Then** library returns true with detected archive format (ZIP) and the offset where the archive data begins
2. **Given** a WinRAR SFX executable (.exe), **When** developer inspects the file using the unified API, **Then** library identifies it as SFX with format RAR or RAR5 and provides the embedded archive offset
3. **Given** a 7-Zip SFX executable on Windows, **When** developer checks SFX status, **Then** library returns true with format 7z and the archive data location
4. **Given** a Linux ELF executable or macOS Mach-O executable containing an embedded ZIP or 7z archive, **When** developer uses SFX detection, **Then** library identifies it as SFX with the correct archive format (per AD 0015: TAR signatures removed from SFX detection)
5. **Given** a Unix ScriptInterpreter-based SFX with an embedded ZIP archive, **When** developer checks SFX status, **Then** library detects the embedded archive and returns the archive format and data offset (per AD 0015: only non-TAR archive signatures are scanned)
6. **Given** a regular archive file (non-SFX ZIP, RAR, 7z), **When** developer checks if file is SFX, **Then** library returns false indicating it's a standard archive
7. **Given** a non-archive executable file (regular .exe or ELF binary), **When** developer checks SFX status, **Then** library returns `SfxDetectionResult::not_sfx()` (a successful result, not an error) indicating no embedded archive found
8. **[RESOLVED per OI-027-001]** **Given** an SFX file with unknown or custom stub, **When** developer attempts detection, **Then** the implementation classifies the stub as `StubType::Unknown` and proceeds to signature scanning; if a recognized archive signature is found, `probable()` with confidence 0.9 is returned.

---

### Edge Cases

- **[RESOLVED]** Symbolic links and hard links are skipped with warnings during extraction and creation (FR-022). All backends now classify link entries: Piz (unix_mode), ZipReader (is_symlink), SevenZ (windows_attributes), UnRAR (redir_type), and libarchive (native metadata). OI-010-001 resolved.
- **[RESOLVED]** When archive requires unavailable codec, library returns ArchiveError::CodecUnavailable with codec name, format, and installation instructions (FR-024)
- **[RESOLVED]** Extraction fails with clear error when files would overwrite existing files, unless ExtractionOptions.overwrite is true (FR-023)
- **[PARTIAL]** SFX detection performs a bounded heuristic scan of the first 1MB with confidence-based results for known stub types (PE, ELF, Mach-O, ScriptInterpreter); archives with stubs >1MB return `SfxDetectionResult::not_sfx()` as a performance trade-off. Unknown/custom stubs now proceed to signature scanning (OI-027-001 resolved). All current SFX tests use synthetic stubs; official-tool SFX archives are planned. (FR-025)
- **[RESOLVED]** For SFX files with multiple embedded archives (rare), library iterates all candidates and returns the first validating candidate's format and offset (per AD 0015) (FR-026)
- **[RESOLVED]** Archives with filenames containing special characters or Unicode are handled using UTF-8 encoding with platform-specific fallback (Assumption 13)
- **[RESOLVED]** Archives larger than available RAM are handled through streaming APIs ensuring <100MB memory for libarchive-backed formats; native backends (Piz, ZipReader, SevenZ, UnRAR) may buffer entries (FR-012)
- **[RESOLVED]** Extraction with insufficient disk space returns ArchiveError::Io with clear error message from underlying filesystem
- **[RESOLVED with limitation]** Concurrent access to same archive file behavior follows typical file handle semantics - multiple readers allowed, writers require exclusive access (FR-020, FR-021 clarified in T120a-c). **RAR note (OI-026-004 resolved)**: UnRAR FFI calls are now serialized via a process-wide mutex (`UNRAR_LOCK`); concurrent caller access is safe but RAR operations run sequentially under the hood.
- **[RESOLVED]** File permissions preserved where supported by archive format and OS; cross-platform handling uses best-effort mapping (FR-015)
- **[RESOLVED]** Missing or wrong passwords return consistent ArchiveError::Password across all formats (FR-014, implemented in T047b)
- **[RESOLVED]** Archives with non-standard extensions detected via magic bytes; missing magic bytes return ArchiveError::Format
- **[RESOLVED]** SFX detection handles corrupted embedded archives gracefully — if a signature is found but archive parsing fails, `SfxDetectionResult::not_sfx()` is returned (FR-030). Direct extraction from SFX offsets is deferred (FR-029).
- **[RESOLVED]** Platform-specific executable formats (PE/ELF/Mach-O) detected via goblin crate, shell scripts via shebang (FR-027, implemented in T093-T097)

## Requirements *(mandatory)*

### Functional Requirements

#### Unified Interface Requirements (Critical)

- **FR-001**: Library MUST provide a shared API surface for all archive operations (inspection, extraction, creation, modification) across all supported formats (7z, RAR, RAR5, ZIP, TAR.GZ, TAR.BZ2, TAR.XZ, ISO) with documented format caveats where backend behavior diverges. **Note**: Standalone GZIP, BZIP2, and XZ are not supported through `Archive::open()`; only TAR compound formats are in scope (AD 0018).
- **FR-002**: Library MUST automatically detect archive format from file content (magic bytes) without requiring explicit format specification from developers
- **FR-003**: Library MUST return archive metadata in a shared `ArchiveEntry` structure across all formats; field population is format-dependent (e.g., compressed_size and CRC32 may be `None` for some backends)
- **FR-004**: Library MUST minimize exposure of format-specific APIs by handling format-specific features (encryption methods, compression algorithms, multi-part archives) through the unified interface where feasible; documented format caveats are expected where full transparency is impractical

#### Format Support Requirements

- **FR-005**: Library MUST support extraction of archives in ZIP, 7z, RAR, RAR5, TAR.GZ, TAR.BZ2, TAR.XZ, and ISO formats. **Note**: Standalone GZIP, BZIP2, and XZ are not supported through `Archive::open()`; only TAR compound formats are in scope (AD 0018). The `ArchiveFormat::Gzip/Bzip2/Xz` enum variants exist but are deferred for standalone use.
- **FR-006**: Library MUST support creation of archives in ZIP, 7z, and TAR-based formats (TAR.GZ, TAR.BZ2, TAR.XZ) at minimum
- **FR-007**: Library MUST support RAR5 format in addition to legacy RAR format for complete compatibility

#### Platform & Runtime Requirements

- **FR-008**: Library targets cross-platform support on Windows, macOS, and Linux without platform-specific code in the public API. **Current status**: Primary development is macOS; Linux and Windows are targets under verification (known `wchar_t` edge cases on Linux).
- **FR-009**: Library MUST operate independently of JVM runtime (pure Rust implementation or FFI bindings to native 7zip libraries)

#### Error Handling & Robustness Requirements

- **FR-010**: Library MUST provide error handling that distinguishes between broad error categories (I/O, format, corruption, password, codec) with consistent error types across all formats; error detail granularity varies by backend and is not fully normalized
- **FR-011**: Library MUST validate archive integrity through CRC checks where supported by the format
- **FR-012**: Memory usage scales with backend; see plan.md for per-module notes. True streaming (libarchive) maintains bounded memory (<100MB for multi-GB archives); native backends (Piz, ZipReader, SevenZ, UnRAR) may buffer entries and have higher memory usage proportional to entry size.

#### Security & Encryption Requirements

- **FR-013**: Library MUST support reading/extracting password-protected archives for 7z, ZIP, and RAR formats. Creation-time encryption is currently ZIP-only (AES-256 via ZipWriter); 7z creation encryption is future work.
- **FR-014**: Library MUST handle password authentication failures consistently across all formats with clear error messages

#### Metadata & Compatibility Requirements

- **FR-015**: Library MUST preserve file metadata (timestamps, permissions, attributes) during extraction and creation where supported by the archive format. OI-022-002 resolved: `commit_changes()` preserves timestamps and Unix permissions via metadata-aware add helpers.
- **FR-016**: Library targets matching the core API surface of 7zip-JBinding (open, list, extract, create, modify, close) to facilitate porting existing Java archive code to Rust; see SC-004 for concrete equivalences
- **FR-017**: Library MUST provide progress callbacks for extraction operations with a uniform callback shape (`ProgressCallback`) across all formats; actual invocation frequency and granularity are backend-dependent. **Note**: Creation progress callbacks are now consumed per-entry by both ZIP and libarchive backends with `total=None` (AD 0021 / OI-025-003 resolved).
- **FR-022**: Library MUST skip symbolic links and hard links during archive operations with a warning, as cross-platform symlink handling is not reliably supported across all archive formats and operating systems
- **FR-023**: Library MUST fail extraction with a clear error when any file would overwrite an existing file at the destination, unless explicitly configured otherwise via ExtractionOptions.overwrite flag
- **FR-024**: Library MUST return `ArchiveError::CodecUnavailable { codec, format, install_instructions }` when an archive requires a compression codec not available on the system

#### Compression & Performance Requirements

- **FR-018**: Library MUST support multiple compression levels (store, fastest, fast, normal, maximum, ultra) with unified compression level API across formats
- **FR-019**: Multi-part archive extraction is RAR-only (via UnRAR backend). Multi-part reading for other formats and multi-part writing are deferred to v0.2.0.

#### Concurrency Requirements

- **FR-020**: Library MUST provide thread-safe APIs for concurrent archive operations on different files. **RAR note (OI-026-004 resolved)**: UnRAR FFI calls are serialized via a process-wide mutex (`UNRAR_LOCK`); concurrent caller access is safe but RAR operations execute sequentially under the hood.
- **FR-021**: Library MUST allow multiple archive handles to be used concurrently from different threads without blocking (subject to RAR limitation in FR-020)

#### Self-Extracting Archive (SFX) Detection Requirements

- **FR-025**: Library MUST detect whether a file is a self-extracting archive (SFX) by performing a bounded heuristic scan for embedded archive signatures (ZIP: "PK\x03\x04", RAR: "Rar!\x1a\x07\x00", 7z: "7z\xbc\xaf\x27\x1c") within the first 1MB of the file, returning confidence-based results (per AD 0015). **Note**: Unknown/custom stubs now proceed to signature scanning (OI-027-001 resolved); all stub types (PE, ELF, Mach-O, ScriptInterpreter, Unknown) reach Stage 2.
- **FR-026**: Library MUST return SfxDetectionResult containing: is_sfx (boolean), archive_format (Option<ArchiveFormat>), data_offset (Option<u64> indicating byte position where archive data begins), stub_type (WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, or Unknown), and confidence (f32). **Note**: Confirmed confidence (1.0 via backend validation) is reserved for future work; current implementation uses `probable()` with caller-supplied confidence for signature-only matches.
- **FR-027**: Library MUST detect SFX archives across multiple platforms: Windows PE executables (.exe), Linux/Unix ELF executables, macOS Mach-O executables, and ScriptInterpreter stubs (shebang-based scripts with embedded ZIP, RAR, or 7z archive). **Note**: TAR signatures were removed from SFX detection per AD 0015.
- **FR-028**: Library MUST identify the specific archive format embedded in the SFX (ZIP, RAR, RAR5, 7z) and return it separately from the SFX stub type. **Note**: TAR-derived formats are not in the SFX signature set per AD 0015.
- **FR-029**: SFX detection reports data_offset for the embedded archive. **Long-term requirement**: Enable direct offset-based opening (`open_at_offset`) for extraction from SFX offsets. **Current implementation note**: `open_at_offset` is deferred and returns Unsupported.
- **FR-030**: Library MUST handle false positives gracefully - if archive signature is found but subsequent validation fails, the SFX pipeline iterates to the next candidate (per AD 0015); if no candidates validate, `SfxDetectionResult::not_sfx()` is returned (not an error)
- **FR-031**: Library MUST provide option to extract SFX stub separately from embedded archive for security analysis and malware scanning scenarios

## Technology Stack

### Language & Edition

This project uses **Rust 2024 edition** (minimum version 1.85). For detailed edition-specific requirements and rationale, see constitution.md "Technology Stack" section (lines 108-117).

### Core Dependencies

**Core Rust Crates**:
- `once_cell` 1.20+ - OnceCell for zero-cost entry caching (provides stable `get_or_try_init`, used until `std::sync::OnceLock::get_or_try_init` stabilizes)
- `crc32fast` 1.4+ - SIMD-accelerated CRC32 verification for archive integrity validation
- `rayon` 1.8+ - Parallel extraction with work-stealing scheduler for performance
- ~~`secstr`~~ - Deferred: passwords currently use `Option<String>`. Secure zeroing is a future consideration.

**Backend Libraries**:
- Piz (pure Rust) - ZIP format reading (memory-mapped)
- zip crate / ZipReader (pure Rust) - ZIP format reading (encrypted ZIP support)
- sevenz-rust2 (pure Rust) - 7z format reading
- UnRAR SDK (FFI) - RAR/RAR5 format support with CRC32 and complete metadata extraction
- libarchive (FFI) - TAR family, ISO format support; also used for archive creation
- ZipWriter (zip crate, pure Rust) - ZIP creation

**Dependency Justification**: Each dependency is chosen per constitutional requirements (Principle III):
- UnRAR SDK: RAR format is proprietary; using official SDK ensures compatibility and correctness
- libarchive: Battle-tested multi-format support; reimplementing would introduce significant risk
- once_cell: Zero-cost abstraction for lazy initialization with fallible operations
- crc32fast: Hardware-accelerated CRC32 provides 10x+ speedup over naive implementations
- rayon: Data parallelism for extraction scales to available CPU cores with minimal code changes
- Password handling: Currently uses `Option<String>`. Secure zeroing via secstr is deferred.

### Key Entities

- **Archive**: Archive handle (struct) representing a compressed file container, with attributes including format type, compression method, encryption status, and file list
- **ArchiveEntry**: Represents a single file or directory within an archive, with attributes including path, size, compressed_size (Option), modified (Option), crc32 (Option), is_directory, and is_encrypted. Field availability is format-dependent.
- **CompressionOptions**: Configuration for archive creation, including compression level, format, encryption password, split size, and format-specific parameters
- **ExtractionOptions**: Configuration for archive extraction, including destination path, password, overwrite behavior (default: error on conflict), file filtering rules, progress callback, verify_crc32 flag, and extraction limits (max_file_size, max_total_size, max_file_count)
- **ArchiveFormat**: Enumeration of supported archive formats with their capabilities (compression methods, encryption support, metadata preservation)
- **SfxDetectionResult**: Result of SFX detection scan, with attributes including is_sfx (boolean), archive_format (detected format of embedded archive), data_offset (byte position where archive begins), stub_type (executable format: WindowsPE/LinuxELF/MacOSMachO/ScriptInterpreter/Unknown), and confidence (f32). `detected()` returns 1.0 confidence when backend confirms archive at offset; `probable()` accepts a caller-supplied confidence value for signature-only matches.

## Reference Projects & Prior Art

This project builds upon proven approaches from existing archive libraries:

### Primary Reference: 7zip-JBinding (Java)

**Location**: Available for analysis
**Key Learning**: Successfully implemented unified interface for multiple archive formats (7z, ZIP, RAR, TAR, GZIP, BZIP2, ISO) through format-agnostic API design. Developers use identical method calls regardless of archive format, with automatic format detection and transparent handling of format-specific features.

**API Patterns to Replicate**:
- Single `Archive.open()` method that detects format automatically
- Unified `ArchiveEntry` interface for file metadata across all formats
- Consistent extraction API: `extractFile()`, `extract()` work identically for all formats
- Format-transparent progress callbacks and error handling
- Common compression level abstraction that maps to format-specific settings

### Secondary References (Rust Ecosystem)

**archive-reader** (`/Volumes/Common/QJoon/unified-archive/archive-reader`)
- Rust implementation approach for archive handling
- Error handling patterns and Rust idiomatic API design
- Performance considerations for streaming large archives

**compress-tools** (`/Volumes/Common/QJoon/unified-archive/compress-tools-rs`)
- Rust FFI patterns for binding to native compression libraries
- Cross-platform compilation and linking strategies
- Memory safety patterns when interfacing with C/C++ libraries

### Implementation Strategy

The library studied 7zip-JBinding's unified interface design and adapted it to Rust's type system and ownership model, while leveraging implementation patterns from existing Rust archive libraries for FFI safety and performance optimization.

## Success Criteria *(mandatory)*

### Measurable Outcomes

#### Unified Interface Success Criteria

- **SC-001**: Same code that inspects a ZIP archive works without modification for 7z, RAR, RAR5, TAR.GZ, and other supported formats (verified by test suite with identical code processing 5+ different formats)
- **SC-002**: Format detection is automatic - developers never need to specify archive format explicitly (100% of test cases work with format auto-detection)
- **SC-003**: Archive metadata uses shared types (`ArchiveEntry`) across all formats with the same struct fields and method signatures; actual field population is format-dependent (e.g., compressed_size and CRC32 may be `None` for some backends). Verified by single generic handler processing all formats.
- **SC-004**: 95% of 7zip-JBinding API patterns have direct Rust equivalents, maintaining the unified interface philosophy that made 7zip-JBinding successful
  - Key API equivalences:
    - `SevenZip.openInArchive()` → `Archive::open()`
    - `IInArchive.getNumberOfItems()` → `Archive::entry_count()`
    - `IInArchive.getProperty(index, PropID)` → `Archive::list_files()` returning `&[ArchiveEntry]`
    - `IInArchive.extract()` → `Archive::extract_all()` / `Archive::extract_file()`
    - `IOutArchive.createArchive()` → `Archive::create()`
    - `IOutArchive.updateItems()` → `Archive::modify()` + `Archive::commit_changes()`
    - `IInArchive.close()` → `Archive::close()` / automatic via Drop

#### Simplicity & Usability Criteria

- **SC-005**: Developers can inspect archive contents (list files with metadata) for any supported format with 3-5 lines of code using format-agnostic API
- **SC-006**: Developers can extract archives in any supported format with 5-10 lines of code using the same extraction method
- **SC-007**: Switching between archive formats in existing code requires changing only the input file path, not the API calls (verified by parameterized tests)

#### Performance & Scalability Criteria

- **SC-008**: [TARGET] Archive inspection returns metadata (path, size, and format-available fields) for all entries in under 1 second for archives with up to 10,000 files. Actual field availability (compressed size, modification date, CRC32) and performance vary by backend. Dedicated inspection benchmark is pending.
- **SC-009**: Library handles archives up to 10GB in size while consuming less than 100MB of memory for libarchive-backed formats (TAR family) through streaming. Native backends (Piz, ZipReader, SevenZ, UnRAR) currently buffer the full entry before returning a streaming wrapper.
- **SC-010**: [TARGET] Extraction performance is within 20% of native 7zip command-line tool for identical operations across all supported formats (measured by extraction throughput in MB/s). **Current status**: No benchmark artifacts exist; this is a performance target, not a validated criterion.

#### Platform & Quality Criteria

- **SC-011**: [TARGET] Library compiles and passes all tests on Windows, macOS, and Linux without platform-specific code changes. **Current status**: Primary development target is macOS; Linux has known `wchar_t` edge cases. Cross-platform verification is a target, not a current guarantee.
- **SC-012**: [TARGET] Archive integrity validation using CRC32 detects corruption in test archives where the format and backend support CRC32 verification. Detection scope is limited to formats that store CRC32 and backends that expose it; 100% detection across all formats is a target, not a guarantee.
- **SC-013**: Progress callbacks are rate-limited to ~60 updates/sec maximum; actual frequency varies by backend. ZIP and 7z provide per-entry granularity. RAR has full progress support.
- **SC-014**: Library documentation includes working examples showing identical code processing multiple archive formats for all four primary user stories (inspect, extract, create, modify)
- **SC-015**: The crate depends on bundled native libraries: libarchive (TAR family, ISO), UnRAR (RAR/RAR5), and optionally the external WinRAR CLI (via `external-rar-create` feature). ZIP uses the pure-Rust Piz and zip crates. 7z uses the pure-Rust sevenz-rust2 crate.

#### SFX Detection Criteria

- **SC-016**: [PARTIAL] SFX detection covers synthetic ScriptInterpreter, PE, ELF, and Mach-O stub types with signature scanning for ZIP, RAR, and 7z (per AD 0015; TAR/gzip/bzip2/xz signatures removed from SFX detection). This is a synthetic-test milestone; official-tool sample testing is planned but not yet implemented.
- **SC-017**: SFX detection completes within 100ms (p95 latency) for files up to 10MB by scanning only necessary portions (first 1MB bounded scan for archive signatures)
- **SC-018**: False-positive testing covers plain archives, text files, random data, shell scripts, and a small set of synthetic executables. **Current corpus is synthetic only**; a large negative corpus (100+ real-world executables from diverse platforms) is planned as the target end-state.
- **SC-019**: Detected SFX files accurately report archive format, offset, and confidence in all test cases.
- **SC-020**: Offset accuracy is validated through numeric offset assertions and stub extraction length/content checks on synthetic test files.

## Assumptions

### Unified Interface Assumptions

1. **7zip-JBinding unified interface model**: Assume 7zip-JBinding's unified interface approach (single API for all formats) is the design pattern to adapt. Specific retained ideas: format auto-detection via `Archive::open()`, shared `ArchiveEntry` structure, and common extraction/creation API shape. **Note**: This project's scope is narrower than 7zip-JBinding — standalone GZIP, BZIP2, and XZ are not supported (AD 0018); only TAR compound formats are in scope. Backend behavior differences are documented rather than hidden.
2. **Format auto-detection**: Assume archive format can be reliably detected from file content (magic bytes/signatures) without requiring developers to specify format explicitly, matching 7zip-JBinding behavior
3. **Reference projects availability**: Assume 7zip-JBinding source, archive-reader, and compress-tools codebases at `/Volumes/Common/QJoon/unified-archive/` provide sufficient reference for API design patterns, FFI implementation strategies, and cross-platform compilation approaches

### Archive Format & Compatibility Assumptions

4. **Archive format standards**: Assume compliance with standard format specifications (ZIP APPNOTE, 7z format docs, RAR specifications) is sufficient for interoperability with all major archive tools
5. **RAR5 support requirement**: Assume RAR5 format support is essential in addition to legacy RAR format, as RAR5 is increasingly common and has improved compression/encryption
6. **Compression method coverage**: Assume supporting the most common compression methods (Deflate, LZMA, LZMA2, BZip2, XZ) covers 95%+ of use cases across all supported formats

### Performance & Platform Assumptions

7. **Performance expectations**: Assume "comparable to native 7zip" means within 20% performance margin for most operations across all formats
8. **Platform support**: Assume current stable Rust compiler versions on Windows 10+, macOS 11+, and Linux kernel 4.4+ distributions
9. **Memory constraints**: Assume target environments have at least 512MB RAM available for applications using this library
10. **Backend strategy**: Mixed backend strategy: libarchive (TAR family, ISO, creation), pure-Rust crates (ZIP via Piz, 7z via sevenz-rust2), and UnRAR FFI (RAR).

### Development & API Assumptions

11. **Error handling**: Assume Rust idiomatic error handling (Result types) is preferred over exception-based patterns, with consistent error types across all formats
12. **Thread safety**: Assume thread safety means multiple archives can be processed concurrently, but individual archive handles need not be thread-safe (matching typical file handle semantics)
13. **Encoding**: Assume UTF-8 file paths are the standard, with fallback to platform-specific encodings where necessary to handle legacy archives
14. **Testing**: Testing uses a mix of synthetic and tool-created archives. **Current status**: The majority of test archives are synthetic (hand-crafted or programmatically generated). Official-tool sample testing (7zip, WinRAR, system TAR) and broader SFX corpus validation are planned targets for compatibility validation. Results should be interpreted with this synthetic-heavy baseline in mind.

### SFX Detection Assumptions

15. **SFX stub size limits**: Assume SFX executable stubs are typically under 1MB in size (7-Zip SFX: ~150KB, WinRAR SFX: ~50-200KB, custom stubs: <1MB), allowing efficient detection by scanning the first 1MB as a pragmatic coverage threshold for typical stub sizes
16. **Archive signature reliability**: Assume standard archive format signatures (ZIP: "PK\x03\x04", RAR: "Rar!\x1a\x07\x00", RAR5: "Rar!\x1a\x07\x01\x00", 7z: "7z\xbc\xaf\x27\x1c") are sufficiently unique to identify embedded archives with low false-positive rates when combined with executable format validation. **Current status**: False-positive testing covers a small synthetic corpus; a larger negative corpus is planned (SC-018).
17. **Platform executable formats**: Assume Windows uses PE format (.exe), Linux/BSD use ELF format, macOS uses Mach-O format, and Unix shell scripts can be detected by shebang (#!) detection only
18. **SFX creation tools**: Assume most SFX archives are created by mainstream tools (7-Zip, WinRAR, Unix makeself/shar) which follow predictable patterns of prepending executable stub to archive data
19. **Security scanning use case**: Assume SFX detection is primarily used for security analysis, malware scanning, and automated archive processing, requiring both high accuracy and ability to extract stub separately from archive
20. **Cross-platform limitations**: Assume SFX archives are platform-specific (Windows SFX won't run on Linux), but detection and extraction should work cross-platform (Linux tool can detect and extract Windows SFX archive data)
