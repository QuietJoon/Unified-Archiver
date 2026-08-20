---
type: ADR
title: "AD: UnRAR FFI calls serialized behind a process-wide mutex"
description: "Implemented (Phase A.2 of the in-flight remediation plan)."
tags: [decision, ADR-0019, DCR-001, OI-0026-004]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: UnRAR FFI calls serialized behind a process-wide mutex

## Context and Problem Statement
Found in DCR-001 / OI-0026-004 (Severity: HIGH).
Location: `src/ffi/wrapper.rs`

The bundled UnRAR C library carries process-global mutable state (file-pointer registries and codec scratch buffers). Concurrent calls into `RAROpenArchiveEx`, `RARReadHeaderEx`, or `RARProcessFile` — even on *different* archive files from *different* `Archive` handles — can produce CRC mismatches, truncated reads, or undefined behavior.

`Archive` is `!Sync`, which prevents misuse of a single handle across threads, but it does **not** prevent two independent handles in two threads from racing inside UnRAR. Production users reported this under Rayon and async runtimes; tests had to pin RAR coverage to single-threaded execution as a workaround.

## Decision Drivers
* Correctness under concurrent use: callers should not have to know that RAR is special among the supported formats.
* Minimal blast radius: avoid a public-API change; avoid `unsafe`-block restructuring beyond the FFI shim.
* Throughput cost is acceptable: RAR is a minority format; serializing FFI entry points does not bottleneck the common ZIP/7z/TAR paths.
* Re-entrancy safety: `fresh_handle()` calls back into `RAROpenArchiveEx` from inside other FFI methods, so a coarse-grained method-level lock would deadlock.

## Considered Options
1. **Process-wide `static Mutex<()>` guarded around each FFI call** (chosen). Each `unsafe` block grabs the lock for its duration only.
2. **`#[serial_test::serial]` on RAR tests only.** Leaves production users exposed; rejected.
3. **Mark `Archive` as `!Send` for RAR variants.** Forces single-thread use even within one logical operation; defeats the unified-API promise.
4. **Per-handle mutex.** Doesn't help — UnRAR's mutable state is process-global, not per-handle.
5. **Spawn a dedicated UnRAR worker thread and channel work to it.** Higher engineering cost, more latency, and would require redesigning the synchronous `Archive::open` contract.

## Decision Outcome
ACCEPT: Option 1. Added `static UNRAR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());` to `src/ffi/wrapper.rs`. Acquired via `let _guard = UNRAR_LOCK.lock().unwrap_or_else(|e| e.into_inner());` immediately before each guarded `unsafe` call:
- `RAROpenArchiveEx`
- `RARSetPassword`
- `RARReadHeaderEx`
- `RARProcessFile`
- `RARCloseArchive`

Lock scope is the individual `unsafe` block (not the surrounding method) so that re-entrant paths through `fresh_handle()` do not self-deadlock. Poison recovery is unconditional because every FFI call opens its own handle and carries no cross-call invariants worth preserving.

Status: Implemented (Phase A.2 of the in-flight remediation plan).

### Implementation
- `src/ffi/wrapper.rs`: `UNRAR_LOCK` static + 8 guarded call sites.
- `tests/integration/concurrency.rs::test_concurrent_rar_open_and_list`: 8 parallel threads × 20 iterations against `tests/fixtures/test.rar` and `test_rar5.rar`, asserts zero entry-count mismatches.
- `specs/001-unified-archive/contracts/archive.md` thread-safety section updated to note the serialization is now provided by the library (rather than being the caller's responsibility).

## Consequences
* Good, because callers no longer need external coordination to use RAR safely under multi-threaded extraction or async runtimes.
* Good, because the change is local to the FFI shim — no public API impact, no behavioral change for non-RAR formats.
* Good, because the lock is fine-grained at the FFI-call level, so multi-threaded ZIP/7z work continues unhindered while RAR work serializes.
* Bad, because RAR throughput is now strictly single-threaded process-wide. For workloads that fan out across many RAR archives, this is the rate limiter. Acceptable for the MVP because RAR is read-only and a minority format.
* Bad, because a poisoned mutex is silently recovered. Justified here because each FFI call is self-contained and the prior panic — if any — happened in a separate guarded scope; the recovery cannot smuggle inconsistent state across calls.

## Amendment (2026-07-18, R0081-0063)

The R0080-0022 `UCM_PROCESSDATA` trampoline runs the caller's `ProgressCallback` **while `UNRAR_LOCK` is held** — the callback fires synchronously from inside `RARProcessFile`, on the lock-holding thread. Because the mutex is non-reentrant, a callback that starts another UnRAR operation on the *same thread* (opening or extracting a second archive) would re-acquire the lock and deadlock the whole process permanently. The original "acquired at small scopes … never re-entrantly" reasoning above no longer holds now that user code executes under the lock.

`unrar_lock()` therefore now guards against same-thread re-entry with a thread-local sentinel (`IN_UNRAR`) and returns a guard type (`UnrarLockGuard`) instead of a bare `MutexGuard`:

* First entry on a thread sets the sentinel, blocks on `UNRAR_LOCK` exactly as before, and returns `Ok(UnrarLockGuard)`. The guard clears the sentinel (and releases the mutex) on drop.
* Re-entry on the same thread — the sentinel is already set — returns an `OperationBlocked` error rather than blocking, so a re-entrant progress callback fails with a typed error instead of deadlocking.
* `try_lock` is deliberately **not** used. Legitimate cross-thread contention must still block, because UnRAR's global state is not thread-safe and the mutex exists to serialize it; turning that contention into a spurious error would be wrong. A thread-local sentinel distinguishes same-thread re-entry (error) from cross-thread contention (block).
* `Drop for UnrarArchive` keeps closing the handle even if the sentinel trips, because a re-entrant close is still correctly serialized — the thread already holds the lock — so no leak or double-lock results.

Every guarded FFI site now acquires the lock via `let _guard = unrar_lock()?;` (all callers already return `Result`); the `Drop` path binds the result without `?`.

This imposes a matching constraint on the **ProgressCallback contract**: a callback must not perform another archive operation on the archiver while extraction is in flight. Re-entering during a RAR extraction returns the typed error above instead of deadlocking. The caller-facing statement of this constraint lives on the `ProgressCallback` trait doc in `src/options.rs`; this ADR records the FFI-layer enforcement that backs it.
