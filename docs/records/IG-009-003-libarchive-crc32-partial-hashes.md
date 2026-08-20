---
type: ADR
title: "IG-009-003: Libarchive CRC32 partial hashes"
description: "CRC32 listing can return partial hashes if a read error occurs mid-file during listing."
tags: [decision, R0009-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-009-003: Libarchive CRC32 partial hashes

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0009-0003 (Review 0009)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/ffi/libarchive_wrapper.rs:145`

## Issue Summary

CRC32 listing can return partial hashes if a read error occurs mid-file during listing.

## Rationale

- Informational only, low impact.
- CRC32 in listing is already an "extra" feature for libarchive (which doesn't expose them in headers).

## Future Consideration

None

## Related

- None
