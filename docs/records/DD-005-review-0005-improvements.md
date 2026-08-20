---
type: ADR
title: "DD-005: Review 0005 Improvements"
description: "Addressed multiple correctness and safety issues: **Windows Rename:** `std::fs::rename` doesn't overwrite on Windows; switched to `MoveFileExW`. **Symlinks:** Enforced consistent skipping of symlinks across backends.…"
tags: [decision, R005-02, R005-03, R005-04, R005-05, R005-06, R005-07]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# DD-005: Review 0005 Improvements

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R005-02, R005-03, R005-04, R005-05, R005-06, R005-07
- **Date:** 2025-01-07
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Addressed multiple correctness and safety issues:
- **Windows Rename:** `std::fs::rename` doesn't overwrite on Windows; switched to `MoveFileExW`.
- **Symlinks:** Enforced consistent skipping of symlinks across backends.
- **Integrity Report:** Fixed underflow in validation counts.
- **Multipart:** Fixed lexicographic sorting of parts (e.g., z10 vs z2).
- **Timestamps:** Added bounds checks for pre-epoch timestamps.
- **Temp Dir:** Added RAII guard to prevent leaks on error.

## Implementation

Applied patches to relevant files.

## Files Modified
- `src/modification.rs`
- `src/extraction.rs`
- `src/inspection.rs`
- `src/ffi/libarchive_wrapper.rs`
- `src/ffi/piz_wrapper.rs`

## Related
- R005-01 (Reject - SFX Offset)
- R005-08 (Reject - Split Size)
