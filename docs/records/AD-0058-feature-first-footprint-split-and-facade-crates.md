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

## Amendment (2026-09-06, the six open choices are ruled — Stage 1 becomes startable)

The 2026-09-04 amendment above enumerated six choices that had to be made before
Stage 1 could start, and concluded "OI-0058-001 stays Type 2." Two were answered
by the owner shortly after (keep the shipped `rar-support` name; delete the
cyclic clause in the `create` row). The remaining four are ruled here, together
with the framing correction that amendment raised. **Stage 1 is now startable.**

These are recorded as decisions, not proposals, so that an implementer is not
stranded mid-refactor. Each is cheap to overturn — they are table entries and
feature lists, not code — and the reasoning is given so that overturning one does
not require re-deriving why it was chosen.

### 1. `modify` depends on `libarchive`, and the operation table is corrected

`modify = ["read", "create", "libarchive"]`.

AD-0071 (2026-09-02) ruled that `Archive::open_modify`'s unconditional libarchive
binding is **permanent**, not a deferral. The operation table's "Depends on `read`
and `create`" was written before that ruling and is wrong as of it. Declaring the
dependency is the honest option: the alternative — marking `libarchive` optional
while `modify` is on — breaks ZIP and 7z `commit_changes()` at link time, and
that is a ruling to reverse, not a patch to write.

Consequence, stated rather than discovered later: **there is no libarchive-free
modify profile**, and the standalone `modify` gate profile pulls libarchive by
construction. A consumer who wants modification pays for libarchive. That is the
cost AD-0071 accepted; this only writes it into the matrix.

### 2. `external-rar-create` implies `create`, and stays out of `full`

`external-rar-create = ["create"]`. It does **not** imply `rar`, and it is **not**
a member of `full`.

It does not imply `rar` because it drives an external WinRAR CLI; it is unrelated
to the vendored UnRAR *reader* that `rar` builds. Coupling them would force a
C++ build on a consumer who only wants to shell out to a binary.

It stays out of `full` because `full` has to mean "everything this crate can do
from its own code", and this feature can do nothing without a third-party WinRAR
installation, on Windows only. Folding it in would make `full` mean different
things on different hosts — and constraint 2 leans on `full` being a *green test
lane*, which a platform-dependent, externally-provisioned aggregate cannot be.
It already has the right home: its own `check-windows-msvc --features
external-rar-create` lane in `scripts/release-gate.sh`.

### 3. The six profiles, defined

`read` alone links no backend and reads nothing, which is what made
`read-minimal` undefined. Each profile below names a complete selection:

| Profile | Features |
| --- | --- |
| `read-minimal` | `read`, `zip-read` |
| `read-zip` | `read`, `zip-read`, `zip-crypto`, `sfx` |
| `read-all-formats` | `read`, `zip-read`, `zip-crypto`, `sevenzip`, `rar-support`, `libarchive`, `sfx` |
| `create` | `create`, `zip-write`, `sevenzip`, `libarchive` |
| `modify` | `modify`, `zip-read`, `zip-write`, `sevenzip`, `libarchive` |
| `full` | every feature above except `external-rar-create` |

`read-minimal` is ZIP-only and deliberately so: it is the **one format, no C
toolchain** floor, and that floor is the footprint claim this whole record
exists to substantiate. A `read`-only profile that reads nothing would measure
an artefact rather than a product.

`read-zip` earns its separate existence by adding the two things a real ZIP
consumer actually hits — encrypted entries and SFX — so the gap between it and
`read-minimal` measures the cost of completeness rather than the cost of a
second format.

`create` names the writable formats the crate description already advertises
(ZIP, 7z, TAR family). `modify` names ZIP and 7z, the two modifiable formats,
and inherits `libarchive` through item 1 above.

Because constraint 2 gates on `read-minimal` and `full` being green lanes, these
two sets are the ones that decide what the gate proves. They are the widest and
narrowest selections in the table, which is the property that makes the pair
worth gating on.

### 4. A disabled format reports a distinct state, not `Support::None`

`Support` gains a variant for "this format is supported by the crate but was
compiled out", carrying the feature name that would enable it.

`Support::None` means *this crate cannot do this*. For a feature-gated format
that is false, and the falsehood is the actionable kind: the caller can fix it by
enabling a feature, but only if something tells them so. Reporting `None` makes
a compiled-out ISO indistinguishable from a format the crate never supported, and
sends the caller looking for a different library.

This is the cheapest of the four: `Support` is already `#[non_exhaustive]`, so the
variant is not a breaking change, and its own rustdoc already anticipates exactly
this case. Naming the feature in the variant is what makes the diagnostic
actionable rather than merely accurate.

### 5. Framing correction: growing `default` to preserve behaviour is not a "flip"

The 2026-09-04 amendment observed that Stage 1 step 1 requires the default to
stay equivalent to today's crate, and that today ZIP, 7z and libarchive are not
features at all — so preserving behaviour means `default` gains roughly eleven
names. It then asked whether that counts as a "flip" under constraint 2, and left
it unsettled.

**It does not.** Constraint 2 exists to stop the default set *losing* capability
before both profiles are proven — that is the failure it protects against, and
the reason it names `read-minimal` and `full`. Adding names that reproduce
today's exact capability set changes nothing observable to any consumer: same
formats, same operations, same build. A re-expression is not a reduction.

So "Stage 1 with no default flip" is a coherent thing after all, and Stage 1 is
not gated on constraint 2. The genuine flip — narrowing `default` — is a later
step and keeps the constraint.

### Effect

OI-0058-001 moves from Type 2 to **Type 1**: the decisions are made and the work
is ready to implement. It remains large — twelve features, `cfg`-gating the
backend enum, format routing and public re-exports, gating `build.rs`'s
unconditional libarchive probe, six gate profiles, and the Required Action 5
measurement — so it should land in increments. The recommended first increment is
unchanged from the owner's 2026-09-02 ruling: **format features only**, which is
where the UnRAR payoff lives and is mechanically checkable.
