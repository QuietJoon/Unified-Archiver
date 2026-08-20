---
type: ADR
title: "AD: Per-Entry Reopen for Parallel Extraction"
description: "Accepted"
tags: [decision, ADR-0005]
timestamp: 2026-04-12T00:00:00Z
status: active
---

# AD: Per-Entry Reopen for Parallel Extraction

Status: Accepted

## Context and Problem Statement
Safe shared mutable access to backend handles is hard across FFI and native libraries. Archive handles are not `Send`/`Sync` and wrapping them in synchronization primitives introduces complexity and deadlock risk, especially for FFI-backed handles.

## Decision Drivers
- Thread safety without shared-handle synchronization
- Straightforward concurrency model that is easy to reason about
- Practical parallelism for filtered/path/id batch extraction

## Considered Alternatives
- **Shared handle pool with synchronization** -- deferred due to complexity and risk of deadlocks or undefined behavior across FFI boundaries.

## Decision Outcome
We decided to reopen the archive per task and run extraction in rayon for batches of 4+ files, because it avoids shared-handle synchronization hazards and enables straightforward concurrency with minimal coordination.

## Consequences
- Good: Avoids shared-handle synchronization hazards and enables straightforward concurrency.
- Bad: Higher I/O and open/close overhead; progress reporting and handle reuse efficiency are weaker compared to a pooled approach.

## Amendment (2026-07-20)

The Decision Outcome above says extraction is "run ... in rayon for batches of 4+ files." **That mechanism does not exist and was never adopted.** `rayon` is not a dependency of this crate, and the library has no internal parallel-extraction path.

The shipped and intended contract is:

- `Archive` is `Send + !Sync`.
- The library spawns no threads of its own.
- Parallelism is a **caller** responsibility: a caller that wants to extract concurrently opens one handle per thread (the per-entry reopen this record describes) and drives them itself.

Internal rayon batch-parallelism was deliberately not taken — its own downside, recorded in the "Bad" bullet above (higher open/close I/O overhead plus weaker progress reporting than a pooled approach), is exactly the reason.

The batch-parallelism claim in the Decision Outcome is therefore **superseded in part by AD 0029** (`extract_some()` redesign), which removed the internal parallel path. The rest of this record stands: the `Send + !Sync` model, per-thread reopen for concurrency, and the avoidance of shared-handle / FFI synchronization remain the design.
