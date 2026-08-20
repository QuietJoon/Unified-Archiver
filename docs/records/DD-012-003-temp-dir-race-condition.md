---
type: ADR
title: "DD-012-003: Temp Dir Race Condition"
description: "The theoretical race condition in temporary directory naming (using `SystemTime`) was deemed extremely unlikely to occur in practice."
tags: [decision, R0012-0003]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-012-003: Temp Dir Race Condition

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0012-0003
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Status:** N/A

## Rationale

The theoretical race condition in temporary directory naming (using `SystemTime`) was deemed extremely unlikely to occur in practice. Fixing it would require adding new dependencies (e.g., `tempfile`) which was out of scope.

## Implementation

See Ignores.md.

## Files Modified

- N/A
