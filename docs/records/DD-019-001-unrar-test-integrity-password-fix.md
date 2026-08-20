---
type: ADR
title: "DD-019-001: UnRAR test_integrity Password Fix"
description: "The `test_integrity()` method in UnrarArchive was opening a fresh handle using `Self::open()` directly, which does not preserve the password."
tags: [decision, R0019-0001]
timestamp: 2026-01-12T00:00:00Z
status: active
---

# DD-019-001: UnRAR test_integrity Password Fix

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0019-0001 (Review 0019)
- **Date:** 2026-01-12
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The `test_integrity()` method in UnrarArchive was opening a fresh handle using `Self::open()` directly, which does not preserve the password. This caused integrity tests to fail on all encrypted RAR archives.

## Implementation

Changed to use `self.fresh_handle()` which correctly preserves the password from the original handle.

## Files Modified

- `src/ffi/wrapper.rs` - Line 674

## Related

- None
