---
type: ADR
title: "AD: Early link rejection in modify pipeline"
description: "Archived 2026-08-07 — the ruling was later inverted: the FR-022 policy silently drops link/special entries during the modify rewrite (Option 2, the option this record rejected). See the 2026-08-07 amendment."
tags: [decision, ADR-0005, R0051-0007]
timestamp: 2026-04-23T00:00:00Z
status: archived
---

# AD: Early link rejection in modify pipeline

## Context and Problem Statement

Found in Review 0051 (Issue R0051-0007, Severity: High).
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

Status: Archived 2026-08-07 — the ruling was inverted by the later FR-022 drop policy; see the amendment at the end.

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

## Amendment (2026-08-07, indy-review-prune — ruling inverted by the FR-022 drop policy; archived)

The tree now implements **Option 2, the option this record rejected**. `commit_changes` does not
pre-scan and reject retained link entries; `rewrite_drops_entry_type` (`src/modification.rs`,
"FR-022 link-skip predicate: entry types the modify rewrite drops entirely") silently drops
Symlink/HardLink/Other entries during the retained-entry replay, and `validate_pending_commit`'s
namespace gate is built on the same drop assumption so the dry-run models the namespace the commit
will produce (R0079-0037; the policy trail runs through R0070-0006/0007). The error variant this
record's outcome names cannot exist either: `ArchiveError::UnsupportedOperation` was removed by
the OI-0001-003 variant split (commit `c9b6685`, 2026-04-17 — see MADR-0011's amendment). No single
record claims the reversal — the governing artifacts are the review findings and the FR-022 policy
statements — which is why this record is **archived** rather than superseded-by-id. The
fail-fast-vs-drop trade-off it documents remains the best statement of the road not taken; whether
the silent drop should become a refusal again is the open owner question tracked by TicGit
`208978e9` and `docs/backlog.md` ("Modify rewrite silently drops symlink, hardlink and special
entries"). The lossiness is documented user-side in `Limitations.md` §2 and the user manual.
