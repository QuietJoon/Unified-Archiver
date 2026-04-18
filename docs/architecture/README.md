# Architecture: unified-archive

## System Purpose

Unified, format-agnostic Rust library for archive operations (inspection, extraction, creation, modification, SFX detection) across ZIP, 7z, RAR, RAR5, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, and ISO. Standalone GZIP, BZIP2, and XZ streams are not directly openable; they are supported only as TAR compound formats per AD 0018. Inspired by 7zip-JBinding's design philosophy — shared API surface across all supported formats, with backend-specific caveats documented per operation.

## Local Run Target

```bash
cargo test                          # Full test suite (867+ tests across unit, contract, and integration suites)
cargo run --example inspect_archive # Inspect any archive file
cargo run --example extract_archive # Extract any archive
cargo run --example create_archive  # Create archives
```

## Topology

**Layered modular monolith** with a backend strategy/facade architecture.

Single library crate (`unified-archive`). No binaries, no processes, no services. Consumers add the crate as a Cargo dependency and call the Rust API directly.

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
| Backend Adapters | `src/ffi/wrapper.rs`, `src/ffi/libarchive_wrapper.rs`, `src/ffi/piz_wrapper.rs`, `src/ffi/sevenz_wrapper.rs`, `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs` | Safe wrappers translating backend-specific behavior into unified domain types |
| Native Bindings | `src/ffi/unrar.rs`, `src/ffi/libarchive.rs`, `src/ffi/common.rs` | Raw C FFI declarations and shared backend utilities |
| SFX Pipeline | `src/sfx.rs` (module root / re-exports), `src/sfx/detection.rs`, `src/sfx/signatures.rs`, `src/sfx/stub_types.rs` | 3-stage SFX detection (stub type -> signature scan -> validation) |
| External Tools | `src/external/rar.rs` (feature-gated) | Optional RAR creation via external WinRAR CLI (`rar.exe`); the `external-rar-create` feature shells out to a system process, unlike all other backends which are in-process |
| Build | `build.rs` | Native toolchain orchestration for libarchive and UnRAR SDK |

## Ownership Overview

- **`archive.rs`** owns `Archive`, `ArchiveBackend`, `ArchiveMode`
- **`entry.rs`** owns `ArchiveEntry`, `EntryType`, `FileAttributes`
- **`error.rs`** owns `ArchiveError`
- **`format.rs`** owns `ArchiveFormat` (12 variants + capabilities)
- **`options.rs`** owns `ExtractionOptions`, `CompressionOptions`, `ProgressCallback`, `RateLimiter`
- **`security.rs`** owns `ExtractionLimits`, `sanitize_entry_path`
- **`modification.rs`** owns `ModificationTracker`, `ModificationOptions`
- Each `ffi/*_wrapper.rs` owns its backend struct (`UnrarArchive`, `LibarchiveArchive`, `PizArchive`, `SevenZArchive`, `ZipArchive`, `ZipWriter`)
- **`sfx.rs`** is the public module root; re-exports types from `sfx/*`
- **`sfx/*`** owns `SfxDetectionResult`, `StubType`, signature tables

## Persistence Overview

**No durable internal database or config state.** All in-memory state lives within `Archive` handles and is dropped when handles go out of scope. Extraction and modification operations create temporary files on the filesystem; these are cleaned up via RAII (`TempDirGuard` / `Drop`) under normal execution. Backup archive creation during modification: `create_backup` and `backup_suffix` are honored via `modify_with_options()` (AD 0020); `preserve_metadata` preserves timestamps and Unix permissions via metadata-aware add helpers (OI-025-002 resolved 2026-04-14).

## File-Handling Overview

- **Input:** Caller-provided archive files (read-only access)
- **Output:** Extracted files to caller-specified destinations; newly created archive files
- **Temp files:** Created during extract-to-memory and modification (RAII cleanup via `TempDirGuard` / `Drop`)
- **Streaming:** Native backends (Piz, ZipReader, SevenZ, UnRAR) buffer full entries into memory for `extract_to_stream()`; only libarchive-backed formats provide true streaming reads
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

- Native toolchain required: `make` + C++ runtime for UnRAR, `pkg-config` + libarchive on non-Windows
- RAR format is proprietary: UnRAR license (free for non-commercial use)
- `Archive` is `Send` but not `Sync` — one handle per thread
- ZIP mmap (Piz backend) bounded by `UNIFIED_ARCHIVE_MAX_MMAP_SIZE`
- Symlinks/hardlinks intentionally skipped during extraction with warnings
- Rust 2024 edition (minimum 1.85): all `extern` blocks must be `unsafe`, explicit unsafe blocks inside `unsafe fn`
