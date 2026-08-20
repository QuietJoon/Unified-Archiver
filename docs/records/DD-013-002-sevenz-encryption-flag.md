---
type: ADR
title: "DD-013-002: SevenZ Encryption Flag"
description: "The current implementation incorrectly ties the `is_encrypted` flag to the presence of a password in `ExtractionOptions`, rather than checking the actual archive metadata."
tags: [decision, R0013-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-013-002: SevenZ Encryption Flag

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0013-0002
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Pending

## Rationale

The current implementation incorrectly ties the `is_encrypted` flag to the presence of a password in `ExtractionOptions`, rather than checking the actual archive metadata. This leads to incorrect metadata reporting.

## Implementation

Pending manual investigation of the `sevenz-rust2` API to retrieve correct encryption status.

## Files Modified

- `src/ffi/sevenz_wrapper.rs`

## Related

- Open_Issues.md
