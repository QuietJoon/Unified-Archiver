---
type: ADR
title: "IG-005-01: SFX open_at_offset unimplemented"
description: "`open_sfx` always fails because the underlying `open_at_offset` is unimplemented."
tags: [decision, R005-01]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# IG-005-01: SFX open_at_offset unimplemented

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R005-01 (Review 0005)
- **Date:** 2025-01-07
- **Decision:** REJECT
- **Severity:** Medium

## Location

`src/archive.rs:234`

## Issue Summary

`open_sfx` always fails because the underlying `open_at_offset` is unimplemented.

## Rationale

- Known limitation.
- Implementing offset-based opening requires significant changes to all backends.
- A documented workaround (`extract_stub()` to create a temp archive) exists.

## Future Consideration

Long-term architectural goal.

## Related

- None
