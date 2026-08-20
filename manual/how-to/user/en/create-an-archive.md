---
type: How-To Guide
title: How to create an archive in a given format
description: Pick a creatable format, build the right options value, add entries, finalize the writer, and recognise the refusals this recipe runs into.
tags: [creation, formats, api, AD-0017, AD-0044]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: creation-facade, resource: src/creation.rs }
  - { id: options, resource: src/options.rs }
  - { id: format, resource: src/format.rs }
  - { id: zip-writer, resource: src/ffi/zip_writer.rs }
  - { id: libarchive-writer, resource: src/ffi/libarchive_wrapper/writer.rs }
  - { id: archive-facade, resource: src/archive.rs }
  - { id: internal-path-validation, resource: src/security.rs }
  - { id: example-create, resource: examples/create_archive.rs }
synced_hash: cc0a79fbcdc42bfb0f8789168d37f07a068c1365c1b0e5f1e7836d008d248d25
---

# How to create an archive in a given format

Creation is always the same four moves: confirm the format can be written, build a
compression-options value for it, add entries through a write-mode `Archive` handle,
and call `finish()`. This page covers the format-specific parts of that sequence and the
refusals you meet while following it.

## Before you start

```rust
use unified_archive::{
    Archive, ArchiveFormat, CompressionLevel, CompressionOptions,
    LibarchiveCompressionOptions, SevenZCompressionOptions, ZipCompressionOptions, Result,
};
```

Four preconditions hold for every format:

- The output path must not exist. There is no overwrite flag (AD 0017).
- No password. Encrypted creation is rejected for every format today. MADR-0027's
  amendment of 2026-07-20 downgraded that ban from permanent to deferred behind an
  explicit opt-in, and the opt-in has not shipped.
- `split_size` must stay `None`. No backend implements split-volume writing.
- One handle per thread. `Archive` is `Send` but not `Sync`.

`cargo run --example create_archive` walks a ZIP, a TAR.GZ, a 7z, and the
password rejection end to end; the source is `examples/create_archive.rs`.

## Confirm the format can be created

Ask the predicate rather than guessing from the format list:

```rust
let format = ArchiveFormat::TarZst;
if !format.can_create() {
    // Nothing to do here — Archive::create would refuse this format.
    return Ok(());
}
```

`ArchiveFormat::can_create` is the same predicate `Archive::create` consults, so its
answer and the facade's behaviour cannot drift. The complete creatable set, and the reason
each remaining variant is refused, live in the
[Format support matrix](../../../reference/user/en/format-support-matrix.md). Two
substitutions come up in practice: `Rar` and `Rar5` have no facade creation path at all,
and for a standalone single-file compressor such as `Gzip` or `Xz` you create the
matching `Tar*` compound instead.

`can_create` is a fixed set, but two members of it depend on the libarchive you link
against. `TarZst` and `TarLz4` need a libarchive built with the matching write filter;
without it, libarchive registers the filter as an external-program fallback and reports a
warning. The writer treats any non-OK return from a format or filter registration as a
hard `ArchiveError::Format` at construction time rather than silently shelling out to a
command-line compressor. `TarLzma` needs no separate codec: it goes through the same
liblzma that `TarXz` already requires. For getting those codecs in place, see
[How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md).

Which backend does the writing matters only when you are diagnosing an error message:
ZIP creation runs through the Rust `zip` crate, and every other creatable format —
7z included — runs through libarchive.

## One complete example

```rust
use std::path::Path;
use unified_archive::{Archive, ArchiveFormat, CompressionLevel, CompressionOptions, Result};

fn build_bundle(out: &Path, staging: &Path) -> Result<()> {
    let options = CompressionOptions {
        format: ArchiveFormat::TarGzip,
        level: CompressionLevel::Maximum,
        password: None,
        split_size: None,
        progress: None,
    };

    // Optional preflight: same gates as Archive::create, no filesystem access.
    options.validate_for_format()?;

    // Fails if `out` already exists.
    let mut archive = Archive::create(out, options)?;

    archive.add_file_from_data("MANIFEST", b"bundle 1\n")?;
    archive.add_file_from_path_as(staging.join("notes.md"), "docs/notes.md")?;
    archive.add_directory("empty-slot")?;
    archive.add_directory_recursive(staging.join("bin"))?;

    println!("{} entries written so far", archive.entry_count()?);

    // Consumes the handle; this is where flush, fsync, and errors surface.
    archive.finish()
}
```

The rest of this page is the per-topic detail behind those calls.

## Build the options

Three shapes construct the same `CompressionOptions` value that `Archive::create`
consumes.

**Struct literal.** All five fields are public and the type is not
`#[non_exhaustive]` in 0.3, so a literal compiles and is what the bundled examples
use:

```rust
let options = CompressionOptions {
    format: ArchiveFormat::SevenZip,
    level: CompressionLevel::Ultra,
    password: None,
    split_size: None,
    progress: None,
};
```

**Constructor.** `CompressionOptions::new(format)` gives `Normal` level and `None`
for password, `split_size`, and progress. `CompressionOptions::default()` is
`new(ArchiveFormat::Zip)`.

**Per-format builders.** Each pairs with its own `create_*` call and exposes only
the fields that format honours, so the combinations that `Archive::create` would
reject at runtime are not representable at all:

```rust
let mut archive =
    Archive::create_zip("out.zip", ZipCompressionOptions::new().level(CompressionLevel::Fast))?;
archive.add_file_from_data("MANIFEST", b"bundle 1\n")?;
archive.finish()?;
```

The other two calls take the same shape, with their own options type in the second
argument:

```rust
Archive::create_seven_zip("out.7z", SevenZCompressionOptions::new())
Archive::create_libarchive(
    "out.tar.xz",
    LibarchiveCompressionOptions::new(ArchiveFormat::TarXz).level(CompressionLevel::Maximum),
)
```

Each returns a write-mode handle that still needs its `add_*` calls and a `finish()`.

- `ZipCompressionOptions` — `level` and `progress`. Format is fixed to ZIP; no
  `password`, no `split_size`.
- `SevenZCompressionOptions` — `level` and `progress`. It has no password setter because
  encrypted creation is rejected for every format today; MADR-0027's 2026-07-20 amendment
  defers it behind an opt-in that has not shipped.
- `LibarchiveCompressionOptions::new(format)` — takes the target format up front,
  plus `level` and `progress`. It has no `Default`, since a format is required.
  `Archive::create_libarchive` rejects `ArchiveFormat::Zip` explicitly (use
  `create_zip` or `create` for that), and any other non-creatable format is caught by
  the shared `can_create` gate inside `create`.

Each builder lowers into `CompressionOptions` through `From`, and each `create_*`
call routes into `Archive::create`, so behaviour is identical to the struct-literal
path. What the type system cannot express — whether a backend can actually write the
requested format — is still validated at create time.

`CompressionOptions` does not implement `Clone`, because the boxed progress callback
cannot be duplicated. `strip_progress()` returns a fresh value with `progress`
cleared when you need to hand options to another operation.

Field-by-field detail and every default:
[Options and defaults](../../../reference/user/en/options-and-defaults.md).

## Choose a compression level

Pass one of `CompressionLevel`'s six variants; each backend maps it onto its own scale, and
the variant-by-variant mapping is in
[Options and defaults](../../../reference/user/en/options-and-defaults.md). Two behaviours
in that mapping will surprise you.

**`Store` on `TarZst` is not "no compression".** The zstd filter passes the value
straight to libzstd, where `0` means "use the library default" (level 3) rather than
"store". Requesting `Store` for `TarZst` therefore sends `1` — the weakest real
level — so the request stays directionally honest instead of quietly becoming
stronger compression than `Fastest`. Every other `TarZst` level passes through
unchanged. If you want genuinely uncompressed output, use `ArchiveFormat::Tar`.

**Plain `Tar` ignores the level entirely.** There is no filter and no format option
to set, so the field is accepted and unused.

A level the backend refuses is not silently downgraded: any non-OK return from
libarchive's option setters — including the "option ignored" warning — frees the
handle and returns `ArchiveError::Format` with the message
`compression-level option rejected: …` (AD 0013, as amended).

## Add content

Five calls put entries into a write-mode handle.

```rust
// In-memory bytes under an explicit archive-internal name.
archive.add_file_from_data("config.json", br#"{"version":"1.0"}"#)?;

// Filesystem file, stored under its own file name.
archive.add_file_from_path("./README.md")?;

// Filesystem file, stored under a name you choose.
archive.add_file_from_path_as("./build/app", "bin/app")?;

// A single directory entry, no contents.
archive.add_directory("logs")?;

// A whole tree.
archive.add_directory_recursive("./assets")?;
```

- `add_file_from_data` writes the buffer as one entry. No source metadata exists to
  carry, so none is stored.
- `add_file_from_path` derives the archive name from the source file name and
  delegates to `add_file_from_path_as`. If that file name is not valid UTF-8 the call
  fails with `InvalidPath` rather than substituting a lossy name (AD 0064) — pass an
  explicit name to `add_file_from_path_as` instead.
- `add_file_from_path_as` preserves the source file's modification time and, on Unix,
  its permission bits. The file's size is read once up front; if the file grows or
  shrinks between that stat and the copy, the add fails loudly instead of writing a
  byte count that disagrees with the header.
- `add_directory` emits one directory entry. Calling it twice for the same normalized
  path is a no-op success the second time, not a duplicate record.
- `add_directory_recursive` walks the tree with links never followed and children
  sorted per directory, so identical trees produce identical archive order. Entries
  are parent-rooted: the source directory's own name survives into the archive
  (`add_directory_recursive("foo/bar")` produces `bar/...` entries), matching `tar`,
  `zip -r`, and `7z a -r`. Non-empty directories are implied by their children, so
  only leaf directories get their own entry; an empty source directory still yields a
  single entry naming the root. Both backends produce the same layout.

**Archive-internal names are validated at this boundary** (AD 0044). Names you supply
to `add_file_from_data`, `add_file_from_path_as` (and therefore `add_file_from_path`),
and `add_directory` must be relative and clean. Rejected with `InvalidPath`: the empty
string, anything containing NUL, `..` segments, absolute or drive-prefixed paths, and a
leading `./` — so `"./file.txt"` is refused, and `"file.txt"` is what you want. The check
walks `std::path::Components`, which normalizes away every `.` except a leading one, so
`"dir/./file.txt"` is accepted and stored as written.
Backslashes are folded to forward slashes before the check, so `..\..\evil` is
rejected on Unix too rather than travelling as one innocent-looking component. The
extraction side would silently rewrite such names; the write side refuses them so a
name round-trips unchanged. `add_directory_recursive` derives its names from the
filesystem, so this check does not apply there.

**Collisions are caught before anything is written.** The facade tracks the names it
has promised: adding the same path twice, or claiming one path as both a file and a
directory (including a file that would live under a path already claimed as a file),
fails with `OperationBlocked`. For `add_directory_recursive` the entire tree is
pre-walked and checked before the backend emits a single entry, so a conflict deep in
the tree leaves the writer usable for a corrected retry.

**The writer's own output file cannot be an input.** Adding it directly, or having it
turn up inside a recursively-added tree, is refused as self-ingestion. Identity is
compared by `(device, inode)` on Unix and by volume serial plus file index on
Windows, so a differently-spelled path or a hard link to the output is caught too.

## Finalize, and what a drop does instead

```rust
archive.finish()?;   // or archive.close()?, which is the same call
```

`finish()` consumes the handle, flushes the backend, and is the durability boundary:
the ZIP writer writes the central directory and then `sync_data`s the file, the
libarchive writer closes the native handle and `sync_data`s the descriptor it owns.
Both then sync the parent directory best-effort. Any failure along that path — a full
disk, a codec finalize error — surfaces as the `Err` from this call.

Dropping the handle instead is a fallback, not an equivalent. `Drop` runs the same
best-effort finalize, but it cannot return anything, so a failure is printed to
stderr and lost:

```text
unified-archive: silent finalize during Drop for `out.tar.gz` failed: …
Call Archive::finish() / Archive::close() to surface this error explicitly.
```

If the handle is poisoned, `Drop` does not attempt the finalize at all — the backend's
state is undefined — and prints a warning that the output should not be treated as
durable. Always call `finish()` if the archive matters.

Poisoning happens when an `add_*` call fails partway through the backend write. After
that, every further `add_*` and `finish()` returns `OperationBlocked` telling you to
recreate the archive. There is no repair path; start a new output file.

While the handle is open, `archive.entry_count()?` reports how many entries the writer
has emitted so far — write progress, not the eventual total.

To attach a progress callback to creation, set `CompressionOptions::progress` (or the
builders' `.progress(…)`); the writer takes ownership of it at construction.
Cancelling from that callback stops future work rather than rolling back what is
already written. See
[How to report progress and cancel an operation](../../../how-to/user/en/report-progress-and-cancel.md).

## Preflight the configuration

`CompressionOptions::validate_for_format()` runs the three configuration gates without
touching the filesystem, in this order: `can_create`, then `password`, then
`split_size`. `Archive::create` calls it internally before constructing any writer,
so calling it yourself is optional — it exists for flows that want to validate a
configuration up front, such as confirming a user's choice in a UI before a file is
created.

Because that gate runs first, a configuration error is reported ahead of any
filesystem problem: options carrying a password fail with `OperationBlocked` even when
the output path already exists.

## The refusals you will hit

Four refusals account for almost every failed first attempt:

| Situation | What you get |
|---|---|
| Output file already exists | `ArchiveError::Io` whose inner kind is `AlreadyExists`, detected atomically by the writer's exclusive open rather than by a pre-check. Delete or rename the file yourself; there is no overwrite flag (AD 0017) |
| `password` is `Some` | `OperationBlocked` (MADR-0027). Reading encrypted archives is unaffected |
| Format is not in the creatable set | `OperationBlocked`, `format … is not supported for creation via Archive::create` |
| A libarchive write filter for `tar.zst` or `tar.lz4` is missing | `ArchiveError::Format` from `Archive::create`, before any entry is written |

Beyond those: a source path the writer cannot represent is refused at the `add_*` call that
names it, or during the recursive pre-walk — symlinks, sockets, FIFOs, and device nodes as
`OperationBlocked`, anything else that is not a regular file as `InvalidPath`. A progress
callback returning `ControlFlow::Break` ends the run as
`ArchiveError::Cancelled { operation: "create" }`, and a handle that is not in write mode is
rejected with `ReadOnlyBackend` before any path validation or file I/O.

Full variant list, messages, and refusal reasons:
[Errors and warnings](../../../reference/user/en/errors-and-warnings.md).

## Where next

- Changing an archive that already exists:
  [How to add, replace, or remove entries in an existing archive](../../../how-to/user/en/modify-a-zip-or-7z.md).
- Why one facade behaves this way across four different engines:
  [One API over many backends](../../../explanation/user/en/one-api-many-backends.md).
- Reading the result back:
  [How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md).
