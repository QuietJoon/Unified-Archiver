---
type: ADR
title: "IG-010-002: Path sanitization creates directories before check"
description: "`sanitize_entry_path` calls `create_dir_all(parent)` before verifying the path is within the destination directory."
tags: [decision, R0010-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-010-002: Path sanitization creates directories before check

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0010-0002 (Review 0010)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Medium

## Location

`src/security.rs:138`

## Issue Summary

`sanitize_entry_path` calls `create_dir_all(parent)` before verifying the path is within the destination directory.

## Rationale

- By design. `canonicalize()` requires the path to exist on disk to resolve components correctly.
- Side-effects (creating parent directories) are acceptable for robust sanitization.

## Future Consideration

Investigate non-IO path normalization if it becomes a security concern.

## Related

- None
