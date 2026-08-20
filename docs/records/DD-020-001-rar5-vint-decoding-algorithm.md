---
type: ADR
title: "DD-020-001: RAR5 vint Decoding Algorithm"
description: "Critical algorithm error - the custom \"leading bits\" implementation was incompatible with RAR5's actual LEB128 encoding."
tags: [decision, R0020-0001]
timestamp: 2026-01-13T00:00:00Z
status: active
---

# DD-020-001: RAR5 vint Decoding Algorithm

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0020-0001 (Review 0020)
- **Date:** 2026-01-13
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Critical algorithm error - the custom "leading bits" implementation was incompatible with RAR5's actual LEB128 encoding. This would cause failure to parse recovery records and other metadata in RAR5 archives.

## Implementation

Replaced with correct little-endian LEB128 decoding that uses continuation flag (0x80) and 7 data bits per byte.

## Files Modified

- `src/ffi/wrapper.rs` - Lines 923-950

## Related

- DD-018-001 (previous vint panic fix - now superseded by complete rewrite)
