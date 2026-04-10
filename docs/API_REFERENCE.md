# API Reference

Complete API reference for unified-archive library.

## Table of Contents

- [Archive](#archive)
- [ArchiveEntry](#archiveentry)
- [ArchiveFormat](#archiveformat)
- [ExtractionOptions](#extractionoptions)
- [ProgressCallback](#progresscallback)
- [ValidationReport](#validationreport)
- [ArchiveError](#archiveerror)
- [StreamingExtractor](#streamingextractor)

---

## Archive

The main entry point for working with archives.

### Opening Archives

#### `Archive::open(path: impl AsRef<Path>) -> Result<Archive>`

Open an archive with automatic format detection.

**Example:**
```rust
use unified_archive::Archive;

let archive = Archive::open("data.zip")?;
```

**Supported formats:** RAR, RAR5, ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, ISO

> **Note:** `Gzip`, `Bzip2`, and `Xz` variants exist in `ArchiveFormat` but currently only work
> as part of TAR compound formats. Standalone `.gz`/`.bz2`/`.xz` files are not yet supported.

**Errors:**
- `ArchiveError::NotFound` - File does not exist
- `ArchiveError::UnsupportedFormat` - Unknown or unsupported format
- `ArchiveError::Format` - Format-specific error

---

#### `Archive::open_encrypted(path: impl AsRef<Path>, password: &str) -> Result<Archive>`

Open a password-protected archive.

**Example:**
```rust
let archive = Archive::open_encrypted("secret.rar", "mypassword")?;
```

**Supported formats:** RAR, RAR5, ZIP, 7z

**Errors:**
- `ArchiveError::Password` - Wrong password or password required
- Other errors same as `open()`

---

### Inspection Methods

#### `Archive::format(&self) -> ArchiveFormat`

Get the detected archive format.

**Example:**
```rust
match archive.format() {
    ArchiveFormat::Zip => println!("ZIP archive"),
    ArchiveFormat::Rar5 => println!("RAR5 archive"),
    _ => println!("Other format"),
}
```

**Returns:** `ArchiveFormat` enum value

---

#### `Archive::list_files(&self) -> Result<&[ArchiveEntry]>`

List all files and directories in the archive. Results are cached after the first call.

**Example:**
```rust
let entries = archive.list_files()?;
for entry in entries {
    println!("{}: {} bytes", entry.path, entry.size.unwrap_or(0));
}
```

**Returns:** Borrowed slice of `ArchiveEntry` structs with metadata (cached)

**Performance Note:** For libarchive-backed formats (TAR), CRC32 is computed during listing by reading file data. ZIP and 7z use central directory metadata.

**Errors:**
- `ArchiveError::Format` - Cannot read archive structure

---

#### `Archive::find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>>`

Find a specific file by path within the archive.

**Example:**
```rust
if let Some(entry) = archive.find_entry("readme.txt")? {
    println!("Found: {} bytes", entry.size.unwrap_or(0));
}
```

**Parameters:**
- `path` - File path within archive (case-sensitive)

**Returns:** `Some(ArchiveEntry)` if found, `None` if not found

---

#### `Archive::is_encrypted(&self) -> Result<bool>`

Check if any files in the archive are encrypted.

**Example:**
```rust
if archive.is_encrypted()? {
    println!("Password required");
}
```

**Returns:** `true` if password protection detected

**Note:** RAR archives with encrypted headers require password to call this method.

---

#### `Archive::validate_integrity(&self) -> Result<ValidationReport>`

Validate CRC32 checksums for all files.

**Example:**
```rust
let report = archive.validate_integrity()?;
println!("Validated {}/{} files",
    report.validated,
    report.total_entries
);

if !report.failed.is_empty() {
    println!("Failed: {:?}", report.failed);
}
```

**Returns:** `ValidationReport` with validation results

**Note:** Verification behavior varies by backend: UnRAR uses native test mode (`RAR_TEST`), ZIP/7z/piz extract entries to memory and verify CRC32, libarchive streams entries and checks read errors.

---

#### `Archive::calculate_archive_crc(&self) -> Result<u32>`

Calculate an archive-level CRC32 by summing all per-file CRC32 values with 32-bit wrapping overflow. This matches the "Archive CRC" shown by 7-Zip.

**Example:**
```rust
let crc = archive.calculate_archive_crc()?;
println!("Archive CRC: {:08X}", crc);
```

**Returns:** `u32` — wrapping sum of all per-file CRC32 values

**Properties:**
- Deterministic: same files always produce the same result
- Order-independent: addition is commutative
- Format-independent: same files in ZIP, 7z, or RAR produce the same CRC
- Returns `0` if no entries have CRC32 values

---

#### `Archive::calculate_manifest_digest(&self) -> Result<String>`

Calculate a content-identity digest from per-entry CRC32 values. Unlike `calculate_archive_crc` (wrapping sum), this sorts individual CRC32 hex strings and hashes the joined result, making it more collision-resistant.

Designed for deduplication: two archives with identical file contents produce the same digest regardless of archive format, compression method, or entry order.

**Algorithm:**
1. Collect CRC32 from each file entry (skip directories, entries without CRC32)
2. Convert each to 8-char hex (big-endian bytes)
3. Sort lexicographically
4. Join with `,`
5. CRC32-hash the joined string
6. Return as 8-char lowercase hex

**Example:**
```rust
let digest = archive.calculate_manifest_digest()?;
if !digest.is_empty() {
    println!("Manifest digest: {}", digest);
}
```

**Returns:** `String` — 8-char hex digest, or empty string if no file entries have CRC32 values

**Properties:**
- Deterministic and order-independent
- Format-independent: same files in ZIP vs 7z produce the same digest
- More collision-resistant than `calculate_archive_crc` (sorted + hashed vs wrapping sum)
- Used by AdvancedDeduplicator for content-identity matching

---

### Extraction Methods

#### `Archive::extract_all(&self, options: ExtractionOptions) -> Result<()>`

Extract all files from the archive.

**Example:**
```rust
use std::path::PathBuf;

let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    preserve_permissions: true,
    preserve_times: true,
    verify_crc32: true,
    ..Default::default()
};

archive.extract_all(options)?;
```

**Parameters:**
- `options` - Extraction configuration (see `ExtractionOptions`)

**Errors:**
- `ArchiveError::Io` - Cannot create destination or write files
- `ArchiveError::Corruption` - CRC32 verification failed
- `ArchiveError::Password` - Password required or wrong password

---

#### `Archive::extract_file(&self, file_path: &str, options: ExtractionOptions) -> Result<()>`

Extract a single file from the archive.

**Example:**
```rust
use unified_archive::ExtractionOptions;
use std::path::PathBuf;

archive.extract_file(
    "document.pdf",
    ExtractionOptions {
        destination: PathBuf::from("./output"),
        ..Default::default()
    }
)?;
```

**Parameters:**
- `file_path` - Path of file within archive
- `options` - Extraction options including destination directory, password, overwrite, etc.

**Errors:**
- `ArchiveError::Format` - File not found in archive
- Other errors same as `extract_all()`

---

#### `Archive::extract_filtered<F>(&self, predicate: F, options: ExtractionOptions) -> Result<()>`

Extract files matching a predicate function.

**Example:**
```rust
// Extract only .txt files
archive.extract_filtered(
    |entry| entry.path.ends_with(".txt"),
    options
)?;

// Extract files larger than 1MB
archive.extract_filtered(
    |entry| entry.size.unwrap_or(0) > 1_000_000,
    options
)?;
```

**Type Parameters:**
- `F: Fn(&ArchiveEntry) -> bool + Send + Sync`

**Parameters:**
- `predicate` - Function returning `true` for files to extract
- `options` - Extraction configuration

**Performance:** Automatically uses parallel extraction for 4+ files

---

#### `Archive::extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>>`

Extract a single file directly to memory.

**Example:**
```rust
let data = archive.extract_to_memory("config.json")?;
let text = String::from_utf8(data)?;
println!("Config: {}", text);
```

**Parameters:**
- `file_path` - Path of file within archive

**Returns:** File contents as byte vector

**Note:** Returns bytes directly in memory. No temporary file is used.

---

#### `Archive::extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor>`

Extract a file as a readable stream for memory-efficient processing.

**Example:**
```rust
use std::io::Read;

let mut stream = archive.extract_to_stream("large.bin")?;
let mut buffer = [0u8; 8192];

while let Ok(n) = stream.read(&mut buffer) {
    if n == 0 { break; }
    // Process chunk
}
```

**Parameters:**
- `file_path` - Path of file within archive

**Returns:** `StreamingExtractor` implementing `std::io::Read`

**Memory Usage:** Processes file in chunks, ~100MB max memory usage

---

## ArchiveEntry

Metadata for a single file or directory within an archive.

### Fields

```rust
pub struct ArchiveEntry {
    /// Full path within archive (UTF-8, forward slashes)
    pub path: String,

    /// File size in bytes (uncompressed), None for directories
    pub size: Option<u64>,

    /// Compressed size in bytes, None if unavailable
    pub compressed_size: Option<u64>,

    /// Last modification time (UTC)
    pub modified: Option<SystemTime>,

    /// CRC32 checksum, None if not available
    pub crc32: Option<u32>,

    /// Entry type (File, Directory, Symlink, HardLink, Other)
    pub entry_type: EntryType,

    /// Unix permissions (e.g., 0o755)
    pub permissions: Option<u32>,

    /// Creation time (UTC)
    pub created: Option<SystemTime>,

    /// Last access time (UTC)
    pub accessed: Option<SystemTime>,

    // NOTE: compression_ratio is a computed method, not a stored field.
    // See ArchiveEntry::compression_ratio() below.

    /// Whether entry is encrypted/password-protected
    pub is_encrypted: bool,

    /// Entry comment (if format supports)
    pub comment: Option<String>,

    /// Platform-specific file attributes
    pub attributes: Option<FileAttributes>,

    /// Sequential entry ID (0-based, assigned during listing)
    pub id: usize,
}
```

### Methods

#### `ArchiveEntry::is_directory(&self) -> bool`

Check if this entry is a directory.

#### `ArchiveEntry::is_file(&self) -> bool`

Check if this entry is a regular file.

#### `ArchiveEntry::compression_ratio(&self) -> Option<f64>`

Compute compression ratio from sizes (compressed_size / size). Returns `None` if either size is missing or original size is zero.

---

## ArchiveFormat

Enumeration of supported archive formats.

```rust
pub enum ArchiveFormat {
    Rar,       // RAR 4.x
    Rar5,      // RAR 5.0+
    Zip,       // ZIP
    SevenZip,  // 7z
    Tar,       // TAR (uncompressed)
    TarGzip,   // TAR + Gzip
    TarBzip2,  // TAR + Bzip2
    TarXz,     // TAR + XZ
    Gzip,      // Gzip — currently only as part of TarGzip
    Bzip2,     // Bzip2 — currently only as part of TarBzip2
    Xz,        // XZ — currently only as part of TarXz
    Iso,       // ISO 9660
}
```

---

## ExtractionOptions

Configuration for extraction operations.

```rust
pub struct ExtractionOptions {
    /// Destination directory for extracted files
    pub destination: PathBuf,

    /// Password for encrypted archives
    pub password: Option<String>,

    /// Overwrite existing files (default: false, fails with error if files exist)
    pub overwrite: bool,

    /// Preserve file permissions (Unix mode bits) - libarchive only
    pub preserve_permissions: bool,

    /// Preserve file timestamps (modified, created, accessed) - libarchive only
    pub preserve_times: bool,

    /// Verify CRC32 during extraction (Piz and SevenZ backends)
    pub verify_crc32: bool,

    /// Resource limits for extraction (zip bomb protection)
    pub limits: ExtractionLimits,

    /// Filter: only extract matching paths
    pub filter: Option<EntryFilter>,

    /// Progress callback for monitoring extraction
    pub progress: Option<Box<dyn ProgressCallback>>,
}
```

### Default Values

```rust
ExtractionOptions {
    destination: PathBuf::from("."),
    password: None,
    overwrite: false,
    preserve_permissions: true,
    preserve_times: true,
    verify_crc32: true,
    limits: ExtractionLimits::default(),
    filter: None,
    progress: None,
}
```

### Field Details

| Field | Description |
|-------|-------------|
| `password` | Password for encrypted archives. Used when reopening archive for extraction. |
| `overwrite` | If `false` (default), extraction fails with an error listing existing files. |
| `verify_crc32` | Enables CRC32 verification. Supported by Piz (ZIP) and SevenZ backends. RAR has built-in verification. |
| `limits` | Zip bomb protection. See `ExtractionLimits` for details. |

### ExtractionLimits

```rust
pub struct ExtractionLimits {
    /// Maximum total uncompressed size (default: 10 GB)
    pub max_total_size: u64,
    /// Maximum single file size (default: 4 GB)
    pub max_file_size: u64,
    /// Maximum compression ratio (default: 100.0)
    pub max_compression_ratio: f64,
    /// Maximum number of entries (default: 100,000)
    pub max_entry_count: usize,
}
```

---

## ProgressCallback

Trait for monitoring extraction progress.

```rust
pub trait ProgressCallback: Send + Sync {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>;
}
```

> **Note:** `total` is `Option<u64>` because some backends (e.g., libarchive streaming)
> cannot determine total size in advance.

### Example Implementation

```rust
struct MyProgress {
    last_percent: u64,
}

impl ProgressCallback for MyProgress {
    fn on_progress(&mut self, current: u64, total: u64) -> ControlFlow<()> {
        let percent = (current * 100) / total;

        if percent != self.last_percent {
            println!("Progress: {}%", percent);
            self.last_percent = percent;
        }

        // Return Continue to keep extracting, or Break to cancel
        if some_cancel_condition {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}
```

---

## ValidationReport

Results of archive integrity validation.

```rust
pub struct ValidationReport {
    /// Total number of entries checked
    pub total_entries: usize,

    /// Number of entries successfully validated (= total_entries - failed.len())
    pub validated: usize,

    /// Paths of entries that failed validation
    pub failed: Vec<String>,
}
```

---

## ArchiveError

Error types for archive operations.

```rust
pub enum ArchiveError {
    /// I/O error during file operations
    Io { operation: String, path: PathBuf, source: std::io::Error },

    /// Archive format error (wrong magic bytes, truncated headers, etc.)
    Format { format: Option<ArchiveFormat>, message: String },

    /// Archive corruption detected (CRC mismatch, bad data)
    Corruption { path: String, details: String },

    /// Password authentication error
    Password { message: String },

    /// Operation not supported for a specific format
    Unsupported { operation: String, format: ArchiveFormat, details: Option<String> },

    /// Codec not available (requires installation)
    CodecUnavailable { codec: String, format: ArchiveFormat, install_instructions: String },

    /// Operation not yet implemented (format-agnostic)
    UnsupportedOperation { operation: String, reason: String },

    /// Invalid path
    InvalidPath { path: String, reason: String },
}
```

### Example Error Handling

```rust
match Archive::open("file.rar") {
    Ok(archive) => { /* use archive */ },
    Err(ArchiveError::Io { path, source, .. }) => {
        eprintln!("I/O error at {}: {}", path.display(), source);
    },
    Err(ArchiveError::Password { message }) => {
        eprintln!("Password required: {}", message);
    },
    Err(ArchiveError::Corruption { details, .. }) => {
        eprintln!("Corrupted: {}", details);
    },
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## StreamingExtractor

Stream-based file extractor implementing `std::io::Read`.

### Traits

Implements:
- `std::io::Read`

### Methods

#### `StreamingExtractor::bytes_read(&self) -> u64`

Get total bytes read so far.

#### `StreamingExtractor::total_size(&self) -> Option<u64>`

Get total file size if known.

#### `StreamingExtractor::progress(&self) -> Option<f64>`

Get extraction progress as fraction (0.0 to 1.0).

**Returns:** `None` if total size unknown

### Example

```rust
use std::io::Read;

let mut stream = archive.extract_to_stream("large.bin")?;

// Read in chunks
let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
loop {
    match stream.read(&mut buffer) {
        Ok(0) => break, // EOF
        Ok(n) => {
            // Process n bytes from buffer
            println!("Read {} bytes ({:.1}% complete)",
                stream.bytes_read(),
                stream.progress().unwrap_or(0.0) * 100.0
            );
        },
        Err(e) => return Err(e.into()),
    }
}
```

---

## Type Aliases

```rust
/// Result type using ArchiveError
pub type Result<T> = std::result::Result<T, ArchiveError>;
```

---

## Re-exports

The library re-exports commonly used types:

```rust
pub use crate::archive::{Archive, ArchiveFormat};
pub use crate::entry::{ArchiveEntry, EntryType};
pub use crate::error::{ArchiveError, Result};
pub use crate::options::{ExtractionOptions, ProgressCallback, ValidationReport};
pub use crate::streaming::StreamingExtractor;
```

---

## Feature Flags

### `rar-support` (default)

Enables RAR/RAR5 support via UnRAR library.

**Disable RAR support:**
```toml
[dependencies]
unified-archive = { version = "0.1.0", default-features = false }
```

---

## Platform-Specific Notes

### macOS
- Requires libarchive: `brew install libarchive`
- UnRAR uses 4-byte wchar_t (UTF-32)

### Linux
- Requires libarchive-dev: `apt-get install libarchive-dev`
- UnRAR uses 4-byte wchar_t (UTF-32)

### Windows
- libarchive bundled automatically
- UnRAR uses 2-byte wchar_t (UTF-16)
- Not yet tested - may require adjustments

---

## Performance Tips

1. **Parallel Extraction**: `extract_filtered()` automatically uses parallel extraction for 4+ files
2. **Streaming**: Use `extract_to_stream()` for large files to minimize memory usage
3. **CRC32 Overhead**: For ZIP/7z, `list_files()` computes CRC32 by reading data - cache results if calling multiple times
4. **Iterator Exhaustion**: For RAR, reopen archive between operations to avoid iterator exhaustion

---

## Common Patterns

### Extract with Progress

```rust
let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    progress: Some(Box::new(|current: u64, total: Option<u64>| {
        if let Some(t) = total {
            print!("\rProgress: {:.1}%", (current as f64 / t as f64) * 100.0);
        }
        ControlFlow::Continue(())
    })),
    ..Default::default()
};

archive.extract_all(options)?;
```

### Selective Extraction by Extension

```rust
let options = ExtractionOptions {
    destination: PathBuf::from("./images"),
    ..Default::default()
};

archive.extract_filtered(
    |entry| {
        entry.path.ends_with(".jpg") ||
        entry.path.ends_with(".png") ||
        entry.path.ends_with(".gif")
    },
    options
)?;
```

### Read Archive Entry to String

```rust
let data = archive.extract_to_memory("readme.txt")?;
let text = String::from_utf8(data)
    .map_err(|_| ArchiveError::Format {
        format: None,
        details: "Invalid UTF-8".to_string()
    })?;

println!("{}", text);
```

---

**Last Updated**: 2025-11-01
**Version**: 0.1.0
