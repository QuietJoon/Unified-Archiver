# API Contract: Extraction Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-31 (Updated for Phase 1 enhancements)
**Status**: Implemented (retrospective documentation)
**Purpose**: Extract files from archives with unified interface across all formats

## Core Operations

### Archive::extract_all

```rust
impl Archive {
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<(), ArchiveError>
}
```

**Unified Interface**: Shared method name (`extract_all`) with format-dependent behavior — each backend delegates to its native library while presenting the same call shape to the caller.

**Preconditions**:
- Archive is opened for reading
- `options.destination` exists or can be created
- Password provided if archive is encrypted
- No existing files at destination paths (unless `options.overwrite` is true)

**Postconditions**:
- All files extracted to destination
- Directory structure preserved
- File metadata preservation is best-effort and backend-dependent when `options.preserve_*` flags are set (e.g., not all backends can restore Unix permissions or nanosecond timestamps)

**Error Conditions**:
- `ArchiveError::Password`: Wrong or missing password
- `ArchiveError::Io`: Disk full, permission denied, or overwrite conflict when file exists and `overwrite=false` (uses `AlreadyExists` I/O kind)
- `ArchiveError::Corruption`: CRC mismatch detected
- `ArchiveError::CodecUnavailable`: Archive requires unavailable compression codec

**Performance**: _Target_ — within 20% of native 7zip (SC-010). No benchmark artifacts exist yet; this is an aspirational target, not a verified claim.

**Example**:
```rust
let options = ExtractionOptions {
    destination: PathBuf::from("output/"),
    preserve_times: true,
    ..Default::default()
};

Archive::open("data.zip")?.extract_all(options)?;
```

---

### Archive::extract_file

```rust
impl Archive {
    pub fn extract_file(&self, path: &str, options: ExtractionOptions)
        -> Result<(), ArchiveError>
}
```

**Purpose**: Extract single file by path.

**Example**:
```rust
archive.extract_file("docs/readme.txt", options)?;
```

---

### Archive::extract_to_memory

```rust
impl Archive {
    pub fn extract_to_memory(&self, path: &str) -> Result<Vec<u8>, ArchiveError>
}
```

**Purpose**: Extract file contents to memory (no disk write).

**Example**:
```rust
let contents = archive.extract_to_memory("config.json")?;
let config: Config = serde_json::from_slice(&contents)?;
```

---

### Archive::extract_filtered

```rust
impl Archive {
    pub fn extract_filtered<F>(&self, predicate: F, options: ExtractionOptions)
        -> Result<(), ArchiveError>
    where
        F: Fn(&ArchiveEntry) -> bool + Sync,
}
```

> The predicate is a generic parameter bounded by `Fn(&ArchiveEntry) -> bool + Sync`.
> Callers pass a closure or function directly; no boxing is required.
> (A separate `EntryFilter` type alias exists in `src/options.rs` for the
> `ExtractionOptions::filter` field, but `extract_filtered` itself is generic.)

**Purpose**: Extract only files matching predicate.

**Example**:
```rust
// Extract only .txt files
archive.extract_filtered(
    |entry| entry.path.ends_with(".txt"),
    options
)?;
```

## Progress Callbacks (Phase 1 Enhanced)

All extraction methods support progress tracking with cancellation:

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;

let options = ExtractionOptions {
    destination: PathBuf::from("output/"),
    progress: Some(Box::new(|current, total: Option<u64>| {
        if let Some(t) = total {
            println!("Progress: {}/{} bytes ({:.1}%)",
                current, t, 100.0 * current as f64 / t as f64);
        } else {
            println!("Progress: {} bytes (total unknown)", current);
        }

        // Check for cancellation (pseudocode — replace with your own
        // cancellation signal, e.g., an AtomicBool or channel check)
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            ControlFlow::Break(())  // Cancel extraction
        } else {
            ControlFlow::Continue(())  // Continue extraction
        }
    })),
    ..Default::default()
};

// Cancellation semantics — see data-model.md §7 "Cancellation Semantics"
// (canonical source) for the authoritative definition.
//
// Summary: ControlFlow::Break(()) from on_progress() causes the backend
// to surface an ArchiveError (typically Format { message: "cancelled" }).
// There is no dedicated Cancelled variant. Partial output may remain on
// disk; cleanup is the caller's responsibility.
match archive.extract_all(options) {
    Ok(_) => println!("Extraction complete"),
    Err(e) => {
        // Cancellation via Break surfaces as an ArchiveError, not Ok(())
        eprintln!("Extraction stopped: {}", e);
    }
}
```

**Cadence and cancellation**: See [data-model.md §7](../data-model.md) for the canonical ProgressCallback definition and cancellation semantics (authoritative source). See [contracts/progress.md](progress.md) for cadence guarantees and callback API usage patterns.

## Format-Agnostic Guarantee

**Critical (FR-001, SC-001)**: Same primary API surface (`Archive::open` + `extract_all`) works for all formats. Backend behavior varies by format (e.g., streaming vs. buffered, parallel vs. sequential, metadata fidelity, password support) and some formats have specific caveats (see RAR concurrency, TAR CRC limitations, ZIP split unsupported). The caller's code shape is identical, but format-specific edge cases may still surface through errors or differing postconditions.

```rust
fn extract_any(path: &Path) -> Result<(), ArchiveError> {
    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        ..Default::default()
    };

    Archive::open(path)?.extract_all(options) // Works for any supported archive format
}

extract_any(Path::new("file.zip"))?;  // Works
extract_any(Path::new("file.7z"))?;   // Works
extract_any(Path::new("file.rar"))?;  // Works
extract_any(Path::new("file.tar.gz"))?; // Works
```

## Streaming & Memory Bounds (Phase 1 Enhanced)

**Aspirational target (SC-009, libarchive-only, unshipped)**: <100MB memory for 10GB archives. This target has not been validated with benchmarks and should be treated as a backlog/research item. The bound would apply only to libarchive-backed formats (TAR family, ISO) which stream with a fixed read buffer. Native backends (Piz, ZipReader, SevenZ, UnRAR) buffer full entries in memory and are not subject to this bound.

Implementation uses `StreamingExtractor` (implements `Read`) for streaming extraction:
- **Libarchive backends** (TAR family, ISO): Read compressed data in chunks, decompress on-the-fly, write to disk immediately
- **Native backends**: Buffer full entry in memory, then wrap in `Cursor<Vec<u8>>`

**Note**: ZIP (Piz), 7z (SevenZ), and RAR (UnRAR) backends currently buffer the full entry in memory before constructing `StreamingExtractor`. True bounded-memory streaming currently applies to libarchive-backed formats only.

See [contracts/streaming.md](streaming.md) for full streaming extraction API contract.

## Password-Protected Archives (Phase 1)

### Password Handling

```rust
let options = ExtractionOptions {
    destination: PathBuf::from("output/"),
    password: Some(String::from("my_password")),
    ..Default::default()
};

archive.extract_all(options)?;
```

### Password Detection

```rust
// Check if archive requires password and supply it programmatically.
// Interactive password prompts are out of scope (see mvp-scope.md);
// the caller is responsible for obtaining the password before extraction.
if archive.is_encrypted()? {
    let password = get_password_from_config_or_env()?; // caller-provided
    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        password: Some(String::from(password)),
        ..Default::default()
    };
    archive.extract_all(options)?;
} else {
    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        ..Default::default()
    };
    archive.extract_all(options)?;
}

// Check individual entry encryption
for entry in archive.list_files()? {
    if entry.is_encrypted {
        println!("Entry '{}' is encrypted", entry.path);
    }
}
```

**Behavior**:
- `Archive::is_encrypted()` returns `true` if any entry is encrypted
- `ArchiveEntry::is_encrypted` shows per-entry encryption status
- Missing password returns `ArchiveError::Password`

### Cross-Format Password Support

| Format | Password Support | Entry-Level Encryption | Archive-Level Encryption | Backend |
|--------|------------------|------------------------|--------------------------|---------|
| RAR | ✅ Yes | ✅ Yes | ✅ Yes (header encryption) | UnRAR |
| RAR5 | ✅ Yes | ✅ Yes | ✅ Yes (header encryption) | UnRAR |
| ZIP | ✅ Yes | ✅ Yes | ❌ No | ZipReader |
| 7z | ✅ Yes | ❌ No | ✅ Yes (solid encryption) | SevenZ |
| TAR.* | ❌ No | ❌ No | ❌ No | libarchive |

> **Notes**: ZIP encrypted-archive support uses the ZipReader backend (not Piz, which does not support decryption). 7z "entry-level" encryption is not individually addressable because 7z uses solid blocks. See [data-model.md](../data-model.md) for backend selection logic and password-handling caveats.

## Multi-Part Archive Support (Phase 1)

> **Scope**: Full multipart extraction (open first part, chain automatically) is RAR-only. ZIP and 7z split archives are unsupported for extraction — see the ZIP section below. 7z split archives (`.7z.001`, `.7z.002`, ...) are detected at open time; callers observe detection through error handling (e.g., `ArchiveError::Unsupported`) when parts are missing or the format is unrecognized. There is no dedicated "is_multipart" query.

### RAR Volumes (Supported ✅)

```rust
// Multi-part RAR: open first part, rest loaded automatically
let archive = Archive::open("backup.part01.rar")?;
archive.extract_all(ExtractionOptions::default())?;

// Also works with .r00, .r01, .r02, ... naming
let archive = Archive::open("backup.rar")?;  // First part
archive.extract_all(ExtractionOptions::default())?;
```

**Behavior**:
- Opening first part automatically chains to subsequent parts
- All parts must be in same directory
- Missing part returns `ArchiveError::Io` (file not found)

**Error handling**:
```rust
match Archive::open("backup.part05.rar") {
    Err(ArchiveError::Format { format, message }) => {
        // Error: "Multi-part RAR: please open the first part (.part01.rar or .rar)"
        let archive = Archive::open("backup.part01.rar")?;
        archive.extract_all(options)?;
    }
    Ok(archive) => { /* use archive */ }
    Err(e) => { /* other errors */ }
}
```

### ZIP Split Archives (Not Supported ❌)

```rust
// Split ZIP (.zip, .z01, .z02, ...) not supported
match Archive::open("backup.z01") {
    Err(ArchiveError::Unsupported { operation, format, details }) => {
        // Error: "Split ZIP archives are not supported. Please use a tool to merge parts first."
        eprintln!("Operation '{}' unsupported for {:?}: {:?}", operation, format, details);
    }
    _ => unreachable!(),
}
```

**Rationale**: Split ZIP support requires reassembly (complex, rare format). Use standard tools to merge before extraction.

> **Troubleshooting**: Split ZIP detection returns `ArchiveError::Unsupported` at open time. As a workaround, merge parts into a standard ZIP before extraction using shell tools:
> - **Unix/macOS**: `cat backup.z01 backup.z02 backup.zip > merged.zip`
> - **Windows**: `copy /b backup.z01+backup.z02+backup.zip merged.zip`
>
> Note: Merged output may not work for all split-ZIP variants (e.g., ZIP64 splits).

## Parallel Extraction (Phase 1)

**Phase 1 Enhancement**: Automatic parallel extraction for archives with enough files.

```rust
// Automatically uses parallel extraction when beneficial
let archive = Archive::open("1000_files.zip")?;
archive.extract_all(ExtractionOptions::default())?;
```

**Behavior**:
- Sequential extraction for small archives; parallel extraction for archives with enough files to amortize thread-pool overhead (uses Rayon work-stealing parallelism)
- **RAR note (OI-026-004 resolved)**: UnRAR FFI calls are serialized via a process-wide mutex (`UNRAR_LOCK`). RAR archives use sequential extraction. Concurrent caller access is safe; the mutex prevents cross-archive corruption.

> _Implementation note_: The current threshold is 4 files. This is a heuristic that may change; callers should not depend on the exact cutoff.

## CRC32 Verification (Phase 1)

**Phase 1 Enhancement**: Automatic CRC32 verification during extraction.

```rust
let options = ExtractionOptions {
    destination: PathBuf::from("output/"),
    verify_crc32: true,  // Default: enabled
    ..Default::default()
};

match archive.extract_all(options) {
    Ok(()) => println!("Extraction complete, CRC32 verified"),
    Err(ArchiveError::Corruption { path, details }) => {
        eprintln!("CRC32 mismatch: {} - {}", path, details);
    }
    Err(e) => eprintln!("Extraction failed: {}", e),
}
```

**Behavior**:
- CRC32 computed during extraction (streaming; overhead is estimated at <2% but this figure is a rough estimate, not a verified benchmark result)
- Mismatch returns `ArchiveError::Corruption`
- Can be disabled with `verify_crc32: false` for performance
- The `verify_crc32()` function is also exposed as a public utility in `unified_archive::security` for callers who need standalone CRC verification outside of extraction (see API_REFERENCE.md)

**Format Support**:
| Format | CRC32 Available | Verification | Method |
|--------|-----------------|--------------|--------|
| RAR | ✅ Metadata | ✅ Automatic | CRC32 comparison against stored checksum |
| RAR5 | ✅ Metadata | ✅ Automatic | CRC32 comparison against stored checksum |
| ZIP | ✅ Metadata | ✅ Automatic | CRC32 comparison against stored checksum |
| 7z | ✅ Metadata | ✅ Automatic | CRC32 comparison against stored checksum |
| TAR | ❌ No stored CRC | ⚠️ Read-based | Integrity detected via libarchive read errors (decompression failure, truncation), not CRC comparison |

## Thread Safety

`Archive` is `Send` but not `Sync`. A single `Archive` instance must not be shared across threads without external synchronization. To extract from the same file on multiple threads, open separate `Archive` handles per thread. Note that the UnRAR backend has additional restrictions due to its sequential C API — concurrent extraction from a single UnRAR handle is not supported even with synchronization.

## Contract Tests

**Guaranteed (implemented in CI)**:
1. Extract from each supported format (ZIP, 7z, RAR, RAR5, TAR.GZ, TAR.BZ2, TAR.XZ, ISO) — standalone BZIP2/XZ not supported per AD 0018
2. Verify extracted files match originals (byte-for-byte)
3. Test password-protected extraction
4. Test progress callbacks fire and respect `ControlFlow::Break` (cadence is backend-dependent)

**Backlog / research (not yet implemented)**:
5. Memory-bound validation: <100MB for 10GB archives (aspirational, libarchive-backed formats only; native backends buffer full entries) — requires benchmark harness and large test archives
6. Performance comparison: within 20% of native 7zip (SC-010) — requires benchmark harness and reference artifacts that do not yet exist
