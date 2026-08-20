---
type: ADR
title: "AD 0063: Review 0075 — closure record + design positions"
description: "Implemented in Review 0075."
tags: [decision, ADR-0063, ADR-0053, ADR-0056, ADR-0058, ADR-0062, R0069-0057, R0075-0001, R0075-0073, R0075-0074, R0075-0075, R0075-0076, R0075-0077]
timestamp: 2026-04-29T00:00:00Z
status: active
---

# AD 0063: Review 0075 — closure record + design positions

## Context and Problem Statement

Review 0075 raised 85 findings against the post-Review-0074 codebase.
Triage during the gate routed every issue to one of: inline-fix,
rejected-as-already-tracked, deferred-then-fixed, or
deferred-then-tracked. This ADR captures the **architectural
positions** the review confirmed and the **rejected findings** that
need a durable record so the same arguments don't get relitigated.

## Decision Drivers

- **Stability of architectural baselines.** Reviewers consistently
  rediscover the design baselines AD 0053 / AD 0056 / AD 0058 / AD
  0062 already locked. Recording the rejection here closes the loop
  without re-debating each item.
- **Phrase-based encryption classifier.** R0069-0057 had reduced the
  noun-only substring classifier to a defensive fallback while
  retrofitting backends. R0075 lets us retire the fallback entirely
  and record the post-retrofit position.
- **Snapshot semantics.** R0075-0001 brought to light that the
  rustdoc on `Archive` claimed stronger snapshot semantics than the
  backends provide. The doc-side fix is in this review; the
  per-backend caching alignment lives under OI-0075-002.
- **Send safety drift.** The `unsafe impl Send for Archive` SAFETY
  comment had drifted as Piz / ZipReader gained interior caches. This
  review refreshes the comment and adds a compile-time `Send`
  assertion as anchor for future drift.

## Considered Options

For each rejected/recorded finding the considered options are
captured inline in the table below.

## Decision Outcome

ACCEPT (closure record + design positions). Per-finding outcomes:

### Architectural baselines reaffirmed (REJECT — already-tracked)

| Issue | Topic | Position |
|---|---|---|
| R0075-0073 | `pub mod ffi` exposes hidden surface | AD 0062 A.1 + OI-0069-001 already locked the visibility plan; the plan is implemented. The `ffi` module remains `pub(crate)` and `#[doc(hidden)]` per AD 0062 A.1, and integration tests reach into it via the same `#[doc(hidden)]` re-exports the user surface relies on. Closed. |
| R0075-0074 | Libarchive monolith | AD 0056 explicitly defers the LibarchiveReader/LibarchiveWriter split to post-D2 (`v2-api` mode-split landing). Position unchanged. |
| R0075-0075 | `Archive` god-object | AD 0053 D2 baseline records the exact split (`ReadArchive` / `WriteArchive` / `ModifyArchive`). Position unchanged. |
| R0075-0076 | Backend enum mixes read/write | AD 0053 D2 implementation baseline plans the `ReadArchiveBackend` / `WriteArchiveBackend` enum split. Position unchanged. |
| R0075-0077 | Trait abstraction incomplete vs enum dispatch | AD 0053 D1 chose the trait + enum coexistence on purpose: the enum is the dispatch primitive, the trait is the variant-agnostic surface helpers consume. Closed. |
| R0075-0085 | Build defaults require RAR / libarchive | OI-0058-001 + AD 0058 already track Stage 1 footprint split. Position unchanged. |

### Already-tracked elsewhere (REJECT — point to existing OI/AD)

| Issue | Topic | Pointer |
|---|---|---|
| R0075-0008 | TOCTOU `validate_file_path` → `File::open` | OI-0070-002 tracks the no-follow-open helper integration. |
| R0075-0012 | `ValidatedSource` does not carry validation witness | OI-0069-002 R0069-0063 + AD 0053 D2 — resolved when the post-D2 ReadArchive token shape lands. |
| R0075-0029 | Libarchive recursive add Windows separators | Same area as OI-0065-001 Windows libarchive enablement; resolved with that work. |
| R0075-0034 | ZIP per-entry compression keyed by index | DEF-005 / OI-0069-002 R0069-0064 — open under that umbrella. |
| R0075-0038 | Modify `looks_like_encryption` substring fallback | Resolved by Review 0075's typed-Password retrofit (see below). |
| R0075-0022 | Libarchive `path: String` lossy | OI-0075-001 (this review) consolidates the non-UTF-8 path policy decision. |

### Typed-Password retrofit (R0075-0026, R0075-0027, R0075-0038)

The substring-based encryption classifier introduced in R0069-0057 as
a defensive fallback is **retired**. Every backend now classifies at
its FFI seam and emits `ArchiveError::Password` directly:

- **Libarchive** (`classify_libarchive_error`): replaces the
  noun-only substring match (`encrypt`/`password`/`passphrase`) with a
  conservative phrase-based predicate that checks against an explicit
  list of libarchive error vocabulary (`encrypted file is
  unsupported`, `passphrase required`, `wrong password`, etc.). A
  corrupt archive whose error text incidentally mentions one of those
  words no longer collides with the classifier.
- **7z** (`open_reader`): same shape — phrase-based encryption
  classifier instead of single-noun substring.
- **UnRAR** (`map_unrar_error`): already used `ERAR_BAD_PASSWORD` /
  `ERAR_MISSING_PASSWORD` error codes; no change needed.
- **ZIP / Piz**: already emit typed `Password` directly from their
  wrappers; no change needed.
- **Modification side** (`looks_like_encryption`): the substring
  fallback is removed. The function dispatches purely on
  `ArchiveError::Password` now that every backend retrofits at the
  FFI seam.

Status: Implemented in Review 0075.

### Snapshot semantics (R0075-0001) — doc-side resolution

`Archive`'s rustdoc rewritten to "best-effort metadata cache" with
explicit per-backend caching behaviour (Piz mmap, ZipReader cached
handle, 7z TOC, UnRAR FFI handle, libarchive lazy reopen).
OI-0075-002 tracks the optional follow-up of unifying the caching
policy across backends.

Status: Doc-side fix landed in Review 0075. Per-backend alignment
tracked in OI-0075-002.

### Send safety audit (R0075-0004)

The per-variant SAFETY comment for `unsafe impl Send for Archive`
was rewritten to match current fields (Piz `OnceCell<Mmap>`,
ZipReader `Mutex<Option<RawZipArchive<File>>>`). A compile-time
`Send` assertion was added so any future field addition that
introduces a non-`Send` type fails at type-check rather than slipping
past a stale comment:

```rust
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Archive>();
};
```

Status: Implemented in Review 0075.

### Drop / finish error semantics (R0075-0005) — partial fix + plan

`Archive` gains two sticky flags (`write_poisoned`, `finalized`) so
`Drop` can:
- emit a structured `eprintln!` when a Write-mode handle is dropped
  without `finish()` / `close()`,
- skip its silent-finalize attempt when `write_poisoned` is set,
- stay silent for Modify-mode handles whose
  `commit_changes()` succeeded (because `finalized` was set on the
  successful path).

The long-term home for this contract is the typed `WriteArchive`
introduced by AD 0053 D2 — at that point `finish()` consuming
`WriteArchive` becomes the *only* durable commit path and the sticky
flags can be dropped. Tracked as OI-0075-003.

Status: Inline fix landed; AD 0053 D2 closes the loop.

### Non-UTF-8 path policy (R0075-0007, R0075-0023, R0075-0055, R0075-0082)

User route during Phase 2 D-D was "preserve raw_path, but keep as
open issue". Tracked as OI-0075-001. Policy decision (Option A —
preserve via raw_path-style fields, vs Option B — reject loudly at
facade) deferred to that OI's ADR.

Status: OI-0075-001 records the bundle.

### API ergonomics & SFX/RAR internals

Bundled into OI-0075-004 with the small inline fixes
(R0075-0050 attributes, R0075-0060 RAR test skip,
R0075-0069 shebang tightening, R0075-0070 ZIP local-header
validation, R0075-0072 `is_confirmed` structural) landing in this
review. The remaining 14 items each request an API decision; see
OI-0075-004 for the per-item table.

## Consequences

* Good, because the architectural baseline rejections stop being
  re-discussed every review.
* Good, because the typed-Password retrofit closes the
  R0069-0057 loose end and removes the substring fallback at every
  layer.
* Good, because the compile-time `Send` assertion structurally
  prevents the SAFETY-comment drift R0075-0004 raised.
* Bad, because the AD-0053 D2 `WriteArchive` work is still pending —
  the sticky-flag fix in R0075-0005 is interim.
* Bad, because the non-UTF-8 path policy decision is deferred to
  OI-0075-001 rather than locked here. The deferral is intentional;
  it is a multi-platform-test-required design call.
