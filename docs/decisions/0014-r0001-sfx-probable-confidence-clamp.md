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
