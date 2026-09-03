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

## Amendment (2026-08-21, ticket 095eeea5 — the `#[cfg(test)]` move-out lands for the three FFI backends)

Half of what this ADR deferred is now retired. The inline `#[cfg(test)]` modules of
`src/ffi/wrapper.rs`, `src/ffi/zip_wrapper.rs` and `src/ffi/sevenz_wrapper.rs` have been moved
out into sibling children. The D10 production-code *splits* stay deferred, with a concrete trigger
restated at the end. The Decision Outcome above is not superseded: option 3's sequencing argument
is about the production splits, and none of those moved.

### The destination changed, and that is what made the move safe

The "Implementation contract" above routes moved test blocks to `tests/unit/<module>.rs`, and then
guards that route with the hidden constraint recorded in the Decision Drivers — only tests that use
the public API may graduate, because an integration test cannot reach `pub(crate)`. For these three
backends that constraint is fatal: their tests exist precisely to pin private parsing, staging and
classification helpers, so almost none of them would have qualified, and the ones that did would
have been separated from their neighbours for no reason.

The owner-ruled destination is different, and the difference is the whole point: a **sibling child
module inside the production module**, following the `src/inspection.rs` + `src/inspection/tests.rs`
precedent already in the tree. A child module's path is unchanged by the move — `ffi::wrapper::tests`
is `ffi::wrapper::tests` whether it is written inline or in `src/ffi/wrapper/tests.rs` — and Rust's
privacy is path-based, so a child still reaches every private item of its ancestors. Consequences
worth recording:

* The public and `pub(crate)` surface is **unchanged**. Nothing was widened to make the move
  compile, which was the failure mode the Decision Drivers warned about.
* **No test stayed behind and none was rewritten.** The blanket-move coverage risk the drivers
  describe belongs to the `tests/unit/` destination only; it does not apply to this one.
* The move is available to any file in the crate, not only to files whose tests are public-API-only.
  A future D10 pass should prefer it and treat `tests/unit/<module>.rs` as reserved for tests that
  are genuinely integration-level.

One stale detail in the contract above, corrected here rather than rewritten: step 1 says to create
`src/<file>/mod.rs`. The project's layout rule is file-as-module, so the D10 landing must use
`src/<name>.rs` plus `src/<name>/<child>.rs` and must not create a `mod.rs` under `src/`. This
move-out used the file-as-module form.

### What moved, and the proof that nothing was lost

| module (path unchanged) | new file | tests before | tests after |
|---|---|---|---|
| `ffi::wrapper::tests` | `src/ffi/wrapper/tests.rs` | 33 | 33 |
| `ffi::wrapper::staged_length_tests` | `src/ffi/wrapper/staged_length_tests.rs` | 4 | 4 |
| `ffi::zip_wrapper::tests` | `src/ffi/zip_wrapper/tests.rs` | 34 | 34 |
| `ffi::sevenz_wrapper::tests` | `src/ffi/sevenz_wrapper/tests.rs` | 23 | 23 |
| **total** | | **94** | **94** |

Counts alone are weak evidence for this operation — a lost test and a newly-collected one cancel out
— so the check was a **sorted name-set diff**: `cargo test --lib --all-features -- --list` filtered
to the four module prefixes, before and after, compared line by line. The two sets are identical, so
nothing was dropped, renamed, or silently `cfg`-ed away.

Two deliberate choices support that:

* **Module names were preserved.** `staged_length_tests` was *not* folded into `tests`; it became its
  own child. Folding it would have re-pathed four tests and invalidated any existing `cargo test`
  filter for them, which is a visible behaviour change dressed up as tidying.
* **Bodies moved verbatim.** Each block was dedented by exactly four spaces and otherwise untouched;
  the result was byte-compared against the pre-move content to confirm it. The three multi-line
  string literals involved use backslash line-continuation, which strips the newline *and* the
  following line's leading whitespace, so dedenting cannot alter their values. The only textual
  difference beyond the dedent is a `rustfmt` pass that re-joined statements now fitting on one line.

Production halves shrank accordingly:

| file | LOC before | LOC after |
|---|---|---|
| `src/ffi/wrapper.rs` | 3141 | 2496 |
| `src/ffi/zip_wrapper.rs` | 3008 | 1713 |
| `src/ffi/sevenz_wrapper.rs` | 2341 | 1589 |

Note that the LOC figures in the original table above were already badly stale when this amendment
was written (`wrapper.rs` was listed at ~1100 against an actual 3141). Line counts in this record
should be read as of their dated section, not as current facts.

### Still carrying inline tests

`src/ffi/libarchive_wrapper.rs` keeps its inline `#[cfg(test)] mod tests` — it was outside this
ticket's file ownership and was not touched. It is the obvious next application of the same move,
and it is independent of D9: moving its tests into `src/ffi/libarchive_wrapper/tests.rs` sits
alongside the existing `reader.rs`/`writer.rs` children and does not prejudge how D9 splits the
production code.

### What stays deferred, and the trigger

The D10 production splits for these three files are still deferred, for the reason option 3 gives.
The move-out did not make them easier or harder; it only made the production half legible enough to
name the seams. Recommended seams, recorded so the eventual pass does not have to re-derive them:

* **`src/ffi/wrapper.rs` (UnRAR)** — the child directory now exists, so no rename to `ffi/unrar/` is
  needed and AD 0019's requirement is met by leaving `UNRAR_LOCK`, its guard and `UnrarArchive` in
  the parent file, which stays the single source of truth. Around that: `wrapper/parse.rs` (header
  parsing, the FILETIME and DOS-time conversions, `unix_mode_from_file_attr`, `read_rar5_vint`),
  `wrapper/extract.rs` (the extract context and abort enum, staged-length and staged-file
  finalisation, the atomic extract, error mapping, the listing-drift constructors) and
  `wrapper/recovery.rs` (the RAR4 and RAR5 recovery-record walks and their read helper). Doing this
  makes `staged_length_tests` a natural inhabitant of `extract.rs`'s own test child, which re-paths
  those four tests — a visible decision to take then, not a side effect to absorb.
* **`src/ffi/zip_wrapper.rs`** — two seams are already independent of D2 and are the
  smallest-viable-splits: the raw central-directory value layer (`RawRecord`,
  `RawCentralDirectory`, the exact central-directory read) which does not touch the `ZipArchive`
  facade at all, and the AES/CRC-exemption cluster (the AES extra-field constants, vendor-version
  probe, CRC exemption predicate, counted CRC drain).
* **`src/ffi/sevenz_wrapper.rs`** — do **not** split mechanically. Nearly the whole production half
  is a single `impl SevenZArchive` block, so the concern boundaries have to be named inside that
  impl first, and that is D2/D4 work. Splitting on the handful of free functions at the end would
  produce a file that is smaller without being clearer.

Trigger, concretely: the two `zip_wrapper` seams above may land as soon as a reviewer has bandwidth,
since they depend on neither D2 nor D9. Everything else waits for D2 (backend-enum reshape) and D9
(libarchive wrapper split) to land, per option 3. `wrapper.rs` at 2496 production LOC is the largest
remaining offender and should be first in the queue once D2 clears.

### Gate evidence for this amendment

`cargo fmt --all -- --check` clean, `cargo check --all-features --all-targets` clean, and
`cargo clippy --all-features --all-targets` clean with zero Rust lint warnings — the only build
output is the pre-existing `-Wnontrivial-memcall` note from the vendored UnRAR C++ sources.

The full suite came out at **38 suites / 1865 passed / 0 failed / 13 ignored**, against a
pre-change baseline of 38 / 1833 / 0 / 13. The `passed` total rose because other work landed lib
tests concurrently (the lib target went from 847 to 869 collected); nothing was lost, and the
`ignored` count is unchanged. The four moved modules contributed exactly 33 + 4 + 34 + 23 passing
tests in that run, which is the third independent confirmation of the counts above — after the
name-set diff and a count of `#[test]` attributes in the new files.

Two failures observed during gating were traced to the environment, not to this change, and are
recorded here so a future reader does not attribute them to the move:

* `tests/performance_test.rs::perf_streaming_overhead` asserts a wall-clock ratio (streaming within
  3x of in-memory extraction) and measured 4.00x once while several other full-suite runs were
  saturating the machine. It passed on an isolated re-run. The assertion is load-sensitive by
  construction.
* `tests/multipart_continuity_test.rs` failed in a concurrent run with `ERAR_EOPEN` and
  `NotFound` on `/Volumes/Temp/claude/ua_multipart_continuity_hole/set.part1.rar`. The suite stages
  volumes into a **fixed, process-shared scratch directory**, so two simultaneous runs of the same
  test binary delete each other's fixtures. It passed cleanly in a run that did not overlap. The
  fix belongs in that test file (per-run unique scratch path) and is out of this amendment's scope.

## Amendment (2026-08-21, a fourth file moved in the same pass — `src/archive.rs`)

The amendment above is titled "for the three FFI backends" and a fourth file moved with them.
`src/archive.rs` is a named target in this record's own D10 table, so without this note a later D10
reader would expect to find its test block still inline and would not.

The same change set that emptied the three FFI files *added* a 126-line inline `#[cfg(test)] mod
in_place_payload_tests` to `src/archive.rs`, on exactly the principle the pass was retiring. It is now
`src/archive/in_place_payload_tests.rs`, declared after `mod tests;`, by the same sibling-child route
and for the same reason: a child reaches every private item of its ancestors, so nothing was widened.

Evidence, recorded because it is the acceptance criterion a later reader cannot reconstruct:
`archive::` reports 69 passed before and after, with the same 4 in-place tests among them, and the
excision was 3 lines added and 136 removed against a pre-move copy of the file — so no other change
travelled with it. The `pub(crate) enum PayloadSource` additions that appear near the moved block
pre-date the move; they belong to the in-place open work, not to this one.

One comment was carried across deliberately rather than dropped as move noise: the note that
`payload_size_for_ratio` is the *denominator* of the compression-ratio (zip-bomb) gate, so counting
stub bytes there would dilute it (R0069-0006). That is the reason `PayloadSource::InPlace` keeps its
`offset` at all, and it is the kind of fact a test file is the last place to lose.

**This does not move the D10 production splits.** `src/archive.rs` remains oversized — 2441 lines —
and remains on the deferred list with the trigger the previous amendment restates.

**And it does not move `src/archive.rs`'s other inline test modules, of which there are three.**
Stating them so a later pass does not have to rediscover them, and does not assume they were
overlooked:

| module | lines | tests | may it move? |
|---|---|---|---|
| `sfx_fallback_error_tests` | 56 | 2 | **No — deliberately inline.** Its own doc says "Inline module (not `archive/tests.rs`) so the fallback tests live next to the fallback sites they pin down." Moving it overrides a recorded decision; anyone who wants to must replace that reason, not delete it. |
| `read_identity_tests` | 136 | 4 | Open question. `#[cfg(all(test, unix))]`, which a child declaration carries fine. No stated reason to stay. |
| `sfx_payload_cap_tests` | 126 | 5 | Open question. No stated reason to stay. |

The block that *did* move was not chosen for being large. It was chosen because the same change
set that emptied 94 tests out of three FFI files had just added it — an inline block created while
the pass removing inline blocks was in flight. That asymmetry is the defect; a pre-existing,
focused test module with a doc comment explaining its scope is not the same thing, and moving the
remaining two is a judgement call about test locality rather than a correction. It is left to the
reviewer who takes the D10 pass on this file.

## Amendment (2026-08-21, the two `zip_wrapper` D10 seams land — the only D10 work this record calls unblocked)

The previous amendment named two seams in `src/ffi/zip_wrapper.rs` and gave them a trigger that was
not a dependency: they "may land as soon as a reviewer has bandwidth, since they depend on neither
D2 nor D9". They have landed. Everything else in D10 still waits on D2 and D9, and this amendment
does not move any of it.

| new child | lines | what moved |
|---|---|---|
| `src/ffi/zip_wrapper/raw_directory.rs` | 192 (166 moved) | `RawRecord`, `RawCentralDirectory` and its five methods, and `read_central_directory_exact` |
| `src/ffi/zip_wrapper/aes.rs` | 121 (108 moved) | the WinZip-AES header id and AE-2 vendor constants, `aes_vendor_version`, `crc32_check_exempt`, `drain_entry_crc32_counted` |

`src/ffi/zip_wrapper.rs` went from 1715 to 1445 lines. Its `tests` child is **byte-identical to the
previous commit** — `git diff` reports no change to it at all — so no test was added, removed,
renamed or re-pathed, and `ffi::zip_wrapper` reports the same 34 passed as before the split. Only
production code moved.

### The visibility question, answered exactly

A child sees every private item of its ancestors; a parent does not see the child's privates. So
items the parent still uses had to gain a visibility keyword. **Nothing became `pub` or
`pub(crate)`** — a grep for `^\s*pub (fn|struct|enum|const|mod)` across both children returns
nothing. Fourteen items are `pub(super)`: two in `aes.rs` (`crc32_check_exempt`,
`drain_entry_crc32_counted`) and twelve in `raw_directory.rs` (one function, two structs, six
fields, three methods).

`pub(super)` on a child is not a widening. It makes an item visible throughout the `zip_wrapper`
subtree, which is precisely where a module-private item in `zip_wrapper.rs` was already visible.
The Decision Drivers warn against enlarging the internal surface to make a move compile; this
does not, and the check for it is the grep above rather than an assurance.

Five items went the other way and are **more** encapsulated than before, having been module-global
in a 1715-line file and now private to a small child: `AES_EXTRA_FIELD_ID`,
`AES_VENDOR_VERSION_AE2`, `aes_vendor_version`, `RawCentralDirectory::unaddressable_records` and
`RawCentralDirectory::ambiguous_names`.

The struct **fields** are the one part that deserves justification rather than a count. The value
layer moved but its *builder* did not: `ZipArchive::scan_raw_central_directory` needs the cached
descriptor, so it stays on the facade and constructs `RawRecord` and `RawCentralDirectory` with
struct literals from the parent — which requires the fields to be nameable there. The alternative
was to move the scan too and leave a thin facade method, but that is a refactor of the building
logic, not a move, and this record classes that as the deferred half. So the seam is drawn where
the previous amendment said it was: at the value layer.

### Two things a reference grep got wrong, and the compiler caught

Recorded because the same mistake is easy to repeat on the `wrapper.rs` pass:

* `raw_len` was measured as used only inside the moving block and was therefore made private. It is
  called from `src/ffi/zip_wrapper/tests.rs` — a *sibling* child, which the grep did not read
  because it only searched `zip_wrapper.rs`. `pub(super)` fixes it and is still the honest reach.
  **When sizing a move, grep the sibling test child too, not just the parent.**
* `Read` moved out of the parent with the blocks, leaving the parent's `use std::io::{Read, Seek,
  SeekFrom}` unused — an error under the gate's `-D warnings`, not a warning.

One doc link did not survive the move and was converted rather than repointed:
`RawCentralDirectory`'s doc linked `[`ZipArchive::with_cached_file`]`, which cannot resolve from a
child and names a private method besides. It is a code span now, saying "in the parent module".

### Gate evidence for this amendment

`cargo check --all-features --all-targets` exit 0 with zero Rust warnings. `ffi::zip_wrapper` 34
passed / 0 failed. Full-gate figures are in the commit that carries this amendment.

## Amendment (2026-09-03, the `sfx_fallback_error_tests` "deliberately inline" verdict was overridden)

The table above records `sfx_fallback_error_tests` as **"No — deliberately inline"**, quoting the
module's own doc. That verdict no longer describes the tree: both it and `sfx_payload_cap_tests` were
moved out of `src/archive.rs` into child files under `src/archive/`, taking the parent from 2560 to
2369 lines with the test count unchanged (1941 passed before and after).

`src/archive.rs` now contains no inline `#[cfg(test)]` block at all; every `#[cfg(test)]` site in it
is a `mod` declaration. The deferral this record states — that the *large-file* refactor of
`archive.rs` proper is not worth doing yet — is unaffected; only the inline-test row is superseded.
