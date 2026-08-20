---
type: ADR
title: "IG-011-003: Empty directories not preserved"
description: "Empty directories are not preserved when creating archives using libarchive backend."
tags: [decision, R0011-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-011-003: Empty directories not preserved

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0011-0003 (Review 0011)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/ffi/libarchive_wrapper.rs:491`

## Issue Summary

Empty directories are not preserved when creating archives using libarchive backend.

## Rationale

- Feature request with no patch.
- Most use cases focus on file preservation.

## Future Consideration

None

## Related

- None
