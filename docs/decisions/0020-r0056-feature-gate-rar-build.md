# AD: Feature-gate UnRAR build behind `rar-support` Cargo feature

## Context and Problem Statement
Found in Review 0056 (Issue R0056-0002, Severity: Critical).
Location: `Cargo.toml:35`, `build.rs:57`

The `rar-support` feature existed in `Cargo.toml` but `build.rs` unconditionally
called `build_unrar()`, meaning `--no-default-features` did not actually disable
the UnRAR native build, toolchain burden, or license surface.

## Decision Drivers
* Consumers must be able to opt out of the UnRAR license (non-free for commercial use)
* `--no-default-features` is the standard Cargo mechanism for disabling optional native deps
* The feature flag was documented but non-functional — a correctness bug

## Considered Options
1. Gate build.rs + all Rust source paths behind `#[cfg(feature = "rar-support")]`
2. Remove the feature flag and always require UnRAR
3. Gate only build.rs and let the linker fail (incomplete)

## Decision Outcome
ACCEPT (Option 1): Gate the entire RAR code path — `build.rs`, `ffi/mod.rs`, the
`ArchiveBackend::Unrar` enum variant, and all match arms across `archive.rs`,
`extraction.rs`, `inspection.rs`, and `creation.rs`. When `rar-support` is
disabled, `Archive::open()` on a RAR file returns
`ArchiveError::UnsupportedOperation` with a clear message.

Status: Implemented

### Implementation
- `build.rs`: `build_unrar()` gated with `#[cfg(feature = "rar-support")]`
- `src/ffi/mod.rs`: `mod unrar` and `mod wrapper` gated
- `src/archive.rs`: `Unrar` variant, import, open paths, and metadata match arms gated
- `src/extraction.rs`: 5 match arms gated
- `src/inspection.rs`: 3 match arms gated
- `src/creation.rs`: 4 match arms gated
- `README.md`: Updated RAR disable instruction to `default-features = false`

## Consequences
* Good, because consumers can now genuinely opt out of RAR support and its license
* Good, because the feature flag in Cargo.toml now matches real behavior
* Bad, because adding `#[cfg]` to every match arm adds maintenance overhead
