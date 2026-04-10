# ADR: Staged SFX Detection Pipeline

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
