---
type: ADR
title: "DD-024-001: RAR5 parse_rar5_recovery Panic Fix"
description: "Critical DoS vulnerability."
tags: [decision, R0024-0001]
timestamp: 2026-03-17T00:00:00Z
status: active
---

# DD-024-001: RAR5 parse_rar5_recovery Panic Fix

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0024-0001 (Review 0024)
- **Date:** 2026-03-17
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Critical DoS vulnerability. `decode_vint` can consume up to 10 bytes, but the block header buffer was only 11 bytes. Indexing `block_start[4 + offset1..]` where `offset1` can be up to 10 causes out-of-bounds panic on crafted RAR5 archives.

## Implementation

Increased buffer from 11 to 34 bytes (enough for CRC32 + 3 max-length vints). Added explicit bounds checks before each `decode_vint` call to return `Ok(None)` on truncated headers instead of panicking.

## Files Modified

- `src/ffi/wrapper.rs` - Lines 239-262

## Related

- DD-018-001, DD-020-001 (previous RAR5 parsing fixes)
