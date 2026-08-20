---
type: ADR
title: "DD-009-001: Zip Path Traversal"
description: "The `ZipArchive` backend was not using `sanitize_entry_path` for extracted files, allowing potential path traversal attacks."
tags: [decision, R0009-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-009-001: Zip Path Traversal

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0009-0001
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The `ZipArchive` backend was not using `sanitize_entry_path` for extracted files, allowing potential path traversal attacks.

## Implementation

Applied path sanitization to `extract_all` and `extract_file` in the Zip backend.

## Files Modified

- `src/ffi/zip_wrapper.rs`
