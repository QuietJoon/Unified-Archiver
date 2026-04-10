# API Contract: Extraction Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-31 (Updated for Phase 1 enhancements)
**Status**: Phase 1 Design
**Purpose**: Extract files from archives with unified interface across all formats

## Core Operations

### Archive::extract_all

```rust
impl Archive {
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<(), ArchiveError>
}
```

**Unified Interface**: Same method extracts ZIP, 7z, RAR, RAR5, TAR.GZ, etc.

**Preconditions**:
- Archive is opened for reading
- `options.destination` exists or can be created
- Password provided if archive is encrypted
- No existing files at destination paths (unless `options.overwrite` is true)

**Postconditions**:
- All files extracted to destination
- Directory structure preserved
- File metadata preserved (if `options.preserve_*`)

**Error Conditions**:
- `ArchiveError::Password`: Wrong or missing password
- `ArchiveError::Io`: Disk full, permission denied, file exists when overwrite=false
- `ArchiveError::Corruption`: CRC mismatch detected
- `ArchiveError::Unsupported`: Archive requires unavailable compression codec

**Performance**: Within 20% of native 7zip (SC-010).

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
        F: Fn(&ArchiveEntry) -> bool
}
```

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

        // Check for user cancellation
        if user_cancelled() {
            ControlFlow::Break(())  // Cancel extraction
        } else {
            ControlFlow::Continue(())  // Continue extraction
        }
    })),
    ..Default::default()
};

match archive.extract_all(options) {
    Ok(()) => println!("Extraction complete"),
    Err(ArchiveError::Cancelled) => println!("Extraction cancelled by user"),
    Err(e) => eprintln!("Extraction failed: {}", e),
}
```

**Requirement**: Callbacks update at least once per entry.

**Phase 1 Enhancement**: ControlFlow-based cancellation with graceful cleanup.

See [contracts/progress.md](progress.md) for full progress callback API contract.

## Format-Agnostic Guarantee

**Critical (FR-001, SC-001)**: Same extraction code works for all formats.

```rust
fn extract_any(path: &Path) -> Result<(), ArchiveError> {
    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        ..Default::default()
    };

    Archive::open(path)?.extract_all(options) // Works for ANY format
}

extract_any(Path::new("file.zip"))?;  // Works
extract_any(Path::new("file.7z"))?;   // Works
extract_any(Path::new("file.rar"))?;  // Works
extract_any(Path::new("file.tar.gz"))?; // Works
```

## Streaming & Memory Bounds (Phase 1 Enhanced)

**Requirement (SC-009)**: <100MB memory for 10GB archives.

Implementation uses streaming:
- Read compressed data in chunks
- Decompress on-the-fly
- Write to disk immediately
- No full file buffering in memory

**Phase 1 Enhancement**: `EntryReader<'a>` provides `Read` trait for memory-bounded extraction (~40KB per file).

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
// Check if archive requires password
if archive.is_encrypted()? {
    let password = prompt_user_for_password()?;
    let options = ExtractionOptions {
        password: Some(String::from(password)),
        ..Default::default()
    };
    archive.extract_all(options)?;
} else {
    // No password needed
    archive.extract_all(ExtractionOptions::default())?;
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

| Format | Password Support | Entry-Level Encryption | Archive-Level Encryption |
|--------|------------------|------------------------|--------------------------|
| RAR | ✅ Yes | ✅ Yes | ✅ Yes (header encryption) |
| RAR5 | ✅ Yes | ✅ Yes | ✅ Yes (header encryption) |
| ZIP | ✅ Yes | ✅ Yes | ❌ No |
| 7z | ✅ Yes | ❌ No | ✅ Yes (solid encryption) |
| TAR.* | ❌ No | ❌ No | ❌ No |

## Multi-Part Archive Support (Phase 1)

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
    Err(ArchiveError::Format { message }) => {
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
    Err(ArchiveError::Unsupported { message }) => {
        // Error: "Split ZIP archives are not supported. Please use a tool to merge parts first."
        eprintln!("{}", message);
    }
    _ => unreachable!(),
}
```

**Rationale**: Split ZIP support requires reassembly (complex, rare format). Use standard tools to merge before extraction.

**Workaround**:
```bash
# Merge split ZIP files
cat backup.zip backup.z01 backup.z02 > merged.zip

# Then extract with unified-archive
```

## Parallel Extraction (Phase 1)

**Phase 1 Enhancement**: Automatic parallel extraction for archives with enough files.

```rust
// Automatically uses parallel extraction when beneficial
let archive = Archive::open("1000_files.zip")?;
archive.extract_all(ExtractionOptions::default())?;
```

**Behavior**:
- Sequential extraction: fewer than 4 files (avoids thread pool overhead)
- Parallel extraction: 4 or more files (uses std thread-based parallelism)
- The 4-file threshold balances thread-spawn cost against decompression gains

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
- CRC32 computed during extraction (streaming, <2% overhead)
- Mismatch returns `ArchiveError::Corruption`
- Can be disabled with `verify_crc32: false` for performance

**Format Support**:
| Format | CRC32 Available | Verification |
|--------|-----------------|--------------|
| RAR | ✅ Metadata | ✅ Automatic |
| RAR5 | ✅ Metadata | ✅ Automatic |
| ZIP | ✅ Metadata | ✅ Automatic |
| 7z | ✅ Metadata | ✅ Automatic |
| TAR | ❌ Not available | ⚠️ Skipped |

## Thread Safety

Extraction operations require `&self`, can be called from multiple threads with external synchronization. Each extraction operation is independent.

## Contract Tests

1. Extract from each supported format (ZIP, 7z, RAR, RAR5, TAR.GZ, BZIP2, XZ, ISO)
2. Verify extracted files match originals (byte-for-byte)
3. Test password-protected extraction
4. Test progress callbacks (at least once per entry)
5. Test memory bounds (<100MB for large archives)
6. Test performance (within 20% of native 7zip)
