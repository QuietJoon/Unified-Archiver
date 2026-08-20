---
type: ADR
title: "DD-012-004: Libarchive Creation Timestamp Precision"
description: "Archive creation was truncating timestamps to seconds, losing nanosecond precision provided by the filesystem."
tags: [decision, R0012-0004]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-012-004: Libarchive Creation Timestamp Precision

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0012-0004
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Archive creation was truncating timestamps to seconds, losing nanosecond precision provided by the filesystem.

## Implementation

Updated `archive_entry_set_mtime` usage to include `subsec_nanos`.

## Files Modified

- `src/ffi/libarchive_wrapper.rs`
