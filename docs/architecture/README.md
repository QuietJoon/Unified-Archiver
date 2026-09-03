---
type: Architecture Overview
title: "Architecture: unified-archive"
description: "Maintainer note: this directory is design and implementation material, not the release-facing contract."
tags: [architecture, ADR-0020, ADR-0059, OI-0065-002]
timestamp: 2026-08-09T00:00:00Z
status: active
---

# Architecture: unified-archive

> Maintainer note: this directory is design and implementation material, not the release-facing contract.
> Some documents capture intermediate investigations or decisions made before `v0.1.0` shipped.
> For current public behavior, start with `README.md`, `docs/USER_MANUAL.md`, `docs/API_REFERENCE.md`, and `Limitations.md`.

## System Purpose

Unified, format-agnostic Rust library for archive operations (inspection, extraction, creation, modification, SFX detection) across ZIP, 7z, RAR, RAR5, TAR-family formats, standalone compressed stream read-extract flows, and ISO. For the exact current support matrix, see `README.md`.

## Local Run Target

```bash
cargo test                                                # Full test suite
cargo run --example inspect_archive -- <archive>          # Inspect any archive file
cargo run --example extract_archive -- <archive> [dest]   # Extract any archive
cargo run --example create_archive                        # Create archives
```

## Topology

**Layered modular monolith** with a backend strategy/facade architecture.

Single library crate (`unified-archive`). No bundled CLI or service. Consumers add the crate as a Cargo dependency and call the Rust API directly. An optional Windows-only `external-rar-create` feature can shell out to `rar.exe`, but the core library workflows are in-process.

## Binary / Process Inventory

| Artifact | Type | Purpose |
|---|---|---|
| `unified-archive` | Library crate | The only artifact. Consumers depend on it via Cargo.toml. |

## Major Module Groups

| Layer | Modules | Responsibility |
|---|---|---|
| Public API | `src/lib.rs` | Crate surface and re-exports |
| Orchestration | `src/archive.rs`, `src/inspection.rs`, `src/extraction.rs`, `src/creation.rs`, `src/modification.rs` | Core `Archive` type, format detection, backend routing, operation workflows |
| Domain / Policy | `src/entry.rs`, `src/error.rs`, `src/format.rs`, `src/options.rs`, `src/security.rs`, `src/streaming.rs`, `src/stream_crc.rs` | Data types, error types, configuration objects, path sanitization, extraction limits |
| Backend Adapters | `src/ffi/wrapper.rs`, `src/ffi/libarchive_wrapper.rs`, `src/ffi/sevenz_wrapper.rs`, `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs` | Safe wrappers translating backend-specific behavior into unified domain types |
| Native Bindings | `src/ffi/unrar.rs`, `src/ffi/libarchive.rs`, `src/ffi/common.rs` | Raw C FFI declarations and shared backend utilities |
| SFX Pipeline | `src/sfx.rs` (module root / re-exports), `src/sfx/detection.rs`, `src/sfx/signatures.rs`, `src/sfx/stub_types.rs`, `src/sfx/result.rs`, `src/sfx/limits.rs` | 3-stage SFX detection (stub type -> signature scan -> plausibility screening; open validates payload) |
| External Tools | `src/external.rs`, `src/external/rar.rs` and `src/external/rar/*` — `argv`, `discovery`, `error`, `exit`, `runner`, `session`, `version` (feature-gated) | Optional RAR creation via external WinRAR CLI (`rar.exe`); the `external-rar-create` feature shells out to a system process, unlike all other backends which are in-process |
| Build | `build.rs` | Native toolchain orchestration for libarchive and UnRAR SDK |

## Ownership Overview

- **`archive.rs`** owns `Archive`, `ArchiveBackend`, `ArchiveMode`
- **`entry.rs`** owns `ArchiveEntry`, `EntryType`, `FileAttributes`
- **`error.rs`** owns `ArchiveError`
- **`format.rs`** owns `ArchiveFormat` variants and capabilities
- **`options.rs`** owns `ExtractionOptions`, `CompressionOptions`, `ProgressCallback`, `RateLimiter`
- **`security.rs`** owns `ExtractionLimits`, `sanitize_entry_path`
- **`modification.rs`** owns `ModificationTracker`, `ModificationOptions`
- Each `ffi/*_wrapper.rs` owns its backend struct (`UnrarArchive`, `LibarchiveArchive`, `SevenZArchive`, `ZipArchive`, `ZipWriter`)
- **`sfx.rs`** is the public module root; re-exports types from `sfx/*`
- **`sfx/*`** owns `SfxDetectionResult`, `StubType`, signature tables

## Persistence Overview

**No durable internal database or config state.** All in-memory state lives within `Archive` handles and is dropped when handles go out of scope. Extraction, SFX payload staging, and modification create temporary files on the filesystem. Cleanup is still RAII, but it is owned by concrete `tempfile` types rather than by a shared in-crate guard — the `TempDirGuard` named here previously was removed (`docs/records/AD-0059-r0069-wide-modular-design-closure.md`, R0001-0094). The live owners are:

| Temp artifact | Owning type | Commit / drop lifecycle |
|---|---|---|
| UnRAR extract-to-memory staging directory | `tempfile::TempDir` (`src/ffi/wrapper.rs`) | Collision-free name; the whole tree is removed when the call returns and the `TempDir` drops (R0069-0028 / R0069-0029) |
| Per-file atomic extraction write (ZIP / 7z, via `write_entry_atomically`) | `tempfile::NamedTempFile` behind `AtomicOutputFile` (`src/ffi/common.rs`) | Written as a randomly-named sibling of the destination; `commit()` consumes the value and renames it into place, and dropping it beforehand unlinks the partial file |
| Per-file atomic extraction write (UnRAR) | `tempfile::TempPath` sibling (`src/ffi/wrapper.rs`) | UnRAR needs a path rather than a handle, so the path is handed to the library and mirrors `AtomicOutputFile`'s semantics: rename on success, unlink on drop |
| Staged SFX payload | `tempfile::TempPath` held in `Archive::_backing_tempfile` (`src/archive.rs`) | Created by `Archive::open_at_offset`; lives exactly as long as the returned `Archive` handle and is unlinked when that handle drops |
| Modification rewrite target | Hand-named sibling path, not a `tempfile` type (`src/modification.rs`) | `commit_changes` builds `<archive>.tmp.<pid>.<nanos>.<counter>`, hands it to the creation API, and either consumes it via the atomic rename or removes it explicitly on every failure path |

Backup archive creation during modification: `create_backup` and `backup_suffix` are honored via `modify_with_options()` (AD 0020); `preserve_metadata` preserves modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries (where the backend supports the timestamp), per OI-0065-002. Directory metadata still uses backend defaults.

## File-Handling Overview

- **Input:** Caller-provided archive files (read-only access)
- **Output:** Extracted files to caller-specified destinations; newly created archive files
- **Temp files:** Created when SFX offset opening has to *stage* — ZIP, RAR and 7z payloads open in place (DCR-015) and allocate nothing, while libarchive-backed payloads and any declined open stage into a `tempfile::TempPath` owned by the returned `Archive` — the UnRAR extract-to-memory path (`tempfile::TempDir`, dropped when the call returns), per-file atomic extraction writes (`AtomicOutputFile` over `tempfile::NamedTempFile` — renamed into place on commit, unlinked on drop), and modification (self-named sibling temp archive, consumed by the atomic rename or explicitly removed on failure). `TempDirGuard` no longer exists — see the Persistence Overview table above and `docs/records/AD-0059-r0069-wide-modular-design-closure.md` (R0001-0094)
- **Streaming:** Native backends (ZIP, SevenZ, UnRAR) buffer full entries into memory for `extract_to_stream()`; only libarchive-backed formats provide true streaming reads
- **Modification:** Copy-on-write rewrite with atomic rename

See `docs/architecture/persistence-and-files.md` for detailed file lifecycle.

## Non-Goals

- Database or persistent state of any kind
- Network operations or remote archive access
- GUI or CLI binary (library only)
- Platform-specific public API surface
- JVM dependency or Java interop
- Interactive password prompts

## Key Constraints

- Native toolchain required: C++ compiler for the bundled UnRAR sources (driven by the `cc` crate), `pkg-config` + libarchive on non-Windows
- RAR format is proprietary: the vendored UnRAR sources are free of charge for handling RAR archives, but may not be used to build a RAR-compatible archiver or re-create the RAR compression algorithm
- `Archive` is `Send` but not `Sync` — one handle per thread
- Symlinks/hardlinks intentionally skipped during extraction with warnings
- Rust 2024 edition (minimum 1.85): all `extern` blocks must be `unsafe`, explicit unsafe blocks inside `unsafe fn`
