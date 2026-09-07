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

## Amendment (2026-09-06, first increment landed — `sevenzip`, with one ruling corrected)

The format-features increment recommended above is started. `sevenzip` is the
first one, chosen because it is a pure-Rust dependency (so the gate is a clean
`dep:` selection with no C toolchain in the way) and because it is small enough
to prove the shape before the 505-reference `libarchive` split is attempted.

### Required Action 5, partially delivered: the dependency measurement

The footprint claim is now measured rather than asserted. Dropping `sevenzip`
removes **ten crates** from the dependency graph — not merely the code paths:

`sevenz-rust2`, `lzma-rust2`, `ppmd-rust`, `bzip2`, `libbz2-rs-sys`, `sha2`,
`aes`, `cbc`, `cipher`, `block-padding`.

154 crates with default features, 144 without, and the delta is attributable to
`sevenzip` alone (`--no-default-features` vs `--no-default-features --features
sevenzip` gives the same ten). Build-time and binary-size measurements are still
outstanding.

### Correction to decision 4 of the previous amendment

That decision said a disabled format should report a new behind-a-feature state
instead of `Support::None`. Implementing it surfaced a fact the decision was made
without: **`capabilities()` does not report `None` for a disabled format today.**
It reports `Full`, and R0001-0063 documents that as deliberate — the capability
matrix describes the *format*, is build-independent by design, and RAR already
advertises `Full` in a build compiled without `rar-support`.

So the premise was wrong, and putting `BehindFeature` into that matrix would have
silently reversed a documented decision. The variant is right; its home was not.

`Support::BehindFeature { feature }` is therefore reported by a **new,
build-aware accessor**, `ArchiveFormat::availability()`, while `capabilities()`
keeps its build-independent contract untouched and pinned by a test. The two
answer different questions — "what can this format do" and "can I open this file
in this build" — and conflating them was the actual defect behind open question
6, rather than the choice of variant.

`Support::is_available()` is added alongside, so callers gating behaviour do not
have to enumerate variants of a `#[non_exhaustive]` enum.

### What this increment does not do

Only `sevenzip` is gated. `zip-read` / `zip-write` / `zip-crypto`, `libarchive`
and `sfx` are untouched, and the five operation features do not exist yet — so
the six profiles in the previous amendment's table remain definitions rather than
selectable configurations. `libarchive` is the hard one (505 references across 29
files) and inherits the `modify` dependency ruled in decision 1.

Coverage note worth carrying forward: gating a format feature tends to
over-gate tests. The first mechanical pass disabled four multi-backend tests in
`stream_bound_test.rs` wholesale because they *mentioned* a 7z fixture, silently
removing ZIP and TAR coverage from the minimal profile. They now skip only the
7z fixture. Expect the same trap on every later format feature, and check for it
rather than trusting a green minimal lane.

## Amendment (2026-09-06, second increment — `zip-crypto`, and a diagnostic that names the wrong crate)

`zip-crypto` is the second format feature. It is not a `dep:` gate like
`sevenzip` — it toggles the `zip` crate's own `aes-crypto` feature — and it is
the larger of the two by some margin.

### Measurement

**Twenty-two crates**, against `sevenzip`'s ten: `aes`, `cipher`, `block-buffer`,
`constant_time_eq`, `cpufeatures`, `crypto-common`, `digest`, and the rest of the
hashing/AES stack. The minimal profile is now **122 crates against 154 for
default** — a third of the graph gone, from two features.

That ordering is worth noting for the remaining work: the biggest footprint win
so far came from a *capability* of a format, not from a whole backend. The
remaining format features should be measured before being assumed cheap or
expensive.

### The bug this increment surfaced

With `zip-crypto` off, reading a WinZip-AES entry produced the `zip` crate's own
message: *"AES encrypted files cannot be decrypted without the aes-crypto
feature."*

Accurate, and useless. `aes-crypto` is **not a feature of this crate**. A caller
would search this `Cargo.toml` for it, find nothing, and have no way to reach the
thing that actually fixes their build. This is the same failure the previous
amendment's decision 4 was about — a diagnostic that is true but not actionable —
arriving by a different route, from a dependency rather than from our own code.

`is_aes_feature_missing` now classifies it and the error names `zip-crypto`.
The match is on the stable half of the upstream sentence rather than the whole
string, so a rewording upstream degrades to the generic format error instead of
silently mis-classifying; and the classifier is compiled unconditionally, so its
unit test runs in the ordinary build rather than only in the profile nobody uses
by default.

### A committed fixture, which this project mostly avoids

Most ZIP fixtures are written in-test through the `zip` crate. This one cannot
be: the test that consumes it exists to run in a build **without** `zip-crypto`,
which is exactly a build that cannot write a WinZip-AES archive. So
`tests/fixtures/test_aes256.zip` is generated out of band by the new
`scripts/generate-zip-fixtures.sh` and committed, following the same
run-by-hand-never-from-the-build rule as the 7z, RAR and TAR fixture scripts.

Both directions are pinned — the refusal names `zip-crypto` and does *not* leak
`aes-crypto`, and the same fixture decrypts when the feature is on — and both
were mutation-checked, including the end-to-end path, so the wiring is covered
and not just the predicate.

### Running total

| Feature | Crates removed | Kind |
| --- | --- | --- |
| `sevenzip` | 10 | optional dependency |
| `zip-crypto` | 22 | upstream feature toggle |

Still to do: `zip-read`, `zip-write`, `libarchive`, `sfx`, and the five operation
features.

## Amendment (2026-09-06, third increment — `libarchive`, and the C dependency is gone)

`libarchive` is the third format feature and the one this record was really
written for. It is unlike the first two in kind: `sevenzip` gates a cargo
dependency and `zip-crypto` gates an upstream feature, but libarchive is a
**system C library**, discovered by `pkg-config` in `build.rs` and linked as
`dylib=archive`.

### What actually changed, verified rather than asserted

The `build.rs` probe was unconditional. On a host without libarchive it
panicked —

> `libarchive not found. Install it with: brew install libarchive`

— for *every* consumer, including one who only ever wanted to read a ZIP and
would never reach the backend. That panic is now behind
`CARGO_FEATURE_LIBARCHIVE` (the form Cargo exposes features to build scripts in),
on both the Unix and Windows arms.

Verified in the emitted build-script output rather than by reading the source:
the `--no-default-features` build directory emits **no `rustc-link-lib` and no
`rustc-link-search` for archive at all**, against `dylib=archive` plus a link
search path in the default build. A read-only ZIP consumer now builds with no
system C library present.

The crate count does not move (154 default, 122 minimal) because libarchive is
not a cargo crate — which is exactly why the crate count was never the whole
measurement. The build-time system dependency is the thing that was removed, and
it is the one that decides whether the crate builds at all on a bare host.

### Decision 1 is now enforced by the compiler

The previous amendment ruled `modify = ["read", "create", "libarchive"]` on the
strength of AD-0071, and warned that the consequence was "there is no
libarchive-free modify profile". That is now a fact in the code rather than a
line in a table: `Archive::modify`'s backend binding diverges when the feature is
off, with an error that names AD-0071 and the feature. Everything after the
binding is unreachable in that configuration, which is the intended shape and is
marked as such rather than left for a future reader to puzzle over.

### Dead code that only the minimal profile can see

Turning the backend off stranded a scatter of helpers that are genuinely used —
`rename_noclobber` (all three platform arms), `path_to_cstring`,
`FileIdentity::capture` / `revalidate`, `stage_unknown_size_entry`, and two
`ExtractionPlan` cap fields. None is dead code; each simply has no caller in a
build with no libarchive, so each is `allow(dead_code)` **conditioned on that
configuration** rather than unconditionally. An unconditional allow would have
hidden real dead code in the default build, which is the failure mode the
minimal clippy lane exists to catch.

### The test-gating pass, and two ways it lied

The `sevenzip` amendment predicted the over-gating trap would recur on every
later format feature and said to check for it rather than trust a green minimal
lane. It recurred, once, and the check is what found it:
`contract_metadata_consistent_across_formats` opens `test.zip` **and**
`test.tar`, so the mechanical pass gated the whole test — but every assertion
with substance in it is about ZIP entry sizes, and the TAR half is one
`is_empty()` call. Gating it wholesale took the ZIP metadata contract out of the
minimal profile, and the lane stayed green while doing it. Only the TAR half is
gated now.

The second lie was new, and it was the automation's own guard. The gating loop
skips a test that is already gated by looking for the string `libarchive` in the
characters just before its `#[test]`. Two tests carry doc comments that mention
libarchive in prose — "via libarchive's `format_raw`", "when libarchive's first
header reports" — so the guard read documentation as a gate, reported
`SKIP already gated`, and left both ungated. The loop then exhausted its rounds
and reported **`gave up after 8 rounds`** rather than green.

That failure was loud, and it is worth being precise about why, because the
quiet version of it is the dangerous one. A guard that string-matches prose can
equally well fire on a test that genuinely needs gating in a run whose *other*
failures resolve — and then the loop exits green with a test silently dropped
from both profiles. The guard should match the attribute, not the word. Recorded
here because the next format feature will run the same script.

### The third lie was the worst, and it was `cargo test`'s default

The loop ran `cargo test --no-default-features` without `--no-fail-fast`, and
**cargo stops after the first test binary that fails.** `format_compatibility_test`
sorts early and was red every round because of the two tests the prose-matching
guard had skipped — so `integration_tests` never executed at all, in any of the
eight rounds. The loop was not converging slowly; it was structurally incapable
of converging, and its `gave up after 8 rounds` was the only reason anyone
looked.

Gating those two tests let cargo reach the next binary and **43 further failures
appeared at once** — roughly a third again of the whole gating pass, invisible
until the suites ahead of them were green. Had the two prose-skipped tests been
gated by hand at round one, the loop would have exited **green** with 43 tests
still ungated in a binary it never ran, and the minimal lane would have said so.

The rule this leaves: a mechanical gating pass must run with `--no-fail-fast`,
because the thing it is trying to enumerate is *all* failures, and the default
gives it one binary's worth. The same applies to any loop that treats a test
run as a worklist rather than a verdict.

Note that neither the L3/L4 release-gate lanes nor a human reading a summary
line would have caught this: the run really did fail, the loop really did report
failure, and the count of failures was simply truncated by a flag nobody chose.

### The mechanical pass reverted the previous increment's fix, in the same file

The most instructive failure of the increment. `tests/stream_bound_test.rs` is
where the `sevenzip` pass was caught over-gating four multi-backend tests, and
the fix was to loop over fixtures and `continue` past just the 7z one, so ZIP and
TAR stayed covered. That fix is still in the file, comment and all.

The libarchive pass then gated two of those same repaired tests **wholesale**,
because they now failed for a new reason — their `test.tar` fixture — and a
mechanical pass sees only pass/fail. The repair was still there, in the body of a
test that no longer ran in the minimal profile. Coverage of ZIP through
`StreamBound::DeclaredSize` and `StreamBound::Cap` was removed by the same
pass, in the same file, that a previous increment had already fixed for the same
reason.

Two things follow. First: **the audit must be run against the diff, not against
the tree's history** — "this file was fixed before" is not protection, because
the fix and the regression live at different granularities (the fix is inside the
body, the regression is an attribute above it). Second: a fixture loop is now the
established shape for a multi-backend test here, so the audit should treat
*gating a test that contains a fixture loop* as suspect on its face, before
looking at which fixtures it names.

Both looping tests now skip per-fixture on both features, and neither carries a
whole-test gate.

### The modify tests are decision 1, arriving as a bill

**230 tests run with `libarchive` and not without it** — measured as the delta
between the new isolation lane (1719 passed) and the minimal lane (1489), not
counted from the diff. 190 of those are the gates this pass added; the other 40
are the `ffi::libarchive_wrapper::*` unit tests, which ride on the module's own
gate and needed no attribute. The two figures reconcile exactly, which is the
check worth doing: a static count of added attributes and a measured lane delta
that disagree mean either a gate landed somewhere unintended or a module gate is
doing invisible work. **Sixty-three of the 190 route through
`Archive::modify` / `commit_changes`, and every single one operates on a ZIP
archive** — a format whose own backend is compiled in and working. They are gated
because AD-0071 puts modification on libarchive for *every* format, so they
cannot run, and the audit had to confirm one by one that this is decision 1 being
honest rather than over-gating.

That the ZIP share is 63 of 63 rather than a mixture is the number to carry
forward: the minimal profile does not lose *some* modify coverage, it loses the
whole of it, and the format it loses it for is the one that build exists to
serve. This is the cost that ruling predicted, now countable — a third of the
whole gating pass — and it is worth having in hand before the `modify` operation
feature is designed.

### Running total

| Feature | Crates removed | Kind |
| --- | --- | --- |
| `sevenzip` | 10 | optional cargo dependency |
| `zip-crypto` | 22 | upstream feature toggle |
| `libarchive` | 0 | **system C library + build probe** |

Three features, 154 → 122 crates, and the C toolchain requirement gone. Still to
do: `zip-read`, `zip-write`, `sfx`, and the five operation features.

Lane results for this increment, all with `--no-fail-fast`:

| lane | suites | passed | failed |
| --- | --- | --- | --- |
| `--no-default-features` | 43 | 1489 | 0 |
| `--no-default-features --features libarchive` | 43 | 1719 | 0 |
| `--all-features` | 43 | 2088 | 0 |

`clippy --all-targets -- -D warnings` is clean on both the minimal and the full
profile.

One process note that cost real time here and is not specific to this feature:
the harness reported two of these runs as "exit code 0" while cargo's own status
was 101 with failures. The `CARGO_EXIT=` marker written into the artifact after
the command terminates is the only reason neither was reported as green. That
marker is not ceremony.

## Amendment (2026-09-07, Required Action 5 completed — and the metrics disagree)

Required Action 5 asked for dependency, build-time and binary-size measurement.
The dependency half was done per-increment; this completes the other two, across
the whole feature matrix rather than one feature at a time. That turns out to
matter, because **the three metrics rank the features in almost opposite
orders**, and any one of them alone would have produced a wrong conclusion.

### Method

Binary size is `examples/inspect_archive` built `--release` and **stripped**
(`strip -x`), because unstripped sizes are dominated by debug symbols and
flatter the comparison. Each feature is measured alone against
`--no-default-features`, so the numbers attribute rather than accumulate. Build
cost is the wall time to re-run `build.rs` and relink, which isolates the
build-script contribution — the part a feature can make expensive independently
of how much Rust it pulls in.

### Results

| feature | crates removed | stripped bytes added | `build.rs` cost |
| --- | ---: | ---: | --- |
| `zip-crypto` | **22** | **+17 KB** (+1.9 %) | none |
| `sevenzip` | 10 | **+581 KB** (+65 %) | none |
| `libarchive` | **0** | +197 KB (+22 %) | probe only (~0 s) |
| `rar-support` | — | +403 KB (+45 %) | **~21 s** (vendored C++) |

Minimal is 891 KB stripped; everything on is 1,897 KB — **2.13×**. The individual
deltas sum to 1,198 KB against a measured combined 1,006 KB, so roughly 16 % of
the per-feature cost is shared and disappears when features are combined. Feature
costs are not additive, and a footprint claim that adds them up overstates.

### What each metric would have told you on its own

`zip-crypto` removes **22 crates — more than any other feature — and 17 KB of
binary.** `sevenzip` removes 10 crates and **581 KB**. By crate count
`zip-crypto` is the biggest win available; by binary size it is nearly free to
keep, and `sevenzip` is thirty-four times more valuable. The zip-crypto
amendment's caution that "the remaining features should be measured rather than
assumed cheap or expensive" was right, and understated: crate count here is not
an incomplete proxy for footprint, it is an **actively misleading** one, because
a crate's presence in the graph says nothing about how much of it survives
`--gc-sections` and inlining.

`libarchive` inverts it the other way. It removes **zero crates**, adds a middling
197 KB, and costs no measurable build time — by all three numbers it looks like
the least interesting feature in the table. It is the most important one, because
its cost is not on any of these axes: without it the crate **does not build at
all** on a host with no `libarchive` installed. A prerequisite is not a
quantity, and Required Action 5's three metrics cannot see it.

`rar-support` is the only feature whose dominant cost is time — ~21 s of vendored
C++ per clean build, invisible to both other metrics.

### Consequence for the profiles

`read-minimal` (ZIP only, no C toolchain) is 891 KB stripped and builds with no
system library. That is the footprint claim this record exists to substantiate,
and it is now a measured number.

The ranking to use when choosing what to gate next is **binary size**, not crate
count — with the standing exception that a feature carrying a build prerequisite
or a long C/C++ compile is worth gating regardless of what it weighs. On those
grounds the remaining `zip-read` / `zip-write` / `sfx` split should be expected
to be cheap in bytes, and should be justified on API-surface and
prerequisite grounds rather than by a footprint number it probably will not
deliver.

## Amendment (2026-09-07, fourth increment — `sfx`, and three blind spots in the method)

`sfx` is the fourth format feature: SFX detection and offset opening, per this
record's own feature table. It is the cheapest increment so far to *write*
(15 files, 112 insertions, against `libarchive`'s 49 and 385) and the one that
found the most wrong with how the previous three were verified.

### Scope, and one thing deliberately left ungated

Gated: the `sfx` module (detection, signatures, stub types, result), the five
public entry points (`detect_sfx`, `open_sfx`, `open_with_sfx_progress`,
`open_at_offset`, `extract_stub`) on both `Archive` and the v2 facade, and the
two SFX fallbacks inside `Archive::open` / the password-taking open.

**Not** gated: `PayloadWindow` and the backends' `payload_offset` field. The
field defaults to 0 and only the offset-open paths ever set it, so with the
feature off the window wraps at offset 0 and is a no-op; and the RAR4 recovery
walk drives `PayloadWindow` directly, independent of SFX. Gating a `u64` and a
219-line adapter would have bought nothing and put a compound `cfg` on both
backends. Public API *shape* also stays build-independent — `PayloadAccess` keeps
both variants and `SfxStagingProgress` keeps its methods under a conditional
`allow(dead_code)` — consistent with R0001-0063 and with what `capabilities()`
already does.

### Blind spot 1: neither extreme lane can see a cross-feature reference

`Archive::first_rar_signature_before` is gated on `rar-support` and calls
`crate::sfx::signatures::scan_for_signatures`. A build with **RAR on and SFX
off** therefore did not compile — and nothing in the existing verification could
have told us. With everything on it compiles; with everything off both sides are
`cfg`'d away. It was found by sweeping nine feature *combinations*, not by either
extreme.

The general shape: one feature's code may reference another feature's module, and
that is invisible to any check that only tests all-on and all-off. The fix in the
code is a compound `cfg(all(...))` on the three items involved. The fix in the
method is that **every format feature needs its own isolation lane** — so this
increment adds two, `test-sfx-only` and `test-rar-support-only`. `rar-support`
never had one, and a `rar-support`-only lane is precisely what would have caught
this without a combination sweep.

### Blind spot 2: a mechanical gating pass cannot see a test that is already gated

The compile-error-driven pass skipped every SFX test that was already behind
`#[cfg(feature = "libarchive")]`, `sevenzip`, or `target_os`, because a test that
is `cfg`'d out raises no error. Four whole files of SFX tests were left partly
gated, and would have failed to compile in a `libarchive`-on / `sfx`-off build —
the same class of hole as blind spot 1, arriving from the other direction.

Converting those four files to whole-file `#![cfg(feature = "sfx")]` fixes it and
composes correctly with the inner per-test gates. The rule: when a mechanical
pass is driven by failures, anything already excluded is invisible to it, so
prefer a whole-file gate whenever a file is *about* the feature.

### Blind spot 3: `--all-targets` does not include doc tests

The nine-combination sweep used `cargo check --all-targets`, which does **not**
build doc tests. Two doc examples calling `detect_sfx` and `open_sfx` passed the
entire sweep and failed the lane. They are now feature-aware (a hidden
`# #[cfg(feature = "sfx")] # { ... # }` around the SFX lines) rather than
downgraded to `ignore`, so they stay compile-checked in the default build.

### What the new `rar-support` lane found, which was not about `sfx` at all

The `test-rar-support-only` lane was added above as insurance for a hole this
increment created. On its first run it failed with **32 failures**, none of them
caused by this increment. They were pre-existing, and they had been invisible
because no build had ever had RAR without everything else.

The pattern: a multi-format test gated `#[cfg(feature = "rar-support")]` because
it *mentions* a `.rar` fixture, while also opening `test.zip` and `test.7z`. That
has two consequences, and the second is the serious one:

1. With RAR on and the others off, the test runs and fails on a backend that is
   not there. Loud, which is why the lane found it.
2. With RAR **off**, the test does not run at all — so its ZIP coverage silently
   left the minimal profile. Quiet, and it had been that way for as long as the
   gates existed.

Two files said so in their own header comments, accurately, and nobody had read
them as a defect:

> `tests/integration/format_compatibility.rs`: "This suite's cases are
> `rar-support`-gated, so the minimal profile compiles the file to nothing"

> `tests/property_tests.rs`: "Every property in this file is `rar-support`-gated
> … On the minimal profile this file compiles to nothing."

A whole cross-format compatibility suite and the entire property-test suite were
absent from every build without RAR — including `read-minimal`, the profile this
record exists to substantiate. The comments describe the mechanism exactly; they
frame it as a `-D warnings` housekeeping note rather than as lost coverage.

The fix is the same shape as every other over-gating fix here, applied at the
list rather than the test: `common::backend_available(fixture)` filters a fixture
list to the backends compiled into this build, so each profile covers exactly
what it can read. Gate counts after:

| file | `rar-support` gates before | after |
| --- | ---: | ---: |
| `tests/integration/format_compatibility.rs` | 14 | **0** |
| `tests/property_tests.rs` | 12 | **1** |
| `tests/performance_test.rs` | 10 | 4 |
| `tests/integration/extraction.rs` | 8 | 3 |

The one gate left in `property_tests.rs` is the property that opens `test.rar`
directly and asserts RAR CRC32 behaviour — genuinely format-specific. The
proptest strategies now draw from `available_fixtures()`, which always contains
ZIP, so `select` never sees an empty list.

**The general lesson, and it outranks the `sfx` work in this amendment.** A
feature gate on a test is a claim that the test is *about* that feature. When the
test is about several, the gate is wrong in both directions at once, and only one
of those directions is ever visible: the failure. A per-feature isolation lane is
what converts the invisible direction into the visible one. That is the argument
for having one per feature, and it is now evidence rather than symmetry — the
lane paid for itself on its first run, against a defect that predates every
increment in this record.

### And what the `sevenzip` lane found: the feature does not mean what its name says

Running all five isolation lanes rather than only the one this increment needed
turned up a second, unrelated defect — **6 failures on `sevenzip`-only**, and a
fact about the feature matrix that was nowhere written down:

**`sevenzip` gives 7z *read*. 7z *write* needs `libarchive`.** Creation and
modification of 7z route through `archive_write_set_format_7zip` in the
libarchive writer, not through `sevenz-rust2`. So a build with `sevenzip` and
without `libarchive` reads 7z and cannot create or modify it — which is the
honest routing, and the `Unsupported` error it produces is correct. What was
wrong is that six tests covering 7z *writes* were gated on `sevenzip` alone.

This is the same shape as decision 1's `modify = [..., "libarchive"]`, arriving
for a format feature instead of an operation feature, and it deserves the same
treatment: a feature name that implies a capability it does not deliver is a
documentation defect even when the code is right. `Cargo.toml` now says so at
the feature definition, where someone selecting features will actually read it,
and the six tests carry `cfg(all(feature = "sevenzip", feature = "libarchive"))`.

Worth stating plainly, because it generalises past this record: **the isolation
lanes are not a formality.** Two of the five found real defects on their first
run — `rar-support` a coverage loss predating every increment here, `sevenzip` an
undocumented cross-feature dependency — and neither was reachable from the
all-on or all-off lanes that this project ran for its whole history.

### Measurement

| | stripped |
| --- | ---: |
| minimal, before this increment | 891,592 |
| minimal, with `sfx` gated out | **837,304** |
| cost of `sfx` | **+54,288** (+6.5 %) |

54 KB for 3,334 lines of Rust — cheap, as the 2026-09-07 Required Action 5
amendment predicted for the remaining format features, and further confirmation
that line count is no better a footprint proxy than crate count. It sits between
`zip-crypto` (+17 KB) and `libarchive` (+197 KB).

**This moves a number published one commit earlier.** That amendment gave the
`read-minimal` floor as 891 KB; extracting `sfx` lowers it to **837 KB**. The
figure was correct when measured and is superseded rather than wrong — but it is
worth noting that the headline footprint claim moves with every increment, so it
should be read as "the floor as of increment N", not as a fixed property.

### Lane results

| lane | suites | passed | failed |
| --- | --- | --- | --- |
| `--no-default-features` | 43 | 1294 | 0 |
| `--all-features` | 43 | 2088 | 0 |

The full-profile count is unchanged from before the increment, which is the
check that the gating removed nothing from the default build. The minimal count
drops 1489 → 1294, so `sfx` carries 195 tests.

Remaining after this increment: `zip-read` / `zip-write`, and the five operation
features.

## Amendment (2026-09-07, ordering — the last two format features are not separable from the operation features)

Four of the seven format features have landed as standalone increments
(`sevenzip`, `zip-crypto`, `libarchive`, `sfx`). The remaining two — `zip-read`
and `zip-write` — should **not** be attempted the same way, and this is worth
settling before someone starts one and discovers it mid-refactor.

### Why the pattern stops working here

Every profile in this record's own table pairs the ZIP features with an
operation feature, and never selects one without the other:

| Profile | Features |
| --- | --- |
| `read-minimal` | `read`, `zip-read` |
| `create` | `create`, `zip-write`, `sevenzip`, `libarchive` |
| `modify` | `modify`, `zip-read`, `zip-write`, `sevenzip`, `libarchive` |

`zip-write` on its own selects a ZIP writer backend with no creation API in
front of it, and `create` on its own selects a creation API whose only
always-present format has no writer. **Neither is a configuration any consumer
would choose**, so shipping either alone would add an isolation lane that tests
a combination nobody uses — and the value of those lanes, demonstrated twice in
the `sfx` increment, comes precisely from their being real configurations.

The other four features had no such partner. `sevenzip`, `libarchive`, `sfx` and
`zip-crypto` each name a backend or a capability that is meaningful with the
current always-on operation surface, which is why each worked as an increment.

### The other reason: the write path is where the entanglement is

Measured rather than assumed. `ZipWriter` is referenced from `modification.rs`
(11 sites), `archive.rs` (9), `creation.rs` (6) and — the one that matters —
`libarchive_wrapper/writer.rs` (5), so the ZIP writer and the libarchive writer
are not cleanly separable at the backend boundary either. Compare `sfx`, whose
whole public surface was five methods.

### Ruling

`zip-read` / `zip-write` land **together with** the five operation features, as
one increment, not before them. That increment is larger than any so far and
should be planned as such; it is the one that turns the six profiles from
definitions into selectable configurations, which is Required Action 4.

Nothing here changes the feature list or the profile table — this is a
sequencing ruling only. The four shipped format features are unaffected.

### What is already true, so the next increment starts from evidence

- Required Actions 3 and 5 are complete; the `read-minimal` floor is **837 KB
  stripped** and moves with each increment.
- Each shipped feature has an isolation lane, and the lane set is the thing that
  catches cross-feature defects — two of five found real ones on first run.
- `common::backend_available` is the established way to keep a multi-format test
  covering every backend a build actually has. The operation-feature increment
  will need the operation equivalent of it, because the same trap applies:
  a test gated on `create` that also asserts read behaviour loses the read half
  from every read-only profile, silently.
- AD-0071 fixes one shape in advance: `modify` depends on `libarchive` for every
  format, so there is no libarchive-free `modify` profile, and 63 modify tests
  all operate on ZIP.

## Amendment (2026-09-07, Stage 1 complete — the operation features and the ZIP split)

The last increment, and the one the 2026-09-07 sequencing ruling said had to be
a single unit: the five operation features (`read`, `integrity`, `create`,
`modify`, `full`) together with `zip-read` / `zip-write`. Stage 1's feature list
is now fully implemented, and **the six profiles are selectable configurations
rather than definitions** — Required Action 4.

### The empty configuration is no longer valid, and that is a real change

With every backend behind a feature, selecting none leaves `ArchiveBackend` with
**no variants at all**. A `match` on a reference to an empty enum is not
accepted, so the failure mode was a wall of `non-exhaustive patterns` errors
pointing at code that was not the problem.

`lib.rs` now carries a `compile_error!` that names the fix. This is not a
regression: AD-0058's profile table has always said the floor is `read-minimal`
= `read` + `zip-read`, never the empty set, and that is the profile the 837 KB
footprint number was measured against. The consequence for verification is
concrete — **`cargo test --no-default-features` is no longer a valid lane** and
is replaced in `scripts/release-gate.sh` by the six profile lanes. Every
per-feature isolation lane also gains `read,zip-read`, since a format feature
alone selects no reader.

### `NamespaceTracker` had to move, and the first attempt to avoid moving it was wrong

`NamespaceTracker` is the two-set file/directory conflict gate used by **both**
write paths — Modify's `commit_changes` and Write's per-add — and it lived in
`modification.rs`. Gating that module on `modify` alone broke `create`; the first
fix was to widen the gate to `any(create, modify)`, which meant a create-only
build compiled all 2,726 lines of the modification module to reach 30 lines of
shared logic.

That was the wrong call and the compiler said so: the cascade of
`no field modifications` errors that followed was the module being half-selected.
It is now `src/write_namespace.rs` (161 lines, gated on `any(create, modify)`),
`modification` is cleanly `modify`-only, and the three helpers it depends on
(`record_file`, `record_dir`, `normalize_dup_check_path`, `ancestor_paths`) moved
with it. Worth recording because the instinct to avoid a refactor mid-increment
produced more work than the refactor did.

### The `create` profile cannot be a test lane as this record defines it

`create` = `create, zip-write, sevenzip, libarchive` — **no reader**. Running the
suite there produced **236 failures**, every one of them "wrote the archive,
cannot read it back". That is the profile behaving exactly as specified: it is a
legitimate consumer configuration (a program that only writes archives) and an
impossible test configuration, because every creation test verifies its work by
reading.

Resolved by splitting rather than by redefining the profile or by tolerating 236
red tests that describe correct behaviour:

* `check-profile-create-pure` — compile-only, the profile exactly as this record
  defines it. This is the lane that catches the feature being wired to nothing.
* `profile-create` — the test lane, with `read,zip-read` added, which is the only
  configuration in which "what `create` writes is correct" is an assertable
  claim.

The general point, since the operation features make it recurrent: **a profile
that cannot observe its own effects needs a compile lane and a separate,
larger test configuration.** Conflating them silently changes what the record
promises.

### `integrity` was declared and wired to nothing

Caught in this increment, in this increment's own work: `integrity` sat in the
feature table with **no `#[cfg]` anywhere referencing it** — precisely the
"feature wired to nothing" defect the isolation lanes exist to detect, and it
would have passed every lane, because a feature that gates nothing never breaks
a build. It now gates `validate_integrity`, `calculate_archive_crc`,
`calculate_manifest_digest`, `calculate_content_multiset_digest_and_size`,
`has_recovery_record` and `recovery_percentage`.

It was found by reading the feature table against the source, not by a lane.
That is worth stating plainly: **the lanes catch a feature that is wired
incorrectly; nothing but inspection catches a feature that is wired to nothing at
all.**

### Two mistakes of my own, recorded because both are cheap to repeat

**A name grep cannot see a trait method.** I removed `use std::io::{Read, Write}`
from a test as unused — they are used through `read_to_end` and `write_all`,
which name the *method*, not the trait. Five profiles broke. Restored with
conditions matching the actual call sites.

**Two `cfg_attr` allows on one item are a clippy error, not a redundancy.**
Batch-adding conditional allows produced pairs like
`#[cfg_attr(not(A), allow(dead_code))] #[cfg_attr(not(B), allow(dead_code))]`,
which `clippy::duplicated_attributes` rejects. The correct merge is
`#[cfg_attr(not(all(A, B)), allow(dead_code))]` — allow unless *both* features
are present — and doing it by hand file-by-file was whack-a-mole until it was
done as one repo-wide transformation.

### The footprint claim, finally measured against the real floor

The whole record exists to substantiate one claim: that a read-only consumer
should not pay for a writer. It is now a number.

| Profile | stripped | × floor |
| --- | ---: | ---: |
| `read-minimal` | **577,784** | 1.00 |
| `read-zip` | 632,408 | 1.09 |
| `read-all-formats` | 1,638,536 | 2.84 |
| `full` | 1,897,112 | **3.28** |

**The operation features alone take 259 KB off the floor — 31 %** (837 KB before
this increment, 578 KB now), which is the largest single reduction of the entire
split, larger than any format feature. That is worth pausing on, because it
inverts the assumption the split started from: the first four increments all
gated *formats*, on the theory that backends are where the weight is, and the
Required Action 5 amendment already showed crate count ranks features roughly
backwards. The operation split says something stronger — **what the crate can
*do* costs more than what it can do it *to*.**

A read-only ZIP consumer now links **578 KB against 1,897 KB for everything**, a
3.28× spread, with no C toolchain and no system library. Both halves of that
sentence were assertions when this record was written.

### Lane results

| Profile | suites | passed | failed |
| --- | --- | --- | --- |
| `read-minimal` | 43 | 1137 | 0 |
| `read-zip` | 43 | 1338 | 0 |
| `read-all-formats` | 43 | 1720 | 0 |
| `create` (with a reader, see above) | 43 | 1487 | 0 |
| `modify` | 43 | 1611 | 0 |
| `full` | 43 | 2127 | 0 |

`clippy --all-targets -- -D warnings` is clean on all six plus `default`,
`--all-features`, `read+integrity`, and the reader-less `create` profile.

### Feature table as shipped

| Operation | Depends on |
| --- | --- |
| `read` | — |
| `integrity` | `read` |
| `create` | — |
| `modify` | `read`, `create`, `libarchive` (AD-0071 / decision 1) |
| `full` | everything except `external-rar-create` (decision 2) |

| Format | Depends on |
| --- | --- |
| `zip-read`, `zip-write` | — |
| `zip-crypto` | `zip-read` |
| `sfx` | `read` |
| `sevenzip`, `rar-support`, `libarchive` | — |

`sevenzip` reads 7z; libarchive writes it (2026-09-07 amendment).
`external-rar-create` implies `create`.

Stage 2 (facade crates) remains parked by the 2026-09-02 owner ruling; Required
Action 5's numbers did not make the case for it.
