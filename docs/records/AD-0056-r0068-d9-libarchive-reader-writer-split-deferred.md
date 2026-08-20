---
type: ADR
title: "AD 0056: Defer the LibarchiveArchive read/write struct split (D9 deferred)"
description: "Deferred."
tags: [decision, ADR-0056, ADR-0053, ADR-0051, R0068-0057]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0056: Defer the LibarchiveArchive read/write struct split (D9 deferred)

## Context and Problem Statement

Found in Review 0068 (R0068-0057). AD 0053 baselined the design for
splitting `LibarchiveArchive` into `LibarchiveReader` and
`LibarchiveWriter` so the writer-only fields (`write_handle`,
`progress`, `bytes_written`, `entries_written`, `stream_buffer`) live
on a separate struct from the read-only path-only state.

The split is genuinely valuable:

- `LibarchiveArchive` today has fields meaningless in one mode (the
  five write fields above when used for `Archive::open`; `path` is
  the only read-mode-relevant field).
- The `ReadBackend` trait (D1, AD 0051) is already implemented on
  `LibarchiveArchive` — a separate `LibarchiveReader` type would
  make the trait surface match the struct surface exactly.

But landing the split in this session would be a force-fit:

1. The struct ownership change ripples through every dispatch site
   in `archive.rs`, `extraction.rs`, `inspection.rs`,
   `creation.rs`, `modification.rs`, and `backend.rs`. The
   `ArchiveBackend::Libarchive(Box<LibarchiveArchive>)` variant
   would need to become two variants
   (`LibarchiveReader` / `LibarchiveWriter`) or a sum type, and
   every match arm that opens, lists, or modifies a libarchive-backed
   archive needs to pick the right one.
2. The modify-mode entry path uses `LibarchiveArchive` for the source
   read AND drives a fresh writer at commit time. The split must
   carefully separate those two flows without changing observable
   behavior.
3. D2 (Archive god-object split) introduces mode-specific backend
   enums in any case. Doing D9 *before* D2 means re-shaping the
   libarchive variant twice — once now, once when the backend enums
   split into `ReadArchiveBackend` / `WriteArchiveBackend`.

## Decision Drivers

* **Sequencing.** D9 is most cleanly done after D2. Doing it first
  imposes churn on the backend enum that D2 immediately rewrites.
* **Risk/benefit.** The current `LibarchiveArchive` works correctly;
  the split is purely a code-cleanliness concern (R0068-0057 was
  Medium, not Critical). Deferring it does not regress any external
  contract.
* **D1's trait surface is already in place.** `ReadBackend` is
  implemented on `LibarchiveArchive`. After D2's enum split, that
  impl moves to `LibarchiveReader` mechanically; nothing about D1
  blocks D9.

## Considered Options

1. **Land the full struct split now.** Touch ~6 files, replace
   `Libarchive` enum variants, migrate every dispatch arm. Gets the
   field-level cleanup but has to be re-touched when D2 lands.
2. **Land a newtype-wrapper first cut now**
   (`LibarchiveReader(LibarchiveArchive)` /
   `LibarchiveWriter(LibarchiveArchive)`). Surfaces the
   read-vs-write distinction at the type level today, defers the
   actual field split. Adds boilerplate that has to be removed in
   the real-split commit.
3. **Defer D9 entirely until D2 lands.** Documents the design
   contract here so future implementers don't re-derive it; lets
   D2 reshape the backend enum once and D9 fold into that shape
   cleanly.

## Decision Outcome

ACCEPT: option 3.

D9's design baseline lives in AD 0053; this ADR records the
sequencing decision (D9 ← D2) and the rationale for not landing the
newtype wrapper as a stepping stone.

Status: Deferred. Will land as its own ADR after D2 (`v2-api`
feature-flag mode-split landing).

### Implementation steps for the future D9 landing

1. Confirm D2's `ReadArchiveBackend` / `WriteArchiveBackend`
   enums are in place (otherwise the libarchive variant has nowhere
   to go).
2. Define `pub(crate) struct LibarchiveReader { path: String, /*
   D4 validated-handle memo if landed by then */ }` in
   `src/ffi/libarchive_wrapper.rs` (or a future
   `src/ffi/libarchive/reader.rs` per D10's split blueprint).
3. Define `pub(crate) struct LibarchiveWriter { path: String,
   write_handle: Option<*mut Archive>, progress, bytes_written,
   entries_written, stream_buffer }`.
4. Move the existing `LibarchiveArchive` reader-side methods to
   `LibarchiveReader`; writer-side to `LibarchiveWriter`.
5. Move the `impl ReadBackend for LibarchiveArchive` block (in
   `src/backend.rs`) to `impl ReadBackend for LibarchiveReader`.
6. Delete `LibarchiveArchive`. Update all dispatch sites to use
   the post-D2 mode-specific variants.

## Consequences

* Good, because deferring avoids churn-and-rewrite when D2 lands.
* Good, because the design contract is captured here for future
  implementers.
* Good, because no external behavior change in the meantime.
* Bad, because `LibarchiveArchive` keeps fields meaningless in the
  read-mode use until D9 lands.
* Bad, because the `ReadBackend` impl on `LibarchiveArchive` is
  technically callable on a write-mode handle today (the methods
  themselves are read-only and would simply re-open the file, which
  works — but the type system does not enforce mode).

## Amendment (2026-07-20, owner decision — deferral reversed)

The deferral above is reversed. The sequencing premise that justified
option 3 — "D9 is most cleanly done after D2, because D2 reshapes the
backend enum into `ReadArchiveBackend` / `WriteArchiveBackend` and D9
should fold into that shape" — was **falsified by how D2 actually
shipped**. D2 landed as *additive typed handles* layered over the
**unchanged** `ArchiveBackend` enum (the `Libarchive(Box<LibarchiveArchive>)`
variant was never split, and no `ReadArchiveBackend` /
`WriteArchiveBackend` pair was introduced). There is therefore no
pending backend-enum reshape for D9 to wait on, and the "touch it now,
re-touch it when D2 lands" churn argument no longer applies.

Meanwhile `src/ffi/libarchive_wrapper.rs` had grown to ~3775 LOC with
the read and write code paths fully interleaved in a single ~2314-line
`impl LibarchiveArchive` block, which is the concrete
code-cleanliness cost this ADR was tracking. The split was executed
now.

### What actually landed

A **module-level read/write split** (not the two-struct field split
sketched in "Implementation steps for the future D9 landing" above —
that field-level separation remains available as later work). The
single `LibarchiveArchive` struct is retained; only the code was
partitioned into file-as-module children:

- **`src/ffi/libarchive_wrapper.rs` (module root):** the
  `LibarchiveArchive` struct + `unsafe impl Send`, the local
  `archive_entry_size_is_set` FFI declaration, helpers shared by both
  sides or referenced by the test module (`get_archive_error`,
  `systemtime_to_signed`, `ReadHandleGuard`,
  `is_libarchive_checksum_failure` + `CHECKSUM_FAILURE_MARKERS`), the
  `mod reader; mod writer;` declarations, and the `#[cfg(test)]` tests.
- **`src/ffi/libarchive_wrapper/reader.rs`:** the read/extract/integrity
  `impl LibarchiveArchive` methods (`open`, listing, `extract_*`,
  `test_integrity`, the streaming entry points), the
  `LibarchiveStreamReader` type and its `Read`/`Drop` impls, and the
  read-only free helpers (`open_read_handle`, `copy_data`,
  `parse_entry`, `extract_flags`, `checked_data_skip`,
  `classify_libarchive_error`, staging/drift/raw-name helpers, etc.).
- **`src/ffi/libarchive_wrapper/writer.rs`:** the create/write
  `impl LibarchiveArchive` methods (`create`, the `add_*` family,
  `close_write`, `write_entry` / `write_entry_inner`,
  `add_directory_recursive`) and the write-only helpers
  (`EntryDataSource`, `EntryGuard`).

This is a **pure refactor**: code was moved and module paths /
visibility adjusted (`open_read_handle` widened to `pub(super)` so the
root test module can still reach it; `super::common::*` references in
moved code rewritten to `crate::ffi::common::*`). No signatures, error
text, control flow, or observable behavior changed. `cargo check`,
`clippy -D warnings`, and the full test suite pass unchanged.

The original **Decision Outcome** above is left intact as the record
of the state at 2026-04-25; this amendment records only the later
reversal and the layout that shipped.
