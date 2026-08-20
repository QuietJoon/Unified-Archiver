---
type: ADR
title: "IG-009-002: ExtractionOptions.filter unused"
description: "`ExtractionOptions.filter` is defined but ignored by all extraction methods."
tags: [decision, R0009-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-009-002: ExtractionOptions.filter unused

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0009-0002 (Review 0009)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Medium

## Location

`src/options.rs:34`

## Issue Summary

`ExtractionOptions.filter` is defined but ignored by all extraction methods.

## Rationale

- By design.
- Filtering is intended to be handled via the explicit `extract_filtered()` method rather than a field in options, to keep the API clear.

## Future Consideration

Consider removing the field from `ExtractionOptions` to avoid confusion.

## Related

- None
