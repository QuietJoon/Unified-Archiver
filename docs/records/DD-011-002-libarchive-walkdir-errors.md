---
type: ADR
title: "DD-011-002: Libarchive Walkdir Errors"
description: "`add_directory_recursive` was silently dropping files if `walkdir` encountered an error (e.g., permission denied), leading to incomplete archives without warning."
tags: [decision, R0011-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-011-002: Libarchive Walkdir Errors

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0011-0002
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

`add_directory_recursive` was silently dropping files if `walkdir` encountered an error (e.g., permission denied), leading to incomplete archives without warning.

## Implementation

Updated the iterator loop to propagate errors.

## Files Modified

- `src/ffi/libarchive_wrapper.rs`
