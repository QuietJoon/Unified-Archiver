# Data Model: unified-archive

**Date**: 2025-10-31 (Updated for Phase 1 enhancements; incremental drift fixes ongoing)
**Feature**: 001-unified-archive
**Purpose**: Define core entities, their relationships, and state transitions for the unified archive interface

> **Post-v0.1.0 reality check (2026-04-18):** Data model below has drifted from the shipped 0.1.0 crate. Canonical types live in `src/options.rs`, `src/error.rs`, `src/archive.rs`. Known deltas: (1) `ExtractionOptions.password` and `CompressionOptions.password` are `Option<SecStr>` (not `Option<String>`); (2) `ArchiveError` has no `UnsupportedOperation` variant — use `WriteModeOnly` / `ReadOnlyBackend` / `NotImplemented` / `OperationBlocked`; (3) standalone `.gz` / `.bz2` / `.xz` are openable per MADR-0019; (4) extraction methods return `Result<ResultWithWarnings<()>>` per MADR-0010.

## Overview

The data model provides a unified, format-agnostic representation of archives and their contents. All entities are designed to work across different archive formats (7z, RAR, RAR5, ZIP, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, ISO) through a shared API surface, though individual field population is format-dependent (see per-entity notes). Standalone GZIP, BZIP2, and XZ streams are not directly openable; only their TAR compound forms are supported per AD 0018.

## Core Entities

### 1. Archive

**Purpose**: Handle to an opened archive file, providing access to inspection, extraction, creation, and modification operations.

**Fields**:
```rust
pub struct Archive {
    // Backend-specific implementation (Piz, ZipReader, SevenZ, Libarchive, or UnRAR)
    backend: ArchiveBackend,
    // Entry cache for efficient repeated access (Phase 1 enhancement)
    // Uses once_cell::sync::OnceCell for lazy initialization
    entry_cache: OnceCell<Vec<ArchiveEntry>>,
    // Detected format (eagerly initialized during open())
    format: ArchiveFormat,
    // Path to archive file
    path: PathBuf,
    // Access mode
    mode: ArchiveMode,
}

enum ArchiveBackend {
    Unrar(UnrarArchive),
    Piz(PizArchive),
    SevenZ(SevenZArchive),
    ZipWriter(ZipWriter),
    ZipReader(ZipArchive),
    Libarchive(LibarchiveArchive),
}

enum ArchiveMode {
    Read,
    Write,
    Modify,
}
```

**Relationships**:
- **Contains**: 0..N `ArchiveEntry` (files within archive)
- **Has**: 1 `ArchiveFormat` (detected format type)
- **Uses**: `ExtractionOptions`, `CompressionOptions` for operation configuration
- **Uses**: `ModificationOptions` for modification configuration (consumed by `modify_with_options()`; `create_backup` and `backup_suffix` honored, `preserve_metadata` accepted but no-op pending OI-025-002)

**Lifecycle**:
```
[New] --open(path)--> [Open/Read]
[New] --create(path, format)--> [Open/Write]
[New] --modify(path)--> [Open/Modify]
[Open/Write] --finish()--> [Closed]
[Open/Write] --close()--> [Closed]
[Open/Modify] --commit_changes()--> [Closed]
[Open/Modify] --close()--> [Closed]
[Open/Read] --close()--> [Closed]
[Open/Read] --drop--> [Closed]
[Open/*] --error/drop--> [Failed] (resources released, no rollback)
```

> **Simplified diagram.** `close(self)` is a public finalization method available in all modes — it consumes the handle and releases resources. In Write mode it is equivalent to `finish()`; in Modify mode it is equivalent to `commit_changes()`; in Read mode it closes the handle explicitly instead of relying on Drop. On error during `finish()`, `commit_changes()`, or `close()`, the archive transitions to a failed state; partial output may remain on disk. There is no automatic rollback. Read-mode errors do not leave side-effects.

**Invariants**:
- Path must be valid UTF-8 or platform-specific encoding
- Format detection happens eagerly during open() (read and modify modes only; write mode uses the caller-supplied format)
- Cannot perform write operations in Read mode
- Cannot perform read operations in Write mode (until closed and reopened)
- Write mode: finalize with finish() or close(). Modify mode: finalize with commit_changes() or close(). Read mode: close() or Drop closes automatically.

**Validation Rules**:
- File exists and readable (for Read/Modify modes)
- Parent directory exists and writable (for Write mode)
- Format is supported (checked on open)
- Multiple read-mode Archive handles on the same file are allowed. Do not share a single Archive instance across threads (Archive is Send but not Sync).

**Known Backend Caveats**:
- **RAR (UnRAR):** UnRAR FFI calls are serialized via a process-wide mutex (`UNRAR_LOCK`); concurrent caller access is safe but RAR operations execute sequentially (OI-026-004 resolved).
- **ZipReader:** Used for encrypted-ZIP extraction; buffered in memory. See Backend Notes below for details.

### 2. ArchiveEntry

**Purpose**: Metadata for a single file or directory within an archive. The struct provides a shared schema across all formats; however, individual fields are populated on a best-effort, format-dependent basis (e.g., `crc32` is None for TAR; `created`/`accessed` are None for most formats).

**Fields**:
```rust
pub struct ArchiveEntry {
    // Full path within archive (UTF-8, forward slashes)
    pub path: String,

    // File size (uncompressed), None for directories
    pub size: Option<u64>,

    // Compressed size, None for uncompressed formats or directories
    pub compressed_size: Option<u64>,

    // Last modification time (UTC)
    pub modified: Option<SystemTime>,

    // **Phase 1 Enhancement**: Creation time (UTC), format-dependent
    pub created: Option<SystemTime>,

    // **Phase 1 Enhancement**: Last access time (UTC), format-dependent
    pub accessed: Option<SystemTime>,

    // CRC32 checksum, None if not available for format
    pub crc32: Option<u32>,

    // compression_ratio() -> Option<f64> — computed from size and compressed_size

    // **Phase 1 Enhancement**: Whether this entry is encrypted
    pub is_encrypted: bool,

    // **Phase 1 Enhancement**: File comment (if supported by format)
    // NOTE: No backend currently populates this field; always None at runtime.
    pub comment: Option<String>,

    // Entry type
    pub entry_type: EntryType,

    // Unix permissions (if preserved by format)
    pub permissions: Option<u32>,

    // **Phase 1 Enhancement**: Platform-specific file attributes
    pub attributes: Option<FileAttributes>,

    // Entry index within archive (for extraction)
    pub id: usize,
}

pub enum EntryType {
    File,
    Directory,
    Symlink,  // Phase 1: Representable in metadata
    HardLink, // Phase 1: Representable in metadata
    Other,    // Phase 1: Catch-all for unknown entry types
}
// Operation policy: Symlink and HardLink entries are skipped with warnings
// during extraction per FR-022. This is an extraction-time policy, not a
// limitation of the EntryType representation.

// **Phase 1 Enhancement**: Platform-specific file attributes
#[derive(Clone, Debug)]
pub struct FileAttributes {
    // Windows: FILE_ATTRIBUTE_* flags
    pub windows: Option<u32>,
    // Unix: Extended attributes
    pub unix_xattr: Option<Vec<(String, Vec<u8>)>>,
    // Archive-specific attributes
    pub archive_specific: Option<String>,
}
```

**Relationships**:
- **Belongs to**: 1 `Archive` (parent archive)

**Invariants**:
- Path uses forward slashes (/) as separator (converted from format-specific)
- Path is relative (no leading /)
- Directories end with / in path
- size and compressed_size are Some(0) for empty files, None for directories
- crc32 is None for directories
- **Phase 1**: compression_ratio() is a computed method returning `compressed_size / size` when both Some, >= 0.0 (values above 1.0 indicate data expansion); returns None for directories or when size/compressed_size unavailable
- **Phase 1**: is_encrypted reflects entry-level encryption (format-dependent)
- **Phase 1**: created/accessed timestamps UTC-normalized from format-specific local times
- **Phase 1**: FileAttributes platform-specific, None when not preserved by format
- Symbolic links and hard links are representable in metadata but skipped during extraction (FR-022)

**Validation Rules**:
- Path is valid UTF-8
- modified time is representable on target platform
- permissions are valid Unix mode bits (if Some)
- **Extraction-time only:** Paths containing `..` components are rejected during extraction (path sanitization). ArchiveEntry itself reflects stored metadata as-is and may contain `..` segments.

### 3. ArchiveFormat

**Purpose**: Enumeration of supported archive formats, used for format detection and capability queries.

**Definition**:
```rust
pub enum ArchiveFormat {
    SevenZip,
    Zip,
    Rar,
    Rar5,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    Gzip,   // Dormant: not directly openable (AD 0018); exists for detection/display only
    Bzip2,  // Dormant: not directly openable (AD 0018); exists for detection/display only
    Xz,     // Dormant: not directly openable (AD 0018); exists for detection/display only
    Iso,
}

impl ArchiveFormat {
    // Detect format from file magic bytes
    pub fn detect(path: &Path) -> Result<Self, ArchiveError>;

    // Query format capabilities
    pub fn supports_compression(&self) -> bool;
    pub fn supports_encryption(&self) -> bool;
    pub fn supports_multipart(&self) -> bool;
    pub fn can_modify(&self) -> bool;

    // Get format-specific extensions
    pub fn extensions(&self) -> &[&str];
}
```

**Relationships**:
- **Describes**: 1 `Archive` (format type)

**Invariants**:
- Detection uses format signatures (magic bytes) and heuristics; some formats share prefixes or lack a magic number, so detection may fall back to extension matching
- Capabilities are fixed per format
- Extensions list is non-empty

**Format Capabilities Matrix**:
| Format | Compression | Encryption | Multipart | Modification | Notes |
|--------|------------|------------|-----------|--------------|-------|
| 7z | Yes | Yes | Yes | Partial | `commit_changes()` may lose metadata (OI-025-001/002) |
| ZIP | Yes | Yes | Yes | Partial | Same `commit_changes()` caveat |
| RAR | Yes | Yes | Yes | No (read-only) | |
| RAR5 | Yes | Yes | Yes | No (read-only) | |
| TAR | No (plain) | No | No | No | |
| TAR.GZ/BZ2/XZ | Yes (\*) | No | No | No | (\*) Compression is applied to the outer stream, not per-entry |
| ISO | No | No | No | No | |

### 4. ExtractionOptions

**Purpose**: Configuration for archive extraction operations.

**Fields**:
```rust
pub struct ExtractionOptions {
    // Destination directory
    pub destination: PathBuf,

    // Password for encrypted archives
    pub password: Option<String>,

    // Overwrite existing files (default: false, fails with error if files exist - FR-023)
    pub overwrite: bool,

    // Preserve file permissions (Unix)
    pub preserve_permissions: bool,

    // Preserve modification times
    pub preserve_times: bool,

    // Filter: only extract matching paths
    pub filter: Option<Box<dyn Fn(&ArchiveEntry) -> bool + Send + Sync>>,

    // **Phase 1 Enhancement**: Progress callback (boxed trait object with dynamic dispatch)
    pub progress: Option<Box<dyn ProgressCallback>>,

    // **Phase 1 Enhancement**: Verify CRC32 during extraction
    pub verify_crc32: bool,

    // **Phase 1 Enhancement**: Safety limits for extraction
    pub limits: ExtractionLimits,
}

// Pseudocode — see src/options.rs for the compiled Default impl.
// Fields shown here may not match the current struct exactly.
impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            destination: PathBuf::from("."),
            password: None,
            overwrite: false,
            preserve_permissions: true,
            preserve_times: true,
            filter: None,
            progress: None,
            verify_crc32: true,  // Phase 1: Verify by default for integrity
            limits: ExtractionLimits::default(),
        }
    }
}
```

**Relationships**:
- **Configures**: `Archive::extract_all()`, `Archive::extract_file()`, `Archive::extract_filtered()`, `Archive::extract_files()`, and `Archive::extract_by_ids()`. The memory/stream paths (`extract_to_memory()` and `extract_to_stream()`) do not accept ExtractionOptions.

**Invariants**:
- destination must be a directory (not file)
- password is required for encrypted archives (checked at runtime)
- When overwrite=false (default), extraction fails if any destination file already exists (FR-023)

**Validation Rules**:
- destination directory exists or can be created
- **Caller responsibility:** The filter function must not panic. A panic inside the filter will unwind through the extraction loop and may leave partial output on disk.

### 5. CompressionOptions

**Purpose**: Configuration for archive creation operations.

**Fields**:
```rust
pub struct CompressionOptions {
    // Target archive format
    pub format: ArchiveFormat,

    // Compression level (named variants: Store, Fastest, Fast, Normal, Maximum, Ultra)
    pub level: CompressionLevel,

    // Password for encryption
    pub password: Option<String>,

    // Split archive into parts (size in bytes)
    pub split_size: Option<u64>,

    // Progress callback — wired to creation backends (OI-025-003 resolved, AD 0021).
    // Invoked per-entry during add_file_from_data/add_file_from_path; total is None
    // because creation streams are not pre-sized. ControlFlow::Break cancels creation.
    pub progress: Option<Box<dyn ProgressCallback>>,
}

pub enum CompressionLevel {
    Store,      // 0 - no compression
    Fastest,    // 1 - fastest compression
    Fast,       // 3
    Normal,     // 5 - balanced
    Maximum,    // 7
    Ultra,      // 9 - maximum compression
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            format: ArchiveFormat::Zip,
            level: CompressionLevel::Normal,
            password: None,
            split_size: None,
            progress: None,
        }
    }
}
```

**Relationships**:
- **Configures**: `Archive::create()` operation

**Invariants**:
- format must support compression if level != Store

> **Deferred (DEF-002):** `split_size` field exists in the struct but is currently ignored by all creation backends. The intended future invariant is `split_size >= 64KB` when `Some`. Validation of `split_size` against format multipart support is also deferred.

**Validation Rules**:
- format supports encryption if password is Some

### 6. ArchiveError

**Purpose**: Unified error type for all archive operations, consistent across formats.

**Definition**:
```rust
pub enum ArchiveError {
    // I/O errors (file not found, permission denied, etc.)
    Io {
        operation: String,
        path: PathBuf,
        source: std::io::Error,
    },

    // Format-specific errors (unsupported format, corrupted data)
    Format {
        format: Option<ArchiveFormat>,
        message: String,
    },

    // Corruption detected (CRC mismatch, truncated data)
    Corruption {
        path: String,
        details: String,
    },

    // Password required or incorrect
    Password {
        message: String,
    },

    // Unsupported operation for format
    Unsupported {
        operation: String,
        format: ArchiveFormat,
        details: Option<String>,
    },

    // Required codec not available
    CodecUnavailable {
        codec: String,
        format: ArchiveFormat,
        install_instructions: String,
    },

    // Operation not supported in current context
    UnsupportedOperation {
        operation: String,
        reason: String,
    },

    // Invalid path (security: .. traversal, absolute paths)
    InvalidPath {
        path: String,
        reason: String,
    },
}

impl std::error::Error for ArchiveError {}
impl std::fmt::Display for ArchiveError {
    // User-friendly error messages
}
```

**Relationships**:
- **Returned by**: All archive operations

**Invariants**:
- Error messages are actionable (tell user what to do)
- source errors are preserved (for debugging)
- Error type distinguishes between recoverable and fatal errors

**Error Categories** (illustrative, not exhaustive):
1. **Potentially recoverable**: Wrong password (retry with correct password), file not found (user action)
2. **Typically fatal**: Corruption, unsupported format

> Whether an error is truly recoverable depends on the caller's context. These categories are guidance for API consumers, not hard guarantees.

### 7. ProgressCallback (Phase 1 Enhanced)

**Purpose**: Trait for progress reporting during long-running operations with cancellation support.

**Definition**:
```rust
use std::ops::ControlFlow;

// **Phase 1 Enhancement**: Boxed trait object with ControlFlow for cancellation
// Requires Send + Sync bounds for thread-safe usage
pub trait ProgressCallback: Send + Sync {
    // Called periodically during operation
    // current: bytes processed so far
    // total: total bytes (None if unknown)
    // Returns: ControlFlow::Continue(()) to proceed, ControlFlow::Break(()) to cancel
    fn on_progress(&mut self, current: u64, total: Option<u64>) -> ControlFlow<()>;
}

// Convenience: Implement for any compatible closure (must be Send + Sync)
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> ControlFlow<()> + Send + Sync,
{
    fn on_progress(&mut self, current: u64, total: Option<u64>) -> ControlFlow<()> {
        self(current, total)
    }
}

// **Phase 1**: Time-based rate limiter (internal use, defined in src/options.rs)
pub(crate) struct RateLimiter {
    last_call: Instant,
    interval: Duration,  // Default: 16ms
}
```

**Relationships**:
- **Used by**: `ExtractionOptions`, `CompressionOptions`

**Invariants**:
- on_progress() rate-limited to a maximum of ~60 updates/second (16ms interval). Actual callback frequency varies by backend and workload.
- current <= total always (monotonically increasing)
- **Phase 1**: Boxed trait object (`Box<dyn ProgressCallback>`)
- **Phase 1**: Rate limiting caps maximum frequency; does not guarantee minimum cadence

**Cancellation Semantics** (canonical source — supersedes any conflicting description elsewhere):
- Returning `ControlFlow::Break(())` from `on_progress()` signals cancellation.
- The backend surfaces an `ArchiveError` (typically `Format { message: "cancelled" }` or similar). There is no dedicated `Cancelled` error variant.
- Partial output may remain on disk after cancellation; cleanup is the caller's responsibility.
- Cancellation is checked between entries or between read chunks, depending on the backend; it is not instant.

### 8. StreamingExtractor

**Purpose**: Streaming reader for extracting individual archive entries. Created via `Archive::extract_to_stream()`. Memory behaviour is backend-dependent: libarchive backends stream with a fixed read buffer, while Piz/SevenZ/UnRAR backends buffer the full entry in memory before wrapping it in a `StreamingExtractor`.

**Definition**:
```rust
use std::io::Read;

/// Streaming extractor that implements Read trait.
/// The internal reader is an abstraction over backend-specific sources:
/// - Libarchive backends: fixed-size read buffer (true streaming, bounded memory)
/// - Native backends (Piz/ZipReader/SevenZ/UnRAR): in-memory Cursor over the
///   fully-buffered entry data
/// No temporary files are used by StreamingExtractor itself.
pub struct StreamingExtractor {
    /// Internal reader - backend-dependent (true stream or in-memory Cursor)
    reader: Box<dyn Read + Send>,
    /// Total bytes available (if known)
    total_size: Option<u64>,
    /// Bytes read so far
    bytes_read: u64,
}

impl StreamingExtractor {
    /// Get total size if known
    pub fn total_size(&self) -> Option<u64>;

    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64;

    /// Get progress as percentage (0.0 to 1.0) if total size is known
    pub fn progress(&self) -> Option<f64>;
}

impl Read for StreamingExtractor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Delegates to internal reader, tracks bytes_read
    }
}
```

**Relationships**:
- **Created by**: `Archive::extract_to_stream()` method
- **Reads from**: Archive backend (boxed `Read + Send`)

**Invariants**:
- Implements `Read` trait for standard I/O composition
- **Libarchive backends:** Only the read buffer is held in memory (true streaming). **Piz/ZipReader/SevenZ/UnRAR backends:** The full entry is buffered in memory before constructing StreamingExtractor. Bounded-memory streaming is backend-dependent.
- Single-use: consumed after reading to EOF
- Tracks progress via `bytes_read` / `total_size`. When `total_size` is `None` (e.g., for entries where the backend does not report uncompressed size), `progress()` returns `None`; callers should display an indeterminate progress indicator.

## Entity Relationships Diagram

```
Archive (1) --contains--> (0..N) ArchiveEntry (enhanced with 6 new fields)
  |
  +--caches--> (0..1) Vec<ArchiveEntry> via OnceCell (Phase 1)
  |
  +--has--> (1) ArchiveFormat
  |
  +--has--> (1) ArchiveBackend (Piz | ZipReader | SevenZ | Libarchive | UnRAR | ZipWriter)
  |
  +--creates--> (0..N) StreamingExtractor (streaming extraction)
  |
  +--uses--> (0..1) ExtractionOptions (enhanced with 3 new fields)
  |          |
  |          +--uses--> (0..1) ProgressCallback (Phase 1: ControlFlow)
  |          +--uses--> (0..1) String (password)
  |
  +--uses--> (0..1) CompressionOptions
             |
             +--uses--> (0..1) ProgressCallback

StreamingExtractor
  |
  +--wraps--> Box<dyn Read + Send> (backend reader)

All operations return Result<T, ArchiveError>
```

## State Transitions

### Archive Lifecycle

```
                    +------------------+
                    | Uninitialized    |
                    +------------------+
                            |
        +-------------------+-------------------+
        |                   |                   |
    open()            create()              modify()
        |                   |                   |
        v                   v                   v
+----------------+  +----------------+  +----------------+
| Open/Read      |  | Open/Write       |  | Open/Modify      |
| - list()       |  | - add_file_from_ |  | - add_entry()    |
| - extract_all()|  |   data()         |  | - remove_entry() |
| - validate()   |  | - add_file_from_ |  | - replace_entry()|
|                |  |   path()         |  | - extract_all()  |
|                |  | - add_file_from_ |  |                  |
|                |  |   path_as()      |  |                  |
|                |  | - add_directory() |  |                  |
|                |  | - add_directory_ |  |                  |
|                |  |   recursive()    |  |                  |
+----------------+  +----------------+  +----------------+
        |                   |                   |
   drop/close()      finish()/close()  commit_changes()/close()
        |                   |                   |
        +-------------------+-------------------+
                            |
                            v
                    +------------------+
                    | Closed           |
                    +------------------+
```

### Entry Discovery

```
Archive::open()
    |
    v
Format detection (magic bytes)
    |
    v
Parse archive header
    |
    v
Return Archive handle (entries NOT yet enumerated)
    |
    v
First list_files() call:
    Enumerate entries → OnceCell<Vec<ArchiveEntry>> (cached for subsequent calls)
    |
    v
Return &[ArchiveEntry]
```

## Invariant Preservation

### Cross-Format Consistency

All operations preserve these invariants across formats:

1. **Path normalization**: Forward slashes, relative paths (stored metadata may contain `..`; sanitization occurs at extraction time)
2. **Time representation**: UTC SystemTime (converted from format-specific)
3. **Size consistency**: compressed_size may exceed size (data expansion is possible)
4. **CRC availability**: Present if format supports, None otherwise
5. **Error semantics**: All formats use `ArchiveError`, but the specific variant produced for a given situation may differ by backend (e.g., a corrupted RAR may yield `Format` while a corrupted ZIP yields `Corruption`). The variant set is shared; the mapping is best-effort.

### Format-Specific Handling (Internal)

Internally, format-specific adapters handle:
- Path separator conversion (\ → / for ZIP)
- Time zone conversion (local → UTC for TAR)
- Permission mapping (Windows → Unix for cross-platform)
- Encryption method abstraction (AES, ZipCrypto, RAR)

### Memory Safety

- No raw pointers exposed in public API
- FFI handles wrapped in RAII types (Drop impl for cleanup)
- Format is stored eagerly during open() (no interior mutability needed)
- Send/Sync bounds where appropriate (ProgressCallback is Send + Sync)

## Validation Strategy

**Note:** These examples are schematic; see `src/options.rs` for the current extraction API shape.

### Input Validation

```rust
impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError> {
        let path = path.as_ref();

        // Validate file exists
        if !path.exists() {
            return Err(ArchiveError::Io { ... });
        }

        // Validate file is readable
        File::open(path)?;

        // Detect format
        let format = ArchiveFormat::detect(path)?;

        Ok(Self { ... })
    }
}
```

### Runtime Validation

```rust
impl Archive {
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<(), ArchiveError> {
        // Validate mode (extract_all requires Read or Modify mode)
        if self.mode == ArchiveMode::Write {
            return Err(ArchiveError::UnsupportedOperation {
                operation: "extract_all".into(),
                reason: "cannot extract in Write mode".into(),
            });
        }

        // Validate destination
        if !options.destination.is_dir() {
            return Err(ArchiveError::Io { ... });
        }

        // Check password for encrypted archives
        if self.is_encrypted()? && options.password.is_none() {
            return Err(ArchiveError::Password { ... });
        }

        // Proceed with extraction
        ...
    }
}
```

## Performance Considerations

### Memory Efficiency

- **Entry listing**: Uses a cached `Vec<ArchiveEntry>` populated on first access via `list_files()`
- **Eager format detection**: Format detected and backend initialized during `open()`
- **FFI data mapping**: Metadata fields are copied from FFI structures into owned Rust types during entry enumeration. The bulk data path (extraction) streams through backend-provided buffers.

### Time Complexity

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Archive::open() | O(1) | Eager format detection and backend initialization |
| Archive::list_files() | O(n) | n = number of entries |
| Archive::extract_file() | O(n + m) | n = entries scanned, m = file size |
| Archive::extract_all() | O(n * m) | n = entries, m = avg file size |
| Archive::validate_integrity() | O(n * m) | Full CRC check |

### Space Complexity

| Operation | Space Complexity | Notes |
|-----------|-----------------|-------|
| Archive::list_files() | O(n) | Vec<ArchiveEntry> |
| Archive::extract_all() | O(1) + buffer | Backend-dependent: libarchive streams with fixed buffers; Piz/ZipReader/SevenZ/UnRAR buffer full entries in memory. |
| Archive::create() | O(1) + buffer | Streaming writes |

## Testing Strategy

### Unit Tests

- Each entity's validation rules
- State transition enforcement
- Error handling for all failure modes

### Integration Tests

- Cross-format consistency (same operations on different formats)
- Round-trip testing (create → extract → verify)
- Large file handling (10GB archives, bounded memory for libarchive-backed formats; native backends buffer entries)

### Property-Based Tests

```rust
proptest! {
    #[test]
    fn path_normalization_consistent(path: String) {
        // All formats produce same normalized path
        let entry_zip = extract_from_zip(&path);
        let entry_7z = extract_from_7z(&path);
        prop_assert_eq!(entry_zip.path, entry_7z.path);
    }
}
```

## Phase 1 Enhancements Summary

### New Fields Added

**ArchiveEntry enhancements** (6 new fields):
1. `created: Option<SystemTime>` - Creation time (format-dependent)
2. `accessed: Option<SystemTime>` - Last access time (format-dependent)
3. `compression_ratio()` method - Computed ratio (>= 0.0, can exceed 1.0 for data expansion), no longer a stored field
4. `is_encrypted: bool` - Entry-level encryption flag
5. `comment: Option<String>` - File comment (ZIP, RAR). **Currently always `None`** — no backend populates this field yet.
6. `attributes: Option<FileAttributes>` - Platform-specific attributes

**ExtractionOptions enhancements** (3 new fields):
1. `password: Option<String>` - Password for encrypted archives
2. `verify_crc32: bool` - CRC32 verification during extraction
3. `limits: ExtractionLimits` - Safety limits for extraction (max file size, max entry count, etc.)

### New Entities

1. **FileAttributes** - Platform-specific file attributes (Windows, Unix xattr)
2. **StreamingExtractor** - Streaming reader via `Archive::extract_to_stream()`
3. **RateLimiter** - Time-based progress callback rate limiter (16ms default interval, defined in `src/options.rs`)

### Architectural Changes

1. **Entry Caching**: `Archive.entry_cache: OnceCell<Vec<ArchiveEntry>>`
   - Zero-cost after first access
   - Resolves UnRAR sequential iteration limitation
   - Returns `&[ArchiveEntry]` (no allocation on repeated access)

2. **Progress Callbacks**: Trait-based with `ControlFlow`
   - Boxed trait object (`Box<dyn ProgressCallback>`)
   - Cancellation support via `ControlFlow::Break(())`
   - Rate-limited to ~60 updates/second max (16ms interval)

3. **Streaming Extraction**: `impl Read for StreamingExtractor`
   - Libarchive backends: true streaming with only read buffer in memory. Piz/ZipReader/SevenZ/UnRAR: full entry buffered in memory before streaming.
   - Standard library composition (`io::copy`, `BufReader`)
   - Progress tracking via `bytes_read()` / `total_size()`

### Dependency Changes

**New dependencies** (Phase 1):
- `crc32fast` - SIMD-accelerated CRC32 verification
- `rayon` - Parallel extraction (conditional, >10 files)

### Constitution Compliance

All Phase 1 enhancements comply with updated constitution (v1.1.0):

- ✅ **Principle II (Pragmatic Performance)**:
  - OnceCell: Small change, significant caching gain
  - Rayon: Large change justified by 10%+ speedup
  - CRC32fast: <2% overhead (within threshold)

- ✅ **Principle III (Unified Interface + Minimal Deps)**:
  - 3 dependencies justified by critical benefits
  - Shared API surface across all platforms with documented backend caveats (see Known Backend Caveats above)

- ✅ **Principle I (Robustness)**:
  - All patterns maintain error handling
  - Memory safety preserved (no raw pointers exposed)

## References

- 7zip-JBinding API: Reference for entity naming and relationships
- libarchive documentation: Format-specific metadata handling
- Rust API guidelines: Naming conventions and error handling patterns
- Phase 0 Research: `research.md` - Technical decisions and rationale
- Constitution v1.1.0: `.specify/memory/constitution.md` - Pragmatic performance principles
