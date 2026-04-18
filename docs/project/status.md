# Project Status

## Active Baseline
- Design baseline ID: BL-001-retroactive
- Baseline status: approved
- Stub manifest version: 1

## Design Track
- Current phase: 5 (Design Validation and Handoff)
- Highest completed phase: 5
- Open change records: none
- Closed change records: DCR-001 (in-flight gap remediation, 2026-04-13)
- Rewind required to phase: n/a
- Summary: All design-first-architecture artifacts created retroactively. Phases 0–3 and 5 structurally complete; Phase 4 (skeleton generation) retroactively satisfied by existing implementation. ADRs 0019–0021 added under DCR-001 to record the architectural decisions taken during in-flight gap remediation (UnRAR mutex; additive `modify_with_options`; per-entry creation progress).

## Implementation Track
- Current phase: 6 (complete with diminishing gaps)
- Highest completed phase: 6
- Synced baseline ID: BL-001-retroactive
- Sync status: synced
- Blocked slices: none
- Allowed parallel slices: all (implementation done)
- Summary: All five user stories implemented. DCR-001 closed Phases A+B on 2026-04-13:
  - **OI-027-001 RESOLVED** (Phase A.1): unknown-stub SFX scanning now proceeds to signature scan instead of bailing at Stage 1. SCN-SFX-08 → Covered.
  - **RAR concurrency serialization RESOLVED** (Phase A.2): UnRAR FFI calls serialized behind a process-wide `UNRAR_LOCK` mutex (AD 0019). Concurrent RAR access from caller threads is now safe.
  - **Documentation drift CLOSED** (Phase A.3): archive-level integrity contract section added to `inspection.md`; rough-schema cross-reference added; design-baseline phase-state checkbox ticked.
  - **OI-025-003 RESOLVED** (Phases B.1 + B.2): `CompressionOptions.progress` wired through both creation backends per-entry (AD 0021). `ModificationOptions::create_backup` + `backup_suffix` honored via new `Archive::modify_with_options(path, opts)` (AD 0020).
  - **OI-025-001 RESOLVED** (2026-04-14): `ModificationOptions.compression` overrides archive recreation settings; `commit_changes()` uses caller-supplied compression level and password.
  - **OI-025-002 RESOLVED** (2026-04-14): `commit_changes()` preserves timestamps and Unix permissions via `add_file_from_data_with_metadata()` on both ZipWriter and LibarchiveArchive backends. `preserve_metadata` flag is now functional.
  - **DEF-006 Closed**, **DEF-007 Closed**, **DEF-008 Partially Closed** in stub manifest.

  Remaining tracked gaps:
  - DEF-001 (`open_at_offset`), DEF-002 (split creation), DEF-004 (true streaming for non-libarchive backends), DEF-005 (some ZIP modify edge cases) — all explicitly out-of-MVP-scope and tracked as DEFERRED. DEF-003 (SecStr migration) closed 2026-04-17 (OI-0057-003).

## Immediate Next Actions
- No critical Phase C gaps remain. Focus shifts to polish, test coverage, and deferred items for v0.2.0.

## Supporting Artifacts
- Local run / bootstrap notes: `docs/architecture/bootstrap-config.md`
- Active impact report: `docs/project/implementation-impact-report.md` (IIR-001) — drift items closed; in-flight gap rows now reflect Phase A+B closures.
- Design change records: `docs/project/design-change-records/DCR-001-in-flight-gap-remediation.md`
- New ADRs: `docs/architecture/decisions/0019-unrar-process-wide-mutex.md`, `0020-additive-modification-options-extension.md`, `0021-per-entry-creation-progress.md`
- Source-of-truth table: N/A (single-crate library; the design-first-architecture skill marks this artifact "multi-binary only").
