---
type: ADR
title: "AD: RAR extraction progress pre-scan uses an isolated handle"
description: "Implemented (2026-04-14)."
tags: [decision, ADR-0002, ADR-0021, ADR-0019, R0045-0001]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: RAR extraction progress pre-scan uses an isolated handle

## Context and Problem Statement

Found in Review 0045 (Issue R0045-0001, Severity: Medium).
Location: `src/ffi/wrapper.rs::UnrarArchive::extract_all_with_options`.

The pre-scan that computes `total_bytes` for the progress callback called
`self.list_files()` directly against `self.handle`. UnRAR handles are
single-pass: prior calls (notably `list_files_for_limits()` performed during
`Archive::open`) leave the underlying handle EOF-positioned. The pre-scan
therefore returned an empty list and `total_bytes == 0`, breaking the
progress contract — every callback reported `Some(0)` as the total.

The wrapper already had the right primitive (`fresh_handle()`), and the
*post*-scan/extraction path already used it; only the pre-scan was wrong.

## Decision Drivers

* Per AD 0021, creation-side progress fires per-entry without a meaningful
  total because the producer can't pre-size; for extraction we *can* compute
  total bytes by walking headers, so we should preserve that signal rather
  than collapsing to an unknown total.
* UnRAR handles are stateful — they cannot be rewound. The only correct way
  to "list, then extract" is to open two handles.
* The fix must not introduce a re-entrant lock acquisition with `UNRAR_LOCK`
  (AD 0019). `fresh_handle()` already isolates each `unsafe` block.

## Considered Options

1. Compute `total_bytes` from a fresh `fresh_handle()` pre-scan; extraction
   still uses its own `fresh_handle()` afterward.
2. Cache the entry list on `Archive` itself and consume it during extract
   (cross-cutting refactor of the extraction path).
3. Drop the per-byte total and report `None` (regression on the public
   progress contract for RAR; users can't render a percentage).
4. Status quo (silently broken progress).

## Decision Outcome

**ACCEPT** — Option 1.

Mirrors the established pattern in the same method (`fresh_handle()` for
extraction) and is contained within `extract_all_with_options`. No public
API change, no cross-module refactor.

Status: Implemented (2026-04-14).

### Implementation

`src/ffi/wrapper.rs::UnrarArchive::extract_all_with_options`: replaced the
`self.list_files()` pre-scan with `self.fresh_handle()?.list_files()?` when
a progress callback is registered. Each `unsafe` block continues to acquire
`UNRAR_LOCK` independently per AD 0019.

Verified by full `cargo test` suite (all RAR-related tests green, including
`tests/integration/concurrency.rs::test_concurrent_rar_open_and_list`).

## Consequences

* Good — RAR extraction progress now reports a real `total` value; consumers
  can render meaningful percentages.
* Good — Pattern is consistent with the rest of `extract_all_with_options`
  and `fresh_handle()` is the documented escape hatch for UnRAR's
  single-pass handle model.
* Bad — Open + extract on a RAR with progress now opens three handles
  (initial + pre-scan + extraction) instead of two. Each `RAROpenArchiveEx`
  is bounded by `UNRAR_LOCK`, so contention is serialized but real; the
  extra cost is O(headers), bounded by archive entry count.
