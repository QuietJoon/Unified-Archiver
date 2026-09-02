---
type: Open Issues
title: "Open Issues"
description: "ACCEPT (explicit user request to track the plan in Open Issues)"
tags: [project-control, ADR-0058, ADR-0046, ADR-0065, ADR-0020, ADR-0009, ADR-0062, ADR-0053, ADR-0055, ADR-0064, ADR-0054, ADR-0063, ADR-0027]
timestamp: 2026-07-17T00:00:00Z
status: active
---

# Open Issues

Issues accepted from code reviews or architecture planning. Entries stay in this file after they
are resolved, so **file membership is not an open/closed signal** — the `- **Status:**` line
(`OPEN` / `PARTIALLY …` / `RESOLVED <date>`) is the authoritative one. Read it before assuming an
entry is outstanding.

[`open-issues-resolved.md`](open-issues-resolved.md) is a closed historical archive of 48 entries
(relocated 2026-08-04 from `reviews/Open_Issues_Resolved.md`); it takes no new entries, and
resolved entries are no longer moved there.
Straightforward test-hygiene fixes are removed outright without an archive entry.

**2026-08-21 reconciliation.** Not a scan — a pass over this register against the two commits of
2026-08-21, verifying each claim by reading the tree rather than the commit message or the closing
ticket comment. Ten entries changed status or gained a dated note. Three claims did **not** survive
the check and the entries stay open because of it: OI-0001-002's ticket is closed while the five
drift guards still compare names alone; OI-0076-006's shell-out was hardened without closing either
design hole it names; and OI-0001-010 landed for the libarchive writer only. Every verification box
ticked below carries an HTML comment saying what was read to tick it, and every box left unticked
carries a dated reason — a box whose criterion is only half-satisfied is left unticked with the
half named, because a tick that overstates is worse than an open box.

**2026-09-02 reconciliation follow-up.** Another pass against the tree, prompted by a backlog triage
that found ten of the 34 live `docs/backlog.md` entries describing work that is finished. The
2026-08-21 preamble above is left exactly as written — it was true when written — and this paragraph
records what has since become false in it, because falsifying a dated finding is worse than
appending to it. All three of the claims it says "did not survive the check" have now moved:
OI-0001-002's exposure is closed by binding a read handle to the archive **file's** identity rather
than by the per-entry fingerprint its Required Actions ask for — a deliberate substitution recorded
in `docs/records/DCR-014-read-handle-bound-to-archive-file-identity.md`, already reflected in that
entry on 2026-09-01; OI-0001-010's ZIP half landed, so both writers now emit directory metadata;
and OI-0076-006's two design holes are open by an owner deferral ruling rather than by oversight. A
fourth entry moved the same way without being named there: OI-0080-005's Required Action 4 is done.
OI-0080-006 and OI-0001-006 are resolved outright. Six entries below therefore gain a dated
`### Update (2026-09-02)` section and, where a criterion is genuinely satisfied, a newly ticked box
whose comment says what was read to tick it. Every 2026-08-21 comment that a tick now contradicts is
kept **beneath** the tick rather than replaced. Saying plainly what this pass found: this register
was the stale document this time, in four coordinated places, and all four were accurate on the day
they were written. Leaving them is how the next reconciliation re-derives conclusions the tree has
already refuted.

---

## OI-0058-001: Feature-first footprint split and facade crates

- **Source:** AD 0058
- **Date:** 2026-04-25
- **Decision:** ACCEPT (explicit user request to track the plan in Open Issues)
- **Status:** OPEN

### Problem

The current crate couples operation families (read, integrity, create, modify)
with format/backend support (ZIP, 7z, RAR, libarchive, SFX) in one package
surface. Users who only need read/extract APIs can still pay for writer,
modifier, native build, and backend dependencies they do not use.

AD 0058 defines the plan: first split by functional and format features inside
the existing crate, then introduce read-only and full facade crates after the
feature matrix is proven.

### Impact

Small-footprint read-only consumers do not have a clean dependency profile yet.
Build scripts and optional native dependencies also remain harder to reason
about because libarchive and RAR behavior are not fully isolated by feature.

### Required Actions

1. Implement Stage 1 functional features: `read`, `integrity`, `create`,
   `modify`, and `full`.
2. Implement Stage 1 format/backend features: `zip-read`, `zip-write`,
   `zip-crypto`, `sevenzip`, `rar`, `libarchive`, and `sfx`.
3. Make backend dependencies optional and gate `build.rs` native probes/builds
   by the corresponding feature.
4. Add CI/test profiles for `read-minimal`, `read-zip`, `read-all-formats`,
   `create`, `modify`, and `full`.
5. Measure dependency tree, build time, and binary size for read-minimal and
   full profiles before changing defaults.
6. After Stage 1 stabilizes, introduce `unified-archive-core`,
   `unified-archive-read`, and the full `unified-archive` facade.

### Verification

- [ ] Read-minimal build works without libarchive, RAR, create, or modify
      dependencies.
- [ ] Full build preserves the current API surface.
- [ ] `build.rs` does not probe or build disabled native backends.
- [ ] CI covers both read-only and full profiles.
- [ ] Facade-crate migration notes are documented before publishing the split.

### Related

- AD 0058 (Feature-first footprint split and facade crates)
- `Cargo.toml`, `build.rs`, `src/lib.rs`

---

## OI-0065-001: Windows libarchive discovery (vcpkg path)

- **Source:** R0065-0004, R0065-0013, R0065-0020 (Review 0065)
- **Date:** 2026-04-22
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 DEFER)
- **Status:** OPEN (narrowed 2026-08-04 — the UnRAR half of Required Action 2 landed with Innovation I4; see the update note)

### Update (2026-08-04)

Innovation I4 (2026-07-22) overtook the UnRAR half of this issue and part of the Problem
statement below, which is retained as written for history:

- `build.rs` no longer "unconditionally invokes `make lib`" — the vendored UnRAR SDK is compiled
  by the `cc` crate with sources and defines selected from `CARGO_CFG_TARGET_OS` /
  `CARGO_CFG_TARGET_ENV`, including an MSVC-compatible Windows source set. **Required Action 2
  is satisfied** (no prebuilt `.lib`, CMake, or manual path needed).
- The Review-0065 "emit a clear `cargo:warning` and panic with a pointer to this OI" stopgap is
  gone with it (MADR-0020 amendment — MADR-0020 "Feature-gate UnRAR build behind `rar-support`" is
  the record cited as "AD 0020" in documents written before the 2026-07-23 renumber; today's
  `AD-0020` is the unrelated additive-modification-options record); building with `rar-support` on
  Windows now compiles the
  vendored sources natively. The panic-removal half of Required Action 3 is therefore done;
  its README half remains open together with Required Action 1.

Still open and unchanged: the libarchive discovery block in `build.rs` is a warning-only no-op
on Windows (no `rustc-link-search` / `rustc-link-lib` directives), so libarchive-backed formats
cannot link there — Required Actions 1, 3 (README half), and 4 (Windows CI job, which
OI-0081-005's runtime verification also waits on). The discovery-mechanism choice
(vcpkg autodetect vs explicit env vars vs vendoring) is an owner decision tracked in
`docs/backlog.md` (Type 2).

### Update (2026-08-06)

The stale-prerequisite part of the README half of Required Action 3 is done: README, the user
manual, and the getting-started guide no longer require `make`, and README's Windows paragraph
now names libarchive discovery (not `make`) as the blocker and keeps `rar-support` enabled. The
remainder of Required Action 3 — documenting the *chosen* discovery mechanism — still waits on the
owner decision above.

### Problem

`build.rs` does not actually configure libarchive on Windows — it only prints
a `cargo:warning=... future implementation` line with no `rustc-link-search`
or `rustc-link-lib` directives. At the same time, the default feature set
includes `rar-support`, which unconditionally invokes `make lib` on the
vendored UnRAR sources. Stock Windows environments have neither.

Review 0065 Cluster C applied the minimum immediate fix per AD 0046's intent
of not downgrading the Windows support story:

- `build.rs`: when building with `rar-support` on Windows, emit a clear
  `cargo:warning` and panic with a pointer to this OI instead of running
  `make`. Users are told to build with `--no-default-features` until vcpkg
  discovery lands.
- `README.md`: Windows instructions no longer claim "Dependencies bundled
  automatically"; they now describe the real prerequisites and pointers.

### Impact

Without proper Windows libarchive discovery, all libarchive-backed formats
(TAR family, ISO) and the ZIP source-read side of `commit_changes()` fail to
link on Windows. End users see an opaque linker error rather than a clean
diagnostic.

### Update (2026-09-03) — the blocker is a missing Windows HOST, not a missing CI service

AD-0070 defines CI-coverage as recorded per-platform verification: a committed
recipe, run on a host that can observe the property, native exit status
recorded last, bound to a fingerprint. Applied here, this issue splits into
three sub-properties with different blockers, and only one of them was ever
blocked on anything resembling CI:

- **Choosing the discovery mechanism** (vcpkg autodetect / explicit env vars /
  vendoring) is a design decision. Host-independent, and owed by nobody but the
  owner. `build.rs` already reads `LIBARCHIVE_LIB_DIR`, so explicit env vars are
  the smallest step from what ships.
- **The `cfg(windows)` arms compiling** is checkable on the dev host today via
  `cargo check --target x86_64-pc-windows-msvc --no-default-features --features
  external-rar-create`, which skips the vendored UnRAR C++ build. It is lane L6
  of `scripts/release-gate.sh`, **defaults off**, and is owner-invoked only
  under the 2026-09-01 hold. An agent must not enable it.
- **libarchive actually linking, and the Windows tests actually running**, is
  the genuine block. No script on a macOS host can observe it.

So the recorded blocker changes from "this repository has no CI configuration"
to "this project has no Windows host to run on". The owner's own Windows
machine is the intended host and the run is pending as of this update. That is
a smaller and more honest statement: the first phrasing implied that adopting a
CI service would unblock the issue, and under AD-0070 no service will be
adopted.

### Required Actions

1. Implement vcpkg-based libarchive discovery in `build.rs` — either:
   - detect an installed vcpkg root via `VCPKG_ROOT` / `VCPKG_INSTALLATION_ROOT`
     and emit the right `rustc-link-search` / `rustc-link-lib` lines; or
   - accept explicit env vars (`LIBARCHIVE_LIB_DIR`, `LIBARCHIVE_INCLUDE_DIR`)
     and fail fast with a precise diagnostic when neither is provided.
2. Provide a Windows-compatible UnRAR build path (prebuilt `.lib`,
   CMake-based build, or an explicitly-documented manual path).
3. Update the README / docs to describe the chosen mechanism and remove the
   panic-with-OI-pointer fallback.
4. Run the release gate on a Windows host and record it under
   `docs/verification/`: `cargo build` with default features, `cargo build
   --no-default-features`, and `cargo test --all-features -- --test-threads=4`.
   **Restated 2026-09-03 (AD-0070).** This used to read "Add a Windows CI job
   that compiles the crate with default features", which named a mechanism the
   project has ruled out rather than the property it needs.

### Verification

- [ ] Windows build succeeds with default features.
- [ ] Windows build succeeds with `--no-default-features`.
- [ ] RAR extraction works on Windows (or `rar-support` remains explicitly
      gated with a documented path to enable it).

### Related

- AD 0046 (Windows support messaging — reject downgrade)
- `build.rs`, `README.md`

---

## OI-0065-002: `created` / `accessed` timestamps dropped during modify rewrite

- **Source:** R0065-0009, R0065-0010 (Review 0065)
- **Date:** 2026-04-22
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 DEFER)
- **Status:** RESOLVED 2026-04-30 — the reader closes the listing-side gap: `Archive::open(...).list_files()` surfaces `accessed` / `created` from the 0x5455 extended-timestamp extra field on unencrypted ZIPs. (Mechanism note, 2026-08-06: the fix originally shipped as a hybrid `zip`-metadata + `piz`-mmap reader; DCR-009 removed `piz`, and the extended-timestamp parsing now lives in the sole `zip`-crate backend — `src/ffi/zip_wrapper.rs`. The resolution itself is unaffected.)

### Problem

Listing code populates `ArchiveEntry.created` and `ArchiveEntry.accessed`
from the source archive, but both metadata-aware write helpers in
`src/ffi/libarchive_wrapper.rs` and `src/ffi/zip_writer.rs` only propagate
`metadata.modified` plus permissions. A `commit_changes()` round-trip with
`preserve_metadata=true` therefore silently drops `created` and `accessed`.

Review 0065 Cluster D resolved the documentation half of the problem by
narrowing every public-facing reference to "modification time and Unix
permissions" (README, USER_MANUAL, API_REFERENCE, architecture docs,
scenario matrix, verification matrix, stub-manifest, status, baseline,
impact report, rustdoc for `modify_with_options` / `ModificationOptions`,
and the `tests/modification_options_test.rs` header comment). This OI tracks
the code-side implementation gap.

### Impact

Callers opting into `preserve_metadata=true` receive a narrower guarantee
than a naive read of earlier docs suggested. The archive's `modified` time
still survives; `created` and `accessed` regress to whatever the backend
defaults to.

### Required Actions

1. Thread `entry.created` and `entry.accessed` through
   `ZipWriter::add_file_from_reader_with_metadata` and
   `LibarchiveArchive::add_file_from_reader_with_metadata` /
   `add_file_from_data_with_metadata` on every backend that supports them
   (ZIP extra fields, libarchive `archive_entry_set_atime` /
   `archive_entry_set_birthtime`). **Done 2026-04-28.** ZIP writer now
   attaches a 0x5455 ("Universal Time") extra-field block carrying
   mtime/atime/ctime; libarchive writer calls
   `archive_entry_set_atime` / `archive_entry_set_birthtime`. Source-side
   listing reads ZIP 0x5455 ctime via libarchive's `archive_entry_ctime`
   fallback so the round-trip closes through libarchive.
2. Add a metadata round-trip test. **Done 2026-04-28** —
   `tests/modification_options_test.rs::modify_zip_preserves_atime_and_btime_through_commit`
   verifies the 0x5455 block survives commit_changes via the `zip` crate's
   wire-level reader.
3. Once landed, widen the public wording back from "modification time and
   Unix permissions" to the full field list. **Done 2026-04-30** — both
   the writer side and the default-reader listing surface
   `accessed` / `created`. `ModificationOptions::preserve_metadata`
   rustdoc and the public docs can drop the Piz-reader caveat.

### Resolution (2026-04-30) — hybrid metadata reader

User route during Phase 2 discussion of this OI rejected path (a) and
asked for a **hybrid** option: keep `piz` for mmap-backed extraction
(its main reason to exist) but use the `zip` crate purely for its
`extra_data_fields()` accessor on the listing path. Implementation
landed in `src/ffi/piz_wrapper.rs::read_zip_extended_timestamps`:

- `PizArchive::list_from_mapping` walks the central directory through
  piz exactly as before, then runs a second, metadata-only pass through
  `zip::ZipArchive<Cursor<&[u8]>>` over the same mmap bytes.
- For each entry index, 0x5455 timestamps (`mod_time` /
  `ac_time` / `cr_time`) are merged into the piz-derived
  `ArchiveEntry`. piz still owns extraction; `zip` is metadata-only on
  this path.
- The merge is best-effort — a `zip` parse failure leaves the
  piz-derived listing untouched. piz already errors out on genuinely
  broken archives at its own parse step, so soft-failing here cannot
  let corruption slip through.
- AD 0065 (backend caching baseline — Option A) memoises this richer
  listing in `cached_listing`, so the second-pass cost is paid once
  per `Archive`.

### Verification

- [x] `created` round-trip covered by a wire-level test
      (`modify_zip_preserves_atime_and_btime_through_commit`).
- [x] `accessed` round-trip covered by the same wire-level test.
- [x] `preserve_metadata` rustdoc widened.
- [x] Hybrid reader test at
      `tests/integration/zip_extended_timestamps.rs` confirms
      `Archive::open(...).list_files()` surfaces `accessed` and
      `created` from 0x5455 (Piz-default code path).

### Related

- AD 0020 (additive modification options extension)
- `src/modification.rs::ModificationOptions`
- `src/ffi/zip_writer.rs`, `src/ffi/libarchive_wrapper.rs`

---


## OI-0069-001: R0069 Group A — API-shaping & format-detection design tracker

- **Source:** R0069-0001, R0069-0010, R0069-0017, R0069-0052, R0069-0058, R0069-0059, R0069-0070, R0069-0071 (Review 0069 Group A)
- **Date:** 2026-04-25
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 Group A)
- **Status:** RESOLVED 2026-04-28 — all six A-group decisions plus R0069-0052 landed inline
- **Plan:** [`docs/records/AD-0062-r0069-group-a-v0.3-api-shaping.md`](../records/AD-0062-r0069-group-a-v0.3-api-shaping.md)
- **Implementation commits:**
  - A.4 (modify-side typed `EntrySource`) + R0069-0052 (create-side namespace gate): `227746b`
  - A.5 (extension-guided ISO detect) + A.6 (TAR.* open confirmation): `877971a`
  - A.2 (`extract_to_stream` bounded by default) + A.3 (`verify_crc32` typed error): `bf43ddc`
  - A.1 (visibility tightening: `pub(crate)` for behaviour modules, `#[doc(hidden)]` for `ffi`): `b4464cc`

### Problem

Eight findings in Review 0069 reshape API contracts or
format-detection behavior in ways that need a coordinated planning
round before landing:

- R0069-0001 — `pub mod ffi` exposes raw FFI internals; hiding them is
  a public-API break aligned with AD 0009.
- R0069-0010 — `extract_to_stream*` returns an unbounded reader by
  default; bounded-by-default is a contract change.
- R0069-0017 — `extract_to_memory_with_options` CRC contract varies
  by backend; the option needs explicit documentation or
  normalization.
- R0069-0052 — Write-mode internal namespace tracking is not
  performed; duplicate names in `add_*` go undetected until commit.
- R0069-0058 / R0069-0059 — `ModificationTracker.added: Vec<u8>`
  forces in-memory buffering; needs `add_entry_from_reader` /
  `add_file_from_path` on the modify surface.
- R0069-0070 — `detect_from_file` reads only 512 bytes, so ISO PVD
  detection (offset 32769) is structurally unreachable from a file
  path. Either widen the read or document the limitation.
- R0069-0071 — TAR.GZ/TAR.BZ2/TAR.XZ extension fallback can route a
  plain gzip with `.tar.gz` filename to the tar parser. Needs a
  verify-after-decompress step or a separate raw-stream variant.

### Impact

These findings affect public API shape, observable contracts, or
format-detection completeness. Landing them piecemeal without a
single planning round risks contradictory commits and partial
deprecation states.

### Required Actions

AD 0062 records the per-finding decisions. Implementation is staged:

1. **Additive items (land first, no caller migration):**
   - A.4 typed `EntrySource` enum + `add_entry_from_path` /
     `add_entry_from_reader` methods (R0069-0058 / R0069-0059).
   - A.5 extension-guided 33 KiB read for ISO-extension paths
     (R0069-0070).
   - A.6 backend-confirmed TAR.* error mapping (R0069-0071).
2. **Breaking items (land after additives):**
   - A.2 streaming method rename:
     `extract_to_stream` → bounded by default;
     `extract_to_stream_unbounded` → explicit unbounded variant
     (R0069-0010).
   - A.3 `verify_crc32` typed-error normalisation against TAR
     family (R0069-0017).
   - A.1 visibility tightening: `pub mod ffi`,
     `pub mod creation/extraction/inspection/modification` →
     `pub(crate)` (R0069-0001 / R0070-0080 / R0070-0092).
3. **Create-side namespace gate (R0069-0052):** apply the same
   directory-vs-file conflict detection landed for modify under
   OI-0069-002 R0069-0062 (commit `9ea5fdb`) to the create
   surface so `Archive::create` followed by duplicate `add_entry`
   calls fail with a consistent diagnostic before the writer
   backend's format-specific error.

Future evolution recorded in AD 0062: option γ (polymorphic
`add_entry<S: Into<EntrySource>>(...)`) is a non-breaking follow-up
once the typed enum stabilises.

### Review 0070 cross-references (2026-04-26)

- **R0070-0080**: `docs/API_REFERENCE.md` claims `pub mod ffi` is
  "exposed for crate-internal composition" while the modules are
  publicly accessible. Same root issue as R0069-0001 (visibility);
  collapses into Action 1 above.
- **R0070-0092**: `pub mod creation` / `extraction` / `inspection` /
  `modification` keep behaviour modules in the public surface, which
  complicates the future read-only / full facade split. Same root
  issue as the OI-0058-001 footprint plan; resolved alongside the
  facade-crate work, not as a separate task.

### Verification

- [x] Group A plan recorded in an ADR (per-finding outcomes documented).
- [x] Each landed sub-fix references this OI in its commit message.
- [x] All Group A items closed or explicitly rejected before v0.3
      release.
  <!-- Ticked 2026-08-12: reopen verified all three against the tree; they were satisfied when the Status flipped to RESOLVED and were simply never ticked. -->

### Related

- AD 0009 (Isolate Unsafe Behind Safe Wrappers)
- AD 0053 (Group D architectural pass design baseline)

---

## OI-0069-002: R0069 Group B — modify/extraction architecture cleanups

- **Source:** R0069-0003, R0069-0005, R0069-0006, R0069-0007, R0069-0057, R0069-0062, R0069-0063, R0069-0064, R0069-0065 (Review 0069 Group B residual)
- **Date:** 2026-04-25
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 Group B)
- **Status:** PARTIALLY LANDED (8/9 sub-items as of 2026-04-30; the ninth is tracked as ticgit
  `84b324d1`, open and blocked — see the note below)

> **Where the ninth sub-item lives (2026-08-21).** The residual is R0069-0063, the
> `ValidatedSource` construction order, and it has carried its own TicGit ticket since 2026-08-12:
> `84b324d1`, verified `open` / `blocked` on 2026-08-21. AD 0055 now carries a dated amendment
> saying the same thing, so a reader of either document does not file it twice. It stays blocked on
> the same sequencing this entry already records — after the AD 0053 D2 typed handles are the sole
> facade — not on any new obstacle.

### Problem

Nine findings in Review 0069 require non-local rework of extraction
or modification semantics:

- R0069-0003 — `dispatch_extract_core` backend-specific parameter
  ordering (overlaps with AD 0053 D1 sequencing). **Landed 2026-04-30** —
  `ExtractionPlan` carries every option in one struct; `ReadBackend`
  gained an `extract_all(plan)` trait method, so `dispatch_extract_core`
  is now a single trait call through `read_backend_view().extract_all`.
  Per-backend match ladder retired. (D1 prerequisite for the future
  unified `extract_with_plan` consolidation.)
- R0069-0005 — Overwrite/collision preflight filters to `is_file()`,
  letting directory or special entries skip the gate. **Landed 2026-04-28**
  (`check_overwrite_conflicts` now also walks directory entries and
  rejects directory-vs-existing-file conflicts pre-extract).
- R0069-0006 — SFX archives opened through an offset reuse the outer
  executable path as the compressed-size denominator; zip-bomb gates
  weaken accordingly. **Landed 2026-04-30** — `check_archive_ratio` /
  `check_extraction_safe_with_archive` /
  `check_single_entry_safe_with_archive` now take `compressed_size:
  u64` directly; callers pass `Archive::payload_size_for_ratio()`
  which returns the staged payload tempfile size for SFX archives,
  the on-disk file size otherwise.
- R0069-0007 — `Archive::extract_stub(path, detection)` trusts a
  caller-supplied detection without verifying its origin. **Landed
  2026-04-28** — `extract_stub` now re-runs `detect_sfx` internally
  and rejects mismatched / forged offsets before reading.
- R0069-0057 — Encrypted-modify probe substring-matches error text;
  needs typed `ArchiveError::Password` propagation from libarchive.
  **Landed 2026-04-28 / fully retired 2026-04-29** —
  `classify_libarchive_error` at the FFI boundary upgrades
  encryption-shaped strings to `ArchiveError::Password`; modify-side
  `looks_like_encryption` substring fallback was removed in Review
  0075's typed-Password retrofit pass.
- R0069-0062 — Modify duplicate detection misses directory/file
  relationship conflicts (`a` vs `a/b`). **Landed 2026-04-28** —
  `commit_changes`'s namespace gate now tracks file-paths and
  directory-paths separately and rejects when they intersect.
- R0069-0063 — `ValidatedSource` is constructed *before* the fresh
  listing, so the type-level invariant the comment claims is not
  actually enforced. **Partially mitigated 2026-04-30** — D2
  `ReadArchive` now exists (v2-api) and structurally separates read
  from write, eliminating the "ValidatedSource from a write handle"
  loophole on the typed surface. Migrating the two internal callers
  (`commit_changes`, `entry_crc32_for_digest`) to construct
  `ValidatedSource` from `&ReadArchive` is queued for v0.4 — both
  callers currently take `&Archive` and a clean migration depends on
  threading `ReadArchive` through `ModifyArchive`'s commit path and
  the manifest-digest helper.
- R0069-0064 — ZIP source extras are keyed by raw zip-crate index
  without cross-checking name/CRC/size against the facade's listing.
  **Landed 2026-04-30** — `commit_changes` now runs
  `cross_check_source_listing` before trusting the per-index
  compression-method lookup; drift surfaces as a typed `Format` error
  with the diverging index/path/CRC/size from each reader. Closes
  R0075-0034 in the same code path.
- R0069-0065 — Retained entries with unknown streamable size fall
  back to `extract_to_memory` during commit; one large entry can blow
  RAM. **Landed 2026-04-28** — `stage_unknown_size_entry` drains the
  source stream into a tempfile so RAM stays bounded by the streaming
  chunk size; the staged file is then handed to libarchive's
  `add_file_from_reader_with_metadata`.

### Impact

Each individually has a narrow blast radius, but they share dependencies
on the post-D2 `ReadArchive` shape and on the libarchive error-mapping
work tracked elsewhere. Landing them ad-hoc would re-touch the same
surfaces multiple times.

### Required Actions

1. Sequence after AD 0053 D2 (`Archive` god-object split) so
   `ValidatedSource` (R0069-0063) can be constructed from the same
   token as the listing.
2. Add a libarchive error-classification helper that surfaces
   `ArchiveError::Password` for header-encrypted opens; remove the
   substring matcher in `Archive::modify` (R0069-0057).
3. Per-finding ADR (or grouped ADR) with concrete fixes referencing
   this OI before each lands.

### Verification

- [ ] Each item has either a landed fix or an explicit reject ADR.
- [ ] No new substring-based encryption detection introduced.
- [ ] R0069-0006 SFX safety check fixed before any future SFX-related
      release.

### Review 0070 cross-references (2026-04-26)

- **R0070-0008**: `ValidatedSource` is obtainable from any `&Archive`
  and does not actually carry the listing it claims to validate. Same
  root issue as R0069-0063; resolved together with the post-D2
  `ReadArchive` token shape.
- **R0070-0011**: SFX `open_at_offset` swaps `archive.path` back to
  the outer SFX file, so the extraction ratio check uses the whole
  executable size as the compressed-size denominator. Same root
  issue as R0069-0006; resolved together when the SFX-aware
  payload-size path lands.

### Related

- AD 0053 (Group D architectural pass)
- AD 0055 (D3 ValidatedSource token)
- OI-0058-001 (Feature-first footprint)

---

## OI-0069-003: R0069 R0069-0033 — RAR memory-extract path verification

- **Source:** R0069-0033 (Review 0069 Group C residual)
- **Date:** 2026-04-25
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 Group C)
- **Status:** RESOLVED 2026-04-28

### Problem

`UnrarArchive::extract_to_memory` extracts to a temp directory then
recomputes the expected output path with `sanitize_entry_path`. If
UnRAR normalises paths differently (case folding, separator
canonicalisation, edge-case escapes), the follow-up open can fail or
read an unexpected location.

### Resolution

`UnrarArchive::extract_file` and `extract_file_with_options` now
return the canonical written `PathBuf` and `extract_to_memory_with_limit`
opens that exact path instead of re-deriving it from the caller's
`file_path` spelling. The `ReadBackend` trait impl drops the path so
the public surface stays unchanged. Existing RAR unit and
`unrar_crc32_test` integration tests continue to pass; no
normalisation-divergent fixture has been observed yet but the
recompute-mismatch class is now structurally impossible.

### Verification

- [x] No silent `NotFound` path on the secondary read.
- [x] All existing RAR memory-extract tests still pass.

### Related

- `src/ffi/wrapper.rs::UnrarArchive::extract_to_memory`

---

## OI-0075-001: Non-UTF-8 path fidelity across backends

- **Source:** R0075-0007, R0075-0023, R0075-0055, R0075-0082 (Review 0075)
- **Date:** 2026-04-29
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 D-D — "preserve raw_path, keep as open issue")
- **Status:** RESOLVED 2026-04-29 (Option A landed for libarchive + UnRAR PathBuf migration; add_file_from_path rejects non-UTF-8 loudly; detect_multipart rustdoc-noted)
- **Resolution:** AD 0064 + commits 8a0bd4f, c4b6c28, 8782033, 48c702b in deferred-OI-closure plan Phase 1

### Problem

Several backends and helpers convert filesystem paths to `String`
via `to_string_lossy`, dropping or substituting bytes that don't form
valid UTF-8. The current state:

- `LibarchiveArchive` already preserves listing-side raw bytes via
  `ArchiveEntry.raw_path` (R0070-0025). Its own `path: String` field
  and the `extract_*` destination string still go through
  `to_string_lossy`, so an archive opened from a non-UTF-8 path can
  re-emit a different path string back to the caller.
- `UnrarArchive` stores `path: String` populated via
  `path.to_string_lossy()` (R0075-0055).
- `add_file_from_path` derives the default archive name via
  `file_name().to_string_lossy()` (R0075-0007).
- `detect_multipart` lossy-converts file names and stems
  (R0075-0082).

Review 0075 routed this bundle as a single design question: settle
the policy (reject loudly at the facade vs. preserve raw bytes
via `raw_path`-style fields throughout) and apply it consistently.

### Impact

A user who keeps a non-UTF-8 archive on a filesystem that allows
non-UTF-8 paths can:
- Receive an archive name from `Archive::path()` that does not match
  the path they passed in.
- Have `add_file_from_path` archive a file under a substituted name.
- Have `detect_multipart` mis-group volumes whose names contain
  non-UTF-8 bytes.

The current pattern is asymmetric: listing surfaces raw bytes but
the archive-level metadata does not.

### Required Actions

1. Pick the policy with an ADR:
   - Option A — preserve raw bytes via `raw_path`-style fields on
     every backend and on `Archive` itself; keep `to_string_lossy`
     copies for display only.
   - Option B — reject non-UTF-8 source / destination paths at the
     facade boundary with a typed `InvalidPath` error.
2. Apply the chosen policy to:
   - `LibarchiveArchive::path` (currently `String`; should be
     `PathBuf` if option A).
   - `UnrarArchive::path` (same shape).
   - `Archive::add_file_from_path` archive-name derivation.
   - `detect_multipart` and the multipart sibling enumeration.
   - Libarchive's `archive_entry_set_pathname` destination call
     (R0075-0023) — Windows libarchive ships with
     `archive_entry_copy_pathname_w` for wide-char paths; the Unix
     side accepts UTF-8 already.
3. Add platform-specific tests under `tests/integration/` that
   exercise non-UTF-8 paths on Unix (filenames containing
   `\x80`-style bytes) and on Windows (UTF-16 surrogate-half names
   when option A) — gated `#[cfg(unix)]` / `#[cfg(windows)]`.

### Verification

- [x] ADR records the chosen policy. (AD 0064)
- [x] LibarchiveArchive::path migrated to PathBuf with raw-byte
      CString conversion via path_to_cstring (Unix bytes preserved).
- [x] UnrarArchive::path migrated to PathBuf; libunrar's own
      open API does not accept non-UTF-8 byte paths on Unix
      (libunrar wire-format limitation), but the wrapper layer no
      longer pre-truncates at construction.
- [x] add_file_from_path rejects non-UTF-8 source filenames with
      a typed InvalidPath error pointing callers to
      add_file_from_path_as.
- [x] detect_multipart rustdoc-noted: the lossy substitution is
      consistent across source and siblings, so prefix matching
      works correctly for the typical multi-volume case. The edge
      case where two byte-divergent stems coalesce into the same
      lossy form is deferred to v0.4 alongside R0075-0083 (typed
      MultipartLayout return shape).
- [x] Platform-specific tests cover the non-UTF-8 round-trip
      (tests/integration/non_utf8_paths.rs, Unix-only). Windows
      wide-char path support deferred to OI-0065-001 follow-up.

### Related

- `src/archive.rs::Archive::path`
- `src/ffi/libarchive_wrapper.rs::LibarchiveArchive`
- `src/ffi/wrapper.rs::UnrarArchive`
- `src/creation.rs::Archive::add_file_from_path`
- `src/inspection.rs::detect_multipart`
- AD 0009 (Isolate Unsafe Behind Safe Wrappers)
- AD 0064 (Non-UTF-8 path policy — Option A)

---

## OI-0075-002: Snapshot semantics and `unsafe Send` audit

- **Source:** R0075-0001, R0075-0004 (Review 0075)
- **Date:** 2026-04-29
- **Decision:** ACCEPT (explicit user route into Open_Issues during Phase 2 D-A — "Tighten document, but keep as open issue")
- **Status:** RESOLVED 2026-04-30 — Option A landed (AD 0065). Libarchive backend memoises listing in a `OnceCell<Vec<ArchiveEntry>>`; Piz backend gains the same listing cache. Every read backend now agrees on "frozen at first observation." Regression test in `tests/integration/backend_caching_baseline.rs` modifies the archive on disk between two `list_files()` calls and asserts both return the cached snapshot.

### Problem

Review 0075 raised two coupled concerns:

- **R0075-0001** — `Archive`'s rustdoc claimed snapshot semantics
  the backends do not uniformly deliver. Piz/ZipReader/SevenZ now
  cache parsed metadata; libarchive re-reads on every operation;
  UnRAR keeps its FFI handle alive. The rustdoc was rewritten in
  Review 0075 to call out per-backend caching behaviour explicitly
  (`docs see "Snapshot semantics — best-effort metadata cache"`),
  but the runtime caching gap remains.
- **R0075-0004** — the `unsafe impl Send for Archive` SAFETY
  comment had drifted (Piz now holds an `OnceCell<Mmap>`, ZipReader
  holds a `Mutex<Option<RawZipArchive<File>>>`). The SAFETY comment
  was rewritten in this review and a compile-time `Send` assertion
  added (`const _: fn() = || { fn assert_send<T: Send>() {} … };`)
  so future drift fails at type-check time.

### Impact

For R0075-0001 the documentation-side fix lands the contract
explicitly; the latent risk is that callers reading the old wording
expected a frozen view of the archive on disk and may be surprised
when the libarchive backend observes external rewrites. Aligning the
backends behind a uniform caching policy (or formalising the lazy
re-read backends in their own type so the contract is structural)
is the work this OI tracks.

For R0075-0004 the SAFETY comment is now accurate and a static
assert anchors it; a future field addition that introduces a
non-`Send` type will fail compilation.

### Required Actions

1. Decide the caching baseline:
   - Option A — every read backend memoises listing/metadata once,
     freezing the view at first use.
   - Option B — accept lazy reopen as documented and treat
     "snapshot" wording as a best-effort metadata cache (current
     state).
2. If option A is chosen, extend the libarchive backend to cache the
   parsed metadata after the eager open-time validation.
3. Land regression tests covering observable cache divergence
   (modify the file on disk between two `list_files()` calls and
   assert backend behaviour matches the chosen policy).

### Verification

- [x] Rustdoc on `Archive` reflects the actual per-backend caching
      story (Review 0075).
- [x] Compile-time `Send` assertion in `archive.rs` (Review 0075).
- [x] Backend caching policy ADR (AD 0065 — Option A).
- [x] Per-backend regression tests
      (`tests/integration/backend_caching_baseline.rs` covers libarchive
      and Piz; ZipReader/SevenZ/UnRAR already satisfied the contract).

### Related

- AD 0054 (Piz / Zip handle caching)
- AD 0053 (Group D architectural pass)
- AD 0065 (Backend caching baseline — Option A)
- `src/archive.rs::Archive`
- `src/ffi/libarchive_wrapper.rs`, `src/ffi/piz_wrapper.rs`

---

## OI-0075-003: Drop / finish error semantics for write archives

- **Source:** R0075-0005 (Review 0075)
- **Date:** 2026-04-29
- **Decision:** ACCEPT (Phase 2 D-B — "Fix it. Can change API. Plan with AD-0053")
- **Status:** RESOLVED 2026-04-30 — AD-0053 D2 typed-handle split landed (`v2-api` feature). `WriteArchive::finish(self)` consumes the handle, making it the only durable commit path; `ModifyArchive::commit_changes(self)` does the same for modify mode. The legacy `Archive` sum type keeps the `finalized` / `write_poisoned` sticky flags for v0.3 source-compat — they will be retired together with the legacy facade in v0.4.

### Problem

`Archive::Drop` previously discarded the `Result` from
`finalize_write_backend`, hiding finalize errors from any caller who
forgot `finish()` / `close()`. Review 0075 routed this for inline
fix plus AD-0053 D2 sequencing.

### Resolution (this review)

Review 0075 adds two fields to `Archive`:

- `write_poisoned: bool` — set when a write/modify operation has
  failed in a way that leaves the backend in an undefined state.
  Subsequent `add_*` and `finish` calls return
  `OperationBlocked` so callers cannot keep stacking writes on a
  broken handle.
- `finalized: bool` — set when `finish()`/`close()` (Write) or
  `commit_changes()` (Modify) succeeds. The `Drop` impl checks this
  flag and emits a structured `eprintln!` when it sees a Write-mode
  handle dropping without finalize, and refuses to re-finalize a
  poisoned handle silently.

The contract recorded in `Archive::finish` rustdoc still expects
callers to invoke `finish()`/`close()` for durability; Drop is
explicitly best-effort.

### Remaining work

The user's Phase 2 routing said "plan with AD-0053 D2". The typed
`WriteArchive` split planned in AD-0053 D2 is the long-term home for
this contract — at that point `finish()` consuming `WriteArchive`
becomes the *only* durable commit path, and Drop on `WriteArchive`
can either panic (for unfinished writes) or fail loudly via the
poison logic added here. Finalising the Drop story is part of the
D2 implementation pass.

### Verification

- [x] `write_poisoned` blocks `add_*` and `finish` after a backend
      failure.
- [x] Successful `commit_changes()` flips `finalized = true` so Drop
      stays silent.
- [x] Drop emits a structured `eprintln!` when called without
      finalize.
- [x] AD-0053 D2 lands the typed `WriteArchive` and removes the
      sticky-flag indirection on the typed surface (legacy `Archive`
      keeps the flags for v0.3 source-compat).

### Related

- AD 0053 (Group D architectural pass — D2 split)
- `src/archive.rs::Archive`
- `src/modification.rs::Archive::commit_changes`

---

## OI-0075-004: API ergonomics & SFX/RAR internals — v0.4 backlog

- **Source:** R0075-0003, R0075-0031, R0075-0032, R0075-0034, R0075-0039, R0075-0061, R0075-0062, R0075-0071, R0075-0078, R0075-0079, R0075-0080, R0075-0081, R0075-0083, R0075-0084 (Review 0075)
- **Date:** 2026-04-29
- **Decision:** ACCEPT (Phase 2 D-H — "Fix them. Can change API, if it is ideal")
- **Status:** PARTIALLY LANDED — small inline fixes done in Review 0075 (R0075-0050 attributes, R0075-0060 RAR test skip, R0075-0069 shebang, R0075-0070 ZIP local header, R0075-0072 is_confirmed structural). Phase 3 (2026-04-30): R0075-0031, R0075-0032, R0075-0034, R0075-0079. Phase 4 (2026-04-30): R0075-0003 (SFX staging progress + cancel), R0075-0061 / R0075-0062 (streaming RAR5 vint parser), R0075-0071 (SfxDetectionResult fields → pub(crate) + accessors). Phase 5 (2026-04-30): R0075-0078 (ArchiveEntryBuilder), R0075-0080 (EntryFilter→FnMut), R0075-0083 (MultipartLayout typed return), R0075-0084 (perf sentinel — fix queued behind D1 backend trait). Phase 6 (2026-04-30): R0075-0081 (per-format CompressionOptions builders). Phase 7 (2026-04-30, AD 0053 D1): `ReadBackend::extract_all(plan)` trait method; per-backend dispatch ladder retired (closes R0069-0003 in OI-0069-002). Phase 8 (2026-04-30, AD 0053 D2): `ReadArchive` + `WriteArchive` + `ModifyArchive` typed handles behind the `v2-api` cargo feature; `ModifyArchive::try_commit_changes` (R0075-0039) closes the last R0075 actionable item via a hoisted `Archive::validate_pending_commit` helper. **All 14 R0075 OI items resolved.** Remaining R0075-0084 streaming-walk perf fix is queued behind the post-D2 streaming-traversal API on the read backend. **Update 2026-08-17:** R0075-0084 is resolved — the walk landed as `ReadBackend::visit_payloads_by_listing_id` under OI-0001-009 / TicGit `82bf8fd4`, not behind the post-D2 traversal API this sentence predicted; see the R0075-0084 row and its verification box below.

### Problem

Review 0075 raised a cluster of API-shape and internals findings that
each individually have a narrow blast radius but together amount to
a v0.4 API tightening pass. They are bundled into one OI so the user
can sequence them as a batch when the v0.4 cycle opens, rather than
threading 14 separate OIs through the planning queue.

### Items

| Issue | Topic | Notes |
|---|---|---|
| R0075-0003 | SFX staging progress / cancellation | **RESOLVED 2026-04-30** — `Archive::open_with_sfx_progress(path, Option<SfxStagingProgress>)` lets callers observe (or abort) the staging copy. Returning `false` from a `with_cancel` callback surfaces `ArchiveError::Cancelled { operation: "sfx_staging" }`. `Archive::open` keeps its current shape (threads `None`). |
| R0075-0031 | Compressed-tar enum extension | **RESOLVED 2026-04-30** — Six new variants added: `TarZst` / `TarLz4` / `TarLzma` (read/extract via libarchive; create deferred to v0.4 alongside the `archive_write_add_filter_zstd/lz4/lzma` bindings) and `Zst` / `Lz4` / `Lzma` (read-only, matching the existing Gzip/Bzip2/Xz policy). Magic-byte detection now recognizes Zst (0x28 B5 2F FD) and Lz4 (0x04 22 4D 18). |
| R0075-0032 | Zip modify reader-size verification | **RESOLVED 2026-04-30** — `ZipWriter::add_file_from_reader_with_size` wraps `Read::take(size+1)` and verifies post-read length; under- and over-production both surface as typed `Format` errors. `commit_changes` now routes `EntrySource::Reader { size: Some(n) }` through this path for ZipWriter targets. |
| R0075-0034 | ZIP per-entry compression by index | **RESOLVED 2026-04-30** — `commit_changes` runs `cross_check_source_listing` before trusting the per-index compression lookup; drift between facade listing and central-directory walker surfaces as a typed `Format` error with the diverging index/path/CRC/size. Closed alongside OI-0069-002 R0069-0064. |
| R0075-0039 | `try_commit_changes(&mut self)` | **RESOLVED 2026-04-30** — `ModifyArchive::try_commit_changes(&mut self)` (under `v2-api`) runs the same dup-path namespace check + ZIP source-extras cross-check that `commit_changes` performs at its pre-write gate. On rejection the handle is unchanged; callers can fix or clear operations and retry. The shared logic is hoisted into `Archive::validate_pending_commit`, so drift between the two paths is impossible by construction. |
| R0075-0061 | RAR5 recovery record fixed prefix | **RESOLVED 2026-04-30** — `parse_rar5_recovery` now uses streaming `read_rar5_vint(&mut Read)` instead of pre-loading 50 bytes; blocks with arbitrarily-long vint fields are walked correctly. The buffer-based `decode_vint` is retired. |
| R0075-0062 | RAR5 recovery percentage heuristic | **RESOLVED 2026-04-30** — Recovery percentage scan now starts after the verified `"RR"` service-name match and inspects only the next 32 bytes (down from 64 over arbitrary header content), narrowing the false-positive window. Full spec-based extraction stays heuristic — RAR5 doesn't fix the percentage byte's offset within the service header. |
| R0075-0071 | SFX result fields public + mutable | **RESOLVED 2026-04-30** — Fields demoted to `pub(crate)`; public read access goes through `is_sfx()`, `archive_format()`, `data_offset()`, `stub_type()`, `confidence()`. External callers can no longer mutate into a contradictory state; downstream code is decoupled from the field shape. |
| R0075-0078 | `ArchiveEntry` weak invariants | **PARTIALLY RESOLVED 2026-04-30** — `ArchiveEntryBuilder` + `ArchiveEntry::file` / `dir_at` / `try_file` constructors landed. `try_file` rejects empty paths; the builder's `permissions()` setter masks off non-Unix bits. `ArchiveEntry::new` / `directory` stay non-deprecated for v0.3 (40+ internal sites would otherwise emit migration noise); v0.4 deprecates them and demotes the public fields. |
| R0075-0079 | `permissions` semantics | **RESOLVED 2026-04-30** — `ArchiveEntry::permissions` rustdoc locks the contract: `permissions & !0o7777 == 0` always holds, regardless of source archive's host platform. `tests/integration/permissions_contract.rs` pins the contract across ZIP / TAR / 7z / RAR backends. Windows-specific attributes live in `attributes.windows`. |
| R0075-0080 | `EntryFilter` requires `Fn` | **RESOLVED 2026-04-30** — `EntryFilter` is now `Box<dyn FnMut(&ArchiveEntry) -> bool + Send>`. Stateful closures work without interior-mutability shims; `Fn` closures still satisfy `FnMut` so existing call sites compile. `extract_some` / `extract_filtered` generic bounds widened to `F: FnMut`. Helper `entry_filter_from_fn` lifts plain `Fn` closures into the alias. |
| R0075-0081 | `CompressionOptions` flat bag | **RESOLVED 2026-04-30** — `ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions` ship alongside `Archive::create_zip` / `create_seven_zip` / `create_libarchive`. Each builder surfaces only the fields the target format honours; `password` and `split_size` are absent from formats that reject them. The flat-bag `CompressionOptions` + `Archive::create` stay un-deprecated for v0.3 source-compat. |
| R0075-0083 | Multipart non-match singleton | **RESOLVED 2026-04-30** — `Archive::multipart_layout()` returns the typed `MultipartLayout::{Single { path }, Multi { parts }}`. Callers can no longer ignore a boolean and misinterpret a single-part archive as a one-volume set. `detect_multipart` stays for v0.3 source-compat; v0.4 will deprecate it. |
| R0075-0084 | Manifest digest streaming | **PARTIALLY RESOLVED 2026-04-30** — Issue acknowledged: a `#[ignore]`d perf sentinel (`tests/integration/manifest_digest_perf.rs`) builds a 1k-entry tar.gz and asserts `<= 50× baseline` ratio. The check currently fails at ~191× because libarchive reopens the archive per entry on CRC-less formats; running `cargo test -- --ignored` surfaces the limitation. Proper fix needs the streaming-walk API on the read backend and is queued behind AD 0053 D1 (backend trait abstraction). **RESOLVED 2026-08-17 (OI-0001-009 / TicGit `82bf8fd4`)** — the streaming-walk API landed as `ReadBackend::visit_payloads_by_listing_id` with a libarchive override that services every target from one handle, so the per-entry re-open is gone. The sentinel is no longer `#[ignore]`d, runs in the default lane, and passes at a measured ~2.4–2.6× against its 50× bound (was ~191×). The 2026-04-30 verdict above is kept as written; this row is its closure. |

### Verification

- [x] Per-item ADRs (or one umbrella ADR) record the chosen direction
      — closure recorded in AD 0063 (`docs/records/AD-0063-r0075-closure-and-design-positions.md`).
- [x] Each landed sub-fix references this OI in its commit message —
      RESOLVED rows above name the matching R0075 issue ID.
- [x] **R0075-0084 (Manifest digest streaming) resolved.** All other
      Group D-H items were already closed.
      <!-- Ticked 2026-08-17: the streaming-walk API this box was waiting on landed as
      `ReadBackend::visit_payloads_by_listing_id` (src/backend.rs) with the libarchive override in
      src/ffi/libarchive_wrapper/reader.rs servicing every target from a single handle via
      `BorrowedEntryReader`, driven by `Archive::resolve_crc32_single_pass`. It did NOT wait for
      AD 0053 D1 as this box predicted; it landed under OI-0001-009 / TicGit `82bf8fd4`. Verified by
      running the sentinel in the DEFAULT lane (no `--ignored`):
      `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test integration_tests --
      manifest_digest_perf --test-threads=4 --nocapture` exits 0, 1 passed, printing a ~2.4× ratio
      against the 50× bound where the pre-fix behaviour measured ~191×. Also see the R0075-0084 row
      above, whose 2026-04-30 verdict is left intact with its closure appended. -->


### Related

- MADR-0027 (Encrypted archive creation reject)
- AD 0053 (Group D architectural pass — D2 split)
- AD 0058 (Feature-first footprint split)
- AD 0062 (R0069 Group A v0.3 API shaping)
- DEF-002 (Split archive creation deferral)

---

## OI-0065-003: Two-layer caching memory tradeoff (AD 0065)

- **Source:** Simplify-pass review against AD 0065 implementation (Code Quality Agent, Efficiency Agent — 2026-05-01)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (explicit user route into Open Issues — "note it as open issue")
- **Status:** RESOLVED 2026-07-06 — `Arc<Vec<ArchiveEntry>>` landed end to end:
  `ReadBackend::list_files` returns the shared listing; all five backends
  cache `OnceCell<Arc<Vec<ArchiveEntry>>>` (the zip-crate backend gained a
  listing cache in the process — it previously had none, so this also closes
  its AD 0065 conformance gap); `Archive::entry_cache` holds the same `Arc`
  via the new `pub(crate) Archive::list_files_shared()`. The extraction
  preflight (`list_files_for_limits_with_mmap_cap`) now returns the shared
  `Arc` instead of deep-cloning per call; the public
  `list_files_for_limits()` keeps its owned-`Vec` return (one clone) so no
  public signature changed. The two `entry_cache.get().is_none()` assertions
  compile unchanged against the new cell type. AD 0065's "two-layer caching"
  cost note carries a dated amendment. Verified by the full
  `--all-features` suite including `tests/integration/backend_caching_baseline.rs`.

### Problem

AD 0065 mandates per-backend listing memoisation ("frozen at first
observation"). The facade `Archive::entry_cache: OnceCell<Vec<ArchiveEntry>>`
predates AD 0065 and serves the public `Archive::list_files() -> &[ArchiveEntry]`
contract. The new backend-level `cached_listing` cells (added on
`PizArchive` and `LibarchiveArchive` per AD 0065) serve the
`list_files_for_limits` bypass-the-facade callers.

When an extraction's safety preflight (which runs through
`list_files_for_limits_with_mmap_cap` → backend cache) is followed by a
user-visible `Archive::list_files()` call (which populates the facade
cache from a clone of the backend's Vec), **both caches hold a
`Vec<ArchiveEntry>` for the lifetime of the `Archive` handle**. The
listing's bytes are pinned twice.

### Impact

Bounded constant-factor memory cost. For typical archives (<10K
entries) the listing footprint is small relative to payload bytes the
user is processing alongside, so the doubling is negligible. For
million-entry archives it can amount to tens of MB of redundant
storage per `Archive` handle.

The agents' surveyed concern (Code Quality #3, Efficiency #2/#3) was
that the backend cache is redundant for callers that go through the
facade, since the facade already memoises. The bypass path is where
the backend cache earns its keep, but that's a narrower set of callers
than originally implied by AD 0065.

### Required Actions

The clean architectural fix is to share storage via `Arc<Vec<ArchiveEntry>>`:

1. Change `ReadBackend::list_files` return shape to
   `Result<Arc<Vec<ArchiveEntry>>>`.
2. Update all five backend impls (Piz, ZipReader, SevenZ, Libarchive,
   UnRAR) to cache `OnceCell<Arc<Vec<ArchiveEntry>>>` and return
   `Arc::clone`.
3. Change `Archive::entry_cache` to the same `Arc` shape so both cells
   reference the same underlying `Vec`.
4. `Archive::list_files()` returns `arc.as_slice()`; `list_files_for_limits()`
   clones the inner `Vec` for owned-Vec callers.
5. Update the two unit tests that assert `entry_cache.get().is_none()`.

This is mechanical but cross-cutting. A simpler alternative — drop the
backend cache and have `list_files_for_limits` re-parse on each call —
violates the AD 0065 contract for the bypass path.

### Verification

- [x] `Arc<Vec<ArchiveEntry>>` plumbed through `ReadBackend::list_files`
      and all five backend impls.
- [x] `Archive::entry_cache` shares storage with the backend cache.
- [x] Existing AD 0065 regression tests
      (`tests/integration/backend_caching_baseline.rs`) still pass.
- [x] Memory profile of the typical extract-then-list flow shows a
      single `Vec<ArchiveEntry>` instead of two.
  <!-- Ticked 2026-08-12: reopen verified all four against the tree. Two wording defects noted and left for the OKF pass: the Status says "all five backend impls" where DCR-009 leaves four, and it cites `list_files_for_limits_with_mmap_cap`, which OI-0080-003 renamed to `list_files_for_limits_budgeted`. -->

### Sequencing

Recommend executing this alongside AD 0053 D2's typed-handle default-surface
migration (currently behind the `v2-api` feature gate). When `ReadArchive` /
`WriteArchive` / `ModifyArchive` become the canonical handles, the listing
cache plumbing can be redesigned in one pass instead of touching the legacy
`Archive` facade and the typed handles separately.

### Related

- AD 0065 (Backend caching baseline — Option A; "Bad / costs" section
  explicitly notes this tradeoff)
- AD 0053 (Group D architectural pass — D2 typed-handle split)
- OI-0075-002 (the OI that AD 0065 closed; this entry tracks
  follow-up tightening)
- `src/ffi/piz_wrapper.rs::PizArchive::cached_listing`
- `src/ffi/libarchive_wrapper.rs::LibarchiveArchive::cached_listing`
- `src/archive.rs::Archive::entry_cache`
- `src/backend.rs::ReadBackend::list_files`

---

## OI-0076-001: Non-UTF-8 path fidelity round 2 (write + extract paths)

- **Source:** R0076-0019, 0020, 0021, 0039, 0040, 0041, 0042, 0066, 0067, 0083, 0092, 0094 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, batch route for the lossy-path family that AD 0064 left in the write/extract paths)
- **Status:** OPEN

### Problem

OI-0075-001 (RESOLVED 2026-04-29) closed the listing-side and `add_file_from_path`
lossy sites under AD 0064 Option A. Review 0076 finds 12 lossy `to_string_lossy`
sites that the prior pass did not reach:

- `src/ffi/zip_writer.rs::add_directory_recursive` — recursive ZIP create
  (R0076-0019).
- `src/creation.rs::add_directory_recursive` namespace prewalk — duplicate
  lossy generator (R0076-0020).
- `src/ffi/libarchive_wrapper.rs` write constructor stores `path_str`
  built via `to_string_lossy` (R0076-0021); extract-all destinations
  (R0076-0039); single-file extract destination (R0076-0040).
- `raw_format_name` for raw `.gz`/`.xz` pseudo-entries (R0076-0041).
- `is_compound_tar_extension` lowercases via `to_string_lossy`
  (R0076-0042).
- UnRAR directory + atomic-tempfile path conversions (R0076-0066,
  R0076-0067).
- AD 0064 left a public-API gap: there is no extract-by-raw-bytes
  surface, so callers cannot extract a non-UTF-8 entry by exact name
  (R0076-0083).
- `detect_multipart` matching still lossy (R0076-0092); format
  extension helpers (`extension_in`, `format_from_extension`,
  `promote_to_compound_tar`) still lossy (R0076-0094).

### Impact

A non-UTF-8 archive name on disk, or a non-UTF-8 destination directory,
can be silently substituted with replacement characters before the bytes
reach libarchive / ZIP writers / UnRAR. The asymmetry from OI-0075-001
("listing preserves raw bytes; the rest does not") is partially closed
but persists across these 12 sites.

### Required Actions

1. Extract one shared archive-path derivation helper used by both the
   creation namespace prewalk and per-backend recursive emission
   (R0076-0019, R0076-0020).
2. Migrate libarchive write `path_str` to `PathBuf` and use the
   existing `path_to_cstring` for libarchive handoff
   (R0076-0021, R0076-0039, R0076-0040).
3. Decide raw-byte support per backend for raw pseudo-entries
   (R0076-0041) and compound tar extension detection (R0076-0042).
4. Migrate UnRAR destination + atomic tempfile paths to raw bytes on
   Unix and wide on Windows (R0076-0066, R0076-0067).
5. Add raw-name or entry-id extraction surface (e.g.
   `extract_by_id(entry_id)` accepting `ArchiveEntry::id`); audit
   `mode_split.rs::ReadArchive` to expose it (R0076-0083).
6. Migrate `detect_multipart` matching to `OsStr` bytes
   (R0076-0092) and the format extension helpers to byte-level
   comparisons (R0076-0094).

### Verification

- [ ] One shared archive-path helper covers create/recursive-add/zip-writer paths
- [ ] libarchive write + UnRAR + ZIP writers use `path_to_cstring` / wide handoff
- [ ] `tests/integration/non_utf8_paths.rs` extends to write + extract paths
- [ ] Public extract-by-id (or extract-by-raw-bytes) API lands and is documented

### Related

- AD 0064 (Non-UTF-8 path policy — Option A; established `raw_path` field on `ArchiveEntry`)
- OI-0075-001 (RESOLVED — closed read-side and add_file_from_path)
- OI-0065-001 (Windows libarchive build — partial overlap with wide-path support)

---

## OI-0076-002: Backend single-entry defense parity (`ValidatedEntry` token)

- **Source:** R0076-0036, 0037, 0038, 0050, 0053, 0054, 0055, 0056, 0057, 0058, 0059, 0060, 0061, 0090 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, ValidatedEntry approach unifies the 14 individual fixes)
- **Status:** RESOLVED 2026-07-06 (TicGit `2e546a2a`) — `ValidatedEntry<'_>` token +
  `validate_single_entry` gate landed in `src/security.rs` (no public
  constructor; `check_single_entry_safe` = gate + limits, returns the token).
  Every backend single-entry method (piz/zip/7z/UnRAR/libarchive ×
  extract_file / extract_to_memory / extract_to_stream families) now
  validates FIRST against its own memoised listing, then **seeks by the
  token's stable listing id** instead of name-scanning — killing first-match
  duplicate resolution — with a listing-drift guard at the seek site (the
  AD 0065 snapshot can go stale if the file is rewritten; drift errors
  loudly). Output naming uses the validated normalized path (R0076-0050).
  In-method link/dir checks and directory-materialize branches were deleted;
  kind policy lives only in the gate.
  - **Design note vs. the original criteria:** the token flows *inside* the
    backend boundary rather than through method signatures — `pub mod ffi`
    exposes the backends publicly, so a `pub(crate)` token type cannot
    appear in their signatures (E0446), and the trait-object dispatch /
    ValidatedSource path-based callers would make signature-level tokens a
    D1-scale refactor. Policy centralization, the token, and "backends
    consume only validated resolutions" are achieved; the `&str` parameters
    remain as the resolution *input*, not the policy carrier.
  - **Known asymmetry (documented in the parity suite):** the zip-crate
    backend's central-directory map dedupes duplicate names upstream
    (R0079-0026 — later records replace earlier ones), so its own listing
    holds one record per name and duplicate rejection cannot trigger there;
    facade traffic for unencrypted ZIPs routes through Piz, which rejects.
  - **Behavior changes:** direct backend single-entry calls on directory
    entries now error instead of mkdir-and-Ok (the R0064-0013-era pin in
    `tests/integration/directory_entry_single_file.rs` was flipped
    accordingly); duplicate-path CRC-less digests
    (`calculate_content_multiset_digest_and_size` on e.g. `tar -rf` tars)
    now error at the gate instead of silently re-hashing the first
    occurrence — id-based digest streaming is tracked as TicGit `2a6e3153`.
  - **Tests:** new `tests/integration/single_entry_defense_parity.rs`
    (10 tests: directories, symlinks, hardlinks, duplicates, not-found
    shape, happy-path retention across piz/zip/7z/libarchive + RAR
    not-found) plus a libarchive stale-listing drift regression and the
    rewritten 7z duplicate-rejection unit test. Full `--all-features`
    suite green; clippy `-D warnings` clean.

### Problem

The facade gate `check_single_entry_safe` (`src/security.rs`) rejects
non-regular entries, duplicate paths, and ambiguous names for public
entry-point APIs. Each backend (`zip_wrapper`, `piz_wrapper`,
`sevenz_wrapper`, `libarchive_wrapper`) repeats parts of this gate or
omits it for direct backend methods. Review 0076 enumerates 14 sites
where direct backend calls produce inconsistent results: directories
treated as success, duplicate names resolved to first match, non-file
entries handed out as bytes/streams.

### Impact

A direct backend caller bypasses the facade-level safety contract:
- ZIP / Piz / SevenZ `extract_to_memory` can buffer non-regular entries.
- ZIP / Piz / SevenZ single-file extract create directories and return
  `Ok` instead of `OperationBlocked`.
- ZIP / Piz / SevenZ duplicate-name resolution is "first match wins"
  rather than "ambiguous → reject".
- libarchive single-file / memory / stream extract do not check
  `EntryType::File` before reading.

The facade-vs-backend asymmetry is the design problem; fixing each of
the 14 sites independently duplicates work that R0076-0090's
`ValidatedEntry`/resolved-token approach replaces.

### Required Actions

1. Define `ValidatedEntry` (or equivalent typed token) carrying the
   resolved entry index/path, the entry kind already validated, and a
   `pub(crate)` constructor only.
2. Migrate backend single-entry methods to accept `&ValidatedEntry`
   instead of `&str`.
3. Move single-entry safety policy into one place; backends consume
   only validated tokens.
4. Make direct backend single-entry methods inaccessible outside the
   validated-facade path (or document the new contract clearly).

### Verification

- [x] `ValidatedEntry` lands with `pub(crate)` constructors only
- [x] All 14 R0076 sites resolve through the shared gate and seek by the token's listing id  <!-- Reworded 2026-08-12: Required Action 2 ("accept `&ValidatedEntry` instead of `&str`") was deliberately not executed — see this entry's design note; the token is consumed inside the backend rather than carried in signatures. All 14 sites are migrated in that sense. -->
- [x] `tests/integration/single_entry_defense_parity.rs` (new) covers
      directories, symlinks, hardlinks, duplicates per-backend
  <!-- Ticked 2026-08-12: reopen verified the suite exists with 10 tests. Coverage gaps the file itself documents and this box does not claim: 7z duplicates are covered at unit level only, RAR covers not-found only, and hardlinks are tar-only by format limitation. -->
- [x] Every exported backend single-entry method routes through `validate_single_entry` before any archive or filesystem access (the `&str` signatures remain — `pub mod ffi` keeps them reachable — but they are gate-protected)  <!-- Reworded 2026-08-12: the original criterion was false in the tree and unachievable while `pub mod ffi` is public; it also contradicted this entry's own design note. -->

### Related

- AD 0053 (Group D architectural pass — D3 ValidatedSelection token established the precedent)
- R0076-0090 (the unifying recommendation)

---

## OI-0076-003: Security and durability boundary refactors

- **Source:** R0076-0005, 0014, 0017, 0035, 0045, 0089 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, six items each touch a security/durability boundary)
- **Status:** PARTIALLY LANDED (2/6 as of 2026-06-06) — the two self-contained, no-dependency durability/correctness items landed: **item 3 (R0076-0017)** — `ZipWriter::finish` now captures the `File` from `RawZipWriter::finish()`, `sync_data()`s it, and fsyncs the parent dir via `super::common::sync_parent_dir`, giving ZIP creation the same crash-consistency as extract/modify (idempotent with the `Drop`-finalize via `writer.take()`); **item 4 (R0076-0035)** — `extract_file_with_options` now stages each entry to a sibling tempfile (`build_staging_path`) and `rename_with_overwrite`s on success, with the staging file removed on every error path, so a `copy_data`/header/finish failure can no longer leave a partial file at the destination (mirrors extract-all, including the `!overwrite` pre-check and `sanitize_entry_path` traversal guard). Both adversarially verified (handle-cleanup / partial-file-guarantee / drop-idempotency — pass) and covered by the green `--all-features` suite (extraction, creation, integrity, zip/7z binaries). **Still OPEN:** items 1 (R0076-0005 parent-creation race / re-canonicalise — couples to AD 0066 v0.4 strict-path flag), 2 (R0076-0014 `O_NOFOLLOW` no-follow open — needs a Unix `libc` dep), 5 (R0076-0045 staging unlink→reopen-by-name race — needs owned-fd / libarchive callback design, MADR-0009), 6 (R0076-0089 `commit_changes` phase split — couples to AD 0053 D2 `ModifyArchive`).

### Problem

Six independent boundary-level concerns surfaced by Review 0076:

1. **R0076-0005** — `sanitize_entry_path` canonicalises only the
   deepest existing ancestor; a symlink can be inserted into a
   not-yet-existing parent path between sanitization and `create_dir_all`
   (parent-creation race).
2. **R0076-0014** — `open_file_no_follow_symlinks` exists at
   `src/ffi/common.rs` but the creation paths in `zip_writer` and
   `libarchive_wrapper` still call `reject_symlink_path` then
   `File::open`, leaving a TOCTOU window between metadata check and
   open.
3. **R0076-0017** — `ZipWriter::finish` does not call `sync_data`
   on the underlying file or `sync_parent_dir`, so ZIP creation has
   weaker durability than extraction/modify which both use
   `AtomicOutputFile::commit`.
4. **R0076-0035** — `LibarchiveArchive::extract_file_with_options`
   writes directly to the final destination; a failed copy leaves a
   partial file. Extract-all uses sibling staging + rename and is
   immune.
5. **R0076-0045** — Libarchive staging closes the tempfile then
   reopens by name. An attacker with destination-directory write
   access can race the path between unlink and libarchive open.
6. **R0076-0089** — `commit_changes` (~430 lines) bundles backup
   policy, option validation, temp archive creation, retained-entry
   replay, new-entry staging, writer finish, rename, backup cleanup,
   and poison/finalized state into one transaction. Failure-order
   regressions are likely.

### Impact

Each is a real boundary defect:
- (1)/(2) widen the symlink-attack window during extraction or
  creation.
- (3) breaks the durability contract callers expect from creation
  having parity with modify/extract.
- (4) leaves partial outputs on the public single-file API.
- (5) is a TOCTOU on extract staging.
- (6) makes `commit_changes` a single point of fragility for the
  modify pipeline.

### Required Actions

1. Plan an extraction-plan API that creates and verifies each
   directory component with no-follow / openat-style operations
   (R0076-0005); re-canonicalise after directory creation before
   opening output files.
2. Replace creation-time `reject_symlink_path + File::open` call
   sites with `open_file_no_follow_symlinks` (R0076-0014). Evolve
   the helper toward platform-native `O_NOFOLLOW` / reparse-point
   protection.
3. Capture the underlying `File` from `ZipWriter::finish`, call
   `sync_data` and `sync_parent_dir` before declaring durable
   commit (R0076-0017).
4. Reuse extract-all's staging + rename in
   `extract_file_with_options` (R0076-0035).
5. Replace libarchive staging path reopen-by-name with an owned
   file descriptor or libarchive callback hand-off (R0076-0045).
6. Split `commit_changes` into `ModificationCommitPlan`,
   `RewriteSession`, and `CommitSwap` phases with narrow invariants
   and per-phase tests (R0076-0089).

### Verification

- [ ] No-follow / openat-style extraction plan in place
  <!-- Still unticked 2026-08-21, but its stated blocker has dissolved: item 1's recorded coupling was "the AD 0066 v0.4 strict-path flag", and that flag shipped on 2026-08-21 — `ExtractionLimits::reject_unsafe_paths` is now enforced by `check_entry_paths_safe` in src/security.rs, which refuses the archive before a destination directory is created. The parent-creation race / re-canonicalise work (R0076-0005) is therefore unblocked and no longer waiting on a decision elsewhere. Nothing of item 1 itself landed. -->
- [ ] Creation paths use `open_file_no_follow_symlinks`
- [x] ZIP creation is durable (`sync_data` + parent fsync) — R0076-0017, 2026-06-06
- [x] `extract_file_with_options` uses staging + rename — R0076-0035, 2026-06-06
- [ ] Libarchive staging keeps owned fd
- [ ] `commit_changes` split into named phases; per-phase tests cover
      backup-noclobber, retained-replay, dup-path, poison

### Related

- AD 0053 (Group D architectural pass)
- AD 0059 (libarchive return-code sweep — partial; remainder tracked
  in OI-0076-007)
- AD 0066 (strict-reject flag, enforced 2026-08-21) — item 1's blocker; see the note on the first
  verification box. Item 6's blocker (the AD 0053 D2 `ModifyArchive` handle) is unchanged.

---

## OI-0076-004: v2-api typed handle parity gaps

- **Source:** R0076-0085, 0086, 0087, 0088 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, additive expansion of the v2-api typed handles)
- **Status:** RESOLVED 2026-06-06 — full mode-appropriate parity reached and the split-Drop contract unified. `ReadArchive` gained the complete inspection + extraction surface (`extract_file`/`extract_files`/`extract_by_ids`/`extract_some`/`extract_filtered`/`extract_to_memory[_with_options]`/`extract_to_stream[_with_options][_unbounded]`, `entry_count`/`find_entry`/`find_entries`/`list_files_for_limits`/`calculate_archive_crc`/`calculate_content_multiset_digest_and_size`/`calculate_manifest_summary`/`detect_multipart`/`check_symlinks`/`is_solid`/`has_recovery_record`/`recovery_percentage`/`extension_format`) plus `open_with_sfx_progress` and the static `extract_stub` (R0076-0085). `WriteArchive` gained `add_directory_recursive` + write-progress `entry_count` (R0076-0086). `ModifyArchive` gained `add_entry_from_reader`/`replace_entry`/`replace_entry_from_path`/`replace_entry_from_reader` plus source inspection (`list_files`/`entry_count`/`find_entry`/`find_entries`) so `remove_entry_by_id` is actually usable (R0076-0087). R0076-0088: `WriteArchive` is now the **single finalization owner** via the new `pub(crate) Archive::finalize_write_on_drop` — drop-without-finish runs one best-effort finalize (freeing the libarchive write handle that would otherwise leak, since `LibarchiveArchive` has no `Drop` of its own; `ZipWriter` self-finalizes) and warns once; `finish()` remains the only error-surfacing commit path. Parity matrix documented in `docs/API_REFERENCE.md`; recorded as DCR-004 + AD 0053 addendum. Tests: 5 new in `src/archive/mode_split.rs` (read parity, recursive add, reader/replace + source inspection, ZIP best-effort drop, libarchive no-leak drop).

### Problem

AD 0053 D2's typed handles (`ReadArchive`, `WriteArchive`, `ModifyArchive`)
under the `v2-api` cargo feature are intentionally additive but miss
several legacy facade operations that block adopting the typed API
without falling back to `Archive`:

- **R0076-0085** — `ReadArchive` lacks selective extract by id, raw-name
  extraction, and several inspection workflows present on the legacy
  facade.
- **R0076-0086** — `WriteArchive` lacks `add_directory_recursive`.
- **R0076-0087** — `ModifyArchive` lacks `add_entry_from_reader` and
  the `replace_entry*` operations.
- **R0076-0088** — Both `WriteArchive::Drop` and the inner
  `Archive::Drop` own write-mode finalization warning and execution;
  ownership of the durability boundary is split.

### Impact

Users adopting the typed API in v0.4 still need to drop back to the
legacy facade for common workflows (recursive add, reader-based
modification, replace). The split Drop ownership creates two paths to
finalization with subtly different semantics.

### Required Actions

1. Define v2 feature parity explicitly; either expose missing
   operations or document the typed API as intentionally partial.
2. Add `WriteArchive::add_directory_recursive`,
   `ModifyArchive::add_entry_from_reader`, and
   `ModifyArchive::replace_entry*`.
3. Make the typed wrapper the sole owner of finalization semantics
   by taking or marking the inner archive before Drop, or document
   the legacy delegation explicitly.

### Verification

- [x] v2-api parity matrix documented (`docs/API_REFERENCE.md` Typed Handle API section)
- [x] Add/replace/recursive parity reached (ReadArchive full read surface; WriteArchive `add_directory_recursive`; ModifyArchive reader/replace + source inspection)
- [x] Drop ownership unified (single finalization path via `Archive::finalize_write_on_drop`)
- [x] Existing v2-api tests extended for new methods (5 new tests in `src/archive/mode_split.rs`; full `--features v2-api` suite green)

### Related

- AD 0053 D2 (typed handle split) — addendum records the parity + Drop-ownership decision
- OI-0075-003 (Drop / finish error semantics — overlaps with R0076-0088; resolved consistently: `finish()` is the error-surfacing commit path, drop is best-effort)
- DCR-004 (this session's parity + single-finalization-owner change)

---

## OI-0076-005: Encapsulate public-field structs (v0.4)

- **Source:** R0076-0003, 0082 (Review 0076), R0076-0095 partial
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, deferred to v0.4 deprecation cycle)
- **Status:** OPEN (substantially advanced — `ExtractionLimits` done, R0081 I1, 2026-07-22; the
  `#[non_exhaustive]` half is now complete for every struct in scope — the five landed 2026-08-29
  and `ExtractionOptions` 2026-09-01, joining `ArchiveEntry`; see Progress below. What remains is
  Required Action 3: the `CompressionOptions` field-demotion timeline, which is blocked on a
  `level`/`progress` setter pair that does not exist)

### Problem

`ExtractionLimits` (R0076-0003) and `ArchiveEntry` (R0076-0082) expose
every field as public mutable. Validators have invariants that public
field assignment can bypass: `ArchiveEntry::permissions` should not
contain file-type bits, directories should not have sizes,
`ExtractionLimits::max_compression_ratio` should be finite and
positive (the R0076-0001 inline fix added a runtime guard but does not
prevent invalid construction).

`CompressionOptions` (R0076-0095) overlaps: format-specific builders
(`ZipCompressionOptions`, etc.) already exist as the recommended path,
but the flat-bag `CompressionOptions` remains struct-literal-compatible
for v0.3.

### Impact

Future invariants added by builders/setters can be bypassed by struct
literal updates. Adding a field is a source-breaking change for every
struct-literal initialiser in the wild.

### Required Actions

1. Define a private-fields + builder API for `ExtractionLimits` with
   validation in setters. Deprecate public-field access paths.
2. Promote `ArchiveEntryBuilder` (already shipped, R0075-0078) to the
   primary construction path; deprecate `ArchiveEntry::new` /
   `directory` and demote public fields to `pub(crate)` in v0.4.
3. Decide CompressionOptions deprecation timeline alongside the
   format-specific builders — coordinate with v0.4 cut.

### Verification

- [x] `ExtractionLimits::builder` is the documented primary API (R0081 I1) — public fields removed outright (pre-1.0), not `#[deprecated]`
- [x] `ArchiveEntry::new` / `directory` carry `#[deprecated]`
  <!-- Ticked 2026-09-01, verified by reading `src/entry.rs` rather than inferred from a closing report: `ArchiveEntry::new`, `ArchiveEntry::directory` and `ArchiveEntry::symlink` all carry `#[deprecated(since = "0.5.0", …)]`, each note naming its builder replacement (`file(..).build()`, `dir_at(..).build()`, `symlink_at(..).build()`) and the 0.6.0 removal. The 2026-08-21 note above ("still unticked, item 2 untouched") was true when written and is superseded. The rest of item 2 — demoting `ArchiveEntry`'s public fields to `pub(crate)` — is NOT done; what landed instead is `#[non_exhaustive]` on the struct, which closes external construction and exhaustive matching while leaving the fields `pub` and assignable. That is a different guarantee (field-addition freedom, not invariant enforcement) and is recorded as such in the 2026-09-01 Progress section below. OI-0001-005 (ticgit deff990a) still owns the entry-kind invariant half. -->
- [ ] `CompressionOptions` deprecation pathway documented
  <!-- Deliberately unticked 2026-08-21: partly satisfied, and the tick would overstate it. What landed (ticgit 165103b8) is a documented pathway for the *loose libarchive constructor* and for the *7z-style password setter*: `LibarchiveCompressionOptions::new` and `CompressionOptions::password` are both `#[deprecated(since = "0.4.0")]` with notes naming the replacement (`for_writable` / `try_new`) or the condition that lifts the deprecation (OI-0081-006). The flat-bag `CompressionOptions` itself is NOT deprecated and no timeline for it is recorded — `CompressionOptions::new` is the ordinary construction path for ZIP and 7z creation and carries no attribute. Item 3's decision, "the deprecation timeline alongside the format-specific builders", is therefore still unmade. -->
- [ ] Internal call sites migrated to builders
  <!-- Deliberately unticked 2026-08-21: satisfied for the typed-options work, not for this entry's scope. Three in-crate callers of the deprecated loose libarchive constructor were migrated to `for_writable`, and two are kept on the loose path with recorded reasons rather than by oversight (one because `WritableFormat` cannot express `Rar`, so migrating the test would delete its subject; one to pin the loose path's back-compat behaviour). That is the CompressionOptions axis. The `ArchiveEntry` axis of this box is untouched, so the box stays open with the item it belongs to. -->

### Progress (2026-07-22, R0081 I1)

`ExtractionLimits` is done. Its fields are now private behind
`ExtractionLimits::builder()` (`ExtractionLimitsBuilder`); the raw `f64`
compression ratio and `f64::MAX` / `u64::MAX` sentinels are gone,
replaced by the typed `Cap` (`Limited(u64)` / `Unlimited`) and the exact
rational `CompressionRatio` (integer `u128` cross-multiplication). This
resolves the Problem statement's specific concern that
`max_compression_ratio` "should be finite and positive ... but [runtime
guard] does not prevent invalid construction" — an invalid ratio is now
unconstructible (`CompressionRatio::new` rejects zero operands). The SFX
ceiling (AD 0040) and `reject_unsafe_paths` (AD 0066) were folded in as
first-class validated fields at the same time. All in-crate callers,
tests, and examples were migrated to the builder / accessors.

Because this is pre-1.0, the old public fields were removed rather than
`#[deprecated]` — the deprecation-cycle framing in the Required Actions
does not apply to `ExtractionLimits`. `ArchiveEntry` (item 2) and
`CompressionOptions` (item 3) remain open.

### Progress (2026-08-21) — the two folded-in `ExtractionLimits` fields now do something

Bookkeeping, because the Progress note above records these two as "folded in as first-class
validated fields" and a reader could take that to mean they were *enforced*. They were validated and
stored; neither was read. Both are now wired, which does not advance items 2 or 3 but does remove the
worst reading of this entry — a security setter that accepts a value and ignores it.

* **`max_sfx_payload_size`** (ticgit `1dfb92d6`, closed). The real gate in `stage_sfx_payload`
  compared against a `pub(crate)` alias of the default constant, so the default happened to agree
  and only a caller who *lowered* the cap was betrayed, silently. The ceiling is now a parameter,
  and three additive entry points — `open_sfx_with_limits`, `open_with_sfx_progress_and_limits`,
  `open_at_offset_with_limits` — source it from the caller's `ExtractionLimits`. The three
  limits-free entry points keep today's default, so nothing breaks, and
  `default_sfx_cap_matches_extraction_limits_default` pins the default and the alias together. The
  plumbing question this entry's sibling register framed as "signature change across the SFX
  surface" was answered additively.
* **`reject_unsafe_paths`** (ticgit `0d98ed8c`, closed). AD 0066 had specified the behaviour down to
  the error, and this is that implementation: `check_entry_paths_safe` is the gate where the flag
  binds, running one pass over the entries about to be materialized *before* a destination directory
  is created or a byte is decoded, and reporting the offending name rather than a laundered one.
  Default `false` returns `Ok(())` immediately, so the AD 0066 lossy-repair baseline pays nothing
  and behaves exactly as before.

### Progress (2026-08-29 and 2026-09-01) — the `#[non_exhaustive]` half is complete

Recorded here because this ledger was silent on both landings and a reader would otherwise take the
entry to be where it was in August.

**2026-08-29 — five structs.** `CompressionOptions`, `ModificationOptions`, `ValidationReport`,
`ResultWithWarnings<T>` and `StreamChecksum` gained `#[non_exhaustive]`, joining `ArchiveEntry`. No
new constructor was added for any of them, because none was missing. Nothing under `src/` needed an
edit — the attribute never restricts the defining crate.

**2026-09-01 — `ExtractionOptions`, the sixth, and the one deliberately split out of that group.**
It was the largest migration of the six on its own: 9 public fields, and the one type where
`..Default::default()` was the idiomatic in-tree spelling. It now carries the attribute and, unlike
the other five, gained a constructor because it needed one:

* `ExtractionOptions::new(destination)` — the documented defaults with `destination` filled in.
* Consuming setters chaining off it, in the style of the pre-existing `.password(…)`:
  `.overwrite(bool)`, `.preserve_permissions(bool)`, `.preserve_times(bool)`, `.verify_crc32(bool)`,
  `.limits(ExtractionLimits)`, `.filter(…)` and `.progress(…)`. The last two box internally, so a
  caller passes the closure or the callback itself.
* `Default` is untouched: a bare `ExtractionOptions::default()` still compiles outside the crate,
  with `destination` at `PathBuf::from(".")`. Only the literal form is gone — including
  `..Default::default()`, which `E0639` refuses by syntax whatever the base expression is, so the
  functional-update form is not an escape hatch.
* Two shapes stay field assignment and no setter is owed for either: an already-boxed
  `Box<dyn ProgressCallback>`, which does not satisfy `impl ProgressCallback` because the trait has
  no impl for the box, and clearing an `Option` back to `None`.

**What this does and does not settle.** It buys field-addition freedom on all seven structs and
nothing else: the fields stay `pub` and assignable, so no invariant is enforced by the attribute.
The Problem statement's concern about bypassable invariants is answered for `ExtractionLimits`
(private fields since R0081 I1) and for the entry-kind invariants only insofar as `build_checked`
exists; it is not answered by `#[non_exhaustive]`. Required Action 3 — the `CompressionOptions`
deprecation/demotion timeline — is still unmade, and still blocked on the same missing
`level`/`progress` setter pair, so this entry stays OPEN for it.

**Downstream debt this created, tracked in `docs/backlog.md` under the same id:** the `manual/`
bundle's `--ignored` snippet lane now has eight red fences, its allowlist escape hatch is closed by
three separate floors in `tests/manual_snippets.rs`, and two reference pages state the opposite of
what the code now does. That is a `write-diataxis-manual` sync job, not a hand-edit — editing a page
body invalidates its `synced_hash`.

### Update (2026-09-02) — the residual is re-homed to ticgit `c1296744`; nothing is owed by the owner

Recorded because `docs/backlog.md`'s copy of this entry was discharged on 2026-09-02 as a stale
premise, and an entry that stays OPEN here while leaving the backlog needs to say where its work
went. This entry's own status is unchanged in substance: the `#[non_exhaustive]` half is complete and
the field-demotion half is not.

What changed is that the demotion half is no longer waiting on a decision from this register. Ticket
`c1296744`, filed 2026-09-01, owns it and is the fuller record — it carries acceptance criteria,
scope, out-of-scope and verification commands this entry does not, and it fixes the accessor shape
that Required Action 3 was written to collect: bare name for `Copy` values, `_ref` for references,
`has_` for boxed callbacks. The ordering constraint that made demotion impossible is stated there
too and is real in the tree: `CompressionOptions` has read accessors (`format`, `level`,
`split_size`, `password_ref`, `has_progress`) but **no** `level` or `progress` setter, so demoting
the fields today would strand both. The setters come first; the demotion follows.

One half of this entry is not API work at all and is filed where it can actually be done. Seven
English manual pages still hold `ExtractionOptions { … }` struct literals, and two reference pages
are false rather than merely stale against the tree — `options-and-defaults.md` states "All fields
are public and the struct is not `#[non_exhaustive]`", and `public-api-surface.md` states "Nine
public types are marked `#[non_exhaustive]`" against 30-plus. `manual/` is a one-way generated
bundle, so those corrections must ride a `write-diataxis-manual` sync run rather than be
hand-patched. Worth knowing before that sync is scheduled: the snippet lane has headroom but not
much — `tests/fixtures/manual/unmarked_fragments.txt` holds 32 entries against a
`MAX_UNMARKED_FRAGMENTS` of 34, so absorbing six more red fences would breach it, and the
allowlisted-block and minimum-checked-snippet ceilings close the other two escapes. The sync has to
fix the pages, not silence the lane.

### Related

- R0075-0078 / R0075-0081 (builder + format-specific options shipped)
- AD 0053 (Group D architectural pass)

---

## OI-0076-006: External RAR creator design hardening

- **Source:** R0076-0072, 0073 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, design-level fix)
- **Status:** OPEN, **narrowed 2026-08-21** — the shell-out around the two design holes was
  hardened (ticgit `642488a0`, closed), but **neither of the two holes this entry names is
  closed**. What landed: `src/external/rar/` split into `argv` / `discovery` / `version` / `exit` /
  `runner` / `session` / `error`; arguments assembled as a `Vec<OsString>` handed to the process API
  with a `--` sentinel and leading-dash paths re-rooted, so there is no command string and no
  quoting layer to get wrong; arguments that cannot survive the process boundary (interior NUL, an
  empty password that would make `rar` block on an interactive prompt, CR/LF in a password, an empty
  entry list) refused up front; passwords redacted in every diagnostic via `redact_argv`; exit codes
  mapped to typed variants with an unrecognised code treated as failure; and `BinaryNotFound`
  (carrying the paths searched) kept distinct from unsupported-version / `UnsupportedPlatform`, so a
  caller handling the first installs something and the second upgrades it. `tests/external_rar_cli_contract.rs`
  drives a stub runner, so all of that is covered on a machine with no RAR installed. **Still open:**
  Required Action 1 — `create_archive` re-checks output existence immediately before the run
  (narrowing the window R0076-0072 named) but does not stage to an exclusive tempfile and does not
  pass a no-overwrite flag; the code says so at the check site. Required Action 2 — there is no
  path-shaping policy at all: `CreateRequest` carries `compression_flag`, `password`, `recurse`,
  `output` and `entries` and no cwd or base-path field, no `-ep`/`-ep1` switch is emitted, and
  nothing in the module documents which archive layout v0.4 promises. Required Action 3 —
  `tests/integration/external_rar_layout.rs` does not exist.

### Problem

`RarCreator` (the external WinRAR-CLI mode) has two design holes:

1. **R0076-0072** — Output existence is checked in `assemble`
   (`src/external/rar.rs`) much earlier than `create` invokes
   `rar.exe`. Another process can create the output between those
   two steps; `rar.exe`'s own behavior on existing output is
   format-version-dependent.
2. **R0076-0073** — `create` passes `self.output_path` and every
   input entry path verbatim to `rar.exe`. Absolute paths, the
   current working directory, and WinRAR path-stripping defaults
   can leak host layout into the archive and produce different
   archive name patterns depending on caller cwd.

### Impact

External RAR archives can:
- Overwrite an existing file at the destination on Windows
  silently (or fail in a way that loses the user's prior content).
- Embed absolute paths or per-host directory names that the
  user did not intend, since the archive layout depends on whoever
  invoked `rar.exe` and from where.

### Required Actions

1. Stage external RAR output to an exclusive tempfile in the
   same directory and rename on success, or pass a no-overwrite
   flag the chosen WinRAR build supports (and verify the result).
2. Define and document a path-shaping policy (which archive layout
   does v0.4 promise?), then invoke WinRAR with the cwd / flags
   needed to enforce it (e.g. `-ep1` for "exclude base path",
   `-ep` for "no path", explicit `cd` to a known root).
3. Cover the external-rar path matrix in
   `tests/integration/external_rar_layout.rs` (Windows-gated,
   feature-gated).

### Verification

- [ ] Output staging + rename in place
  <!-- Deliberately unticked 2026-08-21: load-bearing. Verified by reading src/external/rar/session.rs::create_archive — it re-checks `output_exists(request.output)` before the spawn and again after a success code, and its own comment states "this narrows the window rather than closing it (staging to an exclusive tempfile is tracked separately in OI-0076-006)". Post-run `OutputMissing` is new and worth having, but a success code is still allowed to land on a path another process created in between. -->
- [ ] Documented path-shaping policy
  <!-- Deliberately unticked 2026-08-21: load-bearing, and nothing landed for it. `CreateRequest` in src/external/rar/session.rs has no cwd or base-path field; grepping src/external/ for `-ep`, `ep1`, "exclude base", "path-shaping" and "layout" finds nothing about archive layout. The argv module's contract is about *safety* (no shell, no quoting, `--` sentinel, refuse un-passable arguments) and explicitly not about which names end up inside the archive, so R0076-0073's leak of host layout into the archive is untouched. -->
- [ ] Layout regression test on the Windows external-rar lane
  <!-- Unticked 2026-08-21: tests/integration/external_rar_layout.rs does not exist. tests/external_rar_cli_contract.rs is the new lane and it is a stub-runner contract test for quoting, exit mapping and the two unavailability errors — deliberately binary-free — so it cannot cover layout. The one lane that needs the real binary is `#[ignore]`d with its command line, and the Windows execution environment is still OI-0065-001 / ticgit 1340e934. -->

### Related

- AD 0019 (UnRAR process-wide mutex — adjacent boundary)
- OI-0065-001 (Windows libarchive — adjacent Windows policy)

---

## OI-0076-007: Libarchive `archive_read_data_skip` return-code sweep

- **Source:** R0076-0029, 0030, 0031, 0032, 0033, 0034, 0044, 0068 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, mechanical follow-up to AD 0059)
- **Status:** RESOLVED 2026-06-06 — added `unsafe fn checked_data_skip(archive) -> Result<()>` (ARCHIVE_OK/ARCHIVE_WARN = success, else `classify_libarchive_error`; caller frees its own handles on the Err path). All 10 physical bare-skip sites in `src/ffi/libarchive_wrapper.rs` now route through it (the 6 enumerated findings span 10 sites: `extract_all` selection 679 / symlink 697 / hardlink 705 — these free both `ext` and `archive`; single-file search 1142; memory extraction 1204/1336; integrity walk 2282/2287; streaming search 2649/2669 — these free `archive`). The two pre-existing checked `list_files` skips (R0075-0020) were also converted to the helper so the skip-check has a single source of truth. UnRAR `RAR_TEST` recovery skip (`src/ffi/wrapper.rs`, R0076-0068): the `RAR_SKIP` return is now checked and a failed recovery skip `break`s the best-effort integrity walk (the entry is already in `failed_files`; the old silent `continue` risked re-processing/stalling a wedged handle). `build_staging_path` (R0076-0044): the placeholder `remove_file` now surfaces non-`NotFound` errors as `ArchiveError::io`. Per-site handle cleanup (no leak / no double-free) and the UnRAR break + staging change adversarially verified (2 independent auditors, pass). No DCR: this completes the mechanical sweep AD 0059 already named; no design decision changed. Tests: full `--all-features` suite green (lib 464, integration 145, format-compat 45, integrity 16, crc 9, streaming 9).

### Problem

AD 0059's libarchive return-code hygiene sweep was partial; it
explicitly noted `archive_read_data_skip` checks as the "mechanical
follow-up sweep". Review 0076 enumerates the remaining 8 unchecked
sites:

- `extract_all` selection skip (R0076-0029).
- Symlink + hardlink skip in `extract_all` (R0076-0030).
- Single-file search skip (R0076-0031).
- Memory extraction skip (R0076-0032).
- Integrity walk skip (R0076-0033).
- Streaming search skip (R0076-0034).
- `build_staging_path` discards `temp_path.keep()` and `remove_file`
  results (R0076-0044).
- UnRAR `RAR_TEST` recovery skip return ignored (R0076-0068).

### Impact

A failed skip leaves the libarchive cursor in an undefined position;
the next iteration's `archive_read_next_header` may report a misleading
error or report success on a torn payload stream. Memory extraction
can buffer the wrong bytes after a skipped non-target entry. Integrity
walks can claim success after failing to advance past a metadata-only
entry. UnRAR `RAR_TEST` recovery can leave the iterator on the same
entry while the loop continues.

### Required Actions

1. Introduce a small `unsafe fn checked_data_skip(archive)` helper
   that returns `Result<()>` and surfaces skip failures via
   `classify_libarchive_error`.
2. Replace each of the 8 unchecked sites in
   `src/ffi/libarchive_wrapper.rs` with the helper, plus the matching
   unchecked spot in `src/ffi/wrapper.rs::940` for UnRAR.
3. Surface tempfile keep/remove errors in `build_staging_path` rather
   than discarding via `let _ =` (R0076-0044) — return the IO
   error to the caller.

### Verification

- [x] All skip sites use the checked helper (10 libarchive sites + the UnRAR `RAR_SKIP` site; the 2 pre-existing `list_files` checks also folded into the helper)
- [x] `build_staging_path` surfaces non-`NotFound` placeholder-removal failures as `ArchiveError::io`
- [x] Existing libarchive tests still pass (full `--all-features` suite green; per-site handle cleanup adversarially verified — no leak/double-free)

### Related

- AD 0059 (libarchive return-code sweep — explicitly notes this as
  the mechanical follow-up; this OI completes it, no new design decision)

---

## OI-0078-001: Fixture coverage for read-only ZST/LZ4/LZMA formats

- **Source:** R0078-0010 (Review 0078)
- **Date:** 2026-05-04
- **Decision:** ACCEPT (Phase 2 routing — explicit `track` for fixture
  generation work that exceeds in-session scope)
- **Status:** RESOLVED 2026-07-06 — six committed fixtures under
  `tests/fixtures/` (`test.txt.{zst,lz4,lzma}` standalone streams,
  `test.tar.{zst,lz4,lzma}` single-member ustar tars; all < 200 bytes,
  generation commands recorded in `tests/fixtures/README.md`) plus
  `tests/integration/readonly_codec_formats.rs` with one test per codec
  family covering format detection, the MADR-0019 stem-named raw
  pseudo-entry, `extract_to_memory` byte-exact payload, tar-member
  listing, and `extract_all` round-trip. All three tests pass against
  the linked libarchive — no `#[ignore]` fallback was needed.

### Problem

The newer libarchive-backed read-only formats (`.zst`, `.lz4`, `.lzma`,
`.tar.zst`, `.tar.lz4`, `.tar.lzma`) gained enum variants, capability
matrix entries, extension routing, and magic-byte detection (R0075-0031,
this review). They have **no fixture coverage** under
`tests/fixtures/`, so any regression in the read/list/extract path for
these formats would slip through the integration suite.

### Impact

The most regression-prone surface (magic-byte detection) is covered by
unit tests landed in this review (`test_detect_zst_magic`,
`test_detect_lz4_magic`, plus the extended `format_compatibility_test`
extension table). The deeper open/list/extract round-trip for each new
codec is not exercised end-to-end.

### Required Actions

1. Add a `.zst`, `.lz4`, `.lzma` fixture under `tests/fixtures/` (or
   generate at test time via a new dev-dependency such as the `zstd` /
   `lz4` / `lzma-rs` crates — pick one approach and apply consistently).
2. Add a `.tar.zst`, `.tar.lz4`, `.tar.lzma` fixture under the same
   directory.
3. Add one cross-format integration test per family that opens the
   fixture, lists entries, and extracts payload bytes. Existing
   integration patterns under `tests/integration/` are the right
   shape.

### Verification

- [x] Fixtures committed (or programmatic generation wired in).
- [x] Integration tests pass on every backend matrix.
- [x] No regression in build time or fixture footprint beyond a small
      delta.
  <!-- Ticked 2026-08-12: reopen verified the six fixtures, their sizes, the generation commands in tests/fixtures/README.md, and the three tests, none `#[ignore]`d. Note "committed" is not yet literally true — the fixtures are untracked along with the rest of the working tree. -->

### Related

- OI-0075-004 (R0075-0031 — added the enum variants)
- MADR-0019 (libarchive `format_raw` policy for standalone codecs)

---

## OI-0076-008: Misc small correctness items (residual)

- **Source:** R0076-0009, 0012, 0013, 0015, 0018, 0023, 0024, 0025, 0026, 0046, 0047, 0048, 0062, 0063, 0064, 0065, 0069, 0070, 0074, 0075, 0091, 0093 (Review 0076)
- **Date:** 2026-05-01
- **Decision:** ACCEPT (Phase 2 routing — auto mode default, residual small items deferred to keep this session bounded)
- **Status:** RESOLVED 2026-07-06 — all 22 sub-items landed (or verified moot),
  per-item disposition:
  - **0009 / 0062** — cap honoured *before* buffering via the new
    `ffi::common::read_entry_to_memory_capped` (declared > cap rejected up
    front, no over-reserve): Piz threads `max_bytes` through its memory chain
    (`extract_to_memory_with_limits`), 7z through
    `extract_to_memory_capped` + a `ReadBackend::extract_to_memory_with_limit`
    override.
  - **0012 / 0064 / 0065** — every cancellation path (extraction loop
    boundaries, mid-entry chunk copies, creation notifies) now returns the
    typed `ArchiveError::Cancelled { operation }` instead of
    `Format`/`OperationBlocked`; AD 0021 amended, `API_REFERENCE.md` updated.
    Thanks to the earlier helper consolidation this was 3 sites in
    `ffi/common.rs`, not 10 per-backend ones.
  - **0013** — `AtomicOutputFile::commit` parent-fsync failure now emits an
    `eprintln!` warning (house style; no logging dep exists) instead of
    `let _ =`. Still non-fatal per R0075-0037.
  - **0015 / 0018** — ZIP writer: `add_file_from_data_with_metadata` shares
    the chunked `write_data_chunked` notify loop; `Drop` warns on finalize
    failure (mode_split style), panic-free.
  - **0023** — investigated: `pkg-config` 0.3.32 self-emits
    `cargo:rerun-if-env-changed` for every env var it consults
    (`env_metadata: true`), so duplicating the set would force spurious
    rebuilds; documented the delegation in `build.rs` and added the one
    directly-read variable (`MAKE`).
  - **0024 / 0025 / 0026** — UnRAR vendored build: staging dir recreated
    from a clean slate; rerun-if-changed now walks sources recursively;
    `MAKE` env honoured with `make` fallback.
  - **0046 / 0047** — libarchive checksum classification centralized in
    `is_libarchive_checksum_failure` with a tested marker set;
    `copy_data` threads the caller op label (extract_all/extract_file).
  - **0048** — ZIP by-name opens resolve through the normalized
    central-directory index (`normalized_path_eq` fallback scan +
    `by_index_decrypt`), so backslash-stored entries are reachable by their
    listed name; regression test added.
  - **0063** — moot: the overflow-prone manual chunk loop was replaced by
    the shared `read_entry_to_memory_bounded` (take-bounded `read_to_end`,
    saturating length checks) during the 2026-07-05 simplify pass.
  - **0069 / 0070** — UnRAR FILETIME conversion preserves 100-ns sub-second
    precision (secs + subsec-nanos split, overflow-free); progress byte
    counter uses `saturating_add`. Three new unit tests pin the conversion.
  - **0074 / 0075** — SFX fallback failures now surface a combined
    diagnostic carrying both the original detection error and the SFX probe
    outcome, symmetrically in `Archive::open` and `Archive::open_encrypted`;
    unit tests cover both.
  - **0091** — manifest-digest streaming bounded via
    `with_hard_cap(declared size, 1 GiB fallback)` so an over-producing
    decoder errors instead of feeding unbounded bytes to the hasher.
  - **0093** — `ArchiveFormat::detect` falls back to extension detection for
    files too small to carry magic bytes (2-byte `tiny.zip` → Zip);
    normal-size precedence (magic wins) unchanged; tests updated.

  Verified: `cargo clippy --all-targets --all-features -D warnings` clean;
  full `--all-features` suite green (see also OI-0065-003 / OI-0078-001
  resolved the same day).

### Problem

Review 0076's residual small fixes are individually mechanical but
collectively significant. Grouped here so they don't get lost; each
sub-item is independently mergeable.

| Issue | Fix shape |
|---|---|
| R0076-0009 | Piz `extract_to_memory_with_max_mmap` reserves declared size before the trait-level `max_bytes` cap runs → reserve `min(declared, cap)` first |
| R0076-0012, 0064, 0065 | Cancellation returns format errors → `ArchiveError::Cancelled { operation }` (3 sites: `src/ffi/common.rs:469`, `sevenz_wrapper.rs:317`, `wrapper.rs:535/620`) |
| R0076-0013 | `AtomicOutputFile::commit` parent fsync error swallowed → `tracing::warn!` instead of `let _ =` |
| R0076-0015 | ZIP `add_file_from_data_with_metadata` not interruptible → share chunked write loop with `add_file_from_data` |
| R0076-0018 | `ZipWriter::Drop` hides finalize failures → emit warning |
| R0076-0023 | `build.rs` missing `cargo:rerun-if-env-changed` for pkg-config / libarchive override variables |
| R0076-0024 | UnRAR staging does not delete stale files → recreate / sync staging dir |
| R0076-0025 | UnRAR rerun emits only top-level files → walk recursively |
| R0076-0026 | UnRAR build hard-codes `make` → honour `MAKE` env var |
| R0076-0046 | Libarchive checksum classification stringly typed → centralize classifier with explicit tested message set |
| R0076-0047 | Libarchive `copy_data` hard-codes `EXTRACT` op → thread caller op label |
| R0076-0048 | ZIP listing/extract by-name disagree on backslash → resolve by normalized index |
| R0076-0062 | SevenZ memory reserves declared size before applying read cap → honour cap up front |
| R0076-0063 | SevenZ memory loop counter overflow → checked arithmetic |
| R0076-0069 | UnRAR FILETIME drops 100-ns precision → preserve nanos |
| R0076-0070 | UnRAR progress byte counter overflow → saturating_add |
| R0076-0074, 0075 | SFX fallback error precedence asymmetric → combined diagnostic |
| R0076-0091 | Manifest digest can stream unbounded payloads → reuse bounded stream extractor |
| R0076-0093 | Format detection skips extension fallback for tiny files → run extension check first |

### Verification

- [ ] Each sub-item has a regression where applicable — in practice these live in `#[cfg(test)]` modules under `src/`, not under `tests/`  <!-- Reworded 2026-08-12: no `tests/` file references any of the 22 sub-items. Reopen also found five sub-items with no regression anywhere (R0076-0009/0062, R0076-0015 among them); those remain genuinely unverified and are NOT covered by this box. -->
- [ ] CI green on the typical backend matrix  <!-- Cannot be satisfied 2026-08-12: the repository has no CI configuration at all (no .github/workflows, no .gitlab-ci.yml). Tracked as OI-0065-001 / ticgit 1340e934. -->

### Related

- AD 0059, AD 0064, OI-0075-001

---

## OI-0080-001: Cross-compilation support in build scripts

- **Source:** R0080-0049, R0080-0050 (Review 0080)
- **Date:** 2026-07-17
- **Decision:** ACCEPT (tracked; explicitly out of scope pre-v2 per owner)
- **Status:** PARTIALLY RESOLVED 2026-07-22 (Innovation I4) — the **vendored-UnRAR half** (R0080-0050) is resolved; the **libarchive-discovery half** (R0080-0049) is still **OPEN**. See note below.

### Resolution note (2026-07-22, Innovation I4)

Required Action 2 landed: `build.rs` now compiles the vendored UnRAR C++ with the `cc` crate
instead of shelling out to `make lib`. `cc` forwards Cargo's `TARGET`/`CC`/`CXX`/`AR` and sysroot
flags automatically, and the source list + preprocessor defines are now selected from
`CARGO_CFG_TARGET_OS` / `CARGO_CFG_TARGET_ENV` (the Cargo *target*), not the make invocation's host
toolchain — so a cross build emits target-arch objects and target link directives for the UnRAR
half. This also retired the make staging / artifact-purge machinery (see AD 0039 amendment) and the
Windows `panic!` (see the MADR-0020 amendment — the record older documents cite as "AD 0020").

Still OPEN: Required Action 1 (the libarchive pkg-config/Homebrew discovery block still branches on
host `#[cfg(target_os = ...)]`, evaluating for the build host, not `--target`) and Required Action 3
(no cross-compile smoke job has landed). The libarchive discovery axis is shared with OI-0065-001
(Windows-native libarchive) and remains the open half of this issue.

### Problem

`build.rs` selects libarchive discovery/link behaviour with `#[cfg(target_os = ...)]`, which in a
build script evaluates for the build **host**, not Cargo's `--target` (R0080-0049). The vendored
UnRAR build forwards only `MAKE` — `TARGET`, `CC`, `CXX`, `AR`, sysroot flags and Cargo linker
configuration are never translated into the make invocation, so a cross build compiles host-arch
objects and links them as target code (R0080-0050).

### Impact

Cross-compiling produces wrong-platform link directives and wrong-architecture static objects.
Native builds (host == target) are unaffected. Cross-compilation is not currently claimed,
documented, or tested; the owner has ruled it out of scope before v2 (native Windows/macOS/Linux
builds are the support matrix — see Review 0080 gate, 2026-07-17).

### Update (2026-09-03) — blocked on the scope ruling, and it always was

This issue has been carried as though "no CI" were part of its blocker. Under
AD-0070 that phrasing is retired, and removing it makes the real position
clearer rather than weaker: **cross-compilation is out of scope pre-v2 by a
standing owner ruling**, and that ruling is what blocks this, on its own, with
or without any CI arrangement.

Worth recording so the lane is not re-derived when it is unparked: the check
half is already reachable here. `x86_64-unknown-linux-musl` std is installed on
the dev host, and with `rar-support` disabled the build compiles no vendored
C++ — so `cargo check --target x86_64-unknown-linux-musl --no-default-features`
runs today and is exactly what would expose Required Action 1's defect, since
the libarchive discovery block would still branch on the *host* `cfg` rather
than on `--target`. What is not reachable is linking and running a Linux
binary, which needs a Linux host or container; OrbStack is installed on this
machine and its use is deferred by owner ruling 2026-09-03.

### Required Actions

1. Branch `build.rs` on `env::var("CARGO_CFG_TARGET_OS")` (plus `rerun-if-env-changed`) instead of host `cfg`.
2. Drive the vendored UnRAR build from Cargo target variables (`cc` crate or explicit `CC`/`CXX`/`AR`/flags plumbing into the makefile), preserving the current native macOS/Linux static build behaviour exactly.
3. When this is unparked, enable release-gate lane L7
   (`cargo check --target x86_64-unknown-linux-musl --no-default-features`),
   which is runnable on the dev host today — that target's std is installed and
   with `rar-support` off there is no vendored C++ to build. The link-and-run
   half needs a Linux sysroot or a container and is an environment need rather
   than a CI need. **Restated 2026-09-03 (AD-0070)**, which was previously
   "Add at least one cross-compile smoke job … when this lands".

### Verification

- [ ] Native macOS/Linux builds byte-identical in behaviour
- [ ] One documented cross-target builds and links

### Related

- OI-0065-001 (Windows-native libarchive discovery — different axis: native enablement, not cross)

---

## OI-0080-002: Piz mmap lifetime vs. external truncation/mutation (AD 0065 hardening)

- **Source:** R0080-0003 (Review 0080)
- **Date:** 2026-07-17
- **Decision:** ACCEPT (doc correction landed with Review 0080; code hardening tracked here)
- **Status:** RESOLVED 2026-07-23 — **eliminated by construction**, not mitigated. The AD 0007 R4 collapse (owner-approved; paired DCR-009) removed the `piz` backend and its `memmap2::Mmap` entirely. The sole ZIP backend (`zip` crate) reads over `std::fs::File`: external truncation surfaces as an ordinary I/O error, never a `SIGBUS`, and there is no shared mapped view for an in-place overwrite to silently mutate. The `memmap2` dependency is gone. No hardening code was needed because the hazard's substrate no longer exists. Note: a future opt-in `Cursor<Mmap>` fast path (AD 0007 Option C) would re-introduce this hazard and must reopen this OI if pursued.

### Problem

The Piz backend memoises a `memmap2::Mmap` per archive handle (`OnceCell<Mmap>`). AD 0065
described this as a stable snapshot, but an mmap is a live view of the inode: a non-cooperating
process truncating the archive makes later access to mapped pages past EOF raise SIGBUS
(deterministic process crash), and an in-place same-length overwrite silently changes bytes under
the cached listing. Read paths take no advisory lock (only modify mode locks — MADR-0009).

### Impact

Crash (not memory-unsafety UB) or silently inconsistent reads when an external writer mutates the
archive mid-session. Requires a non-cooperating local writer — the same threat class AD 0009
accepts — but AD 0065's original wording over-promised immutability (now corrected).

### Required Actions

1. Decide the hardening shape: (a) capture open-time file identity + length and revalidate before mmap-backed reads, (b) snapshot hot ranges into owned storage where stable semantics are required, and/or (c) a SIGBUS-tolerant read wrapper. Option (a) is the cheapest and mirrors the identity revalidation landed for modify mode in Review 0080.
2. Weigh perf/memory cost against the mmap fast path AD 0065 exists for; record the outcome as an AD 0065 amendment.

### Verification

- [x] Resolved by removal — the mmap substrate is deleted (AD 0007 2026-07-23 collapse / DCR-009); `memmap2` no longer a dependency; the sole ZIP backend reads over `File`. A concurrent-truncation regression test is moot because no mmap remains to fault.

### Related

- AD 0065 (amended 2026-07-17: "live mapping" wording; amended 2026-07-23: live-mmap portion removed), AD 0007 (2026-07-23 collapse), DCR-009, AD 0009, R0080-0005 identity-revalidation work

---

## OI-0080-003: Thread an entry-count budget into listing parsers

- **Source:** R0080-0010 (Review 0080)
- **Date:** 2026-07-17
- **Decision:** ACCEPT (tracked — architectural)
- **Status:** RESOLVED 2026-07-19 — `ReadBackend::list_files_budgeted(Option<usize>)` is now the required trait method (`list_files()` = provided delegate with `None`); all five backends enforce the budget in their uncached parse paths, and every limits-carrying extraction path threads `Some(limits.max_entry_count)` via `Archive::list_files_for_limits_budgeted`. Honest per-backend guarantee (documented on each override): libarchive and UnRAR abort before reading past the budget+1-th header (bounds library work too); zip/piz/sevenz-rust2 materialize their library-internal TOC on reader construction, so the budget bounds only our `ArchiveEntry` Vec — the post-materialization gate remains the effective cap there. Cached listings (AD 0065) bypass the budget by design; failed budgeted parses do not poison the cache. Tests: 3 in zip_wrapper (abort shape, None succeeds, cache-hit bypass). TicGit `fcdd2e` closed.

### Problem

`max_entry_count` has exactly one consumer: `check_extraction_safe`, which receives an
already-materialized `&[ArchiveEntry]`. Every backend fully parses and allocates the listing
before the limit is consulted, so the limit cannot prevent the memory/CPU cost of parsing an
attacker-controlled number of metadata records.

### Impact

Bounded DoS surface: a crafted archive with millions of central-directory/TOC records costs full
parse + allocation before rejection. Payload bytes are separately capped; this is metadata-only.

### Required Actions

1. Thread an entry budget into each backend's listing parser (`list_files*` implementations) and abort before allocating past it.
2. Document the residual: C libraries (libarchive/unrar/sevenz-rust2) materialize their own internal TOCs, capping how early Rust can abort — state the achievable guarantee per backend.

### Verification

- [x] Over-budget listing is a true streaming abort on libarchive and UnRAR (never reads past budget+1 headers); on the `zip` crate and sevenz-rust2 it bounds only this crate's `Vec<ArchiveEntry>`, because those libraries parse their central directory / TOC at reader construction  <!-- Reworded 2026-08-12: the original criterion named the pure-Rust backends, which are precisely the two that cannot do it; the abort landed on the other two. -->
- [x] Regression test with a synthetic high-entry-count archive
  <!-- Ticked 2026-08-16: verified against the tree. `src/ffi/zip_wrapper.rs::test_zip_list_files_budget_aborts_over_budget` builds a synthetic 50-entry ZIP through `build_many_entry_zip`, asserts `list_files_budgeted(Some(10))` fails as `OperationBlocked` with a reason containing "parsed more than 10 entries", then asserts a later unbudgeted parse still returns all 50 — covering the abort shape and the no-cache-poisoning half in one test. Its doc comment names OI-0080-003. The box was satisfied when the Status flipped to RESOLVED and was simply never ticked. Note the coverage this box does not claim: the synthetic-archive regression is zip-only, so the libarchive/UnRAR true-streaming-abort guarantee asserted by the box above rests on the implementation, not on a test. -->

### Related

- AD 0062 (extraction limits), R0080-0011 (archive-wide count gate — fixed in Review 0080)

---

## OI-0080-004: Typed multipart volume parser with sequence-continuity validation

- **Source:** R0080-0092 (+ R0080-0093 routing question) (Review 0080)
- **Date:** 2026-07-17
- **Decision:** ACCEPT (tracked — this is MADR-0013's deferred "Option 2", now with a concrete design sketch)
- **Status:** PARTIALLY LANDED 2026-08-21 (ticgit `513f99fc`, closed) — the typed model exists as
  **additive public API** and nothing inside the crate consumes it yet. `src/format/multipart.rs`
  ships `VolumeScheme` (`ZipSplit` / `RarPart` / `RarOldStyle` / `Numeric`, ordered so a stray
  old-style sibling in a new-style set is reported as the anomaly), `VolumeName` +
  `parse_volume_name`, `Volume`, `VolumeSet`, `parse_volume_set` → `VolumeSetReport`, and
  `continuity_defects(&VolumeSet) -> Vec<VolumeSetDefect>` — Required Actions 1 and 2, with the
  return type deliberately a *defect list* rather than a bool or a single error, so a caller learns
  **which** volume is missing and learns about more than one problem per set. Volume number `0` is
  refused for every scheme and a digit run wider than `u32` is not a volume number, so the 1-based
  continuity arithmetic cannot go ambiguous. **What has not landed:** Required Action 4 —
  `Archive::detect_multipart` (`src/inspection.rs`) still does its own sibling scan with the
  pre-existing string predicates (`is_zip_split_ext`, `zip_part_boundary`, `parse_rar_part_suffix`)
  and never calls the parser, so there are two implementations of "is this a volume name" in the
  crate; and Required Action 3 — the 7z `.001` routing question (R0080-0093) is untouched, with the
  parser's own scope note confirming ZIP `.z01` and 7z `.001` remain unsupported end to end.

### Problem

`detect_multipart` is name-heuristic + count-after-sort: any ≥2 matching siblings report a
complete multipart set. There is no sequence-continuity validation — a set missing `.z02`
(present: `.zip`, `.z01`, `.z03`) still reports as multipart with no incomplete-set signal.
`MultipartLayout::Multi { parts: Vec<PathBuf> }` (R0075-0083) carries no numbering or gap data,
so callers cannot detect the gap either. Additionally (R0080-0093), the numeric `.001` branch is
unreachable for SevenZip-detected content because `SevenZip.supports_multipart()` is `false` —
the branch today only serves zip/rar-magic `.001` splits, and no test exercises it.

### Impact

Extraction of an incomplete volume set fails late (mid-extraction, backend-specific errors)
instead of failing fast with an actionable "volume N missing" diagnostic. Silent wrong-set
grouping is also possible when unrelated files match the loose patterns (largely mitigated by the
Review 0080 anchored-parser fixes, but the parser remains string-heuristic).

### Required Actions

1. Introduce a typed volume model, e.g. `VolumeSet { scheme: VolumeScheme, base: String, volumes: Vec<Volume { path, number }> }` with `VolumeScheme::{ZipSplit, RarPart, RarOldStyle, Numeric}` — one parser shared by matching, numbering, continuity checking, and sorting (Review 0080 already unified match+sort anchoring; this OI lifts it into a typed structure).
2. Add continuity validation: detect gaps in the numbered sequence and require the terminal/main volume (`.zip` for zip-split, lowest `.partN`/`.rar` for RAR). Surface incompleteness explicitly — either a new `MultipartLayout::Incomplete { missing: Vec<u32>, .. }` variant or a `gaps: Vec<u32>` field on `Multi` (v0.4 API decision; `MultipartLayout` is `#[non_exhaustive]`-ready).
3. Decide 7z `.001` routing (R0080-0093): either route numeric splits before the `supports_multipart` capability gate (detection keyed on the numeric extension, not the inner format) with real fixtures + tests, or remove the dead branch. Today's comment documents the limitation.
4. Migrate `detect_multipart` to delegate to the typed parser; keep the boolean/list surface as a thin adapter for source compatibility until v0.4.
5. Tests: gap detection (`.z01`+`.z03`), missing main volume, mixed-case sets (case handling landed in Review 0080), `.7z.001` routing per the chosen design.

### Verification

- [ ] Typed parser is the single source of truth (no residual ad-hoc `.part`/digit scanning)
  <!-- Deliberately unticked 2026-08-21: load-bearing, and the criterion is the part that did not land. Verified by reading src/inspection.rs::detect_multipart, which still builds its own lowercased-name predicates and walks the sibling directory itself, and by grepping the whole of src/ for `parse_volume_set`, `continuity_defects` and `VolumeSet`: outside src/format/multipart.rs and its tests child, the only hits are the unrelated `VolumeSetSize` local in the vendored UnRAR C++ sources. Two parsers now exist where the criterion asks for one. -->
- [ ] Incomplete sets produce a first-class diagnostic before any backend open
  <!-- Deliberately unticked 2026-08-21: the *diagnostic* exists, the *before any backend open* does not. `continuity_defects` is public and returns a per-volume defect list, and tests/multipart_continuity_test.rs exercises it, but no open, extract or detect path in the crate calls it — so an incomplete set still fails wherever it failed before, and a caller only gets the actionable answer if they know to ask for it themselves. -->
- [x] Existing multipart integration tests remain green through the adapter
  <!-- Ticked 2026-08-21 with its premise corrected: there is no adapter — `detect_multipart` was not migrated — so this box is satisfied trivially rather than meaningfully. The suite is green at 38 suites / 1865 passed / 0 failed / 13 ignored (1833 was the pre-change baseline; the
  figure first recorded here was that baseline rather than the observed run), and the multipart lanes are green because the code under them is unchanged. Recorded rather than quietly ticked, because a green result here is not evidence for the criterion the box was written to test. -->

### Related

- MADR-0013 (heuristic tightening; Option 2 deferral), R0075-0083 (`MultipartLayout`), R0080-0086..0095 fixes (Review 0080)

---

## OI-0080-005: Manifest-based recursive creation (single-walk pinning)

- **Source:** R0080-0034 (Review 0080)
- **Date:** 2026-07-17
- **Decision:** ACCEPT (tracked)
- **Status:** RESOLVED 2026-09-02 — all four Required Actions. Actions 1–3 landed 2026-08-21
  (ticgit `330f3851`, closed); Action 4 landed 2026-08-26 in `2832f40`, verified in the tree and
  written up in the Update of 2026-09-02 below. The 2026-08-21 status text is kept verbatim from
  here on, because everything in it except its Action-4 clause still describes the design correctly,
  and that clause is superseded rather than deleted. *(2026-08-21, as written:)* RESOLVED
  2026-08-21 (ticgit `330f3851`, closed) for Required Actions 1–3; **Action 4
  (root-preservation semantics, R0081-0046) is untouched and restated below.** `src/creation/manifest.rs`
  holds a `SourceManifest` built by one walk — path, kind, size, mtime, unix mode and, on Unix,
  inode identity per entry — which reserves every entry in the write-mode namespace as it records
  it and is then replayed by `manifest.write_into(backend, ...)`. Because the replay is
  backend-agnostic, both writer backends emit the manifest rather than re-walking, which is Action 2
  without the reshaping of two separate recursive-add APIs the action anticipated. Action 3 is
  decided the cheaper way and documented as such: **path + identity revalidation at emission**, not
  open-at-validation — `verify_unchanged` runs immediately before each source is handed to the
  backend, and the module states plainly what the pin does and does not promise (the *set* is
  pinned; the window between `verify_unchanged` and the backend's own stat is narrowed, not closed,
  and is covered by the writers' declared-size guards). Each drift class carries its own error so a
  caller restoring a backup can tell them apart: vanished → `Io`/`NotFound`, size change →
  `Corruption` naming "grew"/"shrank", replaced object or changed inode → `OperationBlocked`
  ("was replaced"), same-size in-place rewrite → `OperationBlocked` ("was modified in place").
  Directory mtime is deliberately excluded from the drift check, because a directory's mtime moves
  whenever a child is created.

### Problem

Recursive archive creation performs two independent `WalkDir` traversals: the facade's paths-only
pre-walk (namespace + type validation) and each backend's own emission walk. Filesystem mutations
between the walks can introduce paths that were never validated, change entry types, or create
namespace conflicts after output has begun. The split is a documented deliberate design
(one cheap validation pass up front; backends own their I/O walk).

### Impact

TOCTOU requiring a racing local writer mutating the source tree mid-create — the cooperating-
process assumption of AD 0009 applied to creation. Severity is low after Review 0080: specials
are now rejected at preflight (R0080-0033), the output archive cannot be ingested
(R0080-0035/0036), and emission-side checks remain as defense in depth. The residual is
unvalidated late-appearing regular files.

### Required Actions

1. Design a single-manifest creation flow: the validation walk produces a manifest of (path, kind, metadata, optionally opened fd); backends emit exactly that manifest instead of re-walking.
2. Requires reshaping both writer backends' recursive-add APIs (zip_writer + libarchive) — schedule with the v0.4 backend-trait work.
3. Decide fd-pinning depth (open-at-validation defeats swaps but costs descriptors on huge trees; path+identity revalidation at emission is the cheaper middle ground).
4. Define root-preservation semantics (R0081-0046): when the added directory is a filesystem root (`/`, `C:\`) the relative path strips to empty and the root entry is currently skipped — decide whether to synthesize a stable archive name or document the drop, as part of the manifest model.

### Verification

- [x] One traversal feeds both validation and emission
  <!-- Ticked 2026-08-21: verified by reading src/creation.rs — `add_directory_recursive` calls `SourceManifest::build(dir_path, &output_path, OP, ...)` once and then `|backend, manifest| manifest.write_into(backend, OP)`, so the emission consumes the recorded manifest and neither writer re-walks. Confirmed by the manifest module's own tests: `build_records_the_whole_tree_in_walk_order` and `build_reserves_every_recorded_entry_exactly_once`. -->
- [x] Regression test: file appearing between validation and emission is not silently archived
  <!-- Ticked 2026-08-21: `test_recursive_add_ignores_sources_created_after_the_walk` in src/creation.rs uses the per-entry progress callback as the hook to create a file strictly between the walk and the write, then asserts through the public facade that the archive holds the recorded file and NOT the latecomer — and it first asserts the hook actually ran ("the test's own hook did not run — it proves nothing"), so it cannot pass vacuously. Its siblings `test_recursive_add_reports_source_removed_mid_write` and `test_recursive_add_reports_source_growth_mid_write` pin the two reportable drift classes. Note the resolved reading: appearance is *excluded* by the pin rather than reported, which the module doc states as a design choice, not an oversight. -->

### Residual (2026-08-21) — Required Action 4 is still owed

Root-preservation semantics (R0081-0046) were not addressed. When the added directory is a filesystem
root (`/`, `C:\`) the relative path strips to empty and the root entry is skipped; nothing in
`src/creation.rs` or `src/creation/manifest.rs` mentions R0081-0046 or a root case, so the manifest
model inherited the behaviour without ruling on it. The choice the action asks for — synthesize a
stable archive name, or document the drop — is unmade.

### Update (2026-09-02) — Required Action 4 landed; the Residual above is discharged

Root-preservation semantics were decided the first of the two ways the action offered — synthesize a
stable archive name — and the decision lives in the shared walk rather than in either writer, which
is why it did not show up where the 2026-08-21 check looked. `src/ffi/common.rs` defines
`archive_base_for_root()` and `compose_archive_path()` alongside
`DEFAULT_ROOT_ARCHIVE_NAME = "rootfs"`, called from `walk_directory_tree()`, which
`SourceManifest::build` uses; both writer backends inherit it because both replay the manifest
rather than walking themselves. Five unit tests in the same file pin the behaviour, including the
`/rootfs` → `rootfs/rootfs` collision case that a synthesized name has to survive. Landed in
`2832f40` (2026-08-26) and recorded in `CHANGELOG.md` as a behaviour change.

The Residual section above is left exactly as it was written on 2026-08-21 and is discharged by this
update rather than by an edit to it.

One thing this update deliberately does not claim: the ruling has no record under `docs/records/`.
Grepping that tree and `docs/project/` for `rootfs` returns nothing, so `add_directory_recursive`
on a filesystem root changed the shape of its output backed only by code comments and a
`CHANGELOG.md` line — unlike the comparable creation-output change of the same week, which got
`DCR-013`. That gap is reported to the record owner rather than filled here.

### Related

- AD 0009 (threat model), R0080-0033/0035/0036 (Review 0080 layered fixes), AD 0053 (backend trait evolution)

---

## OI-0081-001: Read-side file-identity revalidation (detect-then-reopen)

- **Source:** R0081-0015, R0081-0016 (Review 0081); also covers the untracked R0080-0009 (Review 0080)
- **Date:** 2026-07-18
- **Decision:** ACCEPT (tracked)
- **Status:** RESOLVED 2026-07-19 — the R0081-0017 `StubFileIdentity` helper was generalized to a read-side identity (Unix `(dev, ino, len)`; non-Unix len-only, documented weaker) and applied at every read entry point: `Archive::open` / `open_encrypted` capture identity at detection and revalidate after backend construction (typed `OperationBlocked` on drift); both SFX staging paths revalidate before the payload copy (R0075-0002 size assertion still covers mid-copy). `open_as_format` documents that binding spans detect→construct and is owned by its callers. The R0080-0009 bulk-walk half was already mitigated by the Review 0080 listing-drift cross-checks. Residuals documented: backend-internal open-by-path window (AD 0009 accepted class), `open_at_offset` with a raw caller offset has no detection open to bind, non-Unix same-length in-place overwrite undetected. Tests: identity-drift swap test (cfg(unix)) + stable-open happy path. TicGit `11da3a` closed.

### Problem

Read paths detect a format / probe an SFX from one open of the pathname, then reopen the pathname
in the selected backend (or to stage payload), never binding the two opens to one file identity:

- `Archive::open` / `open_encrypted` — format detected from one open, backend constructed from a
  second open (R0081-0015).
- `detect_sfx` + `stage_sfx_payload` — two independent opens; a same-size replacement between them
  stages different bytes than were detected (R0081-0016; size-change is already caught by the
  R0075-0002 `take`+size assertion).
- Bulk extraction — the safety gate runs on the cached AD-0065 listing while the extraction walk
  reopens `self.path` afresh (R0080-0009, landed in Review 0080 without a tracking entry — recorded
  here).

`extract_stub` (R0081-0017), the security-analysis-primitive sibling, was **fixed** in this review
(bytes read from the detection handle) and is not part of this OI.

### Impact

A non-cooperating local process replacing the pathname between the two opens can select a backend
for, or stage, bytes that are no longer present. Per MADR-0009 (advisory locking, cooperating-process
threat model), plain read has no third-inode escalation — the caller pointed at the path and
receives whatever now lives there — so this sits inside the accepted read-side threat class. Tracked
rather than fixed because it is accepted-risk hardening across several entry points, not a fresh
escalation.

### Required Actions

1. Extend the DCR-007 `LockedFileIdentity` capture+revalidate pattern to the read entry points: capture `dev`/`ino`/len at the first (detection) open and revalidate before/after backend construction and before SFX staging, aborting on drift.
2. Document the residual check-to-use window (libarchive/backends open by path with no fd hand-off).
3. Cross-reference OI-0080-002 (Piz mmap identity) — the same read-side identity concern at the mmap layer.

### Verification

- [x] Identity revalidated at each read entry point; regression test with a between-opens replacement (Unix `cfg`)
  <!-- Ticked 2026-08-12: reopen verified revalidation at every scoped read entry point plus the cfg(unix) between-opens regression test. R0001-0002 (landed 2026-08-09) closed a further gap this entry did not list — detect_sfx released the file before the identity was captured — and OI-0001-002 records that the bulk-walk drift cross-checks compare names only. -->

### Related

- DCR-007 (modify-side identity revalidation — pattern template), AD 0009, OI-0080-002, R0080-0009, R0081-0017 (fixed sibling)

### Update (2026-07-22, R0081 I6)

Required Action #2 documented the residual as "libarchive/backends open by path with no
fd hand-off." The fd hand-off (`archive_read_open_fd`) was evaluated this pass to close
that residual and **rejected** as unworkable for the libarchive read backend on a
macOS-first-class target: the handle is iterator-shaped (per-operation reopen, no single
lifetime fd), a re-handed owned fd shares one file offset across reopens while the public
API allows overlapping live `StreamingExtractor` handles, and no portable independent
open-file description can be derived from an fd (`dup`/macOS `/dev/fd/N` both share the
offset; only Linux `/proc/self/fd/N` doesn't). The backend-internal open-by-path window
is therefore **accepted-permanent** for this architecture, not a pending fd follow-up.
What landed: the read/modify/UnRAR `(dev, ino)` captures were unified into
`crate::fs_identity::InodeId` (single audited `MetadataExt` site); read-side comparison
semantics unchanged. See AD 0009 / DCR-007 amendments of the same date.

---

## OI-0081-002: Typed compression-option builder invariants

- **Source:** R0081-0005 (residual), R0081-0006 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** ACCEPT (tracked — the dead 7z password setter + over-absolute claim were fixed in this review; the deeper invariant work is tracked here)
- **Status:** RESOLVED 2026-08-21 (ticgit `165103b8`, closed) — Required Action 1 landed as **both**
  branches it offered. `WritableFormat` (`src/options/writable_format.rs`) is a newtype over
  `ArchiveFormat` whose sole invariant is `inner.can_create() == true`, checked once in
  `WritableFormat::new`, with per-format associated constants (`ZIP`, `SEVEN_ZIP`, `TAR`,
  `TAR_GZIP`, `TAR_BZIP2`, `TAR_XZ`, `TAR_ZST`, `TAR_LZ4`, `TAR_LZMA`) and an `ALL` slice. It is
  **derived from `can_create()` rather than hand-listed**, which is the constraint the register
  attached to this item after the writable set grew on 2026-08-04: unit tests in that file fail if
  `new`, `ALL` or any constant stops agreeing with `ArchiveFormat::can_create`, so the predicate
  keeps exactly one definition. Both `CompressionOptions` and `LibarchiveCompressionOptions` gained
  `for_writable(WritableFormat)` (infallible, for a literal format) and `try_new(ArchiveFormat)`
  (fallible, for a computed one). `LibarchiveCompressionOptions::new` — the loose path this item was
  filed against — is `#[deprecated(since = "0.4.0")]` with a note naming both replacements, rather
  than removed, so behaviour is unchanged for callers that keep using it. Required Action 2 is
  answered in the same pass and in the direction the action anticipated: `CompressionOptions::password`
  is now `#[deprecated]` too, on the ground that every value it can produce is rejected by
  `Archive::create`, with the deprecation explicitly scheduled to lift when OI-0081-006's opt-in
  ships. `WritableFormat` is re-exported at the crate root, so a caller no longer takes the
  constructor from the root and its argument from `unified_archive::options::`.

### Problem

The compression-option builders advertise that "invalid combinations cannot be constructed at all,"
but two escape hatches exist: `SevenZCompressionOptions::password` (fixed in this review — setter
removed / claim softened per MADR-0027's no-encrypted-creation stance, which its 2026-07-20
amendment downgraded from permanent to deferred behind an explicit opt-in — see OI-0081-006) and
`LibarchiveCompressionOptions::new(format)`, which accepts any `ArchiveFormat` and defers
write-capability rejection to `Archive::create_libarchive` (R0081-0006, claim softened in this
review). A true compile-time guarantee for the libarchive builder needs a restricted
writable-format enum.

### Impact

The typed-builder guarantee is weaker than documented for libarchive; a caller can build an options
value that only fails at call time. Low severity (runtime rejection is correct), but the type-level
promise is imperfect.

### Required Actions

1. Introduce a `WritableFormat` (or equivalent restricted enum) for `LibarchiveCompressionOptions::new`, or a fallible constructor, so non-writable formats cannot be selected.
2. Revisit the 7z password setter when encrypted creation lands (rejected at the creation boundary
   today, but no longer permanently: MADR-0027's 2026-07-20 amendment defers it behind an explicit
   opt-in, tracked as OI-0081-006 / ticgit 2c54e5).

### Verification

- [x] Non-writable libarchive formats are unconstructable (or rejected in a fallible constructor)
  <!-- Ticked 2026-08-21, both halves of the disjunction: read first-hand in src/options.rs and src/options/writable_format.rs. `LibarchiveCompressionOptions::for_writable` takes a `WritableFormat`, which cannot hold a non-creatable format, so that path is unconstructable; `try_new` is the fallible constructor and returns Err for one. The loose `new` still exists and still accepts anything — deliberately, `#[deprecated]` rather than removed for source compatibility — and two call sites keep it on purpose with recorded reasons: `libarchive_options_with_uncreatable_format_fails_at_create_time` needs it because `WritableFormat` cannot express `Rar` at all, so migrating it would delete the test's subject, and one assertion in src/options.rs exists to pin the loose path's back-compat behaviour. -->

### Related

- MADR-0027 (no encrypted-archive creation), R0081-0005/0006 (in-review fixes)

---

## OI-0081-003: Old / V7 tar content detection

- **Source:** R0081-0011 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** ACCEPT (tracked — new feature scope)
- **Status:** RESOLVED 2026-07-19 — `tar_v7_header_ok` probe added after the ustar branch: requires 512 bytes, no `ustar` magic, valid stored checksum (shared `tar_header_checksum_ok`, signed variant kept), non-empty name, octal size/mtime fields, and V7 linkflag (NUL/`0`/`1`/`2`). Detection-only change (extraction unchanged; libarchive already reads V7). Tests: synthesized V7 tar detects as Tar; all-zero, corrupted-checksum, and random-byte buffers rejected; ustar tests untouched. False-positive surface at or below the R0080-0076 baseline. TicGit `830d26` closed.

### Problem

TAR content detection requires the `ustar` marker plus header-checksum validation (checksum added
by R0080-0076). Pre-POSIX V7 tar has no `ustar` marker, so extensionless or misnamed V7 tar
archives are not content-detected, even though libarchive can read them. `.tar`-named archives still
detect via extension fallback.

### Impact

Extensionless/misnamed V7 tar archives fail to open by content detection. Narrow (the common case is
covered by extension fallback).

### Required Actions

1. Add a strict 512-byte checksum-validated old-tar probe (octal size/typeflag validation, magic absent) that does not reintroduce the false-positive surface R0080-0076 just cut.
2. Add false-positive fixtures (random data must not classify as V7 tar) and a V7 fixture (none exists today).

### Verification

- [x] V7 tar fixture detects; false-positive fixtures rejected
  <!-- Ticked 2026-08-12: reopen verified the synthesized V7 fixture detects as Tar (asserting the ustar branch is not what matched) and that six false-positive fixtures are each rejected, three with the checksum recomputed so the secondary gate does the rejecting. -->

### Related

- R0080-0076 (ustar checksum tightening — do not regress), AD 0015

---

## OI-0081-004: ZIP extended-timestamp central-record convention

- **Source:** R0081-0049 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** ACCEPT (tracked — blocked by the `zip` crate API)
- **Status:** OPEN

### Problem

One `0x5455` Extended-Timestamp payload carrying mtime+atime+ctime is attached to both the local and
central-directory records. The Info-ZIP convention is that the central-directory UT block carries
ModTime only. Most readers key off the block's `TSize` and tolerate the 13-byte central block, so
impact is a strict-reader edge.

### Impact

Strict readers may misparse or ignore the central-directory UT record. Low.

### Required Actions

1. Emit the full payload locally and an mtime-only payload centrally — **blocked**: `zip` 2.4.2's `FullFileOptions` offers only local+central (`central_only=false`) or central-only, no local-only channel; emitting two `0x5455` blocks centrally would be an invalid duplicate header id. Needs raw-header emission or an upstream crate change.
2. Reassess when the `zip` dependency exposes a local-only extra-field channel.

### Verification

- [ ] Central UT record carries mtime only (once the crate API allows)

### Related

- OI-0065-002 (extended-timestamp round-trip), `tests/integration/zip_extended_timestamps.rs`

---

## OI-0081-005: Windows self-ingestion file identity

- **Source:** R0081-0032 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** ACCEPT (tracked — Windows is a first-class target)
- **Status:** RESOLVED 2026-07-19 (implementation; runtime verification pending Windows CI, OI-0065-001) — `same_file_as_output` now has a `cfg(windows)` arm comparing `dwVolumeSerialNumber` + `nFileIndexHigh/Low` via a function-local `GetFileInformationByHandle` extern block (mirrors the `rename_noclobber` pattern), closing the hard-link-alias gap; the fail-open contract (R0081-0033) is preserved and `cfg(not(any(unix, windows)))` keeps the canonical-path fallback. The Windows arm cannot be compiled on this macOS host — flagged for the Windows CI job. TicGit `a2a683` closed.

### Problem

The self-ingestion guard (R0080-0035/0036) compares device+inode on Unix (robust) but falls back to
canonical-path equality on non-Unix, which does not resolve hard links. A hard-link alias to the
output archive on NTFS is therefore not blocked and could be recursively ingested into itself.

### Impact

Windows-only, hard-link-only self-ingestion edge. The primary (Unix) platform is correct.

### Required Actions

1. Implement Windows file identity via `GetFileInformationByHandle` (volume serial number + `nFileIndexHigh`/`nFileIndexLow`) and use it in `same_file_as_output`, matching the Unix `dev`/`ino` robustness.
2. Until then, the non-Unix guarantee is weaker — document it.

### Verification

- [ ] Windows hard-link alias to the output is rejected (tested on Windows)  <!-- Deliberately unticked 2026-08-12: load-bearing, not bookkeeping. This entry is RESOLVED on implementation only — its Status line says so — and the criterion waits on a Windows CI job existing at all (OI-0065-001 / ticgit 1340e934). Exclude it from any blanket tick-them-all pass. Reopen also noted the Unix half is testable today: `std::fs::hard_link` plus the (dev, ino) arm would pin the hard-link case on the dev host, where the three current tests pass identical paths and prove nothing about links. -->

### Related

- R0080-0035/0036 (self-ingestion guard), project platform directive (Win/macOS/Linux first-class)

### Update (2026-07-22, R0081 I6)

Windows file identity via `GetFileInformationByHandle` (this OI's landed fix) remains the
guard. A handle-based hand-off to the archive backend — the Windows analogue of the Unix
`archive_read_open_fd` idea explored this pass — is **not** pursued: it hits the same
architectural blockers documented in the DCR-007 / AD 0009 amendments (libarchive's
per-operation reopen has no single lifetime handle; a re-handed handle shares one file
pointer across reopens while overlapping live streams are allowed), and libarchive on
Windows would additionally require bridging a Win32 `HANDLE` to a CRT fd
(`_open_osfhandle`) for `archive_read_open_fd`. The by-name/by-handle identity capture,
now unified in `crate::fs_identity`, stays the mechanism on all platforms.

---

## OI-0081-006: Encrypted-archive creation behind an explicit opt-in

- **Source:** MADR-0027 amendment (2026-07-20, owner decision reversing the permanent rejection)
- **Date:** 2026-07-20
- **Decision:** ACCEPT (deferred opt-in — encrypted creation stays OFF by default until this lands)
- **Status:** OPEN

### Problem

MADR-0027 originally rejected encrypted-archive **creation** permanently. The owner has reversed that
to a deferral: the capability should ship behind an explicit opt-in. The original premise no longer
holds — the `zip` crate (`aes-crypto` already enabled) and `sevenz-rust2` (`aes256` + header
encryption, already a dependency) own the ciphers and cover encrypted ZIP and 7z; the read side
already parses the harder ciphertext surface.

### Impact

Users on the Win/macOS/Linux matrix cannot produce password-protected archives, a mainstream
expectation. Leaving it permanently rejected was the wrong long-term call; leaving it default-on
would create an AE-2 tool-compatibility / support burden.

### Required Actions

1. Design the opt-in surface: an explicit creation-encryption option (e.g. a password + algorithm on the ZIP/7z creation builders) that is inert unless the caller sets it, and an unmistakable "you are producing an encrypted archive" contract. Decide the default algorithm (AES-256) and whether to expose AE-1 vs AE-2 for ZIP.
2. Wire the `zip` crate AES creation path (ZIP) and `sevenz-rust2` aes256 path (7z); keep libarchive/other formats rejecting as before.
3. Document the AE-2 interop caveat prominently; keep encrypted creation OFF by default.
4. Reconcile the reversal chain in the records: gate-0007 (accepted AES creation) / gate-0022 (tracked its regression) / MADR-0027 (rejected) — mark the superseding relationships when this lands.

### Verification

- [ ] Opt-in encrypted ZIP creation round-trips (create encrypted → read back with password)
- [ ] Opt-in encrypted 7z creation round-trips
- [ ] Default (no opt-in) still rejects any creation password exactly as MADR-0027 established

### Related

- MADR-0027 (amended 2026-07-20), gate-0007 (ZIP AES-256 creation), gate-0022 (AES creation regression), AD 0007 (dual-ZIP — the `zip` crate is the creation backend)

---

## OI-0080-006: Libarchive header-status policy is split across the six read walks

- **Source:** R0080-0018 (Review 0080; review archived to the cold store 2026-08-07)
- **Date:** 2026-08-07
- **Decision:** ACCEPT (recovered by the `/indy-review-cleanup` triage of Review 0080, which had
  no single closure record; this finding was never routed)
- **Status:** RESOLVED 2026-09-02 (ticgit `12d4310b`, closed) — all four Required Actions. The
  policy is accept-`ARCHIVE_WARN` everywhere, it is implemented once rather than at eight call
  sites, a source-shape test stops it re-diverging, and the warning text that used to be discarded
  now reaches callers. Required Action 3 — the one genuinely outstanding piece as of 2026-09-02 —
  was completed in the same pass that wrote this line; see the Update below. Note for a later
  reader: the Problem section that follows describes the state on 2026-08-07 and is kept as the
  record of what was found, not as a description of the tree.

### Problem

`src/ffi/libarchive_wrapper/reader.rs` calls `archive_read_next_header` from six places and
applies two different success policies to the result:

- **Accept `ARCHIVE_WARN`** — `test_integrity`, and both `LibarchiveStreamReader::open` walks
  (backing `extract_to_stream` and `extract_to_stream_by_listing_id`).
- **Reject `ARCHIVE_WARN`** — `list_files_metadata_only_uncached`, `extract_all_with_options`
  and `extract_file_with_options`, which use the bare `result != ARCHIVE_OK` form and return
  `ArchiveError::format`.

### Impact

On a single archive whose header read returns `ARCHIVE_WARN`, `test_integrity` reports it clean
and `extract_to_stream` succeeds, while `list_files`, `extract_all` and `extract_file` all fail
with a format error. That is the "integrity says valid, normal reads say broken" contradiction the
finding named — plus a same-entry `extract_file` vs `extract_to_stream` split that did not exist
when the review was written.

`docs/investigation/codebase/ffi-libarchive.md` states the contract as uniform ("OK and WARN are
the two statuses the wrapper accepts as success on header reads and data skips"), which only three
of the six sites implement, so the documented contract is itself wrong today.

### Required Actions

1. Decide one header-status policy for all six walks. `checked_data_skip` already standardises
   OK/WARN for skips (OI-0076-007), so the precedent points at accept-WARN everywhere — but
   accepting WARN on a listing walk deserves an explicit ruling, not an inherited one.
2. Apply it to all six call sites and record the decision.
3. Reconcile `docs/investigation/codebase/ffi-libarchive.md` with whatever lands.
4. Add a regression test on a WARN-producing fixture asserting that integrity, listing and both
   extraction paths agree.

### Update (2026-09-02) — resolved; what was read to say so

Checked against the tree rather than against the closing ticket comment.

* **One policy, one call site.** `unsafe fn next_header_status(archive, archive_path, entry_ptr)`
  in `src/ffi/libarchive_wrapper/reader.rs` is the only place production code calls
  `archive_read_next_header`, and all eight read walks route through it. It maps `ARCHIVE_EOF` to
  `HeaderStatus::Eof`, `ARCHIVE_OK` to `HeaderStatus::Ok`, `ARCHIVE_WARN` to
  `HeaderStatus::Warned(text)` and everything else through `classify_libarchive_error_at`. The only
  other occurrence of the symbol in `src/` is inside `src/ffi/libarchive_wrapper.rs`'s
  `#[cfg(test)] mod tests`, which is test code, not a read walk.
* **The ruling Action 1 asked for was taken explicitly, not inherited.** Accepting `ARCHIVE_WARN` is
  argued in the helper's own rustdoc: libarchive returns `ARCHIVE_WARN` when it *recovered* and the
  header is usable, so refusing it makes this crate stricter than the library it wraps, in a way no
  caller asked for.
* **The discarded half was fixed too, and it is the part a status-only fix would have missed.** The
  text libarchive attaches was previously dropped at every accepting site, so a caller was told the
  archive was fine with no way to learn what the library had objected to. It now returns inside
  `HeaderStatus::Warned`, is wrapped by `libarchive_advisory` as
  `ArchiveWarning::BackendAdvisory { backend, operation, message }` (`src/error.rs`), and drains to
  the facade through `Archive::take_backend_warnings`.
* **Action 4's regression test exists and it guards the shape as well as the behaviour.**
  `tests/libarchive_header_policy_test.rs`: `every_header_read_goes_through_the_single_policy_helper`
  asserts exactly one call site *and* that it sits inside the helper — the divergence class a
  behavioural test cannot catch, because a ninth site added tomorrow would still pass every
  behavioural assertion on today's fixtures — and `no_read_walk_compares_the_warn_status_on_its_own`
  bounds the stray comparisons. Beside them,
  `listing_and_integrity_agree_on_an_archive_libarchive_warns_about` pins the contradiction this
  entry was filed for, and `the_warning_text_reaches_the_caller_instead_of_being_discarded` and
  `a_listing_caller_can_still_reach_what_the_backend_said` pin the advisory route.
* **Action 3 is done as of this update.** `docs/investigation/codebase/ffi-libarchive.md` still
  asserted the old split as current — that `test_integrity` and the stream readers accept
  `ARCHIVE_WARN` while listing and extraction reject it, so "one warning-producing archive can
  therefore pass integrity/streaming and fail ordinary listing or disk extraction". That sentence
  was false against the tree, which made it the one place in this slice where a document
  *contradicted* the code rather than merely lagged it. Both the "FFI Return-Code Discipline" bullet
  and the `OK`/`WARN` contract line in its Key Data Structures section were rewritten to the
  implemented contract on 2026-09-02.

One caveat carried forward rather than hidden: `docs/investigation/` is a **regenerated** tree —
`tests/record_citations_test.rs` excludes it for exactly that reason, calling it "regenerated
snapshots; a hand-fix is churn the next generation undoes", and the page's own frontmatter carries a
`generated:` block. The hand-fix is correct today and the next generation may re-stale it. If it
does, that is a generator-input problem, and this box should be re-opened with a dated reason rather
than the page hand-patched a second time.

### Verification

- [x] All six `archive_read_next_header` sites share one documented policy
  <!-- Ticked 2026-09-02: eight sites, not six - the 2026-08-21 recount stands - and all eight now go through unsafe fn next_header_status in src/ffi/libarchive_wrapper/reader.rs, which is the only production caller of archive_read_next_header in src/. Verified by grepping src/ for the symbol: three hits, one being the extern declaration in src/ffi/libarchive.rs, one being the helper, and one inside src/ffi/libarchive_wrapper.rs's #[cfg(test)] mod tests. The policy is documented in the helper's rustdoc, which states why WARN is accepted rather than leaving it inherited from checked_data_skip. -->
- [x] A WARN-producing fixture yields consistent verdicts across integrity/list/extract/stream
  <!-- Ticked 2026-09-02: tests/libarchive_header_policy_test.rs stages a malformed PAX tar built in the test itself (no committed fixture needed) and asserts listing and integrity agree on it, that the advisory text survives to the caller rather than being discarded, that a listing caller can reach what the backend said, and that a backend with no advisories returns an empty list. Read the test source first-hand for the assertions rather than inferring them from the names. -->
- [x] `ffi-libarchive.md` matches the implemented contract
  <!-- Ticked 2026-09-02, and this is the box that was genuinely outstanding: the page had asserted the OLD split as current, which is a document contradicting the tree rather than lagging it. Both affected passages were rewritten in this pass - the "FFI Return-Code Discipline" bullet and the OK/WARN sentence under Key Data Structures. Caveat recorded in the Update above and repeated here because it is the reason this tick could go stale without anyone touching the policy: docs/investigation/ is a regenerated tree, excluded from tests/record_citations_test.rs on exactly that ground, so a future generation may reintroduce the old sentence. If it does, re-open this box with a dated reason. -->

### Related

- OI-0076-007 (`checked_data_skip` OK/WARN standardisation — the precedent)
- AD 0047 (content-based digest; separate walk, unaffected)

---

## OI-0080-007: 7z backend fidelity gaps — error classification and integrity drain

- **Source:** R0080-0025, R0080-0026 (Review 0080; review archived to the cold store 2026-08-07)
- **Date:** 2026-08-07
- **Decision:** ACCEPT (recovered by the `/indy-review-cleanup` triage; consolidated from two
  findings in the same backend that were never routed)
- **Status:** OPEN

### Problem

Two independent gaps in `src/ffi/sevenz_wrapper.rs`, both leaving 7z stricter-looking but
less truthful than the other backends:

1. **Corruption reclassified as wrong password** (`classify_decode_error`). When
   `entry_encrypted && self.password.is_some()`, every `ArchiveError::Io{operation == "read"}` and
   every `ArchiveError::Corruption` is rewritten to `ArchiveError::password(...)`. The behaviour is
   narrower and softer than at review time — unencrypted entries and write-side I/O keep their
   class, the original error text survives inside the message, and the wording hedges with
   "likely" — but the *typed* variant is unrecoverably `Password`, so a caller matching on the
   error type cannot distinguish damaged media (correct password) from a wrong passphrase.
2. **Integrity ignores solid-stream drain failures** (`test_integrity`). The post-corruption
   recovery drain is `while entry_reader.read(&mut buf).unwrap_or(0) > 0 {}` — the first drain
   error yields 0, silently ends the drain, and the callback returns `Ok(true)`, so the walk
   continues with the solid-stream cursor possibly mid-entry and a later good file can be blamed.

### Impact

Item 1 is genuinely ambiguous at the format level: 7z AES has no authentication tag, so the
backend cannot always know which case it is in. The defect is that the API does not *say* so.
Item 2 makes 7z the outlier of three: the same gate fixed the sibling findings the opposite way in
libarchive (`classify_libarchive_error` on a non-EOF terminal status, R0080-0019) and in UnRAR
(partial-progress error on a failed `RAR_SKIP`, R0080-0020).

### Required Actions

1. Item 1 — choose between an explicitly ambiguous typed shape (e.g. a `PasswordOrCorruption`
   variant, or `Password { ambiguous: true }`) and a documented public statement that `Password`
   on an encrypted 7z entry means "wrong password **or** corrupt payload". Either is acceptable;
   silence is not.
2. Item 2 — decide whether the drain aborts the walk on error. The in-code comment (R0075-0054)
   declines the finding's *reporting* ask ("would require a parallel failed-list the caller cannot
   interpret") but never addresses aborting, which is the cheaper half.
3. Item 2 — first verify whether `sevenz-rust2`'s `for_each_entries` re-syncs the solid cursor
   after a short callback read. If it does, the mis-blame is unreachable and this half closes as
   moot with a comment; if it does not, it is a real mis-attribution bug.

### Verification

- [ ] An encrypted 7z entry with a correct password and corrupt payload is distinguishable from a
      wrong-password failure, by type or by documented contract
- [ ] Solid-stream drain behaviour on error is decided, implemented and commented
- [ ] `for_each_entries` cursor re-sync question answered in the record or the code comment

### Related

- R0080-0019 / R0080-0020 (the sibling findings, fixed the opposite way in libarchive and UnRAR)
- R0080-0030 (narrowed the blast radius: real archive-file I/O errors abort via the stashed
  `operational_error` and never reach the drain, so only corruption-class errors are suppressed)
- `docs/investigation/codebase/ffi-sevenz.md` ("Password Classification (three layers)") —
  a description of the behaviour, not a disposition of it

---

## OI-0080-008: Zstd archives with a leading skippable frame are undetectable

- **Source:** R0080-0075 (Review 0080; review archived to the cold store 2026-08-07)
- **Date:** 2026-08-07
- **Decision:** ACCEPT (recovered by the `/indy-review-cleanup` triage; never routed)
- **Status:** RESOLVED 2026-08-21 (ticgit `75db9f0f`, closed) — Required Action 1 took the
  *skip-the-prefix* branch and deliberately did **not** add `Zst` to the extension-fallback set,
  because content-based detection is the policy and that set is small on purpose.
  `ArchiveFormat::detect_from_bytes` now calls `zstd_frame_offset`, which walks leading skippable
  frames (`0x184D2A50`–`0x184D2A5F`) by their **stored length** — an exact jump, never a scan —
  and returns `None` when the probe window ends inside the prefix or a per-call frame budget is
  exhausted, so a crafted chain of empty skippable frames cannot make detection walk the whole
  buffer and a short buffer falls through to "undetected" rather than being guessed. Action 3 is
  covered: the compound-tar promotion runs on whatever `detect_from_bytes` returned, so a `.tar.zst`
  behind the same prefix reaches `TarZst`.

### Problem

`ArchiveFormat::detect_from_bytes` (`src/format.rs`) matches zstd only on
`magic.starts_with(&[0x28, 0xB5, 0x2F, 0xFD])`, and the comment above it still claims that a
leading skippable frame "would instead be a custom wrapper not produced by mainstream encoders".
RFC 8878 contradicts that: skippable frames (magic `0x184D2A50`–`0x184D2A5F`) are legal anywhere
between frames, first position included.

### Impact

`ArchiveFormat::detect` falls through to the extension fallback, which is gated on
`is_extension_fallback()` = {Tar, Iso, Lzma, TarLzma}. `Zst` is absent from that set, so a
standards-conforming `.zst` with a leading skippable frame ends as
`ArchiveError::format(None, "Unknown archive format")` and **cannot be opened by any path** —
not by extension, not by content. Blast radius is contained (Zst is read-only via libarchive),
which is why this is an open issue rather than a decision record.

### Required Actions

1. Skip leading skippable frames when probing for the zstd magic, or add `Zst` to the
   extension-fallback set — decide which, since they have different failure modes on a truncated
   file.
2. Confirm libarchive's zstd filter actually bids on a skippable-frame prefix before detection
   promises extraction; a detection fix that hands libarchive a file it then refuses is worse than
   the current honest failure.
3. Cover `.tar.zst` with the same prefix in whichever fix lands.

### Verification

- [x] A `.zst` file with a leading skippable frame is **detected** — the *opens* half rests on a
      hand verification, see the note
  <!-- Ticked with a narrowed scope 2026-08-21: detection is pinned first-hand by test_detect_zst_behind_leading_skippable_frame and test_detect_zst_behind_zero_length_and_high_variant_skippable_frames in src/format.rs, plus test_detect_zst_skippable_frame_budget_boundary for the budget edge. The *opens* half is NOT pinned by any lane: no fixture in tests/fixtures/ carries a skippable-frame prefix, so nothing exercises Archive::open → libarchive on such a stream. What stands behind it is a recorded manual verification in the src/format.rs comment ("Verified against the linked libarchive (3.8.9): its zstd read filter bids on the skippable magic and extracts such a stream"), which is exactly what Required Action 2 asked for but is a one-time check against one linked build, not a regression. tests/fixtures/ is outside this pass's file ownership. -->
- [x] The same holds for `.tar.zst`
  <!-- Ticked 2026-08-21 with the same scope as the box above: test_detect_tar_zst_behind_leading_skippable_frame in src/format.rs writes a skippable-prefixed stream to `test_detect.tar.zst` and asserts ArchiveFormat::detect returns TarZst, and asserts the same bytes named `skipped.zst` detect as Zst — so the compound promotion demonstrably runs on the post-prefix verdict. Detection only; no extraction lane. -->
- [x] A truncated/garbage file still fails with a clear error rather than being misdetected
  <!-- Ticked 2026-08-21: test_detect_zst_skippable_prefix_truncated_probe_is_undetected covers a short header, a truncated skippable payload and a prefix-only buffer, and test_detect_zst_skippable_prefix_does_not_widen_the_zst_claim covers a skippable prefix followed by ZIP bytes and by noise — every one asserts `detect_from_bytes` returns Err rather than guessing Zst. Read first-hand in src/format.rs; the suite is green at 38 suites / 1865 passed / 0 failed / 13 ignored (1833 was the pre-change baseline; the
  figure first recorded here was that baseline rather than the observed run). -->

### Related

- README support matrix (Zst is read-only; creation is out of scope here)

---

## OI-0056-010: Test lanes that silently self-disable

- **Source:** R0056-0037, R0056-0064 (Review 0056; review archived to the cold store 2026-08-07)
- **Date:** 2026-08-07
- **Decision:** ACCEPT (recovered by the `/indy-review-cleanup` triage; Review 0056 routed only
  2 of its 130 findings, and neither of these)
- **Status:** RESOLVED 2026-08-21 (ticgit `371828`, closed) — both halves, and wider than the entry
  asked. Required Action 1 took the *library-native* branch wherever a fixture could be built in
  process (`streaming_memory.rs`, `integrity_edge_cases_test.rs`, `stream_crc_test.rs`,
  `digest_duplicate_path.rs`, `link_skip_single_file.rs`, `single_entry_defense_parity.rs`,
  `non_utf8_paths.rs`, `concurrency.rs`) and the *declare-it-explicitly* branch where the external
  tool is irreplaceable — those lanes are `#[ignore]`d with the reason **and** the exact command
  line, on the stated principle that "an `#[ignore]` nobody can run is indistinguishable from a
  deleted test". Required Action 2 was answered by splitting the perf file in two: the controlled
  half runs in the default lane and **asserts** the SC-010 `unified / native <= 1.2` ratio against
  the shared `SC010_MAX_RATIO` constant, and the wall-clock half that shells out to a native
  archiver is `#[ignore]`d with its command, so nothing prints a verdict nobody checks. Vacuity
  floors were added where a lane iterates a fixture set, so an empty glob now fails instead of
  passing. The ignored count rose from 6 to 13, which is the honest direction: those are lanes that
  used to pass while skipping.

### Problem

Two test lanes report success without testing anything:

1. **External `zip` CLI dependency.** `tests/integration/streaming_memory.rs` and
   `tests/integration/performance_baseline.rs` build fixtures via `Command::new("zip")` and
   `eprintln`-skip when the binary is absent, so coverage silently vanishes on hosts without the
   zip CLI — even though `Archive::create` can build ZIP fixtures in-process. (The same pattern in
   `concurrency.rs` was already fixed with library-native fixtures, which is the model.)
2. **Perf baseline never asserts.** `tests/integration/performance_baseline.rs` prints
   `✓ PASS` / `△ ACCEPTABLE` / `✗ NEEDS IMPROVEMENT` for the measured unified-vs-native ratio; the
   only assertions against the stated 1.2 threshold are synthetic-arithmetic tests, so a real
   regression never fails the suite.

### Impact

A green run on a machine without the zip CLI is indistinguishable from a green run with full
coverage, and a genuine performance regression cannot turn the suite red. Both defeat the point of
having the lanes at all. The perf file was revisited later (R0074-0078 quieted its doc prints)
without anyone ruling on gate-or-demote.

### Required Actions

1. Replace CLI fixture creation with library-native creation, or declare the external dependency
   explicitly so a missing tool fails rather than skips.
2. Decide whether the perf baseline asserts in a controlled bench lane (where machine variance is
   manageable) or is demoted out of `tests/` to `benches/`. Either is defensible; printing a
   verdict nobody checks is not.

### Verification

- [x] No test lane silently skips on a missing external binary
  <!-- Ticked 2026-08-21, and the tick is exactly as wide as its wording. Verified by grepping tests/ for `eprintln!` and reading every hit: no lane skips on a missing *binary* any more — the CLI-dependent fixtures are built in process and the irreplaceable lanes are `#[ignore]`d with their command lines. What remains, and is NOT covered by this box, is four silent skips on a missing *fixture* in tests/integration/extraction.rs ("Skipping {}: fixture not found" for test.rar / test.zip / test.7z inside three loops and one single-archive lane). Those are tracked fixtures, so absence means a broken checkout rather than a host difference, but the lane still reports success having tested nothing. tests/ is outside this pass's file ownership; filed here rather than fixed. -->
- [x] The perf threshold either fails the run or no longer claims to be a test
  <!-- Ticked 2026-08-21: read first-hand in tests/integration/performance_baseline.rs. The controlled lane asserts against `SC010_MAX_RATIO = 1.2` and runs by default; the wall-clock lane carries `#[ignore = "wall-clock comparison against a native archiver CLI; run with ..."]` naming its own command; and the file's own comment records that the former `UA_PRINT_PERF_BASELINE` opt-in "only println!ed prose about SC-010 and asserted nothing, so it could not fail in either direction" and has been removed. The commit message for the perf sentinel also warns against re-`#[ignore]`ing it to silence a failure. -->

### Related

- CI absence: the repository still has no CI configuration (OI-0065-001), so nothing catches a
  host-dependent skip today

---

## OI-0001-001: Declared-size streams accept a clean early EOF as success

- **Source:** R0001-0009 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-08-12 (R0001-0011) — `StreamBound::DeclaredSize` is now exact-length
  whenever the preflight listing declares a size: an inner clean end-of-stream below the declaration
  surfaces as `io::ErrorKind::UnexpectedEof` (sticky across retries) instead of an ordinary EOF, while
  over-production keeps DCR-006's `InvalidData`. `Cap(n)` stays ceiling-only, and unknown-size entries
  degrade to a ceiling-only cap at `min(max_file_size, max_total_size)` rather than having a
  declaration invented for them. Mechanism: `HardCapReader::expected` +
  `StreamingExtractor::with_exact_size`, applied at the single `match bound` site in
  `Archive::extract_to_stream_impl`. Closing reference: DCR-006 Amendment 3 (2026-08-12), which also
  settles the backend-cap derivation this issue's sibling deferral (DEF-004 first step) named as open.

### Problem

`HardCapReader` in `src/streaming.rs` probes for over-production only once `seen >= cap`: it reads
one extra byte and turns a non-empty result into `InvalidData`. Under-production has no equivalent
treatment — an inner `Ok(0)` while `seen < cap` is returned to the caller unchanged, as an ordinary
EOF. A truncated entry in a format without a per-entry checksum therefore reads as a short but valid
payload.

DCR-006 settles over-production and is silent on under-production, so this is an unfilled half of
that decision rather than a departure from it.

### Impact

`StreamBound::DeclaredSize` reads as a guarantee that the stream yields exactly what the listing
declared, and it does not. A caller streaming a `.tar.gz` member whose payload was truncated in
transit receives a short buffer and a clean EOF, with nothing to distinguish it from a genuinely
short file.

### Required Actions

1. Decide whether `DeclaredSize` becomes exact-length (`InvalidData` on EOF before the expected byte
   count) or stays ceiling-only with the gap documented. Caller-supplied `Cap(n)` must keep
   ceiling-only semantics either way — a caller-chosen cap is a limit, not an assertion.
2. If exact-length: sequence it after R0001-0008 has landed. Until the authoritative listing size
   feeds `DeclaredSize` instead of the materialised buffer length, an exactness check on native
   backends compares the decoder's output against itself and can never fail.
3. Amend DCR-006 with whichever way it goes, so the next reviewer finds the answer.

### Verification

- [x] `DeclaredSize` semantics stated explicitly in the `StreamBound` rustdoc
- [x] DCR-006 amended (Amendment 3, 2026-08-12)
- [x] Exact-length chosen: a truncated CRC-less entry surfaces an error rather than a short read —
  `tests/stream_bound_test.rs::declared_size_truncated_entry_surfaces_unexpected_eof` builds a plain
  TAR, takes the listing snapshot, replaces the archive with a same-named shorter payload, and pins
  `UnexpectedEof`; `cap_bound_tolerates_a_short_entry` pins that `Cap(n)` stays ceiling-only over the
  same input.

### Related

- DCR-006 (bounded streaming hard cap) — the over-production half; Amendment 3 carries this one
- R0001-0008 (accepted, fixed in this run) — makes the declared size authoritative
- OI-0001-002 (open when written; PARTIALLY LANDED 2026-09-01) — the name-only drift guards are what
  let the shortened archive through; the exact-length stream is now a partial backstop for that
  class, not a replacement for the guards. Since 2026-09-01 a read handle is also bound to the
  archive file's identity, which refuses the *shortened* archive outright (the length moves) — but
  a same-inode same-length in-place rewrite is invisible to that binding, so this exactness bound is
  named in DCR-014 as one of the defences that remain load-bearing

---

## OI-0001-002: Listing-drift guards compare only entry names

- **Source:** R0001-0020 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** PARTIALLY LANDED 2026-09-01 (ticgit `f84b31d1`) — the exposure this entry describes is
  closed by a **different mechanism than the one Required Actions 1–3 ask for**, and the difference
  is deliberate, not a shortfall in execution. Every read backend now binds its handle to the
  archive **file's** identity — `(dev, ino, len)` on Unix, `len` alone off Unix — captured when the
  handle is opened and re-checked at every by-path re-open, refusing with `OperationBlocked`
  carrying "identity changed". There is still **no per-entry metadata fingerprint**, there is not
  going to be one under this entry, and the Verification boxes below stay unticked because of it.
  See the Update of 2026-09-01 for what landed, why the fingerprint was rejected, and what the
  identity binding cannot see. Controlling record:
  `docs/records/DCR-014-read-handle-bound-to-archive-file-identity.md`.

### Update (2026-08-21) — the ticket is closed; the tree does not carry the fix

TicGit `f84b31d1` was closed `resolved` on 2026-08-21 with the comment "The comparison is widened to
the fields that make an entry the same entry, and the rustdoc now states what is compared and what is
deliberately not." **That is not what the tree contains**, so this entry stays OPEN and the closure
should be treated as premature rather than as a reason to stop tracking.

Checked first-hand on 2026-08-21 by reading every guard site rather than the report:

* libarchive bulk walk — compares `listing.get(current_idx).path` against the walked
  `archive_entry_pathname`, normalised, and nothing else.
* libarchive single-entry seeks (all four) and the streaming search — same, through
  `listing_drift_mismatch(index, expected, found)`, whose three parameters are an index and two
  **names**.
* ZIP — `check_listing_drift` compares `zip.name_for_index(target_id)` normalised against
  `validated_path`. No size, CRC, kind or encryption field participates.
* 7z — the visit-order check and both single-entry guards compare the normalised name.
* UnRAR — `extract_file_core` and the bulk walk use the same name-only
  `listing_drift_mismatch` / `listing_drift_extra` pair.

There is also no fingerprint helper anywhere in `src/` (no hit for `fingerprint`, `entry_matches`, or
any comparison of a cached entry's `size` / `crc32` / `entry_type` / encryption flag at a guard
site), which is what Required Action 1 asks for and Required Action 3 asks to be applied at all five
sites. The two commits of 2026-08-21 do not mention this work in either message. Nothing here is
half-landed, so there is nothing to revert — the entry stands as originally written, and its
"Impact if deferred" paragraph is still the accurate statement of exposure.

### Update (2026-09-01) — the read handle is bound to the archive **file**, not to a fingerprint

The note above stays exactly as written: it was true of the tree it was checked against, and
deleting a dated finding to make a later one read cleanly would falsify the record. What follows is
what landed afterwards, under the same ticket.

**What landed.** A shared `FileIdentity` in `src/fs_identity.rs` — the module that already owns the
crate's single audited `MetadataExt` site — with best-effort capture, fail-closed revalidation, and
one shared drift-error constructor. Each read backend records the identity of the file its cached
listing describes and re-checks it at every by-path re-open: libarchive captures in `open` inside a
`stat` / open / `stat` bracket and re-opens through a bound helper at all six read sites plus the
stream reader (`open` itself keeps the raw opener, being the capture point); ZIP captures from an `fstat` of the descriptor every later read goes through; 7z
captures-or-compares inside `open_reader`, with an op label threaded from its six callers; UnRAR
compares in `fresh_handle`, and its pre-existing `UnrarFileIdentity` was migrated onto the shared
type in the same change. **All five name and cardinality guards were kept** — the binding is
additive, and the two failure vocabularies are disjoint by construction: identity drift is
`OperationBlocked` + "identity changed", name and cardinality drift stays `ArchiveError::Format` +
"listing drift".

**What was rejected, and why it is not a shortfall.** Required Actions 1–3 ask for a per-entry
metadata fingerprint over entry type, declared size, CRC and encryption. That approach was rejected
by the owner on the strength of this entry's own Action 2. Reading the tree, the per-backend
normalisations do not agree: a directory's declared size is `None` on ZIP, 7z and UnRAR but
`Some(0)` on libarchive tar; CRC is `None` for the entire tar / ISO / raw family and a placeholder
for AE-2 ZIP. A comparison widened over those fields is the fail-closed-on-healthy-archives outcome
Action 2 warns about, paid on the formats this crate supports best.

**The two approaches are not ordered, and this entry should not be read as saying the fingerprint
was wrong.** A fingerprint *would* catch the same-inode same-length in-place rewrite that a
stat-based identity cannot see. Identity *does* catch every replacement primitive a fingerprint
would have to be perfect to notice — rename-over, `fs::copy`-over (which preserves the destination
inode; verified on the dev host), append, truncate-and-rewrite to a different length — and catches
them at the file, before an entry is read. The choice was made on **false-positive risk**, not on
coverage dominance.

**What the binding does not cover**, stated here rather than left to be rediscovered:

* a same-inode, same-length in-place rewrite is invisible to any stat-based identity; off Unix, so
  is any same-length replacement, because `InodeId` is Unix-only (stable `std` has no analogue —
  `volume_serial_number` / `file_index` are nightly `windows_by_handle`). Timestamps were rejected
  as the substitute: forgeable, and selling forgeable metadata as identity is worse than a
  documented gap. The name and cardinality guards, DCR-006's declared-size exactness bound and
  ZIP/7z/RAR CRC verification remain the defence for that case, which is why none was removed;
* ZIP's window is genuinely **closed** (identity comes from an `fstat` of the descriptor every later
  read uses); libarchive, 7z and UnRAR capture by path `stat` and retain the accepted stat-to-open
  sliver that DCR-007's 2026-07-22 amendment already evaluated and closed off as
  accepted-permanent when it rejected the fd hand-off;
* for a multi-volume RAR set only the **first** volume is bound: continuation volumes are opened
  inside the SDK via the volume-change callback, and there is no per-continuation-volume listing
  snapshot to bind them to. Out of scope by structure, not by oversight.

### Problem

Every backend's drift guard validates a positional index by comparing the normalised path and
nothing else — libarchive's bulk walk and single-entry seek, ZIP's `check_listing_drift`, the 7z
visit-order check, and UnRAR's `extract_file_core`. The cached snapshot each guard compares against
carries entry type, declared size, CRC and encryption status, none of which participates.

An archive rewritten on disk between listing and extraction that keeps entry names while replacing
file types, sizes, checksums or encryption flags passes every guard.

### Impact

OI-0081-001 names the drift cross-checks as the mitigation for the detect-then-reopen class. That
mitigation is weaker than it reads: it proves the *n*-th entry still has the same name, not that it
is the same entry. The safety gate's decisions — ratio, size, entry-kind policy — were made against
metadata that the guard does not re-verify.

### Required Actions

1. Define a stable per-entry metadata fingerprint that every backend can produce identically from
   both a cached `ArchiveEntry` and a live walk position.
2. Establish, per backend, that the live walk normalises each participating field the same way
   `parse_entry` does. This is the hard part: any disagreement turns a legitimate extraction into a
   hard `OperationBlocked`, so the change must fail closed only on genuine drift.
3. Apply the fingerprint at all five guard sites; fail closed on any mismatch.
4. Fixture matrix: same-name payload swap, same-name type change, same-name encryption-flag change,
   and a control that must still succeed on every backend.

### Impact if deferred

Bounded — the existing name check still catches reordering, truncation and growth, which are the
common accidental cases. What remains uncovered is deliberate same-name substitution.

### Verification

- [ ] One fingerprint helper, used by all five guards
  <!-- Unticked 2026-09-01, and deliberately so: the fingerprint approach was rejected, not deferred. No `fingerprint` / `entry_matches` helper exists in src/ and none is planned under this entry. What landed instead is one shared `FileIdentity` in src/fs_identity.rs used by all four read backends. This box cannot be ticked by the change that discharged the exposure, and overwriting it to say otherwise would misreport what is in the tree. -->
- [ ] Per-backend normalisation agreement demonstrated by test, not assumed
  <!-- Unticked 2026-09-01: this is the criterion the rejection turns on, not one the change skipped. The disagreements are real and were read first-hand — directory declared size is None on ZIP/7z/UnRAR and Some(0) on libarchive tar; CRC is None for the tar/ISO/raw family and a placeholder for AE-2 ZIP — so the agreement this box asks for does not hold today and would have to be manufactured per backend before a widened comparison could be trusted. -->
- [ ] No false refusal on the control fixtures across all backends
  <!-- Unticked 2026-09-01 for the fingerprint it was written about. The identity binding has its own equivalent evidence in tests/listing_identity_test.rs (healthy archives read normally; only a swapped, renamed-over or appended file is refused) plus the reworked in-place-rewrite tests in tests/common, but that is not the fixture matrix Action 4 specifies and is not claimed as one. -->

### Related

- OI-0081-001 (RESOLVED) — names these cross-checks as the mitigation; this is a completeness gap in it
- OI-0076-002 (RESOLVED) — the single-entry `ValidatedEntry` gate the guards hang off
- DCR-014 (2026-09-01) — the controlling record for what landed: the file-identity binding, the
  rejected per-entry fingerprint, and the residual each one leaves

---

## OI-0001-003: ZIP duplicate-collapse guard is neither source-atomic nor applied across the API

- **Source:** R0001-0026, R0001-0030 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-09-02 — the record half landed after all, so all three Required Actions
  are met. `docs/records/DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md` carries a
  **second** dated amendment, "Amendment (2026-08-21, OI-0001-003 landed — the scope question is
  closed, and this record predicted the mechanism correctly)", committed in `33712ee`; grepping that
  record for its `## Amendment` headings returns two, not the one the 2026-08-21 check found. The
  amendment is dated the same day as the check that reported it absent, so the two notes disagree
  about a single day; the checkable fact today is that the amendment is present, and the earlier
  note stays as written because it records how that conclusion was reached. The 2026-08-21 status
  text follows verbatim, superseded only in its final clause. *(2026-08-21, as written:)* PARTIALLY
  LANDED 2026-08-21 (ticgit `25285d67`, closed) — both **code** defects are
  closed exactly the way Required Action 1 prescribed, by one index rather than more guard calls;
  the **record** half (Action 3) is not. `ZipArchive` now memoises a single
  `RawCentralDirectory` — every physically stored record with its raw name bytes, the crate-side
  indices each maps to, the deduped length, the ambiguous-name set, and an `any_undetected` flag
  decided by counting rather than by `duplicate_names.is_empty()` (R0001-0028). It is read through
  the descriptor the cached handle already owns (`with_cached_file`), never a second
  `File::open`, which is the atomicity fix. Three gates hang off it: `reject_if_duplicate` for the
  by-name single-entry paths, `reject_if_collapsed` for the whole-view operations (bulk extraction,
  the integrity walk, an id-addressed stream) and `reject_if_unlocalizable` for listings, which is
  deliberately the narrow gate — a localizable ambiguity still lists, so the refusal text's own
  advice ("address the entry by id") stays reachable, and only an unattributable collision fails the
  listing closed.

### Problem

Two defects in the same mechanism, with one fix between them.

1. **Not source-atomic** (R0001-0026). `scan_duplicate_names` takes `central_directory_start()` and
   the deduped length from the cached `zip` handle, then does an independent `File::open(&self.path)`
   and scans by pathname. A replacement at that path between the two opens lets the guard bless one
   file while extraction reads another. The backend records no open-time identity, so there is
   nothing to revalidate against.
2. **Narrower than the public surface** (R0001-0030). `reject_if_duplicate` is called by the by-name
   memory and disk paths only. Listing, `entry_count`, extraction by ID, bulk extraction and the
   integrity walk all iterate `zip.len()` — the `zip` crate's already-collapsed view — so an
   ambiguous archive is refused when asked for an entry by name and silently under-reported when
   listed or extracted wholesale.

### Impact

DCR-009 states the intent as collapse-safe behaviour. As implemented, an archive with colliding
central-directory records presents a complete-looking listing and a complete-looking bulk
extraction, and only the by-name single-entry surface refuses it. A caller who never calls a by-name
method never learns the archive is ambiguous.

### Required Actions

1. Build a raw central-directory index at archive open or first listing — record count, per-record
   name bytes, and the crate index each maps to — and make every operation consult it, rather than
   adding `reject_if_duplicate` calls to more entry points.
2. That index removes the second open, which is also the atomicity fix: the scan and the extractor
   read one source. If a second open remains unavoidable, capture read identity at
   `ZipArchive::open` and revalidate across both.
3. Amend DCR-009 with the resulting scope.

### Verification

- [x] An ambiguous archive is refused (or reported ambiguous) by listing, count, bulk extraction and
      integrity, not only by the by-name paths
  <!-- Ticked 2026-08-21: verified by reading src/ffi/zip_wrapper.rs, not from the closing report. `reject_if_collapsed` covers the whole-view routes and `reject_if_unlocalizable` covers listing and the counts derived from it; both consult the same memoised index as `reject_if_duplicate`. Note the scope the code chose and documents: a *localizable* ambiguity still lists (every read of the ambiguous names is refused instead), so "refused by listing" holds only for the unattributable case — which is the honest reading of "refused OR reported ambiguous". -->
- [x] The duplicate scan and the extraction read the same source
  <!-- Ticked 2026-08-21: `scan_raw_central_directory` reads through `ZipArchive::with_cached_file`, i.e. the descriptor the cached `RawZipArchive` already owns, and performs no `File::open` of its own — read first-hand in src/ffi/zip_wrapper.rs. The second open R0001-0026 named is gone. -->
- [x] DCR-009 amended
  <!-- Unticked 2026-08-21: verified absent. docs/records/DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md carries exactly one amendment, dated 2026-08-12 for R0001-0027/0028/0029, and its closing paragraph still says this residual "is untouched by any of the above and is tracked as OI-0001-003" — a sentence the code has now falsified. The scope Required Action 3 asked for is unrecorded. docs/records/ is outside this pass's file ownership. -->
  <!-- Ticked 2026-09-02: the 2026-08-21 comment above is kept verbatim and the file now contradicts it. docs/records/DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md carries two dated amendment headings, not one: the 2026-08-12 one for R0001-0027/0028/0029, and "## Amendment (2026-08-21, OI-0001-003 landed - the scope question is closed, and this record predicted the mechanism correctly)", committed in 33712ee. Read first-hand by grepping the record for "## Amendment". The two code boxes above were already ticked on 2026-08-21 and were re-checked in the same pass: RawCentralDirectory with its any_undetected flag is in src/ffi/zip_wrapper/raw_directory.rs and reads through ZipArchive::with_cached_file rather than a second File::open, and reject_if_duplicate / reject_if_collapsed / reject_if_unlocalizable in src/ffi/zip_wrapper.rs are wired to list_files, extract_all, validate_integrity, extract_file and extract_to_stream. -->

### Related

- DCR-009 (collapse dual ZIP backend to the single `zip` crate) — the controlling decision
- R0001-0027/0028/0029 (accepted, fixed in this run) — hardened the scan's error handling and its
  duplicate accounting; those fixes make the guard sound where it applies, and do not widen where it
  applies

---

## OI-0001-004: Write-side path validation is host-dependent rather than archive-portable

- **Source:** R0001-0040 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-08-21 (ticgit `9c00264f`, closed) — Required Action 1 was decided the
  *rejecting* way and Action 2's rules landed: `security::validate_archive_internal_path` no longer
  delegates to `Path::components()` at all. The (backslash-normalised) name is split on `/` and each
  segment judged in place, so `.`, `..`, empty segments, NUL and absolute prefixes are refused in
  **every** position on every host, and under the new default `ArchivePathPolicy::Portable` a
  drive-letter prefix, any `:`, a segment ending in `.` or ` `, and the Windows reserved device names
  (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, matched case-insensitively on the stem so
  `nul.txt` is refused and `nullable.rs` is not) are refused too. `validate_archive_internal_path_as`
  takes an explicit policy and `set_archive_path_policy` / `with_archive_path_policy` select
  `ArchivePathPolicy::Host` for callers who deliberately want the writing host's rules — Action 3's
  "warning-and-allow tier" resolved as an explicit opt-out rather than a warning tier. Action 4's
  record is the dated 2026-08-20 amendment on AD 0044; no separate DCR was written. One trailing `/`
  stays legal as the conventional directory spelling. Deliberately still accepted: the rest of the
  Windows-forbidden character set (`< > " | ? *`) and control characters — the portable policy covers
  the classes that silently *rename* or *fail* an entry, and widening it is a separate decision.

### Problem

`validate_archive_internal_path` normalises separators and then delegates to the host's
`Path::components`. On Unix that means `C:`, `CON`, `name:stream` and `trailing.` are ordinary path
components, so an archive created on Linux or macOS can carry entry names that a Windows extractor
must either mangle or refuse: drive-relative prefixes, reserved device names, alternate-data-stream
separators, and components ending in a dot or space.

### Impact

The crate treats all three desktop platforms as first-class, so an archive produced on one and
consumed on another is the normal case, not an edge case. The write path currently encodes the
producer's filesystem rules into a portable container format. AD-0064 governs non-UTF-8 names and
does not reach this.

### Required Actions

1. Decide the policy: reject Windows-hazardous names on every host, or accept them and document that
   archives created on Unix may not extract cleanly on Windows.
2. If rejecting: apply platform-neutral archive-name rules explicitly — no drive-letter or UNC
   prefixes, no reserved device basenames (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`,
   with or without an extension), no `:` in a component, no component ending in `.` or space.
3. Weigh the compatibility cost first: a Unix user archiving a directory that legitimately contains
   `aux.txt` would start getting an error. A warning-and-allow tier may be the right middle.
4. Record the decision and, if the rules land, pair it with a DCR.

### Verification

- [x] Same input name accepted or rejected identically on Unix and Windows
  <!-- Ticked 2026-08-21: verified by reading `archive_internal_path_reason` in src/security.rs, not inferred from the closing report. The Portable arm contains no `cfg` branch and no `Path` call — it splits the normalised string on '/' and applies `portable_segment_reason` per segment — so the verdict cannot vary by host. The one host-sensitive branch is guarded by `policy == ArchivePathPolicy::Host`, which is opt-in and documented as such. Scope of the tick: host-independence is established by construction and by reading; it is NOT established by executing the suite on Windows, which this macOS host cannot do (OI-0065-001 / ticgit 1340e934 owns that gap). -->
- [ ] Decision recorded; DCR written if behaviour changes
  <!-- Deliberately unticked 2026-08-21: half-satisfied, and the unsatisfied half is not this file's to satisfy. The decision IS recorded — docs/records/AD-0044-validate-archive-internal-paths-at-facade-boundary.md carries a dated 2026-08-20 amendment stating the portable-versus-host policy, the experiment that established the root cause, the exact reject list and the two deliberate limits. Behaviour did change (names that used to be written are now refused), and no DCR was written for it. Whether the AD amendment discharges the DCR clause or a DCR is still owed is a record-owner call, and docs/records/ is outside this pass's file ownership, so it is reported rather than decided. -->
- [x] Round-trip test for each hazard class
  <!-- Reworded and ticked 2026-08-21: a *round-trip* test is unreachable by construction — every hazard class is now refused at the write boundary, so no archive carrying one can be created to read back. The verified equivalent is a rejection test per class in src/security.rs: test_validate_archive_internal_path_rejects_curdir, _rejects_traversal, _rejects_empty, _rejects_nul, _rejects_absolute, _rejects_empty_segments, _rejects_reserved_device_names, _rejects_trailing_dot_or_space, _rejects_colon_segments, plus test_validate_archive_internal_path_accepts_trailing_slash pinning the directory carve-out and test_archive_path_policy_portable_and_host_differ pinning that the two policies actually disagree. Read first-hand; the whole suite is green at 38 suites / 1865 passed / 0 failed / 13 ignored (1833 was the pre-change baseline; the
  figure first recorded here was that baseline rather than the observed run). -->

### Related

- AD-0064 (non-UTF-8 path policy) — adjacent, does not cover portable-unsafe names
- OI-0076-001 (OPEN) — non-UTF-8 path fidelity on the write and extract paths
- AD 0044 (2026-08-20 amendment) — the resolving decision; `Path::components()` eating interior and
  trailing `.` segments is named there as the root cause
- AD 0066 — the extraction-side counterpart (`ExtractionLimits::reject_unsafe_paths`) landed in the
  same pass, so the write and read sides now agree about hostile names
- **Residual, reported not resolved:** the host opt-in is a process-wide switch, so two consumers of
  this crate in one process share it. The setter returns the previous value and
  `with_archive_path_policy` restores it, and AD 0019's process-wide UnRAR mutex is the precedent —
  but a per-operation options field would need signature changes on the write path
  (`add_file_from_data` and friends take no options today), which belongs with the OI-0076-005
  encapsulation work that touches those types anyway.
  *Update 2026-09-01 — that hand-off did not happen, so this residual is still live.* OI-0076-005's
  `#[non_exhaustive]` half has now landed on all of its structs, including `ExtractionOptions`, and
  it added **no** per-operation options parameter to the write path: the attribute closes external
  struct-literal construction and nothing else. `add_file_from_data` and friends still take no
  options, and the host opt-in is still the process-wide switch described above. Re-target this
  residual rather than assuming the encapsulation pass absorbed it.

---

## OI-0001-005: `ArchiveEntryBuilder` can construct entries that violate entry-kind invariants

- **Source:** R0001-0043 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** OPEN

### Problem

Every setter on `ArchiveEntryBuilder` is available for every entry kind, and `build` performs no
validation or normalisation. The advertised builder API will therefore hand back a `Directory`
carrying a payload size and a CRC32, or a regular `File` carrying a link target. The one invariant
the builder does enforce is the `permissions & !0o7777 == 0` mask (R0075-0079).

### Impact

The builder was introduced to make well-formed entries easy to construct. Downstream code that
pattern-matches on `entry_type` and then trusts the kind-appropriate fields can be handed a
contradiction by a caller who did nothing unusual.

### Required Actions

1. Choose the shape: kind-specific builders (`file()`, `directory()`, `symlink()` entry points, each
   exposing only its legal setters) or a fallible `build() -> Result<ArchiveEntry>` that enforces
   size / link-target / CRC invariants per kind.
2. Land it with its two siblings rather than alone — this is the same "make the type system carry
   the invariant" work as OI-0076-005 and OI-0081-002, and three separate passes over the same
   public surface is three separate breaking changes.
   <!-- Overtaken in part, 2026-09-01: OI-0076-005's `#[non_exhaustive]` half has now landed on its
   own, in two passes (five structs 2026-08-29, `ExtractionOptions` 2026-09-01), so the "land it
   together" advice can no longer be followed for that axis. The cost this action was trying to
   avoid is not fully incurred, though: `#[non_exhaustive]` closes external construction without
   touching field visibility or adding any invariant, so the *field-demotion* break this item cares
   about is still unspent and can still ride one pass with OI-0081-002. Re-read this action as
   "land the demotion together", not "land the attribute together". -->
3. Public API change: v0.4.

### Update (2026-09-02) — the blocker is dead, most of the work landed additively, one invariant is missing

Three separate things, kept apart because they have different consequences.

**The blocker no longer exists.** Required Action 3 says "Public API change: v0.4", and the
corresponding `docs/backlog.md` entry recorded "the v0.4 breaking-change window" as its blocker.
That window closed: 0.4.0 released 2026-08-17. The 0.5.0 window is open and in use — `CHANGELOG.md`'s
`[Unreleased]` section opens "This will be 0.5.0, and the bump is forced", `Cargo.toml` is still at
0.4.0, and five breaking commits landed into that window on 2026-09-01/02. This item was waiting for
a window that had closed while missing one that is open.

**Required Action 2 can no longer be followed, and by events rather than by argument.** Its
2026-09-01 comment already recorded half of this. The other half: OI-0081-002 / ticgit `165103b8`
closed 2026-08-21 and OI-0076-005 / ticgit `479aa5b8` closed 2026-09-01, both without the builder.
"Land it with its two siblings" is not a plan that can still be executed; the live question is
whether the remainder folds into ticket `c1296744`, which is already scheduled to take a breaking
pass over `src/entry.rs`, or takes a pass of its own — which would be the third separate pass over
this surface, the exact outcome Action 2 was written to prevent.

**Most of Required Action 1 landed additively.** `src/entry.rs` defines `build_checked()` backed by
`entry_kind_violation`, enforcing: no empty path; a `Directory` carries no size, compressed size or
link target; `Symlink` and `HardLink` must carry a link target; a `File` carries none. The
kind-specific entry points the action's first option describes exist too — `file`, `dir_at`,
`symlink_at`, `hardlink_at`, each with a fallible `try_*` form — and `CHANGELOG.md` already
advertises `.build_checked()` as the migration. Five unit tests pin the rejections.

The Verification boxes below stay **unticked**, and the reason is the difference between "rejected"
and "unconstructible". Plain `build()` is still infallible and still unchecked behind roughly 101
call sites, so a directory with a payload size remains constructible by the default path; and
`entry_kind_violation` never inspects `crc32`, so `dir_at(..).crc32(x).build_checked()` succeeds
today — the CRC half of Required Action 1 is genuinely missing, not merely unmigrated. A tick that
overstates is worse than an open box.

### Verification

- [ ] A directory with a payload size is unconstructible or rejected
  <!-- Half-satisfied, and left unticked 2026-09-02 for the half that is missing. Rejected on the checked path: entry_kind_violation in src/entry.rs refuses a Directory carrying size or compressed_size, and build_checked() surfaces it. NOT satisfied on two counts: plain build() is still infallible and unchecked, so the default construction path still admits it; and crc32 is never inspected, so dir_at(..).crc32(x).build_checked() succeeds, which is exactly the "Directory carrying a payload size and a CRC32" case the Problem section names. Read src/entry.rs first-hand. -->
- [ ] A file with a link target is unconstructible or rejected
  <!-- Half-satisfied, and left unticked 2026-09-02 for the same reason as the box above: entry_kind_violation refuses a File carrying a link_target and build_checked() surfaces it, but plain build() does not, and build() is what roughly 101 call sites across src/, tests/ and examples/ use. Whether build() becomes fallible or is kept infallible forever is the open decision, not the rejection logic. -->
- [ ] Existing `ArchiveEntryBuilder` call sites migrated
  <!-- Unticked 2026-09-02: unchanged and not started. The kind-specific entry points and build_checked() were added alongside the existing API rather than replacing it, which is why nothing broke and also why nothing migrated. This box moves with whichever ticket takes the src/entry.rs breaking pass. -->

### Related

- OI-0076-005 (OPEN) — encapsulate public-field structs, v0.4. Its `#[non_exhaustive]` half is
  complete as of 2026-09-01 (`ExtractionOptions` was the last), so what it still shares with this
  entry is the *field-demotion* half, not the attribute.
- OI-0081-002 (OPEN) — typed compression-option builder invariants; ticgit `165103b8`
- R0075-0078 — the builder's introduction

---

## OI-0001-006: Opening an old-style RAR continuation volume cannot discover its set

- **Source:** R0001-0048 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-08-29, recorded here 2026-09-02 (ticgit `61660f`, closed) — absorbed by
  OI-0080-004 exactly as the Required Actions directed, rather than fixed independently.
  `src/format/multipart.rs` (commit `4a791fc`) is the typed parser those actions were waiting for:
  `VolumeScheme::RarOldStyle` is documented as "base.rar, base.r00, base.r01 … rolling into
  base.s00", and `parse_volume_name` derives the base stem from **any** member — there is no
  `.rar`-only source predicate left anywhere, which is the asymmetry this entry described. Both
  carried-across criteria are met; see the Update below for the test that pins the second one
  literally.

### Problem

In `detect_multipart`, old-style `.rNN` / `.sNN` sibling matching is gated on `src_is_rar`, which is
`file_name_lc.ends_with(".rar")`. Opening `archive.rar` therefore discovers the whole set, while
opening `archive.r00` — a legitimate member of the same set, and the one a user is most likely to
click when the main volume is elsewhere — reports no multipart relationship at all.

Multipart reporting consequently depends on which volume the caller happened to open.

### Impact

Low severity, real asymmetry. The caller gets a materially different answer for the same archive set
depending on entry point, with nothing in the API to indicate why.

### Required Actions

Fold into OI-0080-004 as an explicit acceptance criterion rather than patching the current
string-matching code. That issue replaces this matching wholesale with a typed `VolumeSet` /
`VolumeScheme` parser, so a fix landed here now is work that parser deletes. The criterion to carry
across:

1. A source whose name matches `<stem>.rNN` or `<stem>.sNN` derives the base stem, and the resulting
   set includes the main `<stem>.rar` volume plus the whole continuation series.
2. The reported set is identical whichever member of the set is opened.

### Update (2026-09-02) — resolved, and the acceptance criterion is tested literally

The reason this is recorded as resolved rather than as "the parser landed, so presumably this works"
is that the second criterion — "the reported set is identical whichever member of the set is opened"
— has a test written as that sentence rather than as a proxy for it.
`src/inspection/tests.rs::test_detect_multipart_old_style_set_is_the_same_from_every_member` stages
`archive.rar`, `archive.r00` and `archive.r01` as byte-copies of `tests/fixtures/test_rar5.rar`,
then asserts the identical `MultipartLayout::Multi { parts: [main, r00, r01] }` from every one of
the three entry points. Grepping `src/inspection.rs` for the old matcher's vocabulary —
`parse_rar_part_suffix`, `MAX_VOL_DIGITS`, `is_ascii_digit`, `strip_suffix` — returns nothing, so
the string-matching code the Required Actions warned against patching is gone rather than merely
bypassed, and the crate holds one answer to "is this a volume name".

Note what this does **not** resolve, since OI-0080-004 remains open for narrower reasons:
`MultipartLayout` still cannot report a *defective* set (a hole, a duplicate number, a foreign
sibling) even though `parse_volume_set` computes that information, and the 7z `.001` routing gate is
untouched. Neither is this entry's subject.

### Verification

- [x] Opening `.rar`, `.r00` and `.r01` of one set yields the same `MultipartLayout`
  <!-- Ticked 2026-09-02: test_detect_multipart_old_style_set_is_the_same_from_every_member in src/inspection/tests.rs, read first-hand rather than taken from ticgit 61660f's closure. It stages the three names as byte-copies of tests/fixtures/test_rar5.rar and asserts the same MultipartLayout::Multi { parts: [main, r00, r01] } from each entry point, which is this criterion stated as an assertion. Landed 2026-08-29 in the OI-0080-004 parser work (4a791fc); this box went unticked for four days only because nobody re-filed the entry. -->

### Related

- OI-0080-004 (PARTIALLY LANDED) — typed multipart volume parser; ticgit `513f99fc`. **This item was
  absorbed there, not resolved independently**, which is what the Required Actions asked for. What
  keeps OI-0080-004 itself open is narrower and is not this entry's subject: the `MultipartLayout`
  shape cannot report a defective volume set, and the 7z `.001` routing gate is unchanged.

---

## OI-0001-007: Exact content total silently treats unknown entry sizes as zero

- **Source:** R0001-0049 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** OPEN

### Problem

The content-total walk in `src/inspection.rs` adds only `Some(size)` and returns a plain `u64`. An
entry whose size the format never declared contributes zero and leaves no trace, so an advertised
exact total can be an arbitrary underestimate with nothing in the return value to say so.

### Impact

Deduplication and reporting code that compares two totals as exact quantities can conclude two
archives hold the same bytes when one of them has unknown-size members.

Note the interaction with R0001-0014, fixed in this run: libarchive previously reported unknown
sizes as `Some(0)`, so those entries were already contributing zero — but *invisibly*, disguised as
genuinely empty files. After that fix they are honestly `None`, which is better internally and
equally invisible at this API.

### Required Actions

1. Return `Option<u64>` (None when any file size was unknown) or a `(total, complete: bool)` pair —
   or decode unknown-size entries to determine the true total, at a cost that must be measured
   before being chosen.
2. Public signature change: schedule with the v0.4 API pass rather than alone.
3. Audit callers of the existing total for the same assumption.

### Update (2026-09-02) — Required Action 2's schedule is gone; the defect is unchanged

Required Action 2 says "schedule with the v0.4 API pass rather than alone", and the corresponding
`docs/backlog.md` entry recorded "the v0.4 breaking-change window" as its blocker. That window
closed with the 0.4.0 release of 2026-08-17. The window that is open is 0.5.0 — `CHANGELOG.md`'s
`[Unreleased]` section opens "This will be 0.5.0, and the bump is forced", `Cargo.toml` is still at
0.4.0, and five breaking commits landed into it on 2026-09-01/02. Read Action 2 as "schedule with
the 0.5.0 API pass"; the reason it was written — do not spend a signature break alone — is
unaffected, only the release it names.

The defect itself is unchanged and live, re-verified in the tree rather than assumed:
`content_multiset_digest_and_size` in `src/inspection.rs` accumulates inside `if let Some(size)` and
returns a plain `Result<(String, u64)>`, so an entry whose size the format never declared
contributes zero with nothing in the return to say so. It is reached from the public
`calculate_content_multiset_digest_and_size` and its `calculate_manifest_summary` shim, across 18
real call sites in `src/`, `tests/` and `examples/`.

What is now owed is the return-shape choice Required Action 1 lists, not a wait: `Option<u64>`,
a `(total, complete: bool)` pair, or a typed newtype in the style the crate has otherwise used for
this "is this number trustworthy" question. Recorded so the next reader does not re-derive the
blocker from a release that has already shipped.

### Verification

- [ ] An archive with an unknown-size member cannot present a total that reads as exact
- [ ] `calculate_content_multiset_digest_and_size`'s rustdoc states what the size term guarantees

### Related

- R0001-0014 (accepted, fixed in this run) — makes unknown sizes honest at the source
- OI-0001-008 — the digest half of the same API

---

## OI-0001-008: The content-multiset digest depends on duplicate-path grouping

- **Source:** R0001-0050 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-08-17 — Required Action 1's *first* branch was taken and the prior
  decision overturned explicitly. `content_digest_element` in `src/inspection.rs` is now
  `fn(crc32: u32) -> String` returning `format!("{crc32:08x}")`; the `path_occurrences` map and its
  ordinal suffix are deleted, and multiplicity is carried by repetition in the element vector, so
  `n` copies of one payload still contribute `n` elements. Two archives holding the same content
  multiset now digest identically whether the duplicates share one path or occupy distinct ones.
  Recorded as **DCR-012**
  (`docs/records/DCR-012-content-digest-drops-per-path-occurrence-ordinal.md`), which cites ticgit
  `2a6e3153` by name and overturns its *encoding* clause only — the id-keyed resolution that ticket
  built stands, is reaffirmed, and is now the sole mechanism that distinguishes shadowed duplicates.
  Unique-path digests are byte-identical across the change (ordinal 0 already emitted the bare hex);
  duplicate-path digests move once, carried as a BREAKING 0.4.0 changelog entry. TicGit `61426550`
  was closed `resolved` on 2026-08-17 once the full gate came back green — the sentence that stood
  here saying it was "not yet closed" was true when written and is superseded (ticgit 65d90b56).
  See the update note for what this resolution does and does not cover.

### Update (2026-08-17)

DCR-012 (2026-08-16, landed uncommitted in this tree) overtook the Problem statement and Required
Actions below, which are retained as written for history:

- The Problem paragraph is **false in the present tense**. The code no longer counts occurrences
  per `entry.path` and no longer appends a per-path ordinal; it describes the tree as Review 0001
  found it on 2026-08-09.
- **Required Action 1** — the choice is made: count nothing, encode one bare 8-hex element per file
  entry. DCR-012's property table records that the old encoding failed three contract properties,
  not one: grouped-vs-spread duplicates (the finding above), *and* re-listing one and the same
  archive in a different order, which the OI did not name.
- **Required Action 2** — honoured as a breaking change, not a silent correction: DCR-012's
  *Observable Behaviour Change* section bounds the affected population (only archives with two or
  more `EntryType::File` entries sharing one path) and routes it to a 0.4.0 minor bump with a
  BREAKING changelog entry. A digest schema marker was considered and deliberately rejected.
- **Required Action 3** — reconciled *with* AD-0047 rather than against it: AD-0047 is the record
  that says identity is content-based, and dropping the ordinal is what makes the implementation
  agree with it. `docs/API_REFERENCE.md`'s pre-existing Contract paragraph ("regardless of
  filenames, directory layout, modification times, permissions, or entry order") and its Algorithm
  step 2 ("Convert each CRC32 to 8-char hex") already described the encoding installed here, so
  that document needed no edit to become true.

Not covered by this resolution, and not created by it — recorded here because it lands on the same
API and a reader of this entry will meet it:
`calculate_content_multiset_digest_and_size` on an **AES-encrypted ZIP** now requires the archive's
password. The sibling ticgit `04ba4897` fix (also uncommitted in this tree) makes AE-2 entries list
`crc32 = None` instead of the placeholder `Some(0)`, so the digest walk streams those payloads —
which decrypts them, and which routes through `ReadBackend::extract_to_stream_by_listing_id`.

**Correction (2026-08-17, later pass — supersedes the wording it replaces).** An earlier
revision of this note, written the same day, stated that
`ZipArchive::extract_to_stream_by_listing_id` was "not wired into the `ReadBackend` impl for
`ZipArchive` in `src/backend.rs`", cited a `cargo check` `never used` warning for it, and concluded
that an AE-2 ZIP holding `sub/a.txt` and `sub\a.txt` returned `OperationBlocked` — the OI-0076-002
defect through a new door — so that **"a new ticket is owed for the `src/backend.rs` wiring"**. All
of that is **false against this tree, and no such ticket is owed.** The note was written
concurrently with the change that wired the forwarder, and so recorded a state that had already
stopped being true. Re-verified first-hand here, by running the commands rather than reading a
report:

- `impl ReadBackend for ZipArchive` in `src/backend.rs` carries the
  `extract_to_stream_by_listing_id` forwarder, delegating to the inherent method, under a rustdoc
  block that names DCR-012 and the OI-0076-002 aliasing the forward exists to prevent.
- `cargo check --all-features --lib` exits 0 and emits **no** `never used` warning of any kind; the
  only warnings in its output come from the vendored UnRAR C++ sources.
- `tests/integration/digest_ae2_duplicate_path.rs` builds exactly the archive the falsified
  paragraph described — an AE-2 AES ZIP whose two records are written `sub/a.txt` and `sub\a.txt`
  and normalize to one listing path — and asserts *through the public facade* that the digest
  returns `Ok` and that both payloads are hashed distinctly. It is registered in
  `tests/integration/mod.rs`.

What genuinely survives from that paragraph is the other, smaller half. The password precondition is
no longer undocumented — `calculate_content_multiset_digest_and_size`'s rustdoc carries a *Password
required for AE-2 AES ZIP entries* section stating that the call needs a handle opened with
`Archive::open_encrypted` and that the previous `Ok` was wrong rather than merely cheaper. **The
residual is which error a password-less handle gets:** the ZIP no-password read path still returns
`ArchiveError::Format` carrying a "Password required to decrypt file" message rather than
`ArchiveError::Password`. That mislabel predates the AE-2 fix, is tracked as ticgit `9bdf2c`, and is
named in the same rustdoc, which tells callers not to pattern-match on `Format` to detect a missing
password. It is the one thing here still owed to a ticket. Register coverage: re-checked 2026-08-17,
`04ba4897` still has no OI entry of its own here or in `open-issues-resolved.md` — the only *other*
AE-2 mention in either file is OI-0081-006, about *creating* encrypted archives — and its
`docs/backlog.md` entry was discharged the same day. This note is therefore the register's carrier
for the AE-2 consequences, and `9bdf2c` is the ticket holding the one live residual. `61426550` and
`04ba4897` both remain open.

### Problem

`calculate_content_multiset_digest_and_size` counts occurrences per `entry.path` and appends that
per-path ordinal to each digest element. Two archives holding the same multiset of file contents
therefore digest differently when identical files share one path in the first archive and occupy
distinct paths in the second — although the API documents paths and layout as ignored, and AD-0047
establishes content-based identity.

### Impact

The digest is presented as a layout-independent content fingerprint and is not one. A caller using
it to decide "these two archives hold the same files" gets a false negative for a repacking that
changed only where duplicates live.

### Prior decision — read before acting

TicGit `2a6e3153`, closed `resolved` 2026-07-19: *"Restore manifest digests for duplicate-path
CRC-less archives (id-based digest streaming)"*. Its stated goal included *"The R0079-0028 per-path
occurrence ordinal in the digest encoding then reflects genuinely distinct payloads"* — so keying
the ordinal on path was a deliberate choice, reaffirmed at closure, not an oversight.

That closure solved a different problem: before it, duplicate-path CRC-less entries could not be
digested at all, and the fix made each occurrence hash its own payload. This finding is about the
*keying* of the ordinal, which the closure did not examine. Whoever picks this up must treat the
prior decision as deliberate and overturn it explicitly, or close this item as
working-as-intended and fix the rustdoc instead.

### Required Actions

1. Choose: count occurrences by content digest (making the digest genuinely layout-independent, and
   changing every emitted digest value), or keep path-keyed ordinals and amend the API contract to
   state that duplicate-path grouping participates.
2. If changing: this invalidates any stored digest. Treat it as a breaking change with a version
   note, not a silent correction.
3. Reconcile with AD-0047 either way — it is the record that says identity is content-based.

### Verification

- [x] Documented contract and implemented behaviour agree — for the digest **encoding**, which is
  what this issue is about: `docs/API_REFERENCE.md`'s Contract paragraph and Algorithm step 2 were
  already written for a bare-hex element and needed no edit, and the rustdoc on
  `Archive::calculate_content_multiset_digest_and_size` now states the layout-independence
  positively plus a `# Changed in 0.4.0 (DCR-012)` section.
  <!-- Ticked 2026-08-17: verified by reading both texts against `content_digest_element` and `content_multiset_digest_and_size` in src/inspection.rs. Scope of the tick is the encoding only. Corrected later the same day: this comment previously added that the method's *availability* contract was broken — that a duplicate-after-normalization AE-2 ZIP returned OperationBlocked because `ZipArchive::extract_to_stream_by_listing_id` was unwired in src/backend.rs. That was written against a state the tree had already left; the forwarder is present in `impl ReadBackend for ZipArchive`, `cargo check --all-features --lib` exits 0 with no `never used` warning, and tests/integration/digest_ae2_duplicate_path.rs pins the `sub/a.txt` + `sub\a.txt` AE-2 case as Ok through the facade. What remains true is narrower: on AES ZIPs the call now needs a password (documented in the rustdoc), and the missing-password error is still classified Format rather than Password — ticgit 9bdf2c, not this issue's. -->
- [x] The chosen direction is recorded, citing `2a6e3153`
  <!-- Ticked 2026-08-17: DCR-012 names the ticket in its `Overturns:` header, quotes its goal clause verbatim, and separates what is overturned (the encoding) from what stands (id-keyed resolution). Read first-hand, not inferred from index.yaml. -->
- [x] A **listing** pair differing only in duplicate-path grouping digests identically —
  `test_content_digest_grouped_and_spread_duplicates_agree` in `src/inspection.rs`, with
  `test_duplicate_path_digest_equals_bare_hex_join` pinning the values by literal (`247f72d4`,
  `e212a99e`) and asserting the pre-DCR-012 `ee663b6e` cannot return.
  <!-- Reworded and ticked 2026-08-17: the original criterion said "fixture pair", and no pair of on-disk archive fixtures exists for this case. The verified coverage is a synthetic-listing pair driven through the same private helper the public method calls, plus golden values. Ran `TMPDIR=/Volumes/Temp/claude cargo test --all-features --lib content_digest -- --test-threads=4`: 7 passed, 0 failed, CARGO_EXIT=0. The archive-level sentinel that remains is `duplicate_path_tar_digests_both_payloads_distinctly` (tests/integration/digest_duplicate_path.rs), which pins the opposite direction — distinct payloads must still differ — and was not run in this pass. -->

### Related

- AD-0047 — content-based identity
- DCR-012 — the resolving record (2026-08-16); overturns `2a6e3153`'s encoding clause only
- ticgit `2a6e3153` (closed: resolved) — the prior, deliberate design
- ticgit `61426550` — the implementation ticket for this resolution; **still open**
- R0079-0028 — the per-path ordinal's introduction
- OI-0001-009 (RESOLVED 2026-08-17) — the single-traversal digest rewrite; DCR-012 sequenced it
  after this change and it has since landed
- ticgit `04ba4897` (open) — the AE-2 listing fix; its `src/backend.rs` forward is wired and tested,
  and the one residual it leaves (`Format` instead of `Password`) is stated in the update note above

---

## OI-0001-009: CRC-less compressed-TAR digests recompute from the archive start per entry

- **Source:** R0001-0051 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-08-17 — Required Action 1 landed as ticgit `82bf8fd4` (uncommitted in
  this tree). `Archive::calculate_content_multiset_digest_and_size` in `src/inspection.rs` now calls
  `resolve_crc32_single_pass` before its aggregation loop: every `EntryType::File` entry whose
  listing carries no CRC becomes a `PayloadTarget`, and the whole vector goes to
  `ReadBackend::visit_payloads_by_listing_id`, a new trait method in `src/backend.rs`. The
  libarchive backend — the only one whose CRC-less entries run to the thousands — overrides it with
  a shared-handle single-pass walk, reading each target through `BorrowedEntryReader` in
  `src/ffi/libarchive_wrapper/reader.rs` under the same `HardCapReader::exact` / `::ceiling` bound
  (`src/streaming.rs`) the per-entry route uses, so DCR-011's truncation verdict is unchanged. The
  trait default still returns `NotImplemented` and the per-entry `entry_crc32_for_digest` resolver
  stays as the fallback, so a backend without a one-pass walk (ZIP, whose CRC-less set is the
  random-access AE-2 entries) produces the identical digest by the old route. See the update note
  for what is verified, what is not, and the one residual left behind.

### Update (2026-08-17)

Three things a reader of this entry should have in front of them.

**Why the resolution is keyed on ids.** `resolve_crc32_single_pass` maps `id → crc32`, never
`path → crc32`, and bails out of the one-pass route entirely if two targets ever share an id rather
than letting one map slot absorb two payloads. That is not defensive decoration: DCR-012 removed the
per-path occurrence ordinal from the digest encoding, which makes per-occurrence payload resolution
the *sole* remaining mechanism that distinguishes shadowed duplicates. A path-keyed one-pass map
would re-hash the first occurrence of every repeated path and silently reinstate the OI-0076-002
aliasing that `2a6e3153` removed.

**Ordering of errors changed, deliberately.** Payload faults (a truncated CRC-less member) now
surface before the aggregation loop's `total_size` overflow check rather than interleaved with it.
Both remain errors and both still name their cause; only which one an archive tripping both reports
first has moved.

**A correction to this register's own sibling entries.** Two same-day notes — the residual recorded
under OI-0001-008's closing item in `docs/backlog.md`, and this entry's own `- **Status:** OPEN` —
asserted that this rewrite "did not land" and that "nothing of it is half-landed". That was written
concurrently with the pass that landed it, against files the writing agent could not see change.
The four files named in the Status line above were each read first-hand in this pass before the
status was moved.

### Problem (as found on 2026-08-09; superseded by the Status above)

For formats whose listings expose no per-entry CRC, the digest walk obtains each missing checksum by
routing that entry through a new libarchive stream positioned by a fresh sequential walk. On a
compressed TAR this decompresses from the beginning once per entry, so a summary call is quadratic
in archive size. The `src/inspection.rs` rustdoc already acknowledges the O(n²) behaviour.

### Impact (as found on 2026-08-09; the quadratic term is gone, the linear one is not)

A `calculate_manifest_summary` or content-digest call on a large `.tar.gz` can consume CPU and I/O
wildly out of proportion to what the caller asked for, with no limit and no progress signal. Unlike
the other items in this batch this is a cost defect, not a correctness one — but it is unbounded,
and no open issue or accepted deferral currently disposes of it.

Still true after the resolution, in reduced form: the call reads every CRC-less payload once, so it
remains linear in the archive's decompressed bytes, still unbounded, and still reports no progress.
What the resolution removed is the per-entry re-decompression that made it quadratic. `# Performance`
on the public methods now says so; a bound or a progress signal was never part of this issue's
Required Actions and is not claimed here.

### Required Actions

1. Compute every missing CRC in a single archive traversal and aggregate afterwards, instead of one
   traversal per entry.
2. Reconcile with the id-based streaming path that ticgit `2a6e3153` landed
   (`extract_to_stream_by_listing_id`, per-backend id-seek): a single-traversal digest walk changes
   how that path is used, and the duplicate-path regression test it added must keep passing.
3. Bound or report the work: even single-pass, a full decompression is expensive enough that the
   rustdoc should say so.

### Verification

- [x] One traversal per digest call on CRC-less formats, demonstrated by instrumentation or timing
  <!-- Ticked 2026-08-17 on two independent grounds, both checked first-hand. (1) Timing: `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test integration_tests manifest_digest_does_not_reopen_archive_per_entry -- --ignored --nocapture --test-threads=4` runs the R0075-0084 sentinel in tests/integration/manifest_digest_perf.rs — 1 passed, 0 failed, CARGO_EXIT=0. That test builds a 1000-entry .tar.gz and asserts the digest costs under 50x a bare `list_files()`; its own module doc records the pre-fix ratio as ~150-200x and says the assertion "will FAIL until the streaming-walk refactor lands". It now passes, so the bound the sentinel was written to catch is no longer tripped. (2) Structure: `resolve_crc32_single_pass` issues exactly one `visit_payloads_by_listing_id` call for the whole target vector, and the libarchive override services every target from one handle via `BorrowedEntryReader` — there is no per-entry re-open left on that route to time. PENDING, and outside this pass's file ownership: the sentinel is still `#[ignore]`d and its module doc still describes the per-entry re-open as the current state and the assertion as expected to fail. Until its owner drops the `#[ignore]` and rewrites that header, the timing evidence has to be produced by hand with `--ignored`, as above, and CI does not guard the bound. CORRECTION (2026-08-17, later pass): that PENDING clause is false and is withdrawn. It was written concurrently with the pass that lifted the `#[ignore]` and rewrote the header, so it recorded a state the tree had already left. Re-verified by running the command: `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test integration_tests -- manifest_digest_perf --test-threads=4 --nocapture` — note NO `--ignored` — exits 0 with 1 passed, printing `list_files 1.352166ms, manifest_digest 3.245833ms over 1000 entries (ratio 2.4x, bound 50x)`. The sentinel therefore runs in the DEFAULT lane and the timing evidence no longer has to be produced by hand. Its module doc now opens with a "Shipped behaviour" paragraph describing the single traversal, states "This test runs in the default lane", records the pre-fix ~150-200x ratio as history, and warns against re-`#[ignore]`ing it. What survives of the clause is only the CI half, and for a reason that has nothing to do with this sentinel: the repository has no CI configuration at all (`1340e934`). -->
- [x] The duplicate-member tar regression from `2a6e3153` still passes
  <!-- Ticked 2026-08-17: `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test integration_tests duplicate_path -- --test-threads=4` — 4 passed, 0 failed, CARGO_EXIT=0. The named regression is `integration::digest_duplicate_path::duplicate_path_tar_digests_both_payloads_distinctly`, which builds a `tar -cf` + `tar -rf` duplicate member with distinct payloads and a control where both occurrences are identical, and asserts the two digests differ. The other three are the AE-2 ZIP lane in digest_ae2_duplicate_path.rs, which covers the ZIP side of the same id-keyed contract. -->
- [x] Cost documented on the public methods
  <!-- NOT ticked 2026-08-17 — two of three, and the gap is named rather than rounded up. `calculate_content_multiset_digest_and_size` and `calculate_manifest_digest` each carry a `# Performance` section stating that the libarchive backend now resolves CRC-less entries in a single traversal, that a compressed TAR is decompressed once rather than once per member, and that every payload is still read so `calculate_archive_crc` remains the cheap option. `calculate_manifest_summary` — the third public entry point, a thin shim over the helper — inherits the *Password* and *Truncated entries* notes by explicit reference but has no `# Performance` heading and no pointer to one, so a caller reading only the shim learns nothing about cost. That is Required Action 3's remainder. src/inspection.rs was not in this pass's file ownership; reported to the parent rather than edited. TICKED 2026-08-17 (ticgit 5c597517): `calculate_manifest_summary` now carries its own `# Performance` section, stating that the call is not metadata-only, that CRC-less formats decompress every payload, that OI-0001-009 made that a single traversal so a compressed TAR is decompressed once rather than once per member, that it is still a full read, and pointing at `calculate_archive_crc` for the cheap path. All three public entry points now document cost. -->

Additional verification performed for the resolution, beyond the criteria above:

- [x] The one-pass route and the per-entry fallback produce the same digest
  <!-- Verified 2026-08-17 by reading the code rather than by a differential test, and stated as such: `calculate_content_multiset_digest_and_size` consults `resolved.get(&entry.id)` first and calls `entry_crc32_for_digest` for anything the pass did not cover, and both routes compute the CRC through the same `crc32_of_bounded_payload` helper with the same declared-size bound — so DCR-011's truncation verdict and diagnostics are shared, not reimplemented. No test pins the equality of the two routes on one archive; a backend that gained a buggy one-pass walk would not be caught by the suite. -->
- [x] The resolution does not reintroduce the OI-0076-002 aliasing
  <!-- Verified 2026-08-17: `resolve_crc32_single_pass` keys its map on `e.id`, collects the target ids into a `HashSet`, and returns an empty map — abandoning the one-pass route entirely — if the set is smaller than the target vector, rather than letting one slot absorb two payloads. Exercised end to end by `duplicate_path_tar_digests_both_payloads_distinctly` and by `ae2_duplicate_path_digest_tracks_the_shadowed_payload`, both passing in the run recorded above. -->
- [x] `cargo check --all-features --lib` is clean of dead-code warnings for the new surface
  <!-- Verified 2026-08-17: exit 0, and no `never used` warning of any kind. The only warnings in its output are `-Wnontrivial-memcall` from the vendored UnRAR C++ sources, which are not rustc diagnostics. Recorded because the falsified claim this pass corrects rested on exactly this command reporting the opposite. -->

### Related

- ticgit `2a6e3153` (closed: resolved) — built the id-based streaming this rewrite must not break
- ticgit `82bf8fd4` — the implementation ticket for this resolution; **still open**
- MADR-0001 — listing stays metadata-only, which is why the CRCs must be computed on demand at all
- DCR-011 — the exact/ceiling payload bound the one-pass walk reuses unchanged
- DCR-012 — dropped the per-path ordinal, which is why this rewrite had to stay id-keyed
- OI-0001-008 (RESOLVED 2026-08-17) — the encoding half of the same API

---

## OI-0001-010: Recursive creation drops metadata for nonempty directories

- **Source:** R0001-0074 (Review 0001)
- **Date:** 2026-08-09
- **Decision:** ACCEPT (user-routed `track`, Phase 2)
- **Status:** RESOLVED 2026-09-02 — the ZIP half landed, so both writers now emit directory
  metadata, and Required Action 3's record exists. `src/ffi/zip_writer.rs` defines
  `add_directory_entry_with_metadata(archive_path, mtime, mode)`, applying
  `FileOptions::last_modified_time` from `system_time_to_zip_datetime` and, under `#[cfg(unix)]`,
  `unix_permissions(mode)`; the metadata-free `add_directory_entry` now delegates to it with
  `None, None`, and `src/creation/manifest.rs`'s `write_into` passes `entry.mtime` and `entry.mode`
  on the ZIP arm exactly as it does on the libarchive arm. `DCR-013` is written and registered. The
  2026-08-21 status text follows verbatim, superseded in the clause that says otherwise.
  *(2026-08-21, as written:)* PARTIALLY LANDED 2026-08-21 (ticgit `9cc3d714`, closed) — the
  **libarchive** half is done and the **ZIP** half is not. Required Action 1 landed by way of the single-walk manifest
  (OI-0080-005): `Archive::add_directory_recursive` now emits every recorded directory from the
  manifest, in walk order, carrying the mtime and unix mode the one walk observed, so a non-empty
  directory contributes an entry instead of vanishing and the namespace reservation still fires
  exactly once per entry. The empty-root fallback is gone with it — the root directory is emitted
  from the manifest like any other. **What is not done:** `ZipWriter` has no metadata-carrying
  directory emit (no counterpart to `add_file_from_path`'s `last_modified_time` /
  `unix_permissions` handling), so ZIP directory entries still land with default `FileOptions` —
  present, but mode-less and mtime-less. Actions 3 (DCR + affected creation record) and the Action-4
  release-timing decision are also outstanding; the change did ship inside the 0.4.0 breaking
  window, which is the answer Action 4 asked for in practice if not on the record.

### Problem

Neither writer emits directory entries for nonempty directories during recursive creation, so parent
directory permissions and timestamps are absent from the archive entirely. The ZIP writer emits leaf
directories but without source metadata, and the empty-root fallback uses backend defaults. The net
effect is that a directory tree does not round-trip: only some directories survive as entries, and
none of them carry the source's mode or times.

### Impact

An archive created from a tree and extracted elsewhere reconstructs the files faithfully and the
directory metadata not at all. The accepted directory-metadata caveat on record covers *modification*
mode; general archive creation has no covering decision.

### Required Actions

1. Emit every directory once, before its children, with source metadata, while keeping the existing
   duplicate-namespace suppression so a directory is never written twice.
2. Weigh the compatibility cost first: this changes the entry count and byte content of every
   archive the crate creates recursively, which will move existing test expectations and invalidate
   any consumer's stored archive checksum.
3. Pair with a DCR and an update to the affected creation record, since this changes what created
   archives contain.
4. Decide whether it lands in v0.4 with the other output-affecting changes rather than in a patch
   release.

### Verification

- [x] A created tree round-trips directory modes and times on both writers
  <!-- Deliberately unticked 2026-08-21: load-bearing, not bookkeeping — one writer, not both. Verified first-hand by reading src/creation.rs: test_add_directory_recursive_preserves_nonempty_directory_metadata builds a TAR (libarchive) and asserts the non-empty directory's mtime within a one-second tolerance and its 0o750 mode on Unix, while the sibling lane test_add_directory_recursive_emits_nonleaf_zip_directory asserts only that ZIP *emits* the directory entry, and its own doc comment states why: "ZIP is still metadata-less for directories: ZipWriter has no add_directory_entry_with_metadata counterpart ... so its directory entries land with the default FileOptions". src/ffi/zip_writer.rs is outside this pass's file ownership, so the gap is reported rather than closed. The closing report on ticgit 9cc3d714 reads as if both writers were covered; against the tree it is the libarchive writer only. -->
  <!-- Ticked 2026-09-02: the 2026-08-21 comment above is kept verbatim and no longer describes the tree. It is now both writers, and the test is one shared assertion driven twice rather than two lanes asserting different things. src/creation.rs defines assert_nonempty_directory_metadata_survives(format, extension, mtime_tolerance_secs), which builds a tree with a non-empty directory at 0o750, creates the archive through the public facade, reopens it, finds the directory entry, asserts the stored mtime against the source within the tolerance and asserts permissions == Some(0o750) under cfg(unix). It is driven by test_add_directory_recursive_preserves_nonempty_directory_metadata (Tar, 1s) and test_add_directory_recursive_preserves_nonempty_zip_directory_metadata (Zip, 2s - the DOS timestamp in a ZIP record has 2-second granularity, so the wider tolerance is format accuracy, not slack). The gap the 2026-08-21 comment reported to src/ffi/zip_writer.rs's owner was closed there: add_directory_entry_with_metadata now exists and Manifest::write_into's ZIP arm hands it the recorded mtime and mode. -->
- [x] Namespace suppression still prevents duplicate directory entries
  <!-- Ticked 2026-08-21: verified by reading src/creation/manifest.rs and its tests child. The manifest reserves each recorded entry through the write-mode namespace tracker during the single walk, `build_reserves_every_recorded_entry_exactly_once` pins the once-per-entry property, and `build_stops_at_the_first_rejected_reservation` pins that a conflict aborts before any byte is written. -->
- [x] DCR written; affected record updated
  <!-- Unticked 2026-08-21: no DCR was written and no creation record was amended for this change, though it alters the entry count and byte content of every recursively created archive — exactly the class Required Action 3 wanted a record for. docs/records/ is outside this pass's file ownership, so this is filed for the record owner. -->
  <!-- Ticked 2026-09-02: the record owner acted. docs/records/DCR-013-recursive-creation-emits-directory-metadata.md exists and is registered in docs/records/index.yaml, which is the authoritative catalogue - both checked first-hand rather than taken from a closing report. The 2026-08-21 comment above is kept as written; it was accurate on its date. Half of this box is deliberately read narrowly: a DCR was written, which is what Required Action 3 asked for first; whether a separate pre-existing creation record also needed amending was not re-litigated here, and if the record owner judges that one did, this box should be re-opened with a dated reason rather than argued about in place. -->

### Related

- The accepted directory-metadata caveat (modification mode) — adjacent, does not cover creation
- OI-0080-005 (OPEN) — manifest-based recursive creation with single-walk pinning; the same walk

---
