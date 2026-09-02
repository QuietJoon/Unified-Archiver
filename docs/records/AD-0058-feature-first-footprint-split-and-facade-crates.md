---
type: ADR
title: "AD 0058: Feature-first footprint split and facade crates"
description: "Planned"
tags: [decision, ADR-0058]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0058: Feature-first footprint split and facade crates

Status: Planned

## Plan

Split the crate footprint in two stages. Stage 1 is feature-first inside the
current crate. Stage 2 introduces read-only and full facade crates only after
the feature matrix is stable.

### Stage 1: Functional features

Define operation-level features before changing package layout:

| Feature | Scope |
|---|---|
| `read` | Open archives, list entries, extract entries, stream entries, format detection needed for read paths. |
| `integrity` | Payload validation and checksum verification. Depends on `read`. |
| `create` | New archive creation APIs and writer backends. Depends on the required format writer features. |
| `modify` | Copy-on-write archive modification APIs. Depends on `read` and `create`. |
| `full` | Compatibility aggregate for read, integrity, create, modify, and all supported formats. |

Migration steps:

1. Add the functional feature names while keeping the current default behavior
   equivalent to today's crate.
2. Gate public modules, re-exports, examples, and tests by operation.
3. Replace runtime `write_mode_only` / unsupported-mode paths with compile-time
   absence where the operation feature is disabled.
4. Document read-only usage through `default-features = false` plus explicit
   read and format features.

### Stage 1: Format features

Define format/backend features orthogonally to operation features:

| Feature | Scope |
|---|---|
| `zip-read` | ZIP inspection and extraction through the read backend. |
| `zip-write` | ZIP creation support. Depends on `create`. |
| `zip-crypto` | Encrypted ZIP read support. Depends on `zip-read`. |
| `sevenzip` | 7z read/extract support, and 7z creation where supported by the selected writer path. |
| `rar` | RAR/RAR5 read/extract support and vendored UnRAR build. |
| `libarchive` | TAR family, raw compressed single-file formats, ISO, and libarchive-backed creation. |
| `sfx` | Self-extracting archive detection and offset opening. Depends on read-capable format features. |

Migration steps:

1. Mark backend dependencies optional in `Cargo.toml`.
2. Gate backend modules and `ArchiveFormat` routing by format feature.
3. Gate `build.rs` probes and native builds so `libarchive` and `rar` are not
   touched unless their features are enabled.
4. Add targeted CI/test profiles:
   `read-minimal`, `read-zip`, `read-all-formats`, `create`, `modify`, and
   `full`.
5. Measure dependency tree, build time, and binary size for `read-minimal` and
   `full` before changing defaults.

### Stage 2: Facade crates

Introduce package-level facades only after Stage 1 proves the feature split:

| Crate | Role |
|---|---|
| `unified-archive-core` | Internal implementation crate with the full feature matrix. |
| `unified-archive-read` | Small public facade for read-only consumers. Depends on core with `read` plus selected default read formats. |
| `unified-archive` | Consolidated public facade for the full API. Depends on core with `full`; preserves the existing crate name for compatibility. |

Migration steps:

1. Move implementation into `unified-archive-core` without changing public API
   names.
2. Make `unified-archive` a full facade re-exporting core types and preserving
   current examples.
3. Add `unified-archive-read` with only `ArchiveReader` / read-side exports.
4. Keep read-only facade documentation separate from full facade documentation
   so small-footprint users do not need to learn create/modify concepts.
5. Publish the facade split in a minor pre-1.0 release with explicit migration
   notes for users who previously relied on default features.

### Ordering constraints

1. Do not create facade crates before optional dependencies and feature gates
   are complete.
2. Do not flip default features until `read-minimal` and `full` profiles have
   CI coverage.
3. Keep `integrity` as a read-side extension, not a separate crate, because it
   reuses archive decoding and backend traversal.
4. Keep the full facade as the compatibility path; optimize the read-only path
   through feature selection first, package layout second.

## Amendment (2026-09-02, owner ruling — Stage 1 accepted, Stage 2 parked until measured)

This record has said "Status: Planned" since it was written, and nothing in the tree has moved:
`[features]` still holds only `default = ["rar-support"]`, `rar-support`, `external-rar-create` and
`v2-api`; none of Stage 1's operation features (`read`, `integrity`, `create`, `modify`, `full`) or
format features (`zip-read`, `zip-write`, `zip-crypto`, `sevenzip`, `rar`, `libarchive`, `sfx`)
exists; no dependency is marked `optional = true`; there is no workspace and no facade crate.

The 2026-07-19 decision review recommended re-scoping this — "commit Stage 1 features before the
v0.4 v2-api default-flip; downgrade Stage 2 facade crates to *revisit only if measurement proves
features insufficient*" — and that recommendation was never ruled on. **The owner has now accepted
it.**

**Stage 1 is accepted and becomes ordinary queued work.** Its payoff is concrete and nameable rather
than architectural taste: a consumer that only reads archives currently compiles the vendored UnRAR
C++ tree, because `rar-support` is a default feature and nothing is optional. Feature-gating is what
lets that consumer stop paying for it.

**Stage 2 — the facade crates — is parked, not retired.** The distinction matters: the record is not
wrong, it is unmeasured. Splitting the package costs a multi-crate layout, a release process for
each, and a compatibility surface between them, against a benefit nobody has quantified. It is
revisited only if measurement shows feature selection alone leaves the footprint unacceptable. If
that measurement never happens, Stage 2 never happens, and that is an acceptable outcome rather than
an oversight.

**One thing this ruling deliberately does not do.** It does not schedule Stage 1 against the
`v2-api` default flip, which is how the 2026-07-19 wording framed it ("before the v0.4 flip"). That
flip is its own open question and is itself gated on CI existing — Cargo.toml's own comment records
the constraint: "AD 0058 ordering constraint 2 forbids flipping default features until the
read-minimal and full profiles have CI coverage, and this repository has no CI". Tying Stage 1 to a
flip that is blocked on CI would inherit that block for no reason. Stage 1 is independently useful
and is sequenced on its own.

Scale, stated so nobody starts it expecting a small change: Stage 1 touches optional dependencies,
`#[cfg(feature)]` gating across the backend enum, `ArchiveFormat` routing, the build.rs probes,
examples and tests, and it needs a test profile per feature combination that matters. It is
multi-week work, and the six-profile test matrix it implies is the part most likely to be
underestimated — which is also the part that runs headlong into the absent CI.

This record stays **active** and remains the governing baseline for Stage 1.
