---
type: ADR
title: "DD-013-001: UnRAR Compile Fix"
description: "Fixed a compilation error on non-macOS platforms where `.into_owned()` was called on a `String` (which does not implement it, as it's a `Cow` method)."
tags: [decision, R0013-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-013-001: UnRAR Compile Fix

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0013-0001
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Fixed a compilation error on non-macOS platforms where `.into_owned()` was called on a `String` (which does not implement it, as it's a `Cow` method).

## Implementation

Removed the redundant `.into_owned()` call.

## Files Modified

- `src/ffi/wrapper.rs`
