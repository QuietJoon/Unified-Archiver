---
type: ADR
title: "IG-004-05: bzip2 full file read"
description: "`extract_bzip2_stream_crc` reads the entire file into memory to find the CRC at the end."
tags: [decision, R004-05]
timestamp: 2025-01-07T00:00:00Z
status: active
---

# IG-004-05: bzip2 full file read

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R004-05 (Review 0004)
- **Date:** 2025-01-07
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/stream_crc.rs:96`

## Issue Summary

`extract_bzip2_stream_crc` reads the entire file into memory to find the CRC at the end.

## Rationale

- Optimization deferred.
- Bounded end-scan would be complex and bzip2 usage is infrequent in the target applications.
- Documented as performance issue.

## Future Consideration

None

## Related

- None
