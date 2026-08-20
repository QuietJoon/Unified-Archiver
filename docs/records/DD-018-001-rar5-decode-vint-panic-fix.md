---
type: ADR
title: "DD-018-001: RAR5 decode_vint Panic Fix"
description: "Critical panic vulnerability in RAR5 variable-length integer decoding."
tags: [decision, R0018-0001]
timestamp: 2026-01-12T00:00:00Z
status: active
---

# DD-018-001: RAR5 decode_vint Panic Fix

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0018-0001 (Review 0018)
- **Date:** 2026-01-12
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

Critical panic vulnerability in RAR5 variable-length integer decoding. The expression `7 - bytes_needed` could underflow if `bytes_needed > 7`, causing a panic in debug builds or undefined behavior in release builds. Malicious or corrupted RAR5 archives could trigger this.

## Implementation

Used `(7usize).saturating_sub(bytes_needed)` to safely handle edge cases without panic.

## Files Modified

- `src/ffi/wrapper.rs` - Line 956

## Related

- Security issue (DoS via panic)
