# AD: UnRAR FFI calls serialized behind a process-wide mutex

## Context and Problem Statement
Found in DCR-001 / OI-026-004 (Severity: HIGH).
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
