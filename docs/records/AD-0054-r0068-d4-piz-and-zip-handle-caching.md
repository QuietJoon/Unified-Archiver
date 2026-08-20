---
type: ADR
title: "AD 0054: Cache Piz mmap + ZIP `RawZipArchive` handle (D4 first cut)"
description: "Implemented for Piz and ZIP."
tags: [decision, ADR-0054, ADR-0053, R0068-0032, R0068-0033]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0054: Cache Piz mmap + ZIP `RawZipArchive` handle (D4 first cut)

Status: Amended 2026-07-23 (R4 collapse) — the Piz mmap cache is gone with the piz backend; only
the ZIP `RawZipArchive<File>` handle cache remains. See the amendment at the end.

## Context and Problem Statement

Found in Review 0068 (R0068-0032 Piz remap-per-op, R0068-0033 ZIP
reopen-per-op). AD 0053 baselined the design for D4 per-backend
caching. This record lands the first two backends (Piz, ZIP) and
explains why 7z, UnRAR, and libarchive are deferred to follow-up
records rather than swept in here.

Locations:
- `src/ffi/piz_wrapper.rs::PizArchive`
- `src/ffi/zip_wrapper.rs::ZipArchive`

Before this change every read-side operation
(`list_files`, `extract_*`, `test_integrity`) on those backends paid:

- A fresh `File::open` syscall.
- For Piz: `Mmap::map`.
- For ZIP: `zip::ZipArchive::new` (parses the central directory).

For an `Archive` handle servicing many calls (`list_files` →
`extract_files` → `validate_integrity` → …) the per-call cost is
linear in the number of operations. A real workload of "open → list →
extract 50 files" pays 51× the open+parse overhead.

## Decision Drivers

* **Visible perf win.** The most common usage shape on long-lived
  `Archive` handles is many sequential read calls. Memoising the
  primitive each backend builds at "first use" eliminates 50 of those
  51 redundant builds.
* **Thread-safety vs `Archive` is `!Sync`.** `Archive` is `Send` but
  not `Sync`, so two threads cannot share `&Archive`. The mutex /
  `OnceCell` hidden inside the wrapper is therefore uncontended in
  any well-formed caller. The synchronisation cost is just the
  interior-mutability tax to satisfy `RawZipArchive`'s `&mut self`
  methods under our `&self` API.
* **Stateful vs stateless backends.** Piz's `Mmap` and ZIP's
  `RawZipArchive<File>` are both effectively stateless across calls
  (mmap is a passive view; `zip` re-seeks for each `by_index_raw` /
  `by_name_decrypt`). Caching them is straightforward.
  `sevenz-rust2`'s `ArchiveReader` and libarchive's read handle are
  iterator-shaped — re-using them between calls means trusting that
  upstream's seek-back semantics work, which we cannot verify in this
  cycle. UnRAR's FFI handle is *definitely* one-shot. Those three
  remain on AD 0053's per-backend follow-up list.
* **Bypass for caller-supplied caps.** Piz's
  `extract_all_with_options` accepts an explicit
  `ExtractionLimits::max_mmap_size` that may be tighter than the env
  default. The cached mmap was created under the env-default cap, so
  honouring a per-call tighter cap means bypassing the cache. The
  patch keeps `open_mmap` (cached) and `open_mmap_with_limit`
  (always-fresh) as separate helpers.

## Considered Options

1. **Cache only the entry list (not the underlying reader / mmap).**
   The Archive layer already does this via `entry_cache: OnceCell<Vec<ArchiveEntry>>`,
   so this would be a no-op duplication.
2. **Cache the parsed primitive (mmap / `RawZipArchive<File>`) at the
   wrapper level under interior mutability** — what this ADR lands.
3. **Skip caching pending the D2 god-object split** — defers a
   user-visible perf win for no concrete benefit beyond "smaller
   diffs later". Rejected.

## Decision Outcome

ACCEPT: option 2.

Status: Implemented for Piz and ZIP. 7z, UnRAR, libarchive remain
forward work per AD 0053; each lands as its own ADR with the
upstream-rewind-semantics check captured.

### Implementation

`src/ffi/piz_wrapper.rs`:

- New field `cached_mmap: OnceCell<Mmap>` on `PizArchive`.
- `open_mmap` returns `Result<&Mmap>` and is memoised by `OnceCell`.
- `open_mmap_with_limit` keeps its owned-`Mmap` return and bypasses
  the cache (the per-call cap could differ from the cached mmap's
  cap).
- All five `piz::ZipArchive::new(&mapping)` call sites work
  unchanged: `&Mmap` already implements `AsRef<[u8]>` via the blanket
  ref impl.

`src/ffi/zip_wrapper.rs`:

- New field `cached_zip: std::sync::Mutex<Option<RawZipArchive<File>>>`
  on `ZipArchive`.
- New method `with_zip<R>(&self, f: impl FnOnce(&mut RawZipArchive<File>) -> Result<R>) -> Result<R>`
  that lazily initialises the inner `Option` on first call and
  passes a `&mut` to the closure. Mutex poisoning is recovered via
  `PoisonError::into_inner` because no invariant outlives a panicked
  closure.
- Five existing methods (`list_files`, `extract_to_memory`,
  `extract_all_with_options`, `extract_file_with_options`,
  `test_integrity`) restructured to wrap their bodies in
  `self.with_zip(|zip| { … })` instead of opening a fresh
  `RawZipArchive` each call.
- The free `open_zip` helper stays for the cache-population path
  inside `with_zip`; no other caller invokes it directly any more.

Tests already exercise the full method matrix (creation_roundtrip,
extraction_test, modification_test, password_handling_test,
integrity_check_test) — the diff is therefore self-validating: a
state-leak between calls would surface immediately in the existing
suite. `cargo test --test review_0068_test --all-features` continues
to pass 21/21 after this change.

## Consequences

* Good, because long-lived `Archive` handles servicing many read
  operations now amortise the open + central-directory parse / mmap
  setup over the operation count instead of paying it per call.
* Good, because the `&dyn ReadBackend` view (D1) is unchanged — the
  caching is entirely an inside-the-wrapper concern and does not
  leak into the trait surface.
* Bad, because `ZipArchive` now holds a `File` for the lifetime of
  the wrapper. Callers that opened many archives expecting prompt
  fd-release on drop will see the fd freed at `Archive::close` /
  `Drop` instead of at first-call boundary as before. No behaviour
  change for typical use; documented in the wrapper's struct
  rustdoc.
* Bad, because the four remaining backends (7z, UnRAR, libarchive,
  and the `is_solid` / metadata helpers on 7z) still pay
  per-operation parse cost. Each will get its own ADR + landing once
  the upstream rewind semantics are validated.

## Amendment (2026-07-23, R4 collapse — Piz mmap cache removed)

Per the AD 0007 collapse amendment (owner-approved; paired DCR-009), the `piz` backend and its
`src/ffi/piz_wrapper.rs::PizArchive` are deleted, so **the Piz mmap cache described above no longer
exists**. Half of this record's "first cut" — the memoised `Mmap` and its env-default-cap bypass
(`ExtractionLimits::max_mmap_size` / `get_max_mmap_size`, all removed in the same collapse) — is
retired.

What remains from this record is the **ZIP `RawZipArchive<File>` handle cache** in
`src/ffi/zip_wrapper.rs::ZipArchive`, which is now the sole ZIP backend for both encrypted and
unencrypted archives. Its cache contract is unchanged: the first read-side call opens the file and
parses the central directory under a `Mutex`, later calls reuse it, and the parsed listing is
additionally memoised in a `OnceCell`. The "holds a `File` for the wrapper lifetime; fd released at
`Drop`/`close`" consequence above still applies and is now the only handle-caching consequence for
ZIP. The 7z / UnRAR / libarchive follow-up items are unaffected. This does not rewrite the original
Decision Outcome; it records that one of the two landed backends was removed.
