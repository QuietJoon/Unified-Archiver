---
type: Design Review
title: "ADR / DCR corpus design review — revert, improve, innovate"
description: "Full-corpus critique of all 93 decision records (55 architecture ADs, 30 review-gate MADRs, 8 DCRs) at small and big focus; revert candidates prioritized."
tags: [design-review, project-control]
timestamp: 2026-07-19T00:00:00Z
status: active
---

# ADR / DCR design review — 2026-07-19

Scope: every decision record in the unified `docs/records/` store (55 ADs, 30 review-gate
MADRs, 8 DCRs) — 93 records —
reviewed as **design artifacts** (architecture and decisions, not code). Each record was
critiqued at *small focus* (internal quality: stale premises, contradictions with sibling
records, unacknowledged supersession) and *big focus* (is the decision itself still right;
what would reverting buy). Method: 8 thematic reviewers (core architecture; safety/limits;
SFX/detection/checksums; RAR/FFI/build; modify/creation; API shaping; process/closure
records; gate remainder + the record system itself), each grounding record premises against
the current tree before