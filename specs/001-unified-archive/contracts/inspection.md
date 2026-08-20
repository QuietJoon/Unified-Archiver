# API Contract: Inspection Operations

**Feature**: 001-unified-archive
**Date**: 2025-10-31 (Updated for Phase 1 enhancements)
**Status**: Implemented (retrospective documentation)
**Purpose**: List files, retrieve metadata, and validate integrity without extraction

## Core Operations

### Archive::list_files (Phase 1 Enhanced)

```rust
impl Archive {
    pub fn list_files(&self) -> Result<&[ArchiveEntry], ArchiveError>
}
```

**Unified Interface**: Returns a common `&[ArchiveEntry]` shape for all formats (ZIP, 7z, RAR, RAR5, TAR.GZ, etc.), with format-dependent field availability (e.g., timestamps, comments, attributes, encryption flags).

**Phase 1 Enhancement**: Returns `&[ArchiveEntry]` (borrowed slice) instead of `Vec<ArchiveEntry>`.
- First call: Reads from archive backend, caches in `OnceCell<Vec<ArchiveEntry>>`
- Subsequent calls: Returns cached slice (avoids re-listing and extra allocation)
- Resolves UnRAR sequential iteration limitation

**Returns**: All entries with **enhanced** metadata structure (Phase 1):
- path (normalized with `/`)
- size, compressed_size (availability varies by format and backend; may be None), **compression_ratio()** method
- modified time (UTC), **created, accessed** (format-dependent)
- crc32 (if available)
- **is_encrypted** (entry-level encryption flag)
- **comment** (reserved; currently always None — no backend implements comment extraction yet)
- **attributes** (platform-specific file attributes)

**Design goal**: <1 second for 10,000 files (SC-008). This target is unverified and pending benchmark infrastructure; it is not a shipped guarantee.

**Caching Benefits**:
- Low-cost repeated access (no backend reopening or reallocation)
- Memory efficient (single Vec allocation)
- Internally synchronized cache (OnceCell ensures safe lazy initialization; does not make `Archive` itself `Sync`)

**Example**:
```rust
let archive = Archive::open("data.zip")?;

// First call: reads from backend, caches
let entries = archive.list_files()?;
for entry in entries {
    println!("{}: {} bytes",
        entry.path,
        entry.size.map_or("unknown".to_string(), |s| s.to_string()));
    if let Some(ratio) = entry.compression_ratio() {
        println!("  Compression: {:.1}%", ratio * 100.0);
    }
    if let Some(crc) = entry.crc32 {
        println!("  CRC: {:08x}", crc);
    }

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

**Purpose**: Get total number of entries.

**Implementation**: Calls `list_files().len()` internally. The full entry list is cached after the first call, so subsequent calls are O(1).

**Performance**: O(n) on first call (populates cache), O(1) thereafter.

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

**Purpose**: Validate archive integrity using backend-appropriate checks (e.g., CRC32 verification, decompression test).

**Returns**:
```rust
pub struct ValidationReport {
    pub total_entries: usize,
    pub validated: usize,
    pub failed: Vec<String>, // Paths of corrupted entries
}
```

**Performance**: O(n*m) - reads and validates all entries.

**Requirement**: SC-012 - detects corruption in archives where CRC32 checksums are present and the corruption affects validated entry data. Detection coverage is scoped to the validation modes supported by each backend (CRC32 mismatch, truncated streams); it does not cover all possible corruption modes (e.g., corrupted metadata headers that prevent opening).

**Example**:
```rust
let report = archive.validate_integrity()?;
if !report.failed.is_empty() {
    eprintln!("Corrupted: {:?}", report.failed);
}
```

## Archive-level integrity

The library exposes three companion methods for **archive-wide** integrity and
content-identity, distinct from the per-entry CRC fields exposed via
`ArchiveEntry::crc32`. None of these values are stored in the archive — they are
computed on demand from the cached entry list returned by `list_files()`.

### Archive::calculate_archive_crc

```rust
impl Archive {
    pub fn calculate_archive_crc(&self) -> Result<u32, ArchiveError>
}
```

**Purpose**: Reproduce the "archive CRC" displayed by 7-Zip and similar tools — a
quick fingerprint computed as the wrapping arithmetic sum of every entry's
CRC32. Suitable for fast equality checks but **not** collision-resistant.

**Algorithm**:
1. Iterate `list_files()`.
2. For each entry with `crc32 = Some(c)`, accumulate `sum = sum.wrapping_add(c)`.
3. Entries without `crc32` (e.g. directories, formats that do not surface a
   per-entry CRC) are skipped, not treated as zero.

**Determinism guarantees**:
- Independent of compression method, entry order, archive comments, and
  timestamps — only the multiset of per-entry CRCs matters.
- Cross-format equivalent: ZIP/7z/RAR archives over the same file contents
  produce the same value, provided each backend exposes per-entry CRC32.

**Performance**: O(n) over the cached entry list; O(1) after the first
`list_files()` call has populated the cache.

### Archive::calculate_manifest_digest

> **Reality check (2026-08-17):** the algorithm and performance text in this
> section is planning-era and has drifted from the shipped crate. Canonical
> behaviour is `docs/API_REFERENCE.md` and the rustdoc on
> `Archive::calculate_content_multiset_digest_and_size` (`src/inspection.rs`),
> of which `calculate_manifest_digest` is now a shim. Known deltas below:
> (1) the `"{path}:{size_or_0}"` fallback is **gone** — an entry with no
> stored CRC32 has its payload streamed through a CRC32 hasher instead, so
> the digest is content-identity on CRC-less formats and no longer O(n)
> there (AD 0047); (2) AE-2 AES ZIP entries list `crc32 = None` and therefore
> take that streaming branch, which makes the digest **require the password**
> on password-protected ZIPs; (3) a CRC-less entry whose payload does not
> match its declared size returns `ArchiveError::Corruption` rather than
> digesting the short payload (DCR-011); (4) duplicate archive-internal paths
> no longer carry a per-path occurrence ordinal, so digests stored under the
> older encoding do not match for such archives (DCR-012). Points 2 and 4 are
> behaviour changes not yet carried by any tagged version — see the
> `[Unreleased]` section of `CHANGELOG.md`.

```rust
impl Archive {
    pub fn calculate_manifest_digest(&self) -> Result<String, ArchiveError>
}
```

**Purpose**: Stronger content-identity digest than `calculate_archive_crc`.
Designed for deduplication workflows where wrapping-sum collisions are
unacceptable. Same files in any order, in any supported archive format, produce
the same digest.

**Algorithm**:
1. For every `EntryType::File` entry:
   - If `crc32 = Some(c)`, encode as 8 lowercase hex chars of the big-endian
     bytes of `c`.
   - Otherwise, fall back to `"{path}:{size_or_0}"`. The fallback ensures
     digests remain meaningful for backends that do not expose CRC32.
2. Sort the resulting strings lexicographically.
3. Join with `,`.
4. CRC32-hash the joined byte string (`crc32fast`).
5. Return as 8-char lowercase hex.

**Edge cases**:
- Returns `Ok(String::new())` when no file entries are present (empty archive
  or directories-only). Callers must treat the empty string as "no digest"
  rather than as a literal hash value.
- Directory entries are excluded from the digest input.

**Determinism guarantees**:
- Stable across archive format (ZIP, 7z, RAR, RAR5, TAR.GZ, …) when the
  underlying file contents and CRC32 metadata match.
- Stable across entry insertion order, archive-level metadata, and compression
  level.
- **Not** stable across changes that alter per-entry CRC32 values (e.g.
  recompressing with a checksum-mutating tool, or stripping CRC fields).

### Archive::calculate_manifest_summary

```rust
impl Archive {
    pub fn calculate_manifest_summary(&self) -> Result<(String, u64), ArchiveError>
}
```

**Purpose**: One-shot helper that returns `(digest, total_uncompressed_size)`
for callers that need both values together (e.g. dedup tools that key on
digest and display total size).

**Algorithm**:
1. `digest` = `calculate_manifest_digest()` (same algorithm and edge cases).
2. `total_uncompressed_size` = sum of `entry.size.unwrap_or(0)` over every
   `EntryType::File` entry. Entries without a known size contribute 0; the
   total is therefore a lower bound when sizes are unavailable.

**Performance**: O(n) over the cached entry list, plus the cost of
`calculate_manifest_digest` (also O(n)).

**Example**:
```rust
let archive = Archive::open("data.zip")?;
let (digest, total) = archive.calculate_manifest_summary()?;
if !digest.is_empty() {
    println!("Identity {} — {} uncompressed bytes", digest, total);
}
```

### Choosing among the three

| Need | Method |
|---|---|
| Quick "did anything change?" check, tolerant of collisions | `calculate_archive_crc` |
| Content-identity matching across formats (deduplication) | `calculate_manifest_digest` |
| Identity + size in a single pass | `calculate_manifest_summary` |

All three reuse the cached entry list, so calling them after `list_files()` is
cheap. None of them re-read entry payloads — they operate purely on the
metadata each backend exposes.

## Format-Agnostic Guarantee

**Critical (FR-001, FR-003)**: All formats share the same `ArchiveEntry` struct and field access patterns. Individual fields are populated on a format-dependent basis (e.g., `created` is available for RAR but not TAR; `compressed_size` depends on backend support).

```rust
// Same code, different formats — shared field access patterns
let zip_entries = Archive::open("file.zip")?.list_files()?;
let sevenz_entries = Archive::open("file.7z")?.list_files()?;
let rar_entries = Archive::open("file.rar")?.list_files()?;

// All entries expose the same fields; Option fields may be None for some formats
for entry in zip_entries.iter().chain(sevenz_entries).chain(rar_entries) {
    let _ = (entry.path.as_str(), entry.size, entry.compressed_size,
             entry.modified, entry.created, entry.is_encrypted);
}
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
- Range: >= 0.0 (values above 1.0 indicate data expansion, e.g. incompressible data stored with headers)
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

**Format Support** (reflects backend capability; actual presence depends on how the archive was created):
| Format | Modified | Created | Accessed | Notes |
|--------|----------|---------|----------|-------|
| RAR | ✅ Yes | ✅ Yes | ✅ Yes | Timestamps sourced from RAR headers |
| RAR5 | ✅ Yes | ✅ Yes | ✅ Yes | Timestamps sourced from RAR5 headers |
| ZIP | ✅ Yes | ⚠️ Optional | ⚠️ Optional | Extra-field dependent; many creators omit created/accessed |
| 7z | ✅ Yes | ✅ Yes | ⚠️ Rare | 7z spec supports all three; accessed rarely stored in practice |
| TAR | ✅ Yes | ❌ No | ❌ No | POSIX tar stores only mtime |

> Timestamp precision and presence are ultimately determined by the archiving tool that created the file, not solely by format capability.

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

> **Note**: Comment extraction is not currently implemented by any backend. `entry.comment` is always `None` for now. The field exists in the `ArchiveEntry` struct to support future implementation.

```rust
for entry in archive.list_files()? {
    if let Some(comment) = &entry.comment {
        println!("{}: {}", entry.path, comment);
    }
}
```

#### Roadmap: Comment Support by Format

> **Roadmap material** -- the table below describes format-level capability, not current implementation. No backend currently populates `entry.comment`. This section is retained for future planning and may be relocated to a roadmap document.

| Format | Format Capability | Implementation Status |
|--------|-------------------|----------------------|
| ZIP | Per-entry | Not yet implemented |
| RAR | Per-entry | Not yet implemented |
| RAR5 | Per-entry | Not yet implemented |
| 7z | None | N/A |
| TAR | None | N/A |

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

**Behavioral guarantee**: `list_files()` populates an internal `OnceCell<Vec<ArchiveEntry>>` on first call and returns a borrowed slice on all subsequent calls. Backend dispatch covers Piz, ZipReader, SevenZ, Libarchive, UnRAR, and ZipWriter (write-mode returns an error).

**Benefits**:
1. **Low-cost repeated access**: No re-listing or allocation after first call
2. **Resolves UnRAR limitation**: Sequential iteration cached, no reopening needed
3. **Internally synchronized cache**: OnceCell ensures safe lazy initialization (does not make `Archive` itself `Sync`)
4. **Memory efficient**: Single Vec allocation per Archive instance

**Performance Comparison** (implementation note):
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

`Archive` is `Send` but NOT `Sync`. Inspection operations use `&self` but the archive handle must not be shared across threads without external synchronization.

**Phase 1**: OnceCell provides internally synchronized cache initialization. This does not make `Archive` `Sync` -- the underlying backend handle still requires external synchronization for cross-thread sharing.

## Contract Tests

**Shipped tests**:
1. List files from each supported format (ZIP, 7z, RAR, RAR5, TAR.GZ, etc.)
2. Verify metadata consistency (size, crc32, modified time)
3. **Phase 1**: Verify enhanced metadata where backend-populated (compression_ratio for ZIP/7z, is_encrypted for ZIP/RAR/7z, created/accessed times for RAR; coverage varies by backend)
4. **Phase 1**: Verify caching (repeated calls return same slice pointer)
5. Validate integrity detects corrupted files

**Aspirational benchmarks** (not yet automated):
6. Performance target: <1s for 10k files (SC-008, pending benchmark harness)

## References

- Phase 0 Research: `research.md` - Entry caching strategy decisions
- Data Model: `data-model.md` - Enhanced ArchiveEntry specification
- Constitution v1.1.0 Principle II: Pragmatic performance (cached listing)
