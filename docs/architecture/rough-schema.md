---
type: Rough Schema
title: "Rough Schema"
description: "In-memory entity structure for unified-archive."
tags: [architecture, schema, ADR-0019, ADR-0018, ADR-0015, ADR-0027, ADR-0062, FR-023]
timestamp: 2026-08-09T00:00:00Z
status: active
---

# Rough Schema

In-memory entity structure for unified-archive. This library has no database — all entities are Rust structs living in memory within `Archive` handles.

## Archive

| Field | Type | Notes |
|---|---|---|
| backend | `ArchiveBackend` (enum) | UnRAR, SevenZ, Libarchive, ZipReader variants |
| entry_cache | `OnceCell<Vec<ArchiveEntry>>` | Lazy cache, populated on first `list_files()` |
| format | `ArchiveFormat` | Eagerly detected from magic bytes at open time |
| path | `PathBuf` | Path to archive file on disk |
| mode | `ArchiveMode` | Read, Write, or Modify |

- **Owned by:** `archive.rs`
- **Primary identifier:** `path` (file path)
- **Relationships:** Contains 0..N ArchiveEntry (via entry_cache), has 1 ArchiveFormat
- **Constraints:** `Send` but not `Sync`; one handle per thread
- **Retention:** Dropped when handle goes out of scope (RAII)
- **Derived integrity views:** `calculate_archive_crc()` (wrapping sum), `calculate_manifest_digest()` (sorted-CRC content identity), `calculate_manifest_summary()` (digest + total uncompressed size). All three operate on the cached entry list and are documented in `specs/001-unified-archive/contracts/inspection.md` ("Archive-level integrity").

## ArchiveEntry

| Field | Type | Notes |
|---|---|---|
| path | `String` | Normalized: forward slashes, relative, no `..` |
| size | `Option<u64>` | Uncompressed size; None for directories |
| compressed_size | `Option<u64>` | None for uncompressed formats or directories |
| modified | `Option<SystemTime>` | UTC-normalized |
| created | `Option<SystemTime>` | UTC-normalized, format-dependent |
| accessed | `Option<SystemTime>` | UTC-normalized, format-dependent |
| crc32 | `Option<u32>` | None for directories, for formats without a per-entry CRC, and for AE-2 AES ZIP entries — AE-2 stores `0` in the central-directory CRC32 field by specification, so the ZIP backend lists `None` rather than a placeholder. The gate `crc32_check_exempt` is a **three-way** conjunction: `encrypted() && crc32() == 0 && aes_vendor_version(zip_file) == Some(AE2)`, the last conjunct reading the vendor version from the entry's `0x9901` WinZip-AES extra field. That conjunct is load-bearing: without it the gate is a superset of AE-2 and also sweeps in an encrypted *empty* file (ZipCrypto, or AE-1), whose stored `CRC32(b"") == 0` is a real checksum. Encrypted-empty and plaintext-empty entries therefore both stay `Some(0)`; CRC32 of an empty file really is 0 (AD 0012) |
| is_encrypted | `bool` | Entry-level encryption flag |
| comment | `Option<String>` | File comment (ZIP, RAR) |
| entry_type | `EntryType` | File, Directory, Symlink, HardLink, Other |
| permissions | `Option<u32>` | Unix mode bits |
| attributes | `Option<FileAttributes>` | Platform-specific (Windows flags, Unix xattr) |
| id | `usize` | (pub) Position within archive |

- **Owned by:** `entry.rs`
- **Primary identifier:** `path` (unique within an archive)
- **Relationships:** Belongs to 1 Archive
- **Constraints:** `compressed_size` may be greater or smaller than `size` depending on format/content; directory paths conventionally end with `/` but this is not guaranteed across all backends and formats
- **Retention:** Lives in entry_cache; dropped with Archive

## ArchiveFormat

| Variant | Magic Bytes | Supports Compression | Supports Encryption | Supports Multipart | Can Modify |
|---|---|---|---|---|---|
| SevenZip | `7z\xbc\xaf\x27\x1c` | Yes | Yes | Yes | Yes |
| Zip | `PK\x03\x04` | Yes | Yes | Yes | Yes |
| Rar | `Rar!\x1a\x07\x00` | Yes | Yes | Yes | No |
| Rar5 | `Rar!\x1a\x07\x01\x00` | Yes | Yes | Yes | No |
| Tar | `ustar` at offset 257 | No (outer layer) | No | No | No |
| TarGzip | `\x1f\x8b` | Yes | No | No | No |
| TarBzip2 | `BZ` | Yes | No | No | No |
| TarXz | `\xfd7zXZ\x00` | Yes | No | No | No |
| Iso | `CD001` at offset 32769 | No | No | No | No |

**Raw compressed-stream variants (read/extract supported via libarchive per MADR-0019; standalone *creation* out of scope per AD 0018):**

| Variant | Magic Bytes | Notes |
|---|---|---|
| Gzip | `\x1f\x8b` | Standalone `.gz` openable via `Archive::open()` (read/extract only); use TarGzip for `.tar.gz` multi-file archives |
| Bzip2 | `BZ` | Standalone `.bz2` openable via `Archive::open()` (read/extract only); use TarBzip2 for `.tar.bz2` multi-file archives |
| Xz | `\xfd7zXZ\x00` | Standalone `.xz` openable via `Archive::open()` (read/extract only); use TarXz for `.tar.xz` multi-file archives |

`Archive::open()` routes these variants to libarchive for read/extract (MADR-0019). Standalone stream *creation* is not supported — compressing a single file into `.gz`/`.bz2`/`.xz` must be done outside this crate.

- **Owned by:** `format.rs`
- **Primary identifier:** Enum variant
- **Constraints:** Capabilities are fixed per variant

## ExtractionOptions

Regenerated from `src/options.rs` (R0001-0081 — the previous table carried three types/defaults that no
longer matched the code, so every row below is restated from the live struct and its `Default` impl).
Field order follows the declaration order in `src/options.rs`.

| Field | Type | Default | Notes |
|---|---|---|---|
| destination | `PathBuf` | `"."` | Destination directory; created by `ensure_destination` when missing |
| password | `Option<Password>` | None | Required for encrypted archives. The type is `Option<Password>`, **not** `Option<SecStr>`: `Password` (`src/password.rs`) wraps `secstr::SecStr`, is UTF-8 by construction, redacts itself in `Debug`/`Display`, and exposes an infallible `as_str()` (`docs/records/AD-0042-password-as-str-returns-result-on-non-utf8.md`, as amended by R0081 I5). Prefer the `ExtractionOptions::password()` builder over assigning the field |
| overwrite | `bool` | false | Fail if destination files exist (FR-023); the preflight scan runs before any entry is written |
| preserve_permissions | `bool` | true | Unix mode bits; per-backend semantics documented on the field (R0079-0019). RAR/RAR5 always preserve — UnRAR applies attributes itself and does not consult the flag |
| preserve_times | `bool` | true | Modification time only on the native Rust backends; accessed/created times are listing-only metadata (OI-0065-002). RAR/RAR5 always preserve |
| verify_crc32 | `bool` | **false** | Per `docs/records/AD-0062-r0069-group-a-v0.3-api-shaping.md` (A.3) the default is `false`, not `true`. ZIP honours it explicitly; 7z and RAR/RAR5 validate CRC32 unconditionally either way; libarchive-backed formats surface no per-entry CRC32 and reject `true` with `ArchiveError::Unsupported` |
| limits | `ExtractionLimits` | `ExtractionLimits::default()` | Zip bomb protection (always present) |
| filter | `Option<EntryFilter>` | None | Entry-level filtering. `EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>` — `FnMut` (not `Fn`) since R0075-0080, and `Send` only (not `Send + Sync`) since R0070-0071; use `options::entry_filter_from_fn` to lift a known-`Fn` closure. Supplying it turns `extract_all` into a selective `extract_some` run |
| progress | `Option<Box<dyn ProgressCallback>>` | None | Progress reporting callback |

- **Owned by:** `options.rs`

## CompressionOptions

| Field | Type | Default | Notes |
|---|---|---|---|
| format | `ArchiveFormat` | Zip | Target archive format; rejected with `OperationBlocked` when `ArchiveFormat::can_create()` is false |
| level | `CompressionLevel` | Normal | Store, Fastest, Fast, Normal, Maximum, Ultra |
| password | `Option<Password>` | None | **Rejected for every format, not "ZIP only" (R0001-0082).** `CompressionOptions::validate_for_format` and `Archive::create` both return `ArchiveError::OperationBlocked` whenever this is `Some(_)`, whatever the target format — see `docs/records/MADR-0027-reject-encrypted-archive-creation.md` and scenario SCN-CRE-03. Encrypted **reads** are the supported path: `Archive::open_encrypted()`, or `ExtractionOptions.password` for extraction. The field is retained because MADR-0027's 2026-07-20 amendment defers (rather than forbids) an explicit encrypted-creation opt-in, tracked as OI-0081-006 in `docs/project/open-issues.md`; nothing ships behind it today |
| split_size | `Option<u64>` | None | DEFERRED (DEF-002): rejected with `OperationBlocked` for every format, not silently ignored |
| progress | `Option<Box<dyn ProgressCallback>>` | None | Progress reporting callback. `CompressionOptions` deliberately does not implement `Clone` (a clone would drop the callback); use `strip_progress()` for an explicit copy without it |

- **Owned by:** `options.rs`
- **Typed alternatives:** `ZipCompressionOptions`, `SevenZCompressionOptions`, and `LibarchiveCompressionOptions` narrow the surface per format so `password`/`split_size` are unrepresentable (R0075-0081)

## ArchiveError

| Variant | Fields | Category |
|---|---|---|
| Io | operation, path, source | Recoverable |
| Format | format, message | Fatal |
| Corruption | path, details | Fatal |
| Password | message | Recoverable |
| Unsupported | operation, format, details: Option\<String\> | Fatal |
| CodecUnavailable | codec, format, install_instructions | Fatal |
| WriteModeOnly | operation | Fatal |
| ReadOnlyBackend | operation | Fatal |
| NotImplemented | operation, reason | Fatal |
| OperationBlocked | operation, reason | Fatal |
| InvalidPath | path, reason | Fatal (security) |

- **Owned by:** `error.rs`
- **Implements:** `std::error::Error`, `Display`

## SfxDetectionResult

| Field | Type | Notes |
|---|---|---|
| is_sfx | `bool` | Whether file is a self-extracting archive |
| archive_format | `Option<ArchiveFormat>` | Detected embedded format |
| data_offset | `Option<u64>` | Byte offset where archive data begins |
| stub_type | `Option<StubType>` | WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown |
| confidence | `SfxConfidence` | Tri-state detection verdict (I3, replaced the former `f32`): `NotSfx` / `Probable` / `Confirmed`. Production `detect_sfx()` emits only `NotSfx` or `Probable`; `Confirmed` is reserved for the deferred confirmed-detection patterns (MADR-0015) |
| evidence | `Vec<String>` | Human-readable notes on why the result reached its confidence (matched stub, signature, offset); empty for non-SFX results |

- **Owned by:** `src/sfx/result.rs`
