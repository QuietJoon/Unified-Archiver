# Design Change Record

- **DCR ID:** DCR-001
- **Date:** 2026-04-13
- **Title:** In-flight gap remediation (OI-025-003 progress wiring, OI-026-004 RAR concurrency, OI-027-001 unknown-stub SFX, OI-025-003 ModificationOptions wiring)
- **Status:** approved

## Trigger

- **User request:** "Resolve In-flight gaps at first, and Documentation drift" (2026-04-13).
- **Evidence:** `phase-state.yaml` recorded `implementation.status: complete_with_gaps` against BL-001-retroactive with five tracked open issues (OI-025-001/002/003, OI-026-004, OI-027-001) and one documentation-drift item (missing contracts for `calculate_archive_crc`/`calculate_manifest_digest`).

## Change summary

Closed the documentation-drift item and four of the five in-flight gap categories through the staged remediation plan recorded at `/Users/mac/.claude/plans/cuddly-wibbling-gray.md`:

- **Phase A.1 — OI-027-001:** Unknown-stub SFX scanning. `StubType::detect()` now returns `Ok(StubType::Unknown)` for unrecognized executables; `detect_sfx()` continues to the signature scan instead of bailing at Stage 1.
- **Phase A.2 — OI-026-004:** RAR concurrent-call serialization. Added a process-wide `Mutex<()>` (`UNRAR_LOCK`) in `src/ffi/wrapper.rs`, scoped per `unsafe` block to avoid re-entrant deadlocks. Validated by `tests/integration/concurrency.rs::test_concurrent_rar_open_and_list` (8 threads × 20 iterations).
- **Phase A.3 — Documentation drift:** Added "Archive-level integrity" section to `specs/001-unified-archive/contracts/inspection.md` covering `calculate_archive_crc`, `calculate_manifest_digest`, `calculate_manifest_summary`. Cross-referenced from `docs/architecture/rough-schema.md`. Ticked the previously-open "phase-state synchronized" item in `design-baseline.md`.
- **Phase B.1 — OI-025-003 (creation progress):** Wired `CompressionOptions.progress` into both ZIP and libarchive creation backends. Per-entry granularity (matching extraction). `ControlFlow::Break` surfaces as `ArchiveError::format(_, "Creation cancelled by user")`. Verified by `tests/creation_progress_test.rs`.
- **Phase B.2 — OI-025-003 (modification options):** Added `Archive::modify_with_options(path, opts)` as an additive public method; existing `Archive::modify(path)` unchanged. `commit_changes()` now honors `ModificationOptions::create_backup` + `backup_suffix` (writes a sidecar copy before atomic rename). Verified by `tests/modification_options_test.rs` (4/4 passing).

OI-025-001 (compression-setting preservation) and OI-025-002 (per-entry metadata preservation) were resolved on 2026-04-14: `ModificationOptions.compression` overrides archive recreation settings; `commit_changes()` preserves timestamps and permissions via `add_file_from_data_with_metadata()`. Phase C is now complete.

## Classification

- **Type:** Correction (in-flight gap remediation against an approved retroactive baseline) + contract correction + additive API extension.
- **Earliest affected design phase:** 2 (Ownership, Persistence, Files, and Contracts) — the additive `modify_with_options` API and the `inspection.md` integrity section update.
- **Earliest affected implementation phase:** Existing implementation (Phase 6 in `phase-state.yaml`) — non-breaking changes only.
- **ADRs required:** Yes — three new ADRs:
  - `0019-unrar-process-wide-mutex.md`
  - `0020-additive-modification-options-extension.md`
  - `0021-per-entry-creation-progress.md`

## Impacted artifacts

| File / Document | Why impacted |
|---|---|
| `src/sfx/stub_types.rs`, `src/sfx/detection.rs` | Phase A.1 — unknown-stub continuation |
| `src/ffi/wrapper.rs` | Phase A.2 — `UNRAR_LOCK` mutex |
| `src/creation.rs`, `src/ffi/zip_writer.rs`, `src/ffi/libarchive_wrapper.rs` | Phase B.1 — progress callback wiring |
| `src/archive.rs`, `src/modification.rs` | Phase B.2 — `modify_with_options`, backup-on-commit |
| `tests/creation_progress_test.rs`, `tests/modification_options_test.rs` | New regression suites |
| `tests/integration/concurrency.rs` | New `test_concurrent_rar_open_and_list` |
| `specs/001-unified-archive/contracts/inspection.md` | Phase A.3 — archive-level integrity section |
| `specs/001-unified-archive/contracts/archive.md` | Phase B.2 — document `modify_with_options` and `ModificationOptions` |
| `docs/architecture/rough-schema.md` | Cross-reference to integrity contract |
| `docs/architecture/verification-matrix.md` | SCN-CRE-04, SCN-SFX-08, RAR concurrency rows updated |
| `docs/architecture/decisions/0019-…/0020-…/0021-…` | New ADRs |
| `docs/architecture/decisions/README.md` | Index entries for ADRs 0019–0021 |
| `docs/project/stub-manifest.md` | DEF-006 → Closed; DEF-007 → Closed; DEF-008 → Closed (2026-04-14) |
| `docs/project/implementation-impact-report.md` | Drift rows closed; in-flight items moved to "Closed" |
| `docs/project/design-baseline.md` | Phase-state checkbox ticked |
| `docs/project/phase-state.yaml` | `implementation.notes` reflects closed gaps |
| `docs/project/status.md` | Refreshed to current state |
| `reviews/Open_Issues.md` | OI-026-004 and OI-027-001 → RESOLVED; OI-025-003 → RESOLVED (2026-04-13) for wiring portion; OI-025-001/OI-025-002 → RESOLVED (2026-04-14) — `preserve_metadata` preserves timestamps and Unix permissions and `compression` overrides recreation settings. |

## Impacted slices

- **Blocked slices:** None.
- **Allowed parallel slices:** All. Implementation work continues against the approved baseline; this DCR records correction-only changes that are individually mergeable and non-breaking for downstream callers.
- **Re-sync required?** No. Existing implementation phase advances against the same baseline; phase-state remains BL-001-retroactive with reduced-gap notes.

## Required actions

- [x] Update `phase-state.yaml` (close gaps in `implementation.notes`; record DCR-001 in `design.open_change_records` while open, then clear on closure)
- [x] Update `status.md` to reflect resolved gaps
- [x] Regenerate downstream docs that referenced the closed gaps (verification-matrix, stub-manifest, IIR-001)
- [x] Create ADRs 0019–0021
- [x] Update `implementation-impact-report.md` (Closed rows for the addressed items; new IIR-002 if Phase C lands)

## Resolution

- **Decision:** Approved. All phases (A, B, C) of the remediation plan are merged.
- **Approval:** User-approved on 2026-04-13 (plan cuddly-wibbling-gray).
- **Closed date:** 2026-04-13 (Phases A + B); 2026-04-14 (Phase C — OI-025-001/002 resolved).
