---
type: ADR
title: "AD: SFX iterate all candidate signatures and demote CD signature"
description: "Implemented"
tags: [decision, ADR-0015, R0026-0003, R0026-0004, R0027-0002, R0027-0003]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# AD: SFX iterate all candidate signatures and demote CD signature

## Context and Problem Statement
Found in Review 0026 (Issues R0026-0003, R0026-0004) and Review 0027 (Issues R0027-0002, R0027-0003).
Location: `src/sfx/detection.rs`, `src/sfx/signatures.rs`

The SFX detection pipeline had two problems:
1. It only checked the first signature found (`signatures_found[0]`), missing valid archives if a false-positive signature appeared earlier.
2. The ZIP central-directory signature (`PK\x01\x02`) was in the primary signatures list. CD headers appearing before a local file header would trap the first-match logic into detecting the wrong offset.
3. TAR's fixed-offset `ustar` signature at offset 257 doesn't work for embedded archives in SFX files.

## Decision Drivers
* SFX detection must find the correct archive start, not just any signature match
* ZIP archives can have CD headers before local file headers
* TAR SFX is not a real-world use case
* False positives should be reduced by validating candidates with format-specific probes

## Considered Options
1. Keep first-match behavior but remove problematic signatures
2. Iterate all candidates until one validates, remove CD and TAR signatures
3. Add a scoring/ranking system for candidate signatures

## Decision Outcome
ACCEPT: Option 2 — iterate all candidates, remove ZIP CD (`PK\x01\x02`) and TAR (`ustar`) signatures from the primary list.

Status: Implemented

### Implementation
- `src/sfx/signatures.rs`: Removed `PK\x01\x02` and `ustar` from SIGNATURES array
- `src/sfx/detection.rs`: Stage 3 now iterates all candidates via `for &(offset, format) in &signatures_found`, returning the first that passes format-specific validation
- Changed from `detected()` (confidence 1.0) to `probable()` (confidence 0.9) since these are signature-only matches without full backend parsing

## Consequences
* Good, because detection is more robust against false-positive first matches
* Good, because ZIP CD headers no longer cause incorrect offset detection
* Bad, because TAR SFX files (extremely rare) are no longer detected
