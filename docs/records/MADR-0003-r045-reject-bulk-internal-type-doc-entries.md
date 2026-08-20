---
type: ADR
title: "AD: Reject bulk per-wrapper documentation entries; document the boundary instead"
description: "Implemented (2026-04-14)."
tags: [decision, ADR-0003, ADR-0001, R0045-0004, R0045-0076]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Reject bulk per-wrapper documentation entries; document the boundary instead

## Context and Problem Statement

Found in Review 0045 (Issues R0045-0004 through R0045-0076 — 73 issues, all Low).

The reviewer flagged each `pub` method on the FFI wrappers
(`UnrarArchive`, `LibarchiveArchive`, `ZipArchive`, `PizArchive`,
`SevenZArchive`, `ZipWriter`) plus `ResultWithWarnings::add_warning` and
`security::get_max_mmap_size` as "missing from API_REFERENCE.md".

These types are `pub` only because their parent module (`unified_archive::ffi`)
is `pub mod`. Per AD 0001 (Single Archive Facade with Backend Enum), the
*supported* public surface is the `Archive` facade, not the per-backend
wrappers. Documenting each wrapper individually would mislead callers into
believing the wrappers are part of the stable API and depending on them.

## Decision Drivers

* AD 0001 is explicit that backend selection is automatic and
  `Archive` is the intended public surface; expanding the doc to list each
  wrapper undoes that boundary.
* The wrappers' methods change frequently as backend behavior is unified;
  documenting them as if they were stable API would create a churn surface.
* Future review tools will likely flag this same set again — a written
  decision avoids relitigating.

## Considered Options

1. Reject the 73 individual entries and instead add a one-section
   "Internal types" disclaimer to `docs/API_REFERENCE.md` clarifying that
   `pub` items under `unified_archive::ffi` and similar modules are not
   part of the supported public surface.
2. Accept all 73 issues and grow the API reference to cover every wrapper
   method.
3. Hide the wrappers behind `pub(crate)` and expose only what callers truly
   need (a future-facing refactor; out of scope for this gate).

## Decision Outcome

**REJECT all 73 issues, ACCEPT the disclaimer mitigation** — Option 1.

The disclaimer captures the correct intent without misrepresenting the
internal types as public API. Moving the wrappers behind `pub(crate)`
(Option 3) is the long-term right move but is a separate design change that
this review does not justify.

Status: Implemented (2026-04-14).

### Implementation

`docs/API_REFERENCE.md`: added an "Internal types" section between the
intro paragraph and the table of contents, listing the seven items above
and noting that they are exposed for crate-internal composition only and
may change without notice.

## Consequences

* Good — `docs/API_REFERENCE.md` continues to reflect the supported public
  surface (the `Archive` facade), preventing accidental adoption of
  internal types.
* Good — Future reviewers running the same checker will see this AD and the
  disclaimer, skipping the same 73 issues without needing further analysis.
* Bad — Callers who explore `cargo doc` output will still see public
  rustdoc on the wrapper types. The disclaimer mitigates but does not
  eliminate this; closing it fully requires the `pub(crate)` refactor.
