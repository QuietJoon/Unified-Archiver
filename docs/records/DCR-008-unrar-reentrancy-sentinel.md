---
type: DCR
title: "UnRAR lock gains a same-thread re-entrancy sentinel"
description: "The process-global UnRAR lock now rejects same-thread re-entry with a typed error instead of deadlocking, after the UCM_PROCESSDATA trampoline began running user callbacks under the lock."
tags: [change, project-control, DCR-008]
status: active
---

# DCR-008: UnRAR lock gains a same-thread re-entrancy sentinel

- **Date:** 2026-07-18
- **Source:** Review 0081, Issue R0081-0063
- **Affected ADRs:** AD-0019-unrar-process-wide-mutex.md (updated — amendment)

## What Changed

R0080-0022 (Review 0080) added the `UCM_PROCESSDATA` trampoline so UnRAR extraction can cancel
mid-entry and enforce byte caps — but the trampoline invokes the caller's `ProgressCallback` while
the process-global, non-reentrant `std::sync::Mutex` `UNRAR_LOCK` is held across `RARProcessFile`.
A callback that started another UnRAR operation on the same thread would re-acquire the lock and
deadlock the whole process permanently. AD 0019's original "acquired at small scopes, never
re-entrantly" reasoning only covered `fresh_handle()`, not user code executing under the lock.

`unrar_lock()` now guards against same-thread re-entry with a thread-local sentinel (`IN_UNRAR`):
the first acquisition on a thread sets the flag and takes the mutex (still **blocking**, so
cross-thread serialization — the mutex's actual purpose, since UnRAR is not thread-safe — is
preserved); a same-thread re-entry finds the flag set and returns an `OperationBlocked`
"re-entrant UnRAR access" error instead of deadlocking. `try_lock` was deliberately rejected — it
would convert legitimate cross-thread contention into spurious errors. The guard's `Drop` clears
the flag and releases the mutex. `unrar_lock()`'s signature changed to return `Result`; call sites
use `?`.

## Why

A silent, permanent, process-wide deadlock reachable from a documented public callback is worse
than a typed error. The sentinel also prevents the unsafe recursive `RARProcessFile` call a
reentrant mutex would have allowed. Full rationale in the AD 0019 amendment.

## Affected Areas

- src/ffi/wrapper.rs (`unrar_lock`, the `IN_UNRAR` sentinel, all lock call sites)
- src/options.rs (`ProgressCallback` rustdoc — the re-entrancy constraint)
- AD-0019-unrar-process-wide-mutex.md (amendment)

## Migration / Follow-up

`ProgressCallback` implementations must not perform archive operations from `on_progress`; the docs
now state this. No other caller impact — the error only surfaces for genuinely re-entrant misuse.
