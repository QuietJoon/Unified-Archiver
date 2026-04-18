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
