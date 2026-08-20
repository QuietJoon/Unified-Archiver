---
type: ADR
title: "AD 0041: Remove hardcoded `/Volumes/Temp/claude` from library and examples"
description: "Review 0062 raised three findings about a host-specific path leaking into shipped artefacts:"
tags: [decision, ADR-0041, R0062-0003, R0062-0008, R0062-0009]
timestamp: 2026-04-18T00:00:00Z
status: active
---

# AD 0041: Remove hardcoded `/Volumes/Temp/claude` from library and examples

## Context and Problem Statement

Review 0062 raised three findings about a host-specific path leaking into shipped artefacts:

* **R0062-0003** — `Archive::open_at_offset()` preferred `/Volumes/Temp/claude` as its temp parent whenever that path existed on the host. Runtime behaviour of a library API depended on a filesystem layout that only exists on one developer's machine.
* **R0062-0008** — `examples/create_archive.rs` wrote its sample archives to `/Volumes/Temp/claude/example/…`.
* **R0062-0009** — `examples/modify_archive.rs` wrote its sample archive to the same path.

`/Volumes/Temp/claude` is a developer convention documented in `CLAUDE.md` for *scratch* work during development and testing. Library code and example code running on other machines is not covered by that convention, and in both cases the path was unconditionally hardcoded.

## Decision Drivers

* Library API behaviour must be a function of its inputs and the documented OS temp contract (`TMPDIR` / `std::env::temp_dir()`), not of a specific filesystem layout.
* Example programs must run on any machine that can build the crate, not only on this project's development host.
* The `CLAUDE.md` convention for `/Volumes/Temp/claude` explicitly scopes to **test code**; it does not extend to production library code or to shipped examples.

## Considered Options

1. **Keep the `/Volumes/Temp/claude`-preference, documented as development-only.** Rejected: it silently changes behaviour on one host vs. every other host, which is exactly the anti-pattern the review flagged.
2. **Use `std::env::temp_dir()` directly.** Rejected: that returns a long-lived path with no automatic cleanup, which is wrong for the tempfile that `open_at_offset()` creates and wrong for example output that should be cleaned up on exit.
3. **Use `tempfile::tempdir()` / `tempfile::Builder::new().tempfile()`.** Accepted — these RAII types honour `TMPDIR` if set, fall back to the OS default otherwise, and clean up on drop.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

### `src/archive.rs::open_at_offset()`

The `/Volumes/Temp/claude`-preference block was removed. The method now uses `tempfile::Builder::new().tempfile()` unconditionally, which respects `TMPDIR` and falls back to the OS default. Callers who want a specific temp directory can set `TMPDIR` in their environment; no library knob is exposed.

### `examples/create_archive.rs`

`main()` now creates a `tempfile::tempdir()` at the top and threads `&Path` through the four example functions. Each function builds its output path as `base.join("example.zip")`, etc. Dropping the `TempDir` at the end of `main()` cleans up all sample output. If the tempdir cannot be created, the example returns an `ArchiveError::io` — stricter than the old hardcoded path, which would silently succeed on the dev host and silently fail elsewhere.

### `examples/modify_archive.rs`

Same pattern: a single `tempfile::tempdir()` in `main()`, `&Path` threaded through the four example functions. The sample archive is created in the tempdir and modified in place; the tempdir is dropped at the end of `main()`.

### Test scope preserved

Test code is untouched — tests continue to use `common::temp_test_dir()` (PID+ns unique, project-rooted per `CLAUDE.md`). This decision is scoped to library code and examples only.

### TMPDIR for tests

The project's test suite sets `TMPDIR=/Volumes/Temp/claude` before running tests (see `CLAUDE.md`). That is the explicit dev-only escape hatch; `open_at_offset()`'s `tempfile::Builder` picks up the same envvar, so test runs on this host continue to land in the same scratch volume without the library having to know about it.

## Consequences

* The library's temp-path behaviour is now a function of `TMPDIR` only, with the OS default as fallback. No machine-specific preference remains.
* Examples run on any machine that can build the crate.
* Test runs that need `/Volumes/Temp/claude` rely on the caller (or the test harness) exporting `TMPDIR` — this is already in place for the project's Makefile/CI recipes.

## Revisit trigger

Re-open if:

* A public API knob for specifying the temp parent becomes necessary (e.g. users on Windows needing a specific non-default temp volume).
* The `tempfile` crate's behaviour around `TMPDIR` changes in a way that affects our RAII assumptions.
