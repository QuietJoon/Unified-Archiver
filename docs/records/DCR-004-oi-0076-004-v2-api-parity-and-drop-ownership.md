---
type: DCR
title: "DCR-004: v2-api typed-handle full parity + unified Drop ownership"
description: "The v2-api typed handles (ReadArchive / WriteArchive /"
tags: [change, DCR-004, R0076-0085, R0076-0088, OI-0076-004, OI-0075-003]
timestamp: 2026-06-06T00:00:00Z
status: active
---

# DCR-004: v2-api typed-handle full parity + unified Drop ownership

- **Date:** 2026-06-06
- **Source:** OI-0076-004 (Review 0076 — R0076-0085 / 0086 / 0087 / 0088)
- **Affected ADRs:** [AD-0053-r0068-group-d-architectural-pass-design-baseline.md](AD-0053-r0068-group-d-architectural-pass-design-baseline.md) (updated — D2 addendum)

## What Changed

The `v2-api` typed handles (`ReadArchive` / `WriteArchive` /
`ModifyArchive`) shipped as a *curated subset* of the legacy `Archive`
surface. Two design facts changed:

1. **Full mode-appropriate parity.** Adopting the typed API no longer
   forces a fall-back to `Archive` for any supported workflow.
   - `ReadArchive` gained the complete inspection set
     (`entry_count`, `find_entry`, `find_entries`,
     `list_files_for_limits`, `calculate_archive_crc`,
     `calculate_content_multiset_digest_and_size`,
     `calculate_manifest_summary`, `detect_multipart`,
     `check_symlinks`, `is_solid`, `has_recovery_record`,
     `recovery_percentage`, `extension_format`), the complete
     extraction set (`extract_file`, `extract_files`, `extract_by_ids`,
     `extract_some`, `extract_filtered`, `extract_to_memory_with_options`,
     `extract_to_stream_with_options[_unbounded]`), the
     `open_with_sfx_progress` constructor, and the static
     `extract_stub` helper.
   - `WriteArchive` gained `add_directory_recursive` and the
     write-progress `entry_count`.
   - `ModifyArchive` gained `add_entry_from_reader`, `replace_entry`,
     `replace_entry_from_path`, `replace_entry_from_reader`, and source
     inspection (`list_files`, `entry_count`, `find_entry`,
     `find_entries`). The inspection methods are load-bearing: the
     pre-existing `remove_entry_by_id` was unusable without a way to
     discover entry ids.

2. **Single finalization owner (R0076-0088).** Previously both
   `WriteArchive::Drop` (warning) and the inner `Archive::Drop`
   (silent best-effort finalize) acted on the durability boundary, and
   the `WriteArchive` rustdoc claimed it "leaves a partially-written
   archive" while the inner handle silently completed it. Now
   `WriteArchive::Drop` is the sole owner: it calls the new
   `pub(crate) Archive::finalize_write_on_drop`, which runs **one**
   best-effort finalize and marks the inner handle `finalized` so
   `Archive::Drop` neither finalizes nor warns again.
   `WriteArchive::finish(self)` is documented as the durable,
   *error-surfacing* commit path; drop-without-finish is best-effort
   and swallows errors behind a single warning.

## Why

The OI's Required Action 1 demanded the parity be defined explicitly
(reach it or declare partial). We reached it.

The Drop contract was driven by a backend asymmetry discovered while
implementing: `ZipWriter` finalizes in its **own** `Drop`, but
`LibarchiveArchive` has **no** `Drop` — its raw `write_handle` is freed
only by the finalize path (`close_write` → `archive_write_close` +
`archive_write_free`). A literal "leave a partial archive, never
finalize on drop" reading would therefore have **leaked the libarchive
write handle** and still not produced uniform behavior (ZIP completes
regardless). Best-effort-finalize-with-single-owner is the leak-free,
backend-consistent contract; it matches the option the OI's Required
Action 3 explicitly sanctioned ("sole owner … by marking the inner
archive, OR document the legacy delegation").

## Affected Areas

- `src/archive/mode_split.rs` — new delegating methods on all three
  handles; rewritten `WriteArchive` doc + `Drop`; 5 new tests.
- `src/archive.rs` — new `pub(crate) Archive::finalize_write_on_drop`
  (v2-api-gated).
- `docs/API_REFERENCE.md` — Typed Handle API section gained the parity
  matrix + durability-ownership note (replacing the dangling
  "tracked under OI-0076-004" pointer).

## Migration / Follow-up

- No public API removal or signature change; all additions are behind
  the off-by-default `v2-api` feature. Behaviour change is confined to
  the typed `WriteArchive::Drop` path (single warning; no double
  finalize) — the legacy `Archive` facade Drop is unchanged.
- OI-0075-003 (Drop / finish error semantics) is now consistent with
  this: `finish()` surfaces errors, drop is best-effort.
- The legacy `Archive` facade's own best-effort `Drop`-finalize remains
  for v0.3 source-compat and is slated for removal with the facade in
  v0.4.
