---
type: Open Issues
title: "Open Issues — Resolved Archive"
description: "Audit-trail archive of resolved OI entries, retired from the active open-issues ledger."
tags: [project-control]
timestamp: 2026-08-04T00:00:00Z
status: active
---

# Open Issues — Resolved Archive

Resolved entries moved from `Open_Issues.md` on 2026-04-16 during ledger cleanup.
Retained for audit trail. Do not add new entries here — resolved entries are no longer relocated
out of [`open-issues.md`](open-issues.md); they stay there and carry a `RESOLVED` status line.

> **Relocated 2026-08-04** from the retired `docs/records/legacy/Open_Issues_Resolved.md`
> (originally `reviews/Open_Issues_Resolved.md`). Resolved-issue history is project
> tracking, not a decision record, so it now sits beside the active ledger
> [`open-issues.md`](open-issues.md). Entry bodies are unchanged.

---

## OI-0022-002: ZipWriter creation drops file metadata

- **Source:** R0022-0002 (Review 0022)
- **Date:** 2026-03-17
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** `add_file_from_path` now reads source file metadata and sets `last_modified_time` + `unix_permissions` on per-file options (src/ffi/zip_writer.rs). New helper `system_time_to_zip_datetime` in src/ffi/common.rs.

---

## OI-0024-002: Incorrect block skipping in parse_rar5_recovery

- **Source:** R0024-0002 (Review 0024)
- **Date:** 2026-03-17
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** RAR5: absolute seeking via `stream_position()`, skip = CRC(4) + offset1 + header_size + data_area_size. RAR4: reads ADD_SIZE when LONG_BLOCK (0x8000) flag set.

---

## OI-0024-004: test_integrity OOM in SevenZ backend

- **Source:** R0024-0004 (Review 0024)
- **Date:** 2026-03-17
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** SevenZ `test_integrity` already uses streaming (8KB buffer via `for_each_entries`), not `extract_to_memory`. Fixed misleading doc comment.

---

## OI-0024-006: Fragile magic numbers in recovery record parsing

- **Source:** R0024-0006 (Review 0024)
- **Date:** 2026-03-17
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** RAR5 overhead now computed from actual vint sizes instead of hardcoded 7. RAR4 keeps the constant 7 (correct for fixed header format).

---

## OI-0010-001: Link entries not classified on native backends

- **Source:** R0010-0001 (Review 0010), R0025-0001–005 (Review 0025)
- **Date:** 2026-01-09 (updated 2026-04-10)
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** All four native backends now classify link entries. Extraction paths skip link entries on all backends.

---

## OI-0010-003: extract_all ignores archive_write_finish_entry errors

- **Source:** R0010-0003 (Review 0010)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** All `archive_write_finish_entry` calls now checked.

---

## OI-0011-001: Piz backend 4GB limit on 64-bit systems

- **Source:** R0011-0001 (Review 0011)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Configurable via `ExtractionLimits::max_mmap_size` and `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` env var.

---

## OI-0011-002: Libarchive walkdir error swallowing

- **Source:** R0011-0002 (Review 0011)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Walkdir errors mapped to `ArchiveError::io()` and propagated.

---

## OI-0013-001: Non-mac UnRAR filename conversion compile error

- **Source:** R0013-0001 (Review 0013)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Platform-specific UTF handling via `#[cfg]` blocks.

---

## OI-0013-002: SevenZ encryption flag tied to password presence

- **Source:** R0013-0002 (Review 0013)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** `is_encrypted` now derived from block-level coders (AES256-SHA256 detection).

---

## OI-0012-001: Memory-inefficient file addition in libarchive backend

- **Source:** R0012-0001 (Review 0012)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Streaming buffer loop replaces `read_to_end` in `add_file_from_path`.

---

## OI-0012-002: SFX detection flaws

- **Source:** R0012-0002 (Review 0012)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Robust reading via `read_to_end` + `take`; validation uses file metadata length.

---

## OI-0012-004: Timestamp precision loss in libarchive creation

- **Source:** R0012-0004 (Review 0012)
- **Date:** 2026-01-09
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** All three `archive_entry_set_mtime` call sites now pass `subsec_nanos()`.

---

## OI-0025-001: commit_changes() loses compression settings and password

- **Source:** R0025-0013 (Review 0025)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** `ModificationOptions` now carries `compression: Option<CompressionOptions>`. `commit_changes()` uses it.

---

## OI-0025-002: commit_changes() strips retained-entry metadata

- **Source:** R0025-0014 (Review 0025)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** `commit_changes()` now uses `add_file_from_data_with_metadata()` on both backends, preserving timestamps and Unix permissions.

---

## OI-0025-003: CompressionOptions.progress and ModificationOptions are dead API

- **Source:** R0025-0019, R0025-0022 (Review 0025)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Creation progress wired (Phase B.1), modification options wired (Phase B.2). See DCR-001.

---

## OI-0026-001: Archive creation overwrite flag

- **Source:** R0026-0001 (Review 0026)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-10)
- **Resolution:** `create()` now rejects existing files with `AlreadyExists` error.

---

## OI-0026-002: Zip-bomb ratio checks disabled for libarchive formats

- **Source:** R0026-0002 (Review 0026)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Added `check_archive_ratio()` using archive file size on disk. Called at all extraction entry points.

---

## OI-0026-003: Standalone gz/bz2/xz format support

- **Source:** R0026-0006, R0026-0007, R0026-0008 (Review 0026)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Added `archive_read_support_format_raw` FFI binding. Standalone compressed files exposed as single-entry archives.

---

## OI-0026-005: entry_count() in Write mode

- **Source:** R0026-0011 (Review 0026)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Added `entries_written` counter. `Archive::entry_count()` returns write counter in Write mode.

---

## OI-0026-006: Unknown-stub SFX test and multi-backend concurrency tests

- **Source:** R0026-0029, R0026-0030 (Review 0026)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Both test categories already exist from prior resolutions.

---

## OI-0027-001: Unknown-stub SFX scanning behavior

- **Source:** R0027-0001 (Review 0027)
- **Date:** 2026-04-10
- **Status:** RESOLVED (2026-04-13)
- **Resolution:** `StubType::detect()` returns `StubType::Unknown` for unrecognized executables; `detect_sfx()` proceeds to Stage 2 scanning regardless.

---

## OI-0045-001: RAR extraction progress pre-scan returned zero total

- **Source:** R0045-0001 (Review 0045)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** `total_bytes` computed from fresh handle. See MADR-0002.

---

## OI-0045-003: libarchive list_files decompresses every payload

- **Source:** R0045-0003 (Review 0045)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-14)
- **Resolution:** Libarchive arm switched to `list_files_metadata_only()`. See MADR-0001.

---

## OI-0049-003: ArchiveEntry lacks link target metadata

- **Source:** R0049-0003 (Review 0049)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** Added `link_target: Option<String>` to `ArchiveEntry`. Populated from libarchive and UnRAR backends.

---

## OI-0049-004: `modify()` silently resets to format-default compression

- **Source:** R0049-0004 (Review 0049)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** `modify()` now snapshots compression settings via `snapshot_compression()`.

---

## OI-0049-005: Modify mode buffers entire retained entries in memory

- **Source:** R0049-0005 (Review 0049)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** Streaming CoW: `extract_to_stream()` → chunked relay (64 KB) → `add_file_from_reader_with_metadata()`.

---

## OI-0050-001: Make `ffi` module pub(crate) to enforce facade pattern

- **Source:** R0050-0001 (Review 0050)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** Changed `pub mod ffi` to `pub(crate) mod ffi`. Migrated test to public API.

---

## OI-0050-002: Remove dead `ExtractionOptions.filter` field and `EntryFilter` type

- **Source:** R0050-0002 (Review 0050)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** Removed `filter`, `EntryFilter` from options and public exports.

---

## OI-0050-006: Manifest digest: add `have_metadata_digest()` and compute real CRC32

- **Source:** R0050-0006 (Review 0050)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-15)
- **Resolution:** Added `have_metadata_digest()`, CRC32 computed from content for CRC-less entries. See AD 0006, AD 0008.
  > Correction (2026-08-07, indy-review-prune): `have_metadata_digest()` was never added — the
  > symbol has zero occurrences in any commit's source. The content-CRC32 half is real but shipped
  > later via AD 0047 (`entry_crc32_for_digest`); the `{path}:{size}` fallback was still live when
  > Review 0064 found it. "AD 0006, AD 0008" are the pre-renumber gate records, today's MADR-0006
  > and MADR-0008 — both superseded by AD 0047 on 2026-08-07, whose amendments carry the evidence.

---

## OI-0050-014: Extraction API redesign — extract_some as core selective primitive

- **Source:** R0050-0002, R0050-0007–009, R0050-0014 (Review 0050); R0004-0001–003 (Review 0004)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** `extract_some()` implemented. Three existing methods rewritten as thin wrappers. See AD 0029.

---

## OI-0001-001: Modify-mode backend routing asymmetry

- **Source:** R0001-0001 (Review 0001)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Documented in contracts/archive.md and architecture walkthroughs.

---

## OI-0002-001: BZIP2 stream CRC bit-level scanning

- **Source:** R0002-0026 (Review 0002)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Implemented bit-level EOS marker scanning. See MADR-0017.

---

## OI-0002-002: modify() triple-open optimization

- **Source:** R0002-0009 (Review 0002)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Eliminated redundant `Archive::open()` encryption preflight. See MADR-0016.

---

## OI-0003-001: Clippy lint baseline and dead code strategy

- **Source:** R0003-0001 and others (Review 0003)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Fixed all 45 clippy errors. See MADR-0018.

---

## OI-0003-002: Raw format test verification and entry name normalization bug

- **Source:** R0003-0007 (Review 0003)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Un-ignored 10 raw format tests. Fixed normalization bug. See MADR-0019.

---

## OI-0050-026: Wire SecStr through all password-bearing public API surfaces

- **Source:** R0050-0026 (Review 0050)
- **Date:** 2026-04-14
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Changed both option struct password fields to `Option<SecStr>`, added `password_as_str()` helper, updated all call sites. Breaking API change — callers construct `SecStr` instead of `String`.

---

## OI-0001-002: ArchiveFormat capability API restructuring

- **Source:** R0001-0007 (Review 0001)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-16)
- **Resolution:** Added `Support` enum, `FormatCapabilities` struct, and `capabilities()` method; boolean methods now delegate. Callers can distinguish read-time from write-time support per operation.

---

## OI-0001-003: UnsupportedOperation error variant split

- **Source:** R0001-0003 (Review 0001)
- **Date:** 2026-04-16
- **Status:** RESOLVED (2026-04-17)
- **Resolution:** Commit `c9b6685` split `ArchiveError` into `Unsupported`, `NotImplemented`, `OperationBlocked`, `ReadOnlyBackend`, `WriteModeOnly` variants and migrated all call sites. Callers can programmatically distinguish feature deferral from mode mismatch.

---

## OI-0057-001: ZIP creation encryption is silently unsupported (AD 0007 regression)

- **Source:** R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093 (Review 0057)
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** Superseded by MADR-0027 (`docs/records/MADR-0027-reject-encrypted-archive-creation.md`). `src/ffi/zip_writer.rs::create` rejects `CompressionOptions.password.is_some()` with `operation_blocked`; `src/ffi/libarchive_wrapper.rs` rejects non-ZIP creation passwords with the same variant. Rustdoc capability table now marks ZIP Encryption as `📖§` (read-only) with footnote citing MADR-0027.

---

## OI-0057-003: Internal password storage still uses `Option<String>` in backends

- **Source:** R0057-0031, R0057-0080, R0057-0115 (Review 0057)
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** All four backend structs now store `password: Option<SecStr>` (`src/ffi/sevenz_wrapper.rs`, `src/ffi/zip_wrapper.rs`, `src/ffi/wrapper.rs`, `src/external/rar.rs`). Single `.unsecure()` conversion point lives in `src/options.rs::password_as_str`, which every backend call site routes through. Zeroization-on-drop holds through the FFI boundary.

---

## OI-0057-004: Path sanitization creates directories in a validator (partially mitigated)

- **Source:** R0057-0009, R0057-0010, R0057-0011 (Review 0057)
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** `src/security.rs::sanitize_entry_path` no longer calls `fs::create_dir_all`; it walks to the deepest existing ancestor and canonicalizes that for the zip-slip check. All five extraction backends (zip, sevenz, piz, UnRAR, libarchive) independently call `create_dir_all(parent)` in their per-entry prologue before writing the file. Full extraction suite passes with no regression.

---

## OI-0057-006: Non-atomic file writes in `ffi/common.rs`

- **Source:** R0057-0026, R0057-0027 (Review 0057)
  > Source ids corrected 2026-08-07 (`/indy-review-cleanup`): the line above previously read `R0057-0042, R0057-0043`, which resolves to the UnRAR `make`-dependency and rebuild-invalidation build findings. The ids below are the findings this entry actually tracks, read from Review 0057 before it was moved to the cold store.
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** `AtomicOutputFile` in `src/ffi/common.rs` wraps `tempfile::NamedTempFile` and commits via `persist` / `persist_noclobber`. `copy_with_optional_crc` validates CRC *after* the full stream is written and *before* the caller commits the temp file, so failed extractions drop the temp on `Drop` and never leave a `.partial` in place of the target. All extraction backends use the `AtomicOutputFile::create → copy_with_optional_crc → commit` sequence.

---

## OI-0057-002: RAR-support feature flag is not honored by build or runtime

- **Source:** R0057-0001, R0057-0002, R0057-0003 (Review 0057)
  > Source ids corrected 2026-08-07 (`/indy-review-cleanup`): the line above previously read `R0057-0010, R0057-0014`, which resolves to the `sanitize_entry_path` filesystem-mutation finding and the `modify()` redundant-open finding — neither is about the feature flag. The ids below are the findings this entry actually tracks, read from Review 0057 before it was moved to the cold store.
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** `cargo build --no-default-features` and `cargo test --no-default-features` now both pass.

  > **Superseded 2026-09-08.** AD-0058 Stage 1 made every backend feature-gated, so a bare
  > `--no-default-features` no longer compiles at all — `src/lib.rs` raises a `compile_error!`
  > when no backend feature is selected. The equivalent RAR-free configuration today is
  > `--no-default-features --features read,integrity,create,zip-read,zip-write,zip-crypto,sevenzip,sfx`,
  > and the minimal floor is `--features read,zip-read`. The resolution above stands for the
  > defect it closed; only the command has changed. Library code paths were already properly gated (`ArchiveBackend::Unrar` imports, match arms in `src/archive.rs`, `src/creation.rs`, `src/extraction.rs`, `src/inspection.rs`) and the build script already guarded `build_unrar()`. The remaining work was gating 108 RAR-fixture-dependent test functions across 23 test files with `#[cfg(feature = "rar-support")]` and gating three library-level tests (`test_open_valid_rar`, `test_is_encrypted_encrypted_rar`, `test_list_files_rar`). Runtime error for RAR formats with feature off surfaces as `Unsupported { operation: "open", format: Rar/Rar5, details: "RAR/RAR5 support is disabled in this build (enable the \`rar-support\` Cargo feature to include UnRAR)" }`.

---

## OI-0057-009: Review 0057 documentation/test-staleness sweep

- **Source:** Multiple (R0057-0045 through R0057-0121 misc.) (Review 0057)
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** Empirical sweep against 2026-04-18 HEAD found zero `Option<String>`-as-password references in production code. All capability tables in `README.md`, `src/lib.rs`, and `docs/API_REFERENCE.md` accurately show RAR creation as unsupported (`❌` or `🪟*` with feature caveat). `examples/create_archive.rs::example_encrypted_creation_rejected` correctly demonstrates MADR-0027 rejection behavior. `cargo test --doc` passes all 29 rustdoc examples. The 134 grep hits for `UnsupportedOperation` are concentrated in `.translate/` translation artifacts and historical spec docs — no live production code or rustdoc needs migration.

---

## OI-0057-008: Encrypted RAR fixtures likely need regeneration

- **Source:** R0057-0089, R0057-0090 (Review 0057)
- **Date:** 2026-04-17
- **Status:** RESOLVED (2026-04-18)
- **Resolution:** Regenerated `tests/fixtures/test_encrypted_data.rar` with `rar 7.20` using header encryption (`rar a -hptest123 test_encrypted_data.rar test_file.txt`) and verified via `unrar t -ptest123`. Updated `tests/fixtures/README.md` with the generation command, password, and verification step. Removed the `#[ignore]` attribute from `tests/password_handling_test.rs::test_open_encrypted_rar_with_correct_password`; the test passes three consecutive runs.

---

> Archived 2026-04-23 during `indy-review-prune`. Reason: moved from `Open_Issues.md` after closure.

## OI-0057-005: Extraction limit checks are not applied to memory/stream APIs

- **Source:** R0057-0020 (Review 0057)
  > Source ids corrected 2026-08-07 (`/indy-review-cleanup`): the line above previously read `R0057-0020, R0057-0024, R0057-0025`, which resolves to R0057-0020 correctly (`extract_to_stream` bypasses limits), but R0057-0024/0025 are the SevenZ and UnRAR whole-entry-buffering findings, which belong to the streaming-architecture thread. The ids below are the findings this entry actually tracks, read from Review 0057 before it was moved to the cold store.
- **Date:** 2026-04-17
- **Decision:** ACCEPT
- **Status:** **CLOSED 2026-04-18** (Workstream A of pre-v0.1.0 gap closure — `_with_options` overloads shipped with regression tests)

### Resolution (2026-04-18)

- `Archive::extract_to_memory_with_options(file_path, options)` added at `src/extraction.rs`.
- `Archive::extract_to_stream_with_options(file_path, options)` added at `src/extraction.rs`.
- Both reuse `check_single_entry_safe` with the caller-supplied `ExtractionLimits` and delegate to the existing `_unchecked` dispatchers.
- Regression tests cover accept/reject at tight vs. loose `max_per_file_size` caps for both entry points.
- Aggregate-across-entries tracker remains a v0.2 consideration (not blocking v0.1).

### Related

- FR-021 / FR-022 (resource exhaustion defenses)
- `src/extraction.rs::extract_to_memory_with_options` / `extract_to_stream_with_options`

---

> Archived 2026-04-23 during `indy-review-prune`. Reason: moved from `Open_Issues.md` after closure.

## OI-0057-007: `commit_changes` buffers every retained entry in memory

- **Source:** R0057-0030, R0057-0031 (Review 0057)
  > Source ids corrected 2026-08-07 (`/indy-review-cleanup`): the line above previously read `R0057-0040, R0057-0041`, which resolves to the Windows-build-placeholder and hard-coded-Homebrew-path findings. The ids below are the findings this entry actually tracks, read from Review 0057 before it was moved to the cold store.
- **Date:** 2026-04-17
- **Decision:** ACCEPT
- **Status:** **CLOSED 2026-04-22** (shipped against v0.1.x; SevenZ row handed off to DEF-004 / AD 0035 pending upstream crate support)

### Resolution (2026-04-22)

Retained-entry copies in `src/modification.rs::commit_changes` stream for:

- `ArchiveBackend::ZipWriter` — both `preserve_metadata=true` and `=false` (2026-04-18).
- `ArchiveBackend::Libarchive` with a known entry size — both preservation modes (2026-04-18).

The remaining fallback to `extract_to_memory_unchecked` is bounded and intentional:

- `ArchiveBackend::Piz` — mmap-backed; "memory" is a file-backed slice, not a userland buffer. Not a bounded-memory regression.
- `ArchiveBackend::Libarchive` with **unknown** entry size — a narrow `.tar.*` streaming edge case; acceptable for v0.1.x.
- `ArchiveBackend::SevenZ` — sevenz-rust2 0.19.4 exposes no owned entry-level `Read`. Blocked upstream; tracked as DEF-004 in `docs/project/stub-manifest.md` and recorded in AD 0035. Reopen when upstream gains an owned reader or when the project migrates off sevenz-rust2.

Two enhancements remain possible but are not v0.1.x blockers; raise a fresh OI if they become priorities:

- Pre-stage unknown-size libarchive entries to a tempfile and re-dispatch via the known-size streaming path.
- Multi-GiB `#[ignore]` stress test verifying bounded peak RSS during `commit_changes` on ZIP and libarchive archives.

### Related

- AD 0016 (redundant modify open)
- AD 0035 (SevenZ source streaming deferred — upstream API constraint)
- DEF-004 (`docs/project/stub-manifest.md`) — authoritative tracker for the SevenZ wait
- `src/modification.rs::commit_changes`
- `src/ffi/zip_writer.rs::add_file_from_reader`
- `src/ffi/libarchive_wrapper.rs::add_file_from_reader`

---
