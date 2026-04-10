# Verification Matrix

Scenario-to-verification mapping. Links each mandatory scenario to its verification approach and current coverage.

| Scenario ID | Scenario | Smoke? | Core Skeleton? | Verification Type | Test Location | Status |
|---|---|---|---|---|---|---|
| SCN-INS-01 | Inspect archive contents (unified API) | Yes | Yes | Integration | `tests/integration/`, `tests/format_compatibility_test.rs` | Covered |
| SCN-INS-02 | Inspect password-protected metadata | No | Yes | Integration | `tests/password_handling_test.rs` | Covered |
| SCN-INS-03 | Validate integrity via CRC32 | No | Yes | Integration | `tests/crc32_verification_test.rs`, `tests/integrity_comprehensive_test.rs` | Covered |
| SCN-INS-04 | Inspect multi-part archive | No | No | Integration | `tests/multipart_test.rs` | Covered |
| SCN-INS-05 | Filter large archive listings | No | No | Integration | `tests/integration/` | Covered |
| SCN-EXT-01 | Extract all files (unified API) | Yes | Yes | Integration | `tests/integration/`, `tests/format_compatibility_test.rs` | Covered |
| SCN-EXT-02 | Extract password-protected archive | No | Yes | Integration | `tests/password_handling_test.rs` | Covered |
| SCN-EXT-03 | Extract multi-layer compressed | No | Yes | Integration | `tests/format_compatibility_test.rs` | Covered |
| SCN-EXT-04 | Handle corrupted archive | No | Yes | Integration | `tests/crc32_verification_test.rs` | Covered |
| SCN-EXT-05 | Monitor extraction progress | No | No | Integration | `tests/progress_callback_test.rs` | Covered |
| SCN-CRE-01 | Create archive in multiple formats | Yes | Yes | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-02 | Create with max compression | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-03 | Create password-protected archive | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-CRE-04 | Create from large dataset with progress | No | No | Performance | `tests/performance_test.rs` | Covered |
| SCN-CRE-05 | Create with compression level control | No | No | Integration | `tests/integration/creation.rs` | Covered |
| SCN-MOD-01 | Add files to existing archive | No | Yes | Integration | `tests/integration/modification.rs` | Covered |
| SCN-MOD-02 | Remove entries from archive | No | Yes | Integration | `tests/integration/modification.rs` | Covered |
| SCN-MOD-03 | Replace file in archive | No | No | Integration | `tests/integration/modification.rs` | Partial (some ZIP cases ignore-gated) |
| SCN-MOD-04 | Modify large archive efficiently | No | No | Performance | `tests/performance_test.rs` | Partial |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-02 | Detect WinRAR SFX | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-03 | Detect 7-Zip SFX | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-04 | Detect Linux ELF SFX | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-05 | Detect shell script SFX | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-06 | Reject standard archive (not SFX) | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-07 | Reject non-archive executable | No | Yes | Integration | `tests/sfx_detection_test.rs` | Covered |
| SCN-SFX-08 | Detect SFX with unknown stub | No | No | Integration | `tests/sfx_detection_test.rs` | Not covered |

## Verification Types

- **Integration:** Full end-to-end test using real archive files, exercising the public API
- **Performance:** Benchmark or resource-bounded test (memory, time)
- **Property-based:** Proptest-driven invariant checks (path normalization, cross-format consistency)

## Additional Verification

| Category | Test Location | Coverage |
|---|---|---|
| Property-based tests | `tests/property_tests.rs` | Path normalization, format consistency |
| Concurrency tests | `tests/integration/concurrency.rs` | Thread safety, parallel operations |
| Streaming tests | `tests/streaming_test.rs` | Memory-bounded extraction |
| Benchmark suite | `benches/` | Performance regression detection |
