---
type: Reference
title: Format support matrix
description: Complete per-format capability table, extension and backend mapping, and detection rules as ArchiveFormat defines them.
tags: [formats, archive, api]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: format, resource: src/format.rs }
  - { id: archive, resource: src/archive.rs }
  - { id: creation, resource: src/creation.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: libarchive-writer, resource: src/ffi/libarchive_wrapper/writer.rs }
synced_hash: 8a50642b9505ccbad83d31512ae3e3f6c739561ee01b22fdd9604485a824b255
---

# Format support matrix

This page describes `ArchiveFormat` as defined in `src/format.rs`, the capability values it
reports, the extension and backend mapping for each variant, and the detection rules that turn a
file into a variant. Values are read from the code, not from `README.md` or the crate-level
documentation in `src/lib.rs`.

## Vocabulary

### `Support`

`unified_archive::Support` is the per-capability grade. It is `#[non_exhaustive]` and derives
`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. Three variants exist today:

| Variant | Meaning as documented in `src/format.rs` |
|---|---|
| `Support::Full` | Fully supported and tested. |
| `Support::Partial` | Partially supported (for example, read works but write does not). |
| `Support::None` | Not supported. |

Because the enum is `#[non_exhaustive]`, a downstream `match` on it must carry a wildcard arm.

### `FormatCapabilities`

`unified_archive::FormatCapabilities` is the record returned by `ArchiveFormat::capabilities`.
It is `#[non_exhaustive]` and derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. Every field is
a `Support`:

| Field | Describes |
|---|---|
| `encryption_read` | Reading or decrypting an encrypted archive of this format. |
| `encryption_write` | Producing an encrypted archive of this format. |
| `multipart_read` | Reading a multipart (split, multi-volume) archive of this format. |
| `multipart_write` | Creating a multipart archive of this format. |
| `modification` | Adding, removing, or replacing entries in an existing archive. |
| `compression_read` | Decompression support. |
| `compression_write` | Compression support on the create side. |

`FormatCapabilities::compression()` collapses the read/write split into a single `Support`:
`Full` if either side is `Full`, otherwise `Partial` if either side is `Partial`, otherwise
`None`.

### Boolean predicates on `ArchiveFormat`

Each predicate is defined in terms of the capability record. All of them return `bool`.

| Method | Definition |
|---|---|
| `supports_compression()` | `capabilities().compression() != Support::None` |
| `supports_compression_read()` | `capabilities().compression_read != Support::None` |
| `supports_compression_write()` | `capabilities().compression_write != Support::None` |
| `supports_encryption_read()` | `capabilities().encryption_read != Support::None` |
| `supports_encryption_write()` | `capabilities().encryption_write != Support::None` |
| `supports_encryption()` | `supports_encryption_read() \|\| supports_encryption_write()` |
| `supports_multipart_read()` | `capabilities().multipart_read != Support::None` |
| `supports_multipart_write()` | `capabilities().multipart_write != Support::None` |
| `supports_multipart()` | `supports_multipart_read() \|\| supports_multipart_write()` |
| `can_modify()` | `capabilities().modification != Support::None` |
| `can_create()` | A fixed variant list; see the table below. |

The boolean forms collapse `Full` and `Partial` into the same `true`. `can_create` is not derived
from `FormatCapabilities`: it is an explicit `matches!` list of the variants
`Archive::create` accepts.

## Capability matrix

One row per `ArchiveFormat` variant, in the order the enum declares them. The enum is
`#[non_exhaustive]`, so this list is complete as of crate version 0.4.0 but is not closed.

*Open / inspect* and *Extract* record whether `Archive::open` routes the variant to a backend and
whether that backend serves extraction. *Create* is `ArchiveFormat::can_create`. The remaining
columns are the `Support` values in `ArchiveFormat::capabilities`.

| Variant | Open / inspect | Extract | Create via `Archive::create` | `modification` | `encryption_read` | `encryption_write` | `multipart_read` | `multipart_write` | `compression_read` | `compression_write` |
|---|---|---|---|---|---|---|---|---|---|---|
| `SevenZip` | yes | yes | yes | `Partial` | `Full` | `None` | `None` | `None` | `Full` | `Full` |
| `Zip` | yes | yes | yes | `Partial` | `Full` | `None` | `Partial` | `None` | `Full` | `Full` |
| `Rar` | yes (feature-gated) | yes (feature-gated) | no | `None` | `Full` | `None` | `Full` | `None` | `Full` | `None` |
| `Rar5` | yes (feature-gated) | yes (feature-gated) | no | `None` | `Full` | `None` | `Full` | `None` | `Full` | `None` |
| `Tar` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `None` | `None` |
| `TarGzip` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarBzip2` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarXz` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarZst` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarLz4` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarLzma` | yes | yes | yes | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `Gzip` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Bzip2` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Xz` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Zst` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Lz4` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Lzma` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Iso` | yes | yes | no | `None` | `None` | `None` | `None` | `None` | `None` | `None` |

`Tar` and `Iso` report `compression_read: None` and `compression_write: None` because neither
container applies a compression filter; both are still fully readable.

`encryption_write` is `None` for every variant. `Archive::create` rejects any
`CompressionOptions` that carries a password, for every format, with
`ArchiveError::OperationBlocked`.

`multipart_write` is `None` for every variant. `CompressionOptions::split_size` exists as a field
but `Archive::create` rejects `Some(_)` for every format.

## Extensions, canonical suffix, and backend

`ArchiveFormat::extensions()` returns the recognised extension strings **without** a leading dot.
`ArchiveFormat::suffix()` returns the canonical staging suffix **with** a leading dot; it is
crate-internal (`pub(crate)`) and is used when a self-extracting archive's payload is staged to a
temporary file, so the staged file still routes through the same filter chain.

| Variant | `extensions()` | `suffix()` | Read backend | Create backend |
|---|---|---|---|---|
| `SevenZip` | `7z` | `.7z` | `sevenz-rust2` | libarchive (`7zip` writer) |
| `Zip` | `zip` | `.zip` | `zip` crate | `zip` crate |
| `Rar` | `rar` | `.rar` | UnRAR | none via the facade |
| `Rar5` | `rar` | `.rar` | UnRAR | none via the facade |
| `Tar` | `tar` | `.tar` | libarchive | libarchive (`pax_restricted`) |
| `TarGzip` | `tar.gz`, `tgz` | `.tar.gz` | libarchive | libarchive (`pax_restricted` + gzip filter) |
| `TarBzip2` | `tar.bz2`, `tbz2`, `tb2` | `.tar.bz2` | libarchive | libarchive (`pax_restricted` + bzip2 filter) |
| `TarXz` | `tar.xz`, `txz` | `.tar.xz` | libarchive | libarchive (`pax_restricted` + xz filter) |
| `TarZst` | `tar.zst`, `tzst` | `.tar.zst` | libarchive | libarchive (`pax_restricted` + zstd filter) |
| `TarLz4` | `tar.lz4` | `.tar.lz4` | libarchive | libarchive (`pax_restricted` + lz4 filter) |
| `TarLzma` | `tar.lzma`, `tlz` | `.tar.lzma` | libarchive | libarchive (`pax_restricted` + lzma filter) |
| `Gzip` | `gz` | `.gz` | libarchive | none |
| `Bzip2` | `bz2` | `.bz2` | libarchive | none |
| `Xz` | `xz` | `.xz` | libarchive | none |
| `Zst` | `zst` | `.zst` | libarchive | none |
| `Lz4` | `lz4` | `.lz4` | libarchive | none |
| `Lzma` | `lzma` | `.lzma` | libarchive | none |
| `Iso` | `iso` | `.iso` | libarchive | none |

Read-side routing lives in `Archive::open_as_format`: `Rar`/`Rar5` go to UnRAR, `Zip` to the
`zip` crate, `SevenZip` to `sevenz-rust2`, and every remaining variant to libarchive.
Create-side routing lives in `Archive::create`: `Zip` goes to the `zip`-crate writer, and every
other creatable format to the libarchive writer — including `SevenZip`, whose read and write
backends therefore differ.

Modification opens its read side through libarchive for both `Zip` and `SevenZip`; `Rar`/`Rar5`
are rejected before that point.

The background to this split is in
[One API over many backends](../../../explanation/user/en/one-api-many-backends.md).

## Per-format notes

Only caveats that the code enforces are listed here.

**`Rar` / `Rar5` require the `rar-support` Cargo feature.** It is on by default. In a build
without it, `Archive::open` and `Archive::open_encrypted` return `ArchiveError::Unsupported` for
both variants with a reason naming the feature. The capability table is unconditional: it reports
the same values whether or not the feature is compiled in.

**`Rar` / `Rar5` cannot be created through the facade.** `can_create` is `false` and
`compression_write` is `None`. A separate creator, `unified_archive::external::RarCreator`, is
compiled only under `all(target_os = "windows", feature = "external-rar-create")`. It shells out
to a licensed WinRAR `rar.exe` and is not reachable through `Archive`.

**`Rar` and `Rar5` share the `rar` extension.** Extension-derived guessing therefore cannot tell
them apart, and `format_from_extension` returns `None` for `.rar` rather than choosing. Only
magic-byte detection distinguishes the two.

**`Zip` `multipart_read` is `Partial`.** `Archive::detect_multipart` and
`Archive::multipart_layout` enumerate sibling volumes (`.zip` plus `.zNN`) for formats whose
`supports_multipart()` is true, so a split ZIP set is *reported*. Extraction across those volumes
is not implemented end to end. `Rar`/`Rar5` are `Full`: their volume sets read through.

**`Zip` and `SevenZip` `modification` is `Partial`, not `Full`.** Modification is a
copy-on-write rewrite: entries are copied into a fresh archive, so symlinks, hard links, and
special entries are dropped and metadata and layout are normalised. For `SevenZip` the rewrite
also drops solid/block layout, encryption, and several 7z-specific metadata fields. See
[Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md).

**`TarZst`, `TarLz4`, and `TarLzma` creation depends on the linked libarchive.** The writer calls
`archive_write_add_filter_zstd`, `archive_write_add_filter_lz4`, or
`archive_write_add_filter_lzma`. A libarchive built without the matching codec library registers
the filter as an external-program fallback and returns a warning status, which the writer
converts into `ArchiveError::Format` at writer construction rather than shelling out to an
external compressor. Reading those formats needs only the corresponding read filter.

**`TarZst` treats `CompressionLevel::Store` as level 1.** zstd has no "no compression" level, so
the writer sends `1` instead of `0` for that format; every other creatable format receives the
level mapping unchanged.

**Standalone compressed streams are read-only.** `Gzip`, `Bzip2`, `Xz`, `Zst`, `Lz4`, and `Lzma`
report `compression_read: Full` and `compression_write: None`, and `can_create` is `false`.
Creating single-file compressed streams is out of scope; those codecs are producible only through
the `Tar*` compound formats.

**`Iso` is read-only.** libarchive's ISO support does not write, so `can_create` is `false`.

**`verify_crc32` is refused for libarchive-backed formats.** Setting
`ExtractionOptions::verify_crc32 = true` on a handle whose backend is libarchive — the `Tar*`
family, the standalone compressed streams, and `Iso` — returns `ArchiveError::Unsupported`,
because libarchive does not surface a per-entry CRC32. Per-backend detail is in
[Options and defaults](../../../reference/user/en/options-and-defaults.md).

## Detection

### `ArchiveFormat::detect(path)`

Content first, extension second.

1. The file is opened and one buffer is read. The buffer is 512 bytes, widened to 33 KiB when the
   path's lowercased extension is `iso`, `bin`, `img`, or `cd` — the ISO Primary Volume
   Descriptor sits at byte offset 32769, past the 512-byte window.
2. If fewer than 4 bytes were read, no content signal exists at all. `format_from_extension` is
   consulted and its answer is returned if it has one, without the `is_extension_fallback` gate
   described below. Otherwise the call fails with a format error reading
   `File too small to detect format`.
3. `detect_from_bytes` runs on the bytes that were read. On success the result is passed through
   compound-tar promotion (below) and returned.
4. If `detect_from_bytes` found nothing, `format_from_extension` is consulted, and its answer is
   accepted **only** when `is_extension_fallback()` is true for it.
5. Otherwise the call fails with a format error reading `Unknown archive format`.

Because content wins, a RAR file renamed `.zip` opens as RAR, and a corrupt `foo.zip` still fails
detection rather than being trusted on its extension.

`Archive::open` adds one further step of its own: when `detect` fails and the path's extension is
`exe`, `com`, `scr`, `app`, `run`, `sh`, or `bash`, it routes to the self-extracting-archive path.
See [How SFX detection decides](../../../explanation/developer/en/sfx-detection-pipeline.md).

### `ArchiveFormat::detect_from_bytes(magic)`

Takes raw bytes and uses no extension information. Fewer than 4 bytes returns a format error
reading `Buffer too small to detect format`. Probes run in this order, and the first match wins:

| Order | Variant | Gate |
|---|---|---|
| 1 | `Rar5` | Prefix `Rar!\x1A\x07\x01\x00`. |
| 2 | `Rar` | Prefix `Rar!\x1A\x07\x00`. |
| 3 | `Zip` | Prefix `PK\x03\x04`, or prefix `PK\x05\x06` with at least 22 bytes present (the full fixed end-of-central-directory record). The data-descriptor signature `PK\x07\x08` is excluded. |
| 4 | `SevenZip` | Prefix `7z\xBC\xAF\x27\x1C`. |
| 5 | `Gzip` | At least 10 bytes, bytes 0 and 1 are `0x1F 0x8B`, byte 2 is `0x08` (deflate), and the reserved flag bits (`0xE0`) of byte 3 are clear. |
| 6 | `Bzip2` | Prefix `BZh` followed by a block-size digit `1`–`9`. |
| 7 | `Xz` | Prefix `FD 37 7A 58 5A 00`. |
| 8 | `Zst` | Prefix `28 B5 2F FD`. Skippable-frame magics are not probed. |
| 9 | `Lz4` | Prefix `04 22 4D 18` (modern frame) or `02 21 4C 18` (legacy frame). |
| 10 | `Tar` | At least 512 bytes, `ustar` at offset 257, and the stored header checksum at offsets 148–155 matches the sum computed with that field taken as spaces. Both the unsigned and the historic signed byte-sum variants are accepted. |
| 11 | `Tar` | Pre-POSIX V7 fallback, tried only after the `ustar` probe fails: at least 512 bytes, `ustar` **absent**, the same header checksum valid, a non-empty name field, octal size and mtime fields, and a link flag of NUL, `0`, `1`, or `2`. |
| 12 | `Iso` | At least 32775 bytes, byte 32768 is `0x01`, bytes 32769–32773 are `CD001`, and byte 32774 is `0x01`. |

No match returns a format error reading `Unknown archive format from magic bytes`.

Two consequences follow from the buffer sizes. `Iso` is returned only when the caller supplies at
least 32775 bytes, which `detect`'s widened window and the self-extracting-archive scan buffer do
but the 512-byte default does not: an ISO named `mystery.dat` is not detected from file. And
`Lzma` has no probe at all — raw LZMA has no stable short magic marker — so it is never returned
by `detect_from_bytes`.

### `format_from_extension(path)`

A public, diagnostic-only helper. It returns `Some(format)` when the path's name maps cleanly to
one variant and `None` otherwise. Compound suffixes are checked before the single extension:

| Name ends with | Result |
|---|---|
| `.tar.gz`, `.tgz` | `TarGzip` |
| `.tar.bz2`, `.tbz2`, `.tb2` | `TarBzip2` |
| `.tar.xz`, `.txz` | `TarXz` |
| `.tar.zst`, `.tzst` | `TarZst` |
| `.tar.lz4` | `TarLz4` |
| `.tar.lzma`, `.tlz` | `TarLzma` |

Then, by lowercased extension: `zip` maps to `Zip`, `7z` to `SevenZip`, `tar` to `Tar`, `gz` to
`Gzip`, `bz2` to `Bzip2`, `xz` to `Xz`, `zst` to `Zst`, `lz4` to `Lz4`, `lzma` to `Lzma`, `iso` to
`Iso`. Everything else returns `None`, including two deliberate cases: `rar`, because the
extension cannot distinguish `Rar` from `Rar5`; and `bin`, `img`, `cd`, which widen `detect`'s
read window but do not claim ISO content.

`Archive::extension_format()` is this function applied to the open archive's path. Comparing it
with `Archive::format()` is how an extension/content mismatch is observed.

### `is_extension_fallback()`

The crate-internal predicate that gates step 4 of `detect`. It is true for exactly four variants,
those whose authoritative detection signal *is* the extension:

- `Tar` — no consistent magic across dialects.
- `Iso` — the Primary Volume Descriptor lies past the default read window.
- `Lzma` — no stable short magic marker.
- `TarLzma` — same reason.

Every other variant must pass magic-byte detection on its own. The wider map in
`format_from_extension` is therefore not the fallback set: it is a superset used for diagnostics.

### Compound-tar promotion

`detect_from_bytes` sees only the outer codec frame, so a `.tar.gz` reports as `Gzip`. After a
successful content detection, `detect` promotes the bare codec to its compound-tar variant when
the filename agrees:

| Detected | Filename ends with | Promoted to |
|---|---|---|
| `Gzip` | `.tar.gz`, `.tgz` | `TarGzip` |
| `Bzip2` | `.tar.bz2`, `.tbz2`, `.tb2` | `TarBzip2` |
| `Xz` | `.tar.xz`, `.txz` | `TarXz` |
| `Zst` | `.tar.zst`, `.tzst` | `TarZst` |
| `Lz4` | `.tar.lz4` | `TarLz4` |
| `Lzma` | `.tar.lzma`, `.tlz` | `TarLzma` |

Every other detected variant passes through unchanged, and a path with no filename passes through
unchanged. The `Lzma` row is unreachable from `detect_from_bytes`, which never returns `Lzma`; the
`.tar.lzma` and `.tlz` names reach `TarLzma` through the extension-fallback step instead. The same
helper is reused by the modification path's format detection.

The distinction matters for behaviour, not just naming: a compound variant carries
`compression_read: Full` and is creatable, while the bare codec variant it was promoted from is
read-only.

## See also

- [Options and defaults](../../../reference/user/en/options-and-defaults.md) — every option field,
  its type, and its default.
- [Errors and warnings](../../../reference/user/en/errors-and-warnings.md) — the error variants
  named on this page.
- [Public API surface](../../../reference/user/en/public-api-surface.md) — where these types are
  exported from.
- [Cargo features and MSRV](../../../reference/developer/en/cargo-features.md) — `rar-support`,
  `external-rar-create`, `v2-api`.
