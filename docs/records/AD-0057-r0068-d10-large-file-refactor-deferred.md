---
type: ADR
title: "AD 0057: Defer D10 large-file refactors and `#[cfg(test)]` move-out"
description: "Deferred."
tags: [decision, ADR-0057, ADR-0053, ADR-0019, R0068-0071]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0057: Defer D10 large-file refactors and `#[cfg(test)]` move-out

## Context and Problem Statement

Found in Review 0068 (R0068-0071..0077). AD 0053 baselined the design
for splitting the six largest crate files into concern-focused
submodules and moving production-side `#[cfg(test)]` blocks into
`tests/unit/<module>.rs` where the tests use only public API.

Files in scope (current line counts):

| File | LOC |
|---|---|
| `src/archive.rs` | ~750 |
| `src/extraction.rs` | ~800 |
| `src/inspection.rs` | ~950 |
| `src/modification.rs` | ~750 |
| `src/security.rs` | 701 |
| `src/options.rs` | 449 |
| `src/format.rs` | 788 |
| `src/ffi/libarchive_wrapper.rs` | ~1600 |
| `src/ffi/wrapper.rs` (UnRAR) | ~1100 |

## Decision Drivers

* **D10 lands LAST** per AD 0053's sequencing. Doing it before
  D2/D3/D4/D9 means the splits reflect today's pre-refactor layout,
  not the post-refactor one. Code that moves into a new file in this
  pass would need to move again when D2 reshapes the backend enums
  and D9 splits the libarchive wrapper.
* **Risk vs reward.** The split is pure rearrangement (no behavior
  change), but the rearrangement touches every test file that imports
  `crate::security::*`, `crate::options::*`, etc. — `pub use`
  re-exports cover the public API but `pub(crate)` items used by
  tests need careful re-pathing. A careless split breaks the test
  matrix in non-obvious ways.
* **Test-block move-out has a hidden constraint.** Tests inside
  `#[cfg(test)] mod tests` in src/ access `pub(crate)` items
  freely. Moving those tests to `tests/unit/<module>.rs` only works
  for tests that use the public API — the rest must stay in src/. A
  blanket move would reduce coverage or expose internals.

## Considered Options

1. **Land all D10 splits now.** Six-file mechanical refactor that
   doesn't reflect post-D2/D9 reality. Likely re-touched twice.
2. **Land only the files that don't depend on D2/D9
   (`security.rs`, `options.rs`, `format.rs`).** Three files, no
   backend-enum coupling. Low risk if the existing `pub use`
   re-exports cover every test import.
3. **Defer all of D10 until after D2/D3/D4/D9 land.** Splits
   reflect the post-refactor layout, no double-touching.

## Decision Outcome

ACCEPT: option 3.

The per-file split blueprint already lives in AD 0053. This ADR
records the sequencing rationale and the test-block move-out
constraint that governs the eventual landing.

Status: Deferred. Lands after D2/D3/D4/D9 per AD 0053's sequencing.

### Implementation contract for the future D10 landing

For each file in scope, follow this pattern:

1. **Create a directory module** (`src/<file>/mod.rs` containing the
   public facade and `pub use` re-exports). Move the existing file
   contents into the new module hierarchy.
2. **Split per concern** as listed in AD 0053. Internal helpers
   stay `pub(crate)` and live in the submodule that owns their
   primary concern.
3. **Move `#[cfg(test)] mod tests` blocks** to
   `tests/unit/<module>.rs` only when every test in the block uses
   the public API. Tests that use `pub(crate)` items stay in their
   source module — exposing them publicly to satisfy the move-out
   would be a worse outcome than carrying the test inline.
4. **Run the full test matrix** (`cargo test --all-features`,
   `cargo test --no-default-features`) after each file's split.
   `pub use` re-exports must keep every external import working.

### Specific notes per file

- **`security.rs`**: split into `security/{path,limits,crc}.rs`.
  CRC helpers (`verify_crc32`, `verify_crc32_value`) are entirely
  self-contained and could land first as a smallest-viable-split.
- **`options.rs`**: keep public DTOs (`ExtractionOptions`,
  `CompressionOptions`, `CompressionLevel`, `ProgressCallback`) in
  `options.rs`; move `password_as_str`, `RateLimiter`, the
  `ProgressCallback` `FnMut` blanket impl, and `EntryFilter` into
  `options/runtime.rs`.
- **`format.rs`**: keep detection in `format.rs`; move the
  capabilities matrix (`FormatCapabilities`, `Support`, the
  per-format `capabilities()` impl) into `format/capabilities.rs`.
- **`archive.rs`**: split after D2. The post-D2 layout is
  `archive/mod.rs` (facade) + `archive/{open,read,write,modify,
  lifecycle}.rs`.
- **`extraction.rs`**: split after D3 so `ValidatedSource`'s
  natural home is `extraction/validated.rs`. Other splits:
  `extraction/{preflight,dispatch}.rs`.
- **`inspection.rs`**: split into `{multipart,integrity,
  manifest_digest,encrypted}.rs` after D4 (so per-backend caching
  lands in the originating backends, not the inspection layer).
- **`modification.rs`**: `tracker`, `commit`, `zip_extras`. After
  D2 so `ModifyArchive`'s methods land on the right type.
- **`ffi/libarchive_wrapper.rs`**: split as
  `ffi/libarchive/{mod,reader,writer,common,entry}.rs` *after* D9.
- **`ffi/wrapper.rs`** (UnRAR): split as
  `ffi/unrar/{mod,parse,extract,recovery}.rs`. Per AD 0019 the
  process-wide `UNRAR_LOCK` lives in `mod.rs` so it stays the
  single source of truth.

## Consequences

* Good, because the splits land once instead of twice.
* Good, because the test-move discipline (only-public-API tests
  graduate to `tests/unit/`) is captured here for future
  implementers.
* Good, because the per-file blueprint (AD 0053 + this ADR's
  notes) gives the next session a turn-key plan.
* Bad, because the largest crate files
  (`libarchive_wrapper.rs` at ~1600 LOC) keep their review-cost
  burden until D9+D10 land in sequence. Acceptable cost vs the
  double-touch alternative.
