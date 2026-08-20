---
type: ADR
title: "AD: Clippy lint baseline restoration and dead code strategy"
description: "Implemented"
tags: [decision, ADR-0018, R0003-0001, R0003-0002, R0003-0011, R0003-0017, R0003-0018]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Clippy lint baseline restoration and dead code strategy

## Context and Problem Statement
Found in Review 0003 (Issues R0003-0001, R0003-0002, R0003-0011, R0003-0017, R0003-0018, Severity: High/Medium).
Location: multiple `src/ffi/*.rs` files, `src/archive.rs`

The crate failed `cargo clippy -- -D warnings` with 45 errors: unused FFI
bindings, unused wrapper methods, large enum variant, and style lints. The
task tracker (T124) still claimed the lint gate was clean. Three categories
needed a strategy decision rather than mechanical deletion.

## Decision Drivers
* FFI binding surfaces (`libarchive.rs`, `unrar.rs`) are intentionally
  complete — removing "unused" declarations would force re-binding on every
  feature addition
* Backend wrapper methods (`path()`, `extract_all()`, `extract_file()`) form
  a uniform internal API across all backends, even though the façade does not
  call every method on every backend
* `ArchiveBackend::ZipWriter` stored a ~664-byte `ZipWriter` inline, inflating
  all other (read-side) variant sizes

## Considered Options
1. Prune all dead code (removes FFI completeness and backend uniformity)
2. `#[allow(dead_code)]` with justification comments (retains completeness)
3. Box the large variant to solve size disparity (standard Rust pattern)

## Decision Outcome
ACCEPT (Options 2 + 3):
- FFI modules: `#![allow(dead_code)]` at module level with "intentionally
  complete binding surface" documentation
- Wrapper impl blocks: `#[allow(dead_code)]` with "uniform backend API
  surface" documentation
- `ArchiveBackend::ZipWriter`: `Box<ZipWriter>` (reduces variant from ~664
  bytes to 8 bytes)
- Style lints (hex grouping, saturating_sub, useless conversion, map_or):
  fixed directly

Status: Implemented — amended 2026-07-22 (R0081 I7): the blanket `#![allow(dead_code)]` / broad `#[allow(dead_code)]` strategy is superseded by per-item `#[expect(dead_code)]`. See the Amendment section below.

### Implementation
- `src/ffi/libarchive.rs`: `#![allow(dead_code)]` module-level
- `src/ffi/unrar.rs`: `#![allow(dead_code)]` module-level
- `src/ffi/{libarchive_wrapper,piz_wrapper,sevenz_wrapper,zip_wrapper,wrapper,zip_writer}.rs`:
  `#[allow(dead_code)]` on impl blocks
- `src/archive.rs`: `ZipWriter(Box<ZipWriter>)`
- `src/creation.rs`: `Box::new(writer)` at construction site
- `src/stream_crc.rs`: hex literal grouping, saturating_sub
- `src/ffi/piz_wrapper.rs`: removed useless `usize::try_from`
- `src/ffi/sevenz_wrapper.rs`: `map_or` → `is_some_and`
- `specs/001-unified-archive/tasks.md`: T124 status updated

## Consequences
* Good, because `cargo clippy -- -D warnings` now passes cleanly (0 errors)
* Good, because FFI binding completeness is preserved for future features
* Good, because backend API uniformity is maintained
* Good, because `ArchiveBackend` enum size is now dominated by read-side
  variants (~200–300 bytes) rather than the writer (~664 bytes)
* Bad, because the `#[allow]` annotations may mask genuinely dead code in the
  future — periodic audit recommended

## Amendment (2026-07-22, R0081 I7)

The final "Bad" consequence above — that `#[allow]` annotations *may mask
genuinely dead code in the future* and so a *periodic audit* was only
*recommended* — is now **enforced by the compiler** rather than left to
discipline.

**What changed.** The blanket module-level `#![allow(dead_code)]` on the FFI
binding modules and the broad impl-block `#[allow(dead_code)]` described in the
original Implementation are gone. The R0081 API-reshaping chain (I1–I6) either
wired up or deleted the previously-dead surface, so those blanket allows no
longer had anything to cover; this step (I7) then swept the settled result and
converted every remaining `#[allow(dead_code)]` to a per-item
`#[expect(dead_code)]` (Rust 1.85), each carrying a `reason = "…"`.

**Why `#[expect` instead of `#[allow`.** `#[expect(dead_code)]` *fails* the
build (`unfulfilled_lint_expectations`) the moment the annotated item becomes
used. That is the self-healing audit the original record could only recommend:
when a future caller finally wires up one of these items, the compiler forces
removal of the now-stale annotation instead of silently leaving a misleading
`#[allow]` in place. Dead code that is genuinely orphaned (no documented reason
to exist) is deleted outright rather than annotated.

**Outcome of this pass.**
- `#[allow(dead_code)]` removed: **5** (0 module-level — prior chain steps had
  already cleared those; 5 item-level).
- Items deleted: **0** — every residual dead item had a documented reason to
  exist, so none qualified as truly-orphaned.
- Items annotated with `#[expect(dead_code)]`: **6** (the one enum-level allow
  expanded to two per-variant expects; the other four are 1:1):
  - `ReadBackend::extract_file` (`src/backend.rs`) — canonical disk-write entry
    for the planned D2 refactor; implemented by every backend, no trait-level
    caller yet.
  - `open_file_no_follow_symlinks` (`src/ffi/common.rs`) — TOCTOU-hardening
    helper awaiting its caller under OI-0070-002 / R0070-0021.
  - `CopyByteBudget::Unbounded` and `CopyByteBudget::Cap` (`src/ffi/common.rs`)
    — retained for type-level contract completeness; only `Exact` is currently
    constructed.
  - `LockedFileIdentity` (`src/modification.rs`) and `UnrarFileIdentity`
    (`src/ffi/wrapper.rs`) — non-Unix zero-sized identity markers; drift
    revalidation is Unix-only, so these `#[cfg(not(unix))]` variants are never
    constructed off-Unix (their expects are validated only on non-Unix builds).

The FFI-completeness and backend-uniformity rationale from the original
Decision Outcome still holds; only the *mechanism* for tolerating the resulting
dead code changed (blanket `allow` → enforced per-item `expect`).
