# Known Limitations

This document tracks known limitations and constraints in the unified-archive library.

## Current State

The library provides a unified Rust API for reading, extracting, creating, and modifying archives across multiple formats. Core functionality (inspection, extraction, creation, SFX detection) is shipped. Modification works for ZIP and 7z but is lossy. This document describes the remaining gaps and caveats.

---

### 1. UnRAR Sequential Iteration (Resolved)

The UnRAR library uses a sequential iterator for reading archive entries. Once all entries have been read, the iterator is exhausted and subsequent operations return empty results.

This is no longer a practical issue. Entry caching (AD 0004) transparently caches the result of the first `list_files()` call via `OnceCell`, so repeated access returns the cached list without re-iterating. Callers no longer need to reopen the archive between operations.

**Residual caveat**: If the underlying archive is modified on disk between calls, the cached entry list becomes stale. The cache is per-`Archive` instance and is not invalidated.

---

### 2. CRC32 Verification During Extraction

**Status**: Implemented -- Automatic CRC32 verification during extraction

**Implementation**:
Backend libraries perform **automatic CRC32 verification** during file extraction:

- **UnRAR**: Automatically verifies CRC32 during extraction. Returns `ERAR_BAD_DATA` (error code 12) if CRC32 check fails, mapped to `ArchiveError::Corruption`.
- **libarchive** (TAR/ISO): Automatically verifies checksums during data reading. Returns `ARCHIVE_FAILED` with error message containing "checksum" or "CRC" on failure, detected and mapped to `ArchiveError::Corruption`.
- **Piz** (ZIP read): Verifies CRC32 on read.
- **SevenZ** (7z read): Verifies via sevenz-rust2 internal checks.

**Usage**:
```rust
let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    verify_crc32: true,  // Enabled by default
    ..Default::default()
};

// CRC32 verification happens automatically during extraction
match archive.extract_all(options) {
    Ok(_) => println!("Extraction successful, all files verified"),
    Err(ArchiveError::Corruption { details, .. }) => {
        eprintln!("Archive corrupted: {}", details);
    },
    Err(e) => eprintln!("Other error: {}", e),
}
```

**Limitations**:
- The `validate_integrity()` method performs backend-specific read/test operations (each backend's `test_integrity()` is dispatched), not just metadata-presence checks. However, full CRC32 matching during extraction is more thorough.
- The `verify_crc32` flag documents intent but cannot disable backend-level verification (backend libraries provide no such option).

---

### 3. Multi-Format Support via Multiple Backends

**Status**: Implemented -- Supports RAR, ZIP, 7z, TAR, ISO and related formats

**Supported Formats**:
- RAR 4.x / RAR 5.0 (via UnRAR)
- ZIP (via Piz for ordinary reads, ZipReader for encrypted archives)
- 7z (via SevenZ backend / sevenz-rust2)
- TAR, TAR.GZ, TAR.BZ2, TAR.XZ (via libarchive)
- ISO (via libarchive, read/extract only)

**Not Supported**:
- Standalone .gz, .bz2, .xz are not valid `Archive::open()` targets (AD 0018). These are only supported as TAR compound formats (e.g., `.tar.gz`).

**Backend Architecture**:
- **UnRAR**: RAR/RAR5, full CRC32 support
- **Piz**: ZIP read (fast, memory-mapped)
- **ZipReader**: Encrypted ZIP read (zip2 crate)
- **ZipWriter**: ZIP creation with optional encryption
- **SevenZ**: 7z read via sevenz-rust2
- **libarchive**: TAR variants, ISO read/extract, TAR creation

**CRC32 Support**:
- RAR/RAR5: CRC32 directly exposed by UnRAR SDK
- ZIP: CRC32 from Piz / ZipReader metadata
- 7z: CRC32 from sevenz-rust2
- TAR/ISO: CRC32 computed during `list_files()` by reading and hashing file data (libarchive does not expose stored CRC32 values)

---

### 4. Write/Modify Operations

**Archive Modification**: Implemented (ZIP and 7z only)

Modification uses `Archive::modify()` which returns a mutable `Archive` in Modify mode. Pending changes (add, remove, replace) are tracked and applied by `commit_changes()`.

**Supported modification formats**: ZIP and 7z only (`can_modify()` in `src/format.rs`). RAR, TAR, and ISO are rejected.

**Usage**:
```rust
// Open archive for modification
let mut archive = Archive::modify("archive.zip")?;

// Queue operations
archive.add_entry("new.txt", b"content")?;
archive.remove_entry("old.txt")?;

// Commit all changes
archive.commit_changes()?;
```

**Limitations**:
- `commit_changes()` is implemented but **lossy** (OI-025-001/002): it extracts all entries and rewrites the archive from scratch, dropping original metadata (timestamps, permissions, compression settings) and archive-level settings.
- `ModificationOptions` is defined but some fields (backup creation, metadata preservation) are partial or deferred.
- No in-place entry insertion -- the entire archive is rewritten.
- RAR modification is correctly rejected (read-only format).
- TAR modification is rejected (`can_modify()` returns false for TAR).

**Archive Creation**: Implemented with gaps

The creation API is functional via `Archive::create()` with libarchive and ZipWriter backends:

**Features**:
- Multiple data sources: filesystem, memory, directories
- Compression levels: Store, Fastest, Fast, Normal, Maximum, Ultra
- Format support: ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ
- Recursive directory addition
- Password encryption for ZIP (via ZipWriter)

**Usage**:
```rust
let mut options = CompressionOptions::new(ArchiveFormat::Zip);
options.level = CompressionLevel::Normal;

let mut creator = Archive::create("output.zip", options)?;

creator.add_file_from_path("document.txt")?;
creator.add_file_from_data("readme.txt", b"content")?;
creator.add_directory_recursive("src")?;

creator.finish()?;
```

**RAR Creation** (optional, external):
RAR archive creation is feature-gated behind `external-rar-create` and requires an external WinRAR CLI (`rar.exe`). It is not a baseline capability.

```toml
[dependencies]
unified-archive = { version = "0.1.0", features = ["external-rar-create"] }
```

**RAR creation limitations**:
- Requires Windows + licensed WinRAR installation
- Only supports files from filesystem (not memory data)
- Not pure Rust -- shells out to `rar.exe`

**Creation gaps**:
- Encryption during creation is ZIP-only (via ZipWriter)
- 7z creation encryption is not supported
- `CompressionOptions.progress` callback is invoked per-entry during creation (`total=None`); see AD 0021

---

### 5. Extraction Temporary File Usage

**Issue**: The UnRAR `extract_to_memory()` method currently extracts to a temporary file before reading into memory, rather than extracting directly to memory.

**Impact**:
- Requires disk space for temporary file during memory extraction
- Slightly slower than true in-memory extraction
- May fail if temp directory is full or inaccessible
- Temporary files are cleaned up after extraction

**Scope**: This limitation is specific to the UnRAR backend. Other backends (Piz, SevenZ, libarchive, ZipReader) can extract to memory directly.

**Status**: Working but not optimal. Future enhancement could add RAR callback-based direct memory extraction.

---

### 6. Password-Protected Archives

**Status**: Implemented -- Password detection and handling for RAR, ZIP, and 7z

**Implementation**:
The library supports password-protected archives with automatic encryption detection:

**Features**:
- **Encryption Detection**: `is_encrypted()` checks if any entries require a password
- **Password Handling**: `open_encrypted()` opens archives with password
- **Clear Error Messages**: Wrong or missing passwords return `ArchiveError::Password`
- **Security**: Passwords are never exposed in error messages or logs

**Usage**:
```rust
// Check if archive is encrypted
let archive = Archive::open("archive.rar")?;
if archive.is_encrypted()? {
    println!("Archive requires password");
}

// Open with password (RAR, ZIP, or 7z)
let archive = Archive::open_encrypted("archive.zip", "mypassword")?;
let entries = archive.list_files()?;
archive.extract_all(options)?;
```

**Supported Formats**:
- RAR/RAR5 with encrypted data (via UnRAR with password)
- ZIP (via ZipReader backend for encrypted archives; Piz cannot decrypt)
- 7z (via SevenZ backend with password)

**Limitations**:
- RAR archives with encrypted headers (created with `-hp` flag) cannot be listed without password
- Creation-time encryption is ZIP-only (via ZipWriter)
- Password must be provided before extraction begins (no interactive prompt)

---

### 7. Multi-Part Archive Support

**Status**: Implemented for RAR/RAR5

**Implementation**:
Multi-part (split) archives are automatically supported for RAR/RAR5 formats through the UnRAR library:

**Features**:
- **Automatic Detection**: UnRAR automatically finds and reads continuation volumes
- **Transparent Extraction**: No special API required -- just open the first part
- **Supported Extensions**: .part1.rar/.part2.rar or .rar/.r00/.r01/.r02 etc.

**Usage**:
```rust
// Simply open the first part - UnRAR handles the rest automatically
let archive = Archive::open("archive.part1.rar")?;
archive.extract_all(options)?;  // Automatically uses all parts
```

**Supported Formats**:
- RAR/RAR5 multi-part archives (automatic via UnRAR)

**Limitations**:
- ZIP and 7z split archives are not yet supported (Piz, ZipReader, and SevenZ backends do not handle split volumes)
- All parts must be in the same directory
- Parts must follow standard naming conventions
- Missing parts will cause extraction errors

---

### 8. Parallel Extraction

**Status**: Implemented -- Parallel file extraction with rayon

**Implementation**:
The `extract_filtered()` method (in `src/extraction.rs`) uses parallel extraction for improved performance:

**Features**:
- **Automatic Parallelization**: 4+ files extract in parallel using rayon
- **Thread-Safe**: Each thread gets its own archive handle
- **Efficient**: Sequential for <4 files to avoid overhead
- **Scalable**: Scales to available CPU cores

**Usage**:
```rust
let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    ..Default::default()
};

archive.extract_filtered(|e| e.path.ends_with(".txt"), options)?;
// If 4+ .txt files, extraction happens in parallel
```

**Performance**:
- Sequential for 1-3 files (low overhead)
- Parallel for 4+ files (scales to CPU cores)
- Each thread opens independent archive handle

---

### 9. Streaming Extraction

**Status**: Implemented -- Streaming extraction via `extract_to_stream()`

**Implementation**:
The library supports streaming extraction via the `extract_to_stream()` method, which implements the `Read` trait for processing large files:

**Features**:
- **Progress Tracking**: Built-in progress tracking (bytes_read, total_size, progress%)
- **Standard Trait**: Implements `std::io::Read` for compatibility with Rust ecosystem

**Usage**:
```rust
use std::io::Read;

let archive = Archive::open("large.rar")?;
let mut stream = archive.extract_to_stream("huge_file.bin")?;

let mut buffer = [0u8; 8192];
while let Ok(n) = stream.read(&mut buffer) {
    if n == 0 { break; }
    // Process chunk
}

println!("Read {} of {} bytes ({:.1}%)",
    stream.bytes_read(),
    stream.total_size().unwrap_or(0),
    stream.progress().unwrap_or(0.0) * 100.0
);
```

**Backend-dependent behavior**:
Streaming is not uniform across all backends:
- **libarchive** (TAR/ISO): True bounded-memory streaming. Data is read in chunks without buffering the entire entry.
- **Native backends** (UnRAR, Piz, SevenZ, ZipReader): Currently use `extract_to_memory()` + `Cursor` wrapper. The entire entry is buffered in memory before being presented through the `Read` interface. This means memory usage scales with entry size, not chunk size.

**Limitations**:
- The bounded-memory guarantee (<100MB for multi-GB files) only holds for the libarchive backend
- Native backends provide the streaming API surface but buffer entries fully in memory
- Future enhancement: true streaming via RAR callback API and backend-specific chunked reads

---

### 10. DOS Timestamp Conversion Approximation

**Issue**: The DOS timestamp to SystemTime conversion uses a simplified calculation that doesn't perfectly account for leap years.

**Impact**:
- Modified timestamps may be off by a few hours/days for very old files
- Modern RAR5 uses high-resolution timestamps (unaffected)

**Accuracy**: +/-1 day for files from 1980-2000, more accurate for newer files

**Status**: Low priority -- good enough for most use cases

---

### 11. Platform-Specific wchar_t Handling

**Issue**: The UnRAR FFI bindings use platform-conditional compilation for wchar_t size differences between macOS (4 bytes/UTF-32) and Windows (2 bytes/UTF-16).

**Impact**:
- Windows support not yet tested
- May require adjustments for Windows builds

**Status**: macOS tested, Windows/Linux testing pending

---

## Dead or Partial API Surface

These API elements exist in the codebase but are incomplete or non-functional:

- **`ModificationOptions`**: `create_backup` and `backup_suffix` are honored by `commit_changes()` via `modify_with_options()`; `preserve_metadata` is accepted but currently a no-op (OI-025-002).
- **`CompressionOptions.progress`**: Progress callback is invoked per-entry during creation by both ZIP and libarchive backends (`total=None`); see AD 0021.

---

## Future Work

### Modification Improvements
- Lossless `commit_changes()`: preserve original entry metadata, timestamps, and compression settings during rewrite
- In-place modification for formats that support it

### Streaming Improvements
- True bounded-memory streaming for native backends (UnRAR callback API, chunked SevenZ reads)

### Format Coverage
- ZIP and 7z multi-part (split) archive support
- 7z creation encryption

---

## Reporting Issues

If you encounter a limitation not documented here, please create a new issue with:
- Clear description of the limitation
- Steps to reproduce
- Expected vs actual behavior
- Your environment (OS, Rust version)

---

**Last Updated**: 2026-04-13
**Current Version**: 0.1.0
