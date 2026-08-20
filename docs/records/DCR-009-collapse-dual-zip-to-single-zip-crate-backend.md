---
type: DCR
title: "Dual ZIP backend collapsed to a single `zip`-crate backend (piz removed)"
description: "The piz mmap ZIP reader is removed; the `zip` crate becomes the sole ZIP backend for both encrypted and unencrypted archives. Duplicate-name rejection is re-homed into the ZIP backend and OI-0080-002 (mmap SIGBUS) is eliminated by construction."
tags: [change, project-control, DCR-009, ADR-0007, ADR-0002, ADR-0054, ADR-0065, OI-0080-002, R0079-0026]
status: active
---

# DCR-009: Collapse the dual ZIP backend to a single `zip`-crate backend

- **Date:** 2026-07-23
- **Source:** Owner-approved execution of the AD 0007 R4-benchmark recommendation (Option A)
- **Affected ADRs:** AD-0007-dual-zip-backend-strategy.md (amendment — paired), AD-0002-hybrid-backend-selection.md (amendment), AD-0054-r0068-d4-piz-and-zip-handle-caching.md (amendment), AD-0065-backend-caching-baseline-option-a.md (amendment)
- **Resolves:** OI-0080-002 (Piz mmap SIGBUS / silent-mutation)

## What Changed

The R4 benchmark (AD 0007 amendment, 2026-07-22) settled that `piz` does not win: 0.9 % faster on
DEFLATE (the common case) and 12.2 % on STORED, the latter fully recoverable without piz via
`Cursor<Mmap>` — while piz cost ≈ archive-size resident RSS and carried the OI-0080-002 mmap SIGBUS
hazard. With owner approval, the dual ZIP strategy is collapsed to a single backend:

- **`piz` removed entirely.** `src/ffi/piz_wrapper.rs` deleted; the `pub mod piz_wrapper` declaration
  removed; the `ArchiveBackend::Piz` enum variant and every match arm / dispatch branch for it
  removed (`read_backend_view`, the `ReadBackend for PizArchive` impl, the `extract_file` special
  arm, the `is_solid` / recovery-record / finalize arms, the mmap-cap listing branches).
- **`zip` crate is the sole ZIP backend for BOTH encrypted and unencrypted archives.**
  `Archive::open` on a ZIP now builds `ArchiveBackend::ZipReader` (the `zip`-crate `ZipArchive`),
  exactly as `Archive::open_encrypted` already did — the plain path is the same backend, just
  constructed without a password.
- **memmap2-for-piz machinery removed.** The `piz` and `memmap2` dependencies are dropped from
  `Cargo.toml`; the dual-ZIP decision benchmark `benches/zip_backend_read.rs` (and its `[[bench]]`
  entry) is deleted. The mmap knobs that existed only to gate piz are removed:
  `ExtractionLimits::max_mmap_size` / `platform_default_mmap_size`, `get_max_mmap_size`,
  `DEFAULT_MAX_MMAP_SIZE`, the `ExtractionPlan.max_mmap_size` field, and the
  `ReadBackend::extract_to_memory_with_caps` / `extract_to_stream_with_caps` trait methods (the
  `ValidatedSource` cap-aware methods collapse to `extract_to_memory_with_limit` /
  `extract_to_stream_with_limit`). This is public-API-breaking (allowed pre-1.0).
- **Duplicate-name rejection re-homed (R0079-0026).** The `zip` crate keys its central-directory map
  by exact name, so byte-identical duplicate names collapse to a single deduped listing entry (last
  record wins) — whereas piz kept every record and let `validate_single_entry` refuse the ambiguity.
  To preserve the rejection, `ZipArchive` gains `scan_duplicate_names`: a raw central-directory
  re-walk (from `RawZipArchive::central_directory_start()`, which folds in any SFX/prepended-data
  offset) that tallies each record's normalized name. The by-name single-entry paths
  (`extract_to_memory`, `extract_file`, and `extract_to_stream` via memory) call `reject_if_duplicate`
  right after `validate_single_entry` and refuse any normalized name with raw multiplicity > 1, using
  the shared `security::multiple_entries_reason` text ("Multiple entries match …") that
  `validate_single_entry` itself now uses.
- **0x5455 extended-timestamp surfacing re-homed.** The facade contract that
  `Archive::open(...).list_files()` surfaces `accessed` / `created` from the 0x5455 extra field
  (OI-0065-002) was provided by piz. `ZipArchive::parse_entry` now reads it from the `zip` crate's
  `extra_data_fields()` (central-directory parse), overriding the DOS `modified` and populating
  `accessed` / `created`.

## Why

One backend erases the dual-backend behavioural-divergence class (piz-vs-zip) that R0079-0026 and
OI-0065-002 had to reconcile, and — decisively — eliminates OI-0080-002 by construction: `File` reads
surface truncation as an ordinary I/O error, never a `SIGBUS`, and there is no shared mapped view to
be silently rewritten. RSS for ZIP reads drops from ≈ archive size to streaming buffers. The measured
throughput cost is < 1 % on the common case.

## Affected Areas

- src/ffi/piz_wrapper.rs (deleted), src/ffi/mod.rs (module decl removed)
- src/archive.rs (`ArchiveBackend::Piz` variant + arms removed; ZIP `open` routes to `ZipReader`; `list_files_for_limits_budgeted` de-mmap'd)
- src/backend.rs (`ReadBackend for PizArchive` impl removed; `read_backend_view` arm removed; `with_caps` trait methods + `ExtractionPlan.max_mmap_size` removed)
- src/extraction.rs, src/inspection.rs (mmap-cap plumbing removed; `list_files_for_limits_with_mmap_cap` removed)
- src/security.rs (`max_mmap_size` field/builders/`get_max_mmap_size`/`DEFAULT_MAX_MMAP_SIZE` removed; `multiple_entries_reason` extracted)
- src/ffi/zip_wrapper.rs (`scan_duplicate_names` / `reject_if_duplicate` + `duplicate_guard`; 0x5455 extended-timestamp surfacing in `parse_entry`)
- Cargo.toml (`piz`, `memmap2`, `zip_backend_read` bench removed), benches/zip_backend_read.rs (deleted)
- tests: single_entry_defense_parity.rs (single-backend, duplicate now asserted rejecting), backend_caching_baseline.rs (piz test → zip), extraction.rs (mmap test removed), large_zip_mmap_test.rs (deleted), zip_extended_timestamps.rs (module doc)
- docs: AD 0007 / AD 0002 / AD 0054 / AD 0065 amendments; OI-0080-002 marked RESOLVED

## Migration / Follow-up

Callers using `ExtractionLimits::builder().max_mmap_size(...)` / `.platform_default_mmap_size()` must
drop those calls — the methods are gone and had no effect on any backend but piz. No behavioural
change for the common ZIP read/extract path beyond the < 1 % throughput delta.

**Residual (documented, not a silent regression):** duplicate detection decodes raw
central-directory names UTF-8-lossy. Byte-identical duplicate names always tally together regardless
of encoding, so detection never misses the collapse; the residual is *attribution* of an exotic
mixed-encoding collision (a name the crate deduped by decoded value but whose raw bytes differ). That
case is caught by a raw-count-vs-deduped-count mismatch which refuses the whole by-name single-entry
surface for the archive (`DuplicateGuard::any_undetected`) — a safe over-rejection, never a silent
accept. A future opt-in `Cursor<Mmap>` fast path (AD 0007 Option C) would re-introduce the
OI-0080-002 hazard and must reopen that OI.

## Amendment (2026-08-12, Review 0001 R0001-0027/0028/0029 — the residual's mechanism changed)

**The sentence "duplicate detection decodes raw central-directory names UTF-8-lossy" no longer
describes the code.** `scan_duplicate_names` now keys attribution on the RAW central-directory name
bytes, joined to the crate's own listing through `ZipFile::name_raw()`, so a duplicate whose bytes
are not valid UTF-8 is attributed to a string `reject_if_duplicate` can actually match. That change
was necessary rather than cosmetic: two byte-identical non-UTF-8 records lossy-decode to the *same*
replacement-character key, so the lossy tally reported the collapse as fully attributed
(`attributed == collapsed`) while the key it recorded could never equal the CP437-decoded path the
listing exposes — the duplicate stayed reachable by name.

**The residual itself is narrowed, not closed, and the rest of the paragraph above still stands.**
The genuinely unattributable case is now the reverse one: records whose raw bytes DIFFER while both
decode to a single name (mixed CP437/UTF-8, or an Info-ZIP `0x7075` Unicode Path extra field
disagreeing with the base name). That case falls out as `attributed < collapsed` and still refuses
the archive's whole by-name single-entry surface — a safe over-rejection, never a silent accept.

Two further corrections from the same batch, both to statements this record's Migration section
implies rather than makes:
- `any_undetected` was `raw_records > deduped_len && names.is_empty()`, so a single *attributable*
  duplicate disarmed the global refusal entirely and an encoding-sensitive collision could hide
  behind an ordinary one. It is now duplicate-record accounting: the collapsed count is
  `raw_records - deduped_len`, the attributed count is the sum of `count - 1` over tallied names, and
  the refusal fires whenever attributed is short of collapsed.
- The scan's terminating condition was `read_exact(...).is_err() => break`, which conflated the EOCD
  boundary, a truncated record and a genuine OS I/O error — and an early break DISABLED duplicate
  detection for the archive. It now reads the four-byte signature first and returns
  `Corruption`/`Io` for anything that is not a clean central-directory or end-of-directory record.

The scope question this record left open — that the guard covers only the by-name single-entry paths,
while listing, counts, by-id and bulk extraction and integrity consume the crate's collapsed view —
is untouched by any of the above and is tracked as OI-0001-003 (ticgit `25285d67`), together with the
non-atomicity of the second `File::open`. Fixing it properly means a raw central-directory index that
every operation consults, which is also what would remove the second open.
