---
type: ADR
title: "AD 0061: Review 0071 closure — narrow correctness pass, all 21 findings landed inline"
description: "Implemented inline."
tags: [decision, ADR-0061, ADR-0053, ADR-0058, ADR-0059, ADR-0060, R0071-0001, R0071-0002, R0071-0003, R0071-0004, R0071-0005, R0071-0006, R0071-0007]
timestamp: 2026-04-26T00:00:00Z
status: active
---

# AD 0061: Review 0071 closure — narrow correctness pass, all 21 findings landed inline

## Context and Problem Statement

Found in Review 0071 ("Narrow modular/correctness pass against
v0.2.0"). The reviewer worked from static inspection plus
`cargo clippy --all-targets --no-deps` and `cargo test --no-run`,
deliberately filtering out duplicates of R0069 / R0070 findings and
items already tracked in `docs/project/open-issues.md`. The result was
21 fresh defects spanning correctness, cancellation, error labelling,
test parity, and release-doc drift — all narrowly scoped, none
re-litigating the AD 0053 / AD 0058 architectural surface.

**High-severity correctness:**

- SFX/offset password reopens (`Archive::extract_*_with_options`) re-target the
  outer executable instead of the staged payload, because
  `archive.path` is restored to the caller-facing outer path while
  `_backing_tempfile` holds the actual archive bytes (R0071-0001).
- Modify-mode compression override silently changes container format
  if the caller passes `CompressionOptions::new(other_format)`
  (R0071-0002).
- Memory/stream extraction skip the archive-level ratio guard, so
  CRC-less compressed formats (TAR.GZ / TAR.BZ2 / TAR.XZ) bypass
  zip-bomb detection on single-entry reads (R0071-0003).
- Libarchive `extract_to_memory` keeps the unknown-size branch
  uncapped, so a malicious decoder grows the buffer without bound
  (R0071-0004).

**Medium-severity policy:**

- Selective extraction (`extract_some` / `extract_files` /
  `extract_by_ids`) materialises the destination before the safety /
  overwrite gates run (R0071-0005).
- `add_file_from_path` / `add_file_from_path_as` start an entry
  before checking `metadata.is_file()`, so passing a directory or
  special file poisons the writer mid-entry (R0071-0006).
- Creation-side progress callbacks fire only after the whole entry
  has been copied — `ControlFlow::Break` cannot cancel a large
  in-flight entry (R0071-0007).
- Libarchive `add_directory_entry` increments `entries_written` but
  never calls `notify_progress`, breaking cross-backend parity with
  `ZipWriter::add_directory_entry` (R0071-0008).
- Memory/stream `extract_*_with_options` use the cached `list_files`
  for the preflight listing instead of
  `list_files_for_limits_with_mmap_cap`, dropping caller-supplied
  `max_mmap_size` for Piz preflight (R0071-0009).

**Low-severity error / API hygiene:**

- `check_single_entry_safe` hard-codes `ops::EXTRACT` for the
  size-limit branch instead of the caller's op label (R0071-0010).
- Libarchive `LibarchiveArchive::create` runs `archive_write_new` /
  format setup / compression options before the password-rejection
  policy check, leaking allocations and contradicting AD 0059
  closure ordering (R0071-0011).
- `has_recovery_record` / `recovery_percentage` / `is_solid` silently
  answer `false` / `None` for ZipWriter handles instead of rejecting
  write-mode (R0071-0012).
- `entry_count` masks impossible Write-mode/non-writer-backend
  combinations as zero (R0071-0013).
- `detect_multipart` walks the destination directory for an
  in-progress write handle (R0071-0014).

**Documentation drift:**

- `detect_format_from_locked` rustdoc claims a 32 KiB read but the
  function reads 512 bytes (matching the main detector) (R0071-0015).
- API_REFERENCE.md / walkthroughs.md / `Archive::detect_sfx` rustdoc
  still describe Stage 3 as "archive validation" (R0071-0016).
- README, USER_MANUAL, GETTING_STARTED, Limitations, STREAM_CRC32,
  and several source rustdoc/comment sites still pin the public
  release at v0.1.1 (R0071-0017).
- Active docs and source comments still claim parallel/rayon
  extraction even though the rayon-based path was retired before
  v0.2.0 (R0071-0018).
- `docs/architecture/architecture_investigation_blocks.json` is a
  stale generated cache that conflicts with the current architecture
  document (R0071-0019).
- `docs/architecture/mvp-scope.md` lists "split archive creation
  deferred to v0.2.0" and "backend trait abstraction deferred"
  even though v0.2.0 has shipped without splits and `src/backend.rs`
  now provides the trait abstraction (R0071-0020).

**Backend gap:**

- `ModificationOptions.preserve_metadata` documentation does not
  mention that retained directory entries are re-emitted with
  backend-default permissions and the rewrite's wall-clock time
  (R0071-0021).

## Decision Drivers

* **Correctness vs ceremony.** All 21 findings are concrete,
  contained, and cleanly fixable in one session. Producing per-issue
  ADRs would create 21 records for one review pass — the AD 0060
  closure precedent says "bulk-accept and fix what fits, document
  partial fixes inline."
* **Avoiding the larger D2 surface.** R0071-0001 (SFX password reopen)
  and R0071-0009 (mmap cap on memory/stream) overlap with OI-0069-002
  / OI-0058-001 / AD 0053 D2; the inline fix here keeps the public
  contract intact without forcing the god-object split.
* **Preserving the AD 0060 partial-fix caveats.** R0071-0005
  (selective extraction destination ordering) inherits the same
  canonicalize-needs-existing-dest tension that AD 0060 documented
  for `extract_all` / `extract_file`. The fix here orders
  `ensure_destination` between the in-memory zip-bomb gate and the
  canonicalise-requiring overwrite gate so the bomb path leaves no
  empty directory behind. The fully-deferred mkdir is still tracked
  with R0070-0028.

## Considered Options

1. **Fix everything inline; document partial-fix caveats where the
   complete fix needs a larger refactor.** — adopted.
2. **Defer R0071-0007 (mid-entry progress) as too invasive.**
   Rejected — the closure cadence change is mechanical (`with_writer`
   + `write_entry` → progress notifier param), and the user routed
   "do best effort" / "max effort" / auto mode for this pass.
3. **Reject the review on width grounds.** Rejected — every finding
   is a concrete defect or live doc drift.

## Decision Outcome

ACCEPT: option 1.

Status: Implemented inline. No items routed to OI-0058-001 /
OI-0069-001 / OI-0069-002 (the cross-references in those OIs cover
the higher-severity surfaces R0071 already excluded).

### Implementation summary

**SFX password reopen (R0071-0001).**

- `Archive::source_path_for_reopen` added on the `Archive` impl
  (`pub(crate)`) — returns the staged payload tempfile path when
  `_backing_tempfile.is_some()`, else the caller-facing path. All
  seven `reopen_with_password_if_set` call sites in `src/extraction.rs`
  route through it instead of `&self.path`. The
  `reopen_with_password_if_set` parameter renamed `self_path` →
  `source_path` and the rustdoc points at the new helper.
- For SFX archives without a password, `archive.path` continues to
  be the outer executable (the OI-0069-002 / R0070-0011 SFX
  denominator surface stays explicitly unchanged here).

**Modify-mode compression override format guard (R0071-0002).**

- `commit_changes` rejects an override whose `CompressionOptions.format`
  disagrees with the archive's detected format with a structured
  `OperationBlocked` error. The `ModificationOptions.compression`
  rustdoc spells out that the override controls only level / password
  / split / progress.

**Memory / stream archive-level ratio guard (R0071-0003).**

- `security::check_single_entry_safe_with_archive` added: same
  contract as `check_single_entry_safe` plus an archive-level ratio
  fallback that fires when per-entry `compressed_size` is `None`
  (CRC-less compressed formats). Re-exported from `lib.rs`.
- All four `extract_to_memory` / `extract_to_stream` paths
  (default-limits + with-options) call the new helper with
  `archive.source_path_for_reopen()` so SFX-staged archives use the
  staged size as the denominator.

**Libarchive unknown-size memory cap (R0071-0004).**

- `LibarchiveArchive::extract_to_memory` enforces a 4 GiB cap when
  the entry size is unknown / zero. Declared-size mismatch still
  surfaces as `Corruption`; unknown-size overflow surfaces as
  `OperationBlocked`. The previous "the decoder will surface an
  error" comment is replaced with a hard cap.

**Selective extraction destination ordering (R0071-0005).**

- `extract_selected` runs the in-memory bomb / ratio gate first,
  then `ensure_destination`, then `check_overwrite_conflicts`.
  Caller-side `ensure_destination` calls in `extract_some`,
  `extract_files`, and `extract_by_ids` were removed. A bomb-gated
  failure now leaves no empty destination directory; an
  overwrite-conflict failure still does (matches the AD 0060 caveat
  for `extract_all` / `extract_file`; the fully-deferred mkdir is
  the remaining surface tracked alongside R0070-0028).

**Single-file creation regular-file gate (R0071-0006).**

- `Archive::add_file_from_path_as` (and through it
  `add_file_from_path`) calls `creation::validate_file_path`
  before the writer dispatch. `validate_file_path` already rejects
  symlinks via `symlink_metadata` and now also surfaces "not a
  regular file" before either ZIP/libarchive writer emits a header.
  The previous `cfg_attr(allow(dead_code))` on `validate_file_path`
  was removed because the function is now always exercised.

**Mid-entry creation progress / cancellation (R0071-0007).**

- New `ffi::common::copy_with_progress` helper: 64 KiB chunked
  copy that calls a `notify(chunk)` closure after each successful
  write so `ControlFlow::Break` cancels mid-entry.
- `ZipWriter::with_writer` reshaped to pass a notify closure
  (`&mut dyn FnMut(u64) -> Result<()>`) into the per-add closure.
  The six `add_*` closures call `notify` once for `Bytes` /
  directory entries and per-chunk via `copy_with_progress` for
  `Reader` / `Path` sources. The post-call `self.notify_progress`
  was removed; `with_writer` only increments `entries_written`.
- `LibarchiveArchive::write_entry` constructs the same notify
  closure and passes it into `write_entry_inner`. Inside the inner
  function, `Bytes` notifies once with `data.len()` (even for
  empty bodies), `Stream` notifies per chunk, and an explicit
  `notify(0)` fires on empty streams so per-entry callbacks always
  reach the caller. `#[allow(clippy::too_many_arguments)]` on the
  inner function (8 params is justified — splitting them would
  obscure the FFI call boundary).

**Libarchive directory progress parity (R0071-0008).**

- `LibarchiveArchive::add_directory_entry` calls
  `self.notify_progress(0)` after `entries_written += 1` so
  cross-backend creation progress matches the ZipWriter behaviour.

**Memory / stream mmap cap during listing (R0071-0009).**

- `extract_to_memory_with_options` and
  `extract_to_stream_with_options` route the preflight listing
  through `archive.list_files_for_limits_with_mmap_cap(options.limits.max_mmap_size)`
  instead of `archive.list_files()`, so caller-supplied caps gate
  the Piz metadata mapping during preflight as well as during
  extraction dispatch.

**Op-label propagation (R0071-0010).**

- `check_single_entry_safe`'s size-limit branch now uses the
  caller-supplied `op` label instead of `ops::EXTRACT`.

**Libarchive encrypted-creation rejection ordering (R0071-0011).**

- `LibarchiveArchive::create` rejects `options.password.is_some()`
  before any libarchive allocation / format setup / compression
  option setup runs. Matches the ZIP backend's ordering.

**Capability-query mode gating (R0071-0012, R0071-0013, R0071-0014).**

- `Archive::has_recovery_record`, `recovery_percentage`, and
  `is_solid` reject Write-mode handles up front with
  `write_mode_only`, instead of silently answering `false` / `None`
  for `ZipWriter`.
- `entry_count` returns a structured `OperationBlocked` for
  Write-mode archives whose backend isn't `ZipWriter` /
  `Libarchive` (an internal invariant bug, surfaced rather than
  hidden).
- `detect_multipart` returns `write_mode_only` for write-mode
  handles before scanning the destination directory.

**Doc-drift cleanup (R0071-0015..0020).**

- `detect_format_from_locked` rustdoc rewritten: 512 bytes (matches
  `ArchiveFormat::detect`), with an ISO PVD note explaining why
  the offset-32769 prefix is out of scope for the locked path.
- `Archive::detect_sfx` rustdoc, `docs/API_REFERENCE.md`, and
  `docs/architecture/walkthroughs.md` updated to call Stage 3
  "heuristic offset screening" with explicit pointers to
  `Archive::open_sfx` for full validation.
- README, USER_MANUAL, GETTING_STARTED, Limitations,
  STREAM_CRC32, `src/lib.rs`, `src/extraction.rs`,
  `src/inspection.rs` swept from `v0.1.1` / `0.1.1` to
  `v0.2.0` / `0.2.0`.
- `src/ffi/mod.rs`, `src/ffi/piz_wrapper.rs`,
  `docs/GETTING_STARTED.md`, `docs/project/intake.md`,
  `docs/project/implementation-slice-checklists.md`, and
  `tests/batch_extraction_test.rs` rewrote the parallel/rayon
  claims to "sequential extraction (rayon path retired before
  v0.2.0)". The renamed test
  (`test_extract_files_parallel_extraction` →
  `test_extract_files_batch`) keeps its coverage shape but loses
  the misleading name.
- `docs/architecture/architecture_investigation_blocks.json`
  removed — it was a 26 KB single-line generated cache nothing
  consumes and its content conflicted with the current
  architecture document.
- `docs/architecture/mvp-scope.md` updated: split archive creation
  retains "deferred" but drops the v0.2.0 target; backend trait
  abstraction marked landed in v0.2.0 with a pointer to AD 0053.

**Modify-mode `preserve_metadata` directory documentation
(R0071-0021).**

- `ModificationOptions.preserve_metadata` rustdoc now explicitly
  states the flag applies to regular file entries only; retained
  directories are re-emitted with backend-default permissions and
  the rewrite's wall-clock time. Implementation of directory
  metadata preservation is tracked alongside OI-0065-002.

### Tracked-to-existing-OI

None. R0071 explicitly excluded findings already cataloged by
OI-0058 / OI-0065 / OI-0069 / R0069 / R0070, so no Group-A append
was required this pass.

### Items not landed inline

None. Every accepted finding has a fix in this commit; no partial
state requires source-level documentation beyond the inline notes
already cited. The only tracked follow-up is R0071-0005's
fully-deferred mkdir — that surface was already documented under
R0070-0028 and the inline ordering here is the same partial-fix
shape AD 0060 chose.

## Consequences

* Good, because every routed finding is fixed inline, no new OI
  entries were added, and the inline doc notes give future
  reviewers the rationale for partial-fix shapes.
* Good, because the SFX password reopen surface now distinguishes
  "outer-facing path for diagnostics" (`Archive::path()`) from
  "actual archive bytes for reopens" (`source_path_for_reopen`).
  Future SFX work (denominator unification with R0070-0011, true
  SFX-aware open) can build on the same accessor.
* Good, because creation progress / cancellation now match the
  documented per-chunk contract — a `ControlFlow::Break` cancels
  large entries in flight rather than minutes after.
* Good, because the docs sweep removes the most visible release
  drift (v0.1.1 references, parallel/rayon claims, stale block
  cache, mvp-scope roadmap), keeping live planning docs honest
  about what shipped in v0.2.0.
* Bad, because the selective-extraction overwrite gate still
  requires an existing destination, so an overwrite conflict
  leaves an empty output directory. The fully-deferred mkdir is
  the remaining surface and is tracked under R0070-0028.
* Bad, because the libarchive unknown-size memory cap is a static
  4 GiB ceiling rather than a caller-tunable budget. Threading
  `ExtractionLimits.max_file_size` through every backend's
  `extract_to_memory` is a separate refactor.
