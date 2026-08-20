---
type: ADR
title: "AD: Staged SFX Detection Pipeline"
description: "Accepted"
tags: [decision, ADR-0006]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# AD: Staged SFX Detection Pipeline

Status: Accepted

## Context and Problem Statement
Self-extracting archive (SFX) identification must balance detection speed and false-positive control. Naively scanning every file for archive signatures is expensive, while skipping validation leads to misidentification.

## Decision Drivers
- Fast early exits for non-executables
- Bounded CPU and I/O cost per detection attempt
- False positive control to avoid misidentifying non-SFX executables

## Considered Alternatives
- **Full deep parsing for all candidate formats** -- rejected for excessive complexity and latency on non-SFX inputs.

## Decision Outcome
We decided to implement a 3-stage detection pipeline: executable validation, signature scan up to 1 MB, and basic offset/data validation, because it provides fast rejection of non-executables while keeping resource usage bounded.

## Consequences
- Good: Fast early exits for non-executables and bounded CPU/IO cost per detection.
- Bad: `open_at_offset` backend opening remains unimplemented, so the full SFX open flow is incomplete and requires future work.

## Amendment (2026-07-22, R0081 I3)

Stage 3 no longer emits a floating-point confidence score. The pipeline's
output is now the tri-state `SfxConfidence` enum (`NotSfx` / `Probable` /
`Confirmed`) carried on `SfxDetectionResult`, accompanied by a `Vec<String>`
evidence list naming *why* a candidate matched (stub kind, archive signature,
offset). This does not change the pipeline's accept/reject decisions:

- Stage 1/2 rejection → `SfxConfidence::NotSfx` (was `confidence 0.0`).
- Stage 3 structural-probe pass → `SfxConfidence::Probable` (was the clamped
  `~0.9`).
- `SfxConfidence::Confirmed` remains the reachable-but-not-yet-produced verdict
  reserved for the deferred confirmed-detection patterns (arch/gate AD 0015).

Rationale: the float provably carried no information — only `0.0` and the
clamped `~0.9` were ever produced — so it forced callers to compare against
magic thresholds and spawned the now-retired clamp record (AD 0014) and a
stale deferred-confirmed record (AD 0015). The enum names the reachable states
directly. See I3 and the OI-0080-004 typed-multipart direction (typed results
over heuristic floats).

## Amendment (2026-08-04, decision-review-2026-07-19 §B — stale `open_at_offset` consequence)

The "Bad" consequence above is stale: `Archive::open_at_offset` has been implemented since
2026-04-18 (DEF-001 closure — non-zero offsets stage the payload to a tempfile under the AD 0040
16 GiB ceiling), so the full SFX open flow (`Archive::open_sfx` → `open_at_offset`) is complete;
see the MADR-0023 supersession amendment of the same date.
