# Implementation Impact Report

- **Report ID:** IIR-001
- **Date:** 2026-04-13
- **Related DCR(s):** none (this is the initial impact report for the retroactive baseline)
- **Prepared by:** Documentation reconciliation pass

## Baseline comparison

- **Previous synced baseline ID:** (none — initial retroactive baseline)
- **New active baseline ID:** BL-001-retroactive
- **Previous stub manifest version:** (none)
- **New stub manifest version:** 1

## Context

The project was implemented before the design-first-architecture skill was applied. All design artifacts (Phases 0–5) were created retroactively to describe the as-built system. As a result, `phase-state.yaml` records:

- `design.status: approved_with_acknowledged_drift`
- `implementation.sync_status: synced_with_documentation_drift`
- `implementation.status: complete_with_gaps`

This report enumerates the gaps between the approved baseline and the current implementation/documentation so that future work has a structured ledger to close against.

## Slice impact

### Blocked slices

None. The implementation is complete for all in-scope MVP scenarios. No slice is blocked by this report.

### Allowed parallel slices

All slices are allowed in parallel — the implementation is complete. Reconciliation work listed below can proceed independently per area without cross-slice risk.

## Known implementation gaps (tracked as DEFERRED in stub manifest)

These gaps are explicitly out of MVP scope. They are architecturally present but not functionally complete. See `docs/project/stub-manifest.md` for manifest rows and `reviews/Open_Issues.md` for issue references.

| DEF ID | Gap | Open Issue | Scenario | Status |
|---|---|---|---|---|
| DEF-001 | `Archive::open_at_offset` returns `NotImplemented` | — | SCN-SFX-* open flow | **Closed 2026-04-18** (tempfile-backed implementation shipped; 16 GiB ceiling per AD 0040; `open_sfx()` delegates here) |
| DEF-002 | `CompressionOptions::split_size` not honored | — | SCN-CRE-* split creation | Open |
| DEF-003 | Passwords stored in `Option<String>`; no SecStr | OI-0057-003 | All password scenarios | **Closed 2026-04-17** (password fields migrated to `Option<SecStr>`) |
| DEF-004 | Non-libarchive backends buffer then wrap in `Cursor` | — | SCN-EXT-* streaming | Open |
| DEF-005 | ZIP `commit_changes()` edge cases | — | SCN-MOD-* | Open (some ZIP-specific scenarios remain; OI-025-001/OI-025-002 resolved 2026-04-14) |
| DEF-006 | `CompressionOptions::progress` not consumed | OI-025-003 | SCN-CRE-04 | **Closed 2026-04-13** (DCR-001 Phase B.1, AD 0021) |
| DEF-007 | `StubType::Unknown` scanning deferred | OI-027-001 | SCN-SFX-08 | **Closed 2026-04-13** (DCR-001 Phase A.1) |
| DEF-008 | `ModificationOptions` fields not consumed | OI-025-003 | SCN-MOD-* | **Closed 2026-04-14** (DCR-001 Phases B.2 + C): all fields honored including `preserve_metadata` and `compression` |

Additional acknowledged implementation concerns:

- ~~**Concurrent RAR test flakiness**: UnRAR global state can cause concurrent test failures.~~ **Resolved 2026-04-13** (DCR-001 Phase A.2, AD 0019): `UNRAR_LOCK` process-wide mutex serializes all UnRAR FFI calls.
- **Standalone compression formats** (per AD 0018): `ArchiveFormat::Gzip`, `Bzip2`, `Xz` enum variants exist. Standalone compressed files (.gz/.bz2/.xz) are now supported via libarchive `format_raw` (OI-026-003 resolved); TAR compound formats continue to work natively.

## Known documentation gaps

These items are tracked independently of the implementation gaps and represent drift between written specifications/docs and the as-built code. They do not block implementation but should be reconciled in a future documentation pass.

| Area | Gap | Location | Status |
|---|---|---|---|
| Contract coverage | `calculate_archive_crc()`, `calculate_manifest_digest()`, and `calculate_manifest_summary()` public APIs are not referenced by any contract document. | `specs/001-unified-archive/contracts/inspection.md` | **Closed 2026-04-13** — "Archive-level integrity" section added with algorithm, determinism guarantees, and method-selection table. Cross-referenced from `docs/architecture/rough-schema.md`. |
| Contract coverage | `Archive::modify_with_options(path, opts)` and `ModificationOptions` field semantics not documented. | `specs/001-unified-archive/contracts/archive.md` | **Closed 2026-04-13** (DCR-001 Phase B.2) — new contract section + `ModificationOptions` honored-fields table. |
| Contract coverage | RAR thread-safety wording reflected the pre-mutex behavior ("caller must serialize"). | `specs/001-unified-archive/contracts/archive.md` | **Closed 2026-04-13** (DCR-001 Phase A.2) — wording updated to reflect process-wide `UNRAR_LOCK`. |
| Contract drift | Contract files have drifted from the implemented API in several places; reconciled partially during Review 041/042 review rounds but not exhaustively audited. | `specs/001-unified-archive/contracts/*.md` | Open |
| Architecture docs | Multiple architecture docs (scenario-matrix, verification-matrix, walkthroughs) contained stale status and assignments; partially reconciled during Reviews 041/042. | `docs/architecture/*.md` | Open |
| Baseline checklist | `design-baseline.md` "Handoff Readiness Checklist" still has the "phase-state synchronized" box unchecked pending full doc reconciliation. | `docs/project/design-baseline.md` | **Closed 2026-04-13** — checkbox ticked after archive-level integrity contract gap closed. |
| Decision records | New architectural decisions taken during in-flight remediation (UNRAR mutex, additive `modify_with_options`, per-entry creation progress) lacked ADRs. | `docs/architecture/decisions/` | **Closed 2026-04-13** (DCR-001) — ADRs 0019, 0020, 0021 added; index updated. |
| Project state | `phase-state.yaml`, `status.md`, and IIR-001 listed Phase A/B gaps as open after they had been resolved in code. | `docs/project/` | **Closed 2026-04-13** (DCR-001) — phase-state and status refreshed; IIR-001 closures recorded inline. |

## Required rewind

- **Earliest affected implementation phase:** None. No rewind required; implementation is complete.
- **Verification to rerun:** None required by this report. Existing test suite (`cargo test`, 867+ tests) passes against current code; partial/uncovered scenario status is already documented in `docs/architecture/verification-matrix.md`.
- **Docs to resync:** Items listed under "Known documentation gaps" above. These are tracked as ongoing reconciliation work, not as blocking rewinds.

## Immediate actions

| Item | Owner | Status |
|---|---|---|
| Close out scenario/contract drift for `calculate_archive_crc()` / `calculate_manifest_digest()` / `calculate_manifest_summary()` by adding them to `inspection.md` | Documentation maintainer | **Closed 2026-04-13** |
| Complete handoff checklist item "phase-state synchronized to this baseline" once contract drift is reconciled | Documentation maintainer | **Closed 2026-04-13** |
| Resolve OI-027-001 (unknown-stub SFX scanning) | Implementation slice owner | **Closed 2026-04-13** (DCR-001 Phase A.1) |
| Resolve RAR concurrent-call stabilization | Implementation slice owner | **Closed 2026-04-13** (DCR-001 Phase A.2, AD 0019) |
| Resolve OI-025-003 (`CompressionOptions.progress` + `ModificationOptions` dead API) | Implementation slice owner | **Closed 2026-04-13** (DCR-001 Phases B.1 + B.2, ADs 0020 + 0021) |
| Resolve OI-025-001 / OI-025-002 (modification metadata/settings preservation) | Implementation slice owner | **Closed 2026-04-14** (DCR-001 Phase C): `commit_changes()` preserves timestamps, permissions, and honors compression overrides |
| Decide whether to promote any DEF entry into a subsequent MVP scope (requires new design change record if so) | Project lead | Open |

## Notes

- This report is the initial impact report for the retroactive baseline. Subsequent reports should be filed when design changes are made (new DCRs), when implementation slices are rewound, or when new drift is discovered that affects the synced baseline version.
- When a future design change record is opened, bump `phase-state.yaml` → `implementation.active_impact_report` to point at the new report ID.
