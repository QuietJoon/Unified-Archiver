---
type: ADR
title: "DD-024-003: OOM Protection for Libarchive and Zip extract_to_memory"
description: "DD-021 claimed to fix OOM in all extract_to_memory backends but missed the libarchive and native ZIP backends."
tags: [decision, R0024-0003]
timestamp: 2026-03-17T00:00:00Z
status: active
---

# DD-024-003: OOM Protection for Libarchive and Zip extract_to_memory

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0024-0003 (Review 0024)
- **Date:** 2026-03-17
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

DD-021 claimed to fix OOM in all extract_to_memory backends but missed the libarchive and native ZIP backends. Both used `Vec::with_capacity` with untrusted sizes from archive metadata, allowing DoS via crafted archives.

## Implementation

Applied `try_reserve` + size validation to both backends, matching the pattern from DD-021. Fixed a signed/unsigned comparison bug in the original patch (`usize::MAX as i64` wraps to -1 on 64-bit).

## Files Modified

- `src/ffi/libarchive_wrapper.rs` - `extract_to_memory`
- `src/ffi/zip_wrapper.rs` - `extract_to_memory`

## Related

- DD-021 (original incomplete fix)
