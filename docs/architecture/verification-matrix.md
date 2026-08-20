---
type: Verification Matrix
title: "Verification Matrix"
description: "Scenario-to-verification mapping."
tags: [architecture, verification, ADR-0027, R0065-0002, OI-0065-002]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# Verification Matrix

Scenario-to-verification mapping. Links each mandatory scenario to its verification approach and current coverage.

| Scenario ID | Scenario | Smoke? | Core Skeleton? | Verification Type | Test Location | Status |
|---|---|---|---|---|---|---|
| SCN-INS-01 | Inspect archive contents (unified API) | Yes | Yes | Integration | `tests/integration/`, `tests/format_compatibility_test.rs` | Covered |
| SCN-INS-02 | Inspect password-protected metadata | No | Yes | Integration | `tests/password_handling_test.rs` | Covered |
| SCN-INS-03 | Validate integrity via CRC32 | No | Yes | Integration | `tests/crc32_verification_test.rs`, `tests/integrity_comprehensive_test.rs` | Covered |
| SCN-INS-04 | Inspect multi-part archive (primarily RAR) | No | No | Integration | No dedicated test file; RAR multipart tested indirectly in `tests/integration/`. ZIP split archives (.z01) unsupported. | Partial |
| SCN-INS-05 | Filter large archive listings | No | No | Integration | `tests/integration/` (caller-side filtering over `&[ArchiveEntry]`) | Covered |
| SCN-EXT-01 | Extract all files (unified API) | Yes | Yes | Integration | `tests/integration/`, `tests/format_compatibility_test.rs` | Covered |
| SCN-EXT-02 | Extract password-protected archive | No | Yes | Integration | `tests/password_handling_test.rs` | Covered |
| SCN-EXT-03 | Extract multi-layer compressed | No | Yes | Integration | `tests/format_compatibility_test.rs` | Covered |
| SCN-EXT-04 | Handle corrupted archive | No | Yes | Integration | `tests/crc32_verification_test.rs` | Covered |
| SCN-EXT-05 | Monitor extraction progress | No | No | Integration | `tests/progress_callback_test.rs`, `tests/contract/progress_contract.rs` | Covered — per-backend cadence is heuristic (throttled on libarchive/UnRAR, per-entry elsewhere); `ControlFlow::Break` cancellation exercised by both the progress-callback integration test and the progress contract suite |
| SCN-CRE-01 | Create archive in multiple formats | Yes | Yes | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-02 | Create with max compression | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-03 | Create password-protected archive (rejected) | No | Yes | Integration | `tests/integration/creation.rs`, `tests/password_handling_test.rs` | Covered — tests assert `ArchiveError::OperationBlocked` for every format per MADR-0027; encrypted *reads* remain supported via `open_encrypted()` |
| SCN-CRE-04 | Create from large dataset with progress | No | No | Integration | `tests/creation_progress_test.rs` | Covered (per-entry granularity) — OI-025-003 creation-progress portion resolved 2026-04-13. Per-entry rather than per-byte; `total` is `None` because creation streams are not pre-sized. Cancellation via `ControlFlow::Break` surfaces as `ArchiveError`. |
| SCN-CRE-05 | Create with compression level control | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-MOD-01 | Add files to existing archive | No | Yes | Integration | `tests/integration/modification.rs` | Partial — round-trip (entry count, paths, payload) covered; `tests/modification_options_test.rs` asserts `ModificationOptions` wiring. Modified, accessed, and created timestamps plus Unix permissions are preserved on retained regular-file entries (OI-0065-002 resolved); directory metadata still uses backend defaults. See `tests/modification_options_test.rs` for the dedicated round-trip. Compression preserved via `ModificationOptions.compression` (OI-025-001 resolved 2026-04-14). |
| SCN-MOD-02 | Remove entries from archive | No | Yes | Integration | `tests/integration/modification.rs` | Covered — compression settings preserved per `ModificationOptions`; entry identity threaded via IDs (R0065-0002/3/11/12). Per-entry metadata fields not asserted (see SCN-MOD-01). |
| SCN-MOD-03 | Replace file in archive | No | No | Integration | `tests/integration/modification.rs` | Covered — some ZIP cases ignore-gated; compression preserved (OI-025-001 resolved 2026-04-14). Per-entry metadata fields not asserted (see SCN-MOD-01). |
| SCN-MOD-04 | Modify large archive efficiently | No | No | Performance | `tests/performance_test.rs` | Partial — exploratory; no defined size threshold or pass/fail criteria for "large" |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only (constructed in-test), not real-world SFX executables |
| SCN-SFX-02 | Detect WinRAR SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-03 | Detect 7-Zip SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-04 | Detect Linux ELF SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-05 | Detect ScriptInterpreter SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-06 | Reject standard archive (not SFX) | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Covered (synthetic test data only) |
| SCN-SFX-07 | Reject non-archive executable | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Covered (synthetic test data only) |
| SCN-SFX-08 | Reject SFX with unknown stub | No | Yes | Integration | `src/sfx/detection.rs` — `test_detect_sfx_unknown_stub_with_zip_payload_is_rejected`, `test_detect_sfx_unknown_stub_without_payload_stays_not_sfx` | Covered (synthetic test data only) — R0070-0076: a `StubType::Unknown` header is rejected at Stage 1, before the Stage-2 signature scan, so an arbitrary binary carrying incidental archive magic cannot report `is_sfx=true`. Supersedes the OI-027-001 expectation that unknown stubs proceed to the scan; see `docs/architecture/scenario-matrix.md` and `docs/records/AD-0060-r0070-broad-modular-triage-closure.md` |

> Column note: `Core Skeleton?` here is *intended* as the same flag that
> `docs/architecture/scenario-matrix.md` and `docs/project/design-baseline.md` call
> `Core Verification?`, and on 26 of the 27 scenarios shared with those documents the three
> values agree. **They do not agree on `SCN-CRE-03`** — this matrix says `Yes`, the scenario
> matrix and the design baseline both say `No` — so treat the columns as corresponding
> rather than as guaranteed-identical, and do not derive one from the other for that row.
> The same row is also the one where the scenario matrix (`Critical Negative Path? = Yes`)
> and the design baseline (`No`) diverge, which is consistent with the flags having been
> reworked at different times when `MADR-0027` turned "create password-protected archive"
> into a rejection path. Which value is authoritative is an **owner ruling that has not been
> made**; none of the three documents should be edited to match another until it is.
>
> This matrix carries no `Critical Negative Path?` column — read that flag from the scenario
> matrix, not from `Smoke?`.

## Verification Types

- **Integration:** Full end-to-end test exercising the public API. Note: SFX tests use synthetic (constructed-in-test) archive files, not real-world SFX executables.
- **Performance:** Benchmark or resource-bounded test (memory, time)
- **Property-based:** Proptest-driven invariant checks (path normalization, cross-format consistency)

## Additional Verification

| Category | Test Location | Coverage |
|---|---|---|
| Property-based tests | `tests/property_tests.rs` | Path normalization, format consistency |
| Concurrency tests | `tests/integration/concurrency.rs` | Thread safety, parallel operations. Concurrent RAR access serialized via process-wide `UNRAR_LOCK` mutex (2026-04-13); `test_concurrent_rar_open_and_list` runs 8 threads × 20 iterations against RAR4 + RAR5 fixtures. |
| Streaming tests | `tests/streaming_test.rs` | Streaming API shape verification (memory-bounded only for libarchive-backed formats). ZipReader buffered backend used for encrypted ZIP streaming. |
| Benchmark suite | `benches/` | Three Criterion benches (`archive_operations`, `integrity_validation_bench`, `sfx_detection`); no gating thresholds or regression criteria defined |
