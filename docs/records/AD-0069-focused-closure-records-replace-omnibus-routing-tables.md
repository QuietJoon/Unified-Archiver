---
type: ADR
title: "AD 0069: Review closure is recorded in focused records, not in one omnibus routing table"
description: "Accepted (2026-08-20) — retires the omnibus closure-record form used by AD-0059, AD-0060, AD-0061, AD-0063 and AD-0067."
tags: [decision, ADR-0069, ADR-0059, ADR-0060, ADR-0061, ADR-0063, ADR-0067, project-control]
timestamp: 2026-08-20T00:00:00Z
status: active
---

# AD 0069: Review closure is recorded in focused records, not in one omnibus routing table

## Status

Accepted (2026-08-20) — owner decision. Retires the omnibus closure-record form.

## Context

Five closure records dispose of an entire review in a single document carrying a hand-written
routing table: one markdown row per finding id, each row naming that finding's disposition (fixed
in session, deferred to an open issue, or rejected). AD-0059, AD-0060, AD-0061, AD-0063 and AD-0067
all take this shape.

The form has one failure mode and it occurred. AD-0067's table jumps from `R0076-0049` to
`R0076-0052`. `R0076-0051` — "[LOW] ZIP integrity failures report raw names" — appears in no row, in
no `Source` line, and in no summary count. Its `index.yaml` row shows the same gap: the `reviews:`
array runs `… R0076-0049, R0076-0052 …`, so neither `R0076-0050` nor `R0076-0051` is listed there
either. The omission was harmless — the tree fixed it under `R0080-0063`, and `test_integrity` now
pushes `normalized_name` — but nobody noticed for months, and the 2026-08-07 amendment on that record
says why: *"the closure record's completeness is the thing a future reader trusts."*

A mechanical completeness check was the obvious answer and it is not available. Verifying that every
finding id in a review appears exactly once in its closure record requires the review file, which
lists the ids. Review files were moved to a cold store outside the repository on 2026-08-07, so a
check living in this repository has no input to read. Making the check possible would mean reversing
that archival policy.

There is also a precedent in the other direction. The Review 0001 gate disposed of 95 findings by
writing two focused MADRs plus one DCR, with no routing table at all, and no finding went missing.

## Decision

**Closure is recorded in focused records.** One record per decision or coherent theme, not one record
per review. The hand-written routing table is retired.

Concretely:

- **No new omnibus closure records.** A review with 95 findings produces however many records its
  decisions need — as Review 0001 did — rather than one document with 95 rows.
- **`index.yaml` carries the routing, not prose.** Each record's `reviews:` array already lists the
  finding ids that record disposes of, in machine-readable form. That array becomes the authoritative
  statement of what a record covers; the union across records is the routing table. This is not a new
  mechanism — it is the existing one, promoted from a duplicate of the prose table to the single
  source of truth.
- **The five existing omnibus records stay as they are.** They are content-immutable and they are
  accurate apart from the one recorded gap. They are not rewritten, not split retroactively, and not
  deprecated. This decision governs what is written from here on.
- **Completeness stays unverifiable for now, and that is stated rather than implied.** With review
  files in a cold store outside the repository, nothing in-repo can prove a review was fully
  dispositioned. Focused records reduce the exposure — a missing finding no longer hides inside a
  40-row table whose completeness a reader assumes — but they do not eliminate it. Anyone who wants
  the check must first decide to keep review files in-repo, which is a separate policy question this
  record does not settle.

## Consequences

### Good

- **The failure mode needs a bigger mistake.** Omitting a finding from a 40-row table is a typo.
  Omitting a whole decision that should have had its own record is visible in a way a missing table
  row is not.
- **Routing becomes machine-readable by default.** `index.yaml`'s `reviews:` arrays are already
  parsed by tooling in this repository; prose tables never were. If review files ever return in-repo,
  the check becomes a small script rather than a new format.
- **Records get titles that say what was decided.** "Review 0076 closure and routing record" tells a
  searcher nothing about the decisions inside it. Focused records are findable by subject.

### Bad / costs

- **More files.** A review that would have produced one record now produces several, and
  `index.yaml` grows faster.
- **Cross-finding context can fragment.** An omnibus record shows at a glance how a review's findings
  related to each other; several focused records do not. Where that relationship matters, it has to
  be written into one of them explicitly rather than being implicit in the table.
- **No retroactive uniformity.** The store will hold both forms indefinitely. A reader looking for
  Review 0076's disposition still needs AD-0067; a reader looking for Review 0001's needs three
  records. That inconsistency is accepted as cheaper than rewriting immutable records.

## Alternatives considered

- **Keep omnibus, add a mechanical completeness check.** Rejected as currently impossible, not as
  undesirable: the check needs the review file's finding list and review files are outside the
  repository. It would also require reversing the 2026-08-07 cold-store decision, which was taken
  for its own reasons and is not this decision's to overturn.
- **Keep omnibus and accept unverifiable completeness.** Rejected. It is the status quo that
  produced the `R0076-0051` gap, and the amendment recording that gap is now the only in-repo trace
  of it — the review that would prove the omission is in the cold store.
- **Split the five existing omnibus records retroactively.** Rejected. Records here are
  content-immutable, the existing five are accurate apart from one documented gap, and rewriting
  history to satisfy a new convention buys nothing a reader needs.

## Verification

- `grep -c 'R0076-0051' docs/records/AD-0067-r0076-closure-and-routing-record.md` returns matches
  only inside the 2026-08-07 amendment, not in the routing table — the gap this record is responding
  to is real and still visible.
- `docs/records/index.yaml`'s `AD-0067` row lists `R0076-0049` followed by `R0076-0052`, confirming
  the machine-readable routing carries the same omission as the prose table.
