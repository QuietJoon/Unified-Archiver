---
type: ADR
title: "DD-017-001: ZipWriter OOM Risk"
description: "The `add_file_from_path` method in ZipWriter was reading entire files into memory via `std::fs::read()` before writing to the archive."
tags: [decision, R0017-0001]
timestamp: 2026-01-12T00:00:00Z
status: active
---

# DD-017-001: ZipWriter OOM Risk

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0017-0001 (Review 0017)
- **Date:** 2026-01-12
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The `add_file_from_path` method in ZipWriter was reading entire files into memory via `std::fs::read()` before writing to the archive. This caused O(N) memory usage and potential OOM for large files. This is the same pattern that was fixed in libarchive_wrapper.rs (R0012-0001).

## Implementation

Replaced full-file read with streaming using `std::io::copy()` from file handle directly to ZipWriter.

## Files Modified

- `src/ffi/zip_writer.rs` - Lines 78-103

## Related

- DD-012-001 (same fix for libarchive backend)
