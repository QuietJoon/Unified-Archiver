---
type: ADR
title: "DD-023-001: Libarchive Write-Mode Data Integrity"
description: "Archive creation could report success while writing incomplete entries."
tags: [decision, R0023-0001]
timestamp: 2026-03-17T00:00:00Z
status: active
---

# DD-023-001: Libarchive Write-Mode Data Integrity

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0023-0001 (Review 0023)
- **Date:** 2026-03-17
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Archive creation could report success while writing incomplete entries. `archive_write_data` short writes were unchecked, and `archive_write_finish_entry` return codes were ignored in both `add_file_from_data` and `add_file_from_path`. This is the creation-side counterpart to DD-010-003 (extraction).

## Implementation

Added short-write detection (byte count mismatch) and `archive_write_finish_entry` return code checking in both write paths.

## Files Modified

- `src/ffi/libarchive_wrapper.rs` - `add_file_from_data`, `add_file_from_path`

## Related

- DD-010-003 (extraction-side finish_entry fix)
