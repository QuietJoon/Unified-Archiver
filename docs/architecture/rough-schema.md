# Rough Schema

In-memory entity structure for unified-archive. This library has no database — all entities are Rust structs living in memory within `Archive` handles.

## Archive

| Field | Type | Notes |
|---|---|---|
| backend | `ArchiveBackend` (enum) | UnRAR, Piz, SevenZ, Libarchive, ZipReader variants |
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
| crc32 | `Option<u32>` | None for directories or formats without CRC |
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

**Raw compressed-stream variants (read/extract supported via libarchive per AD 0019; standalone *creation* out of scope per AD 0018):**

| Variant | Magic Bytes | Notes |
|---|---|---|
| Gzip | `\x1f\x8b` | Standalone `.gz` openable via `Archive::open()` (read/extract only); use TarGzip for `.tar.gz` multi-file archives |
| Bzip2 | `BZ` | Standalone `.bz2` openable via `Archive::open()` (read/extract only); use TarBzip2 for `.tar.bz2` multi-file archives |
| Xz | `\xfd7zXZ\x00` | Standalone `.xz` openable via `Archive::open()` (read/extract only); use TarXz for `.tar.xz` multi-file archives |

`Archive::open()` routes these variants to libarchive for read/extract (AD 0019). Standalone stream *creation* is not supported — compressing a single file into `.gz`/`.bz2`/`.xz` must be done outside this crate.

- **Owned by:** `format.rs`
- **Primary identifier:** Enum variant
- **Constraints:** Capabilities are fixed per variant

## ExtractionOptions

| Field | Type | Default | Notes |
|---|---|---|---|
| destination | `PathBuf` | `"."` | Must be a directory |
| password | `Option<SecStr>` | None | Required for encrypted archives; stored zeroed via `secstr::SecStr` |
| overwrite | `bool` | false | Fail if destination files exist (FR-023) |
| preserve_permissions | `bool` | true | Unix mode bits |
| preserve_times | `bool` | true | Modification timestamps |
| verify_crc32 | `bool` | true | CRC verification during extraction |
| filter | `Option<EntryFilter>` | None | Entry-level filtering (`EntryFilter = Box<dyn Fn(&ArchiveEntry) -> bool + Send + Sync>`) |
| progress | `Option<Box<dyn ProgressCallback>>` | None | Progress reporting callback |
| limits | `ExtractionLimits` | `ExtractionLimits::default()` | Zip bomb protection (always present) |

- **Owned by:** `options.rs`

## CompressionOptions

| Field | Type | Default | Notes |
|---|---|---|---|
| format | `ArchiveFormat` | Zip | Target archive format |
| level | `CompressionLevel` | Normal | Store, Fastest, Fast, Normal, Maximum, Ultra |
| password | `Option<SecStr>` | None | Encryption (ZIP only currently); stored zeroed via `secstr::SecStr` |
| split_size | `Option<u64>` | None | DEFERRED: not honored by writers |
| progress | `Option<Box<dyn ProgressCallback>>` | None | Progress reporting callback |

- **Owned by:** `options.rs`

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
| confidence | `f32` | Detection confidence: 1.0 = highest confidence (may result from `probable(..., 1.0)` via clamping — does not strictly imply backend validation); <1.0 = probable (0.9 is current detector policy per AD 0015) |

- **Owned by:** `src/sfx/result.rs`
