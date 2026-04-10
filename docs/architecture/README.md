# Architecture: unified-archive

## System Purpose

Unified, format-agnostic Rust library for archive operations (inspection, extraction, creation, modification, SFX detection) across ZIP, 7z, RAR, RAR5, TAR, GZIP, BZIP2, XZ, and ISO. Inspired by 7zip-JBinding's design philosophy — write code once that automatically works for all supported archive formats.

## Local Run Target

```bash
cargo test                          # Full test suite (82+ tests)
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
| Public API | `lib.rs` | Crate surface and re-exports |
| Orchestration | `archive.rs`, `inspection.rs`, `extraction.rs`, `creation.rs`, `modification.rs` | Core `Archive` type, format detection, backend routing, operation workflows |
| Domain / Policy | `entry.rs`, `error.rs`, `format.rs`, `options.rs`, `security.rs`, `streaming.rs`, `stream_crc.rs` | Data types, error types, configuration objects, path sanitization, extraction limits |
| Backend Adapters | `ffi/wrapper.rs`, `ffi/libarchive_wrapper.rs`, `ffi/piz_wrapper.rs`, `ffi/sevenz_wrapper.rs`, `ffi/zip_wrapper.rs`, `ffi/zip_writer.rs` | Safe wrappers translating backend-specific behavior into unified domain types |
| Native Bindings | `ffi/unrar.rs`, `ffi/libarchive.rs`, `ffi/common.rs` | Raw C FFI declarations and shared backend utilities |
| SFX Pipeline | `sfx/detection.rs`, `sfx/signatures.rs`, `sfx/stub_types.rs` | 3-stage SFX detection (stub type -> signature scan -> validation) |
| External Tools | `external/rar.rs` (feature-gated) | Optional WinRAR CLI integration for RAR creation |
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
- **`sfx/*`** owns `SfxDetectionResult`, `StubType`, signature tables

## Persistence Overview

**None.** This is a stateless library. All state is in-memory within `Archive` handles and dropped when handles go out of scope. No database, no config files, no durable state.

## File-Handling Overview

- **Input:** Caller-provided archive files (read-only access)
- **Output:** Extracted files to caller-specified destinations; newly created archive files
- **Temp files:** Created during extract-to-memory and modification (RAII cleanup via `TempDirGuard` / `Drop`)
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
