# Structural Skeleton Plan (Retrospective)

> This is a retrospective record. The structural skeleton is the working implementation itself.

- **Governing baseline ID:** BL-001-retroactive
- **Stub manifest version:** 1

## Repo / Workspace Layout

- **Repo layout:** Single crate at repository root
- **Packages / workspace members:** `unified-archive` (library crate, no workspace)
- **Binaries / processes:** None (library only)
- **Shared package directories:** N/A (single crate)
- **Contract / codegen output:** `specs/001-unified-archive/contracts/` (7 prose contract files)

See `docs/implementation/workspace-topology.md` for full directory tree.

## Generation Order (As Implemented)

The actual implementation followed this dependency order:

1. **Domain types:** `entry.rs`, `error.rs`, `format.rs` (ArchiveEntry, ArchiveError, ArchiveFormat)
2. **Native bindings:** `ffi/unrar.rs`, `ffi/libarchive.rs`, `build.rs`
3. **Safe wrappers:** `ffi/wrapper.rs`, `ffi/libarchive_wrapper.rs`
4. **Core facade:** `archive.rs` (Archive, ArchiveBackend, format detection, backend routing)
5. **Policy objects:** `options.rs`, `security.rs` (ExtractionOptions, CompressionOptions, ExtractionLimits)
6. **Operations:** `inspection.rs` -> `extraction.rs` -> `creation.rs` -> `modification.rs`
7. **Additional backends:** `ffi/piz_wrapper.rs`, `ffi/sevenz_wrapper.rs`, `ffi/zip_wrapper.rs`, `ffi/zip_writer.rs`
8. **SFX pipeline:** `sfx/detection.rs`, `sfx/signatures.rs`, `sfx/stub_types.rs`
9. **Streaming:** `streaming.rs`, `stream_crc.rs`
10. **Examples and tests:** `examples/`, `tests/`, `benches/`

## Local Execution Plan

- **Local run command:** `cargo test`
- **Smoke path:** Full test suite (82+ tests across 16+ test modules)
- **Backing services needed:** None (library crate; tests use archive fixtures)
- **Compose / equivalent:** Not needed

Additional local commands:
- `cargo clippy` — lint checks
- `cargo run --example inspect_archive -- <path>` — inspect any archive
- `cargo bench` — performance benchmarks

## Marker Plan

- **STUB markers:** None. Implementation is complete for all in-scope scenarios.
- **DEFERRED markers:** 5 items (see `docs/project/stub-manifest.md`):
  - `open_at_offset` — SFX offset-based opening
  - `split_size` — Multi-part archive creation
  - SecStr password migration
  - True streaming extraction
  - ZIP modification reliability
- **Why DEFERRED (not STUB):** These items are explicitly out of MVP scope. The library is functional without them, and workarounds exist.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| UnRAR wchar_t mismatch on Linux | Primary target is macOS; Linux testing deferred (IG-020-003) |
| ZIP modification unreliable via libarchive | Some test scenarios ignore-gated; planned fix: use zip-crate-native pipeline |
| extract-to-memory uses temp files | Functional workaround; true in-memory extraction is a future optimization |
| Streaming wraps full buffer in Cursor | Provides correct API surface; true streaming is future work |
