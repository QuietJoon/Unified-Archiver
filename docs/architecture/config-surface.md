# Config Surface

Configuration categories, ownership, and usage.

## Cargo Features

| Feature | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `rar-support` (default) | `Cargo.toml` | `build.rs`, `src/ffi/wrapper.rs`, `src/ffi/unrar.rs` | Enable RAR/RAR5 format support via UnRAR SDK | Requires UnRAR license compliance (free for non-commercial) |
| `external-rar-create` | `Cargo.toml` | `src/external/rar.rs` | Enable RAR archive creation via external WinRAR CLI | Windows-only; requires licensed WinRAR installation with `rar.exe` in PATH |

## Environment Variables

| Variable | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` | Runtime config | `src/ffi/piz_wrapper.rs` | Bound memory-mapped file size for Piz ZIP backend | Prevents excessive memory usage on very large ZIP files |
| `PKG_CONFIG_PATH` | Build environment | `build.rs` | Locate libarchive installation via pkg-config | Required on macOS/Linux for libarchive linking |

## Runtime Config Objects

| Config Object | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `ExtractionOptions` | `options.rs` | `extraction.rs`, backends | Configure extraction behavior (destination, password, overwrite, progress, CRC, limits) | Policy object pattern — keeps method signatures stable |
| `CompressionOptions` | `options.rs` | `creation.rs`, backends | Configure creation behavior (format, level, password, progress) | `split_size` field exists but not honored; creation encryption is ZIP-only; progress callback invoked per-entry with `total=None` (OI-025-003 resolved, AD 0021) |
| `ExtractionLimits` | `security.rs` | `extraction.rs` | Zip bomb protection (max file size, max total size, max entry count) | Checked during extraction preflight |
| `ModificationOptions` | `modification.rs` | `modification.rs` | Configure modification behavior (backup creation, metadata preservation) | Partially applied — some options not yet honored |
| `RateLimiter` | `options.rs` | `extraction.rs` | Throttle progress callbacks to ~60 Hz | Prevents UI flooding; time-based gate |

## Build-Time Config

| Config | Owned By | Used By | Why Needed | Notes |
|---|---|---|---|---|
| `build.rs` UnRAR compilation | `build.rs` | `src/ffi/unrar.rs` | Compile and statically link UnRAR SDK from bundled C++ source | Requires C++ compiler and `make` |
| `build.rs` libarchive linking | `build.rs` | `src/ffi/libarchive.rs` | Dynamically link system libarchive | Requires `libarchive-dev` (Linux), `brew install libarchive` (macOS) |
| Rust edition | `Cargo.toml` | All source | Rust 2024 edition with minimum version 1.85 | All `extern` blocks must be `unsafe`; explicit `unsafe` blocks inside `unsafe fn` |
| `CARGO_TARGET_DIR` | Environment | `cargo` | Redirect build artifacts to an external directory | Currently set to `/Volumes/Scratch/cargo_target` in the development environment; not required for consumers |
