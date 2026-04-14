# AD 0025: Reject archived review broken-path references (Review 047)

## Context and Problem Statement
Found in Review 047 (Issues R047-004 through R047-126, Severity: Low).
Locations: 123 individual file:line pointers inside `reviews/reviewed/*.md`.

Review 047 flagged that archived review documents under `reviews/reviewed/`
reference source paths that do not resolve from the repository root (e.g.,
`src/callbacks.rs`, `ffi/wrapper.rs`, `contracts/progress.md`). The reviewer
recommended rewriting each reference to the current repo-root path.

## Decision Drivers

- Archived reviews are **historical records** — they capture what the
  reviewer wrote at a specific point in time.
- AD 0023 and AD 0024 already established the policy that archived review
  path references must not be rewritten (Reviews 044 and 046 respectively).
- 123 edits across ~10 archived files with no user-facing or tooling benefit.
- The same suggestion recurs every review cycle, confirming it is a
  perpetual maintenance cost for files nobody reads except during audits.

## Considered Options

1. Accept and rewrite all 123 references to canonical form.
2. Reject per established policy (AD 0023, AD 0024).

## Decision Outcome

REJECT. We chose option 2 because this is the same class of issue rejected
twice before. Archived reviews are history; original short-form paths were
unambiguous when written. Preserving the original text keeps the archival
honest.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record.

## Consequences

- Good: archival integrity preserved; no wasted effort on low-value path churn.
- Bad: readers of archived reviews must occasionally resolve short paths
  mentally. Manageable because `src/` is the only sensible module root.

## References

- Review 047: `reviews/reviewed/047.md` (after archival)
- AD 0023 (Review 044 — same policy)
- AD 0024 (Review 046 — same policy)
