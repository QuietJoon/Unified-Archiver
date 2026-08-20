---
type: ADR
title: "IG-0081-0050: ZIP Extended-Timestamp uses signed 32-bit seconds (2038 ceiling)"
description: "The finding claims the `0x5455` seconds field is unsigned and that valid post-2038 dates are being wrongly discarded by the signed-`i32` handling."
tags: [decision, R0081-0050]
timestamp: 2026-07-18T00:00:00Z
status: active
---

# IG-0081-0050: ZIP Extended-Timestamp uses signed 32-bit seconds (2038 ceiling)

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0081-0050 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** REJECT
- **Severity:** Medium (claimed)

## Location

`src/ffi/zip_writer.rs` (`0x5455` Extended-Timestamp encoding)

## Issue Summary

The finding claims the `0x5455` seconds field is unsigned and that valid post-2038 dates are being
wrongly discarded by the signed-`i32` handling.

## Rationale

The premise is incorrect. The Info-ZIP Extended-Timestamp (extrafld.txt) defines the ModTime/AcTime/
CrTime fields as `time_t` values, which are **signed** — this is the origin of the ZIP Year-2038
boundary. Info-ZIP `unzip` and libzip sign-extend the value; encoding it as `u32` would make
post-2038 timestamps read back as negative (pre-1970) in those readers — a regression, not a fix.
The current signed-`i32` handling with a documented pre-2038 ceiling is spec-conformant and matches
the deliberate documented choice in commit d06adea ("2038 caveat") and the signed-conversion
direction of R0080-0067/0068. If a lenient unsigned dialect is ever wanted it must be an explicit
opt-in with a reader-compat caveat, never a silent change.

## Related

- commit d06adea (documented 2038 caveat), R0080-0067/0068 (signed time conversion)
