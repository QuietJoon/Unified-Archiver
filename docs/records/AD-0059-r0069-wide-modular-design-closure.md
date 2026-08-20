---
type: ADR
title: "AD 0059: Review 0069 closure — accept wide modular-design pass, route by group"
description: "Implemented for the inline-fix subset."
tags: [decision, ADR-0059, ADR-0053, ADR-0058, R0069-0002, R0069-0031, R0069-0033, R0069-0008, R0069-0014, R0069-0015, R0069-0018, R0069-0024, R0069-0025]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0059: Review 0069 closure — accept wide modular-design pass, route by group

## Context and Problem Statement

Found in Review 0069 ("Wide Modular Design and Consistency Pass"). The
review enumerates 88 findings across:

- Single-entry safety gates that don't reject non-regular entries.
- Per-backend extraction hygiene (entry-kind classification, mkdir
  ordering, link rejection).
- TOCTOU races on `Archive::create` and `Archive::modify`.
- libarchive return-code hygiene (format/filter setup, skip/data
  iterators, write handles, option setters).
- UnRAR concurrency (temp directory collision) and decoder edge cases
  (FILETIME pre-1970, ERAR_EOPEN narrowing, vint shift overflow).
- Modify hygiene (backup partial cleanup, suffix validation, dup-path
  normalization).
- Stream/CRC robustness (length checks, progress clamping, BZIP2 prefix).
- Format/SFX/capability cleanup.
- Doc drift (`CompressionOptions::builder`, rayon claims,
  `sanitize_entry_path` side-effects, ADR index).
- Test hygiene.

This ADR records the gate's routing decisions for the full set so
future readers see the bulk-fix shape rather than 88 individual
records.

## Decision Drivers

* The review is wide and surface-shallow — most findings are concrete,
  contained, and fix-and-forget.
* A small subset (Group A: 8 items) reshapes API contracts or
  format-detection completeness in ways that need a coordinated v0.3
  planning round.
* Another subset (Group B: ~15 items) requires non-local rework in
  modify/extraction architecture; some interlock with the post-D2
  `ReadArchive` shape from AD 0053.
* User routing during Phase 2 was: Group A → all to OI;
  Group B → fix R0069-0002/0054/0055/0060/0061, track the rest in OI;
  Group C → fix R0069-0031/0032, track R0069-0033 in OI.

## Considered Options

1. Process every finding individually with its own ADR or OI entry —
   rejected as ceremony explosion (88 records for a single review pass).
2. Bulk-accept and fix what fits in a single session, route the
   architectural items to OI under three category trackers — adopted.
3. Reject the review wholesale on grounds of "too wide for one
   pass" — rejected, the findings are concrete and worth landing.

## Decision Outcome

ACCEPT: option 2.

Status: Implemented for the inline-fix subset. Group A / Group B
tracked / Group C tracked routed to OI-0069-001 / -002 / -003.

### Implementation summary (this session)

**Security-critical entry-kind gating:**

- `security::check_single_entry_safe` now rejects symlinks, hardlinks,
  directories, and `Other` entries up-front via the FR-022
  `link_extract_blocked` helper before any backend touches the entry
  (R0069-0008).
- `security::check_archive_ratio` filters to `is_file()` only when
  summing total uncompressed size; symlinks and special entries no
  longer distort the bomb-detection denominator (R0069-0014).
- `ExtractionLimits::check_ratio` now treats zero compressed size with
  non-zero uncompressed size as a ratio violation rather than a free
  pass (R0069-0015).
- `PizArchive::parse_entry` checks the symlink Unix-mode bit *before*
  the directory fallback; the prior `is_dir = !is_file()` shortcut
  misclassified symlinks as directories (R0069-0018).

**Per-backend extraction hygiene:**

- `zip_wrapper::extract_all_with_options` and the matching paths in
  `piz_wrapper` and `sevenz_wrapper` now skip blocked entries
  (symlinks) *before* creating any parent directories (R0069-0024 /
  R0069-0025 / R0069-0026).
- `wrapper::UnrarArchive` single-file extract uses
  `link_extract_blocked` to surface symlinks/hardlinks with the same
  shape every other backend uses (R0069-0027).

**TOCTOU + concurrency:**

- `wrapper::UnrarArchive::extract_to_memory` uses `tempfile::TempDir`
  for the staging directory (R0069-0028 / R0069-0029). The legacy
  `TempDirGuard` was removed.
- `zip_writer::create` opens the destination via
  `OpenOptions::new().write(true).create_new(true)` to atomically
  reject existing files (R0069-0050).
- `Archive::modify` acquires the advisory exclusive lock *before*
  the encryption probe / backend open so concurrent writers cannot
  swap the file mid-validation (R0069-0056).

**libarchive return-code hygiene:**

- `LibarchiveArchive::open_read_handle` checks `archive_read_support_format_all`,
  `archive_read_support_format_raw`, and `archive_read_support_filter_all`
  return codes individually so setup failures surface immediately
  (R0069-0034).
- `LibarchiveArchive::add_file_from_data*` / single-file write paths
  reject `size > c_longlong::MAX` before calling
  `archive_entry_set_size` (R0069-0046).

**UnRAR semantics:**

- `wrapper::ERAR_EOPEN` now maps to `ArchiveError::Io` with `Other`
  kind instead of `NotFound` — permission/path-encoding open failures
  are no longer mislabeled (R0069-0031).
- `wrapper::decode_vint` rejects encodings whose continuation bit
  asks for shifts past u64 width or whose data bits would overflow
  (R0069-0032).
- RAR FILETIME conversion uses a new `filetime_to_system_time`
  helper that returns `None` for pre-1970 timestamps instead of
  collapsing them to `UNIX_EPOCH` (R0069-0030).

**Modify hygiene:**

- `commit_changes` removes the partial backup file on copy failure
  and on rename failure cleans up the orphaned backup (R0069-0066 /
  R0069-0067).
- `ModificationOptions::with_backup` rejects empty / separator-bearing
  suffixes by falling back to `.bak` (R0069-0068).
- Duplicate-detection paths in `commit_changes` normalise `\\` → `/`
  and trim trailing `/` so `dir`/`dir/` and `a\\b`/`a/b` compare equal
  (R0069-0060 / R0069-0061).
- `Archive::remove_entry_by_id` empty-archive error message now reads
  "no valid IDs" instead of "valid IDs: 0-0" (R0069-0069). Same fix
  applied to `Archive::extract_by_ids` via a shared `invalid_id_reason`
  helper (R0069-0013).

**ZIP single-file create + writer poisoning:**

- `ZipWriter::add_file_from_path` rejects symlinks via
  `symlink_metadata` before opening, matching the recursive-add
  policy (R0069-0054). Same alignment in
  `LibarchiveArchive::add_file_from_path` (R0069-0055).
- `ZipWriter` gained a `poisoned: bool` flag set by
  `poison_on_err`; `add_file_from_data` checks it via
  `assert_not_poisoned` so a partially-written archive can no longer
  accept further entries on top of a half-emitted one. `finish` /
  `close` still drain to a well-formed file (R0069-0053).

**Stream / CRC robustness:**

- `extract_gzip_stream_crc` does an upfront length check before
  seeking (R0069-0078).
- `extract_stream_checksum` returns a structured format error on
  short reads (R0069-0079) and requires the full `BZh` BZIP2 prefix
  before routing to the bzip parser (R0069-0080).
- `StreamingExtractor::progress` clamps to 1.0 (R0069-0081).
- `StreamingExtractor::read` uses saturating arithmetic on
  `bytes_read` (R0069-0082).
- `sfx::detection::detect_sfx` clamps the scan-buffer capacity hint
  as `u64` before casting to `usize` (R0069-0077).

**Format / capability / SFX cleanup:**

- `ArchiveFormat` gains `supports_encryption_read`,
  `supports_encryption_write`, `supports_multipart_read`, and
  `supports_multipart_write`. The legacy boolean
  `supports_encryption` / `supports_multipart` collapse remains for
  backward compatibility (R0069-0072 / R0069-0073).
- `stage_suffix_for` no longer preserves `.tar.zst` / `.tar.lz4` /
  `.tar.lzma` since those formats are not represented in
  `ArchiveFormat` (R0069-0074).
- `SfxDetectionResult::detected` is `#[cfg(test)] pub(crate)` since
  the public `detect_sfx` pipeline never produces a confirmed result
  (R0069-0076). Stage 3 documentation renamed to "heuristic
  screening" so the name no longer over-promises (R0069-0075).

**Operation enum / ad-hoc strings:**

- `Operation::Extract` variant added; `error::ops::EXTRACT` derived
  from it. `security::check_single_entry_safe` and
  `ExtractionLimits::check_ratio` migrated from `"extract".to_string()`
  literals to the typed constant (R0069-0004).

**Helper extraction:**

- `reopen_with_password_if_set` collapses the password-aware reopen
  pattern previously repeated across 8 extraction entrypoints
  (R0069-0002).

**Empty-selection no-op fixes:**

- `extract_files` and `extract_by_ids` short-circuit before
  `ensure_destination` when the requested set is empty
  (R0069-0011 / R0069-0012).

**Dispatch / typed-state:**

- `validate_entry_path` rustdoc updated to match the pure-validation
  reality (no canonicalisation, no I/O) (R0069-0016).

**Doc drift:**

- `CompressionOptions::builder` references removed from
  `docs/API_REFERENCE.md` (R0069-0083).
- Rayon-based parallel-extraction claims replaced with the actual
  sequential behavior in `docs/API_REFERENCE.md` (R0069-0084).
- `sanitize_entry_path` / `validate_entry_path` documentation
  updated to describe the current behavior — `sanitize` does not
  create directories, `validate` performs no I/O (R0069-0085).
- `docs/records/README.md` index extended through
  AD 0058 (R0069-0086).

### Tracked-to-OI

| Group | OI | Items |
|---|---|---|
| A — API shape & format detection | OI-0069-001 | R0069-0001, R0069-0010, R0069-0017, R0069-0052, R0069-0058, R0069-0059, R0069-0070, R0069-0071 |
| B — modify/extraction architecture | OI-0069-002 | R0069-0003, R0069-0005, R0069-0006, R0069-0007, R0069-0057, R0069-0062, R0069-0063, R0069-0064, R0069-0065 |
| C — RAR memory-extract path verification | OI-0069-003 | R0069-0033 |

### Items not landed inline this session

A subset of the libarchive return-code hygiene findings
(R0069-0035..0045) were tracked alongside R0069-0034 in spirit but
not all individually wired this session — the high-impact entries
(format/filter setup checks, c_longlong overflow, password-rejection
ordering) landed; the remainder
(`archive_read_data_skip` checks, write-handle RAII guards,
`close_write` error capture, etc.) carry the same pattern and can be
mechanically extended in a follow-up sweep. They remain low-blast-radius
and are not blocking any release; the gate did not produce a separate
OI for them since they all apply to the same file and follow the same
template (return-code check + structured error remap).

R0069-0087 / R0069-0088 (test header claim and unused imports) are
test-hygiene fixes that the next test pass will catch with
`-D warnings`; deferred without a tracker per the gate's "trivial
test-hygiene fixes are removed outright" policy.

## Consequences

* Good, because the security-critical entry-kind gating now lives in
  one shared `check_single_entry_safe` rather than per-backend.
* Good, because the TOCTOU race surface on `Archive::create` /
  `Archive::modify` is closed without forcing the larger D2 god-object
  split first.
* Good, because the architectural items that genuinely need
  coordinated planning have OI trackers, not piecemeal commits.
* Bad, because the libarchive return-code sweep is partial; a
  mechanical follow-up is still needed for `archive_read_data_skip`
  and write-handle RAII coverage.
* Bad, because the ZIP writer poisoning ships only on
  `add_file_from_data`; the other `add_*` methods need the same
  wrap-and-poison treatment in a follow-up.
