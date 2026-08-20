---
type: ADR
title: "AD 0065: Backend caching baseline — frozen-view at first use (Option A)"
description: "Accepted (2026-04-30) — closes OI-0075-002 R0075-0001 caching contract gap."
tags: [decision, ADR-0065, ADR-0053, ADR-0054, DCR-005, R0075-0001, R0079-0001, OI-0075-002]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# AD 0065: Backend caching baseline — frozen-view at first use (Option A)

## Status

Accepted (2026-04-30) — closes OI-0075-002 R0075-0001 caching contract gap.

## Context

`Archive`'s rustdoc has historically claimed snapshot semantics that the
read backends did not uniformly deliver:

- **Piz** holds a memoised `Mmap` of the archive bytes; central-directory
  walks reparse the same bytes every call, but the mapping is stable for
  the lifetime of the `Archive`, so two consecutive `list_files()` calls
  observe the same source.
- **Zip (encrypted)** holds a `Mutex<Option<RawZipArchive<File>>>` cached
  after first parse — frozen view for the `Archive` lifetime.
- **SevenZ** parses the table-of-contents once and reuses it.
- **UnRAR** keeps its FFI handle alive between calls.
- **Libarchive** reopens the file on every operation. A modify-while-open
  rewrite from another process is observable via `list_files()` returning
  a different set than the one a sibling extract just used.

Review 0075's R0075-0001 finding flagged the rustdoc–reality drift. The
documentation was tightened in that review (`Archive`'s rustdoc now
spells out per-backend caching behaviour explicitly), and a compile-time
`Send` assertion was added so future field changes that introduce a
non-`Send` type fail at type-check time. The runtime gap remained.

OI-0075-002 routed the design question to "decide the caching baseline"
with two options:

- **Option A** — every read backend memoises listing/metadata once.
  Frozen view at first use becomes the canonical contract.
- **Option B** — codify the current lazy-reopen behaviour for
  libarchive in the public contract; treat "snapshot" wording as
  best-effort.

## Decision

We adopt **Option A**. Every read backend caches its listing the first
time it is observed, and subsequent operations on the same `Archive`
handle return the cached view. The contract `Archive` exposes is now
"frozen at first observation," not "freshly re-read on every call."

Concretely:

- `LibarchiveArchive` caches the entry list in a
  `OnceCell<Vec<ArchiveEntry>>` populated by the first `list_files`
  call.  Subsequent listings clone the cached vector. Extraction-time
  walks (which need a fresh libarchive read handle anyway because the
  iterator is one-shot) still reopen the file for the payload pass —
  the *listing* contract is what's memoised, not the per-byte read
  state.
- `PizArchive` already caches the mmap; we additionally memoise the
  parsed `Vec<ArchiveEntry>` so the central-directory reparse cost is
  paid once per `Archive`. (Functionally equivalent to the prior
  behaviour because the mmap was stable, but makes the contract
  explicit and uniform.)
- `ZipReader` (encrypted) and `SevenZ` already satisfy the contract; no
  change.
- `UnrarArchive` caches the entry list in a `OnceCell<Vec<ArchiveEntry>>`
  populated by a walk over a dedicated fresh handle. *(Amended 2026-06-11
  per DCR-005 / R0079-0001: this AD originally recorded UnRAR as "already
  satisfies the contract; no change" because the long-lived FFI handle
  masked the gap for paired facade calls — in fact the header walk consumes
  the handle, so the uncached `list_files_for_limits` path returned empty
  listings on any second walk and could poison the facade cache. The
  memoised fresh-handle walk makes UnRAR genuinely uniform with the other
  backends.)*

## Consequences

### Good

- **Uniform contract.** Every read backend now agrees on what
  "snapshot" means: the entry set returned by the first listing is the
  one all subsequent operations on this `Archive` see.
- **Resilience to concurrent rewrites.** A separate process replacing
  the archive file mid-use no longer flips libarchive's view between
  two `list_files()` calls within the same `Archive`. Callers who want
  fresh-from-disk metadata reopen the archive — same shape Piz/UnRAR
  already had.
- **Simpler reasoning for the post-D2 typed handles.** `ReadArchive`'s
  `list_files()` is purely a function of the `ReadArchive` value, not
  of clock time and external file state.

### Bad / costs

- **Memory.** A libarchive listing of a million-entry archive now sits
  in memory for the `Archive`'s lifetime. In practice the entries are
  small structs (`PathBuf` + sizes + timestamps), and callers who hold
  an `Archive` across many operations were already paying this cost
  somewhere — typically by stashing `list_files()`'s return value
  themselves.
- **Two-layer caching during typical extract flows.** The facade
  `Archive::entry_cache` (predates this ADR) and the new backend-level
  `cached_listing` cells both hold a `Vec<ArchiveEntry>` once an
  extract that ran the safety preflight (which goes through
  `list_files_for_limits` / the backend cache) is followed by a
  user-visible `Archive::list_files()` (which populates the facade
  cache from a clone of the backend's Vec). Total: the listing's bytes
  are pinned twice for the `Archive`'s lifetime. We accept this as a
  bounded cost because (a) the listings are small relative to the
  archive's payload data the user is processing alongside, (b) the
  alternative — making the trait surface return `Arc<Vec<ArchiveEntry>>`
  through five backends and the facade — touches every read backend
  for a constant-factor memory saving, and (c) callers who care can
  drop the `Archive` after the preflight if they don't need the
  facade cache. Future work (when the typed-handle split in AD 0053 D2
  becomes the default surface) will revisit whether the facade cache
  remains useful or the backend cache should expose a slice directly.

  *(Amended 2026-07-06 per OI-0065-003: this cost is now resolved.
  `ReadBackend::list_files` returns `Arc<Vec<ArchiveEntry>>`; all five
  backends cache `OnceCell<Arc<Vec<ArchiveEntry>>>` — the zip-crate
  backend gained a listing cache in the process, closing its gap in
  the "Concretely" list — and the facade `entry_cache` holds the same
  `Arc`, so the listing is pinned once per handle. The extraction
  preflight (`list_files_for_limits_with_mmap_cap`) shares the Arc
  instead of deep-cloning; only the public owned-`Vec` convenience
  `list_files_for_limits()` still clones, for its return contract.)*
- **Behavioural change.** Code that explicitly *wants* to observe
  external rewrites by re-listing the same `Archive` instance must now
  drop and reopen the handle. The previous behaviour was un-documented
  and inconsistent across backends, so we don't expect existing
  callers to depend on it; this change is recorded as a v0.3 minor
  observable difference rather than a deprecation.
- **Mutation in `commit_changes`.** The modify path opens its own
  source `Archive`, runs `list_files()` once, then validates against
  it. The cached listing matches the source-read path's expectations
  exactly, so this reuses rather than fights the cache.

## Verification

- `LibarchiveArchive::list_files_metadata_only` exercised by the existing
  TAR / TAR.GZ / TAR.BZ2 / TAR.XZ / ISO listing tests; second call within
  one `Archive` instance returns a structurally-equal clone.
- New regression test
  (`tests/integration/backend_caching_baseline.rs`) modifies the
  archive file on disk between two listings on the same `Archive`
  handle and asserts both calls return the cached snapshot, not the
  fresh-from-disk view.
- `Archive`'s rustdoc updated to drop the per-backend caveat list and
  state the uniform "frozen at first observation" contract.

## Alternatives considered

- **Option B — codify lazy reopen.** Rejected because the asymmetry
  (libarchive vs everyone else) leaks into every test that expects
  consistent behaviour, and because the post-D2 typed-handle work
  benefits from `ReadArchive::list_files()` being a pure function of
  the handle.

## Related

- AD 0053 (Group D architectural pass — D1 backend trait, D2 typed
  handles)
- AD 0054 (Piz / Zip handle caching)
- OI-0075-002 (this decision closes its caching-baseline gap)
- `src/ffi/libarchive_wrapper.rs::LibarchiveArchive`
- `src/ffi/piz_wrapper.rs::PizArchive`

## Amendment (2026-07-17, R0080-0003)

The Piz description above ("the mapping is stable for the lifetime of the
`Archive`") over-promised. A memoised `Mmap` is a **live view of the
inode**, not an immutable copy: if a non-cooperating process truncates
the archive, later access to mapped pages past the new EOF raises SIGBUS
(process crash), and an in-place same-length overwrite silently changes
bytes underneath the cached listing. Read paths take no advisory lock
(AD 0009 locks modify mode only), so this is exposed to any external
writer.

What this decision actually guarantees is the **listing** frozen at first
observation — not immutability of the mapped bytes. The struct-level
rustdoc has been corrected to say "live mapping; external truncation may
fault the process". Hardening options (open-time identity + length
revalidation, owned snapshots, or a fault-tolerant read wrapper) are
tracked as OI-0080-002; declining them permanently would warrant a
further amendment recording the accepted risk.

## Amendment (2026-07-23, R4 collapse — live-mmap portion removed with piz)

Per the AD 0007 collapse amendment (owner-approved; paired DCR-009), the `piz` backend is deleted.
The **live-mmap** half of this decision — the memoised `Mmap` that made the frozen *listing* sit on
top of a live, mutation-exposed *inode view*, and the SIGBUS / silent-overwrite hazard that came
with it — is therefore gone. OI-0080-002 is consequently RESOLVED (2026-07-23): the sole ZIP backend
reads over `std::fs::File`, so external truncation surfaces as an ordinary I/O error rather than a
`SIGBUS`, and there is no shared mapped view to be silently rewritten.

The frozen-listing contract this record baselined is unchanged and now concerns only the
reopen/handle-caching backends:
- **ZIP (sole `zip`-crate backend)** — caches the open `RawZipArchive<File>` and memoises the parsed
  listing (`OnceCell`); frozen at first observation for the handle lifetime.
- **SevenZ** — parses the table-of-contents once and reuses it.
- **UnRAR** — keeps its FFI handle alive between calls.
- **Libarchive** — memoises the entry list on first `list_files_metadata_only()`.

The "what this guarantees is the listing frozen at first observation, not immutability of bytes"
statement still holds for every backend above; the specifically-mmap caveat no longer applies to any
backend because none memory-maps. This does not rewrite the original Decision Outcome.
