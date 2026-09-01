---
type: API Reference
title: "API Reference"
description: "API reference for the unified-archive library."
tags: [reference, ADR-0001, ADR-0019, ADR-0018, ADR-0029, ADR-0062, ADR-0027, ADR-0064, ADR-0053, R0075-0083, R0074-0081, R0075-0081, R0075-0078]
timestamp: 2026-06-10T00:00:00Z
status: active
---

# API Reference

API reference for the unified-archive library. Covers inspection, extraction, creation, modification, SFX detection, streaming, and safety utilities.

For a task-oriented guide, see the [User Manual](./USER_MANUAL.md).

## Internal types

Per AD 0001 (Single Archive Facade with Backend Enum), the supported public surface
is the `Archive` facade together with the domain types listed in the table of contents
below. The following items are `pub` only because their parent module is `pub mod`,
not because they are part of the supported API:

- `unified_archive::ffi::UnrarArchive`
- `unified_archive::ffi::LibarchiveArchive`
- `unified_archive::ffi::ZipArchive`
- `unified_archive::ffi::SevenZArchive`
- `unified_archive::ffi::ZipWriter`

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
- [Typed Handle API (`v2-api` feature)](#typed-handle-api-v2-api-feature)
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

**Supported formats:** RAR, RAR5, ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST, TAR.LZ4, TAR.LZMA, GZIP, BZIP2, XZ, ZST, LZ4, LZMA, ISO

> **Note:** Standalone `.gz`/`.bz2`/`.xz`/`.zst`/`.lz4`/`.lzma` files are supported for
> read/extract via libarchive's `format_raw` binding (MADR-0019); the single entry is renamed
> to the archive's file stem. Creation of standalone compressed files remains out of scope
> per AD 0018 — use the creatable TAR compound variants (`.tar.gz`, `.tar.bz2`, `.tar.xz`)
> to produce compressed archives. Raw `.lzma` / `.tar.lzma` / `.tlz` use an extension
> fallback because raw LZMA streams have no stable short magic marker.

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

Get count of entries. In Read / Modify mode this is equivalent to
`list_files()?.len()`. In Write mode this is the number of entries the
writer has already emitted through successful `add_*` calls.

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

> **Duplicates:** `find_entry` returns the **first** match. Archives
> may legitimately contain multiple entries with the same path
> (especially after rewrite-based modification). Use `find_entries`
> when duplicate-path semantics matter.

---

#### `Archive::find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>>`

Return every entry whose path matches `path` exactly. Order is the
same as `list_files()`. Use this in preference to `find_entry` when
the archive may contain duplicate paths or when modification flows
need to address a specific occurrence by index.

---

#### `Archive::extension_format(&self) -> Option<ArchiveFormat>`

Format inferred purely from the archive's path extension, with no
header read. Useful for diagnostics that compare the extension-derived
format against [`Archive::format()`](#archiveformat-1) (the
content-derived format) — a mismatch usually means the file was
renamed.

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
    report.total_files
);

if !report.failed.is_empty() {
    println!("Failed: {:?}", report.failed);
}
```

**Returns:** `ValidationReport` with validation results

**Note:** Verification behavior varies by backend: UnRAR uses native test mode (`RAR_TEST`), ZIP/7z extract entries to memory and verify CRC32, libarchive streams entries and checks read errors.

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

> **Contract:** the digest is a *content-multiset* hash — archives with
> the same per-entry content produce the same digest regardless of
> filenames, directory layout, modification times, permissions, or
> entry order. Collision resistance is CRC32-grade (better than
> `calculate_archive_crc`'s wrapping sum, but not cryptographically
> strong).

**Algorithm:**
1. For each file entry (directories excluded), resolve a CRC32:
   - If the listing exposes `entry.crc32` (ZIP, 7z, RAR5): use it directly.
   - Otherwise: stream the entry's bytes through a CRC32 hasher. Two
     populations reach this branch, not one. The libarchive-backed
     formats (TAR family, ISO, raw compressed streams) carry no
     per-entry CRC32 metadata at all. **AE-2 AES ZIP entries also land
     here:** AE-2 stores `0` in the central-directory CRC32 field by
     specification, so the ZIP backend lists those entries as `None`
     rather than surfacing a placeholder. The gate is
     `crc32_check_exempt` in `src/ffi/zip_wrapper.rs`, and it is a
     **three-way** conjunction: `encrypted() && crc32() == 0 &&
     aes_vendor_version(zip_file) == Some(AE2)`, where the vendor version
     is read out of the entry's `0x9901` WinZip-AES extra field. The
     third conjunct is load-bearing rather than decorative — without it
     the gate is a *superset* of AE-2 and also sweeps in an encrypted
     entry whose payload is genuinely **empty** (a ZipCrypto empty file,
     or an AE-1 empty file), whose stored `CRC32(b"") == 0` is a real
     checksum and not a placeholder. Those keep `Some(0)` per AD 0012, as
     does any plaintext empty file. ZIP therefore does **not** always take
     the metadata branch, but only genuine AE-2 entries leave it.
2. Convert each CRC32 to 8-char hex
3. Sort all identity strings lexicographically
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

**Returns:** `String` — 8-char hex digest, or empty string if the archive contains no file entries

**Truncated entries (DCR-011):** in step 1 the payload stream is held to
the listing's declared size **exactly**. A CRC-less entry whose payload
ends before its declaration — the shape a truncated TAR/CPIO/ISO member
takes, where no per-entry checksum exists to catch it — returns
`ArchiveError::Corruption` naming that entry rather than digesting the
short payload; over-production past the declaration returns the same
error. The digest surface is therefore a truncation detector on those
formats, not only a content hash. Entries whose listing carries no size
(raw gzip/bzip2/xz single-file readers) get no invented declaration: they
keep a ceiling-only bound and an early end of stream stays an ordinary
EOF. The digest **value** for a healthy archive is unchanged. The same
behaviour applies to `calculate_content_multiset_digest_and_size` and
`calculate_manifest_summary`.

**Precondition on AE-2 AES ZIP archives:** because step 1 has to stream
those entries, it has to **decrypt** them, so the digest requires a
usable password. Listing an encrypted ZIP does not — only reading entry
data does — so a digest call on a password-protected ZIP opened through
plain `Archive::open()` returns an error where it previously returned a
digest computed over the placeholder `0`. Open with
`Archive::open_encrypted()` instead. The refusal currently surfaces as
`ArchiveError::Format` carrying `"… Password required to decrypt file"`
rather than as `ArchiveError::Password`; that mis-classification predates
this behaviour and is tracked separately (ti-9bdf2c), so treat the
variant as unsettled. The upside of the same change is that the digest
is now genuinely content-sensitive on AE-2 archives: two of them with
identical paths and sizes but different contents no longer collide.

**Performance:** On every format that reaches step 1's streaming branch —
the libarchive-backed formats listed above **and** AE-2 AES ZIP entries —
each file entry is decompressed (and, for AE-2, decrypted) to compute
CRC32. Since OI-0001-009 (ticgit `82bf8fd4`) the libarchive backend
resolves **all** of its CRC-less entries in a single traversal
(`visit_payloads_by_listing_id`) rather than re-opening the archive once
per entry, so a compressed TAR is decompressed **once**, not once per
member, and the cost is linear in the archive's bytes rather than
quadratic in its entry count. Still prefer `calculate_archive_crc` when a
cheaper summary suffices — this call reads every payload either way, and
reports no progress. AE-2 AES ZIP entries are the one case where a
CRC-carrying format pays payload cost; they are resolved by random-access
seek into the central directory, not by a sequential walk. A ZIP with no
AE-2 entries keeps the cheap metadata-only path.

Used by AdvancedDeduplicator for content-identity matching.

---

#### `Archive::detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)>`

Detect if this archive is part of a multi-part archive set. Returns `(is_multipart, part_files)` where `part_files` is the sorted list of all detected parts.

> Prefer [`Archive::multipart_layout()`](#archivemultipart_layout) for
> new code: it returns a typed `MultipartLayout` instead of a tuple.

---

#### `Archive::multipart_layout(&self) -> Result<MultipartLayout>`

Typed multipart layout (R0075-0083). Returns
`MultipartLayout::Single { path }` for a single archive or
`MultipartLayout::Multi { parts }` for a detected multipart set.
Replaces the boolean-tuple return of `detect_multipart`.

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

#### `Archive::extract_all(&self, options: ExtractionOptions) -> Result<ResultWithWarnings<()>>`

Extract all files from the archive.

**Example:**
```rust
let options = ExtractionOptions::new("./output")
    .preserve_permissions(true)
    .preserve_times(true);

// extract_all returns ResultWithWarnings<()>. A successful return still
// carries warnings — e.g., SkippedSymlink / SkippedHardLink entries.
let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

**Parameters:**
- `options` - Extraction configuration (see `ExtractionOptions`)

**Return:** `Result<ResultWithWarnings<()>, ArchiveError>`. The warnings vector contains non-fatal events (skipped symlinks or hard-links on unsupported backends) surfaced from a successful extraction.

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

archive.extract_file(
    "document.pdf",
    ExtractionOptions::new("./output"),
)?;
```

**Parameters:**
- `file_path` - Path of file within archive
- `options` - Extraction options including destination directory, password, overwrite, etc.

**Errors:**
- File not found in archive: the exact error variant is backend-dependent (`ArchiveError::Format` for most backends, `ArchiveError::Io` for some)
- Other errors same as `extract_all()`

---

#### `Archive::extract_filtered<F>(&self, predicate: F, options: ExtractionOptions) -> Result<ResultWithWarnings<()>>`

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
- `F: FnMut(&ArchiveEntry) -> bool`

**Parameters:**
- `predicate` - Function returning `true` for files to extract
- `options` - Extraction configuration

**Performance:** Sequential per-entry extraction. (Earlier docs referenced a rayon-based parallel path; the current implementation does not depend on rayon.)

---

#### `Archive::extract_files(&self, paths: &[&str], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>`

Extract multiple files by their paths within the archive.

**Performance:** Sequential per-entry extraction; passes through `extract_some` (AD 0029).

---

#### `Archive::extract_by_ids(&self, ids: &[usize], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>`

Extract multiple files by their sequential entry IDs (0-based, as returned by `list_files()`). Useful when entries have already been looked up by ID rather than path.

**Performance:** Sequential per-entry extraction; same dispatch as `extract_files()`.

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

#### `Archive::extract_to_stream(&self, file_path: &str, bound: StreamBound) -> Result<StreamingExtractor>`

Extract a file as a readable stream whose output cap is chosen by an
explicit `StreamBound` (I2 / AD 0062 A.2). `StreamBound` is a public
enum with three variants:

- `StreamBound::DeclaredSize` — hold the returned `StreamingExtractor`
  to **exactly** the entry's declared uncompressed size, taken from the
  authoritative preflight listing. Entries that declare no size (raw
  gzip/bzip2/xz readers, libarchive entries with an unset size field)
  degrade to a ceiling-only cap at the effective entry limit,
  `min(max_file_size, max_total_size)`. This is the safe default for
  untrusted input.
- `StreamBound::Cap(n)` — ceiling-only cap at `n` bytes: a **total-resource
  assertion** ("no more than `n` bytes may exist on my behalf, anywhere"),
  not an assertion about the entry's size and not a read window. An entry
  smaller than `n` reaches a normal EOF. For a window of a larger entry use
  `DeclaredSize` + `Read::take(n)`, and size `ExtractionLimits` to what you
  are willing to have materialized on a staging backend.
- `StreamBound::Unbounded` — no caller-chosen output cap; the reader
  forwards bytes verbatim and trusts the source.

Under a hard cap (`DeclaredSize` or `Cap`), an over-producing decoder
does not reach a clean EOF at the cap: the next `read` returns an
`io::ErrorKind::InvalidData` error, so a caller can distinguish a
complete entry from one cut off at the limit (R0080-0007 / DCR-006).
Under `DeclaredSize` with a known declared size the converse also holds:
a stream that ends *before* the declaration returns
`io::ErrorKind::UnexpectedEof` rather than a short read, so a truncated
entry in a checksum-less format (plain TAR, CPIO, ISO) cannot read as a
short-but-valid payload (R0001-0011 / OI-0001-001). Both verdicts are
sticky — a retried read reports the same error again instead of falling
through to `Ok(0)`, and the retry does not consume another byte from the
underlying stream (DCR-006 Amendment 4).

The bound also shapes what the backend may materialize while preparing
the reader: it receives `min(bound, max_file_size, max_total_size)`
(R0001-0011). The caller's limits are a hard ceiling a `StreamBound` may
tighten but never loosen.

##### What is actually bounded, per backend

| Backend | What the budget bounds | `Cap(n)` below the declared size |
|---|---|---|
| libarchive (TAR family, ISO, raw gzip/bzip2/xz) | the reader itself — the wrapper *is* the materialization bound | serves a prefix of `n` bytes; reading past `n` is `InvalidData` |
| ZIP | the staged `Vec<u8>`, refused before it is reserved | refused at the extract call, `ArchiveError::OperationBlocked` |
| 7z | the staged `Vec<u8>`, refused before it is reserved | refused at the extract call, `OperationBlocked` |
| RAR | the staged temporary file, refused before `RARProcessFile` runs | refused at the extract call, `OperationBlocked` |

All three staging backends judge `Cap(n)` on the entry's **declared** size,
before any decode (DCR-006 Amendment 4). Entries whose header declares no
size skip that pre-check — no declaration is invented — and are caught by
each backend's decoded-byte backstop instead. To read a window of a larger
entry portably, wrap a `DeclaredSize` stream in `Read::take`.

##### Where a violation surfaces

**The guarantee differs by bound.** Under `StreamBound::DeclaredSize`
every backend holds the entry to its declaration, so a violation surfaces
**no later than the read that observes it, and never as silent success**.
A backend that observes it while materializing reports it earlier, as a
typed `ArchiveError` from the extract call itself — there, call-time
reporting is an earlier delivery of the same verdict, and the two
deliveries pair:

| Violation (under `DeclaredSize`) | Call-time (ZIP / 7z / RAR) | Read-time (libarchive) |
|---|---|---|
| budget exceeded | `ArchiveError::OperationBlocked` | `io::ErrorKind::InvalidData` at the cap |
| truncation under the declaration | `ArchiveError::Corruption` | `io::ErrorKind::UnexpectedEof` |
| over-production past the declaration | `ArchiveError::Corruption` | `io::ErrorKind::InvalidData` |

Which site fires is not promised to stay fixed: when DEF-004 lands genuine
incremental readers for the staging backends, those violations move from
call time to read time and the contract above still holds.

Under `Cap(n)` and `Unbounded`, **only the budget row is uniform.**
Declaration integrity is not what those bounds ask for, and what you get
differs by backend as a side effect of how each materializes: ZIP, 7z and
RAR still enforce the declaration (staging is how they work — ZIP and 7z
stage with exact-size enforcement, RAR length-checks its staged payload),
so a truncated entry is a call-time `Corruption` even under `Unbounded`;
libarchive does not, and the same entry reads to a **clean short EOF**.

That is a different verdict, not an earlier one — which is why
`DeclaredSize` is the safe default for untrusted input. `Cap(n)` is a
resource budget and answers only the budget question. For a read window
*with* the integrity check, use `DeclaredSize` plus `Read::take`.

Choosing "unbounded" as one value of `StreamBound` — rather than a
separate `_unbounded` method — is what structurally retires the
R0081-0027 drift, where two former unbounded methods disagreed on how
the mmap cap was handled.

**Example — one handler covering both arrival points:**
```rust
use std::io::{ErrorKind, Read};
use unified_archive::{ArchiveError, StreamBound};

let mut stream = match archive.extract_to_stream("large.bin", StreamBound::DeclaredSize) {
    Ok(s) => s,
    // Arrival point 1: a staging backend saw the violation while materializing.
    Err(e @ (ArchiveError::Corruption { .. } | ArchiveError::OperationBlocked { .. })) => {
        eprintln!("damaged or over-budget entry: {e}");
        return Ok(());
    }
    Err(e) => return Err(e.into()),
};

let mut buffer = [0u8; 8192];
// Propagate read errors — corruption and I/O failures surface as
// `Err`, which a `while let Ok(..)` loop would swallow as EOF.
loop {
    match stream.read(&mut buffer) {
        Ok(0) => break,
        Ok(_n) => { /* process chunk */ }
        // Arrival point 2: the incremental path saw it on a read.
        Err(e) if matches!(e.kind(), ErrorKind::UnexpectedEof | ErrorKind::InvalidData) => {
            eprintln!("damaged entry: {e}");
            break;
        }
        Err(e) => return Err(e.into()),
    }
}
```

**Parameters:**
- `file_path` - Path of file within archive
- `bound` - Output-size bound (`StreamBound::{DeclaredSize, Cap, Unbounded}`)

**Returns:** `StreamingExtractor` (implements `std::io::Read`). Under `DeclaredSize` / `Cap` it is hard-capped so over-production surfaces as a read error rather than a silent EOF; under `DeclaredSize` with a known declared size it is exact-length, so under-production surfaces as `UnexpectedEof`; under `Unbounded` it forwards bytes verbatim.

**Memory Usage:** Memory usage depends on backend: libarchive (TAR family) truly streams with bounded memory. ZIP, 7z (SevenZ), and RAR (UnRAR) backends buffer the full entry in memory before wrapping in a `StreamingExtractor`.

**Options-carrying variant:**
- `Archive::extract_to_stream_with_options(file_path, &options, bound)`
  additionally honours `options.password`, `options.limits`, and
  `options.verify_crc32`. `StreamBound::DeclaredSize` uses the effective
  entry ceiling —
  `min(options.limits.max_file_size, options.limits.max_total_size)` — as
  the unknown-size hard-cap ceiling, and the same ceiling bounds the
  backend materialization step under every bound.

---

### Creation

#### `Archive::create(path: impl AsRef<Path>, options: CompressionOptions) -> Result<Archive>`

Create a new archive for writing. The returned handle is in Write mode. Fails if the output file already exists.

**Supported formats:** ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ.

**Important:** `CompressionOptions.password` is rejected by `Archive::create()` in the current main facade. The library reads encrypted archives, but the main creation facade does not produce encrypted archives.

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

#### `Archive::create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Archive>`

Typed entrypoint for ZIP creation. Pairs with `ZipCompressionOptions`
(R0075-0081); fields the ZIP backend cannot honour (such as 7z-only
options) are absent from the builder, so misconfigurations are caught
at compile time. Functionally equivalent to
`Archive::create(path, opts.into())`.

---

#### `Archive::create_seven_zip(path: impl AsRef<Path>, opts: SevenZCompressionOptions) -> Result<Archive>`

Typed entrypoint for 7-Zip creation. Pairs with `SevenZCompressionOptions`.
The 7-Zip builder surfaces `password` because 7-Zip is the only
currently-creatable format that will eventually accept it; today
`password.is_some()` is rejected at this call per MADR-0027.

---

#### `Archive::create_libarchive(path: impl AsRef<Path>, opts: LibarchiveCompressionOptions) -> Result<Archive>`

Typed entrypoint for libarchive-backed creation (TAR / TAR.GZ /
TAR.BZ2 / TAR.XZ today). ZIP is rejected explicitly — use
`create_zip` or `Archive::create`. ISO is read-only and not accepted.

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

Finalize and close the archive. Must be called for Write mode archives
to flush pending data. No-op for Read mode.

For Modify mode the call is rejected with
`ArchiveError::OperationBlocked` when there are pending operations —
modify-mode handles drop queued additions/removals on `Drop` rather
than silently committing on `finish`. Call `Archive::commit_changes`
to apply queued operations or `Archive::clear_operations` to discard
them before finishing. Modify-mode handles with no pending operations
finish silently.

---

#### `CompressionOptions`

```rust
#[non_exhaustive]
pub struct CompressionOptions {
    pub format: ArchiveFormat,
    pub level: CompressionLevel,
    pub password: Option<SecStr>,
    pub split_size: Option<u64>,
    pub progress: Option<Box<dyn ProgressCallback>>,
}
```

Construct with `CompressionOptions::new(format)`. Default level is `CompressionLevel::Normal`.

**Field behavior note:** `password` exists on the type because the crate also supports encrypted-read flows and optional external integrations, but `Archive::create()` rejects password-based archive creation in the current main facade with `ArchiveError::OperationBlocked`.

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

Open an existing archive for modification. Changes are tracked in memory and applied when `commit_changes()` is called. Uses a copy-on-write strategy internally: the archive is rewritten from the surviving entries, so symlinks, hardlinks, and special entries are dropped and metadata/layout may be normalized. This is why ZIP and 7z report `modification: Support::Partial` rather than `Full` (R0080-0048).

**Supported formats:** ZIP, 7z. RAR is read-only; TAR is not yet supported.

---

#### `Archive::modify_with_options(path: impl AsRef<Path>, options: ModificationOptions) -> Result<Archive>`

Open an existing archive for modification with explicit options. Same as `modify()` but allows configuring backup behavior and metadata preservation up front.

---

#### `Archive::add_entry(&mut self, path: &str, data: &[u8]) -> Result<()>`

Track a new file entry for addition. Only available in Modify mode (use `add_file_from_data()` in Write mode).

---

#### `Archive::add_entry_from_path(&mut self, archive_path: &str, source: &Path) -> Result<()>`

Track a new entry whose contents come from a live file on disk. The
source is read at `commit_changes()` time, not at the call.

---

#### `Archive::add_entry_from_reader<R: Read + Send + 'static>(&mut self, archive_path: &str, reader: R, expected_size: Option<u64>) -> Result<()>`

Track a new entry whose contents come from an arbitrary `Read`. When
`expected_size` is `Some`, that value is the declared entry size and
under/over-production fails at `commit_changes()` time; when `None`,
the reader is staged to learn its size with bounded memory.

---

#### `Archive::remove_entry(&mut self, path: &str) -> Result<usize>`

Mark every source entry with this path for removal, returning the number of entries that matched. A return value of `0` means the path did not appear in the source listing. Applied when `commit_changes()` is called.

---

#### `Archive::remove_entry_by_id(&mut self, id: usize) -> Result<()>`

Remove a specific entry by its `ArchiveEntry::id`. Used to disambiguate
duplicate-path entries when more than one entry shares the same path.

---

#### `Archive::replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()>`

Convenience method: removes the old entry and adds a new one with the same path and new data.

---

#### `Archive::replace_entry_from_path(&mut self, archive_path: &str, source: &Path) -> Result<()>`

Replace an entry by reading replacement data from a file on disk. See `add_entry_from_path` for sourcing semantics.

---

#### `Archive::replace_entry_from_reader<R: Read + Send + 'static>(&mut self, archive_path: &str, reader: R, expected_size: Option<u64>) -> Result<()>`

Replace an entry by reading replacement data from a `Read`. See `add_entry_from_reader` for sizing semantics.

---

#### `Archive::add_directory_entry(&mut self, path: &str) -> Result<()>`

Track a new empty directory entry for addition. Only available in Modify mode.

---

#### `Archive::pending_operations(&self) -> Result<usize>`

Get the number of pending modification operations (additions, removals, replacements) that will be applied on `commit_changes()`. Returns `OperationBlocked` if the archive is not in Modify mode.

---

#### `Archive::clear_operations(&mut self) -> Result<()>`

Discard all pending modification operations without committing them. Returns `OperationBlocked` if the archive is not in Modify mode.

---

#### `Archive::commit_changes(self) -> Result<()>`

Apply all tracked modifications (additions, removals, replacements). Creates a new archive, copies non-removed entries, adds new entries, then replaces the original file.

---

#### `ModificationOptions`

```rust
#[non_exhaustive]
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

Enable backup creation before committing changes. The original archive
is copied to `<archive_path><normalized_suffix>` before the atomic
rename. The suffix is normalized so a leading dot is optional —
`with_backup("bak")` and `with_backup(".bak")` both produce
`archive.bak`, never `archive..bak`.

#### `ModificationOptions::without_metadata_preservation(self) -> Self`

Disable metadata preservation during the copy phase. When enabled
(default), `commit_changes()` preserves modified, accessed, and created
timestamps plus Unix permissions on retained regular-file entries
(where the backend supports the timestamp). Directory metadata still
uses backend defaults.

#### `compression: Option<CompressionOptions>`

Override compression settings for the recreated archive. When `None` (default), uses format defaults (`CompressionLevel::Normal`, no password). Use this to control the rewritten archive's compression settings; password-based creation is still rejected by the current main facade.

---

### SFX (Self-Extracting Archives)

#### `Archive::detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>`

Detect if a file is a self-extracting archive. Uses 3-stage detection: executable validation, signature scan, heuristic offset screening. Stage 3 is a cheap plausibility check on the format prefix at the candidate offset — full archive validation runs later when callers invoke `Archive::open_sfx()`. Scans first 1MB only.

**Performance:** Typical detection 15-70ms; non-executable files <1ms (early exit).

---

#### `Archive::open_sfx(path: impl AsRef<Path>) -> Result<Archive>`

Convenience method: detect SFX and open the embedded archive in one call. Equivalent to `detect_sfx()` + `open_at_offset()`.

The embedded payload is materialized to a temporary file and opened through the normal archive pipeline.

---

#### `Archive::open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Archive>`

Open an archive that starts at a specific byte offset within a file. Primarily for SFX archives. `offset == 0` behaves like `Archive::open()`. Non-zero offsets materialize the payload to a temporary file and then open it.

---

#### `Archive::extract_stub(path: impl AsRef<Path>, detection: &SfxDetectionResult) -> Result<Vec<u8>>`

Extract the executable stub from an SFX archive for security analysis. The stub is
bounded in practice by detection, not by the 50 MiB `MAX_STUB_SIZE` constant: the
call re-runs `detect_sfx` and honours only an offset the fresh probe reproduces, and
detection never looks past the 1 MiB scan window, so the returned stub cannot exceed
that window today. `MAX_STUB_SIZE` remains as the ceiling that would bind again if a
future caller supplied an offset obtained by other means (R0079-0042).

---

#### `SfxDetectionResult`

```rust
pub struct SfxDetectionResult { /* fields are private */ }
```

State accessors: `is_sfx()`, `archive_format()`, `data_offset()`,
`stub_type()`, `confidence() -> SfxConfidence`, `evidence() -> &[String]`.
Convenience: `is_probable()`, `is_confirmed()`, and `payload_coordinates()`
which returns `Some((ArchiveFormat, u64, StubType))` when `is_sfx()` is true
and all three fields are populated; prefer it over manual unwraps.

`confidence()` returns the tri-state `SfxConfidence` enum
(`NotSfx` / `Probable` / `Confirmed`), which replaced the former `f32`
score (I3). Production `detect_sfx()` returns only `NotSfx` or `Probable`;
`Confirmed` is reserved for the deferred confirmed-detection patterns
(MADR-0015).

Constructors and helpers: `not_sfx()`, `probable(stub_type, format,
offset, evidence: Vec<String>)`, `is_confirmed()`, `summary()`.

#### `Archive::open_with_sfx_progress(path: impl AsRef<Path>, progress: Option<SfxStagingProgress>) -> Result<Archive>`

Open an SFX archive while observing or cancelling the staging copy.
Returning `false` from a cancellable callback surfaces
`ArchiveError::Cancelled { operation: "sfx_staging" }`.

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

    /// Raw archive-internal path bytes when the source name was not
    /// valid UTF-8 (AD 0064). When `None`, `path` is the authoritative
    /// representation; when `Some`, the displayed `path` may have lost
    /// fidelity to U+FFFD substitutions and matching/extracting through
    /// `raw_path` is the safe option.
    pub raw_path: Option<Vec<u8>>,

    /// Symlink/hardlink target text, when the entry is a link
    pub link_target: Option<String>,

    /// Sequential entry ID (0-based, assigned during listing)
    pub id: usize,
}
```

### Methods

#### `ArchiveEntry::new(path: String, id: usize) -> Self`

Create a new `ArchiveEntry` with the given path and sequential ID. All
optional fields default to `None`/`false`.

> **Prefer the builder for new code.** `ArchiveEntry::new` is kept for
> source-compat. New code should reach for the typed entry-points
> below: they pre-set `entry_type` and route through the builder so
> permission/path invariants (R0075-0078, R0075-0079) cannot be
> violated by hand-rolled struct literals.

#### `ArchiveEntry::file(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder`

Begin building a file-typed entry. Returns the builder so the caller
can chain `.size(n)`, `.modified(t)`, `.permissions(p)`, etc. before
calling `.build()`.

#### `ArchiveEntry::try_file(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder>`

Fallible variant of `file` that rejects empty paths with
`ArchiveError::InvalidPath`.

#### `ArchiveEntry::dir_at(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder`

Begin building a directory-typed entry. Directories report
`size = None` and `compressed_size = None` across every backend.

#### `ArchiveEntryBuilder`

Fluent builder. Setters: `size`, `compressed_size`, `modified`,
`created`, `accessed`, `crc32`, `permissions` (Unix bits are masked
to `0o7777`), `raw_path`, `link_target`, `comment`, `encrypted`,
`attributes`. Call `.build()` to finalise.

```rust
use unified_archive::ArchiveEntry;

let entry = ArchiveEntry::file("docs/README.md", 0)
    .size(1234)
    .modified(std::time::SystemTime::now())
    .permissions(0o644)
    .build();
```

#### `ArchiveEntry::is_directory(&self) -> bool`

Check if this entry is a directory.

#### `ArchiveEntry::is_file(&self) -> bool`

Check if this entry is a regular file.

#### `ArchiveEntry::is_symlink(&self) -> bool`

Check if this entry is a symbolic link.

#### `ArchiveEntry::is_hardlink(&self) -> bool`

Check if this entry is a hard link.

#### `ArchiveEntry::compression_fraction(&self) -> Option<f64>`

`compressed_size / uncompressed_size` for entries where both sizes are
known and the uncompressed size is non-zero. Returns `None` otherwise.
Lower values indicate more aggressive compression. Replaces the
deprecated `compression_ratio()`.

#### `ArchiveEntry::expansion_ratio(&self) -> Option<f64>`

`uncompressed_size / compressed_size` (the inverse of
`compression_fraction`). Useful for "expansion factor" displays and
ratio-bomb checks.

> `compression_ratio()` remains for source-compat but is `#[doc(hidden)]`
> and `#[deprecated]` (R0068-0050). Prefer `compression_fraction()` /
> `expansion_ratio()` for new code.

---

## EntryType

```rust
#[non_exhaustive]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    HardLink,
    Other,
}
```

`#[non_exhaustive]` (R0076-0079): downstream matches must include a
wildcard arm (`_ => …`) so device / FIFO / socket categories can be
added without a breaking change.

---

## FileAttributes

```rust
#[non_exhaustive]
pub struct FileAttributes {
    pub windows: Option<u32>,
    pub unix_xattr: Option<Vec<(String, Vec<u8>)>>,
    pub archive_specific: Option<String>,
}
```

`#[non_exhaustive]`: construct only via `FileAttributes::default()`
followed by field updates so a future field addition does not become
a breaking change. Per-backend coverage of each field is documented
in the rustdoc.

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
    TarZst,    // TAR + Zstandard
    TarLz4,    // TAR + LZ4
    TarLzma,   // TAR + LZMA
    Gzip,      // Standalone GZIP (read/extract only)
    Bzip2,     // Standalone BZIP2 (read/extract only)
    Xz,        // Standalone XZ (read/extract only)
    Zst,       // Standalone Zstandard (read/extract only)
    Lz4,       // Standalone LZ4 (read/extract only)
    Lzma,      // Standalone LZMA (read/extract only)
    Iso,       // ISO 9660
}
// `ArchiveFormat` is `#[non_exhaustive]`; downstream matches need a wildcard arm.
```

### Methods

#### `ArchiveFormat::detect(path: &Path) -> Result<ArchiveFormat>`

Detect the archive format of a file by reading its magic bytes.

#### `ArchiveFormat::detect_from_bytes(magic: &[u8]) -> Result<ArchiveFormat>`

Detect the archive format from a byte slice of magic bytes (header data).

#### `ArchiveFormat::capabilities(&self) -> FormatCapabilities`

Return the read/write capability matrix for compression, encryption,
multipart support, and modification. Prefer this for new code — the
boolean helpers below collapse read/write asymmetry into a single
flag and lose information for read-only compressed streams.

#### `ArchiveFormat::supports_compression(&self) -> bool`

Whether this format supports configurable compression levels.

For new code prefer `supports_compression_read()` or
`supports_compression_write()` so read-only compressed streams are not
confused with creatable compressed archives.

#### `ArchiveFormat::supports_compression_read(&self) -> bool`

Whether this format can be decompressed/read by the crate.

#### `ArchiveFormat::supports_compression_write(&self) -> bool`

Whether this format can be written with configurable compression by the crate.

#### `ArchiveFormat::supports_encryption(&self) -> bool`

Whether the archive format itself supports password-based encryption in either read or write form. This is a **format capability**, not a promise that `unified-archive` can create encrypted archives for that format.

#### `ArchiveFormat::supports_encryption_read(&self) -> bool`

Whether this format's encryption can be read/decrypted by the crate.

#### `ArchiveFormat::supports_encryption_write(&self) -> bool`

Whether the crate can produce encrypted archives in this format.

#### `ArchiveFormat::supports_multipart(&self) -> bool`

Whether this format supports multi-part (split) archives.

#### `ArchiveFormat::supports_multipart_read(&self) -> bool`

Whether multi-part archives in this format can be read by the crate.

#### `ArchiveFormat::supports_multipart_write(&self) -> bool`

Whether multi-part archives in this format can be created by the crate.

#### `ArchiveFormat::can_modify(&self) -> bool`

Whether this format supports modification through the crate's
copy-on-write rewrite flow.

#### `ArchiveFormat::can_create(self) -> bool`

Whether `Archive::create()` accepts this format without relying on an
external tool. Compare with `CompressionOptions::validate_for_format`
for finer-grained per-options validation.

#### `ArchiveFormat::extensions(&self) -> &[&str]`

Get common file extensions for this format.

---

## Support

```rust
#[non_exhaustive]
pub enum Support {
    Full,
    Partial,
    None,
}
```

Capability tri-state used inside `FormatCapabilities`. See rustdoc for variant semantics.

---

## FormatCapabilities

```rust
#[non_exhaustive]
pub struct FormatCapabilities {
    pub encryption_read: Support,
    pub encryption_write: Support,
    pub multipart_read: Support,
    pub multipart_write: Support,
    pub modification: Support,
    pub compression_read: Support,
    pub compression_write: Support,
}
```

Per-operation capability matrix returned by `ArchiveFormat::capabilities()`.
Read/write are split so read-only compressed streams are not confused with
creatable compressed archives. `#[non_exhaustive]` — take a
`FormatCapabilities::default()` and assign the fields you need; the
`..Default::default()` struct-update form does **not** escape the attribute
and is refused outside this crate. See rustdoc on
`FormatCapabilities::compression()` for the worst-of helper.

---

## ExtractionOptions

Configuration for extraction operations.

```rust
#[non_exhaustive]
pub struct ExtractionOptions {
    /// Destination directory for extracted files
    pub destination: PathBuf,

    /// Password for encrypted archives (stored zeroed via `secstr::SecStr`)
    pub password: Option<SecStr>,

    /// Overwrite existing files (default: false, fails with error if files exist)
    pub overwrite: bool,

    /// Preserve file permissions (Unix mode bits) — see Field Details for the per-backend matrix
    pub preserve_permissions: bool,

    /// Preserve modification times — see Field Details for the per-backend matrix
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

### Construction

The struct is `#[non_exhaustive]`, so crates outside `unified-archive`
cannot build one with a struct literal. The `..Default::default()`
functional-update form is **not** an escape hatch — the attribute refuses
it with the same `E0639` as the spelled-out literal. `ExtractionOptions::new`
is the entry point, and every field has a consuming setter to chain off it:

```rust
let options = ExtractionOptions::new("./output")   // destination, plus the defaults below
    .password("hunter2")
    .overwrite(true)
    .preserve_permissions(true)
    .preserve_times(true)
    .verify_crc32(true)
    .limits(ExtractionLimits::builder().max_file_size(64 * 1024 * 1024).build())
    .filter(|entry| entry.path.ends_with(".txt"))
    .progress(|_done: u64, _total: Option<u64>| ControlFlow::Continue(()));
```

`.filter` and `.progress` box their argument, so the call site writes
neither `Some` nor `Box::new`. Two cases stay field assignment: installing
an already-boxed `Box<dyn ProgressCallback>` (`opts.progress = Some(b)`),
and clearing one back to `None` (`opts.filter = None`). The fields remain
`pub`, so reads and assignments both still compile.

`ExtractionOptions::default()` also still compiles outside the crate; its
`destination` is the process working directory, which is why `new` is
preferred.

### Default Values

`ExtractionOptions::default()` — and `new`, for every field but
`destination` — produces:

| Field | Default |
|-------|---------|
| `destination` | `PathBuf::from(".")` (`new` takes it as an argument) |
| `password` | `None` |
| `overwrite` | `false` |
| `preserve_permissions` | `true` |
| `preserve_times` | `true` |
| `verify_crc32` | `false` |
| `limits` | `ExtractionLimits::default()` |
| `filter` | `None` |
| `progress` | `None` |

`verify_crc32` defaults to `false` (AD 0062 A.3): libarchive-backed
formats without per-entry CRC32 (TAR, ISO, raw streams) return
`Unsupported` when this flag is `true`, so the default is opt-out.

### Field Details

| Field | Description |
|-------|-------------|
| `password` | Password for encrypted archives. Used when reopening archive for extraction. |
| `overwrite` | If `false` (default), extraction fails with an error listing existing files. |
| `preserve_permissions` | Applies Unix mode bits to extracted files (Unix only). ZIP: central-directory Unix modes (Unix-host entries only). 7z: Unix modes stored in the attribute field's upper half (p7zip convention). TAR family / ISO (libarchive): `ARCHIVE_EXTRACT_PERM`. RAR: UnRAR applies the archive's Unix mode when writing; when `false`, the staged file is reset to the extractor's default staging mode (`0o600`) before install. Honoured on both bulk and single-file extraction (R0080-0023, R0081-0067). |
| `preserve_times` | Applies the entry's modification time to extracted files. ZIP: DOS precision (2 s). 7z: NT-time precision. TAR family / ISO (libarchive): `ARCHIVE_EXTRACT_TIME`. RAR: UnRAR applies the archive mtime when writing; when `false`, the staged file's mtime is reset to the current time before install. Honoured on both bulk and single-file extraction (R0080-0023, R0081-0067). Accessed/created times are listing-only metadata and are not restored. |
| `verify_crc32` | Enables CRC32 verification where supported by the backend. RAR performs built-in verification regardless of this flag. |
| `limits` | Zip bomb protection. See `ExtractionLimits` for details. |
| `filter` | Optional predicate aliased as `EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>` (R0075-0080). When set, `extract_all` behaves as a selective extraction driven by the predicate (same semantics as `Archive::extract_some`). Link entries that pass the filter surface as `SkippedSymlink`/`SkippedHardLink` warnings (FR-022). `None` means "extract every entry". For ergonomic construction from a `Fn`/`FnMut` closure, use `entry_filter_from_fn`. |

### ExtractionLimits

Fields are **private**; construct with `ExtractionLimits::default()` for
the shipped defaults or `ExtractionLimits::builder()` to override
ceilings. Per-field ceilings are the typed `Cap` and `CompressionRatio`
wrappers rather than raw `u64` / `f64` sentinels, so "unlimited" is a
distinct state and the compression-ratio gate is exact (integer `u128`
cross-multiplication). This retires the INFINITY-vs-MAX sentinel hazard
(R0080-0006) and the 2^53 `f64` precision cliff (R0081-0024)
structurally, and partially advances OI-0076-005.

```rust
pub enum Cap {
    Limited(u64),
    Unlimited,
}
// impl From<u64> for Cap  -> Cap::Limited(v)
// Cap::{get, to_option, as_usize, exceeded_by, is_unlimited}

pub struct CompressionRatio { /* private: exact rational num/den, both > 0 */ }
// CompressionRatio::new(num, den) -> Result<Self>   (rejects zero operands)
// CompressionRatio::whole(ratio)  -> Result<Self>   (ratio : 1)

pub struct ExtractionLimits { /* all fields private */ }
```

Read accessors (all `-> Cap` unless noted):
`max_total_size()`, `max_file_size()`, `max_entry_count()`,
`max_sfx_payload_size()`, `max_compression_ratio() -> Option<CompressionRatio>`,
`reject_unsafe_paths() -> bool`.

Defaults: 10 GiB total, 1 GiB per file, `1000:1` ratio, 100,000 entries,
16 GiB SFX payload staging ceiling (AD 0040),
`reject_unsafe_paths = false` (AD 0066, behaviour deferred).

### Builder

```rust
let limits = ExtractionLimits::builder()
    .max_file_size(64 * 1024 * 1024)              // u64 -> Cap::Limited via From
    .max_total_size(Cap::Unlimited)
    .max_entry_count(Cap::Limited(10_000))
    .max_compression_ratio(CompressionRatio::whole(500)?)  // or .unlimited_compression_ratio()
    .max_sfx_payload_size(Cap::Limited(8 * 1024 * 1024 * 1024))
    .reject_unsafe_paths(true)                    // enforced: blocks the archive pre-extraction (AD 0066)
    .build();
```

`ExtractionLimitsBuilder` setters are infallible and chainable; the only
fallible step is `CompressionRatio::new` / `::whole`, which validates its
operands up front. `build()` returns an always-valid `ExtractionLimits`.

> **Removed in R0081 I1:** the public `ExtractionLimits::unlimited()`
> sentinel constructor and the `with_max_mmap_size()` builder method, plus
> all public fields. Use the builder (`.unlimited_compression_ratio()`,
> `Cap::Unlimited`) instead. An "everything unlimited" preset is no longer
> part of the public surface.

---

## ProgressCallback

Trait for monitoring extraction progress.

```rust
use std::ops::ControlFlow;

pub trait ProgressCallback: Send {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>;
}
```

> **Note:** `total` is `Option<u64>` because some backends (e.g., libarchive streaming)
> cannot determine total size in advance. See `specs/001-unified-archive/contracts/progress.md` for the full progress reporting contract.

> The trait is `Send` only (not `Send + Sync`). Callbacks are
> exclusively driven from the extraction worker; a `Sync` bound would
> force interior mutability that the contract does not require
> (R0070-0070).

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

        // Return Continue to keep extracting, or Break to cancel.
        // A Break surfaces from the operation as
        // `ArchiveError::Cancelled { operation }` (e.g. "extract_all",
        // "create", "sfx_staging").
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
#[non_exhaustive]
pub struct ValidationReport {
    /// Total number of entries checked
    pub total_entries: usize,

    /// Number of regular file entries considered for validation
    /// (directories, symlinks, hardlinks excluded). Invariant:
    /// `validated + failed.len() == total_files`.
    pub total_files: usize,

    /// Number of regular file entries that passed validation
    /// (directories, symlinks, hardlinks excluded). This is the count
    /// of regular file entries whose integrity check succeeded
    /// (CRC32 match or error-free read).
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

    /// Corruption detected (CRC mismatch, bad data). `path` names the corrupt
    /// subject: an *entry* path for payload/CRC failures, an *archive* path for
    /// container-level failures (header walks, recovery records, backend-state
    /// inconsistencies). `Display` stays neutral about which of the two it is
    /// (R0001-0069).
    Corruption { path: String, details: String },

    /// Password authentication error
    Password { message: String },

    /// Operation not supported for a specific format
    Unsupported { operation: String, format: ArchiveFormat, details: Option<String> },

    /// Codec not available (requires installation)
    CodecUnavailable { codec: String, format: ArchiveFormat, install_instructions: String },

    /// Operation not available in the current archive mode (e.g. extracting from Write-mode)
    WriteModeOnly { operation: String },

    /// Backend does not support this operation (e.g. creation on read-only backend)
    ReadOnlyBackend { operation: String },

    /// Feature not yet implemented (deferred to a future phase)
    NotImplemented { operation: String, reason: String },

    /// Operation blocked by resource limits, conflicts, or format constraints
    OperationBlocked { operation: String, reason: String },

    /// Invalid path
    InvalidPath { path: String, reason: String },

    /// Operation cancelled by a caller-supplied callback
    Cancelled { operation: &'static str },
}
```

`ArchiveError` is `#[non_exhaustive]`; downstream matches need a wildcard arm.

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

Create a `WriteModeOnly` error for attempts to read from a write-only archive.

#### `ArchiveError::read_only_backend(operation: impl Into<String>) -> Self`

Create a `ReadOnlyBackend` error for read-only backends that do not support creation.

---

## ResultWithWarnings\<T\>

Result type that carries both a value and non-fatal warnings emitted during an operation.

```rust
#[non_exhaustive]
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
#[non_exhaustive]
pub enum ArchiveWarning {
    /// Symbolic link skipped during operation
    SkippedSymlink { path: String, target: Option<String> },
    /// Hard link skipped during operation
    SkippedHardLink { path: String },
}
```

`#[non_exhaustive]` (matches the source-side R0076-0077 pattern):
downstream `match` arms over `ArchiveWarning` must include a wildcard
arm so future warning categories can land without a breaking change.

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

// These accessors are available under any `StreamBound`. Here we use
// `StreamBound::Unbounded` to omit the cap; `StreamBound::DeclaredSize`
// / `StreamBound::Cap(n)` expose the same accessors on the hard-capped
// reader (AD 0062 A.2 / R0080-0007 / DCR-006).
use unified_archive::StreamBound;
let mut stream = archive.extract_to_stream("large.bin", StreamBound::Unbounded)?;

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

The raw path-sanitization, CRC, and extraction-limit helper functions
(`check_extraction_safe`, `sanitize_entry_path`,
`sanitize_entry_path_with_base`, `validate_archive_internal_path`,
`verify_crc32`) are crate-internal policy plumbing
and are not part of the public API. Public callers configure extraction
through [`ExtractionOptions`](#extractionoptions) and
[`ExtractionLimits`](#extractionlimits); the gate runs automatically
inside `Archive::extract_*`.

---

## Stream Checksum Utilities

### `StreamChecksum`

```rust
#[non_exhaustive]
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

## Typed Handle API (`v2-api` feature)

When the optional `v2-api` feature is enabled, the crate exposes
`ReadArchive`, `WriteArchive`, and `ModifyArchive` (AD 0053). These
typed wrappers narrow the operation surface to mode-appropriate
methods at compile time, eliminating runtime "wrong-mode" rejections
for callers that adopt them.

```rust
# #[cfg(feature = "v2-api")]
# {
use unified_archive::ZipCompressionOptions;
use unified_archive::v2::{ReadArchive, WriteArchive};

let read = ReadArchive::open("backup.zip")?;
let entries = read.list_files()?;

let mut write = WriteArchive::create_zip(
    "out.zip",
    ZipCompressionOptions::new(),
)?;
write.add_file_from_data("hello.txt", b"hi")?;
write.finish()?;
# }
# Ok::<(), unified_archive::ArchiveError>(())
```

The typed handles delegate to the existing `Archive` machinery — no
behavioural divergence — and v0.4 is expected to flip `v2-api` on by
default.

### Operation parity (OI-0076-004)

The typed handles mirror the **full mode-appropriate surface** of the
legacy `Archive` facade; adopting them no longer requires dropping back
to `Archive` for any supported workflow:

| Handle | Exposes |
|---|---|
| `ReadArchive` | every constructor that yields a read handle (`open`, `open_encrypted`, `open_at_offset`, `open_sfx`, `open_with_sfx_progress`); the full inspection set (`list_files`, `list_files_for_limits`, `entry_count`, `find_entry`, `find_entries`, `validate_integrity`, `calculate_archive_crc`, `calculate_manifest_digest`, `calculate_content_multiset_digest_and_size`, `calculate_manifest_summary`, `multipart_layout`, `detect_multipart`, `check_symlinks`, `is_solid`, `has_recovery_record`, `recovery_percentage`, `is_encrypted`, `format`, `extension_format`, `path`); the full extraction set (`extract_all`, `extract_file`, `extract_files`, `extract_by_ids`, `extract_some`, `extract_filtered`, `extract_to_memory[_with_options]`, `extract_to_stream[_with_options]` (each taking a `StreamBound`)); and the static SFX helpers `detect_sfx` / `extract_stub`. |
| `WriteArchive` | every constructor (`create`, `create_zip`, `create_seven_zip`, `create_libarchive`); all `add_*` operations including `add_directory_recursive`; the write-progress `entry_count`; and the consuming `finish` / `close`. |
| `ModifyArchive` | constructors (`open`, `open_with_options`); all queue operations (`add_entry`, `add_entry_from_path`, `add_entry_from_reader`, `add_directory_entry`, `remove_entry`, `remove_entry_by_id`, `replace_entry`, `replace_entry_from_path`, `replace_entry_from_reader`, `clear_operations`); source inspection (`list_files`, `entry_count`, `find_entry`, `find_entries`) so callers can discover the `ArchiveEntry::id`s that `remove_entry_by_id` consumes; and the terminal `try_commit_changes` / `commit_changes`. |

**Intentionally not exposed** (facade-internal, not part of the public
typed surface): `pub(crate)` helpers (`open_at_offset_with_format_hint*`,
`source_path_for_reopen`, `payload_size_for_ratio`), private
constructors, and the unchecked extraction bypasses
(`extract_to_memory_unchecked` / `extract_to_stream_unchecked`).

**Durability ownership (R0076-0088).** `WriteArchive::finish(self)` is
the durable, **error-surfacing** commit path. Dropping a `WriteArchive`
without `finish()` runs a single best-effort finalize (so the writer's
resources are released — notably the libarchive write handle — rather
than leaked) and emits exactly one warning; the inner `Archive`'s
legacy `Drop`-finalize is suppressed so finalization happens once. Use
`finish()` whenever you need to observe a commit failure.

Refer to rustdoc for exact per-method signatures.

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
pub use crate::format::{ArchiveFormat, FormatCapabilities, Support};
pub use crate::entry::{ArchiveEntry, ArchiveEntryBuilder, EntryType, FileAttributes};
pub use crate::error::{ArchiveError, Operation, Result};

// Options and callbacks
pub use crate::options::{
    CompressionLevel, CompressionOptions, EntryFilter, ExtractionOptions,
    LibarchiveCompressionOptions, ProgressCallback, SevenZCompressionOptions,
    SfxStagingProgress, ZipCompressionOptions, entry_filter_from_fn,
};
pub use crate::modification::ModificationOptions;

// Security and limits (only ExtractionLimits is public; raw policy
// helpers are crate-internal — see Security Helpers section).
pub use crate::security::{Cap, CompressionRatio, ExtractionLimits, ExtractionLimitsBuilder};

// Inspection and streaming
pub use crate::inspection::{MultipartLayout, ValidationReport};
pub use crate::streaming::StreamingExtractor;

// SFX detection
pub use crate::sfx::{SfxConfidence, SfxDetectionResult, StubType};

// Stream-level checksum utilities
pub use crate::stream_crc::{
    CheckType, StreamChecksum, extract_bzip2_stream_crc,
    extract_gzip_stream_crc, extract_stream_checksum,
    extract_xz_stream_check,
};
```

### Backend Modules (Advanced)

The `ffi` module is `#[doc(hidden)]` and exists for crate-internal
composition plus selected integration tests. Backend wrapper types are
implementation details and may change without notice. Public consumers
should use the `Archive` facade and crate-root reexports documented above.

---

## Feature Flags

### `rar-support` (default)

Enables RAR/RAR5 support via UnRAR library.

**Disable RAR support:**
```toml
[dependencies]
unified-archive = { version = "0.4.0", default-features = false }
```

### `external-rar-create`

Enables the Windows-only `unified_archive::external::RarCreator` helper for out-of-process RAR creation through `rar.exe`.

Use this only when you explicitly want the WinRAR CLI bridge. It is separate from `Archive::create()`.

### `v2-api`

Enables the additive typed-handle surface:
`unified_archive::v2::{ReadArchive, WriteArchive, ModifyArchive}`.
These wrappers narrow operations by mode at compile time while delegating
to the existing `Archive` implementation. See AD 0053 D2 for the design
record.

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

1. **Sequential Extraction**: Selective extraction methods (`extract_filtered()`, `extract_files()`, `extract_by_ids()`) traverse the archive once and emit entries sequentially via `extract_some` (AD 0029). Earlier docs described a rayon-based parallel path; rayon is not currently a dependency and the parallel implementation has been retired.
2. **Streaming**: Use `extract_to_stream()` for large files to minimize memory usage (bounded-memory streaming is libarchive-only; other backends buffer the full entry in memory)
3. **Listing Cache**: `list_files()` results are cached after the first call; subsequent calls return the cached slice at no cost
4. **RAR Iterator Exhaustion**: The RAR backend may need to reopen the archive when switching between listing and extraction, or when extracting individual files in sequence. This is handled internally; no caller action is needed in most cases

---

## Common Patterns

### Extract with Progress

```rust
use unified_archive::ExtractionOptions;
use std::ops::ControlFlow;

let options = ExtractionOptions::new("./output").progress(
    |current: u64, total: Option<u64>| {
        if let Some(t) = total {
            print!("\rProgress: {:.1}%", (current as f64 / t as f64) * 100.0);
        }
        ControlFlow::Continue(())
    },
);

let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

### Selective Extraction by Extension

```rust
let options = ExtractionOptions::new("./images");

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

**Last Updated**: 2026-04-28
**Version**: 0.4.0
