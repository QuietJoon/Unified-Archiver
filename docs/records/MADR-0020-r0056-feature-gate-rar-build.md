---
type: ADR
title: "AD: Feature-gate UnRAR build behind `rar-support` Cargo feature"
description: "Implemented"
tags: [decision, ADR-0020, R0056-0002]
timestamp: 2026-04-16T00:00:00Z
status: active
---

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

## Amendment (2026-07-22, R0081 I4)

**The Windows `build.rs` `panic!` under `rar-support` is removed; `rar-support` now compiles on Windows, macOS, and Linux from one code path.**

After this record landed, `build.rs` grew a hard `panic!` on Windows targets under `rar-support`
(commit *"build(unrar): refuse to build vendored UnRAR on Windows"*), because the vendored UnRAR
build shelled out to `make lib`, which stock Windows lacks. Combined with `rar-support` being in the
default feature set, the **default `cargo build` failed on Windows — a first-class support
platform** (flagged in the 2026-07-19 architecture decision review as a correctness bug in the
decision, not just the record).

Innovation I4 replaced the `make`-based builder with a `cc::Build`-driven compile of the vendored
sources (see AD 0039 amendment). `cc` selects the MSVC toolchain on Windows and picks the correct
source list / preprocessor defines from `CARGO_CFG_TARGET_OS` / `CARGO_CFG_TARGET_ENV`, so the
vendored UnRAR C++ now compiles on Windows, macOS, and Linux from the same path. Consequently:

* The Windows `panic!` and its "rebuild with `--no-default-features`" warning are **removed**.
  `build_unrar()` is gated on the `rar-support` feature only, not on the build host — which is what
  this record's original Decision Outcome intended ("Gate `build.rs` ... behind
  `#[cfg(feature = "rar-support")]`"). The later host-OS gate that produced the panic was drift; it
  is now retired.
* The default build no longer fails on Windows. The Windows source set mirrors `UnRARDll.vcxproj`
  (RARDLL;UNRAR;SILENT); the Unix set mirrors the makefile `lib` target (RARDLL + the shared
  DEFINES).
* The feature-gate decision itself is unchanged: consumers who want to avoid the UnRAR license
  surface still opt out with `default-features = false`, and a RAR open then returns
  `ArchiveError::UnsupportedOperation`, exactly as the Decision Outcome states.
* Degradation policy: no first-class target falls back to a `compile_error!` — Win/macOS/Linux all
  build. If some future exotic target genuinely cannot compile UnRAR, the intended shape is a
  `cfg`-gated build-time `compile_error!`, never a runtime `panic!`.

Verification: `cargo build --all-features` compiles and links the vendored UnRAR via `cc` on this
macOS host. The Windows/MSVC and cross-compile paths cannot be run on the macOS host and are
unverified at authoring time; `cc` selecting MSVC is the standard cross-platform mechanism.

Status: still active — the `rar-support` feature gate stands; the Windows-panic drift is retired.

## Amendment (2026-09-03, a variant name in the 2026-07-22 amendment is stale)

The feature-gate ruling is unchanged. The 2026-07-22 amendment describes a RAR open without
`rar-support` as returning `ArchiveError::UnsupportedOperation`; check the current variant list in
`src/error.rs` before quoting that name, as the error enum has been reshaped since. The behaviour —
a RAR open fails cleanly rather than linking UnRAR — is what the record decided and still holds.
