---
type: ADR
title: "AD: Reject R0065-0017 — Windows rename locked-file retry test"
description: "Rejected R0065-0017's Windows locked-rename retry test; still active — revisit when the OI-0065-001 Windows CI job lands (amended 2026-08-04, §B)."
tags: [decision, ADR-0049, R0065-0017, R0065-0045]
timestamp: 2026-04-22T00:00:00Z
status: active
---

# AD: Reject R0065-0017 — Windows rename locked-file retry test

## Context and Problem Statement

Found in Review 0065 (Issue R0065-0017, Severity: Low).
Location: `tests/windows_rename_test.rs`, `src/modification.rs::rename_with_overwrite`

R0065-0017 recommended adding a Windows-only test that exercises the
locked-destination case so that regressions in any future retry-loop
implementation would be caught. Historically, the architecture docs
advertised a "retry loop for locked files" on the Windows rename path; the
implementation never grew that loop and is still a single `MoveFileExW` call
with `MOVEFILE_REPLACE_EXISTING`.

The reviewer's own recommendation offered the alternative path: *"or stop
documenting retry-loop behavior."*

## Decision Drivers

* Tests must describe shipped behavior, not aspirational behavior.
* A locked-destination test would fail on the current single-shot
  `MoveFileExW` implementation; adding it would create immediate red tests
  with no code path to make them green.
* R0065-0045 (ACCEPT, same review) already rewrites the doc line so it
  matches the actual one-shot rename semantics.

## Considered Options

1. Accept: implement a retry loop and add the test that exercises it.
2. Accept: add a `#[should_panic]` or `#[ignore]` placeholder test that
   documents the absence of retry.
3. **Reject**: keep the one-shot rename as shipped, rely on R0065-0045 to
   bring the docs in line with the code.

## Decision Outcome

**REJECT** (Option 3). R0065-0017 is closed without implementing a test.

Status: Closed

### Implementation

- `docs/architecture/persistence-and-files.md` updated under R0065-0045 to
  describe the single `MoveFileExW` path and surface transient sharing
  violations as `ArchiveError::Io`, removing the misleading retry-loop claim.
- No code or test changes for this finding. If future platform reliability
  demands a retry loop, raise a fresh OI with a concrete regression scenario
  and failure mode — the implementation change will be paired with the test
  that exercises it.

## Consequences

* Good, because the docs now match the code rather than the other way
  around; readers and future auditors see the real contract.
* Good, because we avoid adding a placeholder test that would either
  fail (if it exercises retry) or silently decay (if `#[ignore]`-gated).
* Bad, because a future retry-loop implementation will need both the code
  and its test landed together — this rejection does not pre-write the
  coverage.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — trigger restated)

The owner's platform directive (Review 0080 gate, 2026-07-17; recorded in OI-0080-001 and the
MADR-0029 owner-reversal amendment) makes Windows a **first-class native target**, which raises
the stakes of this rejection's subject without changing its logic. The facts the ruling rests on
still hold:

* `rename_with_overwrite` is still a **single `MoveFileExW` call with no retry loop** — now
  passing `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`. The helper has moved from
  `src/modification.rs` to `src/ffi/common.rs`, where it is shared by `AtomicOutputFile::commit`,
  modify-mode `commit_changes`, `install_staged_file` in the libarchive reader, and the extract
  path's overwrite branch, so this record's Location reference is stale on the module,
  not on the behaviour.
* `tests/windows_rename_test.rs` still exercises overwrite semantics only; no locked-destination
  test was added, consistent with this ruling.

This record's own revisit condition — "if future platform reliability demands a retry loop, raise
a fresh OI with a concrete regression scenario and failure mode" (mirrored in IG-0065-0017's
Future Consideration) — could not fire while nothing exercised the Windows rename path routinely.
That changes when the **OI-0065-001 Windows CI job** lands (committed under the directive, not yet
landed): CI will run `tests/windows_rename_test.rs` and the modify-mode swap path continuously,
producing the first systematic evidence for or against transient sharing-violation failures.
Restated trigger: **revisit this record when the OI-0065-001 Windows CI job lands** and its runs
supply — or persistently fail to supply — the concrete regression scenario the record asks for.
Until then the rejection stands and the record stays ACTIVE. Whether a retry loop is ever
warranted remains an open question for the owner; this amendment does not decide it.
