# AD 0045: Reject R0063 Low-cluster doc-drift sweep; already covered by OI-0057-009 + post-v0.1.0 banners

## Context and Problem Statement

Review 0063 produced 111 Low findings (R0063-0011 through R0063-0121) across the historical planning tree (`specs/001-unified-archive/**`, `docs/project/{design-baseline,intake,skeleton-plan,status}.md`, `docs/architecture/{mvp-scope,effort-and-risk,dictionary}.md`). Each finding flags one line of pre-0.1.0 prose (passwords as `Option<String>`, `open_at_offset` deferred, obsolete error variant names like `UnsupportedOperation`, etc.) and asks for either a line-edit to current behavior or clearer quarantine.

The reviewer cited `OI-0057-009 / AD 0025 (broader documentation/test-staleness sweep)` in the `Ignore/Decision reference` field on every one of the 111 findings — an acknowledgement that this class of drift was already adjudicated.

## Decision Drivers

* OI-0057-009 is the prior adjudication: it concluded (resolved 2026-04-18) that the remaining `Option<String>`/`UnsupportedOperation`/etc. references live in `.translate/` (non-normative translation artifacts) and historical spec docs, neither of which needs migration. Canonical behavior lives in source, `docs/API_REFERENCE.md`, `README.md`, and `docs/USER_MANUAL.md`.
* Review 0062 closure (2026-04-18) added **post-v0.1.0 reality-check banners** to every spec file the current reviewer flagged. R0063-0013's own `Evidence / reasoning` line quotes that banner verbatim — confirming the quarantine mechanism is already in place and the reviewer is flagging content *below* it.
* Line-editing 111 occurrences across ~20 historical files would produce churn with no reader benefit; the banner already tells readers to prefer the canonical references.

## Considered Options

1. **Accept and line-edit all 111 occurrences to current behavior.** Rejected: churn, no reader benefit, and the reviewer's own `Ignore reference` field concedes this is already covered.
2. **Accept and re-quarantine each file more aggressively (e.g. move under `archive/`).** Rejected: breaks existing inbound links; the banner is a lighter-weight quarantine with the same effect.
3. **Reject, one decision record + one Ignores entry, pointing at OI-0057-009 and the banners.** Accepted.

## Decision Outcome

REJECT the 111-finding cluster. Status: Closed by this record; no code or doc edits.

## Implementation

* No line-edits to historical specs.
* Ignores entry `IG-0063-0011..0121` appended to `reviews/Ignores.md`.
* This MADR records the rationale so future reviewers raising the same concern can land here instead of refiling.

## Consequences

* Future readers of `specs/001-unified-archive/**` rely on the banner + canonical references. This has been the accepted policy since Review 0057; the banner was added during Review 0062 closure.
* If a reader ignores the banner and reads the stale prose, they get out-of-date information. Tradeoff consciously accepted.

## Revisit trigger

Re-open if:

* A new class of drift appears in these files that is *not* pre-0.1.0 planning prose (e.g. an actively misleading current-API claim).
* The banner proves insufficient — e.g. an automated doc indexer picks up the stale prose and surfaces it in a user-facing search result.
* A future release cycle replaces the historical planning tree (migration out of `specs/001-unified-archive/` wholesale).

## References

* OI-0057-009 (resolved 2026-04-18) — documentation/test-staleness sweep.
* AD 0025 — policy for archived review broken-path references.
* Review 0062 closure — added reality-check banners to the affected spec files.
* `reviews/Ignores.md` → `IG-0063-0011..0121`.
