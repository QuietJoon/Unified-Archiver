---
type: ADR
title: "IG-011-005: sanitize_entry_path side-effects"
description: "Potential creation of empty directories outside destination if extraction traverses a symlink."
tags: [decision, R0011-0005]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-011-005: sanitize_entry_path side-effects

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0011-0005 (Review 0011)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/security.rs:136`

## Issue Summary

Potential creation of empty directories outside destination if extraction traverses a symlink.

## Rationale

- Duplicate of R0010-0002 (IG-010-002).
- It is a trade-off for using `canonicalize`, which requires path existence.
- Mitigated by the fact that symlinks are typically skipped.

## Future Consideration

None

## Related

- IG-010-002
