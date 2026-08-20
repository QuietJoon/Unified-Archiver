---
type: ADR
title: "IG-012-003: Temp dir race in extract_to_memory"
description: "Potential race condition in temp directory naming using `SystemTime::now()` if multiple calls happen in the same nanosecond."
tags: [decision, R0012-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-012-003: Temp dir race in extract_to_memory

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0012-0003 (Review 0012)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/ffi/libarchive_wrapper.rs:360`

## Issue Summary

Potential race condition in temp directory naming using `SystemTime::now()` if multiple calls happen in the same nanosecond.

## Rationale

- Nanosecond collision is extremely unlikely in practice.
- No patch provided.
- Fix would require new dependencies (e.g. `tempfile`) or broader changes.

## Future Consideration

Replace with `tempfile` crate in a future cleanup.

## Related

- None
