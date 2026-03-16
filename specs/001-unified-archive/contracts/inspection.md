# API Contract: Inspection Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-31 (Updated for Phase 1 enhancements)
**Status**: Phase 1 Design
**Purpose**: List files, retrieve metadata, and validate integrity without extraction

## Core Operations

### Archive::list_files (Phase 1 Enhanced)

```rust
impl Archive {
    pub fn list_files(&self) -> Result<&[ArchiveEntry], ArchiveError>
}
```

**Unified Interface**: Works identically for all formats (ZIP, 7z, RAR, RAR5, TAR.GZ, etc.).

**Phase 1 Enhancement**: Returns `&[ArchiveEntry]` (borrowed slice) instead of `Vec<ArchiveEntry>`.
- First call: Reads from archive backend, caches in `OnceLock<Vec<ArchiveEntry>>`
- Subsequent calls: Returns cached slice (zero allocation, zero cost)
- Resolves UnRAR sequential iteration limitation

**Returns**: All entries with **enhanced** metadata structure (Phase 1):
- path (normalized with `/`)
- size, compressed_size, **compression_ratio()** method
- modified time (UTC), **created, accessed** (format-dependent)
- crc32 (if available)
- **is_encrypted** (entry-level encryption flag)
- **comment** (if supported by format)
- **attributes** (platform-specific file attributes)

**Performance**: <1 second for 10,000 files (SC-008).

**Caching Benefits**:
- Zero-cost repeated access (no backend reopening)
- Memory efficient (single Vec allocation)
- Thread-safe (OnceLock synchronization)

**Example**:
```rust
let archive = Archive::open("data.zip")?;

// First call: reads from backend, caches
let entries = archive.list_files()?;
for entry in entries {
    println!("{}: {} bytes ({:.1}% compression, CRC: {:08x})",
        entry.path,
        entry.size.unwrap_or(0),
        entry.compression_ratio().unwrap_or(0.0) * 100.0,
        entry.crc32.unwrap_or(0));

    // Phase 1 enhancements
    if entry.is_encrypted {
        println!("  [ENCRYPTED]");
    }
    if let Some(comment) = &entry.comment {
        println!("  Comment: {}", comment);
    }
}

// Second call: returns cached slice (no allocation)
let entries_again = archive.list_files()?;
assert!(std::ptr::eq(entries.as_ptr(), entries_again.as_ptr()));
```

---

### Archive::entry_count

```rust
impl Archive {
    pub fn entry_count(&self) -> Result<usize, ArchiveError>
}
```

**Purpose**: Get total number of entries without allocating full list.

**Performance**: O(n) scan but no Vec allocation.

---

### Archive::find_entry

```rust
impl Archive {
    pub fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>, ArchiveError>
}
```

**Purpose**: Find specific entry by path.

**Performance**: O(n) linear search.

---

### Archive::validate_integrity

```rust
impl Archive {
    pub fn validate_integrity(&self) -> Result<ValidationReport, ArchiveError>
}
```

**Purpose**: Check CRC32 checksums for all entries.

**Returns**:
```rust
pub struct ValidationReport {
    pub total_entries: usize,
    pub validated: usize,
    pub failed: Vec<String>, // Paths of corrupted entries
}
```

**Performance**: O(n*m) - reads and validates all entries.

**Requirement**: SC-012 - detects 100% of intentionally corrupted archives.

**Example**:
```rust
let report = archive.validate_integrity()?;
if !report.failed.is_empty() {
    eprintln!("Corrupted: {:?}", report.failed);
}
```

## Format-Agnostic Guarantee

**Critical (FR-001, FR-003)**: Metadata structure identical across formats.

```rust
// Same code, different formats
let zip_entries = Archive::open("file.zip")?.list_files()?;
let sevenz_entries = Archive::open("file.7z")?.list_files()?;
let rar_entries = Archive::open("file.rar")?.list_files()?;

// All have same ArchiveEntry structure
assert_eq!(std::mem::size_of_val(&zip_entries[0]),
           std::mem::size_of_val(&sevenz_entries[0]));
```

## Enhanced Metadata (Phase 1)

### Compression Ratio

```rust
for entry in archive.list_files()? {
    if let Some(ratio) = entry.compression_ratio() {
        println!("{}: {:.1}% compression",
            entry.path, ratio * 100.0);
    }
}
```

**Calculation**: `compression_ratio = compressed_size / size`
- Range: 0.0-1.0 (0% to 100%)
- None for directories or when sizes unavailable

### Extended Timestamps

```rust
for entry in archive.list_files()? {
    println!("Modified: {:?}", entry.modified);
    if let Some(created) = entry.created {
        println!("Created: {:?}", created);
    }
    if let Some(accessed) = entry.accessed {
        println!("Last accessed: {:?}", accessed);
    }
}
```

**Format Support**:
| Format | Modified | Created | Accessed |
|--------|----------|---------|----------|
| RAR | ✅ Yes | ✅ Yes | ✅ Yes |
| RAR5 | ✅ Yes | ✅ Yes | ✅ Yes |
| ZIP | ✅ Yes | ⚠️ Optional | ⚠️ Optional |
| 7z | ✅ Yes | ✅ Yes | ⚠️ Rare |
| TAR | ✅ Yes | ❌ No | ❌ No |

### Encryption Status

```rust
let encrypted_files: Vec<_> = archive.list_files()?
    .iter()
    .filter(|e| e.is_encrypted)
    .map(|e| &e.path)
    .collect();

if !encrypted_files.is_empty() {
    println!("Encrypted files: {:?}", encrypted_files);
}
```

**Per-Entry vs Archive-Level**:
- ZIP: Per-entry encryption (each file can have different password)
- RAR/RAR5: Can be archive-level or per-entry
- 7z: Archive-level (all-or-nothing)

### File Comments

```rust
for entry in archive.list_files()? {
    if let Some(comment) = &entry.comment {
        println!("{}: {}", entry.path, comment);
    }
}
```

**Format Support**:
| Format | Comment Support |
|--------|-----------------|
| ZIP | ✅ Per-entry |
| RAR | ✅ Per-entry |
| RAR5 | ✅ Per-entry |
| 7z | ❌ No |
| TAR | ❌ No |

### Platform Attributes

```rust
for entry in archive.list_files()? {
    if let Some(attrs) = &entry.attributes {
        if let Some(windows_attrs) = attrs.windows {
            if windows_attrs & 0x01 != 0 {
                println!("{} [READ-ONLY]", entry.path);
            }
        }
        if let Some(xattr) = &attrs.unix_xattr {
            println!("{} has {} extended attributes", entry.path, xattr.len());
        }
    }
}
```

## Entry Caching Architecture (Phase 1)

**Implementation**:
```rust
pub struct Archive {
    backend: ArchiveBackend,
    entry_cache: OnceLock<Vec<ArchiveEntry>>,
    // ...
}

impl Archive {
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.entry_cache.get_or_try_init(|| {
            match &self.backend {
                ArchiveBackend::Unrar(unrar) => unrar.list_files(),
                ArchiveBackend::Libarchive(lib) => lib.list_files(),
            }
        }).map(|v| v.as_slice())
    }
}
```

**Benefits**:
1. **Zero-cost repeated access**: No allocation after first call
2. **Resolves UnRAR limitation**: Sequential iteration cached, no reopening needed
3. **Thread-safe**: OnceLock provides synchronization
4. **Memory efficient**: Single Vec allocation per Archive instance

**Performance Comparison**:
| Operation | Without Cache | With Cache (Phase 1) |
|-----------|---------------|----------------------|
| First `list_files()` | O(n) | O(n) |
| Second `list_files()` | O(n) reopen | O(1) cached |
| N calls | O(n*N) | O(n) + O(1)*N |

**Example Benefit**:
```rust
let archive = Archive::open("large.rar")?;

// Without caching: reopens archive N times
for _ in 0..100 {
    let entries = archive.list_files()?; // Slow: O(n) each time
    process(entries);
}

// With caching (Phase 1): opens once, returns cached N-1 times
for _ in 0..100 {
    let entries = archive.list_files()?; // Fast: O(1) after first
    process(entries);
}
```

## Thread Safety

All inspection operations are `&self` (immutable), safe for concurrent use from multiple threads with proper synchronization.

**Phase 1**: OnceLock-based caching is thread-safe (built-in synchronization).

## Contract Tests

1. List files from each supported format (ZIP, 7z, RAR, RAR5, TAR.GZ, etc.)
2. Verify metadata consistency (size, crc32, modified time)
3. **Phase 1**: Verify enhanced metadata (compression_ratio, is_encrypted, created/accessed times)
4. **Phase 1**: Verify caching (repeated calls return same slice pointer)
5. Validate integrity detects corrupted files
6. Performance: <1s for 10k files

## References

- Phase 0 Research: `research.md` - Entry caching strategy decisions
- Data Model: `data-model.md` - Enhanced ArchiveEntry specification
- Constitution v1.1.0 Principle II: Pragmatic performance (zero-cost caching)
