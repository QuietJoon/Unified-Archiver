---
type: ADR
title: "DD-010-001: Hard Link Skipping"
description: "Documentation stated hard links were skipped for security (FR-022), but the implementation only checked for symlinks."
tags: [decision, R0010-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-010-001: Hard Link Skipping

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0010-0001
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Documentation stated hard links were skipped for security (FR-022), but the implementation only checked for symlinks.

## Implementation

Added detection for hard links in `ArchiveEntry` and updated extraction logic to skip them with warnings.

## Files Modified

- `src/entry.rs`
- `src/ffi/libarchive_wrapper.rs`
- `src/inspection.rs`
