# AD 0028: Reject archived review broken-path references (Review 050)

## Context and Problem Statement
Found in Review 050 (Issues R050-033 through R050-126, Severity: Low).
Locations: 94 individual file:line pointers inside `reviews/reviewed/*.md` and `docs/decisions/*.md`.

Review 050 flagged that archived review documents and historical decision records
reference source paths that do not resolve from the repository root.

## Decision Drivers

- Archived reviews are **historical records** — they capture what the
  reviewer wrote at a specific point in time.
- AD 0023, AD 0024, AD 0025, AD 0026, and AD 0027 established the policy that
  archived review path references must not be rewritten (Reviews 044, 046,
  047, 048, and 049 respectively).
- 94 edits across archived files with no user-facing or tooling benefit.
- This is the sixth consecutive review raising the identical class of issue.

## Considered Options

1. Accept and rewrite all 94 references to canonical form.
2. Reject per established policy (AD 0023–0027).

## Decision Outcome

REJECT. We chose option 2. Same class of issue rejected five times before.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record.

## Consequences

- Good: archival integrity preserved; perpetual maintenance cost avoided.
- Bad: readers of archived reviews must resolve short paths manually.

## References

- Review 050: `reviews/reviewed/050.md` (after archival)
- AD 0023 (Review 044), AD 0024 (Review 046), AD 0025 (Review 047), AD 0026 (Review 048), AD 0027 (Review 049)
