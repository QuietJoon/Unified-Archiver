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

**Important**: These helpers are stream-checksum utilities that read stored metadata from compression format headers/trailers. They do NOT provide standalone archive-opening support for .gz/.bz2/.xz files, and they do NOT recompute checksums for verification.

**Supported Formats**:
- ✅ GZIP - Full CRC32 + size extraction (reads stored metadata from trailer)
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
- ✅ ZIP - Stored metadata read from Piz native backend
- ✅ 7z - Stored metadata read from SevenZ native backend
- ✅ TAR - Read during listing via libarchive

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
Sorts per-entry identifier strings, joins with `,`, then CRC32-hashes the result. For entries with a CRC32, the hex-formatted CRC is used; for entries without CRC32 (e.g., TAR entries), the function falls back to `path:size` as the identifier. More collision-resistant than archive CRC because sorting preserves per-entry identity instead of collapsing it into a sum.

**Comparison:**

| Property | Archive CRC | Manifest Digest |
|----------|-------------|-----------------|
| Algorithm | Wrapping sum | Sort + join + CRC32 hash |
| Output | `u32` | 8-char hex string |
| Collision resistance | Low (addition is lossy) | Higher (preserves per-entry identity) |
| Use case | Quick comparison, 7-Zip compatibility | Archive comparison and deduplication |
| Empty archive | `0` | `""` (empty string) |

Both are order-independent and format-independent. When all entries have CRC32 values, the same file contents produce the same result whether stored in ZIP, 7z, or RAR. However, when the `path:size` fallback is used (for entries lacking CRC32), the digest incorporates path and size metadata rather than pure content identity, so results may differ across formats if paths or metadata vary.

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

**Last Updated**: 2025-11-01
**Version**: 0.1.0
