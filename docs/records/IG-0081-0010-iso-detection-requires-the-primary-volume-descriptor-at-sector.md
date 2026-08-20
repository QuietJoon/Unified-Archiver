---
type: ADR
title: "IG-0081-0010: ISO detection requires the primary volume descriptor at sector 16"
description: "The ISO detector checks the descriptor type/version + `CD001` at sector 16 and does not walk subsequent volume descriptors, so a boot-descriptor-first image with a non-`.iso` name is missed by content detection."
tags: [decision, R0081-0010]
timestamp: 2026-07-18T00:00:00Z
status: active
---

# IG-0081-0010: ISO detection requires the primary volume descriptor at sector 16

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0081-0010 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** REJECT
- **Severity:** High (claimed); Low (assessed)

## Location

`src/format.rs` (ISO branch of `detect_from_bytes`)

## Issue Summary

The ISO detector checks the descriptor type/version + `CD001` at sector 16 and does not walk
subsequent volume descriptors, so a boot-descriptor-first image with a non-`.iso` name is missed by
content detection.

## Rationale

Re-litigates R0080-0077 (Review 0080, landed 2026-07-17), which *deliberately* chose the
single-sector type/version check over a descriptor walk. `.iso`-named images still detect via
extension fallback, and the missed case (boot-first + non-ISO extension relying on content
detection) is an already-documented accepted false-negative
(`test_detect_iso_via_pvd_without_iso_extension_misses`). A bounded descriptor-walk enhancement may
be revisited later but is not a defect against the current decision.

## Related

- R0080-0077 (the same-day decision this re-raises)
