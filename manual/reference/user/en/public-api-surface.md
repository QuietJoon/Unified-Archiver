---
type: Reference
title: Public API surface
description: A complete index of everything unified-archive exports — crate-root re-exports, public modules, every public Archive method by mode, and the v2 typed handles.
tags: [api, archive, extraction, creation, modification]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: crate-root, resource: src/lib.rs }
  - { id: facade, resource: src/archive.rs }
  - { id: mode-split, resource: src/archive/mode_split.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: creation, resource: src/creation.rs }
  - { id: modification, resource: src/modification.rs }
  - { id: entry, resource: src/entry.rs }
synced_hash: a7bb268613d65c7193b29645b75e4fa85f6f6ac98b62bfb739134109dfa3a9ac
---

# Public API surface

`unified-archive` is a library crate with no binary target. This page indexes what it
exports at version 0.4.0. It lists items and signatures; behaviour and defaults are
documented in [Options and defaults](options-and-defaults.md),
[Errors and warnings](errors-and-warnings.md), and the
[Format support matrix](format-support-matrix.md).

Rustdoc covers every public item. The `v2` module appears in it only when the `v2-api`
feature is enabled.

## `#[non_exhaustive]` types

Nine public types are marked `#[non_exhaustive]`: `ArchiveFormat`, `Support` and
`FormatCapabilities` in `src/format.rs`; `EntryType` and `FileAttributes` in `src/entry.rs`;
`ArchiveError`, `Operation` and `ArchiveWarning` in `src/error.rs`; and `SfxDetectionResult`
in `src/sfx/result.rs`. That is the complete set for version 0.4.0. A `match` over any of
those enums in downstream code requires a wildcard arm, and `FormatCapabilities` and
`FileAttributes` cannot be built by struct literal outside the crate despite their public
fields (`SfxDetectionResult` has no public fields either way). The item tables below repeat
the marking per type.

`CompressionOptions` is deliberately not marked, so its struct-literal shape stays
source-compatible through 0.3.

## Crate-root re-exports

These are the paths `use unified_archive::…;` resolves, grouped by area. The declarations
live in `src/lib.rs`.

### Archive handle

| Item | Kind |
|---|---|
| `Archive` | struct — the single handle type for read, write, and modify sessions |

`Archive` is `Send` but deliberately not `Sync`: one handle per thread. The `Send` claim is
pinned by a compile-time assertion in `src/archive.rs`.

### Entries

| Item | Kind |
|---|---|
| `ArchiveEntry` | struct — per-entry metadata |
| `ArchiveEntryBuilder` | struct — fluent builder returned by `ArchiveEntry::file` / `dir_at` |
| `EntryType` | enum — `File`, `Directory`, `Symlink`, `HardLink`, `Other`; `#[non_exhaustive]` |
| `FileAttributes` | struct — `windows: Option<u32>`, `unix_xattr: Option<Vec<(String, Vec<u8>)>>`, `archive_specific: Option<String>`; `#[non_exhaustive]` |

`ArchiveEntry` public fields: `path: String`, `size: Option<u64>`,
`compressed_size: Option<u64>`, `modified: Option<SystemTime>`, `crc32: Option<u32>`,
`entry_type: EntryType`, `permissions: Option<u32>`, `created: Option<SystemTime>`,
`accessed: Option<SystemTime>`, `is_encrypted: bool`, `comment: Option<String>`,
`attributes: Option<FileAttributes>`, `raw_path: Option<Vec<u8>>`,
`link_target: Option<String>`, `id: usize`.

`ArchiveEntry` associated functions and methods:

| Signature | Description |
|---|---|
| `fn file(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder` | start a file-typed builder; path not validated |
| `fn try_file(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder, ArchiveError>` | same, rejecting an empty path |
| `fn dir_at(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder` | start a directory-typed builder |
| `fn try_dir_at(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder, ArchiveError>` | same, rejecting an empty path |
| `fn new(path: String, id: usize) -> Self` | file-typed entry directly; marked for v0.4 deprecation |
| `fn directory(path: String, id: usize) -> Self` | directory-typed entry directly; marked for v0.4 deprecation |
| `fn symlink(path: String, id: usize, target: String) -> Self` | symlink-typed entry with `link_target` set |
| `fn compression_fraction(&self) -> Option<f64>` | compressed size divided by uncompressed size |
| `fn expansion_ratio(&self) -> Option<f64>` | uncompressed size divided by compressed size |
| `fn compression_ratio(&self) -> Option<f64>` | legacy ratio accessor |
| `fn is_directory(&self) -> bool` | `entry_type == EntryType::Directory` |
| `fn is_file(&self) -> bool` | `entry_type == EntryType::File` |
| `fn is_symlink(&self) -> bool` | `entry_type == EntryType::Symlink` |
| `fn is_hardlink(&self) -> bool` | `entry_type == EntryType::HardLink` |

`ArchiveEntryBuilder` setters, each taking and returning `self`: `size(u64)`,
`compressed_size(u64)`, `modified(SystemTime)`, `created(SystemTime)`,
`accessed(SystemTime)`, `crc32(u32)`, `permissions(u32)`, `raw_path(Vec<u8>)`,
`link_target(String)`, `comment(String)`, `encrypted(bool)`,
`attributes(FileAttributes)`; terminated by `build(self) -> ArchiveEntry`.

### Formats

| Item | Kind |
|---|---|
| `ArchiveFormat` | enum — 18 variants: `SevenZip`, `Zip`, `Rar`, `Rar5`, `Tar`, `TarGzip`, `TarBzip2`, `TarXz`, `TarZst`, `TarLz4`, `TarLzma`, `Gzip`, `Bzip2`, `Xz`, `Zst`, `Lz4`, `Lzma`, `Iso`; `#[non_exhaustive]` |
| `FormatCapabilities` | struct — `encryption_read`, `encryption_write`, `multipart_read`, `multipart_write`, `modification`, `compression_read`, `compression_write`, each a `Support`; `#[non_exhaustive]` |
| `Support` | enum — `Full`, `Partial`, `None`; `#[non_exhaustive]` |

`ArchiveFormat` public methods:

| Signature | Description |
|---|---|
| `fn detect(path: &Path) -> Result<Self>` | identify a file by magic bytes, with an extension fallback for a fixed set of formats |
| `fn detect_from_bytes(magic: &[u8]) -> Result<Self>` | identify from a magic-byte prefix alone |
| `fn capabilities(&self) -> FormatCapabilities` | per-operation support record |
| `fn supports_compression(&self) -> bool` | compression in either direction |
| `fn supports_compression_read(&self) -> bool` | read-side compression support |
| `fn supports_compression_write(&self) -> bool` | write-side compression support |
| `fn supports_encryption_read(&self) -> bool` | can read encrypted archives |
| `fn supports_encryption_write(&self) -> bool` | can write encrypted archives |
| `fn supports_encryption(&self) -> bool` | encryption in either direction |
| `fn supports_multipart_read(&self) -> bool` | can read split volumes |
| `fn supports_multipart_write(&self) -> bool` | can write split volumes |
| `fn supports_multipart(&self) -> bool` | split volumes in either direction |
| `fn can_modify(&self) -> bool` | eligible for `Archive::modify` |
| `fn can_create(self) -> bool` | eligible for `Archive::create`; this is the authoritative creation gate |
| `fn extensions(&self) -> &[&str]` | file extensions associated with the format |

`FormatCapabilities::compression(self) -> Support` collapses the read and write compression
fields into one value.

The variant list above is the enum's own declaration order; the crate-doc table in
`src/lib.rs` lists the same 18 formats in a different order. Which of them each operation
accepts is tabulated in the [Format support matrix](format-support-matrix.md).

### Options

| Item | Kind |
|---|---|
| `ExtractionOptions` | struct — destination, password, overwrite, permission/time preservation, CRC verification, limits, filter, progress |
| `CompressionOptions` | struct — format, level, password, split size, progress |
| `CompressionLevel` | enum — `Store`, `Fastest`, `Fast`, `Normal`, `Maximum`, `Ultra` |
| `ZipCompressionOptions` | struct — typed builder pairing with `Archive::create_zip` |
| `SevenZCompressionOptions` | struct — typed builder pairing with `Archive::create_seven_zip` |
| `LibarchiveCompressionOptions` | struct — typed builder pairing with `Archive::create_libarchive` |
| `EntryFilter` | type alias — `Box<dyn FnMut(&ArchiveEntry) -> bool + Send>` |
| `entry_filter_from_fn` | function — `fn entry_filter_from_fn<F>(f: F) -> EntryFilter where F: Fn(&ArchiveEntry) -> bool + Send + 'static` |
| `ProgressCallback` | trait — `pub trait ProgressCallback: Send`, with `fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>`; blanket-implemented for `F` where `F: FnMut(u64, Option<u64>) -> ControlFlow<()> + Send` |
| `SfxStagingProgress` | struct — `new(impl FnMut(u64) + Send + 'static)`, `with_cancel(impl FnMut(u64) -> bool + Send + 'static)` |

`entry_filter_from_fn` takes `Fn`, not `FnMut`, even though the `EntryFilter` alias it
returns is a boxed `FnMut`. A closure that needs to mutate captured state has to be boxed
directly.

Methods on these types:

- `ExtractionOptions`: `password(mut self, password: impl Into<String>) -> Self`, plus
  `Default`. All other fields are public and set directly or through struct-update syntax.
- `CompressionOptions`: `new(format: ArchiveFormat) -> Self`, `format(&self) -> ArchiveFormat`,
  `password(mut self, password: impl Into<String>) -> Self`,
  `strip_progress(&self) -> Self` (a copy without the progress callback, which cannot be
  cloned), `validate_for_format(&self) -> Result<()>`, plus `Default` and a hand-written
  `Debug` that omits the password. It does not implement `Clone`.
- `ZipCompressionOptions` and `SevenZCompressionOptions`: `new() -> Self`,
  `level(mut self, l: CompressionLevel) -> Self`,
  `progress(mut self, p: Box<dyn ProgressCallback>) -> Self`, `Default`, and
  `Into<CompressionOptions>`. Neither exposes a password setter.
- `LibarchiveCompressionOptions`: `new(format: ArchiveFormat) -> Self`, the same `level` and
  `progress` setters, and `Into<CompressionOptions>`. It has no `Default` because the format
  is required.

Every field, default, and rejection rule: [Options and defaults](options-and-defaults.md).

### Security

| Item | Kind |
|---|---|
| `ExtractionLimits` | struct — the extraction resource ceilings; fields are private, read through accessors |
| `ExtractionLimitsBuilder` | struct — builder returned by `ExtractionLimits::builder()` |
| `Cap` | enum — `Limited(u64)`, `Unlimited`; `From<u64>` |
| `CompressionRatio` | struct — a validated numerator/denominator pair |

`ExtractionLimits` accessors: `max_total_size() -> Cap`, `max_file_size() -> Cap`,
`max_compression_ratio() -> Option<CompressionRatio>`, `max_entry_count() -> Cap`,
`max_sfx_payload_size() -> Cap`, `reject_unsafe_paths() -> bool`, plus
`builder() -> ExtractionLimitsBuilder`.

`ExtractionLimitsBuilder` methods: `max_total_size(impl Into<Cap>)`,
`max_file_size(impl Into<Cap>)`, `max_entry_count(impl Into<Cap>)`,
`max_compression_ratio(CompressionRatio)`, `unlimited_compression_ratio()`,
`max_sfx_payload_size(impl Into<Cap>)`, `reject_unsafe_paths(bool)`, and
`build() -> ExtractionLimits`.

`Cap` methods: `get() -> u64`, `to_option() -> Option<u64>`, `as_usize() -> usize`,
`exceeded_by(u64) -> bool`, `is_unlimited() -> bool` — all `const`.

`CompressionRatio`: `new(numerator: u64, denominator: u64) -> Result<Self>`,
`whole(ratio: u64) -> Result<Self>`, `numerator() -> u64`, `denominator() -> u64`.

The raw policy fragments (`check_extraction_safe`, `check_single_entry_safe`,
`sanitize_entry_path`, `verify_crc32`, and their siblings) are `pub(crate)` and are not part
of the public surface; the facade establishes their invariants. See
[The extraction safety model](../../../explanation/user/en/extraction-safety-model.md).

### SFX

| Item | Kind |
|---|---|
| `SfxDetectionResult` | struct — private fields, read through accessors; `#[non_exhaustive]` |
| `SfxConfidence` | enum — `NotSfx`, `Probable`, `Confirmed` |
| `StubType` | enum — `WindowsPE`, `LinuxELF`, `MacOSMachO`, `ScriptInterpreter`, `Unknown` |

`SfxDetectionResult` accessors: `is_sfx() -> bool`, `archive_format() -> Option<ArchiveFormat>`,
`data_offset() -> Option<u64>`, `stub_type() -> Option<StubType>`,
`confidence() -> SfxConfidence`, `is_probable() -> bool`, `is_confirmed() -> bool`,
`evidence() -> &[String]`, `payload_coordinates() -> Option<(ArchiveFormat, u64, StubType)>`,
`summary() -> String`. Public constructors: `not_sfx()` and
`probable(StubType, ArchiveFormat, u64, Vec<String>)`. `Default` yields `not_sfx()`.

`src/sfx/result.rs` also declares `detected(StubType, ArchiveFormat, u64)`, but it is
`#[cfg(test)] pub(crate)` — compiled only in test builds and crate-private, so it is not
callable from outside the crate. It is the only constructor that sets
`SfxConfidence::Confirmed`, so no public API produces a `Confirmed` result. See
[How SFX detection decides](../../../explanation/developer/en/sfx-detection-pipeline.md).

`StubType`: `detect(bytes: &[u8]) -> StubType`, `description() -> &'static str`,
`is_native() -> bool`, `is_known() -> bool`.

### Stream checksums

| Item | Kind |
|---|---|
| `StreamChecksum` | struct — `crc32: Option<u32>`, `crc64: Option<u64>`, `uncompressed_size: Option<u64>`, `check_type: CheckType` |
| `CheckType` | enum — `None`, `Crc32`, `Crc64`, `Sha256`, `Unknown` |
| `extract_gzip_stream_crc` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_bzip2_stream_crc` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_xz_stream_check` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_stream_checksum` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` — dispatches on magic bytes |

`StreamChecksum` methods: `crc32_value() -> Option<u32>` and `crc64_value() -> Option<u64>`,
each returning the value only when `check_type` matches.

### Streaming

| Item | Kind |
|---|---|
| `StreamBound` | enum — `DeclaredSize`, `Cap(u64)`, `Unbounded` |
| `StreamingExtractor` | struct implementing `std::io::Read` |

`StreamingExtractor` methods: `total_size() -> Option<u64>`, `bytes_read() -> u64`,
`progress() -> Option<f64>`, `take_bounded(self, fallback: u64) -> std::io::Take<Self>`.
Its constructors are crate-private — instances come from `Archive::extract_to_stream` and
`Archive::extract_to_stream_with_options`. See
[Streaming and memory behaviour](../../../explanation/developer/en/streaming-and-memory.md).

### Errors

| Item | Kind |
|---|---|
| `ArchiveError` | enum — the crate's single error type; `#[non_exhaustive]` |
| `Operation` | enum — stable operation labels carried in error fields; `#[non_exhaustive]` |
| `Result` | type alias — `Result<T> = std::result::Result<T, ArchiveError>` |

`ArchiveWarning` (also `#[non_exhaustive]`) and `ResultWithWarnings<T>` are **not**
re-exported at the crate root even though they appear in public signatures. Reach them
through the public `error` module: `unified_archive::error::ArchiveWarning`,
`unified_archive::error::ResultWithWarnings`.
Full detail: [Errors and warnings](errors-and-warnings.md).

### Password

| Item | Kind |
|---|---|
| `Password` | struct — wraps a secret string; `Debug` and `Display` both redact |

`Password::new(impl Into<String>)`, `Password::as_str(&self) -> &str`, plus
`From<String>`, `From<&str>`, and `From<&String>`.

### Also re-exported

| Item | Declared in | Kind |
|---|---|---|
| `MultipartLayout` | `src/inspection.rs` | enum — `Single { path: PathBuf }`, `Multi { parts: Vec<PathBuf> }` |
| `ValidationReport` | `src/inspection.rs` | struct — `total_entries: usize`, `total_files: usize`, `validated: usize`, `failed: Vec<String>` |
| `ModificationOptions` | `src/modification.rs` | struct — four public fields plus `new()`, `with_backup(&str)`, `without_metadata_preservation()`, `Default` |

`inspection` and `modification` are `pub(crate)` modules, so these three types are reachable
only through the crate-root re-export.

`ModificationOptions` public fields, with the values `new()` (and therefore `Default`) sets:

| Field | Type | `new()` value |
|---|---|---|
| `preserve_metadata` | `bool` | `true` |
| `create_backup` | `bool` | `false` |
| `backup_suffix` | `String` | `".bak"` |
| `compression` | `Option<CompressionOptions>` | `None` |

The type is not `#[non_exhaustive]`, so all four fields can be assigned directly or through
struct-update syntax. `with_backup` sets `create_backup` to `true` and `backup_suffix` to
its argument, falling back to `".bak"` when the argument is empty or contains `/` or `\`;
`commit_changes` re-validates `backup_suffix` because a caller can assign the field without
going through the builder.

## Public modules

| Module | Gate | Contents |
|---|---|---|
| `archive` | — | `Archive`; `archive::mode_split` when `v2-api` is on. `ArchiveMode` and `ArchiveBackend` are `pub(crate)`. |
| `entry` | — | `ArchiveEntry`, `ArchiveEntryBuilder`, `EntryType`, `FileAttributes` |
| `error` | — | `ArchiveError`, `Operation`, `Result`, `ArchiveWarning`, `ResultWithWarnings`. The `ops` label constants are `pub(crate)`. |
| `format` | — | `ArchiveFormat`, `FormatCapabilities`, `Support`, and `format_from_extension(path: &Path) -> Option<ArchiveFormat>` |
| `options` | — | every options type above, plus `RateLimiter` (`new()`, `with_interval(Duration)`, `should_update()`, `should_call()`, `Default`) |
| `password` | — | `Password` |
| `security` | — | the limit types above, plus the `pub` constants `DEFAULT_MAX_TOTAL_SIZE`, `DEFAULT_MAX_FILE_SIZE`, `DEFAULT_MAX_COMPRESSION_RATIO`, `DEFAULT_MAX_ENTRY_COUNT`, `DEFAULT_MAX_SFX_PAYLOAD_SIZE`, `MAX_COMMENT_SIZE` |
| `sfx` | — | submodules `detection`, `result`, `stub_types`; re-exports `detect_sfx`, `SfxConfidence`, `SfxDetectionResult`, `StubType`. `sfx::limits` and `sfx::signatures` are `pub(crate)`. |
| `stream_crc` | — | `StreamChecksum`, `CheckType`, and the four extraction functions |
| `streaming` | — | `StreamBound`, `StreamingExtractor` |
| `v2` | `feature = "v2-api"` | `ReadArchive`, `WriteArchive`, `ModifyArchive` |
| `external` | `target_os = "windows"` **and** `feature = "external-rar-create"` | `RarCreator` — a WinRAR command-line bridge |
| `ffi` | `#[doc(hidden)]` | **not** part of the supported surface (see below) |

`backend`, `creation`, `extraction`, `fs_identity`, `inspection`, and `modification` are
`pub(crate)` modules: their `Archive` methods are public, the modules themselves are not.

`sfx::detect_sfx` is the free function form of the detector,
`fn detect_sfx<P: AsRef<Path>>(path: P) -> Result<SfxDetectionResult>`; `Archive::detect_sfx`
delegates to it.

`external::RarCreator` methods: `new<P: AsRef<Path>>(output_path: P) -> Result<Self>`,
`with_rar_exe_path<P, R>(output_path: P, rar_exe_path: R) -> Result<Self>`,
`set_compression_level(&mut self, level: CompressionLevel)`,
`set_password(&mut self, password: impl Into<String>)`,
`add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()>`,
`add_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<()>`,
`entry_count(&self) -> usize`, `create(self) -> Result<()>`, `rar_exe_path(&self) -> &Path`.
Feature detail: [Cargo features and MSRV](../../developer/en/cargo-features.md).

### The `ffi` module

`ffi` is declared `#[doc(hidden)] pub mod ffi`. It is public only so that integration tests
under `tests/` — which link against the non-test build — can reach backend types directly.
Rustdoc omits it. Nothing inside it is covered by the crate's compatibility promise; the
supported form of the same functionality is the crate-root re-exports and the `Archive`
methods listed above.

## `Archive` methods

Every method below is on `Archive`. Signatures are as written in the source, with `Self`
spelled out where it aids reading. `Result<T>` is the crate alias.

### Opening and detection

| Signature | Description |
|---|---|
| `fn open(path: impl AsRef<Path>) -> Result<Self>` | detect the format and open a read handle |
| `fn open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Self>` | open a read handle with a password for encrypted content |
| `fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self>` | open the archive embedded at a byte offset, staging the payload to a temporary file |
| `fn open_sfx(path: impl AsRef<Path>) -> Result<Self>` | detect a self-extracting archive and open its embedded payload |
| `fn open_with_sfx_progress(path: impl AsRef<Path>, progress: Option<SfxStagingProgress>) -> Result<Self>` | same, reporting and optionally cancelling payload staging |
| `fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>` | run the SFX detection pipeline without opening the payload |
| `fn extract_stub(path: impl AsRef<Path>, detection: &SfxDetectionResult) -> Result<Vec<u8>>` | read the executable stub bytes that precede a detected payload |

Companion recipes: [How to detect and open a self-extracting archive](../../../how-to/user/en/handle-self-extracting-archives.md)
and [How to open password-protected archives](../../../how-to/user/en/open-password-protected-archives.md).

### Handle metadata

| Signature | Description |
|---|---|
| `fn format(&self) -> ArchiveFormat` | the format the handle was opened as |
| `fn extension_format(&self) -> Option<ArchiveFormat>` | the format the file extension suggests, if any |
| `fn path(&self) -> &Path` | the caller-facing path, which for an SFX handle stays the outer executable |
| `fn is_encrypted(&self) -> Result<bool>` | whether the archive reports encrypted content |
| `fn has_recovery_record(&self) -> Result<bool>` | RAR recovery-record presence; `WriteModeOnly` on a write handle |
| `fn recovery_percentage(&self) -> Result<Option<u8>>` | RAR recovery-record size as a percentage; `WriteModeOnly` on a write handle |
| `fn is_solid(&self) -> Result<bool>` | whether the archive is solid-compressed; `WriteModeOnly` on a write handle |

### Inspection

| Signature | Description |
|---|---|
| `fn list_files(&self) -> Result<&[ArchiveEntry]>` | full listing, cached in the handle after the first call |
| `fn list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>>` | owned listing that does not populate the handle cache |
| `fn entry_count(&self) -> Result<usize>` | listed entries in read/modify mode; entries already written in write mode |
| `fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>>` | first entry with an exactly matching path |
| `fn find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>>` | every entry with that path, for archives holding duplicates |
| `fn validate_integrity(&self) -> Result<ValidationReport>` | verify entry checksums and report totals plus failures |
| `fn calculate_archive_crc(&self) -> Result<u32>` | wrapping sum of the entries' CRC32 values |
| `fn calculate_content_multiset_digest_and_size(&self) -> Result<(String, u64)>` | content-identity digest and total uncompressed size in one pass |
| `fn calculate_manifest_digest(&self) -> Result<String>` | source-compatible shim returning only the digest |
| `fn calculate_manifest_summary(&self) -> Result<(String, u64)>` | source-compatible shim for the digest-and-size pair |
| `fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)>` | legacy multipart probe; `WriteModeOnly` on a write handle |
| `fn multipart_layout(&self) -> Result<MultipartLayout>` | typed replacement for `detect_multipart` |
| `fn check_symlinks(&self) -> Result<Vec<ArchiveWarning>>` | warnings for the link entries extraction would skip |

The digest is a content multiset: archives with identical file contents but different paths
produce the same value. See
[What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md).

### Extraction

| Signature | Description |
|---|---|
| `fn extract_all(&self, options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | extract every entry to `options.destination` |
| `fn extract_file(&self, file_path: &str, options: ExtractionOptions) -> Result<()>` | extract one entry by path to disk |
| `fn extract_files(&self, paths: &[&str], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | extract a named set of entries; an empty slice is a no-op |
| `fn extract_by_ids(&self, ids: &[usize], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | extract by listing id; an empty slice is a no-op |
| `fn extract_some<F>(&self, predicate: F, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> where F: FnMut(&ArchiveEntry) -> bool` | extract the entries a predicate accepts |
| `fn extract_filtered<F>(&self, predicate: F, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> where F: FnMut(&ArchiveEntry) -> bool` | delegates to `extract_some` |
| `fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>>` | decode one entry into a `Vec<u8>` with default options |
| `fn extract_to_memory_with_options(&self, file_path: &str, options: &ExtractionOptions) -> Result<Vec<u8>>` | same, honouring password, limits, and CRC verification |
| `fn extract_to_stream(&self, file_path: &str, bound: StreamBound) -> Result<StreamingExtractor>` | expose one entry as a `Read` under the chosen output bound |
| `fn extract_to_stream_with_options(&self, file_path: &str, options: &ExtractionOptions, bound: StreamBound) -> Result<StreamingExtractor>` | same, honouring password and limits |

Choosing among these: [How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md).

### Creation

Creation handles come from the associated functions; the `add_*` methods then require
`&mut self` and a write-mode handle.

| Signature | Description |
|---|---|
| `fn create(path: impl AsRef<Path>, options: CompressionOptions) -> Result<Self>` | open a write handle after validating the options against the format |
| `fn create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Self>` | typed ZIP entry point |
| `fn create_seven_zip(path: impl AsRef<Path>, opts: SevenZCompressionOptions) -> Result<Self>` | typed 7z entry point |
| `fn create_libarchive(path: impl AsRef<Path>, opts: LibarchiveCompressionOptions) -> Result<Self>` | typed libarchive entry point; rejects `ArchiveFormat::Zip` |
| `fn add_file_from_data(&mut self, path: &str, data: &[u8]) -> Result<()>` | write one entry from a byte slice |
| `fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()>` | write one entry, reusing the filesystem path as the archive path |
| `fn add_file_from_path_as(&mut self, fs_path: impl AsRef<Path>, archive_path: &str) -> Result<()>` | write one entry under a chosen archive path |
| `fn add_directory(&mut self, path: &str) -> Result<()>` | write an explicit directory entry |
| `fn add_directory_recursive(&mut self, path: impl AsRef<Path>) -> Result<()>` | walk a filesystem directory and write its contents |

Recipe: [How to create an archive in a given format](../../../how-to/user/en/create-an-archive.md).

### Modification

A modify handle queues operations and applies them at commit time.

| Signature | Description |
|---|---|
| `fn modify(path: impl AsRef<Path>) -> Result<Self>` | open a modify handle, taking an advisory lock on the file |
| `fn modify_with_options(path: impl AsRef<Path>, options: ModificationOptions) -> Result<Self>` | same, with backup and metadata-preservation settings |
| `fn add_entry(&mut self, path: &str, data: &[u8]) -> Result<()>` | queue an addition from a byte slice |
| `fn add_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()>` | queue an addition read from disk at commit time |
| `fn add_entry_from_reader<R>(&mut self, archive_path: &str, reader: R, size: Option<u64>) -> Result<()> where R: Read + Send + 'static` | queue an addition from an owned reader |
| `fn add_directory_entry(&mut self, path: &str) -> Result<()>` | queue an explicit directory entry |
| `fn remove_entry(&mut self, path: &str) -> Result<usize>` | queue removal of every record with that path; returns how many matched |
| `fn remove_entry_by_id(&mut self, id: usize) -> Result<()>` | queue removal of one record by its listing index |
| `fn replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()>` | remove then add, from a byte slice |
| `fn replace_entry_from_path(&mut self, path: &str, fs_path: &Path) -> Result<()>` | remove then add, from disk |
| `fn replace_entry_from_reader<R>(&mut self, path: &str, reader: R, size: Option<u64>) -> Result<()> where R: Read + Send + 'static` | remove then add, from an owned reader |
| `fn pending_operations(&self) -> Result<usize>` | queued additions, removals, and directory entries; `OperationBlocked` outside modify mode |
| `fn clear_operations(&mut self) -> Result<()>` | discard the queue; `OperationBlocked` outside modify mode |
| `fn commit_changes(mut self) -> Result<()>` | apply the queue by rewriting the archive; consumes the handle |

Why the archive is rewritten: [Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md).
Recipe: [How to add, replace, or remove entries in an existing archive](../../../how-to/user/en/modify-a-zip-or-7z.md).

### Lifecycle

| Signature | Description |
|---|---|
| `fn finish(mut self) -> Result<()>` | finalize a write handle; consumes it |
| `fn close(self) -> Result<()>` | delegates to `finish`, same contract |

`finish` behaves by mode: read handles are a no-op; write handles finalize the backend; a
modify handle with queued operations returns `OperationBlocked` naming the pending count
rather than dropping them, and a modify handle with an empty queue closes silently. A write
handle poisoned by an earlier failure returns `OperationBlocked` instead of finalizing.

`Archive` implements `Drop`. On drop of a write-mode handle that was never finalized, it
attempts the finalize anyway and prints a message to standard error naming the path — either
that the silent finalize failed, or that a poisoned handle was dropped and the output should
not be considered durable. Modify-mode and read-mode drops do nothing: `commit_changes` is
the durability boundary for modify handles. Drop cannot return an error, so `finish` or
`close` is the only way to observe a finalize failure.

## v2 typed handles (`v2-api` feature)

With `features = ["v2-api"]`, `unified_archive::v2` exports three handles that split the
`Archive` sum type by mode. They delegate to `Archive` rather than reimplementing it, so
behaviour is unchanged; the change is structural — a `WriteArchive` cannot be passed to an
extraction call, and `WriteArchive::finish` consumes the handle. They are additive in 0.3;
the feature is planned to become default in 0.4.

### `ReadArchive`

Constructors: `open(path: impl AsRef<Path>)`, `open_encrypted(path, password: impl AsRef<str>)`,
`open_at_offset(path, offset: u64)`, `open_sfx(path)`,
`open_with_sfx_progress(path, progress)`. Associated functions
`detect_sfx(path) -> Result<SfxDetectionResult>` and
`extract_stub(path, detection: &SfxDetectionResult) -> Result<Vec<u8>>` need no handle.

Methods mirror the read side of `Archive`: `path`, `format`, `extension_format`,
`is_encrypted`, `is_solid`, `has_recovery_record`, `recovery_percentage`, `list_files`,
`list_files_for_limits`, `entry_count`, `find_entry`, `find_entries`, `validate_integrity`,
`multipart_layout`, `detect_multipart`, `check_symlinks`, `calculate_archive_crc`,
`calculate_manifest_digest`, `calculate_manifest_summary`,
`calculate_content_multiset_digest_and_size`, `extract_all`, `extract_file`,
`extract_files`, `extract_by_ids`, `extract_some`, `extract_filtered`, `extract_to_memory`,
`extract_to_memory_with_options`, `extract_to_stream`, `extract_to_stream_with_options`.

### `WriteArchive`

Constructors: `create(path, options: CompressionOptions)`, `create_zip(path, opts)`,
`create_seven_zip(path, opts)`, `create_libarchive(path, opts)`.

Methods: `path`, `format`, `add_file_from_data`, `add_file_from_path`,
`add_file_from_path_as`, `add_directory`, `add_directory_recursive`, `entry_count`, and the
consuming `finish(mut self) -> Result<PathBuf>` / `close(self) -> Result<PathBuf>` — both
return the written archive's path, where `Archive::finish` returns `()`. `WriteArchive`
implements `Drop`, which finalizes an unfinished handle once and warns, leaving the inner
`Archive`'s own `Drop` to do nothing.

### `ModifyArchive`

Constructors: `open(path)`, `open_with_options(path, opts: ModificationOptions)`.

Methods: `path`, `format`, `add_entry`, `add_entry_from_path`, `add_entry_from_reader`,
`add_directory_entry`, `replace_entry`, `replace_entry_from_path`,
`replace_entry_from_reader`, `list_files`, `entry_count`, `find_entry`, `find_entries`,
`remove_entry`, `remove_entry_by_id`, `clear_operations`, `pending_operations`,
`try_commit_changes(&mut self) -> Result<()>`, and `commit_changes(mut self) -> Result<()>`.

`try_commit_changes` has no equivalent on `Archive`: it validates and applies the queue
without consuming the handle, so a rejected commit leaves the handle usable.

Both `v2` paths resolve to the same types: `unified_archive::v2::ReadArchive` and
`unified_archive::archive::mode_split::ReadArchive`.
