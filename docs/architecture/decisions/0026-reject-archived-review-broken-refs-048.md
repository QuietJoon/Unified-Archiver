# AD 0026: Reject archived review broken-path references (Review 048)

## Context and Problem Statement
Found in Review 048 (Issues R048-013 through R048-126, Severity: Low).
Locations: 114 individual file:line pointers inside `reviews/reviewed/044.md`.

Review 048 flagged that the archived review document `reviews/reviewed/044.md`
references source paths that do not resolve from the repository root (e.g.,
`ffi/wrapper.rs`, `ffi/piz_wrapper.rs`). The reviewer recommended rewriting
each reference to the current repo-root path.

## Decision Drivers

- Archived reviews are **historical records** — they capture what the
  reviewer wrote at a specific point in time.
- AD 0023, AD 0024, and AD 0025 established the policy that archived review
  path references must not be rewritten (Reviews 044, 046, and 047
  respectively).
- 114 edits in a single archived file with no user-facing or tooling benefit.
- This is the fourth consecutive review raising the identical class of issue,
  confirming the perpetual-maintenance-cost pattern.

## Considered Options

1. Accept and rewrite all 114 references to canonical form.
2. Reject per established policy (AD 0023, AD 0024, AD 0025).

## Decision Outcome

REJECT. We chose option 2 because this is the same class of issue rejected
three times before. Archived reviews are history; original short-form paths
were unambiguous when written. Preserving the original text keeps the archival
honest.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record.

## Consequences

- Good: archival integrity preserved; no wasted effort on low-value path churn.
- Bad: readers of archived reviews must occasionally resolve short paths
  mentally. Manageable because `src/` is the only sensible module root.

## References

- Review 048: `reviews/reviewed/048.md` (after archival)
- AD 0023 (Review 044 — same policy)
- AD 0024 (Review 046 — same policy)
- AD 0025 (Review 047 — same policy)
