---
type: Guide
title: "Stream Checksums and Archive Integrity"
description: "Comprehensive guide to understanding checksums in compression and archive formats."
tags: [reference, ADR-0047]
timestamp: 2026-06-10T00:00:00Z
status: active
---

# Stream Checksums and Archive Integrity

Comprehensive guide to understanding checksums in compression and archive formats.

## Table of Contents

1. [Overview](#overview)
2. [Single-File Compression Formats](#single-file-compression-formats)
3. [Multi-File Archive Formats](#multi-file-archive-formats)
4. [Summary Table](#summary-table)
5. [Implementation](#implementation)
6. [Use Cases](#use-cases)

---

## Overview

There are two categories of checksums in compressed files:

### 1. Per-File CRC32 (Content CRC32)
- **What**: Checksum of each individual file's uncompressed content
- **Purpose**: Verify file integrity after extraction
- **Available in**: ZIP, 7z, RAR4 (always CRC32), RAR5 (CRC32 or optionally BLAKE2sp-256). TAR does not store per-file CRC32.
- **unified-archive**: Accessible via `ArchiveEntry.crc32`

### 2. Stream Check/Checksum (Container Checksum)
- **What**: Checksum or integrity check stored in the compression format's metadata
- **Purpose**: Verify compressed stream integrity
- **Available in**: Single-file compression (GZIP, BZIP2, XZ — XZ commonly uses CRC64 or SHA-256, not just CRC32)
- **unified-archive**: Accessible via `extract_stream_checksum()`

---

## Single-File Compression Formats

These formats compress a single data stream and store the CRC32 in their trailer/footer.

### GZIP (.gz)

**Format**: RFC 1952

**Structure**:
```
[10-byte header] [compressed data] [8-byte trailer]
```

**Trailer (8 bytes)**:
- Bytes 0-3: CRC32 of uncompressed data (little-endian)
- Bytes 4-7: Uncompressed size mod 2^32 (little-endian)

**unified-archive API**:
```rust
let checksum = extract_gzip_stream_crc("file.gz")?;
println!("CRC32: {:08X}", checksum.crc32.unwrap());
println!("Size: {} bytes", checksum.uncompressed_size.unwrap());
```

**Key Points**:
- CRC32 is always present
- Covers entire uncompressed payload
- Doesn't change if you modify header metadata (filename, timestamp, comments)
- **Multi-member files** (RFC 1952 members concatenated by `pigz`, `bgzip`, or `cat a.gz b.gz`): only the trailer at the end of the file is read, so the reported CRC32 and size describe the **last member only**, not the whole concatenated payload
- `uncompressed_size` is the trailer `ISIZE` = uncompressed size **mod 2^32** (RFC 1952): exact only for payloads below 4 GiB. A member of 4 GiB or larger reports its size modulo 2^32, not the true size

---

### BZIP2 (.bz2)

**Format**: BZIP2 file format

**Structure**:
```
[4-byte header] [compressed blocks] [end-of-stream marker + CRC32]
```

**End-of-Stream Marker**:
- Magic: `0x177245385090` (6 bytes, BCD of sqrt(pi))
- Stream CRC32: 32 bits (big-endian)

**CRC32 Calculation**:
Stream CRC is computed by combining all block CRCs:
```c
stream_crc = (stream_crc << 1) | (stream_crc >> 31);
stream_crc ^= block_crc;
```

**unified-archive API**:
```rust
let checksum = extract_bzip2_stream_crc("file.bz2")?;
println!("CRC32: {:08X}", checksum.crc32.unwrap());
```

**Key Points**:
- CRC32 is always present
- Combines CRCs from all compression blocks
- Uses different bit order than GZIP
- The EOS marker is bit-packed (blocks carry no byte-alignment padding), so it usually starts mid-byte; extraction scans the file tail at every bit offset

---

### XZ (.xz)

**Format**: XZ file format specification 1.2.1

**Structure**:
```
[12-byte stream header] [blocks] [index] [stream footer]
```

**Check Types** (in Stream Flags byte 7):
- `0x00`: None (no checksum)
- `0x01`: CRC32 (4 bytes)
- `0x04`: CRC64 (8 bytes) - **most common**
- `0x0A`: SHA-256 (32 bytes)

**unified-archive API**:
```rust
let checksum = extract_xz_stream_check("file.xz")?;
match checksum.check_type {
    CheckType::Crc32 => println!("Uses CRC32"),
    CheckType::Crc64 => println!("Uses CRC64"),
    CheckType::Sha256 => println!("Uses SHA-256"),
    CheckType::None => println!("No checksum"),
    _ => println!("Unknown"),
}
```

**Key Points**:
- Check type is configurable
- Most files use CRC64 (default)
- Actual check value requires parsing entire stream
- Currently we only extract the check type
- **Multi-stream files** (independent XZ streams concatenated, e.g. `cat a.xz b.xz`): only the first Stream Header is read, so the reported check type describes the **first stream only**, not any later streams (which may use a different check type)

---

## Multi-File Archive Formats

These formats contain multiple files and have different checksum structures.

### ZIP (.zip)

**Format**: PKWARE APPNOTE.TXT

**Per-File CRC32**:
- Stored in Local File Header (offset 14-17)
- Stored in Central Directory Header (offset 16-19)
- Covers uncompressed file content
- Little-endian format

**Archive-Level Checksums**:
❌ **No overall archive CRC32**
- ZIP has no single checksum for the entire archive
- Each file has its own CRC32
- Central Directory has CRC32 for its comment field only

**unified-archive**:
```rust
// Per-file CRC32 (available)
let archive = Archive::open("file.zip")?;
for entry in archive.list_files()? {
    println!("{}: CRC32={:08X?}", entry.path, entry.crc32);
}

// Archive-level CRC32 (not applicable)
// ZIP format doesn't have this concept
```

**Why No Archive CRC32?**
- ZIP is a random-access format
- Files can be added/removed without recompression
- Central Directory can be updated independently
- No single "stream" to checksum

---

### 7z (.7z)

**Format**: 7z Format Specification (7zFormat.txt)

**Per-File CRC32**:
- Stored in Files Header section
- Covers uncompressed file content
- Each file has independent CRC32

**Packed Stream CRC32**:
✅ **CRC32 for each packed (compressed) stream**
- Stored in PackInfo section
- Verifies compressed data integrity
- Multiple files can share one packed stream (solid compression)

**Header CRC32**:
✅ **CRC32 of archive headers**
- Next Header CRC in signature header
- Calculated from: Next Header Offset + Size + CRC fields
- Verifies archive structure integrity

**unified-archive**:
```rust
// Per-file CRC32 (available)
let archive = Archive::open("file.7z")?;
for entry in archive.list_files()? {
    println!("{}: CRC32={:08X?}", entry.path, entry.crc32);
}

// Packed stream CRC32 (not yet exposed)
// Would need to parse 7z internal structures
```

**7z Checksum Levels**:
1. **File CRC32**: Each file's uncompressed data
2. **Packed Stream CRC32**: Each compressed stream
3. **Header CRC32**: Archive metadata integrity

---

### RAR / RAR5 (.rar)

**Format**: RAR 5.0 archive format (technote.htm)

**Per-File Checksums**:
- **RAR4**: CRC32 (32-bit) always
- **RAR5**: CRC32 (default) or BLAKE2sp-256 (optional)

**File Header Structure**:
- CRC32 stored directly in file header
- BLAKE2 stored in extra area if used
- Covers uncompressed file content

**Recovery Record CRC**:
✅ **Optional recovery record with its own CRC**
- Stores CRC32 of recovery data
- Used for archive repair
- Independent of file CRCs

**Archive-Level Checksum**:
❌ **No overall archive CRC32**
- Like ZIP, RAR has no single archive checksum
- Each file/volume has independent checksums
- Recovery records are optional

**unified-archive**:
```rust
// Per-file CRC32 (available)
let archive = Archive::open("file.rar")?;
for entry in archive.list_files()? {
    // RAR4/RAR5 always have CRC32; RAR5 may use BLAKE2 instead
    if let Some(crc) = entry.crc32 {
        println!("{}: CRC32={:08X}", entry.path, crc);
    }
}

// BLAKE2 hash (RAR5 only, not yet exposed)
// Would need to parse extra area records
```

**RAR5 vs RAR4 Checksums**:
- **RAR4**: Always CRC32
- **RAR5**: CRC32 (default) or BLAKE2sp-256
- BLAKE2 is cryptographically secure
- Practically impossible for collision

---

## Summary Table

| Format | Stream Check | Per-File CRC32 | Archive CRC32 | Notes |
|--------|--------------|----------------|---------------|-------|
| **GZIP** | ✅ CRC32 in trailer | N/A (single file) | N/A | Always CRC32 + size |
| **BZIP2** | ✅ CRC32 in EOS marker | N/A (single file) | N/A | Combined block CRCs |
| **XZ** | ⚠️ Check-type detection only | N/A (single file) | N/A | Detects CRC32/CRC64/SHA-256/None; actual check value not extracted |
| **ZIP** | ❌ No concept | ✅ In headers | ❌ No | Per-file only |
| **7z** | ⚠️ Packed streams (not exposed) | ✅ In Files section | ⚠️ Header CRC (not exposed) | Multi-level checksums; packed-stream and header CRC not yet exposed by API |
| **RAR4** | ❌ No | ✅ CRC32 always | ❌ No | Optional recovery CRC |
| **RAR5** | ❌ No | ✅ CRC32 or BLAKE2sp-256 | ❌ No | Optional recovery CRC |

**Legend**:
- ✅ Available and exposed by the API
- ⚠️ Present in format but not yet exposed by the API
- ❌ Not applicable / not available
- N/A: Not applicable (single-file format)

---

## Implementation

### Current unified-archive Support

#### Stream Checksum Retrieval (Single-File Compression)

```rust
use unified_archive::stream_crc::extract_stream_checksum;

// Auto-detect format
let checksum = extract_stream_checksum("file.gz")?;
if let Some(crc32) = checksum.crc32 {
    println!("Stream CRC32: {:08X}", crc32);
}

// Format-specific
let gz = extract_gzip_stream_crc("file.gz")?;
let bz = extract_bzip2_stream_crc("file.bz2")?;
let xz = extract_xz_stream_check("file.xz")?;
```

**Important**: These helpers are stream-checksum utilities that read stored metadata from compression format headers/trailers. They do NOT provide standalone archive-opening support for .gz/.bz2/.xz files, and they do NOT recompute checksums for verification. `extract_stream_checksum` detects the format from magic bytes only — the file extension is never consulted, and files whose magic matches no supported format are rejected regardless of extension.

**Supported Formats**:
- ✅ GZIP - Full CRC32 + size extraction (reads stored metadata from trailer); multi-member files report the last member's trailer only
- ✅ BZIP2 - Full CRC32 extraction (reads stored metadata from EOS marker)
- ✅ XZ - Check type detection only (CRC32/CRC64/SHA-256/None); actual check value is not extracted

#### Stored Per-Entry Checksums (All Archives)

Per-entry CRC32 values are read directly from archive metadata — they are stored by the archiving tool at creation time, not recomputed by unified-archive.

```rust
use unified_archive::Archive;

let archive = Archive::open("file.zip")?; // or .7z, .rar, .tar.gz
for entry in archive.list_files()? {
    if let Some(crc32) = entry.crc32 {
        println!("{}: {:08X}", entry.path, crc32);
    }
}
```

**Supported Formats**:
- ✅ RAR/RAR5 - Direct from UnRAR SDK
- ✅ ZIP - Stored metadata read from the `zip` crate native backend
- ✅ 7z - Stored metadata read from SevenZ native backend
- ❌ TAR and the other libarchive-backed formats - the format stores no
  per-entry CRC32 and the libarchive reader sets `crc32 = None` on every
  entry; `calculate_manifest_digest` recomputes content CRCs for these
  instead (see **Manifest Digest** under "Future Enhancements")

**Caveat:** AES-encrypted ZIP entries written in AE-2 mode store `0` in the
central-directory CRC32 field by specification, so that field is a placeholder
rather than a content checksum. The question of how those entries should
surface a CRC32 in the listing has been decided and implemented: **they list
`crc32 = None`.** The gate is `crc32_check_exempt` in
`src/ffi/zip_wrapper.rs`, and it is a **three-way** conjunction:

```text
zip_file.encrypted()
    && zip_file.crc32() == 0
    && aes_vendor_version(zip_file) == Some(AE2)
```

The third conjunct reads the vendor version out of the entry's `0x9901`
WinZip-AES extra field (`0x0001` = AE-1, which stores a real CRC32; `0x0002` =
AE-2, which stores none), and it is load-bearing rather than a belt-and-braces
extra. `encrypted() && crc32() == 0` alone is a *superset* of AE-2: it also
sweeps in any encrypted entry whose payload is genuinely **empty** — a legacy
ZipCrypto empty file, or an AE-1 empty file from a writer that does not follow
the `zip` crate's "under 20 bytes ⇒ AE-2" rule. Those carry a real stored
`CRC32(b"") == 0`, which AD 0012 says is a valid checksum and not an absent
one; exempting them would discard a checksum the archive actually carried and
would force a needless decrypt-and-stream during the digest walk. With the
vendor-version conjunct they keep `Some(0)`, as does any plaintext empty file.
`test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32` in
`src/ffi/zip_wrapper.rs` exists solely to pin that seam.

Extraction likewise skips the CRC comparison for exempt entries and relies on
the AES authentication tag instead. There is no placeholder in the listing to
compare against, so nothing built on `entry.crc32` can silently treat AE-2
entries as matching.

One residual is stated rather than hidden: an AE-2 entry whose central record
omits the `0x9901` field is not recognised by the gate. Such an entry is
unreadable anyway — the `zip` crate rejects "AES encryption without AES extra
data field" while parsing the central directory — so it never reaches a CRC
comparison.

The cost of that correctness is a **password precondition on the digest
surface**. Because AE-2 entries expose no stored CRC32,
`calculate_content_multiset_digest_and_size` (and its
`calculate_manifest_digest` / `calculate_manifest_summary` shims) falls through
to streaming the entry's payload, which means decrypting it. Listing an
encrypted ZIP still needs no password — only reading entry data does — so a
digest call on a password-protected ZIP opened without a usable password
**fails** where it previously returned a digest computed over the placeholder.
Open such archives with `Archive::open_encrypted()`. The refusal currently
arrives as `ArchiveError::Format` carrying `"… Password required to decrypt
file"` rather than as `ArchiveError::Password`; that mis-classification is
pre-existing and tracked separately (ti-9bdf2c), so match on it defensively
rather than reading it as the intended classification.

### Future Enhancements

**Potential additions**:
1. **7z Packed Stream CRC**: Extract compressed stream checksums
2. **7z Header CRC**: Verify archive structure integrity
3. **RAR5 BLAKE2**: Extract BLAKE2sp-256 hash when available
4. **XZ Full Check**: Parse blocks to extract actual CRC32/CRC64 values

---

#### Derived Checksums

unified-archive provides two derived checksums computed from per-entry metadata. Neither is stored in the archive — both are calculated on-the-fly.

**Archive CRC** (`calculate_archive_crc`):
```rust
let crc = archive.calculate_archive_crc()?;
println!("Archive CRC: {:08X}", crc);
```
Wrapping sum of all per-file CRC32 values. Matches 7-Zip's "Archive CRC" display. Simple and fast, but wrapping addition is prone to collisions (e.g., swapping two files' CRCs doesn't change the sum).

**Manifest Digest** (`calculate_manifest_digest`):
```rust
let digest = archive.calculate_manifest_digest()?;
println!("Manifest digest: {}", digest); // e.g. "a1b2c3d4"
```
Sorts per-entry identifier strings, joins with `,`, then CRC32-hashes the result. For entries with a stored CRC32, the hex-formatted CRC is used as-is; for entries whose format does not carry a CRC32 in metadata (e.g., TAR, TAR+gz/bz2/xz, ISO), the decompressed payload is streamed through `compute_crc32_reader()` and the resulting CRC feeds the digest — see AD 0047. Where the backend offers a one-pass walk that streaming happens for all such entries in a single traversal (`resolve_crc32_single_pass`); otherwise it falls back to the per-entry `entry_crc32_for_digest()`. Either way, errors during the stream are propagated, never silently substituted. More collision-resistant than archive CRC because sorting preserves per-entry identity instead of collapsing it into a sum.

**Comparison:**

| Property | Archive CRC | Manifest Digest |
|----------|-------------|-----------------|
| Algorithm | Wrapping sum | Sort + join + CRC32 hash |
| Output | `u32` | 8-char hex string |
| Collision resistance | Low (addition is lossy) | Higher (preserves per-entry identity) |
| Use case | Quick comparison, 7-Zip compatibility | Archive comparison and deduplication (unique-path archives; see the caveats below) |
| Empty archive | `0` | `""` (empty string) |

Both properties are qualified rather than absolute:

- **Order-independence** holds unconditionally, duplicate paths included (DCR-012). Archives may legitimately repeat a path (TAR append/update, shadowed ZIP/7z central-directory entries), and every entry — first occurrence or fifth — contributes the same bare `{crc32:08x}` element for its own payload; multiplicity is carried by that element appearing once per occurrence in the sorted multiset. An earlier encoding appended a per-path occurrence ordinal (`<crc32-hex>#<n>`) from the second occurrence onward, which made duplicate-path digests depend on the relative listing order of the same-path occurrences; that ordinal was removed, so **digests stored under it do not match current ones for duplicate-path archives and must be recomputed**. Unique-path archives digest exactly as before. See the rustdoc on `Archive::calculate_content_multiset_digest_and_size` (`src/inspection.rs`), of which `calculate_manifest_digest` is a thin shim.
- **Format-independence** holds only when every entry of every archive being compared exposes a stored CRC32 in its listing. ZIP, 7z, and RAR do, so identical contents in those formats produce equal results — the exception is the AE-2 entries above, which list `None`; the manifest digest recovers format-independence for them by streaming the decrypted payload (given the password), while `calculate_archive_crc` simply skips them. For `calculate_archive_crc` a returned `0` is additionally ambiguous between an empty archive, an archive whose entries expose no CRC32, and a genuine wrap to zero — see the rustdoc on `Archive::calculate_archive_crc`.

On CRC-less formats (TAR and friends), the manifest digest streams entry content through `compute_crc32_reader()` (AD 0047), so the result is still content-identity — no path/size surrogate. The trade-off is a full pass over entry payloads, and that pass is **linear**, not quadratic: since OI-0001-009 (ticgit `82bf8fd4`) the libarchive backend resolves every CRC-less entry in a single traversal (`LibarchiveArchive::visit_payloads_by_listing_id`, reached through `Archive::resolve_crc32_single_pass` in `src/inspection.rs`), so a compressed TAR is decompressed **once** rather than re-decompressed from byte zero once per member. The per-entry resolver `entry_crc32_for_digest()` remains as the fallback for a backend that offers no one-pass walk; it produces the identical digest, just more slowly. Errors during either route are propagated, never silently substituted. Prefer `calculate_archive_crc` when a cheap summary suffices — the digest still reads every payload, and reports no progress.

---

## Use Cases

### When to Use Stream Checksums

**Good for**:
- Metadata inspection and checksum retrieval from GZIP/BZIP2/XZ files without decompression
- Quick sanity check of compressed files (comparing stored checksums against known values)
- Detecting transmission errors (by comparing stored checksum with a previously recorded value)
- Comparing compressed file versions

**Warning**: Reading a stored checksum does NOT verify data integrity on its own. True verification requires recomputing the checksum over the actual data and comparing it against the stored value.

**Example**:
```rust
// Retrieve stored checksum for comparison (does not recompute)
let crc = extract_gzip_stream_crc("backup.sql.gz")?;
println!("Stored CRC32: {:08X}", crc.crc32.unwrap());
// Compare with a previously recorded known value
assert_eq!(crc.crc32.unwrap(), 0x9A0D2606);
```

### When to Use Per-File CRC32

**Good for**:
- Verifying individual files in archives
- Detecting corrupted files after extraction
- Comparing file versions across archives
- Building file checksums database

**Example**:
```rust
// Verify all files in archive have valid CRC32
let archive = Archive::open("backup.zip")?;
let report = archive.validate_integrity()?;
if report.failed.is_empty() {
    println!("All {} files verified", report.validated);
}
```

### When to Use Manifest Digest

**Good for**:
- Deduplicating archives across different formats (ZIP vs 7z vs RAR with same files)
- Content-identity matching regardless of compression method or entry order
- Building a content-addressable archive index

**Example**:
```rust
let zip = Archive::open("backup.zip")?;
let sevenz = Archive::open("backup.7z")?;

let d1 = zip.calculate_manifest_digest()?;
let d2 = sevenz.calculate_manifest_digest()?;

if d1 == d2 {
    println!("Archives contain identical files");
}
```

---

## Technical Details

### CRC32 Algorithm Variants

**Standard**: CRC-32/ISO-HDLC
- Polynomial: `0x04C11DB7`
- Width: 32 bits
- Used by: GZIP, ZIP, 7z, RAR

**BZIP2 Variant**: CRC-32/BZIP2
- Same polynomial: `0x04C11DB7`
- Different reflection: No input/output reflection
- GZIP uses reflection, BZIP2 doesn't

**XZ CRC64**: CRC-64/XZ
- Polynomial: ECMA-182
- Width: 64 bits
- Most common in XZ files

### Endianness

- **GZIP**: Little-endian
- **BZIP2**: Big-endian
- **ZIP**: Little-endian
- **7z**: Little-endian (usually)
- **RAR**: Little-endian

---

## References

- [RFC 1952](https://www.rfc-editor.org/rfc/rfc1952.html) - GZIP File Format Specification
- [BZIP2 Format](http://www.bzip.org/) - BZIP2 Official Documentation
- [XZ Format](https://tukaani.org/xz/xz-file-format.txt) - XZ File Format 1.2.1
- [PKWARE APPNOTE.TXT](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT) - ZIP Format Specification
- [7z Format](https://www.7-zip.org/7z.html) - Official 7-Zip Archive Format Specification
- [RAR5 Format](https://www.rarlab.com/technote.htm) - RAR 5.0 Archive Format

---

**Last Updated**: 2026-06-10
**Version**: 0.4.0
