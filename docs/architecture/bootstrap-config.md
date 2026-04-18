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
- **UnRAR SDK:** linked via `build.rs`. RAR/RAR5 support is feature-gated behind `rar-support`, which is enabled by default. Disable with `--no-default-features` if UnRAR is unavailable.
- **libarchive:** required for TAR and ISO read, and TAR/7z creation. Expected to be discoverable by `pkg-config`.
- **Optional CLI tooling for fixtures / examples:**
  - `zip` and `7z` command-line tools may be used by some test fixtures and examples.
  - `rar` CLI is not used; external WinRAR integration is feature-gated behind `external-rar-create` and is a deferred item.

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
| `external-rar-create` | disabled | Enables an out-of-process WinRAR CLI bridge for RAR creation. Deferred; Windows-only. |

## Temp/scratch storage

When running locally, the crate uses `/Volumes/Temp/claude/7zip/` (per the project CLAUDE.md) for temporary fixture storage during development. Inside library code, temp files are created via `tempfile::TempDir` under `std::env::temp_dir()` and cleaned up through RAII guards (`TempDirGuard`). See `docs/architecture/persistence-and-files.md` for file lifecycle detail.

## Smoke path

`cargo test` is the smoke path. A small integration run is additionally available via the examples:

```sh
cargo run --example inspect_archive -- path/to/archive.zip
cargo run --example extract_all -- path/to/archive.zip /tmp/out
```

See `examples/` for the full list (9 examples as of BL-001-retroactive).

## Notes for implementers

- Because this is a library crate, there is no "service start order" or "daemon bootstrap". Callers embed the crate and drive `Archive::open(...)` / `create_archive(...)` directly.
- If a future change introduces a binary wrapper (CLI or daemon), this document must be expanded and a `source-of-truth-table.md` must be added per the artifact lifecycle rules.
