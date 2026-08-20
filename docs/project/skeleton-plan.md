---
type: Skeleton Plan
title: "Structural Skeleton Plan (Retrospective)"
description: "This is a retrospective record."
tags: [project-control, ADR-0021, ADR-0020, ADR-0040, OI-0065-002]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# Structural Skeleton Plan (Retrospective)

> This is a retrospective record. The structural skeleton is the working implementation itself.

- **Governing baseline ID:** BL-001-retroactive
- **Stub manifest version:** 1

## Repo / Workspace Layout

- **Repo layout:** Single crate at repository root
- **Packages / workspace members:** `unified-archive` (library crate, no workspace)
- **Binaries / processes:** None (library only)
- **Shared package directories:** N/A (single crate)
- **Contract / codegen output:** `specs/001-unified-archive/contracts/` (6 prose contract files)

See `docs/implementation/workspace-topology.md` for full directory tree.

## Generation Order (As Implemented)

The actual implementation followed this dependency order:

1. **Domain types:** `src/entry.rs`, `src/error.rs`, `src/format.rs` (ArchiveEntry, ArchiveError, ArchiveFormat)
2. **Native bindings:** `src/ffi/unrar.rs`, `src/ffi/libarchive.rs`, `build.rs`
3. **Safe wrappers:** `src/ffi/wrapper.rs`, `src/ffi/libarchive_wrapper.rs`
4. **Core facade:** `src/archive.rs` (Archive, ArchiveBackend, format detection, backend routing)
5. **Policy objects:** `src/options.rs`, `src/security.rs` (ExtractionOptions, CompressionOptions, ExtractionLimits)
6. **Operations:** `src/inspection.rs` -> `src/extraction.rs` -> `src/creation.rs` -> `src/modification.rs`
7. **Additional backends:** `src/ffi/piz_wrapper.rs`, `src/ffi/sevenz_wrapper.rs`, `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs`
8. **SFX pipeline:** `src/sfx/detection.rs`, `src/sfx/signatures.rs`, `src/sfx/stub_types.rs`
9. **Streaming:** `src/streaming.rs`, `src/stream_crc.rs`
10. **Examples and tests:** `examples/`, `tests/`, `benches/`

## Local Execution Plan

- **Local run command:** `cargo test`
- **Smoke path:** Full test suite (867+ tests across unit, contract, and integration suites)
- **Backing services needed:** None (library crate; tests use archive fixtures)
- **Compose / equivalent:** Not needed

Additional local commands:
- `cargo clippy` — lint checks
- `cargo run --example inspect_archive -- <path>` — inspect any archive
- `cargo bench` — performance benchmarks

## Marker Plan

- **STUB markers:** None. Implementation complete for in-scope scenarios; SCN-CRE-04 and SCN-SFX-08 are deferred.
- **DEFERRED markers:** 8 items (see `docs/project/stub-manifest.md`):
  - `open_at_offset` — SFX offset-based opening
  - `split_size` — Multi-part archive creation
  - SecStr password migration
  - True streaming extraction
  - ZIP modification reliability
  - Creation progress callbacks (OI-025-003) — RESOLVED: per-entry progress invoked with `total=None` (AD 0021)
  - Unknown-stub SFX scanning (OI-027-001) — RESOLVED, then **superseded 2026-08-16 (R0070-0076):** unknown stubs are rejected at Stage 1 rather than proceeding to the signature scan. See SCN-SFX-08 in `docs/architecture/scenario-matrix.md`.
  - `ModificationOptions` (AD 0020) — all fields honored: `create_backup`/`backup_suffix`, `preserve_metadata` (modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries — OI-0065-002 resolved; directory metadata still uses backend defaults), `compression` override
- **Why DEFERRED (not STUB):** These items are explicitly out of MVP scope. The library is functional without them. Distinction between exposed and internal deferrals:
  - **Exposed implementation with caveats** (shipped behavior is real but constrained): `open_at_offset` — tempfile-backed implementation (closed 2026-04-18); payloads up to the ceiling in AD 0040 are materialized to a temp copy before opening. `open_sfx()` delegates through it.
  - **Hidden internal deferrals** (no user-visible surface): true streaming for non-libarchive backends, SecStr password migration, split archive creation.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| UnRAR wchar_t mismatch on Linux | Primary target is macOS; Linux testing deferred (IG-020-003) |
| ZIP modification unreliable via libarchive | Some test scenarios ignore-gated; planned fix: use zip-crate-native pipeline. `ModificationOptions` fully honored: backup, metadata preservation (OI-025-001/002 resolved), and compression override all functional via `modify_with_options()` (AD 0020) |
| extract-to-memory uses temp files for UnRAR | Libarchive reads directly into buffer; UnRAR uses temp files via FFI callbacks |
| Native backends buffer full entries in memory | Piz, ZipReader, and SevenZ buffer full entries in memory (not temp files) then wrap in Cursor; distinct from UnRAR's temp-file approach |
| Non-libarchive streaming wraps buffer in Cursor | Libarchive backends truly stream; others (including ZipReader, used as buffered backend for encrypted ZIP) buffer then wrap in Cursor |
