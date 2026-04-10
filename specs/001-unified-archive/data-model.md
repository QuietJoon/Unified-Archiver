# Data Model: unified-archive

**Date**: 2025-10-31 (Updated for Phase 1 enhancements)
**Feature**: 001-unified-archive
**Purpose**: Define core entities, their relationships, and state transitions for the unified archive interface

## Overview

The data model provides a unified, format-agnostic representation of archives and their contents. All entities are designed to work identically across different archive formats (7z, RAR, RAR5, ZIP, TAR, GZIP, BZIP2, XZ, ISO).

## Core Entities

### 1. Archive

**Purpose**: Handle to an opened archive file, providing access to inspection, extraction, creation, and modification operations.

**Fields**:
```rust
pub struct Archive {
    // Backend-specific implementation (Piz, ZipReader, SevenZ, Libarchive, or UnRAR)
    backend: ArchiveBackend,
    // Entry cache for efficient repeated access (Phase 1 enhancement)
    entry_cache: OnceLock<Vec<ArchiveEntry>>,
    // Detected format (lazy-loaded on first access)
    format: OnceCell<ArchiveFormat>,
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
- **Uses**: `ArchiveOptions` for configuration

**Lifecycle**:
```
[Created] --open()--> [Open/Read] --close()--> [Closed]
[Created] --create()--> [Open/Write] --close()--> [Closed]
[Open/Read] --modify()--> [Open/Modify] --close()--> [Closed]
```

**Invariants**:
- Path must be valid UTF-8 or platform-specific encoding
- Format detection happens on first operation (lazy)
- Cannot perform write operations in Read mode
- Cannot perform read operations in Write mode (until closed and reopened)
- Must call close() or Drop will auto-close

**Validation Rules**:
- File exists and readable (for Read/Modify modes)
- Parent directory exists and writable (for Write mode)
- Format is supported (checked on open)
- No concurrent access to same file from multiple Archive instances

### 2. ArchiveEntry

**Purpose**: Metadata for a single file or directory within an archive, consistent across all formats.

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
    pub comment: Option<String>,

    // Entry type
    pub entry_type: EntryType,

    // Unix permissions (if preserved by format)
    pub permissions: Option<u32>,

    // **Phase 1 Enhancement**: Platform-specific file attributes
    pub attributes: Option<FileAttributes>,

    // Entry index within archive (for extraction)
    pub(crate) index: usize,
}

pub enum EntryType {
    File,
    Directory,
    Symlink,  // Phase 1: Added for completeness (still skipped during operations per FR-022)
    Other,    // Phase 1: Added for unknown entry types
}

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
- **Phase 1**: compression_ratio() is a computed method returning `compressed_size / size` when both Some, range [0.0, 1.0]; returns None for directories or when size/compressed_size unavailable
- **Phase 1**: is_encrypted reflects entry-level encryption (format-dependent)
- **Phase 1**: created/accessed timestamps UTC-normalized from format-specific local times
- **Phase 1**: FileAttributes platform-specific, None when not preserved by format
- Symbolic links and hard links are skipped with warnings (FR-022)

**Validation Rules**:
- Path is valid UTF-8
- Path does not contain `..` components (security)
- modified time is representable on target platform
- permissions are valid Unix mode bits (if Some)

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
    Gzip,
    Bzip2,
    Xz,
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
- Each format has unique magic byte signature
- Capabilities are fixed per format
- Extensions list is non-empty

**Format Capabilities Matrix**:
| Format | Compression | Encryption | Multipart | Modification |
|--------|------------|------------|-----------|--------------|
| 7z | Yes | Yes | Yes | Yes |
| ZIP | Yes | Yes | Yes | Yes |
| RAR | Yes | Yes | Yes | No (read-only) |
| RAR5 | Yes | Yes | Yes | No (read-only) |
| TAR.* | Yes (outer) | No | No | No |
| ISO | No | No | No | No |

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
    pub filter: Option<Box<dyn Fn(&ArchiveEntry) -> bool>>,

    // **Phase 1 Enhancement**: Progress callback (trait-based, zero-cost)
    pub progress: Option<Box<dyn ProgressCallback>>,

    // **Phase 1 Enhancement**: Verify CRC32 during extraction
    pub verify_crc32: bool,
}

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
        }
    }
}
```

**Relationships**:
- **Configures**: `Archive::extract_all()`, `Archive::extract_file()`, `Archive::extract_to_memory()`, `Archive::extract_to_stream()` operations

**Invariants**:
- destination must be a directory (not file)
- password is required for encrypted archives (checked at runtime)
- When overwrite=false (default), extraction fails if any destination file already exists (FR-023)

**Validation Rules**:
- destination directory exists or can be created
- filter function does not panic

### 5. CompressionOptions

**Purpose**: Configuration for archive creation operations.

**Fields**:
```rust
pub struct CompressionOptions {
    // Target archive format
    pub format: ArchiveFormat,

    // Compression level (0=store, 9=max)
    pub level: CompressionLevel,

    // Password for encryption
    pub password: Option<String>,

    // Split archive into parts (size in bytes)
    pub split_size: Option<u64>,

    // Progress callback
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
- split_size must be reasonable (>= 64KB if Some)

**Validation Rules**:
- format supports encryption if password is Some
- format supports multipart if split_size is Some

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

**Error Categories**:
1. **Recoverable**: Wrong password, file not found
2. **Fatal**: Corruption, unsupported format

### 7. ProgressCallback (Phase 1 Enhanced)

**Purpose**: Trait for progress reporting during long-running operations with cancellation support.

**Definition**:
```rust
use std::ops::ControlFlow;

// **Phase 1 Enhancement**: Boxed trait object with ControlFlow for cancellation
pub trait ProgressCallback {
    // Called periodically during operation
    // current: bytes processed so far
    // total: total bytes (None if unknown)
    // Returns: ControlFlow::Continue(()) to proceed, ControlFlow::Break(()) to cancel
    fn on_progress(&mut self, current: u64, total: Option<u64>) -> ControlFlow<()>;
}

// Convenience: Implement for any compatible closure
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> ControlFlow<()>,
{
    fn on_progress(&mut self, current: u64, total: Option<u64>) -> ControlFlow<()> {
        self(current, total)
    }
}

// **Phase 1**: Rate limiting wrapper (internal use)
pub(crate) struct RateLimitedCallback<C: ProgressCallback> {
    callback: C,
    last_call: Instant,
    last_bytes: u64,
    min_interval_ms: u64,  // Default: 100ms (≥10 updates/sec)
    min_bytes: u64,        // Default: 100KB
}
```

**Relationships**:
- **Used by**: `ExtractionOptions`, `CompressionOptions`

**Invariants**:
- on_progress() called at least 10 times per second for operations >1 second (via rate limiting)
- current <= total always (monotonically increasing)
- **Phase 1**: Boxed trait object (`Box<dyn ProgressCallback>`)
- **Phase 1**: Rate limiting ensures ≤10ms overhead per update

**Validation Rules**:
- Callback does not panic (wrapped in catch_unwind internally)
- Returning ControlFlow::Break(()) cancels operation gracefully, returns ArchiveError::Cancelled

### 8. StreamingExtractor

**Purpose**: Streaming reader for extracting individual archive entries with bounded memory usage. Created via `Archive::extract_to_stream()`.

**Definition**:
```rust
use std::io::Read;

/// Streaming extractor that implements Read trait
pub struct StreamingExtractor {
    /// Internal reader - either from temporary file or direct stream
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
- Memory bounded: only the read buffer is held in memory
- Single-use: consumed after reading to EOF
- Tracks progress via `bytes_read` / `total_size`

## Entity Relationships Diagram

```
Archive (1) --contains--> (0..N) ArchiveEntry (enhanced with 6 new fields)
  |
  +--caches--> (0..1) Vec<ArchiveEntry> via OnceLock (Phase 1)
  |
  +--has--> (1) ArchiveFormat
  |
  +--has--> (1) ArchiveBackend (Piz | ZipReader | SevenZ | Libarchive | UnRAR)
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
| Open/Read      |  | Open/Write     |  | Open/Modify    |
| - list()       |  | - add_file()   |  | - add_file()   |
| - extract_all()|  | - add_dir()    |  | - remove()     |
| - validate()   |  |                |  | - extract_all()|
+----------------+  +----------------+  +----------------+
        |                   |                   |
        +-------------------+-------------------+
                            |
                        close()
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
Enumerate entries (lazy iterator)
    |
    v
Return Vec<ArchiveEntry>
```

## Invariant Preservation

### Cross-Format Consistency

All operations preserve these invariants across formats:

1. **Path normalization**: Forward slashes, relative paths, no `..`
2. **Time representation**: UTC SystemTime (converted from format-specific)
3. **Size consistency**: compressed_size <= size (if both Some)
4. **CRC availability**: Present if format supports, None otherwise
5. **Error semantics**: Same ArchiveError variants for all formats

### Format-Specific Handling (Internal)

Internally, format-specific adapters handle:
- Path separator conversion (\ → / for ZIP)
- Time zone conversion (local → UTC for TAR)
- Permission mapping (Windows → Unix for cross-platform)
- Encryption method abstraction (AES, ZipCrypto, RAR)

### Memory Safety

- No raw pointers exposed in public API
- FFI handles wrapped in RAII types (Drop impl for cleanup)
- Interior mutability for lazy format detection (OnceCell)
- Send/Sync bounds where appropriate (ProgressCallback is Send)

## Validation Strategy

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
        // Validate mode
        if self.mode != ArchiveMode::Read {
            return Err(ArchiveError::Unsupported { ... });
        }

        // Validate destination
        if !options.destination.is_dir() {
            return Err(ArchiveError::Io { ... });
        }

        // Check password for encrypted archives
        if self.is_encrypted() && options.password.is_none() {
            return Err(ArchiveError::Password { ... });
        }

        // Proceed with extraction
        ...
    }
}
```

## Performance Considerations

### Memory Efficiency

- **Streaming**: Archive operations use streaming iterators (no full file list in memory)
- **Lazy loading**: Format detection deferred until first operation
- **Zero-copy**: Where possible, FFI data structures mapped to Rust without copying

### Time Complexity

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Archive::open() | O(1) | Lazy format detection |
| Archive::list_files() | O(n) | n = number of entries |
| Archive::extract_file() | O(n + m) | n = entries scanned, m = file size |
| Archive::extract_all() | O(n * m) | n = entries, m = avg file size |
| Archive::validate() | O(n * m) | Full CRC check |

### Space Complexity

| Operation | Space Complexity | Notes |
|-----------|-----------------|-------|
| Archive::list_files() | O(n) | Vec<ArchiveEntry> |
| Archive::extract_all() | O(1) + buffer | Streaming, fixed buffer size |
| Archive::create() | O(1) + buffer | Streaming writes |

## Testing Strategy

### Unit Tests

- Each entity's validation rules
- State transition enforcement
- Error handling for all failure modes

### Integration Tests

- Cross-format consistency (same operations on different formats)
- Round-trip testing (create → extract → verify)
- Large file handling (10GB archives, bounded memory)

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
3. `compression_ratio()` method - Computed ratio (0.0-1.0), no longer a stored field
4. `is_encrypted: bool` - Entry-level encryption flag
5. `comment: Option<String>` - File comment (ZIP, RAR)
6. `attributes: Option<FileAttributes>` - Platform-specific attributes

**ExtractionOptions enhancements** (2 new fields):
1. `password: Option<String>` - Password for encrypted archives
2. `verify_crc32: bool` - CRC32 verification during extraction

### New Entities

1. **FileAttributes** - Platform-specific file attributes (Windows, Unix xattr)
2. **StreamingExtractor** - Streaming reader via `Archive::extract_to_stream()`
3. **RateLimitedCallback<C>** - Progress callback rate limiter (≥10 updates/sec)

### Architectural Changes

1. **Entry Caching**: `Archive.entry_cache: OnceLock<Vec<ArchiveEntry>>`
   - Zero-cost after first access
   - Resolves UnRAR sequential iteration limitation
   - Returns `&[ArchiveEntry]` (no allocation on repeated access)

2. **Progress Callbacks**: Trait-based with `ControlFlow`
   - Boxed trait object (`Box<dyn ProgressCallback>`)
   - Cancellation support via `ControlFlow::Break(())`
   - Rate-limited (≤10ms overhead per update)

3. **Streaming Extraction**: `impl Read for StreamingExtractor`
   - Memory bounded: only read buffer held in memory
   - Standard library composition (`io::copy`, `BufReader`)
   - Progress tracking via `bytes_read()` / `total_size()`

### Dependency Changes

**New dependencies** (Phase 1):
- `crc32fast` - SIMD-accelerated CRC32 verification
- `rayon` - Parallel extraction (conditional, >10 files)

### Constitution Compliance

All Phase 1 enhancements comply with updated constitution (v1.1.0):

- ✅ **Principle II (Pragmatic Performance)**:
  - OnceLock: Small change, significant caching gain
  - Rayon: Large change justified by 10%+ speedup
  - CRC32fast: <2% overhead (within threshold)

- ✅ **Principle III (Unified Interface + Minimal Deps)**:
  - 3 dependencies justified by critical benefits
  - Public API identical across all platforms

- ✅ **Principle I (Robustness)**:
  - All patterns maintain error handling
  - Memory safety preserved (no raw pointers exposed)

## References

- 7zip-JBinding API: Reference for entity naming and relationships
- libarchive documentation: Format-specific metadata handling
- Rust API guidelines: Naming conventions and error handling patterns
- Phase 0 Research: `research.md` - Technical decisions and rationale
- Constitution v1.1.0: `.specify/memory/constitution.md` - Pragmatic performance principles
