# Test Suite Layout (R0074-0079)

This directory holds three layers of test coverage with overlapping but
distinct responsibilities. New tests should land in the layer whose
contract they protect.

## Layers

### `tests/contract/*.rs`
Contract suites pin the public-API surface. They assert behavioral
compatibility — error variants, return shapes, mode-dependent
semantics — and run quickly. Avoid timing-dependent assertions here
(R0074-0071); move performance work to `benches/` or a perf-only
profile.

Examples:
- `inspection_contract.rs` — `Archive::list_files`, `find_entry`,
  `entry_count`, `find_entries`, `validate_integrity` contracts.
- `extraction_contract.rs` — `Archive::extract_all`, `extract_file`,
  `extract_to_memory`, `extract_to_stream` contracts.

### `tests/integration/*.rs`
Integration suites exercise end-to-end flows across multiple modules
or backends. They may write real fixtures to disk and consult external
tools when needed (gated behind opt-in env vars per R0074-0080).

Examples:
- `format_compatibility.rs` — cross-format consistency for ZIP/7z/RAR/TAR.
- `concurrency.rs` — parallel `Archive` handles on different files.
- `multipart_typed.rs` — typed `MultipartLayout` return: single-part ZIP / 7z / TAR surface as `Single` (R0075-0083).
- `sfx_detection.rs` — SFX detection + open flows.
- `performance_baseline.rs` — gated baseline (`UA_PRINT_PERF_BASELINE`).

### Root-level `tests/*_test.rs`
Behavior-organized regression and feature-coverage tests. These
typically protect a specific scenario or guard against a regression
flagged in a code review. Naming convention: `<feature>_<axis>_test.rs`
(e.g. `extraction_options_test.rs`, `modification_options_test.rs`).

Two historical exceptions to the naming convention:
- **`review_0068_test.rs`** keeps the review-numbered name for
  traceability against R0068 findings (R0074-0072 acknowledges the
  cost; relocation tracked under "Test reorganization").
- **`format_compatibility_test.rs`** vs
  `integration/format_compatibility.rs` — two suites covering similar
  fixtures (R0074-0073). Consolidation tracked alongside this README.

## Naming axes

Use these axes when picking a destination for new tests:

| Axis              | File-name component | Example                              |
|-------------------|---------------------|--------------------------------------|
| Operation         | `extraction`, `modification`, `creation`, `inspection` | `extraction_options_test.rs` |
| Format            | `zip`, `7z`, `rar`, `tar` | `tar_streaming_test.rs`         |
| Behavior axis     | `password`, `progress`, `metadata`, `crc32` | `password_handling_test.rs`   |
| Integrity         | `integrity_*`       | `integrity_edge_cases_test.rs`       |

## External-tool tests

Tests that shell out to `zip`, `unzip`, `7z`, `tar`, or `rar.exe` must
go through `tests/common/mod.rs::command_exists` so the skip behavior
is uniform (R0074-0075). Direct `std::process::Command::new(...)`
calls in `_test.rs` files should be migrated when touched.

## Fixture builders

Common fixtures live in `tests/common/mod.rs`. Avoid duplicating ZIP /
TAR builder helpers across test files (R0074-0076); add the helper to
`common/` and import.

## Performance tests

Default `cargo test` runs should not produce timing-dependent
assertions or noisy baseline prints. Performance baselines live behind
`UA_PRINT_PERF_BASELINE=1` (R0074-0078) and any contract-level
performance assertion behind `UA_RUN_PERF_CONTRACT_TEST=1`
(R0074-0071). True benchmarks belong in `benches/` (R0074-0070).

## Temp directories

Tests that need scratch files must use `tempfile::tempdir()` (or the
existing `common::temp_test_dir()` helper in `tests/common/`).
Machine-specific absolute scratch paths were removed (R0074-0074, and
the R0060 owner reversal) and should not return — they conflict with
the project's portable tempdir convention.
