---
type: ADR
title: "IG-014-003: Redundant extract_to_stream implementation"
description: "`extract_to_stream` implies streaming but currently just wraps `extract_to_memory` result in a Cursor."
tags: [decision, R0014-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-014-003: Redundant extract_to_stream implementation

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0014-0003 (Review 0014)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/archive.rs`

## Issue Summary

`extract_to_stream` implies streaming but currently just wraps `extract_to_memory` result in a Cursor.

## Rationale

- Duplicate of R004-01 (IG-004-01).
- Known limitation documented in OpenIssues.md.
- Refactoring deferred until true streaming is implemented.

## Future Consideration

Implement true streaming support in backends.

## Related

- IG-004-01
