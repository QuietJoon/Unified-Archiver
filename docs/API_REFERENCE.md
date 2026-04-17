# API Reference

API reference for the unified-archive library. Covers inspection, extraction, creation, modification, SFX detection, streaming, and safety utilities.

## Internal types

Per AD 0001 (Single Archive Facade with Backend Enum), the supported public surface
is the `Archive` facade together with the domain types listed in the table of contents
below. The following items are `pub` only because their parent module is `pub mod`,
not because they are part of the supported API:

- `unified_archive::ffi::UnrarArchive`
- `unified_archive::ffi::LibarchiveArchive`
- `unified_archive::ffi::ZipArchive`
- `unified_archive::ffi::PizArchive`
- `unified_archive::ffi::SevenZArchive`
- `unified_archive::ffi::ZipWriter`
- `ResultWithWarnings::add_warning`
- `security::get_max_mmap_size`

These types are exposed for crate-internal composition and may change without notice.
Use the `Archive` facade for all archive operations; backend selection is automatic.

## Table of Contents

- [Archive](#archive)
  - [Opening Archives](#opening-archives)
  - [Inspection Methods](#inspection-methods)
  - [Extraction Methods](#extraction-methods)
  - [Creation](#creation)
  - [Modification](#modification)
  - [SFX (Self-Extracting Archives)](#sfx-self-extracting-archives)
- [ArchiveEntry](#archiveentry)
- [ArchiveFormat](#archiveformat)
- [ExtractionOptions](#extractionoptions)
- [ProgressCallback](#progresscallback)
- [ValidationReport](#validationreport)
- [ArchiveError](#archiveerror)
- [ArchiveWarning](#archivewarning)
- [StreamingExtractor](#streamingextractor)
- [Type Aliases](#type-aliases)
- [Re-exports](#re-exports)
- [Feature Flags](#feature-flags)
- [Platform-Specific Notes](#platform-specific-notes)
- [Performance Tips](#performance-tips)
- [Common Patterns](#common-patterns)

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
- `ArchiveError::Io` - File does not exist
- `ArchiveError::Format` - Unknown or unsupported format, or format-specific error

---

#### `Archive::open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Archive>`

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

### General Methods

#### `Archive::path(&self) -> &Path`

Get the filesystem path of the archive.

---

#### `Archive::close(self) -> Result<()>`

Consume the archive and release all resources. Called automatically on drop, but explicit close allows error handling.

---

### Inspection Methods

#### `Archive::entry_count(&self) -> Result<usize>`

Get count of entries in archive. Equivalent to `list_files()?.len()`.

---

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

**Performance Note:** CRC32 population varies by backend. See architecture docs for backend-specific details.

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

Validate archive integrity. For formats that provide CRC32 checksums, verification is CRC32-based. For formats or backends that do not expose per-entry CRC32, validation falls back to read-based error detection (e.g., streaming all entries and checking for read errors).

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

> **Contract note:** No formal design contract exists yet for this method. Behavior is based on implementation convention and may be refined in a future spec revision.

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

Calculate a content-identity digest from per-entry CRC32 values. Unlike `calculate_archive_crc` (wrapping sum), this sorts individual CRC32 hex strings and hashes the joined result, providing better collision resistance.

> **Contract note:** No formal design contract exists yet for this method. Behavior is based on implementation convention and may be refined in a future spec revision.

Designed for deduplication: two archives with identical file contents produce the same digest regardless of archive format, compression method, or entry order.

**Algorithm:**
1. For each file entry (directories excluded):
   - If CRC32 is available: convert to 8-char hex (big-endian bytes)
   - If CRC32 is absent: fall back to `path:size` string (e.g., `readme.txt:1234`)
2. Sort all identity strings lexicographically
3. Join with `,`
4. CRC32-hash the joined string
5. Return as 8-char lowercase hex

**Example:**
```rust
let digest = archive.calculate_manifest_digest()?;
if !digest.is_empty() {
    println!("Manifest digest: {}", digest);
}
```

**Returns:** `String` — 8-char hex digest, or empty string if the archive contains no file entries

**Properties:**
- Deterministic and order-independent
- Format-independent: same files in ZIP vs 7z produce the same digest
- Offers better collision resistance than `calculate_archive_crc` (sorted + hashed vs wrapping sum), though still CRC32-based and not cryptographically strong
- Used by AdvancedDeduplicator for content-identity matching

---

#### `Archive::detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)>`

Detect if this archive is part of a multi-part archive set. Returns `(is_multipart, part_files)` where `part_files` is the sorted list of all detected parts.

---

#### `Archive::is_solid(&self) -> Result<bool>`

Check if archive uses solid compression (files compressed together as a single stream). Supported for RAR/RAR5 and 7z; returns `false` for ZIP and TAR.

---

#### `Archive::has_recovery_record(&self) -> Result<bool>`

Check if archive has recovery records for repairing corruption. Currently supported for RAR/RAR5 only; returns `false` for other formats.

---

#### `Archive::recovery_percentage(&self) -> Result<Option<u8>>`

Get recovery record percentage. Returns `Some(percentage)` when recovery records are present (RAR/RAR5), `None` otherwise.

---

#### `Archive::list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>>`

List files, returning an owned `Vec` instead of a borrowed slice. Intended for use in safety-limit checks where the caller needs to own the entries for further processing.

---

#### `Archive::calculate_manifest_summary(&self) -> Result<(String, u64)>`

Calculate a manifest summary for the archive. Returns a tuple of `(manifest_digest, total_uncompressed_size)`.

---

#### `Archive::check_symlinks(&self) -> Result<Vec<ArchiveWarning>>`

Scan the archive for symlink and hard-link entries. Returns a list of `ArchiveWarning` values describing any link entries found. Useful for pre-extraction security auditing.

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
- File not found in archive: the exact error variant is backend-dependent (`ArchiveError::Format` for most backends, `ArchiveError::Io` for some)
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
- `F: Fn(&ArchiveEntry) -> bool + Sync`

**Parameters:**
- `predicate` - Function returning `true` for files to extract
- `options` - Extraction configuration

**Performance:** Currently uses parallel extraction (via rayon) when 4 or more files match and the archive is not solid. This threshold is a heuristic and may change.

---

#### `Archive::extract_files(&self, paths: &[&str], options: ExtractionOptions) -> Result<()>`

Extract multiple files by their paths within the archive.

**Performance:** Currently uses parallel extraction (via rayon) when 4 or more files are selected and the archive is not solid. This threshold is a heuristic and may change in future versions.

---

#### `Archive::extract_by_ids(&self, ids: &[usize], options: ExtractionOptions) -> Result<()>`

Extract multiple files by their sequential entry IDs (0-based, as returned by `list_files()`). Useful when entries have already been looked up by ID rather than path.

**Performance:** Same parallel extraction heuristic as `extract_files()`.

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

**Note:** Most backends extract directly to memory. The UnRAR backend creates a temporary directory, extracts the file to disk, then reads it back into memory.

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

**Memory Usage:** Memory usage depends on backend: libarchive (TAR family) truly streams with bounded memory. ZIP (Piz), ZIP (ZipReader), 7z (SevenZ), and RAR (UnRAR) backends buffer the full entry in memory before wrapping in a `StreamingExtractor`.

---

### Creation

#### `Archive::create(path: impl AsRef<Path>, options: CompressionOptions) -> Result<Archive>`

Create a new archive for writing. The returned handle is in Write mode. Fails if the output file already exists.

**Supported formats:** ZIP (via zip crate), TAR, TAR.GZ, TAR.BZ2, TAR.XZ, 7z (via libarchive).

**Example:**
```rust
use unified_archive::{Archive, ArchiveFormat, CompressionOptions};

let options = CompressionOptions::new(ArchiveFormat::Zip);
let mut archive = Archive::create("output.zip", options)?;
archive.add_file_from_data("hello.txt", b"Hello, world!")?;
archive.finish()?;
# Ok::<(), unified_archive::ArchiveError>(())
```

---

#### `Archive::add_file_from_data(&mut self, path: &str, data: &[u8]) -> Result<()>`

Add a file to the archive from in-memory byte data. Only available in Write mode.

---

#### `Archive::add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()>`

Add a file from a filesystem path. The file is stored with its original filename.

---

#### `Archive::add_file_from_path_as(&mut self, fs_path: impl AsRef<Path>, archive_path: &str) -> Result<()>`

Add a file from a filesystem path with a custom path within the archive.

---

#### `Archive::add_directory(&mut self, path: &str) -> Result<()>`

Add an empty directory entry to the archive. Only available in Write mode.

---

#### `Archive::add_directory_recursive(&mut self, path: impl AsRef<Path>) -> Result<()>`

Add a directory and all its contents recursively to the archive.

---

#### `Archive::finish(self) -> Result<()>`

Finalize and close the archive. Must be called for Write mode archives to flush pending data. No-op for Read mode.

---

#### `CompressionOptions`

```rust
pub struct CompressionOptions {
    pub format: ArchiveFormat,
    pub level: CompressionLevel,
    pub password: Option<String>,
    pub split_size: Option<u64>,
    pub progress: Option<Box<dyn ProgressCallback>>,
}
```

Construct with `CompressionOptions::new(format)` or `CompressionOptions::builder(format)`. Default level is `CompressionLevel::Normal`.

#### `CompressionOptions::builder(format: ArchiveFormat) -> Self`

Alias for `new()`. Creates a `CompressionOptions` with defaults for the given format.

#### `CompressionOptions::format(&self) -> ArchiveFormat`

Get the target archive format.

#### `CompressionLevel`

```rust
pub enum CompressionLevel {
    Store, Fastest, Fast, Normal, Maximum, Ultra,
}
```

---

### Modification

#### `Archive::modify(path: impl AsRef<Path>) -> Result<Archive>`

Open an existing archive for modification. Changes are tracked in memory and applied when `commit_changes()` is called. Uses a copy-on-write strategy internally.

**Supported formats:** ZIP, 7z. RAR is read-only; TAR is not yet supported.

---

#### `Archive::modify_with_options(path: impl AsRef<Path>, options: ModificationOptions) -> Result<Archive>`

Open an existing archive for modification with explicit options. Same as `modify()` but allows configuring backup behavior and metadata preservation up front.

---

#### `Archive::add_entry(&mut self, path: &str, data: &[u8]) -> Result<()>`

Track a new file entry for addition. Only available in Modify mode (use `add_file_from_data()` in Write mode).

---

#### `Archive::remove_entry(&mut self, path: &str) -> Result<()>`

Mark an entry for removal. Applied when `commit_changes()` is called.

---

#### `Archive::replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()>`

Convenience method: removes the old entry and adds a new one with the same path and new data.

---

#### `Archive::add_directory_entry(&mut self, path: &str) -> Result<()>`

Track a new empty directory entry for addition. Only available in Modify mode.

---

#### `Archive::pending_operations(&self) -> usize`

Get the number of pending modification operations (additions, removals, replacements) that will be applied on `commit_changes()`.

---

#### `Archive::clear_operations(&mut self)`

Discard all pending modification operations without committing them.

---

#### `Archive::commit_changes(self) -> Result<()>`

Apply all tracked modifications (additions, removals, replacements). Creates a new archive, copies non-removed entries, adds new entries, then replaces the original file.

---

#### `ModificationOptions`

```rust
pub struct ModificationOptions {
    pub preserve_metadata: bool,
    pub create_backup: bool,
    pub backup_suffix: String,
    pub compression: Option<CompressionOptions>,
}
```

#### `ModificationOptions::new() -> Self`

Create default modification options (no backup, metadata preservation enabled, default compression).

#### `ModificationOptions::with_backup(self, suffix: &str) -> Self`

Enable backup creation before committing changes. The original archive is copied to `<path>.<suffix>` before the atomic rename.

#### `ModificationOptions::without_metadata_preservation(self) -> Self`

Disable metadata preservation during the copy phase. When enabled (default), `commit_changes()` preserves timestamps and Unix permissions on retained entries.

#### `compression: Option<CompressionOptions>`

Override compression settings for the recreated archive. When `None` (default), uses format defaults (`CompressionLevel::Normal`, no password). Set this to preserve the original archive's compression level or password across modifications.

---

### SFX (Self-Extracting Archives)

#### `Archive::detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>`

Detect if a file is a self-extracting archive. Uses 3-stage detection: executable validation, signature scan, archive validation. Scans first 1MB only.

**Performance:** Typical detection 15-70ms; non-executable files <1ms (early exit).

---

#### `Archive::open_sfx(path: impl AsRef<Path>) -> Result<Archive>`

Convenience method: detect SFX and open the embedded archive in one call. Equivalent to `detect_sfx()` + `open_at_offset()`.

> **Status:** Because `open_at_offset()` is not yet implemented, positive detections will fail with `ArchiveError::Unsupported`. Use `detect_sfx()` for detection-only workflows.

---

#### `Archive::open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Archive>`

Open an archive that starts at a specific byte offset within a file. Primarily for SFX archives. **Status:** not yet implemented; returns `ArchiveError::Unsupported`.

---

#### `Archive::extract_stub(path: impl AsRef<Path>, detection: &SfxDetectionResult) -> Result<Vec<u8>>`

Extract the executable stub from an SFX archive for security analysis. Maximum stub size: 50MB.

---

#### `SfxDetectionResult`

```rust
pub struct SfxDetectionResult {
    pub is_sfx: bool,
    pub archive_format: Option<ArchiveFormat>,
    pub data_offset: Option<u64>,
    pub stub_type: Option<StubType>,
    pub confidence: f32, // 0.0-1.0
}
```

Helper methods: `not_sfx()`, `detected(stub_type, format, offset)`, `probable(...)`, `is_confirmed()`, `summary()`.

---

#### `StubType`

```rust
pub enum StubType {
    WindowsPE,        // Windows PE (.exe)
    LinuxELF,         // Linux/BSD ELF
    MacOSMachO,       // macOS Mach-O
    ScriptInterpreter, // Shebang (#!) scripts
    Unknown,
}
```

##### Methods

**`StubType::detect(bytes: &[u8]) -> Result<StubType>`** -- Classify executable format from the leading bytes of a file. Returns `StubType::Unknown` (not an error) when the bytes don't match any recognized format, allowing callers to proceed with heuristic signature scanning.

**`StubType::description(&self) -> &'static str`** -- Human-readable description (e.g., `"Windows PE executable"`).

**`StubType::is_native(&self) -> bool`** -- `true` if this stub type typically runs on the current platform.

**`StubType::is_known(&self) -> bool`** -- `true` for any variant except `Unknown`.

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

#### `ArchiveEntry::new(path: String, id: usize) -> Self`

Create a new `ArchiveEntry` with the given path and sequential ID. All optional fields default to `None`/`false`.

#### `ArchiveEntry::is_directory(&self) -> bool`

Check if this entry is a directory.

#### `ArchiveEntry::is_file(&self) -> bool`

Check if this entry is a regular file.

#### `ArchiveEntry::is_symlink(&self) -> bool`

Check if this entry is a symbolic link.

#### `ArchiveEntry::is_hardlink(&self) -> bool`

Check if this entry is a hard link.

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

### Methods

#### `ArchiveFormat::detect(path: &Path) -> Result<ArchiveFormat>`

Detect the archive format of a file by reading its magic bytes.

#### `ArchiveFormat::detect_from_bytes(magic: &[u8]) -> Result<ArchiveFormat>`

Detect the archive format from a byte slice of magic bytes (header data).

#### `ArchiveFormat::supports_compression(&self) -> bool`

Whether this format supports configurable compression levels.

#### `ArchiveFormat::supports_encryption(&self) -> bool`

Whether this format supports password-based encryption.

#### `ArchiveFormat::supports_multipart(&self) -> bool`

Whether this format supports multi-part (split) archives.

#### `ArchiveFormat::can_modify(&self) -> bool`

Whether this format supports in-place modification (add/remove entries).

#### `ArchiveFormat::extensions(&self) -> &[&str]`

Get common file extensions for this format.

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

    /// Preserve file permissions (Unix mode bits) — where supported by backend
    pub preserve_permissions: bool,

    /// Preserve file timestamps (modified, created, accessed) — where supported by backend
    pub preserve_times: bool,

    /// Verify CRC32 during extraction — supported by backends that expose per-entry CRC32
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
| `verify_crc32` | Enables CRC32 verification where supported by the backend. RAR performs built-in verification regardless of this flag. |
| `limits` | Zip bomb protection. See `ExtractionLimits` for details. |

### ExtractionLimits

```rust
pub struct ExtractionLimits {
    /// Maximum total uncompressed size (default: 10 GB)
    pub max_total_size: u64,
    /// Maximum single file size (default: 1 GB = 1,073,741,824 bytes)
    pub max_file_size: u64,
    /// Maximum compression ratio (default: 1000.0)
    pub max_compression_ratio: f64,
    /// Maximum number of entries (default: 100,000)
    pub max_entry_count: usize,
    /// Maximum mmap size. None uses platform default (4 GB on 64-bit, 100 MB on 32-bit).
    pub max_mmap_size: Option<u64>,
}
```

### Methods

#### `ExtractionLimits::unlimited() -> Self`

Create limits with all thresholds set to their maximum values. Use with caution -- disables all zip-bomb protection.

#### `ExtractionLimits::with_max_mmap_size(self, size: u64) -> Self`

Builder-style method that sets the maximum memory-map size for backends (such as Piz) that memory-map the archive file. Returns `self` for chaining.

---

## ProgressCallback

Trait for monitoring extraction progress.

```rust
pub trait ProgressCallback: Send + Sync {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>;
}
```

> **Note:** `total` is `Option<u64>` because some backends (e.g., libarchive streaming)
> cannot determine total size in advance. See `specs/001-unified-archive/contracts/progress.md` for the full progress reporting contract.

### Example Implementation

```rust
struct MyProgress {
    last_percent: u64,
}

impl ProgressCallback for MyProgress {
    fn on_progress(&mut self, current: u64, total: Option<u64>) -> ControlFlow<()> {
        if let Some(total) = total {
            let percent = (current * 100) / total;

            if percent != self.last_percent {
                println!("Progress: {}%", percent);
                self.last_percent = percent;
            }
        } else {
            // Total unknown (e.g., libarchive streaming) — show bytes only
            println!("Processed: {} bytes", current);
        }

        // Return Continue to keep extracting, or Break to cancel
        // Example: cancel after processing 50MB
        if current > 50 * 1024 * 1024 {
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

    /// Number of entries that passed validation. This is the count of entries
    /// whose integrity check succeeded (CRC32 match or error-free read).
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

    /// Operation invalid in the current mode or context (format-agnostic)
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

### Convenience Constructors

Helper methods for constructing `ArchiveError` variants with ergonomic signatures.

#### `ArchiveError::format(format: Option<ArchiveFormat>, message: impl Into<String>) -> Self`

Create a `Format` error.

#### `ArchiveError::io(operation: impl Into<String>, path: impl Into<PathBuf>, source: std::io::Error) -> Self`

Create an `Io` error with structured context.

#### `ArchiveError::corruption(path: impl Into<String>, details: impl Into<String>) -> Self`

Create a `Corruption` error.

#### `ArchiveError::password(message: impl Into<String>) -> Self`

Create a `Password` error.

#### `ArchiveError::invalid_path(path: impl Into<String>, reason: impl Into<String>) -> Self`

Create an `InvalidPath` error.

#### `ArchiveError::unsupported(operation: impl Into<String>, format: ArchiveFormat, details: Option<impl Into<String>>) -> Self`

Create an `Unsupported` error for format-specific unsupported operations.

#### `ArchiveError::codec_unavailable(codec: impl Into<String>, format: ArchiveFormat) -> Self`

Create a `CodecUnavailable` error. Automatically embeds platform-specific installation instructions (macOS/Linux/Windows) for the requested codec.

#### `ArchiveError::write_mode_only(operation: impl Into<String>) -> Self`

Create an `UnsupportedOperation` error for attempts to read from a write-only archive.

#### `ArchiveError::read_only_backend(operation: impl Into<String>) -> Self`

Create an `UnsupportedOperation` error for read-only backends that do not support creation.

---

## ResultWithWarnings\<T\>

Result type that carries both a value and non-fatal warnings emitted during an operation.

```rust
pub struct ResultWithWarnings<T> {
    /// Operation result
    pub value: T,
    /// Warnings emitted during operation
    pub warnings: Vec<ArchiveWarning>,
}
```

### Methods

#### `ResultWithWarnings::ok(value: T) -> Self`

Create a result with no warnings.

#### `ResultWithWarnings::with_warnings(value: T, warnings: Vec<ArchiveWarning>) -> Self`

Create a result carrying one or more warnings.

#### `ResultWithWarnings::add_warning(&mut self, warning: ArchiveWarning)`

Append a warning to an existing result.

---

## ArchiveWarning

Warning type emitted during archive operations. Indicates non-fatal conditions that may require user attention (e.g., skipped symlinks or hard links).

```rust
pub enum ArchiveWarning {
    /// Symbolic link skipped during operation
    SkippedSymlink { path: String, target: Option<String> },
    /// Hard link skipped during operation
    SkippedHardLink { path: String },
}
```

Returned by `Archive::check_symlinks()` for pre-extraction security auditing.

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
            if let Some(pct) = stream.progress() {
                println!("Read {} bytes ({:.1}% complete)",
                    stream.bytes_read(),
                    pct * 100.0
                );
            } else {
                println!("Read {} bytes (total size unknown)",
                    stream.bytes_read()
                );
            }
        },
        Err(e) => return Err(e.into()),
    }
}
```

---

## Security Helpers

### `check_extraction_safe(entries: &[ArchiveEntry], limits: &ExtractionLimits) -> Result<()>`

Pre-extraction safety gate. Validates all entries against the provided limits before any I/O occurs. Checks entry count, individual file sizes, total uncompressed size, and compression ratios. Returns an error describing the first exceeded limit.

### `sanitize_entry_path(entry_path: &str, dest: &Path) -> Result<PathBuf>`

Sanitize an archive entry path to prevent path traversal (Zip Slip) attacks. Strips absolute prefixes, `..` components, and `.` components, then joins the result under `dest`. Returns the safe resolved path.

### `validate_entry_path(entry_path: &str, dest: &Path) -> Result<PathBuf>`

Pure validation of an entry path without filesystem side-effects. Like `sanitize_entry_path` but does not create directories. Use for conflict checking before extraction begins.

### `verify_crc32(data: &[u8], expected_crc: Option<u32>, file_path: &str) -> Result<()>`

Verify CRC32 of extracted data against an expected value. When `expected_crc` is `None`, the check succeeds unconditionally (no CRC to verify). Returns a `Corruption` error on mismatch.

### `get_max_mmap_size() -> u64`

Return the maximum memory-map size used by the Piz ZIP backend. Reads `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` from the environment if set, otherwise returns the compile-time default.

---

## Stream Checksum Utilities

### `StreamChecksum`

```rust
pub struct StreamChecksum {
    /// CRC32 value from the stream (if available)
    pub crc32: Option<u32>,
    /// CRC64 value from the stream (if available, XZ only)
    pub crc64: Option<u64>,
    /// Uncompressed size from the stream metadata
    pub uncompressed_size: Option<u64>,
    /// Type of checksum found
    pub check_type: CheckType,
}
```

### `CheckType`

```rust
pub enum CheckType {
    None,     // No checksum present
    Crc32,    // CRC32 checksum
    Crc64,    // CRC64 checksum
    Sha256,   // SHA-256 checksum
    Unknown,  // Unknown or unsupported checksum type
}
```

### `extract_stream_checksum(path: impl AsRef<Path>) -> Result<StreamChecksum>`

Auto-detect format via magic bytes and extract the stream-level checksum. Supports GZIP, BZIP2, and XZ. Prefers magic-byte detection over file extension to handle mislabeled files.

### `extract_gzip_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum>`

Extract CRC32 and uncompressed size from a GZIP file's 8-byte trailer (RFC 1952).

### `extract_bzip2_stream_crc(path: impl AsRef<Path>) -> Result<StreamChecksum>`

Extract the stream CRC32 from a BZIP2 file's end-of-stream marker. Scans the final 1 KB for the EOS magic.

### `extract_xz_stream_check(path: impl AsRef<Path>) -> Result<StreamChecksum>`

Extract the check type from an XZ stream header (bytes 6-7). Reports the check algorithm (None, CRC32, CRC64, SHA-256) without parsing the full check value.

---

## Type Aliases

```rust
/// Result type using ArchiveError
pub type Result<T> = std::result::Result<T, ArchiveError>;
```

---

## Re-exports

The library re-exports commonly used types at the crate root:

```rust
// Core types
pub use crate::archive::Archive;
pub use crate::format::ArchiveFormat;
pub use crate::entry::{ArchiveEntry, EntryType, FileAttributes};
pub use crate::error::{ArchiveError, Result};

// Options and callbacks
pub use crate::options::{
    CompressionLevel, CompressionOptions, EntryFilter,
    ExtractionOptions, ProgressCallback,
};
pub use crate::modification::ModificationOptions;

// Security and limits
pub use crate::security::{
    ExtractionLimits, check_extraction_safe, sanitize_entry_path,
    validate_entry_path, verify_crc32,
};

// Inspection and streaming
pub use crate::inspection::ValidationReport;
pub use crate::streaming::StreamingExtractor;

// SFX detection
pub use crate::sfx::{SfxDetectionResult, StubType};

// Stream-level checksum utilities
pub use crate::stream_crc::{
    CheckType, StreamChecksum, extract_bzip2_stream_crc,
    extract_gzip_stream_crc, extract_stream_checksum,
    extract_xz_stream_check,
};
```

### Backend Modules (Advanced)

The `ffi` module is public and exposes per-backend wrapper types (`UnrarArchive`, `LibarchiveArchive`, `PizArchive`, `SevenZArchive`, `ZipArchive`, `ZipWriter`) along with low-level binding modules (`ffi::unrar`, `ffi::libarchive`). These types are **not** re-exported at the crate root and are considered implementation details. Most consumers should use the high-level `Archive` facade documented above. The backend types are available for advanced use cases that require direct backend access (e.g., accessing backend-specific metadata not exposed through the unified API). Refer to the rustdoc on each backend module for method signatures and usage.

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
- Not yet tested on Windows; macOS is the primary platform, Linux secondary
- UnRAR uses 2-byte wchar_t (UTF-16)
- May require adjustments for libarchive linkage and path handling

---

## Performance Tips

1. **Parallel Extraction**: `extract_filtered()`, `extract_files()`, and `extract_by_ids()` currently use parallel extraction (rayon) for 4+ non-solid files. This threshold is a heuristic and may change.
2. **Streaming**: Use `extract_to_stream()` for large files to minimize memory usage (bounded-memory streaming is libarchive-only; other backends buffer the full entry in memory)
3. **Listing Cache**: `list_files()` results are cached after the first call; subsequent calls return the cached slice at no cost
4. **RAR Iterator Exhaustion**: The RAR backend may need to reopen the archive when switching between listing and extraction, or when extracting individual files in sequence. This is handled internally; no caller action is needed in most cases

---

## Common Patterns

### Extract with Progress

```rust
use unified_archive::ExtractionOptions;
use std::path::PathBuf;
use std::ops::ControlFlow;

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
        message: "Invalid UTF-8".to_string()
    })?;

println!("{}", text);
```

---

**Last Updated**: 2026-04-13
**Version**: 0.1.0
