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

## Amendment (2026-09-03, AD-0070 — what ordering constraint 2 actually requires)

Ordering constraint 2 reads *"Do not flip default features until `read-minimal`
and `full` profiles have CI coverage"*, and Stage 1's Required Action 4 asks for
"targeted CI/test profiles". Both were written when "CI" meant an unexamined
hosted service. AD-0070 has since defined it as recorded per-platform
verification, and the two clauses need restating against that definition rather
than deleting — the property they protect is real and unchanged.

**Constraint 2 is restated as:** do not flip default features until
`read-minimal` and `full` are test lanes of `scripts/release-gate.sh`, the
remaining Stage 1 profiles (`read-zip`, `read-all-formats`, `create`, `modify`)
are at least check lanes, and one `docs/verification/` record shows them green
on the fingerprint being flipped. **Required Action 4 is restated as:** add the
six profiles as release-gate lanes.

**Two things this changes in practice.**

First, the constraint is now satisfiable on the dev host. Every Stage 1 profile
compiles and tests on macOS; the one genuinely host-dependent sub-property —
libarchive discovery differing per OS — belongs to OI-0065-001 and is not
Stage 1's to carry. So constraint 2 never needed a service, only a recipe and a
record.

Second, the shape is priced rather than assumed, because the naive reading is
expensive. A warm full `--all-features` test run on this host is roughly
seventeen minutes across 41 test binaries; six *test* profiles would be two to
three hours serial, and considerably worse under the four-to-six foreign cargo
runs this machine routinely carries. The recommended shape is therefore
**tests for the two profiles the constraint actually names, and `cargo check`
for the other four** — check lanes link no executables, so they also cannot hit
the macOS first-exec admission stall that cost this project a 66-minute
abandoned gate. That protects the same property at about a quarter of the cost,
and the record must say which lanes were check-only so nobody reads them as
test coverage.

**Not a blocker for `v2-api`.** The `Cargo.toml` comment on that feature used to
cite this constraint. It was borrowing a constraint written about *footprint*
features for a flag that gates a module, two `cfg` sites and some in-crate
tests, and no platform-specific code at all. That comment is corrected; the two
are sequenced independently, consistent with the 2026-09-02 ruling above.

## Amendment (2026-09-04, Stage 1 is not ready to implement as written)

A 2026-09-04 reconciliation re-typed OI-0058-001 from "needs decision" to
"ready to implement" on the reading that this record specifies Stage 1
concretely and AD-0070 already removed its only blocker. Three independent
readers were then asked to refute that; two did, and this amendment records
what they found. **Stage 1 remains correct as a direction and is still not
blocked — but it is not startable, because six choices inside it are unmade.**
Two of the six are contradictions with records written *after* the Stage 1
tables above, which the 2026-09-02 and 2026-09-03 amendments did not reconcile.

1. **`modify` is not orthogonal to `libarchive`, and AD-0071 froze that.** The
   operation table says `modify` depends on `read` and `create`. In the tree,
   `Archive::open_modify` binds the libarchive backend unconditionally for
   every format, and **AD-0071** (2026-09-02) rules that this is permanent
   rather than a deferral. So `modify` in fact requires `libarchive`, and the
   standalone `modify` gate profile cannot exist as specified. Whether the
   dependency becomes `modify = ["read", "create", "libarchive"]` or the
   profile is redefined is unmade. This is the item most likely to stop an
   implementer mid-change: marking `libarchive` optional will break ZIP and 7z
   `commit_changes()`, and the fix is a ruling, not a patch.
2. **The feature name `rar` collides with the shipped `rar-support`.** MADR-0020
   is `status: active` and chose `rar-support`; it is a default feature, is used
   at roughly 190 `cfg` sites, and is published surface named in the crate
   description and in README. The tables above say `rar`. Rename, alias, or
   deviate — none is ruled, and a rename removes a documented feature from a
   released 0.4.0 crate.
3. **`external-rar-create` has no slot in either matrix.** It exists, has its
   own release-gate lane, and the tables do not mention it. Whether it implies
   `create`, implies `rar`, or belongs inside `full` — which would change what
   `full` means on a non-Windows host — is unruled.
4. **The dependency tables are cyclic.** The operation table has `create`
   depending on the format writer features; the format table has `zip-write`
   depending on `create`. Cargo cannot express both, so the implementer must
   choose which way the matrix points. That is structural, not a detail.
5. **The six profiles have no defined feature sets.** `read-minimal`,
   `read-zip`, `read-all-formats`, `create`, `modify` and `full` appear
   everywhere as bare names and nowhere as selections. `read` alone links no
   backend and reads nothing, so `read-minimal` is undefined. This is
   load-bearing: the restated constraint 2 gates the default flip on
   `read-minimal` and `full` being green test lanes, so whoever picks the sets
   picks what the gate proves.
6. **What a disabled format reports is an open API question.** With `libarchive`
   off, does `ArchiveFormat::Iso` report `Support::None` — indistinguishable
   from a format the crate never supported — or a new behind-a-feature state?
   `Support` is `#[non_exhaustive]` and its own rustdoc anticipates exactly this
   variant. MADR-0020's precedent covers the error path, not
   `FormatCapabilities`, which is public.

Two things are genuinely settled and should not be re-litigated: `ArchiveBackend`
is `pub(crate)`, so gating it is internal and mechanical; and `ArchiveFormat`,
`Support`, `FormatCapabilities`, `ArchiveError` and `Operation` are already
`#[non_exhaustive]`, so gating variants is not a semver break. Gating `build.rs`'s
unconditional libarchive probe is likewise real, unambiguous work.

One framing correction. Stage 1 step 1 requires the default to stay equivalent
to today's crate, and today ZIP, 7z and libarchive are not features at all — so
preserving behaviour means `default` gains roughly eleven names. Whether that
counts as a "flip" under restated constraint 2, which demands the lanes and a
verification record *first*, is itself unsettled. "Stage 1 with no default flip"
is therefore not a thing these records establish.

**Effect:** OI-0058-001 stays Type 2. The work needed before it can start is a
ruling on the six items above — most of which are cheap to decide and none of
which is cheap to discover halfway through a large refactor.
