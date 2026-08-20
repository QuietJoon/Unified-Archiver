---
type: ADR
title: "DD-011-001: Piz Backend 4GB Limit"
description: "The Piz backend had a hardcoded 4GB limit for memory mapping, which prevented opening large ZIP files on 64-bit systems where virtual address space is plentiful."
tags: [decision, R0011-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-011-001: Piz Backend 4GB Limit

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0011-0001
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The Piz backend had a hardcoded 4GB limit for memory mapping, which prevented opening large ZIP files on 64-bit systems where virtual address space is plentiful.

## Implementation

Added `ExtractionLimits::with_max_mmap_size` and environment variable support (`UNIFIED_ARCHIVE_MAX_MMAP_SIZE`) to make this limit configurable.

## Files Modified

- `src/security.rs`
- `src/ffi/piz_wrapper.rs`
