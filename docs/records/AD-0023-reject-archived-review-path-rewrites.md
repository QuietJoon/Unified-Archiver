---
type: ADR
title: "AD 0023: Reject path-reference rewrites inside archived review documents"
description: "Implemented (no changes required); archived — imported from the pre-consolidation decisions/archive/ directory, no successor exists. Its ruling (append-only corrections to archived documents) still informs store governance."
tags: [decision, ADR-0023, ADR-0022, R0044-0036, R0044-0128, rejected]
timestamp: 2026-04-23T00:00:00Z
status: archived
---

# AD 0023: Reject path-reference rewrites inside archived review documents

## Context and Problem Statement

Found in Review 0044 (Issues R0044-0036 through R0044-0128, Severity: Low/Medium).
Locations: 92 individual file:line pointers inside `reviews/reviewed/*.md`.

Review 0044 flagged that many already-archived review documents under
`reviews/reviewed/` reference source paths in forms like `ffi/piz_wrapper.rs`
instead of the canonical `src/ffi/piz_wrapper.rs`. The reviewer recommended
rewriting each reference to the current canonical form.

## Decision Drivers

- Archived reviews are **historical records**. They capture what the
  reviewer wrote at a specific point in time.
- Large churn (92 edits across ~15 archived files) with no user-facing or
  downstream-tooling benefit.
- Rewriting archived records breaks the archival contract: future readers
  expect archived text to be unmodified.
- The same suggestion will recur any time the source layout changes, creating
  a perpetual maintenance cost for files nobody reads except during audits.

## Considered Options

1. Accept and rewrite all 92 references to canonical `src/…` form.
2. Accept but prefix each file with a "note: paths were shortened in the
   original review" banner.
3. Reject and document the rationale so the same 92 issues do not recur.

## Decision Outcome

REJECT. We chose option 3 because archived reviews are history; the
original short-form paths (`ffi/piz_wrapper.rs`, etc.) were unambiguous when
written because `src/` was the only module root in the crate. Preserving the
original text keeps the archival honest.

Status: Implemented (no changes required).

### Implementation

No code or doc edits beyond this record. Reviewers should, going forward:

- Write paths in current canonical form (`src/…`) in **new** review reports.
- Leave archived reviews untouched unless the archived content is actually
  wrong (different source file than the one the review analyzed, not merely
  a different spelling of the same path).

## Consequences

- Good: archival integrity preserved; reviewers don't burn cycles on
  low-value path churn; future reviews won't re-flag the same 92 items.
- Bad: readers of archived reviews must occasionally resolve short paths
  mentally. Manageable because `src/` is the only sensible root.

## References

- Review 0044: `reviews/reviewed/044.md`
- Related: AD 0022 (reject documentation restructuring from Reviews 042/043)
