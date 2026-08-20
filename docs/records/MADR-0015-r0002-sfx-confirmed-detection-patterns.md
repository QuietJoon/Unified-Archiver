---
type: ADR
title: "AD: SFX confirmed detection (confidence 1.0) — known patterns"
description: "Documented — implementation deferred to future work"
tags: [decision, ADR-0015, R0002-0002]
timestamp: 2026-04-16T00:00:00Z
status: active
---

# AD: SFX confirmed detection (confidence 1.0) — known patterns

## Context and Problem Statement
Found in Review 0002 (Issue R0002-0002, Severity: High).
Location: `src/sfx/result.rs:43`, `src/sfx/detection.rs:121`

The shipped `detect_sfx()` function only returns `not_sfx()` or `probable()`
(confidence ≤ 0.99). The `detected()` constructor (confidence 1.0) and
`is_confirmed()` predicate exist in the public API but are unreachable from
the live detector — they are only exercised by tests.

## Decision Drivers
* `detected()` / `is_confirmed()` are public API surface with documented semantics
* Removing them would break callers that match on `is_confirmed()` and reduce expressiveness
* Real 100%-confidence patterns exist but require backend parsing, which is not yet implemented

## Considered Options
1. Remove/scope `detected()` and `is_confirmed()` since they are currently unreachable
2. Keep the current API surface and document the known patterns that would justify confidence 1.0,
   creating a roadmap for implementing confirmed detection

## Decision Outcome
REJECT (Option 1) / ACCEPT (Option 2): Keep `detected()` and `is_confirmed()` in the public API.
Document the concrete patterns that would produce confirmed results when implemented.

Status: Documented — implementation deferred to future work

## Known Patterns for Confidence 1.0

When any of these patterns can be verified, `detect_sfx()` should return
`SfxDetectionResult::detected()` instead of `probable()`:

### Pattern 1: Backend parse succeeds at detected offset
Open the archive backend (libarchive, piz, sevenz-rust2, unrar) at the
detected `data_offset`. If the backend can successfully list at least one
entry, the archive is confirmed. This is the canonical confirmation path
and is blocked on **DEF-001** (`open_at_offset`).

### Pattern 2: Known SFX stub signature match
Some SFX creators embed recognizable stub signatures:
- **7-Zip SFX**: `7z.sfx` / `7zCon.sfx` stubs have known PE section layouts
  and resource strings ("7-Zip self-extracting archive")
- **WinRAR SFX**: `Default.SFX` / `WinCon.SFX` stubs contain the RarSFX
  configuration marker `";The comment below contains SFX script commands"`
- **NSIS**: Contains `"Nullsoft Install System"` in PE version info
- **Inno Setup**: Contains `"Inno Setup Setup Data"` at a known offset

When the stub type matches a known creator AND the archive payload signature
is valid at the detected offset, confidence can be raised to 1.0 without
full backend parsing.

### Pattern 3: End-of-central-directory back-reference (ZIP only)
For ZIP SFX: parse the EOCD record from the end of the file. If the EOCD's
`offset of start of central directory` field points into the data region
after the stub, and the central directory entries are parseable, the
embedded ZIP is confirmed.

## Consequences
* Good, because the public API surface remains stable and forward-compatible
* Good, because callers can rely on the `is_confirmed()` / `probable()` distinction
  becoming meaningful as detection improves
* Bad, because `is_confirmed()` currently always returns false from the live detector,
  which may confuse callers — mitigated by documentation noting current behavior

## Amendment (2026-07-22, R0081 I3)

The confidence surface this record depends on changed: `SfxDetectionResult`
no longer carries an `f32` confidence. It now carries the tri-state
`SfxConfidence` enum (`NotSfx` / `Probable` / `Confirmed`) plus a
`Vec<String>` evidence list. Two consequences for this record:

- **The `Confirmed` variant is now a first-class target.** The three known
  patterns above (backend-parse-at-offset, known-stub-signature match, ZIP
  EOCD back-reference) should, when implemented, construct a result with
  `SfxConfidence::Confirmed`. There is no longer a "confidence 1.0" float to
  reach — the confirmed path sets an enum variant directly, which is both
  clearer and impossible to conflate with a probable result.
- **The float-confidence blocker described in the old framing is gone.** The
  original record was in tension with the retired clamp (AD 0014): the float
  could only ever produce `0.0` or a clamped `~0.9`, so "confidence 1.0" was
  unreachable by construction. With the enum, `Probable` and `Confirmed` are
  distinct, non-numeric states; implementing any of the patterns is now a
  matter of routing that pattern's success to the `detected`/`Confirmed`
  path, not of un-clamping a float.

Implementation of the confirmed-detection patterns remains **deferred**
(still blocked on `open_at_offset` for Pattern 1); this amendment only records
that the API target is now the `Confirmed` enum variant. Feeds the OI-0080-004
typed-multipart direction (typed results over heuristic floats).
