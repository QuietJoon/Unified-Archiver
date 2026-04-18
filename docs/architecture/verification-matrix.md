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
| SCN-EXT-05 | Monitor extraction progress | No | No | Integration | `tests/progress_callback_test.rs` | Partial — progress callback cadence is heuristic (not guaranteed); cancellation-via-callback not fully verified |
| SCN-CRE-01 | Create archive in multiple formats | Yes | Yes | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-02 | Create with max compression | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-03 | Create password-protected archive (rejected) | No | Yes | Integration | `tests/integration/creation.rs`, `tests/password_handling_test.rs` | Covered — tests assert `ArchiveError::OperationBlocked` for every format per AD 0027; encrypted *reads* remain supported via `open_encrypted()` |
| SCN-CRE-04 | Create from large dataset with progress | No | No | Integration | `tests/creation_progress_test.rs` | Covered (per-entry granularity) — OI-025-003 creation-progress portion resolved 2026-04-13. Per-entry rather than per-byte; `total` is `None` because creation streams are not pre-sized. Cancellation via `ControlFlow::Break` surfaces as `ArchiveError`. |
| SCN-CRE-05 | Create with compression level control | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-MOD-01 | Add files to existing archive | No | Yes | Integration | `tests/integration/modification.rs` | Covered — metadata preserved (timestamps, permissions) via `add_file_from_data_with_metadata`; compression configurable via `ModificationOptions.compression` (OI-025-001/002 resolved 2026-04-14) |
| SCN-MOD-02 | Remove entries from archive | No | Yes | Integration | `tests/integration/modification.rs` | Covered — metadata and compression settings preserved per `ModificationOptions` (OI-025-001/002 resolved 2026-04-14) |
| SCN-MOD-03 | Replace file in archive | No | No | Integration | `tests/integration/modification.rs` | Covered — some ZIP cases ignore-gated; metadata and compression preserved (OI-025-001/002 resolved 2026-04-14) |
| SCN-MOD-04 | Modify large archive efficiently | No | No | Performance | `tests/performance_test.rs` | Partial — exploratory; no defined size threshold or pass/fail criteria for "large" |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only (constructed in-test), not real-world SFX executables |
| SCN-SFX-02 | Detect WinRAR SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-03 | Detect 7-Zip SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-04 | Detect Linux ELF SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-05 | Detect ScriptInterpreter SFX | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Partial — synthetic test binaries only |
| SCN-SFX-06 | Reject standard archive (not SFX) | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Covered (synthetic test data only) |
| SCN-SFX-07 | Reject non-archive executable | No | Yes | Integration | `tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs` | Covered (synthetic test data only) |
| SCN-SFX-08 | Detect SFX with unknown stub | No | Yes | Integration | `src/sfx/detection.rs` — `test_detect_sfx_unknown_stub_with_zip_payload`, `test_detect_sfx_unknown_stub_without_payload_stays_not_sfx` | Covered (synthetic test data only) — OI-027-001 resolved: unrecognized executables now classify as `StubType::Unknown` and proceed to signature scan |

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
| Benchmark suite | `benches/` | Benchmark coverage exists; no gating thresholds or regression criteria defined |
