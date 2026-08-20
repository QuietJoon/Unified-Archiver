---
type: ADR
title: "AD: NULL-pathname libarchive headers are uniformly omitted, not format errors"
description: "Rejected R0080-0017's recommendation to error on nameless headers — it would reverse the R0079-0022 bijectivity design."
tags: [decision, ADR-0030, R0080-0017, R0079-0022]
timestamp: 2026-07-17T00:00:00Z
status: active
---

# AD: NULL-pathname libarchive headers are uniformly omitted, not format errors

## Context and Problem Statement
Found in Review 0080 (Issue R0080-0017, Severity: High).
Location: `src/ffi/libarchive_wrapper.rs` (extraction walk NULL-entry branch; `parse_entry` in the listing walk; the integrity walk via `parse_entry`).

The reviewer recommended treating a non-EOF libarchive header with a NULL
entry pointer or NULL pathname as a format error, on the premise that
"listing, extraction, and integrity can all report success while omitting
records" — i.e. that the walks could diverge.

## Decision Drivers
* R0079-0022 (Review 0079) deliberately established positional bijectivity: every walk skips
  NULL-entry/NULL-pathname headers **without consuming an index**, exactly mirroring
  `parse_entry`'s `None` conditions, so listing ids remain stable indices into every
  subsequent walk. Single-entry id-seek drift guards (OI-0076-002) depend on this.
* The finding's premise is factually wrong: all three walks (list, extract, integrity)
  apply the same skip conditions, so a nameless header is *uniformly omitted* — there is
  no divergence between what is listed and what is extracted or tested.
* A nameless header is unaddressable by every public API (no path, no id) — omitting it
  loses nothing a caller could have acted on.

## Considered Options
1. Reject: keep uniform omission (current behavior).
2. Accept locally: error only in the extraction walk — reintroduces list/extract divergence
   in the opposite direction (listed archives would fail extraction).
3. Accept globally: flip all three walks plus the stream walk to fail-loud in lockstep — a
   coherent alternative policy, but it reverses R0079-0022 for malformed-input strictness
   of marginal value and breaks tolerant handling of real-world archives with vendor
   padding/oddity headers.

## Decision Outcome
REJECT: We keep uniform omission (Option 1). The recommendation as written (Option 2) is
unsafe — it breaks the bijectivity contract the drift guards depend on. Option 3 is a
policy reversal with no motivating defect: no divergence exists today.

Status: Implemented (decision-only; no code change).

### Implementation
No source change. This record exists because the finding is likely to recur in future
reviews: any reviewer reading the skip branch in isolation will re-raise it. The governing
design is R0079-0022 (Review 0079) + the OI-0076-002 drift guards; the skip branches carry
comments citing it.

## Consequences
* Good, because listing ids remain valid positional indices in every walk, which the
  single-entry and bulk listing-drift guards rely on.
* Good, because tolerant handling of malformed-but-recoverable archives is preserved.
* Bad, because a truly malformed archive whose every header is nameless lists as empty
  rather than erroring — accepted as the cost of the bijectivity contract.
