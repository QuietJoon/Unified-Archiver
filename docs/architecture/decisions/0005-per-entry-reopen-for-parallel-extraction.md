# ADR: Per-Entry Reopen for Parallel Extraction

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
