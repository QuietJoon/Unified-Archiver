---
type: ADR
title: "DD-021: Extract-to-Memory OOM Protection"
description: "All `extract_to_memory` implementations lacked size validation before allocating buffers."
tags: [decision, R0021-0001, R0021-0002]
timestamp: 2026-01-13T00:00:00Z
status: active
---

# DD-021: Extract-to-Memory OOM Protection

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0021-0001, R0021-0002 (Review 0021)
- **Date:** 2026-01-13
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

All `extract_to_memory` implementations lacked size validation before allocating buffers. An attacker could craft archives with huge declared sizes to trigger OOM panics (zip bomb DoS). The 7z backend was most vulnerable as it streamed directly to memory without any temp file.

## Implementation

Added consistent OOM protection across all backends:
1. Check if declared size exceeds `usize::MAX`
2. Use `try_reserve()` for safe pre-allocation
3. Return `ArchiveError::UnsupportedOperation` on allocation failure instead of panicking

## Files Modified

- `src/ffi/sevenz_wrapper.rs` - 7z backend
- `src/ffi/wrapper.rs` - UnRAR backend
- `src/ffi/libarchive_wrapper.rs` - libarchive backend

## Related

- DD-017-001 (ZipWriter OOM fix)
