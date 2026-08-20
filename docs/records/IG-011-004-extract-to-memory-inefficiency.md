---
type: ADR
title: "IG-011-004: extract_to_memory inefficiency"
description: "Libarchive backend extracts to disk then reads back to memory instead of extracting directly to memory."
tags: [decision, R0011-0004]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-011-004: extract_to_memory inefficiency

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0011-0004 (Review 0011)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/ffi/libarchive_wrapper.rs:360`

## Issue Summary

Libarchive backend extracts to disk then reads back to memory instead of extracting directly to memory.

## Rationale

- Performance optimization only.
- Current implementation is functional and safe.
- Deferred to avoid complexity in FFI data handling.

## Future Consideration

Optimization in future performance-focused sprint.

## Related

- None
