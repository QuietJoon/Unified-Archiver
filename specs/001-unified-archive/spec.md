# Feature Specification: unified-archive

**Feature Branch**: `001-unified-archive`
**Created**: 2025-10-30
**Updated**: 2025-11-11 (Added SFX detection feature)
**Status**: Draft
**Input**: User description: "unified-archive project. This is a unified interface library inspired by 7zip-JBinding's design philosophy. Provides format-agnostic API for archive operations (inspection, extraction, creation, modification, SFX detection) across multiple formats with cross-platform compatibility (Windows, macOS, Linux) and zero JVM dependency."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Archive Inspection (Priority: P1, MVP)

A Rust developer needs to inspect archive contents (list files, check metadata, validate integrity) **using a unified interface that works identically across all supported archive formats** (7z, RAR, RAR5, ZIP, TAR, GZIP, etc.) without extracting the entire archive. This includes retrieving file modification dates, file sizes, compressed sizes, and CRC32 checksums for each file, regardless of the archive format.

**Why this priority**: Inspection is the foundational operation that enables all other archive workflows. Developers must first understand what's inside an archive before deciding whether to extract, which files to extract, or validating archive integrity. This is the lightweight, essential first step that makes the library immediately useful for archive analysis, validation, and management tasks. **The unified interface eliminates format-specific code, allowing developers to write once and support all formats.**

**Independent Test**: Can be fully tested by listing archive contents across different formats (7z, RAR, RAR5, ZIP, TAR.GZ), retrieving file metadata (modification date, size, CRC32) using identical API calls, and comparing against known archive structures. Delivers standalone value as a format-agnostic archive browser/validator without requiring extraction or creation capabilities.

**Acceptance Scenarios**:

1. **Given** archives in different formats (ZIP, 7z, RAR, RAR5, TAR.GZ), **When** developer uses the same inspection API for all formats, **Then** all files with their paths, uncompressed sizes, compressed sizes, modification dates, and CRC32 checksums are returned in a consistent format
2. **Given** a password-protected archive (any format), **When** developer inspects the file list without password, **Then** filenames and metadata (size, date, CRC32) are accessible but content remains protected, with consistent behavior across formats
3. **Given** an archive (any format), **When** developer checks integrity using CRC32 validation, **Then** any corruption is detected and reported with specific file details using the same error handling pattern
4. **Given** a multi-part archive (any format that supports splitting), **When** inspecting contents, **Then** all parts are recognized and the complete file list with accurate metadata is returned through the unified interface
5. **Given** archives with thousands of files (various formats), **When** listing contents, **Then** results can be filtered by path patterns or file attributes while preserving metadata accuracy, using format-agnostic filter APIs

---

### User Story 2 - Archive Extraction (Priority: P2, MVP)

A Rust developer needs to extract files from compressed archives **using a unified interface that works identically for all supported formats** (ZIP, 7z, RAR, RAR5, TAR, GZIP, BZIP2, XZ, etc.) without depending on external system tools or JVM runtime. The same extraction API should handle format detection, decompression, and file extraction seamlessly across all formats.

**Why this priority**: After inspecting archive contents, extraction is the second essential operation. Together with inspection (P1), these two features form the complete "read" capability for archives, enabling developers to both understand and access archive contents. This MVP combination delivers immediate practical value for archive consumption workflows. **The unified interface means developers write one extraction code path that automatically handles all formats, just like 7zip-JBinding achieved.**

**Independent Test**: Can be fully tested by creating archives of various formats (7z, RAR, RAR5, ZIP, TAR.GZ, BZIP2, XZ), extracting them programmatically using identical API calls, and verifying extracted file content matches the original files across all formats. This delivers standalone utility for format-agnostic archive extraction use cases.

**Acceptance Scenarios**:

1. **Given** archives in different formats (ZIP, 7z, RAR, RAR5, TAR.GZ), **When** developer uses the same extract API with archive path and destination directory, **Then** all files are extracted to the destination with correct filenames, directory structure, and content, regardless of format
2. **Given** password-protected archives in different formats (7z, ZIP, RAR), **When** developer provides the correct password using the same API pattern, **Then** files are successfully decrypted and extracted consistently across formats
3. **Given** multi-layer compressed archives (TAR.GZ, TAR.BZ2, TAR.XZ), **When** developer extracts using the unified API, **Then** the multi-layer compression is handled transparently with format auto-detection
4. **Given** corrupted or invalid archives of various formats, **When** developer attempts extraction, **Then** clear format-consistent errors are returned indicating the archive is invalid or corrupted
5. **Given** archives containing 1000+ files in various formats, **When** extraction is initiated, **Then** progress can be monitored throughout the extraction process using the same callback interface across all formats

---

### User Story 3 - Archive Creation (Priority: P3)

A Rust developer needs to create compressed archives from files and directories **using a unified interface that supports multiple formats** (ZIP, 7z, TAR.GZ, TAR.BZ2, TAR.XZ, etc.) with consistent API calls regardless of the chosen output format. Developers should specify the desired format once and use the same creation methods for all formats.

**Why this priority**: Creation enables "write" capability, complementing the "read" operations (inspection and extraction). While valuable for complete archive management workflows and backup solutions, many applications only need to read existing archives, making creation a high-value but non-MVP feature. **The unified interface ensures format selection doesn't require different code paths, matching 7zip-JBinding's design.**

**Independent Test**: Can be tested by creating archives in various formats (7z, ZIP, TAR.GZ) using identical API calls with only format parameter differing, then verifying the archives can be extracted by standard tools (7zip, WinRAR) and match the original content. Delivers value as a format-agnostic compression utility.

**Acceptance Scenarios**:

1. **Given** a directory containing files, **When** developer creates archives in different formats (ZIP, 7z, TAR.GZ) using the same creation API, **Then** all files and subdirectories are included with relative paths preserved, with format-appropriate compression
2. **Given** multiple files from different locations, **When** developer adds them to archives in various formats with maximum compression setting, **Then** archives are created with optimal compression ratios appropriate to each format
3. **Given** sensitive files, **When** developer creates password-protected archives in different formats (7z, ZIP) using the same encryption API, **Then** resulting archives cannot be extracted without the correct password, with format-appropriate encryption standards
4. **Given** a large dataset (10GB+), **When** creating archives in various formats, **Then** memory usage remains bounded and progress can be monitored using the same callback interface
5. **Given** files already compressed (JPEG, MP4), **When** adding to archives in any format, **Then** compression level can be adjusted uniformly across formats to avoid unnecessary processing overhead

---

### User Story 4 - Archive Modification (Priority: P4)

A Rust developer needs to update existing archives **using a unified interface** by adding, removing, or replacing files without full re-compression, with consistent API behavior across all modifiable formats (ZIP, 7z, TAR).

**Why this priority**: Modification is an advanced feature useful for incremental backups and archive maintenance. Most applications only need extract/create, making this a nice-to-have enhancement. **The unified interface ensures modification operations use consistent patterns regardless of archive format, where supported.**

**Independent Test**: Can be tested by adding files to, removing entries from, and replacing files in existing archives of various formats using identical API calls, verifying changes are persisted correctly across formats. Delivers value as a format-agnostic archive maintenance utility.

**Acceptance Scenarios**:

1. **Given** existing archives in different modifiable formats (ZIP, 7z), **When** developer adds new files using the same API, **Then** files are appended without re-compressing existing entries, with format-appropriate behavior
2. **Given** archives with outdated files in various formats, **When** developer removes specific entries using the unified API, **Then** those files are deleted and archive size is reduced accordingly
3. **Given** archives in different formats, **When** developer replaces a file with an updated version using the same API pattern, **Then** only that entry is re-compressed and the rest remains unchanged
4. **Given** large archives in various formats, **When** performing modifications, **Then** temporary disk space usage is minimized using format-appropriate strategies

---

### User Story 5 - Self-Extracting Archive (SFX) Detection (Priority: P5)

A Rust developer needs to identify whether a file is a self-extracting archive (SFX) **using a unified interface** that detects SFX markers across different platforms (Windows .exe, Linux/macOS ELF executables, and shell scripts) and archive formats (ZIP SFX, RAR SFX, 7z SFX).

**Why this priority**: SFX detection is an advanced inspection capability that enables security scanning, automated archive processing, and intelligent file classification. Many users distribute archives as SFX executables for ease of use, and applications need to identify and handle these correctly. While not essential for basic archive operations, SFX detection adds significant value for security-conscious applications, malware analysis tools, and enterprise file management systems.

**Independent Test**: Can be tested by creating SFX archives using various tools (7-Zip SFX, WinRAR SFX, Unix shell-based SFX) on different platforms, then detecting them programmatically and extracting the embedded archive data. The detection API should return both the SFX status and the embedded archive format. This delivers standalone value for security scanning and intelligent file classification without requiring other archive operations.

**Acceptance Scenarios**:

1. **Given** a Windows executable created as ZIP SFX, **When** developer checks if file is SFX, **Then** library returns true with detected archive format (ZIP) and the offset where the archive data begins
2. **Given** a WinRAR SFX executable (.exe), **When** developer inspects the file using the unified API, **Then** library identifies it as SFX with format RAR or RAR5 and provides the embedded archive offset
3. **Given** a 7-Zip SFX executable on Windows, **When** developer checks SFX status, **Then** library returns true with format 7z and the archive data location
4. **Given** a Linux/macOS ELF executable containing embedded archive, **When** developer uses SFX detection, **Then** library identifies it as SFX with the correct archive format (7z, TAR.GZ, etc.)
5. **Given** a Unix shell script-based SFX (tar.gz embedded in shell script), **When** developer checks SFX status, **Then** library detects the embedded archive and returns the archive format and data offset
6. **Given** a regular archive file (non-SFX ZIP, RAR, 7z), **When** developer checks if file is SFX, **Then** library returns false indicating it's a standard archive
7. **Given** a non-archive executable file (regular .exe or ELF binary), **When** developer checks SFX status, **Then** library returns false with appropriate error indicating no embedded archive found
8. **Given** an SFX file with unknown or custom stub, **When** developer attempts detection, **Then** library uses heuristic scanning to locate embedded archive by searching for format signatures (PK, Rar!, 7z markers)

---

### Edge Cases

- **[RESOLVED]** Symbolic links and hard links are skipped with warnings during extraction and creation (FR-022)
- **[RESOLVED]** When archive requires unavailable codec, library returns ArchiveError::Unsupported with codec name and installation instructions (FR-024)
- **[RESOLVED]** Extraction fails with clear error when files would overwrite existing files, unless ExtractionOptions.overwrite is true (FR-023)
- **[RESOLVED]** SFX detection scans first 1MB of file byte-by-byte for 100% accuracy; archives with stubs >1MB return `SfxDetectionResult::not_sfx()` as a performance trade-off (signatures beyond scan limit cannot be reliably detected without full-file scan) (FR-025)
- **[RESOLVED]** For SFX files with multiple embedded archives (rare), library returns the first detected archive format and offset (FR-026)
- **[RESOLVED]** Archives with filenames containing special characters or Unicode are handled using UTF-8 encoding with platform-specific fallback (Assumption 13)
- **[RESOLVED]** Archives larger than available RAM are handled through streaming APIs ensuring <100MB memory usage (FR-012)
- **[RESOLVED]** Extraction with insufficient disk space returns ArchiveError::Io with clear error message from underlying filesystem
- **[RESOLVED]** Concurrent access to same archive file behavior follows typical file handle semantics - multiple readers allowed, writers require exclusive access (FR-020, FR-021 clarified in T120a-c)
- **[RESOLVED]** File permissions preserved where supported by archive format and OS; cross-platform handling uses best-effort mapping (FR-015)
- **[RESOLVED]** Missing or wrong passwords return consistent ArchiveError::Password across all formats (FR-014, implemented in T047b)
- **[RESOLVED]** Archives with non-standard extensions detected via magic bytes; missing magic bytes return ArchiveError::Format
- **[RESOLVED]** Corrupted SFX executables or damaged embedded archives detected during validation and return ArchiveError::Corruption (FR-030)
- **[RESOLVED]** Platform-specific executable formats (PE/ELF/Mach-O) detected via goblin crate, shell scripts via shebang (FR-027, implemented in T093-T097)

## Requirements *(mandatory)*

### Functional Requirements

#### Unified Interface Requirements (Critical)

- **FR-001**: Library MUST provide a unified, format-agnostic API for all archive operations (inspection, extraction, creation, modification) that works identically across all supported formats (7z, RAR, RAR5, ZIP, TAR, GZIP, BZIP2, XZ, ISO)
- **FR-002**: Library MUST automatically detect archive format from file content (magic bytes) without requiring explicit format specification from developers
- **FR-003**: Library MUST return archive metadata (file lists, sizes, dates, CRC32) in a consistent data structure regardless of the underlying archive format
- **FR-004**: Library MUST handle format-specific features (encryption methods, compression algorithms, multi-part archives) transparently through the unified interface without exposing format-specific APIs to developers

#### Format Support Requirements

- **FR-005**: Library MUST support extraction of archives in ZIP, 7z, RAR, RAR5, TAR, GZIP, BZIP2, XZ, and ISO formats
- **FR-006**: Library MUST support creation of archives in ZIP, 7z, and TAR-based formats (TAR.GZ, TAR.BZ2, TAR.XZ) at minimum
- **FR-007**: Library MUST support RAR5 format in addition to legacy RAR format for complete compatibility

#### Platform & Runtime Requirements

- **FR-008**: Library MUST work on Windows, macOS, and Linux operating systems without platform-specific code in the public API
- **FR-009**: Library MUST operate independently of JVM runtime (pure Rust implementation or FFI bindings to native 7zip libraries)

#### Error Handling & Robustness Requirements

- **FR-010**: Library MUST provide error handling that distinguishes between file I/O errors, archive format errors, and corruption, with consistent error types across all formats
- **FR-011**: Library MUST validate archive integrity through CRC checks where supported by the format
- **FR-012**: Library MUST handle archives larger than available memory through streaming APIs using 64KB read buffers with <50MB total memory overhead, enabling processing of 10GB+ archives in <100MB memory (measured by extracting 10GB archive with progress callbacks). Per-module memory budgets are defined in plan.md lines 46-52: extraction (35MB), inspection (10MB), SFX detection (5MB), creation (30MB), modification (40MB), remaining modules (10MB combined), totaling <130MB library overhead.

#### Security & Encryption Requirements

- **FR-013**: Library MUST support password-protected archives with encryption standards used by 7zip, ZIP, and RAR formats
- **FR-014**: Library MUST handle password authentication failures consistently across all formats with clear error messages

#### Metadata & Compatibility Requirements

- **FR-015**: Library MUST preserve file metadata (timestamps, permissions, attributes) during extraction and creation where supported by the archive format
- **FR-016**: Library MUST match the core API surface of 7zip-JBinding to facilitate porting existing Java code to Rust
- **FR-017**: Library MUST provide progress callbacks for long-running operations (extraction, creation, compression) with consistent callback signatures across all formats
- **FR-022**: Library MUST skip symbolic links and hard links during archive operations with a warning, as cross-platform symlink handling is not reliably supported across all archive formats and operating systems
- **FR-023**: Library MUST fail extraction with a clear error when any file would overwrite an existing file at the destination, unless explicitly configured otherwise via ExtractionOptions.overwrite flag
- **FR-024**: Library MUST return ArchiveError::Unsupported with specific codec name and installation instructions when an archive requires a compression codec not available on the system

#### Compression & Performance Requirements

- **FR-018**: Library MUST support multiple compression levels (store, fastest, fast, normal, maximum, ultra) with unified compression level API across formats
- **FR-019**: Library MUST handle multi-part archives (split archives) for both reading and writing across supported formats
  - **v0.1.0 Scope**: Reading multi-part archives (.z01, .001, .part files) is fully supported; multi-part creation is deferred to v0.2.0

#### Concurrency Requirements

- **FR-020**: Library MUST provide thread-safe APIs for concurrent archive operations on different files
- **FR-021**: Library MUST allow multiple archive handles to be used concurrently from different threads without blocking

#### Self-Extracting Archive (SFX) Detection Requirements

- **FR-025**: Library MUST detect whether a file is a self-extracting archive (SFX) by scanning for embedded archive signatures (ZIP: "PK\x03\x04", RAR: "Rar!\x1a\x07\x00", 7z: "7z\xbc\xaf\x27\x1c") byte-by-byte within the first 1MB of the file for 100% detection accuracy
- **FR-026**: Library MUST return SFX detection result containing: is_sfx (boolean), archive_format (Option<ArchiveFormat>), data_offset (Option<u64> indicating byte position where archive data begins), and stub_type (PE executable, ELF executable, Mach-O executable, or shell script)
- **FR-027**: Library MUST detect SFX archives across multiple platforms: Windows PE executables (.exe), Linux/Unix ELF executables, macOS Mach-O executables, and Unix shell script-based SFX (shell script with embedded tar.gz or similar)
- **FR-028**: Library MUST identify the specific archive format embedded in the SFX (ZIP, RAR, RAR5, 7z, TAR.GZ, etc.) and return it separately from the SFX stub type
- **FR-029**: Library MUST support extracting archives from detected SFX files by using the returned data_offset to skip the executable stub and read the embedded archive data directly
- **FR-030**: Library MUST handle false positives gracefully - if archive signature is found but subsequent archive parsing fails, return clear error indicating detection succeeded but archive is corrupted or invalid
- **FR-031**: Library MUST provide option to extract SFX stub separately from embedded archive for security analysis and malware scanning scenarios

## Technology Stack

### Language & Edition

This project uses **Rust 2024 edition** (minimum version 1.85). For detailed edition-specific requirements and rationale, see constitution.md "Technology Stack" section (lines 108-117).

### Core Dependencies

**Core Rust Crates**:
- `once_cell` 1.20+ - OnceCell for zero-cost entry caching (provides stable `get_or_try_init`, used until `std::sync::OnceLock::get_or_try_init` stabilizes)
- `crc32fast` 1.4+ - SIMD-accelerated CRC32 verification for archive integrity validation
- `rayon` 1.8+ - Parallel extraction with work-stealing scheduler for performance
- `secstr` 0.5+ - Secure password storage with automatic zeroing to protect encryption keys

**Backend Libraries** (linked via FFI):
- UnRAR SDK - RAR/RAR5 format support with CRC32 and complete metadata extraction
- libarchive - ZIP, 7z, TAR, and other format support through unified C API

**Dependency Justification**: Each dependency is chosen per constitutional requirements (Principle III):
- UnRAR SDK: RAR format is proprietary; using official SDK ensures compatibility and correctness
- libarchive: Battle-tested multi-format support; reimplementing would introduce significant risk
- once_cell: Zero-cost abstraction for lazy initialization with fallible operations
- crc32fast: Hardware-accelerated CRC32 provides 10x+ speedup over naive implementations
- rayon: Data parallelism for extraction scales to available CPU cores with minimal code changes
- secstr: Security-critical password handling requires guaranteed memory zeroing on drop

### Key Entities

- **Archive**: Archive handle (struct) representing a compressed file container, with attributes including format type, compression method, encryption status, and file list
- **ArchiveEntry**: Represents a single file or directory within an archive, with attributes including path, size, compressed size, timestamp, CRC checksum, and compression method
- **CompressionOptions**: Configuration for archive creation, including compression level, format, encryption password, split size, and format-specific parameters
- **ExtractionOptions**: Configuration for archive extraction, including destination path, password, overwrite behavior (default: error on conflict), and file filtering rules
- **ArchiveFormat**: Enumeration of supported archive formats with their capabilities (compression methods, encryption support, metadata preservation)
- **SfxDetectionResult**: Result of SFX detection scan, with attributes including is_sfx (boolean), archive_format (detected format of embedded archive), data_offset (byte position where archive begins), and stub_type (executable format: PE/ELF/Mach-O/Script). Detection is deterministic: 100% confidence when archive validates at offset, 0% otherwise.

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

The library will study 7zip-JBinding's unified interface design and adapt it to Rust's type system and ownership model, while leveraging implementation patterns from existing Rust archive libraries for FFI safety and performance optimization.

## Success Criteria *(mandatory)*

### Measurable Outcomes

#### Unified Interface Success Criteria

- **SC-001**: Same code that inspects a ZIP archive works without modification for 7z, RAR, RAR5, TAR.GZ, and other supported formats (verified by test suite with identical code processing 5+ different formats)
- **SC-002**: Format detection is automatic - developers never need to specify archive format explicitly (100% of test cases work with format auto-detection)
- **SC-003**: Archive metadata structure is identical across all formats - same struct fields, same method signatures, same data types (verified by single generic handler processing all formats)
- **SC-004**: 95% of 7zip-JBinding API patterns have direct Rust equivalents, maintaining the unified interface philosophy that made 7zip-JBinding successful
  - Key API equivalences:
    - `SevenZip.openInArchive()` → `Archive::open()`
    - `IInArchive.getNumberOfItems()` → `Archive::entry_count()`
    - `IInArchive.getProperty(index, PropID)` → `Archive::list_files()` returning `Vec<ArchiveEntry>`
    - `IInArchive.extract()` → `Archive::extract_all()` / `Archive::extract_file()`
    - `IOutArchive.createArchive()` → `Archive::create()`
    - `IOutArchive.updateItems()` → `Archive::modify()` + `Archive::commit_changes()`
    - `IInArchive.close()` → `Archive::close()` / automatic via Drop

#### Simplicity & Usability Criteria

- **SC-005**: Developers can inspect archive contents (list files with metadata) for any supported format with 3-5 lines of code using format-agnostic API
- **SC-006**: Developers can extract archives in any supported format with 5-10 lines of code using the same extraction method
- **SC-007**: Switching between archive formats in existing code requires changing only the input file path, not the API calls (verified by parameterized tests)

#### Performance & Scalability Criteria

- **SC-008**: Archive inspection returns complete metadata (path, size, compressed size, modification date, CRC32) for all entries in under 1 second for archives with up to 10,000 files across all formats
- **SC-009**: Library handles archives up to 10GB in size while consuming less than 100MB of memory through streaming, regardless of format
- **SC-010**: Extraction performance is within 20% of native 7zip command-line tool for identical operations across all supported formats (measured by extraction throughput in MB/s when extracting the same archive on identical hardware with identical output destination)

#### Platform & Quality Criteria

- **SC-011**: Library successfully compiles and passes all tests on Windows, macOS, and Linux without platform-specific code changes
- **SC-012**: Archive integrity validation using CRC32 detects 100% of intentionally corrupted test archives across all formats
- **SC-013**: Progress callbacks update at minimum 10 times per second during operations on archives with 1000+ files, with consistent callback signature across all formats
- **SC-014**: Library documentation includes working examples showing identical code processing multiple archive formats for all four primary user stories (inspect, extract, create, modify)
- **SC-015**: Zero runtime dependencies on JVM, system compression tools, or non-Rust libraries except necessary platform FFI to native 7zip libraries

#### SFX Detection Criteria

- **SC-016**: SFX detection correctly identifies 100% of test SFX files created by official tools (7-Zip SFX, WinRAR SFX, Unix shell-based SFX) across all supported platforms (Windows, macOS, Linux)
- **SC-017**: SFX detection completes within 100ms (p95 latency) for files up to 10MB by scanning only necessary portions (first 1MB with 512-byte aligned chunks)
- **SC-018**: SFX detection has zero false positives on regular executable files (tested against 100+ non-SFX executables including system binaries and applications)
- **SC-019**: After detecting an SFX file, library can successfully extract the embedded archive in all test cases (10+ SFX samples per format)
- **SC-020**: SFX detection API returns accurate data_offset values - verified by comparing extracted archive data against original archive before SFX conversion (byte-for-byte match in 100% of cases)

## Assumptions

### Unified Interface Assumptions

1. **7zip-JBinding unified interface model**: Assume 7zip-JBinding's unified interface approach (single API for all formats) is the proven design pattern to replicate, as it successfully handled 7z, ZIP, RAR, TAR, GZIP, BZIP2, and ISO formats through format-agnostic methods
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
10. **FFI approach**: Assume FFI bindings to native 7zip libraries (p7zip, 7zip-cpp) provide better format compatibility and performance than pure Rust implementations, based on compress-tools reference

### Development & API Assumptions

11. **Error handling**: Assume Rust idiomatic error handling (Result types) is preferred over exception-based patterns, with consistent error types across all formats
12. **Thread safety**: Assume thread safety means multiple archives can be processed concurrently, but individual archive handles need not be thread-safe (matching typical file handle semantics)
13. **Encoding**: Assume UTF-8 file paths are the standard, with fallback to platform-specific encodings where necessary to handle legacy archives
14. **Testing**: Assume success is validated using archives created by official 7zip, WinRAR, and system tools as compatibility benchmarks across all supported formats

### SFX Detection Assumptions

15. **SFX stub size limits**: Assume SFX executable stubs are typically under 1MB in size (7-Zip SFX: ~150KB, WinRAR SFX: ~50-200KB, custom stubs: <1MB), allowing efficient detection by scanning first 1MB byte-by-byte for 100% accuracy
16. **Archive signature reliability**: Assume standard archive format signatures (ZIP: "PK\x03\x04", RAR: "Rar!\x1a\x07\x00", RAR5: "Rar!\x1a\x07\x01\x00", 7z: "7z\xbc\xaf\x27\x1c") are sufficiently unique to identify embedded archives with minimal false positives when combined with executable format validation
17. **Platform executable formats**: Assume Windows uses PE format (.exe), Linux/BSD use ELF format, macOS uses Mach-O format, and Unix shell scripts can be detected by shebang (#!) or common shell patterns
18. **SFX creation tools**: Assume most SFX archives are created by mainstream tools (7-Zip, WinRAR, Unix makeself/shar) which follow predictable patterns of prepending executable stub to archive data
19. **Security scanning use case**: Assume SFX detection is primarily used for security analysis, malware scanning, and automated archive processing, requiring both high accuracy and ability to extract stub separately from archive
20. **Cross-platform limitations**: Assume SFX archives are platform-specific (Windows SFX won't run on Linux), but detection and extraction should work cross-platform (Linux tool can detect and extract Windows SFX archive data)
