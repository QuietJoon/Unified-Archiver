---
type: DCR
title: "DCR-005: UnRAR listing memoised via fresh-handle walk (AD 0065 correction)"
description: "AD 0065 recorded that UnRAR \"already satisfied\" the frozen-view caching"
tags: [change, DCR-005, ADR-0065, R0079-0001, OI-0065-003, OI-0075-002]
timestamp: 2026-06-11T00:00:00Z
status: active
---

# DCR-005: UnRAR listing memoised via fresh-handle walk (AD 0065 correction)

- **Date:** 2026-06-11
- **Source:** Review 0079, Issue R0079-0001
- **Affected ADRs:** AD-0065-backend-caching-baseline-option-a.md (updated)

## What Changed

AD 0065 recorded that `UnRAR` "already satisfied" the frozen-view caching
contract because the wrapper keeps its FFI handle alive. Review 0079 showed
that claim was wrong: the UnRAR header walk *consumes* the open handle, so
any second walk on the same handle silently yields zero entries. The facade
cache masked this for paired `Archive::list_files()` calls, but the uncached
`list_files_for_limits` path (used by `is_encrypted()` and re-exported on
`ReadArchive`) exhausted the handle — producing order-dependent wrong
answers and permanently poisoning the facade `entry_cache` with an empty
listing.

`UnrarArchive` now carries a `OnceCell<Vec<ArchiveEntry>>` listing cache
populated by a walk over a dedicated fresh handle (the same self-defense the
extraction paths already used). Every listing call — cached facade path or
not — returns the full, identical listing regardless of call order. UnRAR
now genuinely meets AD 0065 Option A instead of accidentally appearing to.

## Why

Correctness fix for order-dependent listing corruption; also makes the AD
0065 "every read backend memoises at first use" contract factually uniform.
Full rationale in the updated AD and in Review 0079 (R0079-0001).

## Affected Areas

- src/ffi/wrapper.rs (`UnrarArchive::listing`, `list_files`, fresh-handle walk)
- AD-0065-backend-caching-baseline-option-a.md

## Migration / Follow-up

- OI-0065-003 (Arc-based listing cache, two-layer caching memory tradeoff)
  now applies to UnRAR as well: the backend `OnceCell` plus the facade
  `entry_cache` pin the RAR listing twice, same as Libarchive/Piz.
- OI-0075-002's resolution note ("UnRAR already satisfied the contract")
  reflected the masked behavior; the regression tests added with this change
  cover the previously-untested uncached path.
