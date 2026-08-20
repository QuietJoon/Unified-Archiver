---
type: ADR
title: "DD-004: Review 0004 Improvements"
description: "**Zip Walkdir:** Fixed silent error dropping in Zip writer. **Piz Alloc:** Added allocation guards to prevent OOM on corrupted ZIP headers. **Timestamp Panic:** Fixed panic on pre-epoch timestamps in libarchive."
tags: [decision, R004-02, R004-03, R004-04]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# DD-004: Review 0004 Improvements

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R004-02, R004-03, R004-04
- **Date:** 2025-01-07
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale
- **Zip Walkdir:** Fixed silent error dropping in Zip writer.
- **Piz Alloc:** Added allocation guards to prevent OOM on corrupted ZIP headers.
- **Timestamp Panic:** Fixed panic on pre-epoch timestamps in libarchive.

## Implementation
Applied patches to `zip_writer.rs`, `piz_wrapper.rs`, and `libarchive_wrapper.rs`.

## Files Modified
- `src/ffi/zip_writer.rs`
- `src/ffi/piz_wrapper.rs`
- `src/ffi/libarchive_wrapper.rs`
