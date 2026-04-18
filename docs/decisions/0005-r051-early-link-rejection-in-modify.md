# AD: Early link rejection in modify pipeline

## Context and Problem Statement

Found in Review 051 (Issue R051-007, Severity: High).
Location: `src/modification.rs` — `commit_changes()` copy loop.

The modify pipeline encountered symlink/hardlink entries mid-copy, producing
a confusing error partway through archive recreation. Since the pipeline does
not support link creation (no backend wires up link semantics during copy),
any retained link entry is guaranteed to fail. Failing late — after partial
work — wastes I/O and leaves the user with an unclear error.

## Decision Drivers

* Fail-fast principle: reject impossible work before starting expensive I/O.
* Link support in modify is a separate feature (no backend supports it today).
* The late-failure error message was generic; an early check can explain the
  specific problem and suggest removing the entry.

## Considered Options

1. Pre-scan retained entries before the copy loop; reject immediately if any
   symlink or hardlink entries remain after removals are applied.
2. Silently skip link entries during copy (lossy but non-failing).
3. Status quo (fail mid-copy with a generic error).

## Decision Outcome

**ACCEPT** — Option 1.

A pre-scan loop runs after `list_files()` and after filtering out removed
entries. If any retained entry has `EntryType::Symlink` or `EntryType::HardLink`,
the method returns `UnsupportedOperation` with a message explaining which entry
caused the rejection and advising the caller to remove it before committing.

The dead match arm for link entries in the copy loop was removed.

Status: Implemented (2026-04-15).

### Implementation

`src/modification.rs` — pre-scan added after `let entries = self.list_files()?;`
and before the copy loop. The old `Symlink | HardLink` arm in the copy loop
was deleted.

## Consequences

* Good — fail-fast with a clear, actionable error message.
* Good — removes dead code path that could never succeed.
* Bad — archives containing links that the caller forgot to remove now fail
  upfront instead of partially succeeding; this is the intended behavior but
  is a behavioral change for any caller that relied on partial output.
