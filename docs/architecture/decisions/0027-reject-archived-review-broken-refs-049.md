# AD 0027: Reject archived review broken-path references (Review 049)

## Context and Problem Statement
Found in Review 049 (Issues R049-032 through R049-126, Severity: Low).
Locations: 95 individual file:line pointers inside `reviews/reviewed/045.md`.

Review 049 flagged that the archived review document `reviews/reviewed/045.md`
references source paths that do not resolve from the repository root (e.g.,
`ffi/wrapper.rs`, `external/rar.rs`, `sfx/signatures.rs`). The reviewer
recommended rewriting each reference to the current repo-root path.

## Decision Drivers

- Archived reviews are **historical records** — they capture what the
  reviewer wrote at a specific point in time.
- AD 0023, AD 0024, AD 0025, and AD 0026 established the policy that
  archived review path references must not be rewritten (Reviews 044, 046,
  047, and 048 respectively).
- 95 edits in a single archived file with no user-facing or tooling benefit.
- This is the fifth consecutive review raising the identical class of issue.

## Considered Options

1. Accept and rewrite all 95 references to canonical form.
2. Reject per established policy (AD 0023–0026).

## Decision Outcome

REJECT. We chose option 2. Same class of issue rejected four times before.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record.

## Consequences

- Good: archival integrity preserved; perpetual maintenance cost avoided.
- Bad: readers of archived reviews must resolve short paths manually.

## References

- Review 049: `reviews/reviewed/049.md` (after archival)
- AD 0023 (Review 044), AD 0024 (Review 046), AD 0025 (Review 047), AD 0026 (Review 048)
