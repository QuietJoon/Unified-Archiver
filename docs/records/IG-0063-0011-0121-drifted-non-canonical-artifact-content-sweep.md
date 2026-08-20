---
type: ADR
title: "IG-0063-0011..0121: Drifted non-canonical artifact content sweep"
description: "Reviewer flagged every line in the historical planning/spec tree that still describes pre-0.1.0 behavior (e.g. passwords as `Option<String>`, `open_at_offset` deferred, unsupported-operation error names)."
tags: [decision, R0063-0011, R0063-0121]
timestamp: 2026-04-19T00:00:00Z
status: active
---

# IG-0063-0011..0121: Drifted non-canonical artifact content sweep

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0063-0011 through R0063-0121 (Review 0063, 111 findings)
- **Date:** 2026-04-19
- **Decision:** REJECT
- **Severity:** Low (documentation staleness)

## Location

Spec and project docs: `specs/001-unified-archive/**`, `docs/project/{design-baseline,intake,skeleton-plan,status}.md`, `docs/architecture/{mvp-scope,effort-and-risk,dictionary}.md`.

## Issue Summary

Reviewer flagged every line in the historical planning/spec tree that still describes pre-0.1.0 behavior (e.g. passwords as `Option<String>`, `open_at_offset` deferred, unsupported-operation error names). Recommendation was to either line-edit each occurrence to match the shipped crate or quarantine the artifact more clearly.

## Rationale

- **Already resolved by OI-0057-009** (Review 0057 → resolved 2026-04-18). The empirical sweep concluded that remaining staleness in historical specs is non-normative and does not require line-editing; canonical behavior lives in source, `docs/API_REFERENCE.md`, `README.md`, and `docs/USER_MANUAL.md`.
- Each affected spec file already carries a **post-0.1.0 reality-check banner** pointing at the canonical references (added during Review 0062 closure). The reviewer's own `Evidence / reasoning` line on R0063-0013 quotes that banner verbatim, confirming the quarantine mechanism is in place.
- Line-editing 111 occurrences across ~20 archival/spec files would produce churn with no reader benefit; the banner already tells readers to ignore the surrounding prose.

## Future Consideration

If a new class of drift appears (not just pre-0.1.0 prose) or if the banner proves insufficient in a specific reader workflow, revisit per-file. Do not line-edit historical spec content speculatively.

## Related

- OI-0057-009 (resolved) — documentation/test-staleness sweep.
- AD 0025 — policy for archived/non-normative doc references (archived under `docs/records/`).
- Review 0062 closure — added reality-check banners to the same files.
