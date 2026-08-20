---
type: ADR
title: "AD: SfxDetectionResult probable() confidence clamped to [0.0, 0.99]"
description: "Superseded by I3 — confidence clamp retired with the f32 score"
tags: [decision, ADR-0014, R0001-0023]
timestamp: 2026-04-16T00:00:00Z
status: superseded
---

# AD: SfxDetectionResult probable() confidence clamped to [0.0, 0.99]

## Context and Problem Statement
Found in Review 0001 (Issue R0001-0023, Severity: Medium).
Location: `src/sfx/result.rs:58-69`

`SfxDetectionResult::probable()` clamped confidence to `[0.0, 1.0]`, and
`is_confirmed()` checked `confidence == 1.0`. This meant `probable(..., 1.0)`
produced a "confirmed" result, violating the documented invariant: "Results
from probable() are never confirmed." Only `detected()` should produce
confirmed results.

## Decision Drivers
* The doc comment explicitly stated probable results are never confirmed
* `detected()` is the intended path for confirmed detections (hardcodes `confidence: 1.0`)
* A caller passing `1.0` to `probable()` could accidentally bypass the `detected()` constructor

## Considered Options
1. Clamp `probable()` to `[0.0, 0.99]` so it can never produce confirmed results
2. Update the docstring to allow probable results to be confirmed

## Decision Outcome
ACCEPT (Option 1): `probable()` now clamps to `[0.0, 0.99]`.

Status: Implemented

### Implementation
- `src/sfx/result.rs`: Changed `confidence.clamp(0.0, 1.0)` to `confidence.clamp(0.0, 0.99)`
- `src/sfx/result.rs`: Updated doc comment to state clamping range
- `src/sfx/result.rs`: Updated three tests that expected `probable(..., 1.0)` to be confirmed

## Consequences
* Good, because the documented invariant is now enforced by the constructor
* Good, because `is_confirmed()` reliably means the result came from `detected()`
* Bad, because callers passing `1.0` to `probable()` now get `0.99` — this is the correct behavior since they should use `detected()` instead

## Amendment (2026-07-22, R0081 I3)

Status: Superseded — the clamp no longer exists.

The `[0.5, 0.99]` (originally `[0.0, 0.99]`) confidence clamp this record
introduced has been **retired** along with the entire `f32` confidence score.
`SfxDetectionResult::confidence` is now the tri-state `SfxConfidence` enum
(`NotSfx` / `Probable` / `Confirmed`); `probable()` fixes the state to
`SfxConfidence::Probable` and takes a `Vec<String>` evidence list instead of a
float, and `is_confirmed()` is a direct `matches!` on `SfxConfidence::Confirmed`
rather than a float comparison.

The invariant this record enforced — "a `probable()` result is never
confirmed" — is now structural and unbreakable: `probable()` cannot set the
`Confirmed` variant at all, so there is no threshold to clamp and no
`probable(..., 1.0)` foot-gun to guard against. The float provably distinguished
only two production values (`0.0` and the clamped `~0.9`), so this clamp was a
guard on a scale that carried no information. See arch AD 0006 (amended) and
gate-0015 (amended), and the OI-0080-004 typed-multipart direction (typed
results over heuristic floats).
