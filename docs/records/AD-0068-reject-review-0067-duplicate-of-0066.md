---
type: ADR
title: "AD 0068: Reject Review 0067 as byte-identical duplicate of Review 0066"
description: "Implemented (both reviews archived to reviews/reviewed/); archived 2026-08-07 — the ruling is discharged and nothing ever superseded it; the 2026-07-23 migration had mislabelled it superseded. Only in-repo explanation of why the review series skips 0067."
tags: [decision, ADR-0051, ADR-0025, rejected]
timestamp: 2026-04-28T00:00:00Z
status: archived
---

# AD 0051: Reject Review 0067 as byte-identical duplicate of Review 0066

## Context and Problem Statement

Review 0067 landed 2026-04-24, one day after Review 0066 (2026-04-23).
Both reports are COMPLETED with valid claim tokens. Raising each
issue on its own would multiply decision work for the same underlying
findings.

Diffing the two reports after normalizing issue IDs, claim tokens,
timestamps, and the one scope-note line that 0067 adds (*"Scope:
ignored `*.ko.md` files by request"*) yields **zero content
differences**:

```
diff <(sed 's/R006[67]-/RNNNN-/g; s/CLAIM_TOKEN.*//g; ...' reviews/0066.md) \
     <(sed 's/R006[67]-/RNNNN-/g; ...' reviews/0067.md)
# (one-line scope-note addition only)
```

Same 6 Critical + 24 High + 40 Medium + 20 Low, same titles, same
locations, same recommendations, same severity assignments. Issue IDs
were renumbered (R0067-NNNN in place of R0066-NNNN) but map 1:1 in
order.

This matches the pattern already handled by **AD 0025** (Review 0058
as a byte-identical duplicate of Review 0057), which established the
"reject the later duplicate, process the original" workflow.

## Decision Drivers

* Processing both reviews independently would produce two parallel
  decision trails (one rooted at R0066-####, the other at R0067-####)
  that collapse to identical fixes. The project's decision store would
  accumulate mirrored ADRs for every accept, doubling the follow-up
  burden.
* Review 0066's gate pass — this release — already applies 17
  Cat-A fixes and several Cat-C2 corrections and defers the
  architectural cluster explicitly via the v0.2.0 CHANGELOG. Re-running
  the same decision logic for R0067-#### would reach the same verdicts.
* Per AD 0025, precedent is clear: reject duplicates with a single ADR
  record and archive both.

## Considered Options

1. Process both reviews in parallel (full decision table for each).
2. Merge R0067-#### IDs into the R0066-#### tracking used in this
   release's ADRs and CHANGELOG.
3. Reject R0067 outright as a duplicate, archive both, and route any
   future re-raise of the same issues back to the R0066 tracking.

## Decision Outcome

REJECT R0067: option 3, mirroring AD 0025. Review 0067 is a
byte-identical duplicate of Review 0066 and adds no new findings.

Status: Implemented (both reviews archived to `reviews/reviewed/`).

### Implementation

* No code change.
* CHANGELOG v0.2.0 references R0066-#### exclusively; R0067-####
  identifiers are *not* propagated into the decision record, OIs, or
  Ignores files.
* Both `reviews/0066.md` and `reviews/0067.md` are moved to
  `reviews/reviewed/` as part of this gate.

## Consequences

* Good, because the decision trail for this pair is compact — one
  reject record, one accept pass, no parallel tracking.
* Good, because future reviewers who re-raise issues from R0067 can be
  pointed at this ADR and then at the substantive R0066 decisions /
  CHANGELOG entries.
* Bad, because a reader coming in cold via R0067 alone might miss the
  actual disposition. Mitigated by the archived copy of R0066 alongside
  R0067 and by this cross-reference record.

## References

- AD 0025 (prior duplicate pattern — R0058 vs R0057)
- Review 0066, Review 0067
- CHANGELOG.md `[0.2.0] - 2026-04-24`

## Amendment (2026-08-07, indy-review-prune — status and title corrected; archived)

Two catalogue corrections, no change to the ruling. **Status:** the 2026-07-23 consolidation
imported this record from the retired `docs/architecture/decisions/archive/0051` path and encoded
"was archived" as `superseded` — but nothing ever superseded it (no successor exists anywhere in
the store). The lifecycle value is corrected to `archived`: the ruling is discharged (Review 0066
was processed in the v0.2.0 gate; no R0067-#### identifier was ever propagated) and this record
remains the only explanation of why the review series skips 0067. **Title:** the frontmatter and
catalogue rows carried the pre-renumber "AD 0051:" label, colliding with the live top-level
AD-0051 ("Review 0068 closure"); they now read "AD 0068:". The H1 above retains its original
wording per content-immutability — `docs/records/README.md`'s numbering note explains the history.
The duplicate-rejection convention this record applied lives on in MADR-0025 (Review 0058 vs 0057),
which stays active as the precedent's home.
