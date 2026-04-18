# Known Limitations

This document lists the important caveats for `unified-archive` `v0.1.0`.

It is intentionally public-facing: the goal is to help users decide whether a workflow is supported, partially supported, or unsupported.

For the task-oriented guide, see [docs/USER_MANUAL.md](./docs/USER_MANUAL.md).

## 1. Encrypted archive creation is not supported by `Archive::create`

`Archive::create()` rejects `CompressionOptions.password` for every format in `v0.1.0`.

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

- encrypted ZIPs are not transparently re-encrypted after modification
- ZIP64 boundary cases are not covered by dedicated large-fixture tests
- there is no crash-recovery journal; the operation is temporary-file + atomic-rename based

RAR, TAR-family, standalone compressed formats, and ISO are read-only.

## 3. Split-volume support is limited

Split archives are **not** uniformly supported across formats.

Supported end-to-end:

- RAR / RAR5 multi-part archives

Not supported end-to-end in `v0.1.0`:

- ZIP split volumes (`.z01`, `.z02`, ...)
- 7z numeric split volumes (`.001`, `.002`, ...)

Some helper logic and naming heuristics exist in the codebase, but public users should treat ZIP and 7z split-volume workflows as unsupported for this release.

## 4. Streaming is not uniformly bounded-memory

`extract_to_stream()` is available for all supported read formats, but its memory behavior depends on backend.

Bounded-memory streaming:

- TAR
- TAR.GZ
- TAR.BZ2
- TAR.XZ
- ISO

Buffered-before-streaming:

- ZIP
- 7z
- RAR / RAR5

So the API surface is unified, but the memory profile is not.

## 5. Standalone `.gz` / `.bz2` / `.xz` are read-only

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

The codebase includes Windows support paths, but `v0.1.0` is only tested on:

- macOS
- Linux

Treat Windows as experimental for now.

## 7. Some operations use temporary files intentionally

Current examples:

- `open_at_offset()` copies the embedded payload to a temporary file before opening it
- UnRAR-backed `extract_to_memory()` stages data through a temporary extraction path

This is expected behavior in `v0.1.0`.

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
