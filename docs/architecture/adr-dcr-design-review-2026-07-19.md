---
type: Design Review
title: "ADR / DCR corpus design review — revert, improve, innovate (incomplete)"
description: "Incomplete: only the scope and method preamble was preserved; the findings were never part of this file. Superseded in practice by decision-review-2026-07-19.md, which covers the same 2026-07-19 corpus review and is complete."
tags: [design-review, project-control]
timestamp: 2026-07-19T00:00:00Z
status: superseded
---

# ADR / DCR design review — 2026-07-19 (incomplete)

> **This file is a fragment.** It ends mid-sentence after the method paragraph, and it has
> existed at exactly this length for its whole recorded history — the findings it introduces
> were never present here. Do not read the absence of findings as "the review found nothing".
>
> **Read [`decision-review-2026-07-19.md`](decision-review-2026-07-19.md) instead.** It covers
> the same 2026-07-19 decision-record corpus at the same two zoom levels, it is complete, and
> its rulings are the ones later records cite. Its `status:` is `advisory`.
>
> The record counts formerly quoted in this file's frontmatter are dropped rather than
> guessed: they cannot be checked against a body that does not exist, and the authoritative
> count today is whatever `docs/records/index.yaml` holds.

Scope, as far as it was preserved: every decision record in the unified `docs/records/` store,
reviewed as **design artifacts** (architecture and decisions, not code). Each record was to be
critiqued at *small focus* (internal quality: stale premises, contradictions with sibling
records, unacknowledged supersession) and *big focus* (is the decision itself still right; what
would reverting buy). Method: 8 thematic reviewers — core architecture; safety/limits;
SFX/detection/checksums; RAR/FFI/build; modify/creation; API shaping; process/closure records;
gate remainder and the record system itself — each grounding record premises against the tree
before reporting.

*(The text ends here.)*
