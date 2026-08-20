---
type: Report
title: "SFX Module Test Coverage Report"
description: "Measured 2026-08-05 with cargo-llvm-cov: 99.4% line coverage across src/sfx/ — the T110a >90% threshold is verified."
tags: [reference]
timestamp: 2026-04-12T00:00:00Z
status: active
---

# SFX Module Test Coverage Report

**Date**: 2026-08-05 (supersedes the 2025-11-13 estimate-only draft)
**Task**: T110a - Verify >90% test coverage for src/sfx/ module (Constitution Principle IV)

## Summary

**Status**: VERIFIED — measured line coverage for `src/sfx/` is **99.4%**, comfortably above the
>90% threshold. This is a real `cargo-llvm-cov` measurement over the full green test suite, not
the test/code-line ratio the previous draft used as a proxy.

The earlier draft predated two structural changes and described a module that no longer exists in
that shape: Innovation I3 (2026-07-22) replaced the `f32` confidence score with the
`SfxConfidence` enum (`NotSfx` / `Probable` / `Confirmed`) plus an `evidence: Vec<String>` trail,
and `src/sfx/limits.rs` now centralises the scan/size ceilings. `limits.rs` does not appear in
the coverage table because it contains only `const` items — there is no executable code to cover.

## Measurement

```bash
cargo llvm-cov --all-features --no-report -- --test-threads=4   # full suite, instrumented
cargo llvm-cov report
```

- Toolchain: `cargo-llvm-cov` 0.8.7 + `llvm-tools-preview` (stable, Rust 2024 edition).
- Test population: the entire suite (unit + all integration binaries), all features enabled;
  every test passed on the measurement run.
- Measured at the head of the 001-unified-archive branch on 2026-08-05.

## Measured coverage (src/sfx/)

| File | Regions | Region cover | Functions | Function cover | Lines | Line cover |
|------|---------|--------------|-----------|----------------|-------|------------|
| `detection.rs` | 1265 | 98.97% | 42 | 95.24% | 534 | **99.06%** |
| `result.rs` | 436 | 99.31% | 42 | 100.00% | 297 | **100.00%** |
| `signatures.rs` | 416 | 100.00% | 31 | 100.00% | 208 | **100.00%** |
| `stub_types.rs` | 451 | 98.89% | 49 | 100.00% | 297 | **98.99%** |
| **Module total** | 2568 | ~99.2% | 164 | ~98.8% | 1336 | **~99.4%** |

(`limits.rs`: constants only, no coverable regions. Whole-crate context: 82.43% lines.)

## Gap closure relative to the 2025-11-13 draft

The draft's action items are all discharged:

- **Install a coverage tool** — `cargo-llvm-cov` installed to `CARGO_HOME/bin`; measurement
  wired as above.
- **Error-path tests** — landed across the intervening review cycles (corrupted-signature,
  bounds, and I/O failure paths are covered; e.g. the bzip2 empty-stream/EOS probe R0080-0085,
  gzip reserved-flag screening R0079-0031) and topped up on 2026-08-05:
  `test_detect_sfx_xz_reserved_flag_rejected` exercises the Stage-3 xz probe's reject path.
- **Edge-case tests** — 1 MB scan-boundary, signature-at-offset-0 demotion, and
  multi-signature preference are covered (`test_detect_sfx_prefers_strong_magic_over_embedded_gzip`
  and friends); `test_detect_sfx_xz_payload` (2026-08-05) covers the previously-unexercised xz
  payload-validation arm.
- **Accessor coverage** (2026-08-05): `test_payload_coordinates_populated_and_none`
  (`result.rs`) and `test_is_known` (`stub_types.rs`) close the last uncovered public accessors.
- **Re-run and verify >90%** — done; see the table.

## Residual uncovered code (accepted)

- `detection.rs` Stage-3 payload probe, `_` fallback arm ("unknown format ⇒ require ≥100
  bytes"): **defensively unreachable** — the signature table in `signatures.rs` contains exactly
  the seven formats the probe matches explicitly (Zip, Rar, Rar5, SevenZip, Gzip, Bzip2, Xz), so
  no scan result can reach the arm today. It guards future signature-table additions and is kept
  deliberately.
- A handful of cold diagnostic lines (panic-format arguments inside test assertions, an
  unreachable padding branch in a fixture builder). None are production logic.

## Test population snapshot (2026-08-05)

114 unit tests live inside the module files themselves — `detection.rs` 33, `result.rs` 23,
`signatures.rs` 24, `stub_types.rs` 34 — and that is the figure to compare against the per-file
coverage above. `cargo test --lib sfx` reports 118: the name filter also matches four
SFX-adjacent tests outside the module (`archive::tests` ×2 and
`archive::sfx_fallback_error_tests` ×2), which exercise the facade entry points rather than
`src/sfx/` itself. On top of those sit the integration suites
`tests/integration/sfx_detection.rs`, `tests/integration/sfx_false_positives.rs`, and
`tests/integration/sfx_staging_progress.rs`. Integration fixtures remain **synthetic**
(programmatically constructed stub+signature byte sequences); incorporating real-world SFX
binaries (NSIS, WinRAR SFX modules) stays a nice-to-have, not a threshold requirement.

## Action Items

- [x] Install `cargo-llvm-cov` (or `cargo-tarpaulin`)
- [x] Run actual coverage analysis
- [x] Add missing error path tests
- [x] Add edge case tests
- [x] Re-run coverage and verify >90% threshold — **verified at ~99.4% lines**
