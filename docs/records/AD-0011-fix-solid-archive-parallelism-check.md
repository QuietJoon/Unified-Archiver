---
type: ADR
title: "AD: Fix solid-archive parallelism check to use password-aware handle"
description: "Superseded by AD 0029 (its own 2026-07-20 amendment records the supersession) — AD 0029's single-pass selective extraction removed the internal parallel branch this fix gated."
tags: [decision, ADR-0011, R0025-0006, R0025-0007, R0025-0008]
timestamp: 2026-04-23T00:00:00Z
status: superseded
---

# AD: Fix solid-archive parallelism check to use password-aware handle

## Context and Problem Statement
Found in Review 0025 (Issues R0025-0006, R0025-0007, R0025-0008, Severity: HIGH).
Location: `src/extraction.rs` — `extract_filtered()`, `extract_files()`, `extract_by_ids()`

Three extraction methods called `self.is_solid()` to decide whether parallel extraction was safe. However, `self` uses the original archive handle which may lack a password. When an encrypted solid archive is opened with `open_encrypted()`, the password-aware `archive` handle (created locally in each method) was ignored for the solidity check, causing `is_solid()` to fail silently and default to allowing parallel extraction on solid archives.

## Decision Drivers
* Correctness: solid archives require sequential decompression
* The password-aware `archive` handle already exists in each method
* Silent failure (defaulting to parallel) could produce corrupted output

## Considered Options
1. Change `self.is_solid()` to `archive.is_solid()` in all three methods
2. Cache solidity at open time — rejected because encrypted archives can't be probed without a password

## Decision Outcome
ACCEPT: Changed all three call sites to use the local `archive` handle.

Status: Superseded by AD 0029 — the single-pass selective-extraction redesign removed the internal parallel branch this fix gated; see the 2026-07-20 amendment.

### Implementation
- `extract_filtered()`: `self.is_solid()` -> `archive.is_solid()`
- `extract_files()`: same
- `extract_by_ids()`: same

## Consequences
* Good, because encrypted solid archives are now correctly detected and extracted sequentially
* Good, because the fix is minimal (3 line changes) with no API impact

## Amendment (2026-07-20)

This fix targeted the internal parallel-extraction branch — the code that chose parallel vs. sequential extraction based on `is_solid()`. That branch, and the whole internal-parallelism story, was **removed by AD 0029** (the `extract_some()` extraction redesign), which made selective extraction single-pass over a shared handle and left parallelism to callers.

With no internal parallel branch remaining, there is no `is_solid()`-gated parallel decision left to get wrong. The solid-archive parallelism-check fix recorded here is therefore **moot / historical**. This record is **superseded by AD 0029**.
