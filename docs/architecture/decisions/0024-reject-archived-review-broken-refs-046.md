# AD 0024: Reject broken-reference rewrites inside archived review documents

## Context and Problem Statement

Found in Review 046 (Issues R046-003 through R046-126, Severity: Low).
Locations: 124 individual file:line pointers inside `reviews/reviewed/*.md`.

Review 046 flagged that many archived review documents reference files that
no longer exist at their cited paths (`reviews/Ignore.md`, `reviews/Decision.md`,
`reviews/NNN.patch`, `contracts/creation.md`, `tests/multipart_test.rs`, etc.).
The reviewer recommended retargeting each reference to the current canonical
artifact or adding archival notes.

## Decision Drivers

- Archived reviews are **historical records**. They capture what the
  reviewer wrote at a specific point in time.
- Large churn (124 edits across ~20 archived files) with no user-facing or
  downstream-tooling benefit.
- Rewriting archived records breaks the archival contract: future readers
  expect archived text to be unmodified.
- Files referenced by archived reviews were valid when written; they were
  later moved, renamed, or deleted as the project evolved.
- The same suggestion will recur after every file rename or deletion,
  creating a perpetual maintenance cost for files nobody reads except
  during audits.
- AD 0023 already established the precedent for this class of rejection.

## Considered Options

1. Accept and rewrite all 124 references to current paths or add archival notes.
2. Reject and document the rationale so the same issues do not recur.

## Decision Outcome

REJECT. We chose option 2 for the same reasons as AD 0023: archived reviews
are history. The referenced paths were valid at the time of writing. Preserving
the original text keeps the archival honest.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record. This extends AD 0023's coverage to
Review 046's broader set of broken references (not just `src/` path prefixes,
but also deleted files like `reviews/Ignore.md`, `contracts/creation.md`,
`tests/multipart_test.rs`, etc.).

## Consequences

- Good: archival integrity preserved; no perpetual maintenance of dead
  cross-references in historical documents.
- Bad: readers of archived reviews must accept that some cited paths no
  longer resolve. This is expected for any versioned document archive.

## References

- Review 046: `reviews/reviewed/046.md`
- AD 0023: reject path-reference rewrites (Review 044)
- AD 0022: reject documentation restructuring (Reviews 042/043)
