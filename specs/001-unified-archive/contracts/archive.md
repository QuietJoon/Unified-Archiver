# API Contract: Archive Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Purpose**: Define public API for opening, closing, and managing archive handles

## Overview

The `Archive` type is the primary entry point for all archive operations. It provides a unified interface that works identically across all supported formats (7z, RAR, RAR5, ZIP, TAR variants, GZIP, BZIP2, XZ, ISO).

## Public API

### Archive::open

**Purpose**: Open an existing archive for reading (inspection and extraction).

**Signature**:
```rust
impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ArchiveError>
}
```

**Preconditions**:
- `path` exists and is readable
- File is a valid archive in a supported format
- User has read permissions

**Postconditions**:
- Archive format is detected automatically (lazy, on first operation)
- Archive handle is in `Read` mode
- File is opened but not locked (allows concurrent reads)

**Error Conditions**:
- `ArchiveError::Io`: File not found, permission denied, or I/O error
- `ArchiveError::Format`: Unsupported format or corrupted header

**Example**:
```rust
let archive = Archive::open("document.zip")?;
```

**Thread Safety**: Safe to call from multiple threads with different paths.

**Performance**: O(1) - Format detection is lazy.

---

### Archive::open_encrypted

**Purpose**: Open an encrypted archive with password.

**Signature**:
```rust
impl Archive {
    pub fn open_encrypted(
        path: impl AsRef<Path>,
        password: impl AsRef<str>
    ) -> Result<Self, ArchiveError>
}
```

**Preconditions**:
- Same as `Archive::open`
- Archive is encrypted (password required)

**Postconditions**:
- Password is verified on first access to encrypted entry
- Archive handle stores password for subsequent operations

**Error Conditions**:
- Same as `Archive::open`
- `ArchiveError::Password`: Incorrect password (detected on first access)

**Example**:
```rust
let archive = Archive::open_encrypted("secret.7z", "my_password")?;
```

**Thread Safety**: Password stored securely (zeroized on drop).

---

### Archive::create

**Purpose**: Create a new archive for writing.

**Signature**:
```rust
impl Archive {
    pub fn create(
        path: impl AsRef<Path>,
        options: CompressionOptions
    ) -> Result<Self, ArchiveError>
}
```

**Preconditions**:
- Parent directory of `path` exists and is writable
- `options.format` is supported for writing

**Postconditions**:
- Archive file is created (empty)
- Archive handle is in `Write` mode
- Entries can be added via `add_file()`, `add_directory()`

**Error Conditions**:
- `ArchiveError::Io`: Cannot create file, permission denied
- `ArchiveError::Unsupported`: Format does not support writing

**Example**:
```rust
let options = CompressionOptions {
    format: ArchiveFormat::Zip,
    level: CompressionLevel::Normal,
    ..Default::default()
};
let archive = Archive::create("backup.zip", options)?;
```

**Thread Safety**: File is locked exclusively during writing.

**Performance**: O(1) - Archive file created, ready for writes.

---

### Archive::modify

**Purpose**: Open an existing archive for modification (add/remove entries).

**Signature**:
```rust
impl Archive {
    pub fn modify(path: impl AsRef<Path>) -> Result<Self, ArchiveError>
}
```

**Preconditions**:
- Archive exists, is readable and writable
- Format supports modification (ZIP, 7z only - not RAR/TAR)

**Postconditions**:
- Archive handle is in `Modify` mode
- Can add new entries and remove existing entries
- Original entries are preserved until explicitly removed

**Error Conditions**:
- `ArchiveError::Io`: File not found, permission denied
- `ArchiveError::Unsupported`: Format does not support modification (RAR, TAR)

**Example**:
```rust
let mut archive = Archive::modify("existing.zip")?;
archive.add_file("new_file.txt", &contents)?;
archive.close()?;
```

**Thread Safety**: File is locked exclusively during modification.

**Performance**: O(n) - Reads existing entries into memory.

---

### Archive::close

**Purpose**: Explicitly close the archive and flush any pending writes.

**Signature**:
```rust
impl Archive {
    pub fn close(self) -> Result<(), ArchiveError>
}
```

**Preconditions**:
- Archive handle is valid (not already closed)

**Postconditions**:
- All pending writes are flushed
- File handles are closed
- Archive handle is consumed (cannot be used after close)

**Error Conditions**:
- `ArchiveError::Io`: Flush failed (disk full, I/O error)

**Example**:
```rust
let archive = Archive::create("output.zip", options)?;
archive.add_file("file.txt", data)?;
archive.close()?; // Explicit close
```

**Note**: `Drop` implementation automatically closes, but explicit close allows error handling.

**Thread Safety**: Safe (consumes self).

**Performance**: O(1) for read mode, O(n) for write mode (finalizes archive).

---

### Archive::format

**Purpose**: Get the detected archive format.

**Signature**:
```rust
impl Archive {
    pub fn format(&self) -> Result<ArchiveFormat, ArchiveError>
}
```

**Preconditions**:
- Archive is opened

**Postconditions**:
- Format is detected (lazy, on first call)
- Subsequent calls return cached value

**Error Conditions**:
- `ArchiveError::Format`: Cannot detect format (corrupted or unsupported)

**Example**:
```rust
let archive = Archive::open("file.unknown")?;
match archive.format()? {
    ArchiveFormat::Zip => println!("ZIP archive"),
    ArchiveFormat::SevenZip => println!("7z archive"),
    _ => println!("Other format"),
}
```

**Thread Safety**: Uses interior mutability (OnceCell) for caching.

**Performance**: O(1) after first call (cached).

---

### Archive::is_encrypted

**Purpose**: Check if archive requires a password.

**Signature**:
```rust
impl Archive {
    pub fn is_encrypted(&self) -> Result<bool, ArchiveError>
}
```

**Preconditions**:
- Archive is opened

**Postconditions**:
- Returns true if any entry is encrypted

**Error Conditions**:
- `ArchiveError::Format`: Cannot read archive metadata

**Example**:
```rust
let archive = Archive::open("archive.zip")?;
if archive.is_encrypted()? {
    println!("Password required");
}
```

**Thread Safety**: Safe (immutable query).

**Performance**: O(n) - May need to scan entries.

---

### Archive::path

**Purpose**: Get the filesystem path of the archive.

**Signature**:
```rust
impl Archive {
    pub fn path(&self) -> &Path
}
```

**Preconditions**: None

**Postconditions**: Returns reference to archive path.

**Error Conditions**: None (infallible).

**Example**:
```rust
let archive = Archive::open("data.zip")?;
println!("Archive at: {}", archive.path().display());
```

**Thread Safety**: Safe (immutable borrow).

**Performance**: O(1).

## Type Definitions

### ArchiveMode (Internal)

```rust
pub(crate) enum ArchiveMode {
    Read,    // Opened for reading (inspection, extraction)
    Write,   // Opened for writing (creation)
    Modify,  // Opened for modification (add/remove entries)
}
```

## Error Handling

All operations return `Result<T, ArchiveError>` with these possible errors:

| Error Variant | When | Recovery |
|--------------|------|----------|
| `ArchiveError::Io` | File system errors | Check path, permissions |
| `ArchiveError::Format` | Invalid/unsupported format | Verify file integrity |
| `ArchiveError::Password` | Wrong or missing password | Retry with correct password |
| `ArchiveError::Unsupported` | Operation not supported by format | Use different format or operation |

## Unified Interface Guarantee

**Critical Requirement (FR-001)**: All operations work identically regardless of archive format.

```rust
// Same code works for ALL formats
let zip_archive = Archive::open("file.zip")?;
let sevenz_archive = Archive::open("file.7z")?;
let rar_archive = Archive::open("file.rar")?;

// All return the same ArchiveEntry structure
let zip_entries = zip_archive.list_files()?;
let sevenz_entries = sevenz_archive.list_files()?;
let rar_entries = rar_archive.list_files()?;
```

## Thread Safety

- `Archive` is `Send` but not `Sync` (one thread per handle)
- Multiple `Archive` handles to different files can be used concurrently
- Concurrent reads to the same file are safe (via multiple handles)
- Writes are exclusive (file system locking)

## Resource Management

### RAII Pattern

```rust
{
    let archive = Archive::open("file.zip")?;
    // Use archive...
} // Automatically closed via Drop
```

### Explicit Close with Error Handling

```rust
let archive = Archive::create("output.zip", options)?;
archive.add_file("data.txt", contents)?;

// Explicit close to handle flush errors
archive.close().expect("Failed to finalize archive");
```

## Performance Guarantees

| Operation | Target | Requirement ID |
|-----------|--------|---------------|
| `open()` | O(1) | - |
| `create()` | O(1) | - |
| `close()` (read) | O(1) | - |
| `close()` (write) | O(n) | Must finalize |
| `format()` | O(1) after first call | Lazy detection |

## Compatibility Notes

### 7zip-JBinding Equivalents

| 7zip-JBinding | unified-archive | Notes |
|--------------|---------------|-------|
| `SevenZip.openInArchive()` | `Archive::open()` | Same concept, Rust naming |
| `IInArchive` | `Archive` | Single type instead of interface |
| `close()` | `Archive::close()` | Explicit close, also Drop |

### Format-Specific Behavior

#### RAR/RAR5 (Read-Only)
```rust
let archive = Archive::open("file.rar")?; // OK
let archive = Archive::create("new.rar", opts)?; // Error: Unsupported
let archive = Archive::modify("file.rar")?; // Error: Unsupported
```

#### TAR (No Modification)
```rust
let archive = Archive::modify("file.tar")?; // Error: Unsupported
```

## Examples

### Basic Open/Close
```rust
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    let archive = Archive::open("document.zip")?;
    println!("Format: {:?}", archive.format()?);
    archive.close()?;
    Ok(())
}
```

### Create Archive
```rust
use unified_archive::{Archive, CompressionOptions, CompressionLevel, ArchiveFormat};

let options = CompressionOptions {
    format: ArchiveFormat::SevenZip,
    level: CompressionLevel::Maximum,
    ..Default::default()
};

let archive = Archive::create("backup.7z", options)?;
// Add files (see creation.md contract)
archive.close()?;
```

### Encrypted Archive
```rust
let archive = Archive::open_encrypted("secret.7z", "password123")?;
let entries = archive.list_files()?;
archive.close()?;
```

## Contract Testing

### Required Tests

1. **Open valid archive**: Each supported format
2. **Open invalid file**: Expect `ArchiveError::Format`
3. **Open non-existent file**: Expect `ArchiveError::Io`
4. **Create archive**: Each writable format
5. **Modify unsupported format**: Expect `ArchiveError::Unsupported`
6. **Concurrent opens**: Multiple `Archive::open()` to same file succeeds
7. **Close error handling**: Handle `Err` from `close()`
8. **Drop behavior**: Archive auto-closes on scope exit

### Property-Based Tests

```rust
proptest! {
    #[test]
    fn open_close_roundtrip(path: ValidArchivePath) {
        let archive = Archive::open(&path)?;
        assert!(archive.close().is_ok());
    }
}
```

## References

- data-model.md: `Archive` entity definition
- extraction.md: Operations using opened archives
- errors.md: Complete `ArchiveError` documentation
