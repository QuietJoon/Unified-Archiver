---
type: ADR
title: "DD-012-002: SFX Detection Flaws"
description: "The SFX detection logic used `read` which could return short reads, and validated offsets against buffer size rather than file size, leading to potential false negatives."
tags: [decision, R0012-0002]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# DD-012-002: SFX Detection Flaws

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Decisions.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0012-0002
- **Date:** 2026-01-09
- **Decision:** ACCEPT
- **Status:** Implemented

## Rationale

The SFX detection logic used `read` which could return short reads, and validated offsets against buffer size rather than file size, leading to potential false negatives.

## Implementation

Switched to `read_to_end` with a bounded buffer and corrected offset validation logic.

## Files Modified

- `src/sfx/detection.rs`
