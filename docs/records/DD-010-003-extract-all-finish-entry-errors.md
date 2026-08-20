---
type: ADR
title: "DD-010-003: Extract All Finish Entry Errors"
description: "`extract_all` was ignoring return codes from `archive_write_finish_entry`, potentially masking write failures."
tags: [decision, R0010-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-010-003: Extract All Finish Entry Errors

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0010-0003
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

`extract_all` was ignoring return codes from `archive_write_finish_entry`, potentially masking write failures.

## Implementation

Added error checking for the finish entry operation.

## Files Modified

- `src/ffi/libarchive_wrapper.rs`
