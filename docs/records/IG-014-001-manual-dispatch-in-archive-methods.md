---
type: ADR
title: "IG-014-001: Manual dispatch in Archive methods"
description: "Code duplication in `Archive` methods (`extract_to_memory`, `extract_to_stream`) which manually match on `backend` enum instead of using a trait abstraction."
tags: [decision, R0014-0001]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-014-001: Manual dispatch in Archive methods

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0014-0001 (Review 0014)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/archive.rs`

## Issue Summary

Code duplication in `Archive` methods (`extract_to_memory`, `extract_to_stream`) which manually match on `backend` enum instead of using a trait abstraction.

## Rationale

- Refactoring suggestion only, no patch provided.
- The current manual dispatch is explicit and maintains performance.
- Architectural changes like a `Backend` trait are deferred.

## Future Consideration

Consider during next major refactoring of the backend architecture.

## Related

- None
