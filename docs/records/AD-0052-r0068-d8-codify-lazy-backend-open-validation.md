---
type: ADR
title: "AD 0052: Codify lazy-validation semantics for backend `open()` (Review 0068 D8)"
description: "Active — lazy-by-default codified in per-backend rustdoc; the promised D1 validate() hook never shipped (orphaned deferral, owner question open; see 2026-08-04 amendment)."
tags: [decision, ADR-0052, R0068-0024, R0068-0025, R0068-0026, R0068-0035]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0052: Codify lazy-validation semantics for backend `open()` (Review 0068 D8)

## Context and Problem Statement

Found in Review 0068 (R0068-0024, R0068-0025, R0068-0026, all High;
plus the partial-fix observation in R0068-0035 about libarchive
"validates and immediately throws the handle away").

Locations:

- `src/ffi/piz_wrapper.rs::PizArchive::open` (line 31)
- `src/ffi/zip_wrapper.rs::ZipArchive::open` (line 92)
- `src/ffi/zip_wrapper.rs::ZipArchive::open_with_password` (line 102)
- `src/ffi/sevenz_wrapper.rs::SevenZArchive::open` (line 27)
- `src/ffi/sevenz_wrapper.rs::SevenZArchive::open_with_password` (line 37)
- `src/ffi/libarchive_wrapper.rs::LibarchiveArchive::open` (line 118)

The reviewer observed that the meaning of "open succeeded" varies by
backend:

- **Piz, ZipReader, SevenZ:** `open(path)` only stores `path` (and an
  optional password); no archive-format validation runs until the next
  real operation. A corrupt or misnamed archive can sit in an opened
  handle and only fail later when the caller calls `list_files`,
  `extract_*`, or similar.
- **Libarchive:** `open(path)` opens a libarchive read handle, walks
  zero entries to verify the magic, then immediately frees the handle.
  Subsequent operations re-open. This catches malformed inputs at
  open-time but the validation is repeated on every operation.

The reviewer's recommendation was to harmonise — pick eager
validation for everyone, or codify lazy validation explicitly.

## Decision Drivers

* **Performance.** Eager validation at every backend's `open()` would
  add a redundant parse for the common case where the caller's next
  step *is* the validating operation (`list_files`, `extract_all`).
  ZIP central-directory parsing on a multi-GiB archive is not free.
* **Footgun severity is low in practice.** Every `Archive::list_files`
  / `extract_*` / `validate_integrity` path already bottoms out in a
  format parse that surfaces corruption; "the open succeeded but the
  next call failed" is operationally indistinguishable from "the open
  failed".
* **The libarchive eager-then-discard pattern is genuinely wasteful.**
  It costs a parse on `open()` *and* a parse on the first real
  operation, with no memoisation between them. R0068-0035 flags this
  separately. Either lazy (skip the open-time parse) or memoised
  (keep the handle) would be improvements; the latter is part of D4
  (per-backend handle reuse) so this ADR does not prescribe it.
* **Public-API stability.** Both shapes are observable today through
  test fixtures that exercise `Archive::open` with corrupted inputs.
  Forcing eager validation would change which call returns the error
  for the lazy backends; forcing lazy would change which call returns
  the error for libarchive. Either is a behavior change.
* **Forward compatibility with D1/D2.** The trait-based backend
  refactor (Group D D1) introduces a `validate(&self) -> Result<()>`
  method on `ReadBackend` that callers can opt into, decoupling
  "constructed handle" from "validated handle". Codifying lazy-by-
  default at the *current* surface keeps a clean migration target.

## Considered Options

1. **Eager-validate at every `open()`.** Force every wrapper to do
   enough parsing to confirm the archive is well-formed. Aligns
   behavior across backends; pays a redundant parse cost on every
   subsequent operation.
2. **Lazy-validate everywhere, including libarchive.** Drop libarchive's
   open-time `archive_read_open` + immediate-free. Fastest open path;
   diverges from current behavior most loudly (existing tests against
   libarchive-backed corruption may shift error sites).
3. **Codify the current mixed shape, document it explicitly, and
   defer harmonisation to D1.** Add a rustdoc paragraph on each
   backend's `open` describing its validation timing; treat this ADR
   as the contract until the trait refactor lands.

## Decision Outcome

ACCEPT: option 3.

Each backend's `open` rustdoc is updated to state explicitly whether
validation runs at open-time or first-use. No behavior change. The
forthcoming `ReadBackend` trait (D1) will introduce an opt-in
`validate(&self)` method so callers who *want* eager validation can
get it without forcing the cost on common operations.

Status: Documented; harmonisation deferred to D1.

### Implementation

Per-backend rustdoc additions on the `open` constructors. The bodies
stay unchanged — this is a contract codification, not a behavior
change. Tests already exercising the lazy/eager seams (see
`zip_wrapper.rs::test_open_succeeds_for_nonexistent_until_use`,
similar for piz/sevenz) document the observable behavior; this ADR
makes the rationale explicit.

When the trait-based backend refactor lands (D1), the `ReadBackend`
trait gains:

```rust
/// Verify the archive is well-formed without touching its data.
///
/// Cheap probe — typically central-directory or magic-bytes parse.
/// `open()` does NOT call this for backends that defer validation
/// (Piz, ZipReader, SevenZ); call it explicitly when the caller
/// needs "is this archive openable?" as a stable signal.
fn validate(&self) -> Result<()>;
```

Implementations re-use whatever parse the backend already runs at
first use (Piz: mmap + central-directory walk; ZIP: `zip::ZipArchive::new`;
7z: `sevenz_rust2::SevenZReader::new`; libarchive:
`open_read_handle` + first `archive_read_next_header`). Backends that
already validate eagerly (libarchive today) keep their open-time
behavior so the only change for them is "validate becomes a no-op".

## Consequences

* Good, because callers reading this ADR (or the per-backend rustdoc)
  know exactly when format errors surface for each backend.
* Good, because no in-flight tests change. The `cargo test --no-default-features`
  matrix and the encrypted-archive test suite continue to pass.
* Good, because the eventual D1 `validate()` hook gives callers an
  explicit opt-in without paying the cost on the common-path.
* Bad, because the documented contract still varies by backend until
  D1 lands. A caller writing backend-agnostic code today must assume
  the lazy semantics (i.e. "open does not guarantee validity").
* Bad, because the libarchive eager-then-discard cost remains until
  D4 (per-backend handle reuse) addresses memoisation. R0068-0035 is
  tracked there, not here.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — orphaned deferral: D1 shipped without `validate()`)

The deferral target this record pointed at was **orphaned**: the D1 trait refactor landed
(`pub(crate) trait ReadBackend`, `src/backend.rs`) **without** the promised
`validate(&self) -> Result<()>` hook, and no equivalent cheap open-validity probe exists anywhere
on the surface — neither the `Archive` facade nor the D2 `ReadArchive` typed handle
(`src/archive/mode_split.rs`, `v2-api` feature) exposes one; the nearest method,
`validate_integrity()`, is a full per-entry integrity walk with a different cost class and
contract. What this record actually decided is still shipped and in force: the per-backend
"Validation timing (AD 0052)" rustdoc survives on the current wrappers — `ZipArchive::open` /
`open_with_password` (`src/ffi/zip_wrapper.rs`) and `SevenZArchive::open` / `open_with_password`
(`src/ffi/sevenz_wrapper.rs`) document lazy first-use validation, and `LibarchiveArchive::open`
(`src/ffi/libarchive_wrapper/reader.rs`) documents the eager open-time autodetect — so the
codified mixed shape remains the contract. Two structural changes have since moved the Locations
list above: the Piz backend was removed entirely (AD 0007 collapse amendment 2026-07-23, paired
DCR-009), so `piz_wrapper.rs` no longer exists, and `libarchive_wrapper.rs` was split into
`libarchive_wrapper/reader.rs` / `writer.rs` (R5, 2026-07-20). Meanwhile a de-facto harmonised
contract emerged without the hook: **first-operation validation plus the AD 0065 frozen-listing
baseline** — every read backend memoises its first parsed listing as `Arc<Vec<ArchiveEntry>>`
(`ReadBackend::list_files_budgeted`, OI-0065-003) — so the first validating parse is paid once
per handle and cached, dissolving much of the "redundant parse" driver that motivated deferring
eager validation in the first place.

**Open owner question** (decision-review-2026-07-19 §B; also `docs/backlog.md` "Residual AD
deferral targets"): land the `validate()` hook on `ReadBackend` / the typed handles, or formally
retire the deferral in favour of "first-op validation + the AD 0065 listing cache is the
harmonised contract". This amendment records the orphaning; it does not rule. The lazy-by-default
codification itself remains in force — the record stays **active**.

## Amendment (2026-08-09, Review 0001 R0001-0012 — libarchive's eager-validation carve-out is now real)

**The one backend this record documents as eagerly validating did not.** `LibarchiveArchive::open`'s
rustdoc has stated since D8 that libarchive "eagerly validates at open-time — `open_read_handle`
runs libarchive's format autodetection against the file, then the handle is freed. A corrupt or
unsupported input therefore fails here rather than at first use." That was not what the code did.
`archive_read_open_filename` sets up the reader without bidding a format; libarchive parses on the
first `archive_read_next_header`, and that call was made **only** when the filename claimed a
compressed tar (`.tar.gz` / `.tgz` and friends, the AD 0062 A.6 mismatch probe). For every other
input — a corrupt `.tar`, a truncated `.iso`, a damaged raw stream — `open` returned `Ok` and the
failure surfaced at the first operation, which is precisely the lazy behaviour this record carves
libarchive **out** of.

R0001-0012 closes the gap: the first header is now probed for every libarchive format. `ARCHIVE_EOF`
remains the legal empty-archive success path, the compressed-tar mismatch error keeps its exact
message and now layers its extra format assertion on top of the one shared probe, and the R0075-0021
non-EOF/non-OK error path is preserved. The cost is one header parse per open, on a call that
already opens the file and configures every format and filter reader.

Nothing in this record's ruling changes — the lazy-by-default codification and the libarchive
exception both stand as written. What changes is that the exception is now implemented, so the
contract a caller reads is the contract they get. No DCR: the code moved to match the record, not
the other way round.

The open owner question above is unaffected and still unresolved.
