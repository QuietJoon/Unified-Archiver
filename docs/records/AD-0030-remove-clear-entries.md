---
type: ADR
title: "AD 0030: Remove `clear_entries()` — dead API surface"
description: "Implemented; archived 2026-08-07 — clear_entries() is gone from the entire tree, the ruling is discharged and cannot recur as written."
tags: [decision, ADR-0030, R0004-0010]
timestamp: 2026-04-23T00:00:00Z
status: archived
---

# AD 0030: Remove `clear_entries()` — dead API surface

## Context and Problem Statement
Found in Review 0004 (Issue R0004-0010, Severity: Low).

`Archive::clear_entries()` was a stub that always returned `ArchiveError::UnsupportedOperation` with the message "Cannot clear already-written entries from an archive." It was never implemented and could never succeed.

## Decision Drivers

* The method always returns an error — no caller can use it productively
* Dead API surface confuses users and inflates documentation
* The `CLEAR_ENTRIES` operation constant in `error::ops` existed solely for this stub

## Considered Options

1. Implement `clear_entries()` properly (clear staged entries before commit).
2. Remove the method entirely as dead API surface.

## Decision Outcome

ACCEPT option 2. The method, its test (`test_clear_entries_returns_error`), and the `ops::CLEAR_ENTRIES` constant were removed. Documentation references in `API_REFERENCE.md`, `API_REFERENCE.ko.md`, and `docs/architecture/codebase_investigation.md` were also cleaned up.

Status: Implemented.

## Consequences

- Good: smaller, cleaner API surface
- Good: removes misleading documentation suggesting the method exists
- Neutral: no callers affected (the method always errored)

## References

- R0004-0010 (Review 0004)
