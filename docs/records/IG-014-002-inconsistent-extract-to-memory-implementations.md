---
type: ADR
title: "IG-014-002: Inconsistent extract_to_memory implementations"
description: "UnRAR and Libarchive backends implement `extract_to_memory` by extracting to a temp file and reading it back, whereas Piz and SevenZ extract directly to memory."
tags: [decision, R0014-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-014-002: Inconsistent extract_to_memory implementations

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0014-0002 (Review 0014)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/ffi/libarchive_wrapper.rs`

## Issue Summary

UnRAR and Libarchive backends implement `extract_to_memory` by extracting to a temp file and reading it back, whereas Piz and SevenZ extract directly to memory.

## Rationale

- Duplicate of R0011-0004 (IG-011-004).
- In-memory extraction for libarchive is a performance optimization that is currently deferred.

## Future Consideration

Optimization in future release.

## Related

- IG-011-004
