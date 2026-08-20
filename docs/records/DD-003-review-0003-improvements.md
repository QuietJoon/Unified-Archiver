---
type: ADR
title: "DD-003: Review 0003 Improvements"
description: "**Partial Extraction:** Enforced password/limits on partial extraction. **Libarchive Checks:** Added return code checks. **Piz Mmap:** Added platform-specific mmap size limits (4GB/100MB). **Options:** Enforced…"
tags: [decision, R003-01, R003-02, R003-03, R003-04, R003-05]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# DD-003: Review 0003 Improvements

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R003-01, R003-02, R003-03, R003-04, R003-05
- **Date:** 2025-01-07
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale
- **Partial Extraction:** Enforced password/limits on partial extraction.
- **Libarchive Checks:** Added return code checks.
- **Piz Mmap:** Added platform-specific mmap size limits (4GB/100MB).
- **Options:** Enforced `overwrite` and fixed separator issues.

## Implementation
Applied patches across extraction logic and backends.

## Files Modified
- `src/extraction.rs`
- `src/security.rs`
- `src/ffi/libarchive_wrapper.rs`
- `src/ffi/piz_wrapper.rs`
- `src/ffi/zip_writer.rs`
