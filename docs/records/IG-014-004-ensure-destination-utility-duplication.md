---
type: ADR
title: "IG-014-004: ensure_destination utility duplication"
description: "`ensure_destination` is defined in `src/extraction.rs` but also effectively re-implemented or checked in individual backends (e.g., `src/ffi/libarchive_wrapper.rs` does `create_dir_all` again inside `extract_all`)."
tags: [decision, R0014-0004]
timestamp: 2026-01-09T00:00:00Z
status: active
---

# IG-014-004: ensure_destination utility duplication

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0014-0004 (Review 0014)
- **Date:** 2026-01-09
- **Decision:** REJECT
- **Severity:** Low

## Location

`src/extraction.rs`

## Issue Summary

`ensure_destination` is defined in `src/extraction.rs` but also effectively re-implemented or checked in individual backends (e.g., `src/ffi/libarchive_wrapper.rs` does `create_dir_all` again inside `extract_all`).

## Rationale

- Minor refactoring suggestion with no patch.
- The duplication is non-harmful and ensures robustness in backends that might be used independently.

## Future Consideration

None

## Related

- None
