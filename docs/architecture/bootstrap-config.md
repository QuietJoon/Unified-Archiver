---
type: Bootstrap Config
title: "Bootstrap Config"
description: "This document describes how the project is run locally, how processes are started, and where environment / configuration is sourced from."
tags: [architecture, config, ADR-0020]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# Bootstrap Config

This document describes how the project is run locally, how processes are started, and where environment / configuration is sourced from. For multi-binary systems this is where per-binary bootstrap sequencing is described.

## Applicability

`unified-archive` is a **single Rust library crate**. There are no long-running processes, daemons, or binaries shipped by this crate. The "local run target" is the test suite, which exercises the full library surface against fixture archives.

For this reason, this document is short and largely informational. The `source-of-truth-table.md` artifact from the design-first-architecture skill is not produced (it is marked "multi-binary only" in the artifact lifecycle).

## Local run target

The canonical local run command is:

```sh
cargo test
```

This runs:
- All unit tests embedded in `src/`
- All integration tests under `tests/`
- All doctests in public API items

## Prerequisites

The library depends on native FFI. Local execution requires:

- **Rust toolchain:** 1.85+ (stable, 2024 edition).
- **UnRAR SDK:** linked via `build.rs`. RAR/RAR5 support is feature-gated behind `rar-support`, which is enabled by default. Build without it by turning defaults off and naming an explicit feature set that omits `rar-support` — e.g. `cargo build --no-default-features --features read,zip-read` (the `read-minimal` floor). A bare `--no-default-features` build is rejected by a `compile_error!` in `src/lib.rs`, because at least one backend feature must be selected.
- **libarchive:** required only when the `libarchive` feature is selected (it is in `default` and `full`, and `modify` implies it per AD-0071). It backs TAR and ISO read, TAR/7z creation, and *all* modification. Expected to be discoverable by `pkg-config` (Homebrew keg prefix probed first on macOS). The `read,zip-read` and `read,zip-read,zip-crypto,sfx` profiles need no C library at all.
- **Optional CLI tooling for fixtures / examples:**
  - `zip` and `7z` command-line tools may be used by some test fixtures and examples.
  - `rar` CLI is not used by the in-process backends; external WinRAR integration is feature-gated behind `external-rar-create` and is implemented in `src/external/rar.rs` (`RarCreator`). It is Windows-only and needs a licensed `rar.exe`.

See `Cargo.toml` for the authoritative feature and dependency list and `build.rs` for native linking logic.

## Configuration surface

This crate has no runtime configuration files, environment variables, or service endpoints. All configuration is delivered as Rust types passed at the call site:

- `ExtractionOptions`, `ExtractionLimits` — extraction-time tunables
- `CompressionOptions` — creation-time tunables (compression level, password, etc.)
- `ModificationOptions` — modification-time tunables (`create_backup`/`backup_suffix` honored via `modify_with_options()`, AD 0020; `preserve_metadata` preserves timestamps and Unix permissions via metadata-aware add helpers, OI-025-002 resolved 2026-04-14)

See `docs/architecture/config-surface.md` for the complete configuration catalog.

## Build-time configuration

`build.rs` controls native linking for UnRAR and any platform-specific build steps. Cargo features gate optional backends:

| Feature | Default | Effect |
|---|---|---|
| `rar-support` | enabled | Links the UnRAR C/C++ SDK and compiles the UnRAR backend. |
| `external-rar-create` | disabled | Enables the out-of-process WinRAR CLI bridge for RAR creation (`src/external/rar.rs`, `RarCreator`). Windows-only; requires a licensed `rar.exe`. |
| `v2-api` | enabled (default since 2026-09-03) | Exposes the D2 typed-handle split as `unified_archive::v2::{ReadArchive, WriteArchive, ModifyArchive}`. Additive in v0.3; enabled by default in v0.4. |
| `read` | enabled | Listing, entry lookup, extraction and streaming. |
| `integrity` | enabled | Implies `read`. Gates `validate_integrity`, `calculate_archive_crc`, the manifest and content-multiset digests, and the recovery-record accessors. |
| `create` | enabled | `Archive::create` and the add/finish surface. |
| `modify` | enabled | Implies `read` + `create` + `libarchive`. AD-0071 puts modification on libarchive for **every** format, ZIP included, so there is no libarchive-free `modify` build. |
| `full` | **not** in `default` | Aggregate: everything the crate can do from its own code. It expands to the same twelve features `default` does — `external-rar-create` is in neither — but Cargo does not list `full` inside `default`, so `cfg(feature = "full")` is false in a default build. |
| `zip-read` | enabled | The pure-Rust ZIP reader. The one format with no C dependency, which is what makes `read,zip-read` a no-toolchain floor. |
| `zip-write` | enabled | The pure-Rust ZIP writer. |
| `zip-crypto` | enabled | Implies `zip-read`; turns on the `zip` crate's `aes-crypto`. WinZip-AES **reading** only. |
| `sevenzip` | enabled | Adds the optional `sevenz-rust2` dependency. 7z **read** only — 7z creation and modification route through libarchive. |
| `libarchive` | enabled | TAR family, ISO, the standalone compressed streams, all non-ZIP creation, and all modification. Gates the `build.rs` pkg-config probe and the `archive` link. |
| `sfx` | enabled | Implies `read`. Self-extracting-archive detection and offset opening. |

At least one backend feature (`zip-read`, `zip-write`, `sevenzip`, `rar-support`, `libarchive`) must be selected: `src/lib.rs` raises a `compile_error!` otherwise, so a bare `--no-default-features` build is refused. The minimum useful configuration is `--features read,zip-read`.

## Temp/scratch storage

When running locally, the crate uses `/Volumes/Temp/claude/7zip/` (per the project CLAUDE.md) for temporary fixture storage during development. Inside library code the crate stages temp artifacts via `tempfile` in two shapes: `tempfile::TempDir` under `std::env::temp_dir()` for batch scratch (the whole tree is removed by `TempDir`'s own `Drop`) and `tempfile::Builder::new().tempfile()` for single-file staging in `Archive::open_at_offset` (released when the returned handle drops). Cleanup is owned by those concrete `tempfile` types — the in-crate `TempDirGuard` named here previously was removed (`docs/records/AD-0059-r0069-wide-modular-design-closure.md`, R0001-0094). See `docs/architecture/persistence-and-files.md` for file lifecycle detail.

## Smoke path

`cargo test` is the smoke path. A small integration run is additionally available via the examples:

```sh
cargo run --example inspect_archive -- path/to/archive.zip
cargo run --example extract_archive -- path/to/archive.zip /tmp/out
```

See `examples/` for the full list (8 examples as of 2026-04).

## Notes for implementers

- Because this is a library crate, there is no "service start order" or "daemon bootstrap". Callers embed the crate and drive `Archive::open(...)` / `Archive::create(...)` directly.
- If a future change introduces a binary wrapper (CLI or daemon), this document must be expanded and a `source-of-truth-table.md` must be added per the artifact lifecycle rules.
