---
type: ADR
title: "AD 0053: Review 0068 Group D — design baseline for the remaining architectural pass"
description: "Design baseline; the 2026-08-04 amendment reconciles D1–D10 against the tree (D1/D2/D3 shipped — D2 additively behind v2-api; D4 pivoted to AD 0065; D9 executed 2026-07-20; D10 still deferred)."
tags: [decision, ADR-0053, ADR-0051, ADR-0019, DCR-004, R0068-0024, R0076-0085, R0076-0088, R0068-0035, OI-0076-004]
timestamp: 2026-06-06T00:00:00Z
status: active
---

# AD 0053: Review 0068 Group D — design baseline for the remaining architectural pass

Status: Active (governing baseline for the unexecuted remainder). Amended 2026-08-04 — the "Design baseline. No code change." snapshot is superseded for most sub-phases: D1/D2/D3 shipped (D2 additively behind `v2-api`, not as the reshape planned here), D4 pivoted to the AD 0065 listing-cache baseline, D9 was executed 2026-07-20, D10 remains deferred. See the reconciliation amendment at the end.

## Context and Problem Statement

Found in Review 0068 (Group D bundle: R0068-0024..0046, 0057,
0071..0077; ~22 architectural-refactor findings). User routing under
the gate's Phase 2 was "do not defer" — these are forward work, not
permanent backlog.

D1 (backend trait scaffold), D5 (finalize dedup, already done), D7
(Operation enum), and D8 (lazy/eager open codification) landed in the
Review-0068 closure pass (AD 0051). The remaining sub-phases — D2, D3,
D4, D9, D10 — each materially shape public API or backend internals
and need design baselining before implementation begins. This ADR is
that baseline.

The intent is **not** to land the implementations here. Each
sub-phase still gets its own implementation ADR when it lands. This
record nails down the questions that future implementers need to
answer the *same* way, so the work is composable across multiple
sessions.

## Decision Drivers

* **Public-API stability vs progress.** The crate is on `0.2.x`. Most
  of D shapes the public surface (Archive split, validated-selection
  token, large-file refactors that move `pub` items). Each landing
  must preserve compile-time compatibility for v0.1.x consumers
  through one minor cycle, then break cleanly on v0.3 — anything
  shorter risks the same churn the prior reviewer flagged.
* **Sequencing.** D2/D9 depend on D1's trait. D3 is independent of
  the trait but couples to D2's surface. D4 is per-backend
  independent. D10 is mechanical and last.
* **Scope realism.** Group D was estimated at 3–5 weeks of focused
  work. Trying to land it in one session forced shortcut decisions
  (no deprecation cycle, no per-backend tests for the refactor, no
  bench validation). The plan now explicitly sequences each item
  with its own ADR + commit — slower but correct.
* **Test-coverage discipline.** Several D items (D4 caching, D9
  read/write split) change observable behaviour (mmap reuse, error
  ordering on misuse). Each needs a regression test in the same
  commit, not a follow-up.

---

## D2 — `Archive` god-object split

### Plan

Three public types replace the current monolithic `Archive`:

```rust
// Read-only handle. Returned from `Archive::open`, `open_encrypted`,
// `open_at_offset`, `open_sfx`, `extract_stub`. All read methods
// (list_files, extract_*, test_integrity, find_entry, detect_*).
pub struct ReadArchive { /* current Archive read state */ }

// Append-only writer. Returned from `Archive::create`. All
// add_*/finish methods.
pub struct WriteArchive { /* current Archive write state */ }

// Modify session. Returned from `Archive::modify`,
// `modify_with_options`. add_entry / remove_entry / commit_changes
// only — no list/extract beyond what commit_changes uses internally.
pub struct ModifyArchive { /* current Archive modify state */ }
```

`Archive` itself becomes a sum type for one minor cycle:

```rust
#[deprecated(since = "0.3.0", note = "use ReadArchive / WriteArchive / ModifyArchive directly")]
pub enum Archive {
    Read(ReadArchive),
    Write(WriteArchive),
    Modify(ModifyArchive),
}
```

with delegating methods so existing v0.2 calls (`archive.list_files()`)
keep compiling. After 0.3 → 0.4 the `Archive` alias is removed.

### Migration steps

1. Land the three new types behind feature gate `v2-api` (off by
   default in 0.3.0). Public API gains the types; `Archive` is
   unchanged. Internal modules continue to use `Archive`.
2. Switch internal code to construct mode-specific handles when
   feature flag is on; keep the old enum for the off case.
3. In 0.3.x cycle: turn `v2-api` on by default; deprecate `Archive`
   construction methods.
4. In 0.4.0: remove `v2-api` feature flag; remove `Archive` enum;
   clean up internal mode dispatch (`ArchiveMode` enum can go too).

### Open question

`open_at_offset` and `open_sfx` produce read-only handles today, but
the SFX detection result + offset already need to flow through to
backend wiring. Decision: SFX entry points return `ReadArchive`
directly (matching `open`); the `_backing_tempfile` lifetime stays
private to that struct.

### Implementation note

The `ArchiveBackend` enum and its `ZipWriter`/`ZipReader`/etc.
variants split symmetrically: `ReadArchiveBackend` (no `ZipWriter`),
`WriteArchiveBackend` (only `ZipWriter` + libarchive write side).
This eliminates the `match … ZipWriter(_) => Err(write_mode_only)`
arms that pepper extraction.rs / inspection.rs today.

### Addendum — D2 typed-handle parity + Drop ownership (2026-06-06, DCR-004)

The D2 typed handles shipped as a *curated subset* of the legacy
`Archive` surface (OI-0076-004 / R0076-0085..0088). They now reach
**full mode-appropriate parity**:

* `ReadArchive` exposes the complete read surface (all inspection +
  extraction methods, the read-only probes, `open_with_sfx_progress`,
  and the static `detect_sfx` / `extract_stub`).
* `WriteArchive` exposes `add_directory_recursive` and the
  write-progress `entry_count`.
* `ModifyArchive` exposes `add_entry_from_reader` and the
  `replace_entry*` family, plus source inspection (`list_files`,
  `entry_count`, `find_entry`, `find_entries`) — without which the
  existing `remove_entry_by_id` was unusable (no way to discover ids).

**Durability ownership (R0076-0088).** `WriteArchive::finish(self)` is
the durable, *error-surfacing* commit path. A `WriteArchive` dropped
without `finish()` runs **one** best-effort finalize via the new
`pub(crate) Archive::finalize_write_on_drop`, which also marks the
inner handle finalized so `Archive::Drop` does not finalize or warn a
second time. This was chosen over "leave a truly partial archive"
because the backends are asymmetric: `ZipWriter` self-finalizes in its
own `Drop`, whereas `LibarchiveArchive` has **no** `Drop`, so its raw
write handle is freed *only* by the finalize path — suppressing
finalization entirely would have leaked it. The legacy `Archive`
facade keeps its own best-effort `Drop`-finalize for v0.3
source-compat. See DCR-004 and `docs/API_REFERENCE.md` for the parity
matrix.

---

## D3 — `_unchecked` → `ValidatedSource` token

### Plan

Replace the `pub(crate) extract_to_memory_unchecked` /
`extract_to_stream_unchecked` methods with a token-typed surface:

```rust
pub(crate) struct ValidatedSource<'a> {
    archive: &'a ReadArchive,
}

impl<'a> ValidatedSource<'a> {
    /// Constructor accepting "I trust this archive's listing".
    /// Only callable from internal code that has already produced
    /// the listing it is iterating.
    pub(crate) fn from_listed_archive(archive: &'a ReadArchive) -> Self {
        Self { archive }
    }

    pub(crate) fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>>;
    pub(crate) fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor>;
}
```

Existing call sites in `modification.rs::commit_changes` and
`inspection.rs::calculate_manifest_digest` migrate to:

```rust
let src = ValidatedSource::from_listed_archive(&self);
for entry in entries {
    let stream = src.extract_to_stream(&entry.path)?;
    …
}
```

The type's existence is the safety invariant; the comment
"`_unchecked` is fine because we already listed" becomes a type fact.

### Migration steps

1. Land `ValidatedSource` alongside the existing `_unchecked`
   methods as an additive change.
2. Migrate internal call sites one module at a time.
3. Once no internal caller uses `_unchecked`, remove those methods.

Depends on D2 only because the token wraps `&ReadArchive` — until
D2, it can wrap `&Archive` and migrate when D2 lands.

---

## D4 — per-backend handle reuse / metadata caching

### Plan (per backend)

| Backend | Cache target | Thread-safety strategy |
|---|---|---|
| Piz | Mmap + parsed central directory | `OnceLock<PizState>`. Mmap is `Send + Sync`. |
| ZIP (zip-rs) | `zip::ZipArchive<File>` reused across calls | `Mutex<Option<RawZipArchive>>`. Cheap to (re)create on lock-poison. |
| 7z | Parsed TOC | `OnceLock<Toc>`. The reader is iterator-shaped so per-extract reopen still happens; only the TOC is cached. |
| UnRAR | One handle + iterator state separated | `Mutex<HandleState>`. UnRAR's process-wide lock (`UNRAR_LOCK`, AD 0019) still serialises FFI; the per-archive cache reduces the parse-per-call cost within that lock. |
| Libarchive | Validated-handle memo (D8 / R0068-0035) | `OnceLock<ValidatedMarker>`. The handle itself is iterator-shaped; only the "validated" bit is cached so the next operation skips re-validation. |

### Sequencing

D4 is per-backend independent. Land each backend's cache as its own
ADR + commit + microbenchmark. None depends on D1/D2/D3/D9. D4 is
the most user-visible perf improvement in the Group D bundle.

### Open questions

* Will the cache change `Archive::is_encrypted` for non-validated
  inputs? No — `is_encrypted` already uses `list_files_for_limits`,
  which goes through the trait; the cache hit replaces a parse with
  a hash lookup but returns the same `Vec<ArchiveEntry>`.
* Lock contention on the Mutex variant? Measured the same way as the
  current `UNRAR_LOCK` benchmark: parallel-extract two archives of
  the same backend, expect linear scaling minus the lock window.

---

## D9 — `LibarchiveArchive` reader/writer split

### Plan

Split into two structs:

```rust
pub(crate) struct LibarchiveReader {
    path: String,
    /* read-only state: nothing else today, possibly the validated
       handle marker added by D4 */
}

pub(crate) struct LibarchiveWriter {
    path: String,
    write_handle: Option<*mut libarchive_sys::archive>,
    progress: Option<Box<dyn ProgressCallback>>,
    bytes_written: u64,
    entries_written: usize,
    stream_buffer: Vec<u8>,
}
```

`LibarchiveReader` impls `crate::backend::ReadBackend`;
`LibarchiveWriter` impls a future `crate::backend::WriteBackend`.

### Migration steps

1. Introduce both structs alongside the existing `LibarchiveArchive`.
2. `Archive::open` constructs `LibarchiveReader`;
   `Archive::create` constructs `LibarchiveWriter`.
3. Once both are in place, remove `LibarchiveArchive`.

Depends on D1's trait surface (already landed) and ideally D2's
mode-specific backend enum so the variants can carry the new types
without unwrapping a sum.

---

## D10 — large-file refactors and `#[cfg(test)]` move-out

### Plan

Files exceeding ~1 KLOC each get split per concern. Pure rearrangement
— no behaviour change. `pub use` re-exports keep public API stable.
Suggested splits:

| File | Splits |
|---|---|
| `archive.rs` | `archive/mod.rs` (facade) + `archive/open.rs` (open/open_encrypted/open_at_offset) + `archive/lifecycle.rs` (finish/close/Drop) |
| `extraction.rs` | `extraction/mod.rs` + `extraction/preflight.rs` (limits + path checks) + `extraction/dispatch.rs` (`dispatch_read_backend`, `extract_selected`, `dispatch_extract_core`) |
| `inspection.rs` | `inspection/mod.rs` + `inspection/multipart.rs` + `inspection/integrity.rs` + `inspection/manifest_digest.rs` |
| `modification.rs` | `modification/mod.rs` + `modification/tracker.rs` + `modification/commit.rs` + `modification/zip_extras.rs` |
| `ffi/libarchive_wrapper.rs` | once D9 lands: `ffi/libarchive/{mod,reader,writer,common}.rs` |
| `ffi/wrapper.rs` (UnRAR) | `ffi/unrar/{mod,parse,extract,recovery}.rs` |

Test blocks (`#[cfg(test)] mod tests { ... }`) move to
`tests/unit/<module>.rs` only when the tests use only public API. Tests
relying on `pub(crate)` items stay in their original module — moving
them out would force exposing internals.

### Sequencing

D10 lands last (after D1/D2/D3/D4/D9) so the splits reflect the
post-refactor layout, not the current one. Otherwise the refactors
land twice.

---

## Decision Outcome

ACCEPT: codify the design baseline for D2, D3, D4, D9, D10 here so
future sessions implement against a single agreed plan. Each
sub-phase still requires its own implementation ADR when it lands —
this ADR is the contract, not the patch.

Status: Design baseline. No code change. Implementation deferred to
future sessions per sequencing above.

### Implementation

None in this ADR. Future commits referencing D2/D3/D4/D9/D10 land
their own implementation records and update this ADR's "open
questions" if the answers shift.

## Consequences

* Good, because Group D is no longer a vague backlog item — every
  remaining sub-phase has a documented entry point and migration
  shape.
* Good, because future contributors (or a future self in a fresh
  session) can read this ADR and start any of D2/D3/D4 without
  having to re-derive the design from R0068's stale-finding context.
* Good, because the `v2-api` feature-flag pattern for D2 limits
  blast radius — v0.2.x consumers see no change until they opt in.
* Bad, because keeping the deprecated `Archive` enum around for one
  minor cycle adds boilerplate that gets removed later. Acceptable
  cost vs forcing a coordinated v0.3 cutover.
* Bad, because the design baseline does not commit to specific
  microbenchmark targets for D4. Future implementations must define
  those before landing.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — D1–D10 status reconciliation)

The Decision Outcome's "Design baseline. No code change. Implementation deferred to future
sessions" status no longer matches the tree: most sub-phases have since shipped, pivoted, or been
executed. This amendment reconciles each against the current source and sibling records; the
original sections above stand unedited per the immutability policy.

- **D1 (backend trait)** — **shipped.** `pub(crate) trait ReadBackend` in `src/backend.rs`
  carries an `extract_all(&self, &mut ExtractionPlan)` method implemented per backend, retiring
  the per-backend dispatch ladder (OI-0075-004 Phase 7, 2026-04-30).
- **D2 (`Archive` god-object split)** — **shipped additively, not as the reshape planned above.**
  `ReadArchive` / `WriteArchive` / `ModifyArchive` live in `src/archive/mode_split.rs` behind the
  `v2-api` cargo feature (off by default), each wrapping the legacy `Archive` as an inner field.
  The planned deprecated `Archive` sum enum and the Implementation-note backend-enum split
  (`ReadArchiveBackend` / `WriteArchiveBackend`) were never built — the `ArchiveBackend` enum in
  `src/archive.rs` is unchanged. Landing recorded under OI-0075-003 and OI-0075-004 Phase 8
  (2026-04-30); handle parity and Drop ownership in the DCR-004 addendum above. Migration steps
  3–4 (default-on flip in 0.3.x; flag and legacy-facade removal in 0.4) remain pending.
- **D3 (`ValidatedSource` token)** — **shipped (AD 0055), but its invariant is not structurally
  enforced.** `ValidatedSource` lives in `src/extraction.rs` and wraps `&Archive` via
  `Archive::validated_source()` — not the post-D2 `&ReadArchive` sketched above — so a handle in
  any mode can mint the token and "I have already produced the listing" remains a convention
  rather than a type fact. Whether to re-home it on `&ReadArchive` or converge it with the
  `ValidatedEntry` token that OI-0076-002 shipped in `src/security.rs` is an open owner decision
  (see AD 0055's amendment and `docs/backlog.md`, "Residual AD deferral targets").
- **D4 (per-backend handle reuse / metadata caching)** — **pivoted.** The first cut landed as
  AD 0054 (piz mmap + ZIP `RawZipArchive<File>` handle caches), but the per-primitive cache table
  above was then overtaken by the **AD 0065 frozen-listing baseline**: every read backend now
  memoises its parsed listing in a `OnceCell<Arc<Vec<ArchiveEntry>>>` (OI-0065-003), which is the
  caching contract that actually shipped for 7z / UnRAR / libarchive — not the `OnceLock<Toc>` /
  `Mutex<HandleState>` / `ValidatedMarker` designs sketched here. The piz backend itself was
  removed by DCR-009 (2026-07-23), deleting that row's substrate; the ZIP handle cache survives
  (AD 0054 amendment).
- **D8 (lazy open codification; noted as landed in the Context above)** — codified as AD 0052,
  but the `validate()` opt-in hook that record promised for D1 **never shipped**: no `validate`
  method exists on `ReadBackend` or any backend wrapper. Land-or-retire is an open owner decision
  (see AD 0052's amendment and `docs/backlog.md`, "Residual AD deferral targets").
- **D9 (libarchive reader/writer split)** — deferred by AD 0056 on a sequencing premise that
  D2's additive landing falsified, then **executed 2026-07-20** as a module-level split:
  `src/ffi/libarchive_wrapper/{reader,writer}.rs`, with the single `LibarchiveArchive` struct
  retained. The two-struct field-level split planned above remains available as later work; see
  AD 0056's amendment.
- **D10 (large-file refactors + `#[cfg(test)]` move-out)** — **still deferred per AD 0057,
  partially overtaken.** The `ffi/libarchive_wrapper.rs` row of the split table was delivered by
  the D9 module split, and the inline test blocks of `archive` / `extraction` / `inspection` /
  `modification` have moved into file-as-module `src/<module>/tests.rs` children (in-crate,
  keeping `pub(crate)` access — not the `tests/unit/` graduation planned above). The per-concern
  splits of the remaining large files (`extraction.rs`, `modification.rs`, `archive.rs`,
  `format.rs`, `security.rs`, `ffi/wrapper.rs`) have not happened, and the "D10 lands last
  (after D1/D2/D3/D4/D9)" premise is partially dissolved now that D2 shipped without the
  backend-enum reshape.

D5 and D7, recorded as landed in AD 0051, are unchanged. This record stays **active**: it remains
the governing baseline for the unexecuted remainder (D2 migration steps 3–4, the D3
`&ReadArchive` re-homing, the D9 field-level split, and D10), with the statuses above superseding
the "Implementation deferred" snapshot for everything else.

## Amendment (2026-09-01, Type 1 reconciliation — D1's `validate()` hook landed; D10 progress)

Two corrections to the 2026-08-04 amendment above, both from checking its claims against the tree
rather than against sibling records.

**D8's `validate()` hook, promised for D1, has shipped.** The 2026-08-04 amendment says it "never
shipped: no `validate` method exists on `ReadBackend` or any backend wrapper", and that
land-or-retire was an open owner decision. That was true when written and was overtaken seventeen
days later: commit `4f521e0` (2026-08-21) added `fn validate(&self) -> Result<()>` to
`ReadBackend` as a *provided* method whose default body is `self.list_files().map(|_| ())`, and
`Archive::validate` exposes it through `dispatch_read_archive`, with `ModifyArchive` forwarding to
the same place. No backend overrides it — one default body covers all four, which is why the
per-backend timing differences are documented on the method rather than in the impls. AD 0052
already records this properly in its own 2026-08-21 amendment ("deferral retired, first-operation
validation is the contract, `validate()` landed"), so the two records disagreed and only this one
was wrong. The owner decision the earlier amendment left open is therefore closed, and closed the
way it named: landed, not retired.

The design note that made the probe worth having is worth preserving here because it is what
dissolved the original objection. Eager validation was deferred partly on a "redundant parse" cost:
a `validate` followed by `list_files` would parse twice. Under the AD 0065 frozen-listing baseline
it does not — `validate` populates the same memoised `Arc<Vec<ArchiveEntry>>` that `list_files`
then hits, and `ffi::zip_wrapper::tests::validate_then_list_files_serves_the_cached_listing` fails
if that stops being true. A failed `validate` does not poison the cache, because `get_or_try_init`
stores only on `Ok`.

**D10 has moved further than the 2026-08-04 status suggests, and what remains is smaller and more
precisely bounded.** Since that amendment: the `#[cfg(test)]` move-out completed for the three
large FFI files into `wrapper/{tests,staged_length_tests}.rs`, `zip_wrapper/tests.rs` and
`sevenz_wrapper/tests.rs`; the two `zip_wrapper` seams AD 0057 marked as bandwidth-gated rather
than dependency-gated landed as `zip_wrapper/raw_directory.rs` and `zip_wrapper/aes.rs`; and, on
2026-09-01, the last two inline `#[cfg(test)]` blocks in `src/archive.rs` — a named large-file
target that had kept them while every other target was cleared — moved to
`archive/sfx_fallback_error_tests.rs` and `archive/sfx_payload_cap_tests.rs`, taking the parent
from 2560 to 2369 lines. Module paths and test names are unchanged by that move, so no test was
renamed, re-pathed or lost.

What is left of D10 is unchanged in kind and still waits on named work rather than on bandwidth:
the per-concern splits of `ffi/wrapper.rs` (the largest remaining production file),
`extraction.rs`, `modification.rs`, `archive.rs`, `format.rs` and `security.rs`, all behind D2's
backend-enum reshape. `ffi/sevenz_wrapper.rs` stays explicitly off the mechanical-split list:
nearly its whole production half is a single `impl` block, so the concern boundaries have to be
named inside that impl before a split can make it clearer rather than merely smaller.

**The unexecuted remainder this record governs is therefore now: D2 migration steps 3-4, the D3
`&ReadArchive` re-homing, the D9 field-level split, and D10's large-file splits.** D1 is complete,
including the hook. The record stays **active**.
