# AD 0031: Reject cross-document caveat deduplication

## Context and Problem Statement
Found in Review 0005 (Issues R0005-0096 through R0005-0119, Severity: Low).

Review 0005 filed 36 separate issues requesting that repeated caveat text (RAR concurrency partial-resolution, ZIP modify reliability, SFX confidence policy, SecStr password debt) be centralized into a single canonical source and replaced with cross-links in all other documents.

## Decision Drivers

* Each document in the project is designed to be read independently (specs, contracts, architecture, planning, and status docs serve different audiences)
* Cross-linking creates a dependency graph between documents that is harder to maintain than the repetition it removes
* Canonical trackers already exist: OI-026-004 (RAR), DEF-005 (ZIP modify), AD 0015 (SFX confidence), OI-050-026 (SecStr)
* Deduplication across 12-36 independently-authored documents adds significant edit churn with proportionally low value

## Considered Options

1. Accept all deduplication issues — centralize each caveat into one canonical source and replace all other instances with cross-links.
2. Reject all deduplication issues — keep independent documents self-contained with their own caveat text.
3. Hybrid — centralize only when a document's primary purpose is tracking that issue; keep self-contained caveats elsewhere.

## Decision Outcome

REJECT (option 2). Repeated caveat text across independently-read documents is intentional. Each document needs self-contained context for its own readers. The canonical trackers (OI entries, DEF items, ADRs) are the authoritative sources; repeated prose in other documents is descriptive context, not normative status.

Status: Implemented.

### Implementation

No changes made. The 36 issues were rejected as a category.

## Consequences

- Good: no edit churn across 30+ documents for a pattern that will naturally recur with each new caveat
- Good: each document remains self-contained and readable without cross-references
- Neutral: future reviewers may raise the same concern; this record explains the rationale
- Bad: caveat text may drift between documents over time (mitigated by the canonical tracker being authoritative)

## References

- R0005-0096 through R0005-0107 (RAR concurrency caveat, 12 issues)
- R0005-0108 through R0005-0114 (ZIP modify caveat, 7 issues)
- R0005-0115 through R0005-0119 (SFX confidence policy, 5 issues)
- R0005-0078 through R0005-0095 (SecStr password cross-links, 18 issues — partially overlapping, 12 rejected on same grounds)
