---
type: Config Surface
title: "Config Surface"
description: "Configuration categories, ownership, and usage."
tags: [architecture, config, ADR-0027, ADR-0021, OI-0065-002]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# Config Surface

Configuration categories, ownership, and usage.

## Cargo Features

| Feature | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `rar-support` (default) | `Cargo.toml` | `build.rs`, `src/ffi/wrapper.rs`, `src/ffi/unrar.rs` | Enable RAR/RAR5 format support via UnRAR SDK | Requires UnRAR license compliance: free of charge for handling RAR archives, but the sources may not be used to build a RAR-compatible archiver or re-create the RAR compression algorithm, and the governing paragraph must be reproduced (see `LICENSE`) |
| `external-rar-create` | `Cargo.toml` | `src/external/rar.rs` | Enable RAR archive creation via external WinRAR CLI | Windows-only; requires licensed WinRAR installation with `rar.exe` in PATH |
| `v2-api` | `Cargo.toml` | `src/archive/mode_split.rs` (`crate::v2`) | Expose the D2 typed-handle split (`ReadArchive` / `WriteArchive` / `ModifyArchive`) | Additive in v0.3; scheduled to become enabled-by-default in v0.4 |

## Environment Variables

| Variable | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `PKG_CONFIG_PATH` | Build environment | `build.rs` | Locate libarchive installation via pkg-config | Required on macOS/Linux for libarchive linking |

## Runtime Config Objects

| Config Object | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `ExtractionOptions` | `options.rs` | `extraction.rs`, backends | Configure extraction behavior (destination, password, overwrite, progress, CRC, limits) | Policy object pattern — keeps method signatures stable |
| `CompressionOptions` | `options.rs` | `creation.rs`, backends | Configure creation behavior (format, level, password, progress) | `password` field is `Option<Password>` (the `Password` newtype zeroizes on drop and redacts in `Debug`/`Display`); setting it to `Some(...)` for any format causes `Archive::create` to return `OperationBlocked` per MADR-0027. `split_size` exists but not honored (DEF-002). Progress callback invoked per-entry with `total=None` (OI-025-003 resolved, AD 0021). |
| `ExtractionLimits` | `security.rs` | `extraction.rs` | Zip bomb protection (max file size, max total size, max entry count) | Checked during extraction preflight |
| `ModificationOptions` | `modification.rs` | `modification.rs` | Configure modification behavior (backup creation, metadata preservation) | `compression`, `preserve_metadata`, `create_backup`, `backup_suffix` are all honored by `modify_with_options()`. `preserve_metadata` covers modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries (where the backend supports the timestamp) per OI-0065-002; directory metadata still uses backend defaults. Comment/xattr propagation on non-ZIP backends is the remaining gap. |
| `RateLimiter` | `options.rs` | `src/ffi/libarchive_wrapper.rs`, `src/ffi/wrapper.rs` (UnRAR) | Throttle progress callbacks to ~60 Hz | Prevents UI flooding; time-based gate. ZIP/SevenZ call the progress callback per entry or chunk without rate-limiting |

## Build-Time Config

| Config | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `build.rs` UnRAR compilation | `build.rs` | `src/ffi/unrar.rs` | Compile and statically link UnRAR SDK from bundled C++ source | Requires a C++ compiler; the sources are compiled through the `cc` crate, so the vendored makefile is not a build input |
| `build.rs` libarchive linking | `build.rs` | `src/ffi/libarchive.rs` | Dynamically link system libarchive | Requires `libarchive-dev` (Linux), `brew install libarchive` (macOS) |
| Rust edition | `Cargo.toml` | All source | Rust 2024 edition with minimum version 1.85 | All `extern` blocks must be `unsafe`; explicit `unsafe` blocks inside `unsafe fn` |
| `CARGO_TARGET_DIR` | Environment | `cargo` | Redirect build artifacts to an external directory | Currently set to `/Volumes/Scratch/cargo_target` in the development environment; not required for consumers |
