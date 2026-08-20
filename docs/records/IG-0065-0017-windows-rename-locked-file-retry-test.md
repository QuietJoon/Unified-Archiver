---
type: ADR
title: "IG-0065-0017: Windows rename locked-file retry test"
description: "Reviewer recommended a Windows-only regression test that exercises the locked-destination case on `rename_with_overwrite`."
tags: [decision, R0065-0017]
timestamp: 2026-04-22T00:00:00Z
status: active
---

# IG-0065-0017: Windows rename locked-file retry test

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0065-0017 (Review 0065)
- **Date:** 2026-04-22
- **Decision:** REJECT
- **Severity:** Low

## Location

`tests/windows_rename_test.rs`, `src/modification.rs::rename_with_overwrite`

## Issue Summary

Reviewer recommended a Windows-only regression test that exercises the
locked-destination case on `rename_with_overwrite`. The reviewer's own
alternative — "or stop documenting retry-loop behavior" — is what we chose.

## Rationale

The implementation is intentionally a single `MoveFileExW` call with
`MOVEFILE_REPLACE_EXISTING` and no retry loop. A locked-destination test
would either fail (if it exercises retry) or silently decay (if
`#[ignore]`-gated). R0065-0045 (accepted in the same review) rewrites the
`persistence-and-files.md` line that falsely claimed a retry loop exists, so
the doc and code now agree.

## Future Consideration

If Windows rename reliability ever genuinely requires retry behavior, open a
fresh OI paired with the code change and its regression test.

## Related

- AD 0049 (REJECT record)
- R0065-0045 (accepted doc fix)
