---
type: ADR
title: "DD-020-002: UnRAR FILETIME Epoch Offset"
description: "High-resolution timestamps (mtime/ctime/atime) in RAR archives use Windows FILETIME format based on 1601-01-01."
tags: [decision, R0020-0002]
timestamp: 2026-01-13T00:00:00Z
status: active
---

# DD-020-002: UnRAR FILETIME Epoch Offset

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0020-0002 (Review 0020)
- **Date:** 2026-01-13
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

High-resolution timestamps (mtime/ctime/atime) in RAR archives use Windows FILETIME format based on 1601-01-01. The code incorrectly added these values directly to Unix epoch (1970-01-01), resulting in dates ~369 years in the future.

## Implementation

Subtract 11,644,473,600 seconds (offset between 1601 and 1970) before adding to `UNIX_EPOCH`. Uses `saturating_sub` for safety.

## Files Modified

- `src/ffi/wrapper.rs` - Lines 766-804

## Related

- None
