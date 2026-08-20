---
type: ADR
title: "IG-020-003: UnRAR FFI Struct Layout on Linux"
description: "The code defines `wchar_t` as 2 bytes (UTF-16) for non-macOS platforms."
tags: [decision, R0020-0003]
timestamp: 2026-01-13T00:00:00Z
status: active
---

# IG-020-003: UnRAR FFI Struct Layout on Linux

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0020-0003 (Review 0020)
- **Date:** 2026-01-13
- **Decision:** REJECT
- **Severity:** High

## Location

`src/ffi/unrar.rs` - multiple lines (58, 63, 84, 92, 97, 111, 118, 122)

## Issue Summary

The code defines `wchar_t` as 2 bytes (UTF-16) for non-macOS platforms. This is only correct for Windows. On Linux and most Unix systems, `wchar_t` is 4 bytes (UTF-32). This mismatch would cause struct layout misalignment when calling into the UnRAR C++ library.

## Rationale

- No patch provided - requires careful analysis of UnRAR SDK's actual behavior
- The library currently primarily targets macOS; Linux support is not yet claimed
- The fix requires platform-specific testing infrastructure not currently available
- Risk of introducing regressions without proper test coverage

## Future Consideration

Track as a future enhancement when adding official Linux support. May require runtime detection or conditional compilation based on target platform.

## Related

- None

## Reopened & Fixed

- **2026-07-17 (Review 0080 gate, R0080-0001):** Formally reopened now
  that Windows, macOS, and Linux are all first-class targets, so the
  original REJECT rationale ("Linux support is not yet claimed") no
  longer holds. Fixed by introducing a single `RarWchar` alias (`u16`
  under `cfg(windows)`, `c_uint` otherwise) used for every wide array
  and pointer in `RARHeaderDataEx` / `RAROpenArchiveDataEx`, keying
  `WCHAR_SIZE` and the wide-string decode helpers on `windows` vs. not,
  and retyping `RARProcessFileW` (R0080-0002). The compile-time layout
  locks cross-check the width against the actual `RarWchar` element size.
