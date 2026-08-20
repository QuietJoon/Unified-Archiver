---
type: ADR
title: "DD-012-001: Libarchive Memory Inefficiency"
description: "The `add_file_from_path` implementation was reading entire files into memory before writing to the archive, causing O(N) memory usage."
tags: [decision, R0012-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-012-001: Libarchive Memory Inefficiency

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0012-0001
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The `add_file_from_path` implementation was reading entire files into memory before writing to the archive, causing O(N) memory usage.

## Implementation

Replaced full-file read with a streaming buffer copy (8KB chunks).

## Files Modified

- `src/ffi/libarchive_wrapper.rs`
