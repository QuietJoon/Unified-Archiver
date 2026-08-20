---
type: DCR
title: "DCR-002: Review 0076 inline fixes — ratio guard hardening, libarchive return-code extension, public-API non-exhaustive markers"
description: "Multiple ADR-tracked decisions had small steady-state extensions applied; still active, with the 2026-08-05 §B amendment correcting item 1's inverted AD 0014 citation (the gate-side precedent is AD 0003) and recording that item 5's piz cap fix and item 1's non-finite ratio guards no longer exist."
tags: [change, DCR-002, ADR-0014, ADR-0059, ADR-0053, R0076-0001, R0076-0002, R0076-0027, R0076-0028, R0076-0043, R0076-0007, R0076-0008, R0076-0006]
timestamp: 2026-05-01T00:00:00Z
status: active
---

# DCR-002: Review 0076 inline fixes — ratio guard hardening, libarchive return-code extension, public-API non-exhaustive markers

- **Date:** 2026-05-01
- **Source:** Review 0076, Issues R0076-0001, 0002, 0007, 0008, 0027, 0028, 0043, 0076, 0077, 0078, 0079, 0080, 0081, 0084
- **Affected ADRs:** AD-0059-r0069-wide-modular-design-closure.md (extended in spirit; partial sweep continues under OI-0076-007), AD-0053-r0068-group-d-architectural-pass-design-baseline.md (typed read constructors now uniformly assert read-mode), AD-0067-r0076-closure-and-routing-record.md (new in this session)

## What Changed

Multiple ADR-tracked decisions had small steady-state extensions applied
during Review 0076's in-session fixes. None of these changes alter the
*shape* of the underlying decisions; they tighten compliance or extend
already-agreed migrations:

1. **`ExtractionLimits::check_ratio` hardening (R0076-0001 / R0076-0002).**
   The ratio guard now rejects non-finite (NaN, ±∞) and non-positive
   `max_compression_ratio` values up front, and `check_archive_ratio`
   refuses an undefined zero-compressed-with-non-zero-uncompressed
   ratio rather than waving it through. AD 0014 ("reject deferred
   password validation") established the spirit of "validate at the
   gate, don't defer"; this is the same posture applied to the ratio
   guard.

2. **Libarchive disk-writer return-code checks (R0076-0027 / R0076-0028).**
   `archive_write_disk_set_options` was the last unchecked
   `c_int`-returning libarchive call in `extract_all` and
   `extract_file_with_options`. AD 0059 noted these as the high-impact
   subset of the libarchive return-code hygiene sweep; both call sites
   are now checked. The remaining unchecked sites are
   `archive_read_data_skip` (8 sites) — tracked separately under
   OI-0076-007 and explicitly outside this DCR's scope.

3. **Libarchive timestamp `_is_set` probes (R0076-0043).**
   `parse_entry`'s `mtime`, `atime`, `ctime`, `birthtime` reads now
   gate on `archive_entry_*time_is_set` rather than `> 0`, so a
   legitimate UNIX_EPOCH timestamp survives instead of being silently
   dropped to `None`. New FFI bindings declared in
   `src/ffi/libarchive.rs`.

4. **Default `ReadBackend::extract_all` no longer reports a fake
   `ArchiveFormat::Tar` (R0076-0007).** The default impl returns
   `OperationBlocked { operation: EXTRACT_ALL, reason: "extract_all
   not implemented for this backend" }` so a future backend that
   forgets to override the trait method fails with a real diagnostic
   rather than a synthetic Tar label.

5. **`Piz::extract_to_stream_with_caps` honours `max_bytes`
   (R0076-0008).** The trait contract advertises a hard byte cap on
   the returned reader; the Piz override previously dropped it. The
   override now wraps the stream in `with_hard_cap(max_bytes)`,
   matching the trait default for cap-aware callers.

6. **`dispatch_extract_core` routes through `dispatch_read_archive`
   (R0076-0006).** The mode gate (Write-mode rejection) now lives in
   one place; the per-callsite manual mode-checking dance is gone.

7. **Public enum non-exhaustive markers
   (R0076-0076 through R0076-0081).** Added `#[non_exhaustive]` to
   `ArchiveError`, `Operation`, `ArchiveFormat`, `EntryType`, `Support`,
   and `FormatCapabilities`. Pre-1.0 stability hygiene; new variants no
   longer constitute breaking changes for downstream exhaustive
   matches. **Not** applied to `CompressionOptions` — R0075-0081
   explicitly preserved its struct-literal source-compat; tightening
   the envelope is queued behind v0.4 in OI-0076-005.

8. **Typed read constructors uniformly assert read-mode
   (R0076-0084).** `ReadArchive::open_encrypted`, `open_at_offset`,
   and `open_sfx` now match `ReadArchive::open`'s
   `debug_assert_eq!(inner.mode, ArchiveMode::Read)` so future drift
   in a legacy open helper cannot wrap a non-read backend as
   `ReadArchive`.

## Why

Each of these is a small extension of an already-decided position:
- The ratio guard hardening matches the gate-side validation posture
  AD 0014 established.
- The libarchive return-code fix continues AD 0059's sweep on the
  high-impact subset (`set_options`); the lower-impact `data_skip`
  sweep is OI-0076-007.
- The non-exhaustive markers are pre-1.0 hygiene that downstream
  callers always benefit from.
- The typed-constructor assertion uniformity matches AD 0053 D2's
  intent.

Detailed reasoning per item lives in the affected ADRs; this DCR is a
high-attention pointer to the in-session deltas.

## Affected Areas

- `src/security.rs::check_ratio`, `check_archive_ratio`
- `src/ffi/libarchive_wrapper.rs` — `extract_all` set_options check,
  `extract_file_with_options` set_options check, `parse_entry`
  timestamp `_is_set` reads
- `src/ffi/libarchive.rs` — new `archive_entry_*time_is_set` bindings
- `src/backend.rs::ReadBackend::extract_all` default impl;
  `Piz::extract_to_stream_with_caps`
- `src/extraction.rs::dispatch_extract_core`
- `src/error.rs` — `ArchiveError`, `Operation` non-exhaustive
- `src/format.rs` — `ArchiveFormat`, `Support`, `FormatCapabilities` non-exhaustive
- `src/entry.rs` — `EntryType` non-exhaustive
- `src/archive/mode_split.rs` — `ReadArchive::open_*` debug_asserts
- `Cargo.toml` — `[target.'cfg(windows)'.dependencies] libc` (R0076-0022)

## Migration / Follow-up

- Downstream callers exhaustively matching the enums above must add
  `_ => ...` arms before upgrading to this v0.3 release.
  `tests/contract/inspection_contract.rs::contract_enhanced_metadata_entry_type`
  was the only in-tree call site needing this change; updated.
- Strict `archive_read_data_skip` checking is OI-0076-007.
- `ExtractionLimits` and `ArchiveEntry` field encapsulation is
  OI-0076-005.

## Amendment (2026-08-05, decision-review-2026-07-19 §B — inverted AD 0014 citation corrected)

The review found item 1's precedent citation **inverted**. This DCR writes, of the `check_ratio`
hardening, that `AD 0014 ("reject deferred password validation") established the spirit of
"validate at the gate, don't defer"`, and the Why section restates it: "The ratio guard hardening
matches the gate-side validation posture AD 0014 established." AD 0014 decided the opposite. Its
title is "AD: Reject proposal to validate passwords at open time"; its options were "1. Accept: add
trial decryption at open time" versus "2. Reject: keep deferred validation (current design)"; its
Decision Outcome reads "REJECT: Deferred password validation is the correct design for this
library."; and it lists as a consequence that "users discover bad passwords at extraction time,
which is standard behavior for archive tools". The record therefore **preserves** deferral rather
than rejecting it — the likely source of the inversion is its slug,
`AD-0014-reject-deferred-password-validation.md`, which reads as if the *ruling* rejected deferral
instead of rejecting the proposal to end it.

The renumber trap does **not** apply: the citation points at today's **AD-0014** (the legacy
`docs/architecture/decisions/0014` record, matching this DCR's own `ADR-0014` frontmatter tag), not
at today's MADR-0014, which is "AD: SfxDetectionResult probable() confidence clamped to
[0.0, 0.99]" — an SFX-confidence record with no password content, itself superseded by I3. The
citation names the right document and mis-states it.

Only the precedent is wrong; the shipped change is unaffected. The gate-side posture item 1 actually
extends is **AD 0003** ("AD: Safety Gates Before Extraction"), whose Decision Outcome has "the
orchestrator apply sanitization, extraction limits, and overwrite conflict checks before backend
extraction". AD 0014 governs password *correctness* checking, which the facade still defers today:
`Archive::open_encrypted` (`src/archive.rs`) performs no trial decryption — it constructs the
password-capable backend and revalidates read identity — and a wrong ZIP password surfaces at entry
access, where `open_entry_by_index` (`src/ffi/zip_wrapper.rs`) maps `ZipError::InvalidPassword` to
`ArchiveError::password`.

Four recorded items have since moved or been voided by later work (grouped into three bullets below,
because items 2 and 3 moved together); none of the fixes was reverted:

- **Item 1's non-finite / non-positive `max_compression_ratio` guards are gone**, retired by the
  typed-limits redesign (I1, 2026-07-22): `CompressionRatio` (`src/security.rs`) is an exact `u64`
  rational whose constructors reject a zero numerator or denominator, so there is no `f64` left to
  be NaN or ±∞, and `ExtractionLimits::check_ratio`'s rustdoc now records that "the former
  non-finite/precision guards (R0080-0006 / R0081-0024) are gone because the typed ratio makes those
  inputs unrepresentable". The zero-compressed-with-non-zero-uncompressed rejection (R0069-0015)
  survives in `check_ratio`. See AD 0003's 2026-07-22 I1 amendment.
- **Item 5 is void.** The piz backend was removed 2026-07-23 (DCR-009, executing the AD 0007
  collapse amendment), and `ReadBackend::extract_to_memory_with_caps` /
  `extract_to_stream_with_caps` were removed with it — neither the `Piz` override this item fixed
  nor the cap-aware trait methods exist in `src/` today.
- **Items 2 and 3 moved with the libarchive reader/writer split** (AD 0056, as amended): both
  `archive_write_disk_set_options` checks and `parse_entry`'s `_is_set` timestamp probes now live in
  `src/ffi/libarchive_wrapper/reader.rs`, while the `archive_entry_*time_is_set` bindings are still
  declared in `src/ffi/libarchive.rs`. Items 4, 6, 7, and 8 are intact where recorded
  (`src/backend.rs::ReadBackend::extract_all` default impl, `src/extraction.rs::dispatch_extract_core`,
  the `#[non_exhaustive]` markers, and the `ReadArchive::open_*` read-mode asserts).

**Disposition: active.** The DCR still accurately records what shipped in Review 0076; the inverted
sentence stays as written per the immutability policy, corrected here rather than rewritten.
