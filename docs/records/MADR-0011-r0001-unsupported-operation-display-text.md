---
type: ADR
title: "AD: UnsupportedOperation display text — \"cannot be performed\" replaces \"not implemented\""
description: "Superseded — the Option 2 variant split this record rejected landed via OI-0001-003 (commit c9b6685, 2026-04-17); ArchiveError::UnsupportedOperation and the single Display text this record ruled on no longer exist."
tags: [decision, ADR-0011, R0001-0003, OI-0001-003]
timestamp: 2026-04-16T00:00:00Z
status: superseded
---

# AD: UnsupportedOperation display text — "cannot be performed" replaces "not implemented"

Status: Superseded by the OI-0001-003 `ArchiveError` variant split (commit c9b6685, 2026-04-17) — the `UnsupportedOperation` variant this ruling governed no longer exists; see the §B amendment below.

## Context and Problem Statement
Found in Review 0001 (Issue R0001-0003, Severity: High).
Location: `src/error.rs:137`

The `Display` implementation for `ArchiveError::UnsupportedOperation` printed
"Operation '…' not implemented: …", but the same variant is used for runtime
states that are implemented but disallowed: lock contention, write-mode
extraction, mmap size limits, and path collisions. The text misled users into
thinking the feature was missing when the actual cause was a runtime constraint.

## Decision Drivers
* 40+ usage sites span four categories: feature deferral, mode mismatch, resource constraints, boundary enforcement
* Users read the Display text in error messages and logs
* Programmatic matching uses the variant name, not the Display text — changing the text is backward-compatible for error-handling code

## Considered Options
1. Change Display text to neutral wording: "cannot be performed"
2. Split into separate error variants (UnsupportedFeature, InvalidOperation, ResourceError)
3. Keep current text — callers can read the `reason` field

## Decision Outcome
ACCEPT (Option 1): Display text changed from "not implemented" to "cannot be performed".
Option 2 (variant split) tracked as future work in Open_Issues.md (OI-0001-003).

Status: Implemented

### Implementation
- `src/error.rs`: Display impl changed to `"Operation '{}' cannot be performed: {}"`
- `src/error.rs`: Updated test `test_display_unsupported_operation` assertion

## Consequences
* Good, because error messages are now accurate for all usage sites
* Good, because no API breakage — variant name and fields unchanged
* Bad, because log messages and user-facing strings change text (could affect grep-based monitoring)

## Amendment (2026-08-05, decision-review-2026-07-19 §B — superseded by the OI-0001-003 variant split)

The 2026-07-19 review found this record still `active` on stale claims: it accepted a Display-text
change (Option 1) and rejected the variant split (Option 2, deferred to OI-0001-003), but **the
rejected Option 2 is what actually shipped** — one day after this record. Commit `c9b6685`
(2026-04-17) removed `ArchiveError::UnsupportedOperation` entirely, splitting it into four new
variants — `NotImplemented`, `OperationBlocked`, `ReadOnlyBackend`, `WriteModeOnly` — alongside the
pre-existing `Unsupported` (which the commit left untouched: it gained no migrated call sites), and
migrated every former `UnsupportedOperation` site to one of the four; OI-0001-003 is RESOLVED
(2026-04-17) in `docs/project/open-issues-resolved.md`, and `UnsupportedOperation` has zero
occurrences in today's Rust sources. The shipped variant names differ from Option 2's sketch
(`UnsupportedFeature` / `InvalidOperation` / `ResourceError`), but the shape is Option 2's: callers
now distinguish feature deferral (`NotImplemented`) from mode mismatch (`WriteModeOnly` /
`ReadOnlyBackend`) programmatically. Two of the four usage categories this record's Decision Drivers
identified are *not* separated by the split — resource constraints and boundary enforcement both
land on `OperationBlocked` ("blocked by resource limits, conflicts, or format constraints") — so
Option 2 shipped in shape, not in full.

The accepted Option 1 also never shipped as recorded: in the commit preceding `c9b6685` the
Display impl still read `"Operation '{}' not implemented: {}"`, and the "cannot be performed"
wording first appears inside `c9b6685` itself — as the Display text of the new blocked-family
variants (`OperationBlocked`, `WriteModeOnly`, `ReadOnlyBackend` in `src/error.rs`). The split even
reinstated the wording this record removed — `NotImplemented` displays "is not yet implemented" —
now accurate precisely because the split confines it to genuine feature deferral, while
`Unsupported` displays "not supported for {format} format". Later error-model work continued in
the same direction: the typed `Operation` label enum (`src/error.rs::Operation`, Review 0068 D7 /
R0068-0045, migration window tracked by AD 0051) is replacing the stringly `error::ops::*`
constants, and R0068-0070 replaced `open_encrypted`'s catch-all "Format not yet supported" reason
with format-aware wording (CHANGELOG, Review 0068 closure pass).

**Disposition: superseded.** The single variant whose Display text this record ruled on is gone,
and the decision that replaced it is the option this record rejected. Cross-refs: OI-0001-003
(resolution entry documents the split); MADR-0012's §B amendment (the same removal voids that
record's `UnsupportedOperation` guard); AD 0051 / R0068-0045 (typed `Operation` follow-on).
