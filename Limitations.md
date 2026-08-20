# Known Limitations

This document lists the important caveats for `unified-archive` `v0.4.0`.

It is intentionally public-facing: the goal is to help users decide whether a workflow is supported, partially supported, or unsupported.

For the task-oriented guide, see [docs/USER_MANUAL.md](./docs/USER_MANUAL.md).

## 1. Encrypted archive creation is not supported by `Archive::create`

`Archive::create()` rejects `CompressionOptions.password` for every format in `v0.4.0`.

That means:

- ZIP creation with a password is rejected
- 7z creation with a password is rejected
- TAR-family creation with a password is rejected

The library **does** support encrypted archive reading through `Archive::open_encrypted()` for:

- RAR / RAR5
- ZIP
- 7z

### Optional exception: external RAR creation

There is an optional Windows-only helper behind the `external-rar-create` feature:

- module: `unified_archive::external`
- type: `external::RarCreator`
- dependency: licensed WinRAR / `rar.exe`

This is not part of the `Archive::create()` facade and should be treated as a separate integration.

## 2. Modification is rewrite-based and limited to ZIP / 7z

`Archive::modify()` and `Archive::modify_with_options()` are supported only for:

- ZIP
- 7z

Modification is implemented by recreating the archive and swapping the result into place. It is not an in-place editor.

Current behavior:

- `modify_with_options()` can create a backup before commit
- `modify_with_options()` can preserve timestamps and Unix permissions on retained entries
- ZIP rewrites preserve the archive comment and stored/deflated compression method

Current caveats:

- modification of an encrypted archive is refused up front: after the capability check, `Archive::modify` probes the source and returns `OperationBlocked` labelled `modify` with the reason "Encrypted archives cannot be modified (password-aware modification not yet supported)". Encryption-shaped open/listing failures (header-encrypted formats) are remapped to the same error, so an encrypted archive can never reach `commit_changes` and lose its encryption
- the rewrite drops symlink, hardlink, and other special entries entirely — they are not re-emitted into the replacement archive, so a commit is lossy for archives containing them. The dry-run namespace gate models the same drop, so a dropped link's path does not block an added entry at that path (`rewrite_drops_entry_type` in `src/modification.rs`)
- ZIP64 boundary cases are not covered by dedicated large-fixture tests
- there is no crash-recovery journal; the operation is temporary-file + atomic-rename based
- because commit installs a **new inode** via atomic rename, only the original archive's Unix **mode bits** are preserved (copied onto the replacement to prevent access-widening). Ownership (uid/gid), POSIX ACLs, extended attributes / security labels (xattrs, SELinux), and the archive file's own timestamps are **not** carried across the replace — the new file has fresh ownership/xattrs and the rewrite's timestamps. Re-apply these yourself after commit if your workflow depends on them. A backup created with `with_backup` follows the same contract: it inherits the original's mode bits but not its ownership/ACLs/xattrs/timestamps.

RAR, TAR-family, standalone compressed formats, and ISO are read-only.

## 3. Split-volume support is limited

Split archives are **not** uniformly supported across formats.

Supported end-to-end:

- RAR / RAR5 multi-part archives

Not supported end-to-end in `v0.4.0`:

- ZIP split volumes (`.z01`, `.z02`, ...)
- 7z numeric split volumes (`.001`, `.002`, ...)

Some helper logic and naming heuristics exist in the codebase, but public users should treat ZIP and 7z split-volume workflows as unsupported for this release.

## 4. Streaming is not uniformly bounded-memory

`extract_to_stream()` is available for all supported read formats, but its memory behavior depends on backend.

Bounded-memory streaming:

- TAR family (including TAR.GZ / TAR.BZ2 / TAR.XZ / TAR.ZST / TAR.LZ4 / TAR.LZMA)
- standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma`
- ISO

Buffered-before-streaming:

- ZIP
- 7z
- RAR / RAR5

So the API surface is unified, but the memory profile is not.

The buffer is, however, bounded by what the caller asked for: the backend receives
`min(StreamBound, max_file_size, max_total_size)` as its materialization budget, so an entry whose
declared size exceeds that budget is refused before it is buffered rather than after. A practical
consequence: `StreamBound::Cap(n)` below an entry's declared size fails at the extract call on the
buffered backends, while the incremental libarchive path serves a prefix up to `n`. To read a window
of a larger entry portably, wrap a `StreamBound::DeclaredSize` stream in `Read::take`.

## 5. Standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma` are read-only

Standalone compressed files are supported for:

- open / inspect
- extraction

They are **not** supported for:

- creation
- modification

If you need compressed archive creation, use:

- `.tar.gz`
- `.tar.bz2`
- `.tar.xz`

## 6. Windows support is not release-verified yet

The codebase includes Windows support paths, but `v0.4.0` is only tested on:

- macOS
- Linux

Treat Windows as experimental for now.

## 7. Some operations use temporary files intentionally

Current examples:

- `open_at_offset()` copies the embedded payload to a temporary file before opening it
- UnRAR-backed `extract_to_memory()` stages data through a temporary extraction path

This is expected behavior in `v0.4.0`.

## 8. Encrypted-header RAR requires the password up front

RAR archives created with encrypted headers (`-hp`) cannot be listed without the password.

This is a format/backend constraint rather than a UI choice.

Use `Archive::open_encrypted()` immediately for those archives.

## 9. CRC32 behavior varies by backend

The library exposes a unified integrity API, but the exact verification mechanism differs by backend:

- some backends expose stored CRC32 directly
- some compute or validate checksums during reads
- some validations are effectively read/test passes rather than stored-CRC comparisons

For most users this is fine, but if you need backend-specific integrity semantics, consult the [API Reference](./docs/API_REFERENCE.md) and architecture notes.

## 10. Public API vs optional advanced modules

The stable public workflow centers on:

- `Archive`
- `ExtractionOptions`
- `CompressionOptions`
- `ModificationOptions`
- `StreamingExtractor`

The `ffi` module and the optional `external` module expose lower-level or platform-specific integration points. They are useful, but they are not the primary public API for general library use.
