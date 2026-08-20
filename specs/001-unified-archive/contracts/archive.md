# API Contract: Archive Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Status**: Implemented (retrospective documentation)
**Purpose**: Define public API for opening, closing, and managing archive handles

> **Post-v0.1.0 reality check (2026-04-18):** Contract text below has drifted from the shipped 0.1.0 crate. Canonical behavior is in `src/archive.rs`, `src/error.rs`, `docs/API_REFERENCE.md`. Known deltas: (1) `Archive::open_at_offset()` / `Archive::open_sfx()` are **shipped** (temp-file-backed), not deferred; (2) `ArchiveError` has no `UnsupportedOperation` variant — use `WriteModeOnly` / `ReadOnlyBackend` / `NotImplemented` / `OperationBlocked`; (3) standalone `.gz` / `.bz2` / `.xz` are openable via `Archive::open()` per MADR-0019 (only stream *creation* is out of scope per AD 0018).

## Overview

The `Archive` type is the primary entry point for all archive operations. It provides a common API surface with documented format caveats across all supported formats (7z, RAR, RAR5, ZIP, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, ISO). Not every operation is available on every format — see the Format-Specific Behavior section for capability differences. Standalone GZIP, BZIP2, and XZ are not supported through `Archive::open()` — only their TAR compound variants are in scope (AD 0018).

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
- Archive format is detected automatically (eager format detection at open time)
- Archive handle is in `Read` mode
- Caller owns the `Archive` handle; underlying resource management varies by backend (some hold file handles, others reopen per operation)

**Error Conditions**:
- `ArchiveError::Io`: File not found, permission denied, or I/O error
- `ArchiveError::Format`: Unsupported format or corrupted header

**Example**:
```rust
let archive = Archive::open("document.zip")?;
```

**Thread Safety**: **RAR**: the UnRAR backend uses process-global mutable state. The library serializes all UnRAR FFI calls behind a process-wide mutex (see AD 0019), so concurrent `open()`/`list`/extract calls on RAR archives are safe across threads from the caller's perspective — they will execute serially under the hood. For non-RAR formats, `open()` is safe to call from multiple threads with different paths and runs concurrently.

**Performance**: Eager format detection at open time.

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

**Thread Safety**: Same as `Archive::open()`. The handle is `Send` but not `Sync`; the stored password does not introduce additional concurrency constraints. See the RAR caveat in `open()`.

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
- `path` must not refer to an existing file (AD 0017: `create()` rejects existing files to prevent silent data loss)
- `options.format` is supported for writing

**Postconditions**:
- Archive file is created (empty)
- Archive handle is in `Write` mode
- Entries can be added via `add_file_from_data()`, `add_file_from_path()`, `add_file_from_path_as()`, `add_directory()`, `add_directory_recursive()`

**Error Conditions**:
- `ArchiveError::Io` with `ErrorKind::AlreadyExists`: Target path already exists (per AD 0017 — callers must delete or choose a different path explicitly)
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

**Thread Safety**: Intended single-writer semantics — only one `Archive` handle should write to a given path at a time. The library does not currently enforce OS-level file locking; callers are responsible for external coordination if multiple processes may target the same file.

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

**Reliability Caveats (OI-025-001, OI-025-002)**:
Modification uses a copy-on-write rewrite strategy — `commit_changes()` writes a new temporary archive alongside the original, then replaces it atomically. This means:
- **Metadata loss**: file-level metadata (extended attributes, NTFS timestamps beyond mtime, format-specific extra fields) may not survive the rewrite. Only standard name/size/mtime/permissions are preserved.
- **Settings loss**: archive-level settings (solid block size, compression dictionary, encryption headers) are rebuilt from `CompressionOptions` defaults. `commit_changes()` takes no options parameter — the original archive's settings are not carried forward automatically.
- **Disk usage**: temporary disk space equal to the full archive size is required during `commit_changes()`.

**Error Conditions**:
- `ArchiveError::Io`: File not found, permission denied, or insufficient disk space for temp archive
- `ArchiveError::Unsupported`: Format does not support modification (RAR, TAR)

**Example**:
```rust
let mut archive = Archive::modify("existing.zip")?;
let contents = b"hello world";
let new_contents = b"updated content";
archive.add_entry("new_file.txt", contents)?;
archive.remove_entry("obsolete.txt")?;
archive.replace_entry("updated.txt", new_contents)?;
archive.commit_changes()?;  // Consumes self; no close() needed after this
```

**Thread Safety**: File is locked exclusively during modification.

**Performance**: O(n) where n is total archive size — `modify()` reads existing entries and `commit_changes()` performs a full copy-on-write rewrite to a temporary archive on the same filesystem, then atomically replaces the original. Expect temporary disk usage equal to the archive size and write amplification proportional to unchanged entries.

---

### Archive::modify_with_options

**Purpose**: Open an existing archive for modification with explicit options. Additive sibling of `Archive::modify(path)` — see AD 0020.

**Signature**:
```rust
impl Archive {
    pub fn modify_with_options(
        path: impl AsRef<Path>,
        options: ModificationOptions,
    ) -> Result<Self, ArchiveError>
}
```

**Preconditions**:
- Same as `Archive::modify(path)`.

**Postconditions**:
- Same as `Archive::modify(path)`.
- The supplied `ModificationOptions` are stored on the handle and applied during `commit_changes()`.

**Honored fields** (current MVP):

| Field | Honored? | Behavior |
|---|---|---|
| `create_backup: bool` | Yes | When `true`, `commit_changes()` copies the original archive to `<path>.<normalized_suffix>` *after* the temp file is fully written but *before* the atomic rename. No-op commits (no pending modifications) skip the backup. |
| `backup_suffix: String` | Yes | Suffix is normalized: empty → `.bak`; `"bak"` → `.bak`; `".bak"` → `.bak`. |
| `preserve_metadata: bool` | **Accepted but no-op (Phase C.2 / OI-025-002)** | The full per-entry metadata pipeline (mtime, permissions, comment, extra attributes) is scheduled for Phase C.2. Setting this flag today does not produce an error and does not yet preserve metadata beyond what `Archive::modify()` already preserves. |

**Error Conditions**:
- Same as `Archive::modify(path)`.
- `ArchiveError::Io` (during `commit_changes()`): backup copy failed (disk full, permission denied, etc.). The atomic rename is **not** performed if backup is enabled and the backup copy fails — the original archive is left untouched.

**Example**:
```rust
use unified_archive::{Archive, ModificationOptions};

let opts = ModificationOptions::default().with_backup(".bak");
let mut archive = Archive::modify_with_options("existing.zip", opts)?;
archive.add_entry("new_file.txt", b"hello")?;
archive.commit_changes()?;
// existing.zip.bak now contains the pre-commit archive contents
// existing.zip contains the new entry
```

**Thread Safety**: Same as `Archive::modify(path)`.

**Performance**: Backup is a `std::fs::copy` of the original archive size — adds O(n) sequential I/O on top of the copy-on-write rewrite cost. Skipped when no modifications are pending.

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
let mut archive = Archive::create("output.zip", options)?;
archive.add_file_from_data("file.txt", b"hello world")?;
archive.close()?; // Explicit close
```

**Finalization Guidance**: The recommended finalization path depends on the archive mode:
- **Read mode**: `close()` consumes the handle and releases any backend resources. Dropping without `close()` is also safe — `Drop` releases resources silently.
- **Write mode**: call `finish()` to finalize the archive (writes central directory / footer). `close()` delegates to `finish()` internally, so either method works, but `finish()` is the canonical spelling for write-mode archives. Prefer `finish()` for clarity.
- **Modify mode**: call `commit_changes()` which consumes `self` and performs the copy-on-write rewrite. Do not call `close()` after `commit_changes()` — the handle is already consumed.

In all modes, `Drop` provides a safety net but cannot report errors. Prefer explicit finalization when error handling matters.

**Thread Safety**: Safe (consumes self).

**Performance**: Read mode releases resources immediately. Write mode must finalize the archive (write central directory / footer), with cost proportional to the number of entries.

---

### Archive::format

**Purpose**: Get the detected archive format.

**Signature**:
```rust
impl Archive {
    pub fn format(&self) -> ArchiveFormat
}
```

**Preconditions**:
- Archive is opened

**Postconditions**:
- Returns the format detected at open time (eager detection)

**Error Conditions**: None (infallible). Format is determined at `open()` time.

**Example**:
```rust
let archive = Archive::open("file.zip")?;
match archive.format() {
    ArchiveFormat::Zip => println!("ZIP archive"),
    ArchiveFormat::SevenZip => println!("7z archive"),
    _ => println!("Other format"),
}
```

**Thread Safety**: Safe (immutable query).

**Performance**: O(1) - returns stored value.

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

### SFX Entry Points

SFX (Self-Extracting Archive) support provides detection and stub extraction for executables that contain embedded archives. The detection pipeline uses a 3-stage approach: PE/ELF header analysis, magic-byte scanning, and format-specific validation.

The following SFX-related methods are also part of the public `Archive` API:

- `Archive::detect_sfx(path) -> Result<SfxDetectionResult>` — 3-stage SFX detection pipeline (PE/ELF header, magic-byte scan, format validation)
- `Archive::open_sfx(path) -> Result<Archive>` — Convenience: detect SFX then open. **Partial**: detection works but offset opening is deferred, so positive detections currently fail.
- `Archive::open_at_offset(path, offset) -> Result<Archive>` — **Deferred**: returns `ArchiveError::Unsupported`
- `Archive::extract_stub(path, result) -> Result<Vec<u8>>` — Extract executable prefix bytes before the embedded archive

Full SFX contract details (detection stages, result types, confidence levels) are documented in source: `src/sfx/detection.rs` and `src/sfx/result.rs`.

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

All operations return `Result<T, ArchiveError>` with these possible errors. Note: the mapping from backend-specific errors to these variants is best-effort — different backends may surface the same underlying condition through different variants (e.g., a corrupt RAR may return `Format` where a corrupt ZIP returns `Corruption`). Treat the table below as the intended mapping; consult the `ArchiveError` source and backend adapter code for exact behavior.

| Error Variant | When | Recovery |
|--------------|------|----------|
| `ArchiveError::Io` | File system errors | Check path, permissions |
| `ArchiveError::Format` | Invalid/unsupported format | Verify file integrity |
| `ArchiveError::Corruption` | Data integrity failure | Re-acquire archive |
| `ArchiveError::Password` | Wrong or missing password | Retry with correct password |
| `ArchiveError::Unsupported` | Operation not supported by format | Use different format or operation |
| `ArchiveError::Io` (`ErrorKind::AlreadyExists`) | Target path already exists (create) | Delete or choose a new path |
| `ArchiveError::CodecUnavailable` | Required codec not installed | Install codec per `install_instructions` |
| `ArchiveError::UnsupportedOperation` | Operation invalid in current context | Check archive mode/state |
| `ArchiveError::InvalidPath` | Path traversal or invalid entry path | Use safe paths |

## Unified Interface Guarantee

**Critical Requirement (FR-001)**: All operations share a common API surface regardless of archive format. Capability varies by format: creation is ZIP/7z/TAR only, modification is copy-on-write (ZIP/7z), RAR is read-only, SFX offset opening is deferred, and streaming memory behavior is backend-dependent. See the Format-Specific Behavior section below for details.

```rust
// The same API works for all supported formats, subject to per-format capability differences
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
- Writes are exclusive (intended single-writer semantics; see `create()` note)
- **RAR**: the UnRAR backend relies on process-global mutable state. The library now serializes all UnRAR FFI calls behind a process-wide mutex (`UNRAR_LOCK`, see AD 0019), so callers can safely use RAR archives from multiple threads — RAR work runs serially under the hood, while non-RAR formats remain fully concurrent.

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
let mut archive = Archive::create("output.zip", options)?;
archive.add_file_from_data("data.txt", contents)?;

// Explicit close to handle flush errors
archive.close().expect("Failed to finalize archive");
```

## Performance Notes

> **Non-normative.** The figures below describe intended implementation characteristics, not guaranteed SLAs. Actual performance depends on the backend library, compression codec, and I/O subsystem. They are provided to help callers reason about resource planning, not as binding contracts.

| Operation | Characteristic | Notes |
|-----------|---------------|-------|
| `open()` | Eager format detection; reads header bytes | I/O-bound; latency depends on storage |
| `create()` | Constant-time handle creation | File is created on disk but no archive content is written yet |
| `close()` (read) | Constant-time handle release | Releases file descriptor only |
| `close()` / `finish()` (write) | Proportional to entry count | Writes central directory / footer; I/O-bound |
| `commit_changes()` (modify) | Proportional to full archive size | Copy-on-write rewrite; requires temp disk equal to archive size |
| `format()` | Constant-time | Returns value cached at open time |

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
// Note: optional feature-gated WinRAR CLI creation exists behind `external-rar-create` flag
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
    println!("Format: {:?}", archive.format());
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

let mut archive = Archive::create("backup.7z", options)?;
// Add files (see `CompressionOptions` in `src/options.rs` for creation configuration)
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
