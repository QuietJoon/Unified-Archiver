---
type: Reference
title: Errors and warnings
description: Every ArchiveError variant, its fields, its Display text and what raises it, plus the Operation labels, ArchiveWarning variants, and ResultWithWarnings.
tags: [api, archive, extraction, security]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-16T18:53:21Z
sources:
  - { id: error-module, resource: src/error.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: security, resource: src/security.rs }
  - { id: options, resource: src/options.rs }
  - { id: creation, resource: src/creation.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: backend-trait, resource: src/backend.rs }
  - { id: zip-backend, resource: src/ffi/zip_wrapper.rs }
synced_hash: 46ef41fe34385cd3324954646d0bf7bf3a8c59fe097ff0169a4804c8e7c024fb
---

# Errors and warnings

Everything on this page lives in `src/error.rs` unless stated otherwise. Three types make
up the error surface: `ArchiveError` (the single failure type), `ArchiveWarning` (a
non-fatal condition), and `ResultWithWarnings<T>` (a success value carrying warnings).

## `Result` alias

```rust
pub type Result<T> = std::result::Result<T, ArchiveError>;
```

Re-exported at the crate root, so `unified_archive::Result<T>` and
`unified_archive::error::Result<T>` are the same alias. Every fallible function in the
public API returns it.

## `ArchiveError`

`#[derive(Debug)]` and `#[non_exhaustive]`. It implements `std::fmt::Display` and
`std::error::Error`. It does **not** implement `Clone`, `PartialEq`, `Copy`, or
`serde::Serialize`, and there is no `impl From<std::io::Error> for ArchiveError`, so `?` does
not convert an `io::Error` into an `ArchiveError`. The only `From` impl in `src/error.rs` is
`impl From<Operation> for String`. `ArchiveError::io` is the constructor that wraps an
`io::Error`.

Because the enum is `#[non_exhaustive]`, a `match` over it in downstream code requires a
wildcard arm.

The variants, in declaration order:

| Variant | Fields |
|---|---|
| `Io` | `operation: String`, `path: PathBuf`, `source: std::io::Error` |
| `Format` | `format: Option<ArchiveFormat>`, `message: String` |
| `Corruption` | `path: String`, `details: String` |
| `Password` | `message: String` |
| `Unsupported` | `operation: String`, `format: ArchiveFormat`, `details: Option<String>` |
| `CodecUnavailable` | `codec: String`, `format: ArchiveFormat`, `install_instructions: String` |
| `WriteModeOnly` | `operation: String` |
| `ReadOnlyBackend` | `operation: String` |
| `NotImplemented` | `operation: String`, `reason: String` |
| `OperationBlocked` | `operation: String`, `reason: String` |
| `InvalidPath` | `path: String`, `reason: String` |
| `Cancelled` | `operation: &'static str` |

Where a `Display` text below interpolates a format, it does so with `{:?}` — the `Debug`
name of the `ArchiveFormat` variant (`Zip`, `Rar5`, `SevenZip`, `TarGzip`, `TarXz`, …), not
a file extension.

### `Io`

```text
I/O error during {operation}: {path} ({source})
```

`path` is rendered with `Path::display()`. `operation` is a short verb chosen by the call
site — `"open"`, `"read"`, `"write"`, `"copy"`, `"flush"` and similar — not one of the
`Operation` labels below.

Raised wherever a filesystem or descriptor call fails: opening or reading the archive
during format detection (`ArchiveFormat::detect` in `src/format.rs`), creating destination
directories and writing entry data during extraction, copying an embedded SFX payload to
its staging file (`src/archive.rs`), and every backend read/write path.

This is the only variant that carries a `source`.

### `Format`

```text
{format:?} format error: {message}
```

when `format` is `Some`, and

```text
Archive format error: {message}
```

when it is `None`.

Raised when a file cannot be identified as an archive at all — `ArchiveFormat::detect`
returns `Format { format: None, message: "Unknown archive format" }` when neither the magic
bytes nor an extension fallback match — and when a backend rejects a header or container
structure it did manage to identify.

### `Corruption`

```text
Corruption detected in '{path}': {details}
```

Raised on CRC32 verification failure, where `details` reads
`CRC32 mismatch: expected {expected:08X}, got {actual:08X}` (see
`security::verify_crc32_value` in `src/security.rs`), and on decode failures inside an entry
reported by the ZIP, libarchive, UnRAR, and 7z backends.

`path` names whichever object was found corrupt — usually the archive-internal entry path, but
several backend sites report the archive's own filesystem path instead (a corrupt stream map or
central directory belongs to the archive, not to any one entry). The wording is deliberately
neutral for that reason; do not parse it to decide which kind of path you were given.

The ZIP and 7z paths reach it through the shared helpers in `src/ffi/common.rs`:
`read_entry_to_memory_bounded` (and its `read_entry_to_memory_capped` wrapper) reports a
decoder that produces more than the declared entry size, `copy_with_optional_crc_bounded`
reports a short decode against an authoritative declared size, and `map_entry_read_error`
translates a decoder-reported checksum failure into `Corruption` with the details
`CRC32 mismatch reported by the entry decoder` rather than into `Io`.

CRC32 checking during extraction only happens when `ExtractionOptions::verify_crc32` is
set; `Archive::validate_integrity` always checks. See
[How to verify an archive's integrity](../../../how-to/user/en/verify-archive-integrity.md).

### `Password`

```text
Password error: {message}
```

Raised by the ZIP, 7z, RAR, and libarchive read paths when an entry or header is encrypted
and no password was supplied, or the supplied password is rejected. The `message` text is
backend-specific.

### `Unsupported`

```text
Operation '{operation}' not supported for {format:?} format: {details}
```

when `details` is `Some`, and

```text
Operation '{operation}' not supported for {format:?} format
```

when it is `None` — no trailing separator is emitted in that case.

Raised when the requested operation has no implementation for the detected format.

### `CodecUnavailable`

```text
{codec} codec not available for {format:?} format. {install_instructions}
```

No code path in the crate constructs this variant; it is produced only by
`ArchiveError::codec_unavailable` (see [Convenience constructors](#convenience-constructors))
and its own unit tests. A missing compression codec surfaces instead as a `Format` or `Io`
error from the backend.

### `WriteModeOnly`

```text
Operation '{operation}' cannot be performed: archive is in write mode
```

Raised when a read-side call is made on a handle returned by `Archive::create` (or one of
the typed `create_*` constructors). Concrete sites include
`Archive::has_recovery_record`, `Archive::recovery_percentage`, `Archive::is_solid`,
`Archive::detect_multipart`, and `Archive::extract_file` against the ZIP writer backend.

### `ReadOnlyBackend`

```text
Operation '{operation}' cannot be performed: backend is read-only
```

Raised when a write-side call is made on a handle that has no writer: the `as_write` and
`require_write_mode` gates in `src/creation.rs` reject every `add_*` call on a read handle.

The shared `finalize_write_backend` helper in `src/archive.rs` also has `ReadOnlyBackend`
arms, for the UnRAR, 7z, and ZIP-reader backends, but they are unreachable from the public
API: `Archive::finish` invokes the helper only in write mode, and write mode is entered only
through `Archive::create` (and the typed `create_*` constructors that delegate to it), which
builds a ZIP-writer or libarchive backend. `finish` on a read-mode handle returns `Ok(())`.

### `NotImplemented`

```text
Operation '{operation}' is not yet implemented: {reason}
```

Two sites raise it, both of them default implementations on the internal `ReadBackend`
trait (`src/backend.rs`), and both labelled with the operation `extract_to_stream`:
`extract_to_stream_by_listing_id`, whose `reason` reads `id-based streaming is not
implemented for backend {type name}`, and `visit_payloads_by_listing_id`, whose `reason`
reads `single-traversal payload resolution is not implemented for backend {type name}`.
The libarchive backend overrides both. The ZIP backend overrides the first, because its
AE-2 AES entries list no CRC32 and so must be read by listing id rather than by name. 7z and
RAR/RAR5 override neither, and the 7z backend really does reach the default: a 7z archive
that omits the optional `kCRC` digest lists `crc32 = None` for the affected entries — a
no-stream empty file is the usual case — so the digest walk falls through to the by-path
stream, which resolves them as long as their paths are unambiguous.

No public API surfaces the variant, and no caller ever observes it. Both raisers are consumed
inside the content-multiset digest walk in `src/inspection.rs`:
`Archive::calculate_content_multiset_digest_and_size` reads a `NotImplemented` from the
single-traversal attempt as "this backend has no one-pass walk" and resolves entry by entry
instead, and the per-entry resolver reads one from the id-addressed stream as "fall back to
the by-path stream". Either way the caller receives a digest or some other error. The variant
exists so a future backend that wires up neither method gets a defined answer instead of a
silently wrong one.

### `OperationBlocked`

```text
Operation '{operation}' cannot be performed: {reason}
```

The catch-all for policy refusals. There is no typed sub-kind — the distinction lives in
the `reason` string. Raised by, among others:

- `CompressionOptions::validate_for_format` (`src/options.rs`) and therefore
  `Archive::create`: a format for which `ArchiveFormat::can_create` is false
  (`format {:?} is not supported for creation via Archive::create`), a non-`None`
  `password` (`encrypted creation for {:?} is not supported (MADR-0027); password must be
  None`), and a non-`None` `split_size`
  (`split_size is not supported by any backend yet (DEF-002); leave it None`).
- Every `ExtractionLimits` gate in `src/security.rs`: total size, single-file size,
  compression ratio, and entry count.
- Symbolic and hard links reached by single-entry extraction, via the shared
  `link_extract_blocked` helper, whose reason reads
  `Entry '{path}' is a symbolic link; refusing to materialize per FR-022 link-skip policy`
  (or `hard link`).
- Out-of-range entry IDs, formatted by `invalid_id_reason`:
  `Invalid ID {id}: archive has {n} entries (valid IDs: 0-{n-1})`, or
  `Invalid ID {id}: archive has 0 entries; no valid IDs` for an empty archive.
- Write-mode namespace conflicts (duplicate paths, file-versus-directory clashes) and
  further `add_*` calls on a write handle that a previous failure poisoned.
- `CompressionRatio::new` and `CompressionRatio::whole` when an operand is zero.

The limit reasons are described alongside the limits themselves in
[Options and defaults](options-and-defaults.md) and
[The extraction safety model](../../../explanation/user/en/extraction-safety-model.md).

### `InvalidPath`

```text
Invalid path '{path}': {reason}
```

Raised by the path policy in `src/security.rs` — `sanitize_entry_path` and
`validate_archive_internal_path` reject traversal components, absolute paths, and
symlinked ancestors that would escape the destination — and by
`ArchiveEntry::try_file` / `ArchiveEntry::try_dir_at` for an empty path
(`ArchiveEntry path must not be empty`).

### `Cancelled`

```text
Operation '{operation}' was cancelled by caller
```

`operation` is a `&'static str`, so it can be compared against a literal directly. Raised
when a caller-supplied callback signals cancellation: a `ProgressCallback::on_progress`
returning `ControlFlow::Break(())`, or an `SfxStagingProgress` cancel callback returning
`false`.

Exactly three labels are emitted: `"sfx_staging"` for SFX payload staging in
`src/archive.rs`, `"extract_all"` for the backend extract-all loops, and `"create"` for
creation writes. `extract_files`, `extract_by_ids` and `extract_some` cancel through those
shared loops and therefore also report `"extract_all"`; single-entry `extract_file` passes
no cancellation hook to the write path and never yields this variant at all. See
[How to report progress and cancel an operation](../../../how-to/user/en/report-progress-and-cancel.md).

## Convenience constructors

All are associated functions on `ArchiveError`. Every string parameter takes
`impl Into<String>`.

| Constructor | Signature | Builds |
|---|---|---|
| `format` | `(format: Option<ArchiveFormat>, message: impl Into<String>)` | `Format` |
| `io` | `(operation: impl Into<String>, path: impl Into<PathBuf>, source: std::io::Error)` | `Io` |
| `corruption` | `(path: impl Into<String>, details: impl Into<String>)` | `Corruption` |
| `password` | `(message: impl Into<String>)` | `Password` |
| `invalid_path` | `(path: impl Into<String>, reason: impl Into<String>)` | `InvalidPath` |
| `unsupported` | `(operation: impl Into<String>, format: ArchiveFormat, details: Option<impl Into<String>>)` | `Unsupported` |
| `codec_unavailable` | `(codec: impl Into<String>, format: ArchiveFormat)` | `CodecUnavailable` |
| `write_mode_only` | `(operation: impl Into<String>)` | `WriteModeOnly` |
| `read_only_backend` | `(operation: impl Into<String>)` | `ReadOnlyBackend` |
| `not_implemented` | `(operation: impl Into<String>, reason: impl Into<String>)` | `NotImplemented` |
| `operation_blocked` | `(operation: impl Into<String>, reason: impl Into<String>)` | `OperationBlocked` |

There is no constructor for `Cancelled`; it is built as a struct literal.

Passing `None` to `unsupported` requires a concrete type for the elided `impl Into<String>`,
for example `None::<&str>`.

`codec_unavailable` fills `install_instructions` itself, from the codec name and the
`target_os` the crate was compiled for. The mapping is exhaustive as follows.

| `codec` | `macos` | `linux` | `windows` |
|---|---|---|---|
| `LZMA`, `LZMA2` | `Install p7zip: brew install p7zip` | `Install p7zip: sudo apt-get install p7zip-full (Debian/Ubuntu) or sudo yum install p7zip (RHEL/CentOS)` | `Install 7-Zip from https://www.7-zip.org/` |
| `BZIP2` | `Install bzip2: brew install bzip2` | `Install bzip2: sudo apt-get install bzip2 (Debian/Ubuntu) or sudo yum install bzip2 (RHEL/CentOS)` | `Install bzip2 from http://gnuwin32.sourceforge.net/packages/bzip2.htm` |
| `XZ` | `Install xz: brew install xz` | `Install xz: sudo apt-get install xz-utils (Debian/Ubuntu) or sudo yum install xz (RHEL/CentOS)` | `Install XZ Utils from https://tukaani.org/xz/` |

`RAR` and `RAR5` produce one platform-independent text:
`RAR/RAR5 support requires UnRAR library (already linked via FFI). If extraction fails, ensure UnRAR SDK is properly compiled.`

Any other codec name falls through to
`Install {codec} codec for your platform. See libarchive documentation: https://libarchive.org/`.
On a target that is not macOS, Linux, or Windows the platform is `"unknown"`, so every
codec name reaches that fallback.

## `std::error::Error::source`

```rust
fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
```

Returns `Some(source)` for `Io` and `None` for every other variant. `Io` also exposes its
`source` field directly, so the underlying `io::ErrorKind` is reachable without downcasting.
The variant's shape:

```rust
use unified_archive::ArchiveError;

match archive_result {
    Err(ArchiveError::Io { source, path, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
        eprintln!("missing: {}", path.display());
    }
    Err(other) => eprintln!("{other}"),
    Ok(_) => {}
}
```

## `Operation`

```rust
pub enum Operation { /* … */ }
```

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` and `#[non_exhaustive]`. It carries
the stable label used in the `operation` field of `Unsupported`, `OperationBlocked`,
`WriteModeOnly`, `ReadOnlyBackend`, `NotImplemented`, and `Cancelled`.

It implements `Display` (writing the label) and `From<Operation> for String`, so it can be
passed straight to any constructor above. The raw `as_str` accessor is crate-private:
outside the crate, use `Display` or `to_string()`.

The 24 variants and their labels, grouped by the operation family they name:

**Extraction**

| Variant | Label | Names |
|---|---|---|
| `Extract` | `extract` | shared per-entry safety checks, before a specific extract path is identified |
| `ExtractAll` | `extract_all` | `Archive::extract_all` |
| `ExtractFile` | `extract_file` | `Archive::extract_file` |
| `ExtractToMemory` | `extract_to_memory` | `Archive::extract_to_memory`, `extract_to_memory_with_options` |
| `ExtractToStream` | `extract_to_stream` | `Archive::extract_to_stream`, `extract_to_stream_with_options` |
| `ExtractFiles` | `extract_files` | `Archive::extract_files` |
| `ExtractByIds` | `extract_by_ids` | `Archive::extract_by_ids` |
| `ExtractSome` | `extract_some` | `Archive::extract_some`, `extract_filtered` |

**Inspection**

| Variant | Label | Names |
|---|---|---|
| `ListFiles` | `list_files` | `Archive::list_files` |
| `ListFilesForLimits` | `list_files_for_limits` | `Archive::list_files_for_limits` |
| `ValidateIntegrity` | `validate_integrity` | `Archive::validate_integrity` |

**Modification**

| Variant | Label | Names |
|---|---|---|
| `Modify` | `modify` | `Archive::modify`, `modify_with_options` |
| `AddEntry` | `add_entry` | `Archive::add_entry` |
| `RemoveEntry` | `remove_entry` | `Archive::remove_entry`, `remove_entry_by_id`, `replace_entry` |
| `CommitChanges` | `commit_changes` | `Archive::commit_changes` |
| `PendingOperations` | `pending_operations` | `Archive::pending_operations` |
| `ClearOperations` | `clear_operations` | `Archive::clear_operations` |
| `AddDirectoryEntry` | `add_directory_entry` | `Archive::add_directory_entry` |

**Creation**

| Variant | Label | Names |
|---|---|---|
| `AddFileFromData` | `add_file_from_data` | `Archive::add_file_from_data` |
| `AddFileFromPathAs` | `add_file_from_path_as` | `Archive::add_file_from_path_as` |
| `AddDirectory` | `add_directory` | `Archive::add_directory` |
| `AddDirectoryRecursive` | `add_directory_recursive` | `Archive::add_directory_recursive` |
| `Create` | `create` | `Archive::create` |

**Lifecycle**

| Variant | Label | Names |
|---|---|---|
| `Finish` | `finish` | `Archive::finish`, `Archive::close` |

Because the enum is `#[non_exhaustive]`, matching on it downstream needs a wildcard arm.

## `error::ops` constants

The `ops` module is declared `pub(crate) mod ops`, so these constants are **not** reachable
from outside the crate. They exist as `const` views over `Operation::as_str` for the
migration window; the public form of the same information is the `Operation` enum above.
The set is exactly one constant per `Operation` variant, with the same label:

- Extraction: `EXTRACT`, `EXTRACT_ALL`, `EXTRACT_FILE`, `EXTRACT_TO_MEMORY`,
  `EXTRACT_TO_STREAM`, `EXTRACT_FILES`, `EXTRACT_BY_IDS`, `EXTRACT_SOME`
- Inspection: `LIST_FILES`, `LIST_FILES_FOR_LIMITS`, `VALIDATE_INTEGRITY`
- Modification: `MODIFY`, `ADD_ENTRY`, `REMOVE_ENTRY`, `COMMIT_CHANGES`,
  `PENDING_OPERATIONS`, `CLEAR_OPERATIONS`, `ADD_DIRECTORY_ENTRY`
- Creation: `ADD_FILE_FROM_DATA`, `ADD_FILE_FROM_PATH_AS`, `ADD_DIRECTORY`,
  `ADD_DIRECTORY_RECURSIVE`, `CREATE`
- Lifecycle: `FINISH`

The labels are what appear inside `Display` text and inside the `operation` field. Because
the `ops` module is `pub(crate)`, the string produced by `Operation` is the only form of
those labels reachable from outside the crate.

## `ArchiveWarning`

```rust
pub enum ArchiveWarning { /* … */ }
```

`#[derive(Debug, Clone, PartialEq, Eq)]` and `#[non_exhaustive]`. It implements `Display`.
Warnings are non-fatal: the operation continues and completes.

### `SkippedSymlink`

Fields: `path: String`, `target: Option<String>`.

```text
Skipped symbolic link '{path}' -> '{target}' (FR-022: cross-platform symlink support not reliable)
```

when `target` is `Some`, and

```text
Skipped symbolic link '{path}' (FR-022: cross-platform symlink support not reliable)
```

when it is `None`.

Emitted by the libarchive, ZIP, 7z, and UnRAR extraction paths when a symlink entry is
reached, and by `Archive::check_symlinks` when it inspects a listing.

### `SkippedHardLink`

Field: `path: String`.

```text
Skipped hard link '{path}' (FR-022: limited cross-platform support)
```

Emitted by the libarchive and UnRAR extraction paths, and by `Archive::check_symlinks`.

### `OutputPathCaseCollision`

Fields: `first: String` (the entry that first claimed the output path), `second: String`
(the later entry that collides).

```text
Entries '{first}' and '{second}' differ only by case and merge on case-insensitive destination filesystems (R0079-0036)
```

Emitted by the extraction preflight in `src/extraction.rs`, which folds each resolved
output path with Unicode lowercasing and compares. The comparison does not detect
Unicode normalization-form collisions (NFC versus NFD). It is a warning rather than an
error because the destination filesystem's case sensitivity cannot be determined portably.

## `ResultWithWarnings<T>`

```rust
pub struct ResultWithWarnings<T> {
    pub value: T,
    pub warnings: Vec<ArchiveWarning>,
}
```

`#[derive(Debug)]` only — no `Clone`, no `PartialEq`. Both fields are public.

| Method | Signature | Behaviour |
|---|---|---|
| `ok` | `fn ok(value: T) -> Self` | wraps `value` with an empty warning list |
| `with_warnings` | `fn with_warnings(value: T, warnings: Vec<ArchiveWarning>) -> Self` | wraps `value` with the given warnings |
| `add_warning` | `fn add_warning(&mut self, warning: ArchiveWarning)` | pushes one warning onto the list |

Returned as `Result<ResultWithWarnings<()>>` by `Archive::extract_all`,
`extract_some`, `extract_filtered`, `extract_files`, and `extract_by_ids`. The other
extraction entry points return `Result<()>`, `Result<Vec<u8>>`, or
`Result<StreamingExtractor>` and therefore surface no warnings — see
[How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md).

An empty `warnings` vector means nothing was skipped; a non-empty one means the extraction
succeeded but some entries were not materialised:

```rust
let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```
