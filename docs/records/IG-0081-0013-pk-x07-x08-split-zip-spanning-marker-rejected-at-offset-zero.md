---
type: ADR
title: "IG-0081-0013: `PK\\x07\\x08` split-ZIP spanning marker rejected at offset zero"
description: "`PK\\x07\\x08` is excluded from ZIP start-of-file detection; it is also the spanning marker at the start of a split archive's first segment."
tags: [decision, R0081-0013]
timestamp: 2026-07-18T00:00:00Z
status: active
---

# IG-0081-0013: `PK\x07\x08` split-ZIP spanning marker rejected at offset zero

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0081-0013 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** REJECT
- **Severity:** High (claimed)

## Location

`src/format.rs` (ZIP branch of `detect_from_bytes`)

## Issue Summary

`PK\x07\x08` is excluded from ZIP start-of-file detection; it is also the spanning marker at the
start of a split archive's first segment.

## Rationale

Re-litigates R0080-0072 (Review 0080, landed 2026-07-17), which removed `PK\x07\x08` because it is
primarily an intra-stream data-descriptor signature and was misclassifying arbitrary data. The
library does not support split-ZIP extraction end-to-end (`src/inspection.rs`), and the openable
segment of a split set is the trailing `.zip` (EOCD), not the `.z01` first volume — so detecting a
`PK\x07\x08`-prefixed first volume would route to a backend that cannot extract it. Not a real
regression. If split-ZIP reading is ever implemented, disambiguate by requiring a following
`PK\x03\x04` — tracked as a dependency of that future feature, not reopened here.

## Related

- R0080-0072 (the same-day decision this re-raises)
