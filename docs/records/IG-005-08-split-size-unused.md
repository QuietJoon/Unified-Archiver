---
type: ADR
title: "IG-005-08: split_size unused"
description: "`CompressionOptions.split_size` is defined but ignored by archive creation."
tags: [decision, R005-08]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# IG-005-08: split_size unused

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R005-08 (Review 0005)
- **Date:** 2025-01-07
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/options.rs:79`

## Issue Summary

`CompressionOptions.split_size` is defined but ignored by archive creation.

## Rationale

- Split archive creation is a complex feature that is not currently planned for implementation.
- Documented as unsupported.

## Future Consideration

None

## Related

- None
