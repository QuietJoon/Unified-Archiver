# Rough Schema

In-memory entity structure for unified-archive. This library has no database — all entities are Rust structs living in memory within `Archive` handles.

## Archive

| Field | Type | Notes |
|---|---|---|
| backend | `ArchiveBackend` (enum) | UnRAR, Piz, SevenZ, Libarchive, ZipReader variants |
| entry_cache | `OnceCell<Vec<ArchiveEntry>>` | Lazy cache, populated on first `list_files()` |
| format | `OnceCell<ArchiveFormat>` | Lazy-detected from magic bytes |
| path | `PathBuf` | Path to archive file on disk |
| mode | `ArchiveMode` | Read, Write, or Modify |

- **Owned by:** `archive.rs`
- **Primary identifier:** `path` (file path)
- **Relationships:** Contains 0..N ArchiveEntry (via entry_cache), has 1 ArchiveFormat
- **Constraints:** `Send` but not `Sync`; one handle per thread
- **Retention:** Dropped when handle goes out of scope (RAII)

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
| entry_type | `EntryType` | File, Directory, Symlink, Other |
| permissions | `Option<u32>` | Unix mode bits |
| attributes | `Option<FileAttributes>` | Platform-specific (Windows flags, Unix xattr) |
| index | `usize` | (pub(crate)) Position within archive |

- **Owned by:** `entry.rs`
- **Primary identifier:** `path` (unique within an archive)
- **Relationships:** Belongs to 1 Archive
- **Constraints:** `compressed_size <= size` when both Some; directories end with `/`
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
| Gzip | `\x1f\x8b` | Yes | No | No | No |
| Bzip2 | `BZ` | Yes | No | No | No |
| Xz | `\xfd7zXZ\x00` | Yes | No | No | No |
| Iso | `CD001` at offset 32769 | No | No | No | No |

- **Owned by:** `format.rs`
- **Primary identifier:** Enum variant
- **Constraints:** Capabilities are fixed per variant

## ExtractionOptions

| Field | Type | Default | Notes |
|---|---|---|---|
| destination | `PathBuf` | `"."` | Must be a directory |
| password | `Option<String>` | None | Required for encrypted archives |
| overwrite | `bool` | false | Fail if destination files exist (FR-023) |
| preserve_permissions | `bool` | true | Unix mode bits |
| preserve_times | `bool` | true | Modification timestamps |
| verify_crc32 | `bool` | true | CRC verification during extraction |
| filter | `Option<Box<dyn Fn(&ArchiveEntry) -> bool>>` | None | Entry-level filtering |
| progress | `Option<ProgressCallback>` | None | Progress reporting callback |
| limits | `Option<ExtractionLimits>` | None | Zip bomb protection |

- **Owned by:** `options.rs`

## CompressionOptions

| Field | Type | Default | Notes |
|---|---|---|---|
| format | `ArchiveFormat` | Zip | Target archive format |
| level | `CompressionLevel` | Normal | Store, Fastest, Fast, Normal, Maximum, Ultra |
| password | `Option<String>` | None | Encryption (ZIP only currently) |
| split_size | `Option<u64>` | None | DEFERRED: not honored by writers |
| preserve_permissions | `bool` | true | Unix mode bits |
| preserve_times | `bool` | true | Modification timestamps |
| progress | `Option<ProgressCallback>` | None | Progress reporting callback |

- **Owned by:** `options.rs`

## ArchiveError

| Variant | Fields | Category |
|---|---|---|
| Io | operation, path, source | Recoverable |
| Format | format, message | Fatal |
| Corruption | path, details | Fatal |
| Password | message | Recoverable |
| Unsupported | operation, format | Fatal |
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

- **Owned by:** `sfx/detection.rs`
