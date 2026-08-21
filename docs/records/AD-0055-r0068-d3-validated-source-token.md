---
type: ADR
title: "AD 0055: `ValidatedSource` token replaces `_unchecked` extraction (D3)"
description: "Implemented, but the named invariant is not structurally enforced (R0070-0008 live); convergence with the OI-0076-002 ValidatedEntry token is an open v0.4 question (see 2026-08-04 amendment)."
tags: [decision, ADR-0055, ADR-0053, R0068-0039, R0068-0040]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0055: `ValidatedSource` token replaces `_unchecked` extraction (D3)

## Context and Problem Statement

Found in Review 0068 (R0068-0039 memory, R0068-0040 stream). AD 0053
baselined the design.

`Archive::extract_to_memory_unchecked` /
`Archive::extract_to_stream_unchecked` are `pub(crate)` helpers used
inside `commit_changes` and `calculate_manifest_digest`. They skip the
per-call entry-list rebuild + safety pre-check on the grounds that the
caller has already produced the listing. The contract was a doc
comment ("only call after you've validated"), not a type fact.

## Decision Drivers

* Type-level enforcement of the safety invariant > comment
  enforcement. A new internal caller can today reach for the
  `_unchecked` variant without obtaining a fresh listing first.
* Migration must keep external behavior identical — these helpers
  are not part of the public API.
* The token wraps `&Archive` today; once D2 (god-object split) lands,
  it wraps `&ReadArchive` so a write/modify-mode handle cannot
  produce one even via the type system.

## Decision Outcome

ACCEPT: introduce `ValidatedSource<'a>` in `src/extraction.rs` with
`pub(crate)` `extract_to_memory(&str)` and `extract_to_stream(&str)`
methods. The only constructor is `Archive::validated_source(&self)`
(also `pub(crate)`), so internal callers implicitly assert "I have a
listing of this archive in hand" by acquiring the token before
iterating.

Status: Implemented. The `_unchecked` variants stay as
forwarding helpers during the migration window — the
`commit_changes` loop in `modification.rs` and
`entry_crc32_for_digest` in `inspection.rs` already use the token.

### Implementation

`src/extraction.rs`:

- New `pub(crate) struct ValidatedSource<'a> { archive: &'a Archive }`
  with `extract_to_memory` / `extract_to_stream` methods that delegate
  to the existing `_unchecked` helpers.
- New `pub(crate) Archive::validated_source(&self)` constructor.
- Doc updates on `extract_to_memory_unchecked` /
  `extract_to_stream_unchecked` pointing new callers at the token.

`src/modification.rs::commit_changes`:

- Replaces `self.extract_to_stream_unchecked(&entry.path)?` with
  `src.extract_to_stream(&entry.path)?` (and same for memory)
  inside the retained-entries loop.
- A single `let src = self.validated_source();` is hoisted above the
  loop — the token's lifetime spans the entire iteration.

`src/inspection.rs::entry_crc32_for_digest`:

- Same swap: `self.extract_to_stream_unchecked(...)` →
  `self.validated_source().extract_to_stream(...)`.

## Consequences

* Good, because the safety invariant ("source listing was validated")
  is now type-checked instead of comment-enforced.
* Good, because the callers' code paths read more directly: "build a
  validated view, then iterate over it" instead of "call magic
  `_unchecked` method".
* Good, because deletion of `_unchecked` becomes a follow-up
  cleanup once D2 lands the `&ReadArchive` form — the migration is
  staged, not big-bang.
* Bad, because the token is a thin wrapper over `&Archive` and can
  technically be obtained from a write-mode `Archive` today. After
  D2 the constructor lives on `&ReadArchive` and that loophole
  disappears.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — invariant not structurally enforced; R0070-0008 live)

The 2026-07-19 review found — and the current tree confirms — that the invariant this record
names (acquiring the token implicitly asserts "I have a listing of this archive in hand") is
**documented, not structurally enforced**. `ValidatedSource<'a>` (`src/extraction.rs`) stores
only `archive: &'a Archive`, and its sole constructor `Archive::validated_source(&self)` takes
no listing and performs no check, so any crate-internal code holding `&Archive` can mint the
token without ever having listed the archive — the invariant lives in the constructor's rustdoc,
i.e. exactly the comment-enforcement this decision set out to replace. This is **R0070-0008**
("`ValidatedSource` is obtainable from any `&Archive` and does not actually carry the listing it
claims to validate"), routed to OI-0069-002 by AD 0060 and still live. The Consequences bullet
expecting D2 to close the write-mode loophole also did not materialise as predicted: D2 shipped
as *additive* typed handles (`ReadArchive`, `src/archive/mode_split.rs`, `v2-api` feature) that
delegate to `Archive`, the constructor still lives on `Archive`, and the `_unchecked` forwarding
helpers (`extract_to_memory_unchecked` / `extract_to_stream_unchecked`) remain — the migration
window never closed. Meanwhile the token accreted real duties beyond the original two methods
(`extract_to_stream_by_id` for the CRC-less content-digest walk per R0079-0028, plus the
`_with_limit` cap-aware variants), so it is load-bearing despite the unenforced invariant.

The **working version of the idea is the OI-0076-002 `ValidatedEntry` token**
(`src/security.rs`, resolved 2026-07-06): no public constructor — only the
`validate_single_entry` gate (exactly one listing match + regular-file kind, checked against a
listing snapshot; `check_single_entry_safe` = gate + limits) produces it — and backends validate
first against their own memoised listing, then seek by the token's stable listing id with drift
guards at the seek site. There, holding the token *is* proof the gate ran: the structural
enforcement this record aimed at. **Open v0.4 design question** (decision-review-2026-07-19 §B;
`docs/backlog.md` "Residual AD deferral targets"): converge `ValidatedSource` onto the
validate-first + seek-by-listing-id `ValidatedEntry` shape, or retire the weaker token — a
change to shipped internal call paths (`modification.rs::commit_changes`,
`inspection.rs::entry_crc32_for_digest`, the by-id digest stream). This amendment records the
enforcement gap and the convergence question; it does not rule. The D3 decision itself —
replace `_unchecked` comment contracts with a capability token — still governs; the record
stays **active**.

## Amendment (2026-08-21, the D3 residual is tracked as its own ticket — do not file it twice)

Bookkeeping only. Nothing above is reversed, narrowed or re-decided: the D3 decision still
governs, the enforcement gap the 2026-08-04 amendment records is still real, and this record
stays **active**. What changes is where a reader is sent next.

**The deferral this record carries is not untracked.** The 2026-08-04 amendment leaves two things
open — the remaining `ValidatedSource` call sites (`modification.rs::commit_changes`,
`inspection.rs::entry_crc32_for_digest`, the by-id digest stream) and the construction-order
residual that sits behind the same unenforced invariant it reports as R0070-0008 — and the second
of those is R0069-0063, the last unlanded sub-item of OI-0069-002's nine. **That residual has had
its own TicGit ticket since 2026-08-12: `84b324d1`, "ValidatedSource construction order
(OI-0069-002 residual, R0069-0063)".**
Verified first-hand on 2026-08-21 rather than taken on trust: `ti show 84b324d1` reports
`Status: open`, `State: blocked`, priority 3, tags `OI-0069-002, R0069-0063, api, reopen, v0.4,
verified`, and its `Source` line already names this record alongside
`docs/project/open-issues.md#OI-0069-002`. Its acceptance criteria are the two this record wants
— `ValidatedSource` cannot be constructed before its validating checks have run, and the
`_unchecked` naming and the token type tell the same story at every call site — and its recorded
blocker is the one this record's Consequences predicted and the 2026-08-04 amendment found
unfulfilled: the reordering is only coherent once the legacy `Archive` sum type is retired and the
typed handles are the sole facade, which is v0.4 work.

**So this record should stop reading as an untracked deferral.** A reader who reaches the "Open
v0.4 design question" paragraph above and files a ticket for it files a duplicate. The question is
held; it is not answered, and this amendment does not answer it either.

**Context on the umbrella ticket.** `84b324d1` is the narrow residual. The wider "rule on three
orphaned deferral targets" item — TicGit `176503b7`, carried in `docs/backlog.md` as "Residual AD
deferral targets" until the 2026-08-21 reconciliation discharged that entry — covered AD 0052,
this record and AD 0057 together. Two of its three halves have
since moved out: the AD 0052 half was ruled and closed (`0a1e54f8`; the deferral is retired, the
cheap `validate()` probe landed, and AD 0052 carries the 2026-08-21 amendment that says so), and
the AD 0057 half **landed in this same change set** under its own ticket (`095eee`, owner-ruled
2026-08-20 "queue and do
it"). What remains under `176503b7` is this record's half, which the owner ruled is bookkeeping —
this amendment — rather than a decision. The convergence-versus-retirement choice for the token
itself stays where the 2026-08-04 amendment put it: an open v0.4 design question, whose
construction-order component is `84b324d1`.
