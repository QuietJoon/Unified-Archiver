---
type: Design Note
title: "OI-0058-001 — Feature-first footprint split & facade crates: cons / pros"
description: "Status: parked design note (not an ADR)."
tags: [design-note, ADR-0058, OI-0058-001]
timestamp: 2026-04-29T00:00:00Z
status: active
---

# OI-0058-001 — Feature-first footprint split & facade crates: cons / pros

**Status:** parked design note (not an ADR). Triggers a planning round when v0.3 stabilises.

## Problem (one sentence)
Read-only consumers of the crate today still pay the full dependency cost (libarchive, RAR, sevenz writer, modify, SFX, …) because operation families and backend formats are not feature-gated.

## Sketch of the chosen plan (per AD 0058)
Stage 1 — feature flags only, single crate:
- functional: `read`, `integrity`, `create`, `modify`, `full`
- format/backend: `zip-read`, `zip-write`, `zip-crypto`, `sevenzip`, `rar`, `libarchive`, `sfx`

Stage 2 — facade crates: `unified-archive-core`, `unified-archive-read`, `unified-archive` (full).

## Pros
- Read-only path drops sevenz writer, RAR FFI, libarchive C library, full mmap-cache stack — measurable binary-size + build-time cut for the common consumer profile.
- Build-time native-dep elimination: a `read`-only build needs no C toolchain at all when `libarchive` and `rar-support` are off.
- Forces a long-lived no-features-on test matrix in CI; bugs that hide behind `--all-features` (the current default test command) get caught.
- Aligns the crate with the rust-ecosystem norm (`tokio`, `serde`, `reqwest`) where consumers opt in to surface area.

## Cons
- Feature-gating spreads `#[cfg(feature = "...")]` across every module; the crate has historically used per-backend match arms in extraction.rs / inspection.rs / etc. — every match becomes a `match … #[cfg(feature)]` ladder. Maintenance load is real.
- The trait-based dispatch (`ReadBackend`) helps with the read side but not with the write/modify side, where the enum dispatch is still per-variant. Footprint split gates whole variants behind features — so the enum becomes feature-conditional, which is awkward for downstream pattern-matching.
- D2 (Archive god-object split) needs to land first or feature gating fights with mode discrimination — a `v2-api`+feature-gates combinatorial explosion in CI.
- Facade crates require a release coordination story (semver across three crates moving in lockstep). The current single-crate workflow is simpler.

## Sequencing (when this work is woken up)
1. Land D2 (Phase 8 of the deferred-OI-closure plan) so each mode lives in its own type and the feature gates only need to choose `pub use ReadArchive` vs `pub use ReadArchive + WriteArchive + ModifyArchive`.
2. Stage 1 functional features: 1 day per feature for the gating boilerplate + CI matrix.
3. Stage 1 format features: 0.5–1 day per backend (mostly cfg-gating the backend module + match arms).
4. Measure (`cargo bloat`, `cargo build --timings`) before flipping default features.
5. Stage 2 facade crates: 1 week of release-coord setup, mostly in CI/docs.

Total effort estimate: 2–3 weeks for Stage 1, ~1 week for Stage 2.

## Why this isn't an ADR
AD 0058 already captured the policy. This design note records the cons/pros so the next planning round doesn't have to re-derive them. The actual decision (start now vs. wait until D2 lands) is the user's call.
