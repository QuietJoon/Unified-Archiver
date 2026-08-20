---
type: ADR
title: "IG-004-01: Streaming buffers entire file"
description: "Streaming extraction APIs currently read full entries into memory before providing a stream."
tags: [decision, R004-01]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# IG-004-01: Streaming buffers entire file

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R004-01 (Review 0004)
- **Date:** 2025-01-07
- **Decision:** REJECT
- **Severity:** Medium

## Location

`src/ffi/wrapper.rs:592`

## Issue Summary

Streaming extraction APIs currently read full entries into memory before providing a stream.

## Rationale

- True streaming requires significant refactoring across all FFI backends.
- Current behavior is functional and memory usage is bounded by single-file size.
- Documented as a limitation.

## Future Consideration

Re-evaluate for Phase 3 performance optimizations.

## Related

- None
