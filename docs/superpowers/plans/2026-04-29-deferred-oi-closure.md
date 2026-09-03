---
type: Plan
title: "Deferred OI Closure — Review-0075 Phase 2 Routing Implementation Plan"
description: "Executed and retired 2026-08-04 — all scoped work landed; the sole OI-0069-002 residual is R0069-0063 (banner corrected 2026-08-07: it previously named R0069-0006, which landed 2026-04-30)."
tags: [project-control, ADR-0053, ADR-0058, ADR-0054, ADR-0064, ADR-0055, ADR-0027, R0075-0039, R0069-0063, R0069-0006, R0069-0064, R0075-0034, R0075-0031]
timestamp: 2026-04-29T00:00:00Z
status: archived
---

# Deferred OI Closure — Review-0075 Phase 2 Routing Implementation Plan

> **RETIRED 2026-08-04 — do not execute.** This plan's scope landed between 2026-04-29 and
> 2026-04-30 and its unchecked checkboxes are stale, not open work: OI-0075-001 (Option A path
> policy, AD 0064), OI-0075-003 (typed-handle finish/commit), and OI-0075-004 (all 14 items,
> D1 + D2 included) are RESOLVED in `docs/project/open-issues.md`; the three design notes shipped
> (see `docs/design-notes/` and AD 0065 which superseded the OI-0075-002 caching half); the
> OI-0065-002 Piz extra-fields question was later re-homed to the sole `zip` backend and closed
> when DCR-009 removed piz entirely. Banner correction (2026-08-07): this banner previously named
> Task 2.1 (R0069-0006 SFX-aware compressed-size denominator) as the one live remainder — wrong;
> Task 2.1 landed 2026-04-30 (`Archive::payload_size_for_ratio()`, marked **Landed** in
> OI-0069-002's own ledger). The actual OI-0069-002 residual is R0069-0063 (`ValidatedSource`
> construction order, v0.4), tracked in the open-issues ledger — not this plan. Kept for audit
> only.

> **For agentic workers: do NOT execute this plan.** It is RETIRED (see the banner above) and is
> kept only as a record of what was done. Its unchecked `- [ ]` boxes are stale, not open work —
> an agent that treats them as a task list will redo landed work. If you arrived here looking for
> live work, use `docs/backlog.md` and the TicGit backlog instead.

**Goal:** Close every "fix-now" route the user assigned during Review-0075 Phase 2 (OI-0069-002 residual, OI-0075-001, OI-0075-003, OI-0075-004) and write standalone design notes (cons/pros) for the three OIs the user wants surfaced for later decision (OI-0058-001, OI-0065-002, OI-0075-002). OI-0065-001 is skipped entirely — Windows CI is not on this host.

**Architecture:** The plan is sequenced around dependencies. Independent fixes go first to land fast wins. Then the v0.3 raw_path policy locks an ADR and propagates through every backend. Then the larger structural pieces — D1 trait-only dispatch, D2 typed-handle split — land per AD-0053 baseline behind a `v2-api` cargo feature so v0.2/v0.3 callers see no break in this version cycle. Post-D2, the typed `WriteArchive::finish` retires the OI-0075-003 sticky flags, R0075-0039 `try_commit_changes` consumes the typed `&mut ModifyArchive`, R0069-0063 tightens the `ValidatedSource` constructor to `&ReadArchive`. The breaking API changes (ArchiveEntry builder, FnMut filter, multipart return shape) land additively in v0.3 with deprecated aliases on the old shapes; v0.4 removes them.

**Tech Stack:** Rust 2024 edition (rustc 1.85+), cargo features (`v2-api` flag for D2 migration window), `tempfile`, `walkdir`, `zip`, `sevenz-rust2`, `unrar`, `piz`, libarchive FFI. Tests run under `TMPDIR=/Volumes/Temp/claude`.

**Out-of-scope by user routing:** OI-0058-001 (footprint split, design-doc only), OI-0065-001 (Windows libarchive, full skip), OI-0065-002 (Piz extra-fields, design-doc only), OI-0075-002 caching half (design-doc only).

---

## File-Structure Map

Files this plan creates or modifies:

### Phase 0 — Design notes (new directory)
- Create: `docs/design-notes/oi-0058-001-feature-footprint.md`
- Create: `docs/design-notes/oi-0065-002-piz-extra-fields.md`
- Create: `docs/design-notes/oi-0075-002-snapshot-caching.md`

### Phase 1 — Non-UTF-8 path policy (Option A: preserve raw bytes)
- Create: `docs/records/AD-0064-r0075-non-utf8-path-policy-option-a.md`
- Modify: `src/ffi/libarchive_wrapper.rs` (`path: String` → `PathBuf`, add wide-pathname call site for Windows)
- Modify: `src/ffi/wrapper.rs` (UnRAR `path: String` → `PathBuf`)
- Modify: `src/archive.rs` (Archive's stored `path` already `PathBuf` — no change; SFX paths use raw_path-aware setup)
- Modify: `src/creation.rs::add_file_from_path` (preserve raw bytes for archive-name derivation; bring `raw_path` along)
- Modify: `src/inspection.rs::detect_multipart` (raw-byte sibling enumeration)
- Modify: `src/entry.rs::ArchiveEntry` (already has `raw_path: Option<Vec<u8>>` — confirm usage from creation paths)
- Test: `tests/integration/non_utf8_paths.rs` (new) — Unix `\x80` bytes, gated `#[cfg(unix)]`

### Phase 2 — OI-0069-002 self-contained items
- Modify: `src/security.rs::check_extraction_safe_with_archive` (use `_backing_tempfile` size when present for SFX denominator) — R0069-0006
- Modify: `src/archive.rs` (add `pub(crate) fn payload_size_for_ratio(&self) -> u64`)
- Modify: `src/modification.rs::load_zip_source_extras` (add name+CRC+size cross-check; key by index but verify) — R0069-0064 (overlaps R0075-0034)
- Test: `tests/integration/sfx_ratio.rs` (new) — SFX with embedded zip-bomb, assert `OperationBlocked` triggers on payload size, not outer-exe size
- Test: `tests/integration/modification.rs` (extend) — modify a ZIP whose source listing has a CRC drift; expect typed error

### Phase 3 — OI-0075-004 small items
- Modify: `src/format.rs::ArchiveFormat` enum (add `TarZst`, `TarLz4`, `TarLzma`, `Zst`, `Lz4`, `Lzma` variants; update `extensions()`, `suffix()`, `can_create()`, `can_modify()`) — R0075-0031
- Modify: `src/ffi/zip_writer.rs` (add `add_file_from_reader_with_size(&mut self, archive_path, reader, expected_size)` that wraps `Read::take` and verifies post-read length) — R0075-0032
- Modify: `src/modification.rs::commit_changes` (route `EntrySource::Reader { size: Some(n) }` through new size-aware path)
- Modify: `src/entry.rs` rustdoc on `permissions: Option<u32>` (lock the contract: Unix bits only after R0075-0024/0044/0045/0056) — R0075-0079

### Phase 4 — OI-0075-004 SFX / RAR internals
- Modify: `src/archive.rs::stage_sfx_payload` (add optional `cancel: Option<&AtomicBool>` param + progress-callback hook for staging copy) — R0075-0003
- Modify: `src/options.rs::ExtractionOptions` (add `staging_progress: Option<Box<dyn ProgressCallback>>`)
- Rewrite: `src/ffi/wrapper.rs::parse_rar5_recovery` (streaming RAR5-vint parser; replace 50-byte fixed prefix) — R0075-0061
- Modify: `src/ffi/wrapper.rs::recovery_percentage` (consume the new parser output instead of byte-scan heuristic) — R0075-0062
- Modify: `src/sfx/result.rs::SfxDetectionResult` (`pub` fields → `pub(crate)`, add accessor methods — `pub fn is_sfx(&self) -> bool`, `pub fn archive_format(&self) -> Option<ArchiveFormat>`, etc.) — R0075-0071
- Test: `tests/integration/rar5_recovery_vint.rs` (new) — fixtures with variable-length vint > 1 byte; assert percentage parsed correctly

### Phase 5 — OI-0075-004 type-shape (breaking, additive in v0.3)
- Modify: `src/entry.rs::ArchiveEntry` (add builder type `ArchiveEntryBuilder`; keep public fields with `#[deprecated]` notes for v0.4; add constructor `ArchiveEntry::file(path, id) -> Self`, `ArchiveEntry::directory(path, id)` already exists — keep) — R0075-0078
- Modify: `src/options.rs::EntryFilter` (widen to `Box<dyn FnMut(&ArchiveEntry) -> bool + Send>`; add `EntryFilterFn` alias for the old `Fn`-shaped form, deprecate it) — R0075-0080
- Modify: `src/inspection.rs::detect_multipart` (introduce typed `MultipartLayout { Single { path: PathBuf }, Multi { parts: Vec<PathBuf> } }`; make a v0.3 wrapper that returns the old `(bool, Vec<PathBuf>)` mapping with `#[deprecated]`) — R0075-0083
- Modify: `src/inspection.rs::calculate_manifest_digest` (introduce streaming traversal: open the read backend once, iterate entries through `ValidatedSource::extract_to_stream`, hash CRCs without per-entry archive reopen) — R0075-0084

### Phase 6 — CompressionOptions builder split
- Modify: `src/options.rs::CompressionOptions` (introduce per-format builders — `ZipCompressionOptions`, `SevenZCompressionOptions`, etc.; old `CompressionOptions` kept with `#[deprecated]` for v0.3 cycle) — R0075-0081

### Phase 7 — D1 dispatch unification (R0069-0003 prerequisite)
- Modify: `src/backend.rs` (add `ExtractionPlan` struct that captures the per-call options bag; `ReadBackend::extract_all(&self, plan: &ExtractionPlan) -> Result<Vec<ArchiveWarning>>`; default impl falls back to per-backend implementations)
- Modify: `src/extraction.rs::dispatch_extract_core` (collapse the per-backend match arms into a single `read_backend_view(...).extract_all(&plan)` call)
- Modify: per-backend `extract_all_with_options` (wrappers that build `ExtractionPlan` from their existing args)

### Phase 8 — D2 Archive god-object split (per AD-0053 baseline)
- Create: `src/archive/mode_split.rs` (new module hosting `ReadArchive`, `WriteArchive`, `ModifyArchive`)
- Modify: `Cargo.toml` (add `[features] v2-api = []`)
- Modify: `src/archive.rs` (`Archive` becomes `#[deprecated(since = "0.3.0")]` enum with `Read|Write|Modify` variants under `v2-api`; otherwise current struct shape preserved when feature is off)
- Modify: `src/lib.rs` (re-exports gated by `v2-api`)
- Modify: every internal call site that constructs `Archive::Read` to construct `ReadArchive` directly under the feature
- Test: `tests/v2_api_compat.rs` (new) — under `--features v2-api`, every old API call still compiles

### Phase 9 — Post-D2 finalization
- Modify: `src/archive/mode_split.rs::WriteArchive::finish(self) -> Result<()>` (consuming method; eliminate `finalized` flag)
- Modify: `src/archive/mode_split.rs::ModifyArchive::commit_changes(self) -> Result<()>` (consuming method)
- Modify: `src/archive.rs::Archive::Drop` impl (when `v2-api` is on, retire sticky-flag logic for new types; old `Archive::Read|Write|Modify` enum still uses sticky flags for the migration window)
- Add: `src/archive/mode_split.rs::ModifyArchive::try_commit_changes(&mut self) -> Result<()>` — R0075-0039
- Modify: `src/extraction.rs::ValidatedSource` constructor signature: `from_listed_archive(&'a ReadArchive)` (under `v2-api`) — R0069-0063
- Update: `docs/project/open-issues.md` (mark resolved entries, leave only items still parked)

---

## Test-Run Conventions

Every `cargo test` invocation in this plan uses:

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features <test_name>
```

Lint+format pass after every commit:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

`cargo build --no-default-features` runs at end of each phase to guarantee the no-default-features profile stays clean.

Reusable bash snippet for "verify clean tree":

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -20
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

---

# PHASE 0 — Independent Design Notes

These three OIs were routed by the user to be deferred *with cons/pros captured* for a later decision round. No implementation. Each note lives under `docs/design-notes/` (separate from `docs/records/` because they are not ADRs — no decision is being made, just the option space is being recorded).

## Task 0.1: Write OI-0058-001 footprint-split design note

**Files:**
- Create: `docs/design-notes/oi-0058-001-feature-footprint.md`

- [ ] **Step 1: Read AD 0058 to ground the doc**

```bash
cat /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/docs/records/AD-0058-feature-first-footprint-split-and-facade-crates.md
```

- [ ] **Step 2: Write the design note**

```markdown
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
```

- [ ] **Step 3: Commit**

```bash
git add docs/design-notes/oi-0058-001-feature-footprint.md
git commit -m "docs: design note for OI-0058-001 (footprint split cons/pros)"
```

## Task 0.2: Write OI-0065-002 Piz extra-fields design note

**Files:**
- Create: `docs/design-notes/oi-0065-002-piz-extra-fields.md`

- [ ] **Step 1: Inspect Piz capabilities**

```bash
grep -rn "pub.*extra" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/piz_wrapper.rs | head -10
```

- [ ] **Step 2: Write the design note**

```markdown
# OI-0065-002 — Piz reader 0x5455/0x000A extra-field parsing: cons / pros

**Status:** parked design note. Surfaces a default-reader trade-off that needs user choice.

## Problem (one sentence)
The writer side of `preserve_metadata=true` now emits ZIP `0x5455` ("Universal Time") + 0x000A NTFS extra fields with `accessed`/`created` timestamps, but the default Piz-backed ZIP reader does not surface those fields back to `ArchiveEntry`, so a round-trip `Archive::open(...).list_files()` reports `accessed=None, created=None`.

## Two paths

### Option A — patch piz upstream to expose extra-field bytes
Walk the central-directory record, expose extra-field bytes on `FileMetadata`, parse 0x5455/0x000A in `src/ffi/piz_wrapper.rs::parse_entry`.

**Pros**
- Keeps Piz's mmap-based read path (fast, low-syscall, no per-entry decompress to peek metadata).
- Single-crate fix once the upstream PR lands.
- No public-API change in unified-archive — `ArchiveEntry.accessed/created` start populating without callers noticing.

**Cons**
- Upstream piz hasn't landed a release in 18+ months. PR may sit idle. Forking adds maintenance burden.
- Even if upstream accepts, semver-bumping piz across our dep tree is its own coordination cost.
- The `0x5455` flags byte (ModTime/AccessTime/CreateTime presence) needs careful parsing — bit-flag layout is in PKWARE APPNOTE, not in the extra-field bytes themselves.

### Option B — switch the default ZIP reader from Piz to the `zip` crate
The `zip` crate (`zip_rs`) already exposes `ZipFile::extra_data_fields()`. Wire `ZipArchive` (already in the codebase as the modify-side reader) as the default for `Archive::open(zip)`.

**Pros**
- Single crate move, no upstream dependency. Already validated for modify-side correctness.
- `zip` crate parses 0x5455 natively (`extra_field_universal_time`).
- Removes dual-reader divergence: today, Piz handles `Archive::open(...)` reads but `ZipArchive` handles modify-mode reads. Picking one closes the divergence.

**Cons**
- Loses Piz's mmap throughput. `zip` reads via `BufReader<File>` — slower for large central directories on big archives. Benchmark needed.
- ZipArchive currently sits behind a `Mutex<Option<RawZipArchive>>` for modify-side caching (per AD 0054). The default-reader switch needs the same caching, exported with the same Mutex semantics.
- Field-coverage cliff: callers depending on Piz-specific behaviour (mmap'd lazy reads, zero-copy slices) lose those — but unified-archive's public API never exposed those, so no API-level regression.

## Recommendation (for the user, not a decision here)
**Option B**. Reason: no upstream dependency, mmap loss is bounded by archive size (large archives are extracted not just listed), and the dual-reader divergence is the larger ongoing cost.

## Effort
- Option A: 1 day local + indefinite upstream wait (could be months). 1 day to consume upstream PR.
- Option B: 1 day to swap reader + benchmark + update tests; 1 day to backport mmap caching policy if needed.

## Why this isn't an ADR
The user routed it as "make a doc with cons/pros." The decision (A vs. B) is open and the answer depends on benchmark numbers we don't have yet on the consumer's archives. ADR will be written when the user picks.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design-notes/oi-0065-002-piz-extra-fields.md
git commit -m "docs: design note for OI-0065-002 (Piz extra-fields cons/pros)"
```

## Task 0.3: Write OI-0075-002 snapshot-caching design note

**Files:**
- Create: `docs/design-notes/oi-0075-002-snapshot-caching.md`

- [ ] **Step 1: Read current rustdoc on Archive snapshot semantics**

```bash
sed -n '85,130p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/archive.rs
```

- [ ] **Step 2: Write the design note**

```markdown
# OI-0075-002 — Snapshot semantics & per-backend caching baseline: cons / pros

**Status:** parked design note. Send-safety half is closed (Review 0075). Caching-baseline half is open.

## Problem (one sentence)
`Archive`'s rustdoc was tightened in Review 0075 to "best-effort metadata cache" with a per-backend caching list, but the backends do not agree on a single caching policy: Piz caches mmap+central directory, ZipReader caches a Mutex'd handle, 7z caches the TOC, libarchive lazily reopens for every operation, UnRAR keeps the FFI handle alive.

## Two paths

### Option A — every read backend memoises listing/metadata once on first use ("freeze the view")
Make `ReadBackend::list_files()` cache its output in a `OnceLock<Vec<ArchiveEntry>>` on the backend struct. Every other read method consumes the cached listing. The libarchive backend gains a parsed-metadata cache after open-time validation.

**Pros**
- Strict snapshot semantics: `archive.list_files()` returns the same vec twice in a row even if the file was rewritten on disk between calls.
- Removes the surprise factor for callers who treat `Archive` as a logical view of the archive (the rustdoc wording today admits this is best-effort).
- Closes the divergence: Piz/Zip/7z already cache; only libarchive doesn't.
- Compile-time `Send` assertion (already landed) is unchanged.

**Cons**
- Memory cost grows: a single `Archive::open` of a 1M-entry tar carries ~200 MB of cached entry metadata for the lifetime of the handle. Today, that's only paid by Piz/Zip/7z.
- Subtle behavior change: a user who edits the archive and re-opens via the same handle would today see new contents on libarchive (lazy reopen sees the new file); under Option A they'd see the cached listing forever. Caller migration story matters.
- Cache invalidation: any "this archive changed under us" pathway needs an explicit `Archive::reopen()` API.

### Option B — accept current behaviour, formalise it as ADR
Lock the wording: "best-effort metadata cache; per-backend caching is documented per backend; no global snapshot guarantee."

**Pros**
- Zero code change. Ship as-is.
- Memory cost stays where it was.
- libarchive's lazy reopen lets long-lived `Archive` handles see disk-side rewrites, which is the more useful behaviour for some callers (build systems, watch loops).

**Cons**
- Future-reviewer hazard: Option A keeps getting suggested every code review because the heterogeneity violates principle of least surprise. ADR would close that loop.
- The compile-time `Send` assertion guards drift but doesn't pin caching.

## Recommendation (for the user, not a decision here)
**Option B + ADR**. Reason: the backends were heterogeneous on purpose (each format gets the caching that matches its FFI shape); forcing them to align would either bloat libarchive memory or weaken Piz/Zip/7z. Codify the heterogeneity instead.

## Effort
- Option A: 2 days for libarchive caching + 0.5 day per backend for cache-invalidation tests + 1 day for the `Archive::reopen()` API.
- Option B: 0.5 day for an ADR. Done.

## Send-safety audit (R0075-0004)
Already resolved in Review 0075:
- SAFETY comment refreshed against current fields.
- Compile-time `Send` assertion at `src/archive.rs` (`const _: fn() = || { ... };`) anchors the invariant.

No further work on the Send half.

## Why this isn't an ADR
The user routed it as cons/pros doc. The Option-A-vs-B call is the user's; the ADR follows the decision.
```

- [ ] **Step 3: Commit**

```bash
git add docs/design-notes/oi-0075-002-snapshot-caching.md
git commit -m "docs: design note for OI-0075-002 (snapshot/caching cons/pros)"
```

---

# PHASE 1 — OI-0075-001: Non-UTF-8 Path Policy (Option A)

The user picked Option A: preserve raw bytes via `raw_path`-style fields throughout. This phase locks the policy in an ADR, then propagates the change across every `path: String` site, then adds platform-gated round-trip tests.

## Task 1.1: Write ADR locking Option A

**Files:**
- Create: `docs/records/AD-0064-r0075-non-utf8-path-policy-option-a.md`

- [ ] **Step 1: Verify ADR numbering**

```bash
ls /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/docs/records/ | grep -E '^00[6-9][0-9]-' | sort | tail -5
```

Expected: `docs/records/AD-0063-r0075-closure-and-design-positions.md` is highest. Next is `0064-`.

- [ ] **Step 2: Write the ADR**

```markdown
# AD 0064: Non-UTF-8 path policy — preserve raw bytes (Option A)

## Context and Problem Statement

Review 0075 raised four findings (R0075-0007, R0075-0023, R0075-0055, R0075-0082)
about path-handling lossy conversions:

- `LibarchiveArchive::path: String` (lossy at construction)
- `UnrarArchive::path: String` (lossy at construction)
- `add_file_from_path` derives the archive name via `to_string_lossy()`
- `detect_multipart` lossy-converts file names + stems

OI-0075-001 routed the bundle as a single design question. User Phase-2
routing (2026-04-29) picked Option A: preserve raw bytes via `raw_path`-style
fields throughout; keep `to_string_lossy` only for display.

This ADR locks the policy. Implementation lands across every backend in
the deferred-OI-closure plan Phase 1.

## Decision Drivers

- **Symmetry.** Listing-side already preserves `raw_path` (per R0070-0025).
  Archive-level metadata being lossy while entry-level isn't is a paper cut.
- **Round-trip correctness.** A user with a non-UTF-8 archive on a
  filesystem that allows non-UTF-8 paths can today get a `Archive::path()`
  string that doesn't match the original bytes. Option A closes that.
- **Cost asymmetry.** `PathBuf` is the same size as `String` on every
  platform unified-archive supports; the runtime cost is zero.
- **Windows wide-path support.** Libarchive ships
  `archive_entry_copy_pathname_w`, so the destination side gets the
  full-fidelity path even on Windows.

## Considered Options

1. **Option A — preserve raw bytes everywhere.** Backend `path` fields
   become `PathBuf`; archive-name derivation preserves `OsStr` bytes;
   detect_multipart uses raw byte comparisons; libarchive uses the wide
   variant on Windows.
2. **Option B — reject non-UTF-8 input loudly.** Add `InvalidPath` error;
   facade refuses to open / create / modify a non-UTF-8 path.

## Decision Outcome

ACCEPT Option A. User Phase-2 routing picked it explicitly:
"preserve raw_path. but keep as open issue."

Status: Implementation in deferred-OI-closure plan Phase 1.

### Implementation

`src/ffi/libarchive_wrapper.rs`:
- `LibarchiveArchive::path: String` → `PathBuf`.
- `archive_entry_set_pathname` call site uses `path_to_cstring`
  (the existing helper) on Unix; on Windows uses
  `archive_entry_copy_pathname_w` with the wide-char form.

`src/ffi/wrapper.rs` (UnRAR):
- `UnrarArchive::path: String` → `PathBuf`.
- All `path.clone()` callers updated.

`src/creation.rs::add_file_from_path`:
- Archive name comes from `path.file_name().ok_or(...)?.as_encoded_bytes()`
  on Unix (`OsStrExt`) or wide-form on Windows.
- `ArchiveEntry::raw_path` populated for non-UTF-8 names.

`src/inspection.rs::detect_multipart`:
- Sibling enumeration uses `OsStr::to_string_lossy` only for matching
  patterns (`.part1.rar`, `.001`); when a sibling matches, the original
  `PathBuf` is kept.

### Tests

- `tests/integration/non_utf8_paths.rs` (new, `#[cfg(unix)]`):
  archive a file whose name is `b"\xff\xfe.txt"`; round-trip through
  every backend; assert `raw_path == Some(b"\xff\xfe.txt".to_vec())`.
- Windows wide-path test deferred to Windows CI lane (OI-0065-001).

## Consequences

- Good: archive-level path metadata round-trips losslessly on every
  Unix-supported filesystem.
- Good: Windows libarchive wide-pathname support lands as a side
  benefit — destination paths with non-BMP code points work.
- Good: closes the asymmetry between entry-level (raw_path) and
  archive-level (path) preservation.
- Bad: PathBuf doesn't `impl Display` — every error-message site that
  prints `archive.path` now uses `.display()`. ~30 call sites; mechanical.
- Bad: `String`-typed FFI return paths (e.g. unrar's archive name)
  still cross a UTF-8 boundary inside the FFI layer; the new `PathBuf`
  is constructed from the same bytes via `OsStr::from_encoded_bytes_unchecked`
  on Unix. Documented at the FFI layer.
```

- [ ] **Step 3: Commit**

```bash
git add docs/records/AD-0064-r0075-non-utf8-path-policy-option-a.md
git commit -m "docs(adr): 0064 — non-UTF-8 path policy (Option A: preserve raw bytes)"
```

## Task 1.2: Migrate `LibarchiveArchive::path` to `PathBuf`

**Files:**
- Modify: `src/ffi/libarchive_wrapper.rs` (struct field + every constructor + every `.path.clone()` site)

- [ ] **Step 1: Find every site that touches `LibarchiveArchive::path`**

```bash
grep -n "self\.path\|libarchive\.path\|\.path = " /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/libarchive_wrapper.rs | head -40
```

- [ ] **Step 2: Read the constructor + all the .path-touch sites**

Use the Read tool on each block flagged by Step 1. Make notes; the migration is mechanical but every site needs `&str` → `&Path` adjustments.

- [ ] **Step 3: Write a failing test**

`tests/integration/non_utf8_paths.rs` (new):

```rust
#![cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::ffi::OsStr;
use std::path::PathBuf;
use unified_archive::Archive;

fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

#[test]
fn libarchive_archive_path_preserves_non_utf8_bytes() {
    // The archive filename itself contains \xff\xfe (invalid UTF-8 on Unix).
    let mut archive_name = std::ffi::OsString::from("");
    archive_name.push(OsStr::from_bytes(b"weird-\xff\xfe.tar"));
    let archive_path = temp_dir().join(&archive_name);
    let _ = std::fs::remove_file(&archive_path);

    // Create a tar with one file inside.
    let opts = unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Tar);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    a.add_file_from_data("hello.txt", b"hi").unwrap();
    a.finish().unwrap();

    // Reopen via Archive::open and assert the round-tripped path
    // (the PathBuf form) equals the bytes we wrote.
    let opened = Archive::open(&archive_path).unwrap();
    assert_eq!(opened.path().as_os_str().as_bytes(), archive_path.as_os_str().as_bytes(),
        "Archive::path() must round-trip the original PathBuf bytes");
    let _ = std::fs::remove_file(&archive_path);
}
```

Add `mod non_utf8_paths;` to `tests/integration/main.rs` (if integration tests use a single binary; otherwise add a top-level entry in `tests/`).

- [ ] **Step 4: Run the test (expect FAIL — String coercion drops bytes)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features non_utf8 2>&1 | tail -30
```

Expected: `assert_eq!` fails because `Archive::path` is currently a `PathBuf` constructed from a `String` lossy round-trip somewhere downstream (or: it actually passes already on Linux — in which case promote the test for Windows path-name-with-NTFS-stream cases). Read the actual diff and adjust.

- [ ] **Step 5: Migrate the field type**

`src/ffi/libarchive_wrapper.rs` — field declaration:

```rust
pub struct LibarchiveArchive {
    pub(crate) path: PathBuf,
    // ... rest unchanged
}
```

Every constructor (`open(...)`, `open_at_offset(...)`, `create(...)` etc.):

```rust
// Before:
let path_str = path.to_string_lossy().into_owned();
Self { path: path_str, ... }

// After:
Self { path: path.to_path_buf(), ... }
```

Every method that took `&str` from `self.path`:

```rust
// Before:
pub fn path(&self) -> &str { &self.path }

// After:
pub fn path(&self) -> &Path { &self.path }
```

Every error-message site that interpolated `self.path`:

```rust
// Before:
return Err(ArchiveError::format(format!("foo at {}", self.path)));

// After:
return Err(ArchiveError::format(format!("foo at {}", self.path.display())));
```

- [ ] **Step 6: Update libarchive's `archive_entry_set_pathname` site**

Find the call site:

```bash
grep -n "archive_entry_set_pathname\|archive_entry_copy_pathname" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/libarchive_wrapper.rs
```

Wrap the call so Windows uses the `_w` form:

```rust
#[cfg(unix)]
{
    use std::os::unix::ffi::OsStrExt;
    let bytes = entry_path.as_os_str().as_bytes();
    let cstring = std::ffi::CString::new(bytes).map_err(|e| ArchiveError::path(format!("nul in path: {e}")))?;
    unsafe { libarchive_sys::archive_entry_set_pathname(entry_handle, cstring.as_ptr()); }
}

#[cfg(windows)]
{
    use std::os::windows::ffi::OsStrExt as _;
    let mut wide: Vec<u16> = entry_path.as_os_str().encode_wide().collect();
    wide.push(0);  // null terminator
    unsafe { libarchive_sys::archive_entry_copy_pathname_w(entry_handle, wide.as_ptr()); }
}
```

Verify `archive_entry_copy_pathname_w` is in our libarchive-sys bindings:

```bash
grep -n "archive_entry_copy_pathname_w\|archive_entry_set_pathname" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/build.rs
```

If the binding is missing, add it. Likely a one-line `bindgen` allowlist addition or an `extern "C"` block extension in `src/ffi/libarchive_sys.rs`.

- [ ] **Step 7: Run the test (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features non_utf8 2>&1 | tail -20
```

- [ ] **Step 8: Run full lint/test pass**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -20
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add src/ffi/libarchive_wrapper.rs tests/integration/non_utf8_paths.rs tests/integration/main.rs
git commit -m "feat(libarchive): PathBuf + Windows wide-pathname for non-UTF-8 path round-trip (OI-0075-001 / R0075-0023)"
```

## Task 1.3: Migrate `UnrarArchive::path` to `PathBuf`

**Files:**
- Modify: `src/ffi/wrapper.rs`

- [ ] **Step 1: Read current shape**

```bash
grep -n "self\.path\|UnrarArchive\.path\|path: String\|path: PathBuf" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/wrapper.rs | head -30
```

- [ ] **Step 2: Add a failing test in non_utf8_paths.rs**

```rust
#[cfg(feature = "rar-support")]
#[test]
fn unrar_archive_path_preserves_non_utf8_bytes() {
    // Use an existing RAR fixture but rename it to a non-UTF-8 name.
    let src = PathBuf::from("tests/fixtures/rar/test.rar");
    if !src.exists() { return; }  // RAR fixtures may be optional
    let mut weird = std::ffi::OsString::from("");
    weird.push(OsStr::from_bytes(b"weird-\xff\xfe.rar"));
    let dst = temp_dir().join(&weird);
    let _ = std::fs::remove_file(&dst);
    std::fs::copy(&src, &dst).unwrap();

    let opened = Archive::open(&dst).unwrap();
    assert_eq!(opened.path().as_os_str().as_bytes(), dst.as_os_str().as_bytes());
    let _ = std::fs::remove_file(&dst);
}
```

- [ ] **Step 3: Run (expect FAIL on String-coerced path)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features --features rar-support unrar_archive_path 2>&1 | tail -20
```

- [ ] **Step 4: Migrate the field**

`src/ffi/wrapper.rs`:

```rust
// Before:
pub struct UnrarArchive {
    handle: RARHandle,
    path: String,
    password: Option<SecStr>,
    flags: c_uint,
}

// After:
pub struct UnrarArchive {
    handle: RARHandle,
    path: PathBuf,
    password: Option<SecStr>,
    flags: c_uint,
}
```

Constructor:

```rust
// Before:
let path_str = path.as_ref().to_string_lossy().into_owned();
Self { handle, path: path_str, password, flags }

// After:
Self { handle, path: path.as_ref().to_path_buf(), password, flags }
```

Path getter:

```rust
// Before:
pub fn path(&self) -> &str { &self.path }

// After:
pub fn path(&self) -> &Path { &self.path }
```

Every error-message site interpolating `self.path` → `self.path.display()`.

The RAR FFI side passes the path as a CString built from a UTF-8 string. On Unix the underlying libunrar API accepts UTF-8 bytes; convert at the FFI boundary:

```rust
#[cfg(unix)]
let cpath = {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(self.path.as_os_str().as_bytes())
        .map_err(|e| ArchiveError::path(format!("nul in path: {e}")))?
};

#[cfg(windows)]
// libunrar exposes RAROpenArchiveExW for wide paths. If the binding is
// missing, add it. The migration here is the same pattern as Task 1.2.
```

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features unrar_archive_path 2>&1 | tail -10
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
git add src/ffi/wrapper.rs tests/integration/non_utf8_paths.rs
git commit -m "feat(unrar): PathBuf for non-UTF-8 path round-trip (OI-0075-001 / R0075-0055)"
```

## Task 1.4: Preserve raw bytes in `add_file_from_path` archive-name derivation

**Files:**
- Modify: `src/creation.rs::add_file_from_path`

- [ ] **Step 1: Read the current implementation**

```bash
sed -n '195,240p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/creation.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/non_utf8_paths.rs`:

```rust
#[test]
fn add_file_from_path_preserves_non_utf8_filename() {
    use std::os::unix::ffi::OsStrExt;
    let dir = temp_dir().join("non_utf8_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut weird_filename = std::ffi::OsString::from("");
    weird_filename.push(OsStr::from_bytes(b"file-\xff\xfe.txt"));
    let src_file = dir.join(&weird_filename);
    std::fs::write(&src_file, b"hello").unwrap();

    let archive_path = dir.join("out.tar");
    let opts = unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Tar);
    let mut a = Archive::create(&archive_path, opts).unwrap();
    a.add_file_from_path(&src_file).unwrap();
    a.finish().unwrap();

    let opened = Archive::open(&archive_path).unwrap();
    let entries = opened.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];

    // raw_path must surface the original bytes, even though path: String
    // is lossy.
    assert_eq!(entry.raw_path.as_ref().expect("raw_path must be Some for non-UTF-8 names").as_slice(),
        b"file-\xff\xfe.txt", "raw_path must round-trip the original filename bytes");

    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run (expect FAIL — current impl uses `to_string_lossy`)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features add_file_from_path_preserves 2>&1 | tail -15
```

- [ ] **Step 4: Implement raw-bytes preservation**

In `src/creation.rs::add_file_from_path`:

```rust
pub fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
    let path_ref = path.as_ref();
    let archive_name_os = path_ref.file_name()
        .ok_or_else(|| ArchiveError::path("source path has no file name"))?;

    // Lossy form for the public-facing archive_path string (matches
    // existing surface). raw bytes preserved separately.
    let archive_path_lossy = archive_name_os.to_string_lossy().into_owned();

    #[cfg(unix)]
    let raw_path_bytes = {
        use std::os::unix::ffi::OsStrExt;
        let bytes = archive_name_os.as_bytes();
        // Only attach raw_path when the lossy form differs from the bytes.
        if bytes == archive_path_lossy.as_bytes() { None } else { Some(bytes.to_vec()) }
    };

    #[cfg(windows)]
    let raw_path_bytes = {
        // Windows doesn't carry "non-UTF-8" filenames in the same way
        // (filesystem layer is UTF-16); raw_path stays None on this
        // platform unless the OsStr contains unpaired surrogates.
        None
    };

    // Existing call to backend.add_file_from_path or similar, but pass
    // the raw_path_bytes through so the backend can attach it to the
    // ArchiveEntry / writer's central directory.
    self.add_file_from_path_with_metadata(path_ref, &archive_path_lossy, raw_path_bytes)
}
```

`add_file_from_path_with_metadata` (a new internal helper) routes `raw_path_bytes` to the backend. For libarchive, this means attaching the raw bytes via `archive_entry_set_pathname` (Unix) or `archive_entry_copy_pathname_w` (Windows). For ZipWriter, attach as the entry name.

- [ ] **Step 5: Update Zip writer to honor the raw bytes**

`src/ffi/zip_writer.rs::add_file_from_path` already takes a path; extend its signature with a `raw_path: Option<&[u8]>` parameter and write the raw bytes as the central-directory filename when present (the `zip` crate's `FileOptions::filename_from_bytes` if available; otherwise `zip` defaults to UTF-8 — fall back to lossy).

If the `zip` crate doesn't support raw byte filenames, surface an `Unsupported` error rather than silently truncating.

- [ ] **Step 6: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features add_file_from_path_preserves 2>&1 | tail -10
```

- [ ] **Step 7: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/creation.rs src/ffi/zip_writer.rs src/ffi/libarchive_wrapper.rs tests/integration/non_utf8_paths.rs
git commit -m "feat(creation): preserve raw bytes in add_file_from_path (OI-0075-001 / R0075-0007)"
```

## Task 1.5: Raw-byte sibling enumeration in `detect_multipart`

**Files:**
- Modify: `src/inspection.rs::detect_multipart`

- [ ] **Step 1: Read current `detect_multipart` impl**

```bash
sed -n '420,540p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/inspection.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/non_utf8_paths.rs`:

```rust
#[test]
#[cfg(unix)]
fn detect_multipart_handles_non_utf8_volume_names() {
    use std::os::unix::ffi::OsStrExt;
    let dir = temp_dir().join("multipart_non_utf8");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Create three "fake" RAR volumes whose stem has non-UTF-8 bytes.
    // .part1.rar / .part2.rar / .part3.rar layout.
    for n in 1..=3u32 {
        let mut name = std::ffi::OsString::from("");
        name.push(OsStr::from_bytes(b"vol-\xff"));
        name.push(format!(".part{n}.rar"));
        let path = dir.join(&name);
        std::fs::write(&path, b"\x52\x61\x72\x21\x1A\x07\x01\x00").unwrap();  // RAR5 header (just for the magic check)
    }

    let mut first = std::ffi::OsString::from("");
    first.push(OsStr::from_bytes(b"vol-\xff"));
    first.push(".part1.rar");
    let first_path = dir.join(&first);

    let archive = Archive::open(&first_path).unwrap();
    let (is_mp, parts) = archive.detect_multipart().unwrap();
    assert!(is_mp, "Should detect 3-volume multipart even with non-UTF-8 stems");
    assert_eq!(parts.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run (expect FAIL — current impl uses to_string_lossy on stems)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features detect_multipart_handles_non_utf8 2>&1 | tail -15
```

- [ ] **Step 4: Replace lossy comparisons with raw byte / OsStr comparisons**

In `src/inspection.rs::detect_multipart` (and helpers `match_zip_split`, `match_rar_part`, `match_numeric_volume`):

For each helper, instead of calling `path.file_name().and_then(|n| n.to_str())`, work with raw bytes:

```rust
// Before:
fn match_rar_part(name: &OsStr) -> Option<u32> {
    let name_str = name.to_str()?;
    // parse `<stem>.part<NN>.rar`
}

// After:
fn match_rar_part(name: &OsStr) -> Option<(Vec<u8>, u32)> {
    // Returns (stem_bytes, volume_num). Works on raw bytes; only the
    // ".partN.rar" suffix needs to be UTF-8 (it always is by definition).
    use std::os::unix::ffi::OsStrExt;
    let bytes = name.as_bytes();
    let dot_part = b".part";
    let suffix = b".rar";
    if !bytes.ends_with(suffix) { return None; }
    let pre = &bytes[..bytes.len() - suffix.len()];
    let dot_pos = pre.windows(dot_part.len()).rposition(|w| w == dot_part)?;
    let num_bytes = &pre[dot_pos + dot_part.len()..];
    let num: u32 = std::str::from_utf8(num_bytes).ok()?.parse().ok()?;
    let stem = &bytes[..dot_pos];
    Some((stem.to_vec(), num))
}
```

When sibling enumeration runs `read_dir`, compare the raw-byte stems for membership instead of `String` equality.

On Windows, fall back to `OsStr::encode_wide()` UTF-16 comparison for the stem; the `.partN.rar` suffix is invariant ASCII.

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features detect_multipart 2>&1 | tail -10
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/inspection.rs tests/integration/non_utf8_paths.rs
git commit -m "feat(multipart): raw-byte sibling enumeration for non-UTF-8 volumes (OI-0075-001 / R0075-0082)"
```

## Task 1.6: Update OI-0075-001 status in open-issues.md

**Files:**
- Modify: `docs/project/open-issues.md` (mark OI-0075-001 RESOLVED)

- [ ] **Step 1: Mark resolution and reference the ADR**

Edit OI-0075-001 entry — change status, add resolution line, link AD 0064.

```markdown
- **Status:** RESOLVED 2026-04-29 (Option A landed across libarchive, UnRAR, creation, multipart; ADR locked policy)
- **Resolution:** AD 0064 + commits in deferred-OI-closure plan Phase 1
```

Update the verification checkboxes:
- [x] ADR records the chosen policy.
- [x] Every backend listed above either rejects or round-trips non-UTF-8 paths per the policy.
- [x] Platform-specific tests cover the non-UTF-8 round-trip. (Unix only; Windows tests deferred to OI-0065-001.)

- [ ] **Step 2: Commit**

```bash
git add docs/project/open-issues.md
git commit -m "docs(oi): close OI-0075-001 (non-UTF-8 path policy Option A landed)"
```

---

# PHASE 2 — OI-0069-002 Self-Contained Items

R0069-0003 is D1-blocked (Phase 7). R0069-0063 is D2-blocked (Phase 9). The two items that can land independently:

- **R0069-0006** — SFX archive ratio gate uses outer-executable size as compressed denominator. Fix uses `_backing_tempfile` size when present.
- **R0069-0064** — ZIP source-extras keyed by index without name+CRC+size cross-check. Add the cross-check.

R0069-0064 overlaps with R0075-0034 (same code path); both close in the same task.

## Task 2.1: SFX-aware compressed-size denominator (R0069-0006)

**Files:**
- Modify: `src/security.rs::check_extraction_safe_with_archive` (signature)
- Modify: `src/archive.rs` (add `payload_size_for_ratio()`)
- Modify: `src/extraction.rs` (callers of `check_extraction_safe_with_archive`)

- [ ] **Step 1: Read current `check_extraction_safe_with_archive`**

```bash
sed -n '450,480p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/security.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/sfx_ratio.rs` (new):

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionLimits, ExtractionOptions};

fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

#[test]
fn sfx_extraction_ratio_uses_payload_size_not_outer_exe_size() {
    // Build an SFX-shaped file: 1 MB of "stub" prefix + a small zip-bomb payload.
    // The payload's compression ratio is high (1:1000), but if we measure
    // against the outer executable size (1 MB + small payload), the ratio
    // looks safe — that's the bug. The fix: measure against payload size only.

    let dir = temp_dir().join("sfx_ratio_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let bomb_path = dir.join("bomb.zip");
    let opts = unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Zip);
    let mut bomb = Archive::create(&bomb_path, opts).unwrap();
    // 1 MB file compressed to ~1 KB (high ratio).
    let zeros = vec![0u8; 1024 * 1024];
    bomb.add_file_from_data("bomb.bin", &zeros).unwrap();
    bomb.finish().unwrap();

    let payload_bytes = std::fs::read(&bomb_path).unwrap();
    assert!(payload_bytes.len() < 100 * 1024, "compressed bomb should be small");

    // Build the SFX file: 1 MiB of bogus stub + payload.
    let sfx_path = dir.join("bomb.sfx.exe");
    let stub = vec![0u8; 1024 * 1024];
    let mut combined = stub;
    combined.extend_from_slice(&payload_bytes);
    std::fs::write(&sfx_path, &combined).unwrap();

    // Open via the SFX detection path and try to extract.
    let archive = Archive::open(&sfx_path).unwrap();
    let mut extract_opts = ExtractionOptions::default();
    extract_opts.destination = dir.join("out");
    extract_opts.limits = ExtractionLimits {
        max_compression_ratio: 100.0, // 100:1 cap
        ..ExtractionLimits::unlimited()
    };

    let result = archive.extract_all_with_options(&mut extract_opts);
    assert!(result.is_err(), "1000:1 ratio should trigger limit even with 1 MiB stub");
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(err_str.contains("Ratio") || err_str.contains("ratio"),
        "Expected ratio error, got: {err_str}");

    let _ = std::fs::remove_dir_all(&dir);
}
```

Add `mod sfx_ratio;` to `tests/integration/main.rs`.

- [ ] **Step 3: Run (expect FAIL — current impl uses outer-exe size)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features sfx_extraction_ratio 2>&1 | tail -20
```

The test passing without the fix would mean the ratio cap fires *anyway* because the stub-relative ratio is fine but the SFX is detected and the payload tempfile is now what gets hit. Read the actual diff and adjust.

- [ ] **Step 4: Add `Archive::payload_size_for_ratio()`**

`src/archive.rs`:

```rust
impl Archive {
    /// Returns the size in bytes that should serve as the
    /// compressed-size denominator for ratio gates. For SFX archives
    /// this is the staged payload size (excluding outer-executable
    /// stub); for plain archives it's the file size.
    pub(crate) fn payload_size_for_ratio(&self) -> std::io::Result<u64> {
        if let Some(tempfile) = &self._backing_tempfile {
            std::fs::metadata(tempfile.as_ref()).map(|m| m.len())
        } else {
            std::fs::metadata(&self.path).map(|m| m.len())
        }
    }
}
```

- [ ] **Step 5: Update `check_extraction_safe_with_archive` signature**

`src/security.rs`:

```rust
// Before:
pub(crate) fn check_extraction_safe_with_archive(
    entries: &[ArchiveEntry],
    archive_path: &Path,
    limits: &ExtractionLimits,
) -> Result<()>

// After:
pub(crate) fn check_extraction_safe_with_archive(
    entries: &[ArchiveEntry],
    compressed_size: u64,  // payload size after SFX staging
    limits: &ExtractionLimits,
) -> Result<()>
```

Internal `check_archive_ratio(entries, archive_path, limits)` becomes
`check_archive_ratio(entries, compressed_size, limits)`. Drop the
`std::fs::metadata(archive_path).len()` call.

- [ ] **Step 6: Update every caller**

Find them:

```bash
grep -rn "check_extraction_safe_with_archive" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/
```

Each caller now has access to an `Archive`; pass `archive.payload_size_for_ratio()?` instead of `&archive.path`.

- [ ] **Step 7: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features sfx_extraction_ratio 2>&1 | tail -10
```

- [ ] **Step 8: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/archive.rs src/security.rs src/extraction.rs tests/integration/sfx_ratio.rs tests/integration/main.rs
git commit -m "fix(security): use SFX payload size for ratio gate denominator (OI-0069-002 / R0069-0006)"
```

## Task 2.2: ZIP source-extras name+CRC+size cross-check (R0069-0064 / R0075-0034)

**Files:**
- Modify: `src/modification.rs::load_zip_source_extras` + `commit_changes` consumer

- [ ] **Step 1: Read the current keying logic**

```bash
sed -n '1390,1440p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/modification.rs
sed -n '810,870p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/modification.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/modification.rs` (extend):

```rust
#[test]
fn modify_zip_rejects_when_source_listing_drifts_from_zip_central_directory() {
    // Construct a ZIP via the libarchive listing path that has a CRC
    // different from what the source ZIP carries. This should fail at
    // commit_changes with a typed "listing drift" error rather than
    // silently committing with mismatched compression metadata.
    //
    // Concretely: open a ZIP, modify it, but between Archive::modify
    // and commit_changes, swap a file's CRC in the source by editing
    // the central directory. Hard to do reliably in a test; instead
    // unit-test the cross-check helper directly.

    use unified_archive::modification::cross_check_source_listing;
    use unified_archive::ArchiveEntry;
    let listing = vec![
        ArchiveEntry { id: 0, path: "a.txt".into(), crc32: Some(0xAABBCCDD), size: Some(100), .. ArchiveEntry::default() },
    ];
    let zip_extras_per_index = vec![
        // Drift: same index, different CRC.
        ZipSourceEntryView { name: "a.txt".into(), crc32: 0xDEADBEEF, size: 100 },
    ];
    let result = cross_check_source_listing(&listing, &zip_extras_per_index);
    assert!(result.is_err(), "CRC drift between facade listing and ZIP central dir must be rejected");
}
```

- [ ] **Step 3: Run (expect FAIL — helper doesn't exist)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features modify_zip_rejects 2>&1 | tail -15
```

- [ ] **Step 4: Implement the cross-check + extend `ZipSourceExtras`**

`src/modification.rs`:

```rust
pub(crate) struct ZipSourceEntryView {
    pub name: String,
    pub crc32: u32,
    pub size: u64,
    pub compression: zip::CompressionMethod,
}

pub(crate) struct ZipSourceExtras {
    pub archive_comment: Option<Vec<u8>>,
    pub per_index: Vec<ZipSourceEntryView>,  // ordered by ZIP central-dir index
}

fn load_zip_source_extras(path: &Path) -> Result<ZipSourceExtras> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let archive_comment = if zip.comment().is_empty() {
        None
    } else {
        Some(zip.comment().to_vec())
    };
    let mut per_index = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i)?;
        per_index.push(ZipSourceEntryView {
            name: f.name().to_string(),
            crc32: f.crc32(),
            size: f.size(),
            compression: f.compression(),
        });
    }
    Ok(ZipSourceExtras { archive_comment, per_index })
}

pub(crate) fn cross_check_source_listing(
    facade_listing: &[ArchiveEntry],
    zip_extras: &[ZipSourceEntryView],
) -> Result<()> {
    // The facade's listing comes from libarchive (or whatever read backend
    // is in use); zip_extras comes from a side-car `zip` crate read of the
    // same source file. Their order must agree; their (name, crc32, size)
    // tuples must agree per index.
    if facade_listing.len() != zip_extras.len() {
        return Err(ArchiveError::format(format!(
            "ZIP listing drift: facade reports {} entries, central dir has {}",
            facade_listing.len(), zip_extras.len()
        )));
    }
    for (i, (facade, zip)) in facade_listing.iter().zip(zip_extras.iter()).enumerate() {
        if facade.path != zip.name {
            return Err(ArchiveError::format(format!(
                "ZIP listing drift at index {}: facade='{}', central dir='{}'",
                i, facade.path, zip.name
            )));
        }
        if let Some(crc) = facade.crc32 {
            if crc != zip.crc32 {
                return Err(ArchiveError::format(format!(
                    "ZIP listing CRC drift at index {} ({}): facade=0x{:08x}, central dir=0x{:08x}",
                    i, facade.path, crc, zip.crc32
                )));
            }
        }
        if let Some(size) = facade.size {
            if size != zip.size {
                return Err(ArchiveError::format(format!(
                    "ZIP listing size drift at index {} ({}): facade={}, central dir={}",
                    i, facade.path, size, zip.size
                )));
            }
        }
    }
    Ok(())
}
```

In `commit_changes`:

```rust
let zip_extras = if self.format == ArchiveFormat::Zip
    && matches!(new_archive.backend, ArchiveBackend::ZipWriter(_))
{
    let extras = load_zip_source_extras(&self.path)?;
    let listing = self.list_files()?;
    cross_check_source_listing(&listing, &extras.per_index)?;  // <-- new
    Some(extras)
} else {
    None
};

// Compression override now uses .per_index instead of a HashMap:
let compression_override = zip_extras.as_ref()
    .and_then(|extras| extras.per_index.get(entry.id).map(|v| v.compression));
```

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features modify_zip_rejects 2>&1 | tail -10
```

- [ ] **Step 6: Lint + full-suite test**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -15
```

- [ ] **Step 7: Commit**

```bash
git add src/modification.rs tests/integration/modification.rs
git commit -m "fix(modify): cross-check ZIP source listing against central dir (OI-0069-002 / R0069-0064 + R0075-0034)"
```

## Task 2.3: Update OI-0069-002 status

**Files:**
- Modify: `docs/project/open-issues.md`

- [ ] **Step 1: Update OI-0069-002 status**

Mark items resolved:
- R0069-0006 → resolved by Task 2.1
- R0069-0064 → resolved by Task 2.2
- R0069-0003 → still open, scheduled for Phase 7
- R0069-0063 → still open, scheduled for Phase 9

- [ ] **Step 2: Update OI-0075-004 status (R0075-0034 resolved alongside R0069-0064)**

Strike R0075-0034 from the Items table; record resolution.

- [ ] **Step 3: Commit**

```bash
git add docs/project/open-issues.md
git commit -m "docs(oi): close R0069-0006 + R0069-0064 + R0075-0034 (Phase 2 of deferred-OI plan)"
```

---

# PHASE 3 — OI-0075-004 Small Items

Independent of D1/D2. Land first as easy wins.

## Task 3.1: Add `TarZst`, `TarLz4`, `TarLzma`, `Zst`, `Lz4`, `Lzma` to `ArchiveFormat` (R0075-0031)

**Files:**
- Modify: `src/format.rs::ArchiveFormat` enum + every match-arm consumer

- [ ] **Step 1: List every match site**

```bash
grep -rn "ArchiveFormat::" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ --include='*.rs' | grep -v "test\|//" | wc -l
```

Expected: a number; every match needs the new arms (or a `_ => ...` catch-all in places where exhaustive isn't useful). Read the actual list.

- [ ] **Step 2: Write a failing test**

`tests/integration/format.rs` (extend or create):

```rust
#[test]
fn archive_format_recognizes_tar_zst() {
    use unified_archive::ArchiveFormat;
    let fmt = ArchiveFormat::detect_from_path("foo.tar.zst");
    assert_eq!(fmt, Some(ArchiveFormat::TarZst));
}

#[test]
fn archive_format_recognizes_zst() {
    use unified_archive::ArchiveFormat;
    let fmt = ArchiveFormat::detect_from_path("foo.zst");
    assert_eq!(fmt, Some(ArchiveFormat::Zst));
}

#[test]
fn archive_format_recognizes_lz4() {
    use unified_archive::ArchiveFormat;
    assert_eq!(ArchiveFormat::detect_from_path("foo.lz4"), Some(ArchiveFormat::Lz4));
    assert_eq!(ArchiveFormat::detect_from_path("foo.tar.lz4"), Some(ArchiveFormat::TarLz4));
}

#[test]
fn archive_format_recognizes_lzma() {
    use unified_archive::ArchiveFormat;
    assert_eq!(ArchiveFormat::detect_from_path("foo.lzma"), Some(ArchiveFormat::Lzma));
    assert_eq!(ArchiveFormat::detect_from_path("foo.tar.lzma"), Some(ArchiveFormat::TarLzma));
}
```

- [ ] **Step 3: Run (expect compile error — variants missing)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features format 2>&1 | tail -20
```

- [ ] **Step 4: Add the variants**

`src/format.rs`:

```rust
pub enum ArchiveFormat {
    SevenZip,
    Zip,
    Rar,
    Rar5,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZst,        // new
    TarLz4,        // new
    TarLzma,       // new
    Gzip,
    Bzip2,
    Xz,
    Zst,           // new
    Lz4,           // new
    Lzma,          // new
    Iso,
}
```

Update `extensions()`:

```rust
ArchiveFormat::TarZst => &["tar.zst", "tzst"],
ArchiveFormat::TarLz4 => &["tar.lz4"],
ArchiveFormat::TarLzma => &["tar.lzma", "tlz"],
ArchiveFormat::Zst => &["zst"],
ArchiveFormat::Lz4 => &["lz4"],
ArchiveFormat::Lzma => &["lzma"],
```

Update `suffix()`:

```rust
ArchiveFormat::TarZst => ".tar.zst",
ArchiveFormat::TarLz4 => ".tar.lz4",
ArchiveFormat::TarLzma => ".tar.lzma",
ArchiveFormat::Zst => ".zst",
ArchiveFormat::Lz4 => ".lz4",
ArchiveFormat::Lzma => ".lzma",
```

Update `can_create()`:

```rust
ArchiveFormat::TarZst | ArchiveFormat::TarLz4 | ArchiveFormat::TarLzma => true,  // libarchive supports
ArchiveFormat::Zst | ArchiveFormat::Lz4 | ArchiveFormat::Lzma => false,  // raw streams: read-only, like Gzip/Bzip2/Xz
```

Update `can_modify()`:

```rust
// Same as TarGzip/TarBzip2/TarXz: false (libarchive reads, but modify needs in-place rewrite which the compressed-tar path doesn't support).
ArchiveFormat::TarZst | ArchiveFormat::TarLz4 | ArchiveFormat::TarLzma => false,
ArchiveFormat::Zst | ArchiveFormat::Lz4 | ArchiveFormat::Lzma => false,
```

Update `path_extension_claims_compressed_tar()`:

```rust
fn path_extension_claims_compressed_tar(path: &Path) -> Option<ArchiveFormat> {
    let s = path.file_name()?.to_str()?.to_lowercase();
    if s.ends_with(".tar.gz")  || s.ends_with(".tgz")  { Some(ArchiveFormat::TarGzip) }
    else if s.ends_with(".tar.bz2") || s.ends_with(".tbz2") { Some(ArchiveFormat::TarBzip2) }
    else if s.ends_with(".tar.xz")  || s.ends_with(".txz")  { Some(ArchiveFormat::TarXz) }
    else if s.ends_with(".tar.zst") || s.ends_with(".tzst") { Some(ArchiveFormat::TarZst) }    // new
    else if s.ends_with(".tar.lz4")                          { Some(ArchiveFormat::TarLz4) }    // new
    else if s.ends_with(".tar.lzma")|| s.ends_with(".tlz")  { Some(ArchiveFormat::TarLzma) }    // new
    else { None }
}
```

Update libarchive backend mapping (open and create):

```bash
grep -n "ArchiveFormat::TarGzip\|ArchiveFormat::TarBzip2\|ArchiveFormat::TarXz" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/libarchive_wrapper.rs
```

Wherever those branches set the libarchive filter (e.g. `archive_write_add_filter_gzip`), add the matching filters: `archive_write_add_filter_zstd`, `archive_write_add_filter_lz4`, `archive_write_add_filter_lzma`.

For raw `Zst`/`Lz4`/`Lzma` (non-tar), libarchive treats them as compressed-only formats; they're decode-only. Their open path goes through `archive_read_support_filter_zstd` / `archive_read_support_filter_lz4` / `archive_read_support_filter_lzma`. They should be openable but NOT modifiable, NOT creatable (matching Gzip/Bzip2/Xz).

- [ ] **Step 5: Update SFX detection**

`src/sfx/detection.rs` — if it has a magic-byte table, add entries for Zst (`0x28 0xB5 0x2F 0xFD`), Lz4 (`0x04 0x22 0x4D 0x18`), Lzma (`0x5D 0x00 0x00`). Verify whether the existing SFX detection logic includes these.

- [ ] **Step 6: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features format 2>&1 | tail -10
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -15
```

- [ ] **Step 7: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/format.rs src/ffi/libarchive_wrapper.rs src/sfx/detection.rs tests/integration/format.rs
git commit -m "feat(format): add TarZst/TarLz4/TarLzma + Zst/Lz4/Lzma variants (R0075-0031)"
```

## Task 3.2: Size-aware ZIP add_file_from_reader (R0075-0032)

**Files:**
- Modify: `src/ffi/zip_writer.rs`
- Modify: `src/modification.rs::commit_changes`

- [ ] **Step 1: Find the existing `add_file_from_reader`**

```bash
grep -n "fn add_file_from_reader\b" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/zip_writer.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/modification.rs` (extend):

```rust
#[test]
fn modify_zip_rejects_reader_under_producing_against_declared_size() {
    use std::io::Cursor;
    let dir = std::env::temp_dir().join("under_producing");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let archive_path = dir.join("a.zip");
    {
        let opts = unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Zip);
        let mut a = Archive::create(&archive_path, opts).unwrap();
        a.add_file_from_data("seed.txt", b"x").unwrap();
        a.finish().unwrap();
    }

    // Modify mode: declare size=1000 but the reader yields only 5 bytes.
    let mut m = Archive::modify(&archive_path).unwrap();
    let reader = Cursor::new(b"hello".to_vec());
    m.add_entry_from_reader("under.txt", reader, Some(1000)).unwrap();
    let result = m.commit_changes();
    assert!(result.is_err(), "Reader under-producing vs declared size must fail commit");
    let err_str = format!("{:?}", result.unwrap_err());
    assert!(err_str.contains("size") || err_str.contains("Size"));

    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run (expect FAIL — current path silently succeeds with truncated output)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features modify_zip_rejects_reader_under 2>&1 | tail -20
```

- [ ] **Step 4: Add `add_file_from_reader_with_size` to ZipWriter**

`src/ffi/zip_writer.rs`:

```rust
pub fn add_file_from_reader_with_size(
    &mut self,
    archive_path: &str,
    mut reader: impl Read,
    expected_size: u64,
) -> Result<()> {
    // Wrap the reader with a take cap +1 byte (so we can detect over-production).
    use std::io::Read;
    let mut bounded = (&mut reader).take(expected_size + 1);
    let mut bytes_read: u64 = 0;
    let opts = zip::write::FileOptions::default()
        .compression_method(self.compression_for(archive_path))
        .last_modified_time(now_dos_time());
    self.zip.start_file(archive_path, opts)?;
    let mut buf = [0u8; 65_536];
    loop {
        let n = bounded.read(&mut buf)?;
        if n == 0 { break; }
        bytes_read += n as u64;
        self.zip.write_all(&buf[..n])?;
        if bytes_read > expected_size {
            return Err(ArchiveError::format(format!(
                "Reader for '{}' over-produced: declared size {}, read at least {}",
                archive_path, expected_size, bytes_read
            )));
        }
    }
    if bytes_read != expected_size {
        return Err(ArchiveError::format(format!(
            "Reader for '{}' under-produced: declared size {}, actual {}",
            archive_path, expected_size, bytes_read
        )));
    }
    Ok(())
}
```

- [ ] **Step 5: Route `EntrySource::Reader { size: Some(n) }` through the new path**

`src/modification.rs::commit_changes` — for `Reader { size: Some(n) }` with ZipWriter target, call `add_file_from_reader_with_size`. Existing under-/over-production checks elsewhere (libarchive path) stay; this is the ZIP-specific addition.

- [ ] **Step 6: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features modify_zip_rejects_reader_under 2>&1 | tail -10
```

- [ ] **Step 7: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/ffi/zip_writer.rs src/modification.rs tests/integration/modification.rs
git commit -m "fix(zip): size-aware add_file_from_reader rejects under/over-production (R0075-0032)"
```

## Task 3.3: Document `permissions` Unix-bits-only contract (R0075-0079)

**Files:**
- Modify: `src/entry.rs::ArchiveEntry::permissions` rustdoc

- [ ] **Step 1: Read current rustdoc**

```bash
sed -n '60,130p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/entry.rs
```

- [ ] **Step 2: Tighten the rustdoc**

```rust
pub struct ArchiveEntry {
    // ...
    /// Unix permission bits (mode & 0o7777) when known. Always
    /// represented as Unix permission bits regardless of the source
    /// archive's host platform — backends mask file-type bits and
    /// platform-specific attribute bits before populating this field
    /// (R0075-0024 / 0044 / 0045 / 0056). Windows-only attributes
    /// (`FILE_ATTRIBUTE_*`) live in [`Self::attributes.windows`].
    ///
    /// `None` when the source archive doesn't carry permission
    /// information (e.g. raw gzip/bzip2/xz streams, RAR archives
    /// stored on a Windows host that record no Unix mode).
    pub permissions: Option<u32>,
    // ...
}
```

- [ ] **Step 3: Add a regression test asserting the contract**

`tests/integration/permissions_contract.rs` (new):

```rust
use unified_archive::{Archive, ArchiveEntry};

#[test]
fn permissions_is_unix_bits_only_for_zip() {
    let archive = Archive::open("tests/fixtures/zip/test_basic.zip").unwrap();
    for entry in archive.list_files().unwrap() {
        if let Some(perms) = entry.permissions {
            assert_eq!(perms & !0o7777, 0,
                "permissions field on {} carries non-Unix bits: {:o}",
                entry.path, perms);
        }
    }
}

#[test]
fn permissions_is_unix_bits_only_for_libarchive_tar() {
    let archive = Archive::open("tests/fixtures/tar/test_basic.tar").unwrap();
    for entry in archive.list_files().unwrap() {
        if let Some(perms) = entry.permissions {
            assert_eq!(perms & !0o7777, 0);
        }
    }
}

#[cfg(feature = "rar-support")]
#[test]
fn permissions_is_unix_bits_only_for_rar() {
    let path = std::path::Path::new("tests/fixtures/rar/test.rar");
    if !path.exists() { return; }
    let archive = Archive::open(path).unwrap();
    for entry in archive.list_files().unwrap() {
        if let Some(perms) = entry.permissions {
            assert_eq!(perms & !0o7777, 0);
        }
    }
}

#[test]
fn permissions_is_unix_bits_only_for_seven_zip() {
    let archive = Archive::open("tests/fixtures/seven_zip/test.7z").unwrap();
    for entry in archive.list_files().unwrap() {
        if let Some(perms) = entry.permissions {
            assert_eq!(perms & !0o7777, 0);
        }
    }
}
```

Add `mod permissions_contract;` to integration entry point.

- [ ] **Step 4: Run (expect PASS — already true after Review 0075 fixes)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features permissions_contract 2>&1 | tail -10
```

If any backend fails: fix the backend's permissions field to mask off non-Unix bits.

- [ ] **Step 5: Commit**

```bash
git add src/entry.rs tests/integration/permissions_contract.rs tests/integration/main.rs
git commit -m "docs(entry): lock permissions Unix-bits-only contract + regression test (R0075-0079)"
```

## Task 3.4: Close R0075-0034 status (already done in Phase 2)

**Files:**
- (no code change; covered in Task 2.3)

- [ ] **Step 1: Verify Task 2.3 already updated open-issues.md for R0075-0034**

If yes, skip. If no, edit OI-0075-004 entry's table to mark R0075-0034 resolved.

---

# PHASE 4 — OI-0075-004 SFX/RAR Internals

## Task 4.1: SFX staging cancel/progress (R0075-0003)

**Files:**
- Modify: `src/options.rs::ExtractionOptions` (add `staging_progress`)
- Modify: `src/archive.rs::stage_sfx_payload` (accept progress callback)

- [ ] **Step 1: Read current `stage_sfx_payload`**

```bash
sed -n '237,310p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/archive.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/sfx_staging_progress.rs` (new):

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use unified_archive::Archive;

#[test]
fn sfx_open_invokes_staging_progress_callback() {
    // Build an SFX file with a 1 MiB stub + small zip payload.
    let dir = std::env::temp_dir().join("sfx_staging_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("payload.zip");
    let mut a = Archive::create(&zip_path, unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Zip)).unwrap();
    a.add_file_from_data("test.txt", b"hi").unwrap();
    a.finish().unwrap();
    let payload = std::fs::read(&zip_path).unwrap();
    let mut sfx = vec![0u8; 1 << 20];  // 1 MiB stub
    sfx.extend_from_slice(&payload);
    let sfx_path = dir.join("test.sfx.exe");
    std::fs::write(&sfx_path, &sfx).unwrap();

    let bytes_seen = Arc::new(AtomicU64::new(0));
    let bytes_seen_cb = Arc::clone(&bytes_seen);
    let cb = unified_archive::SfxStagingProgress::new(move |bytes| {
        bytes_seen_cb.store(bytes, Ordering::Release);
    });

    let archive = Archive::open_with_sfx_progress(&sfx_path, Some(cb)).unwrap();
    let total = bytes_seen.load(Ordering::Acquire);
    assert!(total >= payload.len() as u64,
        "Staging progress should have observed ≥ payload size, got {total}");
    drop(archive);
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run (expect FAIL — `open_with_sfx_progress` doesn't exist)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features sfx_open_invokes_staging 2>&1 | tail -15
```

- [ ] **Step 4: Add the new types and method**

`src/options.rs`:

```rust
/// Callback invoked during SFX staging. Receives cumulative bytes copied.
/// Return `false` to request cancellation; the staging copy aborts and
/// `Archive::open` returns `Cancelled`.
pub struct SfxStagingProgress {
    pub(crate) cb: Box<dyn FnMut(u64) -> bool + Send>,
}

impl SfxStagingProgress {
    pub fn new(cb: impl FnMut(u64) + Send + 'static) -> Self {
        let mut cb = cb;
        Self { cb: Box::new(move |b| { cb(b); true }) }
    }

    pub fn with_cancel(cb: impl FnMut(u64) -> bool + Send + 'static) -> Self {
        Self { cb: Box::new(cb) }
    }

    pub(crate) fn emit(&mut self, bytes: u64) -> bool {
        (self.cb)(bytes)
    }
}
```

`src/error.rs` — add `Cancelled` variant if missing:

```rust
ArchiveError::Cancelled { operation: &'static str },
```

`src/archive.rs`:

```rust
impl Archive {
    pub fn open_with_sfx_progress(
        path: impl AsRef<Path>,
        progress: Option<SfxStagingProgress>,
    ) -> Result<Self> {
        // Same body as open_at_offset / SFX path, but stage_sfx_payload
        // is called with progress.
        // ...
    }
}

pub(crate) fn stage_sfx_payload(
    path_ref: &Path,
    offset: u64,
    format_hint: Option<ArchiveFormat>,
    prefix: &str,
    mut progress: Option<&mut SfxStagingProgress>,
) -> Result<tempfile::TempPath> {
    let payload_size = std::fs::metadata(path_ref)?.len() - offset;
    let src = std::fs::File::open(path_ref)?;
    let mut bounded = (&src).take(payload_size);
    let dest = tempfile::Builder::new().prefix(prefix).tempfile()?;
    let mut writer = std::io::BufWriter::new(dest.as_file());
    let mut buf = [0u8; 64 * 1024];
    let mut copied: u64 = 0;
    loop {
        let n = bounded.read(&mut buf)?;
        if n == 0 { break; }
        writer.write_all(&buf[..n])?;
        copied += n as u64;
        if let Some(p) = progress.as_mut() {
            if !p.emit(copied) {
                return Err(ArchiveError::Cancelled { operation: "sfx_staging" });
            }
        }
    }
    if copied != payload_size {
        return Err(ArchiveError::format(format!(
            "SFX staging short copy: expected {payload_size}, got {copied}"
        )));
    }
    drop(writer);
    Ok(dest.into_temp_path())
}
```

`Archive::open` keeps the no-progress shape and calls `stage_sfx_payload(..., None)`.

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features sfx_open_invokes_staging 2>&1 | tail -10
```

- [ ] **Step 6: Add a cancellation test**

```rust
#[test]
fn sfx_open_can_be_cancelled_via_progress() {
    // Use a 100 MiB stub so the staging copy has time to be cancelled.
    let dir = std::env::temp_dir().join("sfx_cancel_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("payload.zip");
    let mut a = Archive::create(&zip_path, unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::Zip)).unwrap();
    a.add_file_from_data("test.txt", &vec![b'x'; 100_000_000]).unwrap();
    a.finish().unwrap();
    let payload = std::fs::read(&zip_path).unwrap();
    let mut sfx = vec![0u8; 100 << 20];
    sfx.extend_from_slice(&payload);
    let sfx_path = dir.join("big.sfx.exe");
    std::fs::write(&sfx_path, &sfx).unwrap();

    let cb = unified_archive::SfxStagingProgress::with_cancel(|bytes| bytes < (10 << 20));  // cancel after 10 MiB
    let result = Archive::open_with_sfx_progress(&sfx_path, Some(cb));
    assert!(matches!(result, Err(unified_archive::ArchiveError::Cancelled { operation: "sfx_staging" })));
    let _ = std::fs::remove_dir_all(&dir);
}
```

Run + verify pass. (Test creates a large file; consider gating with `#[cfg_attr(not(target_os = "linux"), ignore)]` if the macOS CI is flaky.)

- [ ] **Step 7: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/options.rs src/archive.rs src/error.rs tests/integration/sfx_staging_progress.rs
git commit -m "feat(sfx): staging progress + cancellation hook (R0075-0003)"
```

## Task 4.2: Streaming RAR5-vint parser (R0075-0061 / R0075-0062)

**Files:**
- Rewrite: `src/ffi/wrapper.rs::parse_rar5_recovery` + `recovery_percentage`

- [ ] **Step 1: Read current implementation**

```bash
sed -n '230,330p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ffi/wrapper.rs
```

- [ ] **Step 2: Read RAR5 spec for vint encoding**

RAR5 vints are LEB128-style: each byte's MSB signals continuation, lower 7 bits are payload. Maximum 10 bytes (covers u64).

- [ ] **Step 3: Write a failing test**

`tests/integration/rar5_recovery_vint.rs` (new):

```rust
use unified_archive::ffi::wrapper::parse_rar5_vint;  // pub(crate); test from src/ffi/wrapper/tests.rs instead

#[test]
fn rar5_vint_one_byte() {
    let mut cursor = std::io::Cursor::new(vec![0x05]);
    assert_eq!(parse_rar5_vint(&mut cursor).unwrap(), 5);
}

#[test]
fn rar5_vint_two_bytes() {
    // 0x80 | 0x01, 0x01 = 1 + 128 = 129
    let mut cursor = std::io::Cursor::new(vec![0x81, 0x01]);
    assert_eq!(parse_rar5_vint(&mut cursor).unwrap(), 0x81);  // 1 + (1 << 7) = 129 (= 0x81)
}

#[test]
fn rar5_vint_max_10_bytes() {
    // 10 bytes of 0xFF, last byte 0x7F.
    let mut bytes = vec![0xFF; 9];
    bytes.push(0x7F);
    let mut cursor = std::io::Cursor::new(bytes);
    assert!(parse_rar5_vint(&mut cursor).is_ok());
}

#[test]
fn rar5_vint_reject_overflow_more_than_10_bytes() {
    let mut bytes = vec![0xFF; 10];
    bytes.push(0x01);
    let mut cursor = std::io::Cursor::new(bytes);
    assert!(parse_rar5_vint(&mut cursor).is_err());
}
```

Since the function lives at `pub(crate)` scope, write tests inline in `src/ffi/wrapper.rs::tests` rather than as integration tests.

- [ ] **Step 4: Run (expect FAIL — function doesn't exist)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features --features rar-support rar5_vint 2>&1 | tail -15
```

- [ ] **Step 5: Implement the parser**

`src/ffi/wrapper.rs`:

```rust
/// Parse a RAR5 variable-length integer (LEB128-style). Returns the
/// decoded u64. Caps at 10 bytes; longer sequences are rejected.
pub(crate) fn parse_rar5_vint<R: std::io::Read>(reader: &mut R) -> Result<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    for _ in 0..10 {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf).map_err(|e| ArchiveError::format(format!("vint read: {e}")))?;
        let byte = buf[0];
        result |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 64 {
            return Err(ArchiveError::format("RAR5 vint exceeds 64-bit range"));
        }
    }
    Err(ArchiveError::format("RAR5 vint exceeds 10 bytes"))
}

impl UnrarArchive {
    fn parse_rar5_recovery(&self, file: &mut std::fs::File) -> Result<Option<u8>> {
        // Walk the RAR5 main header looking for the recovery service header.
        // Pseudocode:
        // 1. Skip the 8-byte signature.
        // 2. Read each block header: vint(crc), vint(header_size), vint(header_type),
        //    vint(flags), [optional extra_area_size if flags bit set],
        //    [optional data_size if flags bit set].
        // 3. If header_type == 0x05 (recovery service header), read the
        //    recovery percentage (1 byte after the header proper).
        // 4. Stop when the main header (type=1) is hit and recovery wasn't seen.

        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(0))?;
        let mut sig = [0u8; 8];
        file.read_exact(&mut sig)?;
        if &sig != b"Rar!\x1A\x07\x01\x00" {
            return Ok(None);  // Not RAR5.
        }
        loop {
            let _crc = match parse_rar5_vint(file) {
                Ok(v) => v,
                Err(_) => return Ok(None),  // EOF or malformed; treat as no recovery.
            };
            let header_size = parse_rar5_vint(file)?;
            let block_start = file.stream_position()?;
            let header_type = parse_rar5_vint(file)?;
            let flags = parse_rar5_vint(file)?;
            let _extra_area_size = if flags & 0x01 != 0 { parse_rar5_vint(file)? } else { 0 };
            let _data_size = if flags & 0x02 != 0 { parse_rar5_vint(file)? } else { 0 };

            if header_type == 0x05 {
                // Recovery service header. Read the recovery percentage byte.
                let mut perc = [0u8; 1];
                file.read_exact(&mut perc)?;
                return Ok(Some(perc[0]));
            }
            // Skip remainder of this header + its data.
            let consumed = file.stream_position()? - block_start;
            let to_skip = header_size.saturating_sub(consumed) + _data_size;
            file.seek(SeekFrom::Current(to_skip as i64))?;
        }
    }
}
```

- [ ] **Step 6: Update `recovery_percentage`**

Replace the byte-scan heuristic at `src/ffi/wrapper.rs:232` with:

```rust
pub fn recovery_percentage(&self) -> Result<Option<u8>> {
    let mut file = std::fs::File::open(&self.path)?;
    self.parse_rar5_recovery(&mut file)
}
```

(For RAR4 — the older format uses a different recovery layout; the existing fallback for RAR4 stays.)

- [ ] **Step 7: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features --features rar-support rar5 2>&1 | tail -15
```

Also test against any existing RAR fixture with recovery enabled:

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features --features rar-support recovery 2>&1 | tail -15
```

- [ ] **Step 8: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/ffi/wrapper.rs
git commit -m "feat(unrar): streaming RAR5 vint parser for recovery records (R0075-0061 / 0062)"
```

## Task 4.3: SFX result accessors instead of public fields (R0075-0071)

**Files:**
- Modify: `src/sfx/result.rs::SfxDetectionResult` (fields → `pub(crate)`, add accessors)
- Modify: every test/example call site (49+ sites)

- [ ] **Step 1: Find every call site**

```bash
grep -rn "\.is_sfx\|\.archive_format\|\.data_offset\|\.stub_type\|\.confidence" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/ --include='*.rs' | grep -i sfx | wc -l
grep -rn "\.is_sfx\|\.archive_format\|\.data_offset\|\.stub_type\|\.confidence" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/ --include='*.rs' | grep -i sfx | head -30
```

- [ ] **Step 2: Modify the struct**

`src/sfx/result.rs`:

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SfxDetectionResult {
    pub(crate) is_sfx: bool,
    pub(crate) archive_format: Option<ArchiveFormat>,
    pub(crate) data_offset: Option<u64>,
    pub(crate) stub_type: Option<StubType>,
    pub(crate) confidence: f32,
}

impl SfxDetectionResult {
    pub fn is_sfx(&self) -> bool { self.is_sfx }
    pub fn archive_format(&self) -> Option<ArchiveFormat> { self.archive_format }
    pub fn data_offset(&self) -> Option<u64> { self.data_offset }
    pub fn stub_type(&self) -> Option<StubType> { self.stub_type }
    pub fn confidence(&self) -> f32 { self.confidence }
}
```

`is_confirmed`, `payload_coordinates`, `not_sfx`, `probable` stay unchanged.

- [ ] **Step 3: Run (expect compile errors at every `result.is_sfx` site)**

```bash
cargo build --all-features 2>&1 | tail -40
```

- [ ] **Step 4: Mass-rewrite the call sites**

Use `sed` (carefully) or hand-edit:

```bash
# Inside src/, tests/, examples/:
# For each <result>.is_sfx → <result>.is_sfx()
# For each <result>.archive_format → <result>.archive_format()
# (etc.)
```

(Hand-edit is safer; a mass `sed` could mistakenly hit `archive.is_sfx` calls on the `Archive` struct.)

- [ ] **Step 5: Run (expect PASS)**

```bash
cargo build --all-features 2>&1 | tail -10
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -20
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/sfx/result.rs src/ tests/ examples/
git commit -m "refactor(sfx): SfxDetectionResult fields → pub(crate) + accessors (R0075-0071)"
```

---

# PHASE 5 — OI-0075-004 Type-Shape (Breaking, Additive in v0.3)

These are public-API changes that lock in v0.4. The strategy: ship the new shape additively in v0.3 with `#[deprecated]` wrappers on the old shape; v0.4 removes the old shape.

## Task 5.1: ArchiveEntry builder + deprecation of public-field construction (R0075-0078)

**Files:**
- Modify: `src/entry.rs` (add `ArchiveEntryBuilder`, `ArchiveEntry::file()`)
- Test: `tests/integration/entry_builder.rs` (new)

- [ ] **Step 1: Write a failing test**

`tests/integration/entry_builder.rs`:

```rust
use unified_archive::ArchiveEntry;

#[test]
fn archive_entry_builder_constructs_valid_state() {
    let entry = ArchiveEntry::file("test.txt", 0)
        .size(100)
        .crc32(0x12345678)
        .build();
    assert_eq!(entry.path, "test.txt");
    assert_eq!(entry.size, Some(100));
    assert_eq!(entry.crc32, Some(0x12345678));
}

#[test]
fn archive_entry_builder_rejects_empty_path() {
    let result = ArchiveEntry::try_file("", 0);
    assert!(result.is_err(), "Empty path must be rejected at construction");
}

#[test]
fn archive_entry_directory_is_size_none() {
    let entry = ArchiveEntry::dir_at("dir/", 0).build();
    assert_eq!(entry.size, None);
    assert!(matches!(entry.entry_type, unified_archive::EntryType::Directory));
}
```

- [ ] **Step 2: Run (expect FAIL)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features archive_entry_builder 2>&1 | tail -15
```

- [ ] **Step 3: Implement**

`src/entry.rs`:

```rust
pub struct ArchiveEntryBuilder {
    inner: ArchiveEntry,
}

impl ArchiveEntryBuilder {
    pub fn size(mut self, size: u64) -> Self { self.inner.size = Some(size); self }
    pub fn compressed_size(mut self, size: u64) -> Self { self.inner.compressed_size = Some(size); self }
    pub fn modified(mut self, t: SystemTime) -> Self { self.inner.modified = Some(t); self }
    pub fn crc32(mut self, c: u32) -> Self { self.inner.crc32 = Some(c); self }
    pub fn permissions(mut self, p: u32) -> Self { self.inner.permissions = Some(p & 0o7777); self }
    pub fn raw_path(mut self, bytes: Vec<u8>) -> Self { self.inner.raw_path = Some(bytes); self }
    pub fn link_target(mut self, t: String) -> Self { self.inner.link_target = Some(t); self }
    pub fn comment(mut self, c: String) -> Self { self.inner.comment = Some(c); self }
    pub fn attributes(mut self, a: FileAttributes) -> Self { self.inner.attributes = Some(a); self }
    pub fn build(self) -> ArchiveEntry { self.inner }
}

impl ArchiveEntry {
    /// New builder for a file-typed entry. Path must be non-empty.
    pub fn file(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder {
        let path = path.into();
        ArchiveEntryBuilder {
            inner: ArchiveEntry {
                path,
                id,
                entry_type: EntryType::File,
                size: None,
                compressed_size: None,
                modified: None,
                crc32: None,
                permissions: None,
                created: None,
                accessed: None,
                is_encrypted: false,
                comment: None,
                attributes: None,
                raw_path: None,
                link_target: None,
            },
        }
    }

    pub fn try_file(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder> {
        let path = path.into();
        if path.is_empty() {
            return Err(ArchiveError::path("ArchiveEntry path must not be empty"));
        }
        Ok(Self::file(path, id))
    }

    /// New builder for a directory-typed entry.
    pub fn dir_at(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder {
        let path = path.into();
        ArchiveEntryBuilder {
            inner: ArchiveEntry {
                path,
                id,
                entry_type: EntryType::Directory,
                .. // same as file
            },
        }
    }

    /// **Deprecated**: use `ArchiveEntry::file(path, id)` instead. The
    /// public-fields-only construction shape will be removed in v0.4.
    #[deprecated(since = "0.3.1", note = "use ArchiveEntry::file or ArchiveEntry::dir_at")]
    pub fn new(path: String, id: usize) -> Self {
        Self::file(path, id).build()
    }

    /// **Deprecated**: use `ArchiveEntry::dir_at(path, id)` instead.
    #[deprecated(since = "0.3.1", note = "use ArchiveEntry::dir_at")]
    pub fn directory(path: String, id: usize) -> Self {
        Self::dir_at(path, id).build()
    }
}
```

(Public fields stay public for v0.3 — the deprecation warning lands when callers `let _ = entry.size = Some(_)` outside the crate. The fields go `pub(crate)` in v0.4.)

- [ ] **Step 4: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features archive_entry_builder 2>&1 | tail -10
```

- [ ] **Step 5: Migrate internal callers from `ArchiveEntry::new` / `ArchiveEntry::directory` to the builder**

```bash
grep -rn "ArchiveEntry::new\|ArchiveEntry::directory" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/
```

For each site, swap to the builder API. This silences the deprecation warning internally (we're the only legitimate consumer of the builder shape).

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -10
git add src/entry.rs src/ tests/integration/entry_builder.rs
git commit -m "feat(entry): ArchiveEntryBuilder + deprecate public-fields constructor (R0075-0078)"
```

## Task 5.2: Widen `EntryFilter` to `FnMut` (R0075-0080)

**Files:**
- Modify: `src/options.rs::EntryFilter`

- [ ] **Step 1: Write a failing test**

`tests/integration/entry_filter_fnmut.rs` (new):

```rust
use unified_archive::{Archive, ExtractionOptions, EntryFilter};

#[test]
fn entry_filter_supports_mutable_state() {
    let archive = Archive::open("tests/fixtures/zip/test_basic.zip").unwrap();
    let mut counter: u32 = 0;
    let filter: EntryFilter = Box::new(move |_entry| {
        counter += 1;  // requires FnMut
        counter % 2 == 0
    });
    let mut opts = ExtractionOptions::default();
    opts.destination = std::env::temp_dir().join("entry_filter_fnmut_dest");
    let _ = std::fs::remove_dir_all(&opts.destination);
    opts.filter = Some(filter);
    archive.extract_all_with_options(&mut opts).unwrap();
    let _ = std::fs::remove_dir_all(&opts.destination);
}
```

- [ ] **Step 2: Run (expect FAIL — `Fn` rejects mutable closures)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features entry_filter_fnmut 2>&1 | tail -15
```

- [ ] **Step 3: Widen the type alias**

`src/options.rs`:

```rust
pub type EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>;
```

- [ ] **Step 4: Update every call site that invokes the filter**

```bash
grep -rn "options\.filter\|opts\.filter\|filter\.as_ref()\|\.filter(" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/ | head -30
```

For each `if let Some(ref f) = opts.filter { ... f(entry) ... }`, change to `if let Some(ref mut f) = opts.filter { ... f(entry) ... }`.

This is a public-API breaking change (`Fn → FnMut`). Existing callers passing `Box<dyn Fn>` must migrate to `Box<dyn FnMut>` — `Fn: FnMut` so any `Fn` closure satisfies the wider trait, but `Box<dyn Fn>` is not subtype-coercible to `Box<dyn FnMut>` automatically.

Add a migration shim:

```rust
/// Helper: lift a `Fn` closure into the `FnMut`-typed `EntryFilter`.
/// Source-compat shim for callers previously using `Box<dyn Fn>`.
pub fn entry_filter_from_fn<F>(f: F) -> EntryFilter
where F: Fn(&ArchiveEntry) -> bool + Send + 'static
{
    Box::new(f)
}
```

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features entry_filter_fnmut 2>&1 | tail -10
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/options.rs src/extraction.rs src/inspection.rs tests/integration/entry_filter_fnmut.rs
git commit -m "refactor(options): widen EntryFilter to FnMut (R0075-0080)"
```

## Task 5.3: Typed `MultipartLayout` return for `detect_multipart` (R0075-0083)

**Files:**
- Modify: `src/inspection.rs::detect_multipart`

- [ ] **Step 1: Write a failing test**

`tests/integration/multipart_typed.rs` (new):

```rust
use unified_archive::{Archive, MultipartLayout};

#[test]
fn detect_multipart_returns_single_for_non_multipart_zip() {
    let archive = Archive::open("tests/fixtures/zip/test_basic.zip").unwrap();
    let layout = archive.multipart_layout().unwrap();
    assert!(matches!(layout, MultipartLayout::Single { .. }));
}
```

- [ ] **Step 2: Run (expect FAIL — type doesn't exist)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features detect_multipart_returns_single 2>&1 | tail -10
```

- [ ] **Step 3: Add typed enum + new method, deprecate old method**

`src/inspection.rs`:

```rust
#[derive(Debug, Clone)]
pub enum MultipartLayout {
    /// Archive is single-part. `path` is the canonical archive path.
    Single { path: PathBuf },
    /// Archive is multipart. `parts` is the volume list, in volume order.
    Multi { parts: Vec<PathBuf> },
}

impl Archive {
    /// Returns the multipart layout for this archive.
    /// Replaces [`Self::detect_multipart`] (deprecated in v0.3).
    pub fn multipart_layout(&self) -> Result<MultipartLayout> {
        let (is_mp, parts) = self.detect_multipart_inner()?;
        if is_mp {
            Ok(MultipartLayout::Multi { parts })
        } else {
            Ok(MultipartLayout::Single { path: parts.into_iter().next().unwrap_or_else(|| self.path.clone()) })
        }
    }

    /// **Deprecated**: prefer `multipart_layout()` (typed return).
    #[deprecated(since = "0.3.1", note = "use multipart_layout()")]
    pub fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)> {
        self.detect_multipart_inner()
    }
}
```

`detect_multipart_inner` is the (renamed-but-otherwise-identical) impl.

- [ ] **Step 4: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features detect_multipart 2>&1 | tail -10
```

- [ ] **Step 5: Migrate internal callers (only doctests)**

`src/extraction.rs:338`, `src/inspection.rs:418` — update doctests to demonstrate `multipart_layout()`.

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/inspection.rs src/extraction.rs tests/integration/multipart_typed.rs
git commit -m "feat(inspection): typed MultipartLayout return; deprecate detect_multipart (R0075-0083)"
```

## Task 5.4: Streaming `calculate_manifest_digest` (R0075-0084)

**Files:**
- Modify: `src/inspection.rs::calculate_manifest_digest`

- [ ] **Step 1: Read current implementation**

```bash
sed -n '270,360p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/inspection.rs
```

Identify the per-entry archive reopen, if any.

- [ ] **Step 2: Write a benchmark-style test**

`tests/integration/manifest_digest_perf.rs` (new):

```rust
use std::time::Instant;
use unified_archive::Archive;

#[test]
fn manifest_digest_does_not_reopen_archive_per_entry() {
    // Build a 1k-entry tar.gz; manifest_digest must complete in < 5x the
    // time of a single list_files() call (otherwise the implementation
    // is O(n²) — reopening per entry).
    let dir = std::env::temp_dir().join("manifest_digest_perf");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let archive_path = dir.join("big.tar.gz");
    {
        let opts = unified_archive::CompressionOptions::new(unified_archive::ArchiveFormat::TarGzip);
        let mut a = Archive::create(&archive_path, opts).unwrap();
        for i in 0..1000 {
            a.add_file_from_data(&format!("file_{i:04}.txt"), b"x").unwrap();
        }
        a.finish().unwrap();
    }

    let archive = Archive::open(&archive_path).unwrap();
    let t0 = Instant::now();
    let _ = archive.list_files().unwrap();
    let baseline = t0.elapsed();

    let t1 = Instant::now();
    let _ = archive.calculate_manifest_digest().unwrap();
    let digest_time = t1.elapsed();

    assert!(digest_time < baseline * 10,
        "manifest_digest ({digest_time:?}) is > 10x the baseline list_files ({baseline:?}); likely reopening per entry");

    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 3: Run (expect FAIL only if current impl reopens; else passes — check)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features manifest_digest_does_not_reopen 2>&1 | tail -15
```

If it passes today: the current path is already O(n). Document that in the commit and skip implementation. (R0075-0084 was speculative; verify before refactoring.)

- [ ] **Step 4: If failing, refactor to streaming**

Pseudocode:

```rust
pub fn calculate_manifest_digest(&self) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let entries = self.list_files()?;
    let src = self.validated_source();
    for entry in &entries {
        if !entry.entry_type.is_file() { continue; }
        hasher.update(entry.path.as_bytes());
        hasher.update(b"\0");
        let crc = match entry.crc32 {
            Some(c) => c,
            None => entry_crc32_streaming(&src, &entry.path)?,
        };
        hasher.update(&crc.to_be_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}
```

`entry_crc32_streaming` opens a stream via `src.extract_to_stream(&entry.path)`, hashes through it. Token-typed `validated_source()` (already exists per AD 0055) keeps the safety invariant.

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features manifest_digest 2>&1 | tail -10
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/inspection.rs tests/integration/manifest_digest_perf.rs
git commit -m "perf(inspection): streaming traversal in calculate_manifest_digest (R0075-0084)"
```

---

# PHASE 6 — CompressionOptions Builder Split (R0075-0081)

## Task 6.1: Per-format compression option builders

**Files:**
- Modify: `src/options.rs` (introduce `ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions`)

- [ ] **Step 1: Read current `CompressionOptions`**

```bash
sed -n '125,180p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/options.rs
```

- [ ] **Step 2: Write a failing test**

`tests/integration/compression_builder_split.rs` (new):

```rust
use unified_archive::{Archive, ZipCompressionOptions, SevenZCompressionOptions};

#[test]
fn zip_compression_options_is_zip_only() {
    let opts = ZipCompressionOptions::new()
        .level(unified_archive::CompressionLevel::Maximum);
    let archive_path = std::env::temp_dir().join("split_test.zip");
    let _ = std::fs::remove_file(&archive_path);
    let mut a = Archive::create_zip(&archive_path, opts).unwrap();
    a.add_file_from_data("hi.txt", b"hello").unwrap();
    a.finish().unwrap();
    let _ = std::fs::remove_file(&archive_path);
}

#[test]
fn sevenz_compression_options_supports_password() {
    use secstr::SecStr;
    let opts = SevenZCompressionOptions::new()
        .password(SecStr::from("mypass"))
        .level(unified_archive::CompressionLevel::Normal);
    let archive_path = std::env::temp_dir().join("split_test.7z");
    let _ = std::fs::remove_file(&archive_path);
    let mut a = Archive::create_seven_zip(&archive_path, opts).unwrap();
    a.add_file_from_data("hi.txt", b"hello").unwrap();
    a.finish().unwrap();
    let _ = std::fs::remove_file(&archive_path);
}
```

- [ ] **Step 3: Run (expect FAIL)**

- [ ] **Step 4: Add per-format builders**

`src/options.rs`:

```rust
#[derive(Default)]
pub struct ZipCompressionOptions {
    level: CompressionLevel,
    progress: Option<Box<dyn ProgressCallback>>,
    // No password: ZIP encryption goes through a separate `ZipEncryptedOptions`
    // (deferred under DEF-006); see AD 0027 reject for create-time encryption.
}

impl ZipCompressionOptions {
    pub fn new() -> Self { Self::default() }
    pub fn level(mut self, l: CompressionLevel) -> Self { self.level = l; self }
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self { self.progress = Some(p); self }
}

#[derive(Default)]
pub struct SevenZCompressionOptions {
    level: CompressionLevel,
    password: Option<SecStr>,
    progress: Option<Box<dyn ProgressCallback>>,
}

impl SevenZCompressionOptions {
    pub fn new() -> Self { Self::default() }
    pub fn level(mut self, l: CompressionLevel) -> Self { self.level = l; self }
    pub fn password(mut self, p: SecStr) -> Self { self.password = Some(p); self }
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self { self.progress = Some(p); self }
}

#[derive(Default)]
pub struct LibarchiveCompressionOptions {
    format: ArchiveFormat,  // must be a libarchive-creatable format
    level: CompressionLevel,
    progress: Option<Box<dyn ProgressCallback>>,
}

impl LibarchiveCompressionOptions {
    pub fn new(format: ArchiveFormat) -> Self {
        Self { format, level: CompressionLevel::Normal, progress: None }
    }
    pub fn level(mut self, l: CompressionLevel) -> Self { self.level = l; self }
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self { self.progress = Some(p); self }
}
```

`src/archive.rs`:

```rust
impl Archive {
    pub fn create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Self> {
        // Same shape as Archive::create but format-specific.
    }

    pub fn create_seven_zip(path: impl AsRef<Path>, opts: SevenZCompressionOptions) -> Result<Self> {
        // ...
    }

    pub fn create_libarchive(path: impl AsRef<Path>, opts: LibarchiveCompressionOptions) -> Result<Self> {
        // ...
    }

    /// **Deprecated**: prefer the per-format builder
    /// (`create_zip`, `create_seven_zip`, `create_libarchive`).
    #[deprecated(since = "0.3.1", note = "use create_zip / create_seven_zip / create_libarchive")]
    pub fn create(path: impl AsRef<Path>, opts: CompressionOptions) -> Result<Self> {
        match opts.format {
            ArchiveFormat::Zip => Self::create_zip(path, opts.into()),
            ArchiveFormat::SevenZip => Self::create_seven_zip(path, opts.into()),
            f if f.can_create() => Self::create_libarchive(path, LibarchiveCompressionOptions { format: f, level: opts.level, progress: opts.progress }),
            f => Err(ArchiveError::Unsupported { reason: format!("Format {f:?} not creatable") }),
        }
    }
}

impl From<CompressionOptions> for ZipCompressionOptions { /* ... */ }
impl From<CompressionOptions> for SevenZCompressionOptions { /* ... */ }
```

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features compression_builder_split 2>&1 | tail -10
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/options.rs src/archive.rs tests/integration/compression_builder_split.rs
git commit -m "feat(options): per-format compression option builders (R0075-0081)"
```

---

# PHASE 7 — D1 Dispatch Unification (R0069-0003 prerequisite)

The trait `ReadBackend` exists (Phase 0 found it) but `dispatch_extract_core` still uses match arms with per-backend argument shapes. Unify on a single `extract_all(&self, plan: &ExtractionPlan)` trait method.

## Task 7.1: Introduce `ExtractionPlan` struct on `ReadBackend`

**Files:**
- Modify: `src/backend.rs` (add `ExtractionPlan` + new trait method)
- Modify: `src/extraction.rs::dispatch_extract_core`
- Modify: per-backend `extract_all_with_options` wrappers

- [ ] **Step 1: Read current `dispatch_extract_core`**

```bash
sed -n '230,300p' /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/extraction.rs
```

- [ ] **Step 2: Define `ExtractionPlan`**

`src/backend.rs`:

```rust
/// All inputs a backend needs to perform an extraction. Built once
/// per call from `ExtractionOptions` + the active `Archive` state.
pub(crate) struct ExtractionPlan<'a> {
    pub destination: &'a Path,
    pub progress: Option<&'a mut dyn ProgressCallback>,
    pub overwrite: bool,
    pub preserve_permissions: bool,
    pub preserve_times: bool,
    pub verify_crc32: bool,
    pub limits: &'a ExtractionLimits,
    pub selection: Option<&'a HashSet<usize>>,
    pub max_mmap_size: Option<u64>,  // Piz-specific; other backends ignore
    pub max_file_size: Option<u64>,  // libarchive-specific; other backends ignore
    pub max_total_size: Option<u64>, // libarchive-specific; other backends ignore
}

pub(crate) trait ReadBackend {
    // ... existing methods unchanged

    /// Default impl returns `Unsupported`; backends override.
    fn extract_all(&self, _plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        Err(ArchiveError::Unsupported { reason: "extract_all not implemented".into() })
    }
}
```

- [ ] **Step 3: Implement `extract_all` per backend**

For each `impl ReadBackend for FooArchive`, route the trait method to the existing `extract_all_with_options` wrapper. Backends that ignored a `_max_*` field continue to ignore it; the trait is a uniform shape.

- [ ] **Step 4: Collapse `dispatch_extract_core`**

`src/extraction.rs`:

```rust
fn dispatch_extract_core(
    archive: &Archive,
    options: &mut ExtractionOptions,
    selection: Option<&HashSet<usize>>,
) -> Result<Vec<ArchiveWarning>> {
    let backend_view = read_backend_view(&archive.backend).ok_or_else(|| ArchiveError::operation_blocked(
        "extract", "Archive is in Write mode"
    ))?;
    let mut plan = ExtractionPlan {
        destination: &options.destination,
        progress: options.progress.as_deref_mut(),
        overwrite: options.overwrite,
        preserve_permissions: options.preserve_permissions,
        preserve_times: options.preserve_times,
        verify_crc32: options.verify_crc32,
        limits: &options.limits,
        selection,
        max_mmap_size: options.limits.max_mmap_size,
        max_file_size: Some(options.limits.max_file_size),
        max_total_size: Some(options.limits.max_total_size),
    };
    backend_view.extract_all(&mut plan)
}
```

The per-backend match arms disappear from this function.

- [ ] **Step 5: Run (expect PASS)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -20
```

If something fails it's likely a per-backend `extract_all` that doesn't honor the same arg semantics as the old direct call. Fix per backend.

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/backend.rs src/extraction.rs src/ffi/
git commit -m "refactor(backend): trait-based extract_all unifies dispatch (R0069-0003 / D1 completion)"
```

## Task 7.2: Update OI-0069-002 status (R0069-0003)

**Files:**
- Modify: `docs/project/open-issues.md`

- [ ] **Step 1: Mark R0069-0003 resolved**

- [ ] **Step 2: Commit**

```bash
git add docs/project/open-issues.md
git commit -m "docs(oi): close R0069-0003 (Phase 7 D1 dispatch unification)"
```

---

# PHASE 8 — D2 Archive God-Object Split

Per AD 0053 baseline. Lands behind `v2-api` cargo feature; v0.2/v0.3 callers see no change unless they opt in. v0.4 removes the old enum.

## Task 8.1: Add `v2-api` cargo feature

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add the feature**

```toml
[features]
default = ["rar-support", "libarchive", "sfx"]
rar-support = []
libarchive = []
sfx = []
v2-api = []  # new — D2 typed-handle split
```

- [ ] **Step 2: Verify**

```bash
cargo build --no-default-features
cargo build --features v2-api
```

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(cargo): v2-api feature flag for D2 typed-handle split"
```

## Task 8.2: Create `ReadArchive` typed handle

**Files:**
- Create: `src/archive/mode_split.rs` (new module)
- Modify: `src/archive.rs` (add `mod mode_split;`)

- [ ] **Step 1: Define `ReadArchive`**

`src/archive/mode_split.rs`:

```rust
#[cfg(feature = "v2-api")]
pub struct ReadArchive {
    pub(crate) backend: ReadArchiveBackend,
    pub(crate) path: PathBuf,
    pub(crate) format: ArchiveFormat,
    pub(crate) entry_cache: OnceCell<Vec<ArchiveEntry>>,
    pub(crate) _backing_tempfile: Option<tempfile::TempPath>,
}

#[cfg(feature = "v2-api")]
pub(crate) enum ReadArchiveBackend {
    #[cfg(feature = "rar-support")]
    Unrar(Box<UnrarArchive>),
    Piz(Box<PizArchive>),
    SevenZ(Box<SevenZArchive>),
    ZipReader(Box<ZipArchive>),
    Libarchive(Box<LibarchiveArchive>),
    // Note: no ZipWriter variant — the type system enforces no
    // write handle leaks into a read context.
}

#[cfg(feature = "v2-api")]
impl ReadArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> { /* ... */ }
    pub fn open_encrypted(path: impl AsRef<Path>, password: &SecStr) -> Result<Self> { /* ... */ }
    pub fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> { /* ... */ }
    pub fn open_sfx(path: impl AsRef<Path>) -> Result<Self> { /* ... */ }
    pub fn extract_stub(...) -> Result<Self> { /* ... */ }
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> { /* ... */ }
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<Vec<ArchiveWarning>> { /* ... */ }
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> { /* ... */ }
    pub fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> { /* ... */ }
    pub fn test_integrity(&self) -> Result<Vec<String>> { /* ... */ }
    pub fn find_entry(&self, path: &str) -> Result<&ArchiveEntry> { /* ... */ }
    pub fn detect_sfx(&self) -> Result<SfxDetectionResult> { /* ... */ }
    pub fn is_encrypted(&self) -> Result<bool> { /* ... */ }
    pub fn has_recovery_record(&self) -> Result<bool> { /* ... */ }
    pub fn recovery_percentage(&self) -> Result<Option<u8>> { /* ... */ }
    pub fn is_solid(&self) -> Result<bool> { /* ... */ }
    pub fn multipart_layout(&self) -> Result<MultipartLayout> { /* ... */ }
    pub fn validate_integrity(&self) -> Result<ValidationReport> { /* ... */ }
    pub fn calculate_manifest_digest(&self) -> Result<String> { /* ... */ }
    pub fn path(&self) -> &Path { &self.path }
    pub fn format(&self) -> ArchiveFormat { self.format }
}
```

The implementations delegate to the existing free functions or `Archive` impls — D2 is structural, not behavioural. The body of each method matches what `Archive` does today in Read mode.

- [ ] **Step 2: Write a passing test under `--features v2-api`**

`tests/v2_api_compat.rs` (new):

```rust
#![cfg(feature = "v2-api")]
use unified_archive::v2::ReadArchive;

#[test]
fn v2_read_archive_lists_zip() {
    let r = ReadArchive::open("tests/fixtures/zip/test_basic.zip").unwrap();
    let entries = r.list_files().unwrap();
    assert!(!entries.is_empty());
}
```

`src/lib.rs`:

```rust
#[cfg(feature = "v2-api")]
pub mod v2 {
    pub use crate::archive::mode_split::{ReadArchive, ReadArchiveBackend};
}
```

- [ ] **Step 3: Run (expect PASS under v2-api)**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --features v2-api v2_read_archive 2>&1 | tail -10
```

- [ ] **Step 4: Verify default-features build still passes (no v2-api)**

```bash
cargo build --all-features
cargo build --no-default-features
TMPDIR=/Volumes/Temp/claude cargo test 2>&1 | tail -10
```

- [ ] **Step 5: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/archive/mode_split.rs src/archive.rs src/lib.rs tests/v2_api_compat.rs
git commit -m "feat(archive): ReadArchive typed handle behind v2-api feature (D2)"
```

## Task 8.3: Add `WriteArchive` typed handle

**Files:**
- Modify: `src/archive/mode_split.rs`

- [ ] **Step 1: Define `WriteArchive`**

```rust
#[cfg(feature = "v2-api")]
pub struct WriteArchive {
    pub(crate) backend: WriteArchiveBackend,
    pub(crate) path: PathBuf,
    pub(crate) format: ArchiveFormat,
    pub(crate) write_namespace: NamespaceTracker,
    pub(crate) write_poisoned: bool,
    // No `finalized` field — the consuming `finish` makes it unnecessary.
}

#[cfg(feature = "v2-api")]
pub(crate) enum WriteArchiveBackend {
    Zip(Box<ZipWriter>),
    Libarchive(Box<LibarchiveArchive>),  // (libarchive write side; will split with D9 later)
}

#[cfg(feature = "v2-api")]
impl WriteArchive {
    pub fn create(path: impl AsRef<Path>, opts: CompressionOptions) -> Result<Self> { /* ... */ }
    pub fn create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Self> { /* ... */ }
    pub fn create_seven_zip(path: impl AsRef<Path>, opts: SevenZCompressionOptions) -> Result<Self> { /* ... */ }
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> { /* ... */ }
    pub fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> { /* ... */ }
    pub fn add_directory(&mut self, archive_path: &str) -> Result<()> { /* ... */ }
    /// Consumes the writer; finalizes the archive durably.
    pub fn finish(self) -> Result<PathBuf> { /* returns the final archive path */ }
    /// Alias for `finish`. Kept for source-compat with v1.
    pub fn close(self) -> Result<PathBuf> { self.finish() }
    pub fn path(&self) -> &Path { &self.path }
    pub fn format(&self) -> ArchiveFormat { self.format }
}

#[cfg(feature = "v2-api")]
impl Drop for WriteArchive {
    fn drop(&mut self) {
        // Drop without finish: emit warning. Sticky-flag check is gone.
        if !self.write_poisoned {
            eprintln!(
                "unified-archive: WriteArchive for `{}` dropped without finish() — \
                 archive may be incomplete. Call WriteArchive::finish() to commit \
                 durably.",
                self.path.display()
            );
        }
    }
}
```

- [ ] **Step 2: Test**

```rust
#[test]
fn v2_write_archive_round_trip_zip() {
    let path = std::env::temp_dir().join("v2_write_test.zip");
    let _ = std::fs::remove_file(&path);
    let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
    w.add_file_from_data("test.txt", b"hello").unwrap();
    let final_path = w.finish().unwrap();
    assert_eq!(final_path, path);
    let r = ReadArchive::open(&path).unwrap();
    let entries = r.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 3: Implement, test, commit (same pattern as Task 8.2)**

```bash
git add src/archive/mode_split.rs tests/v2_api_compat.rs
git commit -m "feat(archive): WriteArchive typed handle with consuming finish (D2)"
```

## Task 8.4: Add `ModifyArchive` typed handle

**Files:**
- Modify: `src/archive/mode_split.rs`

- [ ] **Step 1: Define `ModifyArchive`**

```rust
#[cfg(feature = "v2-api")]
pub struct ModifyArchive {
    pub(crate) read_side: ReadArchive,        // listing/extraction view
    pub(crate) tracker: ModificationTracker,
    pub(crate) options: ModificationOptions,
    pub(crate) lock: std::fs::File,           // exclusive flock
    pub(crate) write_namespace: NamespaceTracker,
}

#[cfg(feature = "v2-api")]
impl ModifyArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> { /* ... */ }
    pub fn open_with_options(path: impl AsRef<Path>, opts: ModificationOptions) -> Result<Self> { /* ... */ }

    pub fn add_entry(&mut self, archive_path: &str, source: EntrySource) -> Result<()> { /* ... */ }
    pub fn add_entry_from_path(&mut self, archive_path: &str, fs_path: impl AsRef<Path>) -> Result<()> { /* ... */ }
    pub fn add_entry_from_reader<R: Read + Send + 'static>(&mut self, archive_path: &str, reader: R, size: Option<u64>) -> Result<()> { /* ... */ }
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> { /* ... */ }
    pub fn remove_entry_by_path(&mut self, archive_path: &str) -> Result<usize> { /* ... */ }
    pub fn remove_entry_by_id(&mut self, id: usize) -> Result<()> { /* ... */ }
    pub fn clear_operations(&mut self) -> Result<()> { /* ... */ }

    /// Validate without consuming: returns Ok(()) if `commit_changes`
    /// would succeed, or the same error it would have produced. R0075-0039.
    pub fn try_commit_changes(&mut self) -> Result<()> {
        // Build the candidate listing, run namespace + path validation,
        // run the load_zip_source_extras cross-check (Phase 2.2), but
        // don't write the new archive.
        // ...
    }

    /// Consumes the handle; commits durably with rename + parent dir fsync.
    pub fn commit_changes(self) -> Result<PathBuf> { /* ... */ }

    pub fn path(&self) -> &Path { self.read_side.path() }
}

#[cfg(feature = "v2-api")]
impl Drop for ModifyArchive {
    fn drop(&mut self) {
        // No durability boundary at Drop for Modify mode — commit_changes
        // is the only durable path. No warning needed: the original
        // archive is still on disk.
    }
}
```

- [ ] **Step 2: Test (commit_changes round-trip + try_commit_changes)**

```rust
#[test]
fn v2_modify_archive_try_commit_validates_without_writing() {
    let path = std::env::temp_dir().join("v2_modify_test.zip");
    let _ = std::fs::remove_file(&path);
    {
        let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
        w.add_file_from_data("seed.txt", b"x").unwrap();
        w.finish().unwrap();
    }
    let original_modtime = std::fs::metadata(&path).unwrap().modified().unwrap();

    let mut m = ModifyArchive::open(&path).unwrap();
    m.add_entry_from_path("added.txt", "/etc/hostname").unwrap();
    m.try_commit_changes().unwrap();

    // try_commit_changes must NOT have written to disk.
    let new_modtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(original_modtime, new_modtime, "try_commit_changes must not write to disk");

    // Now real commit.
    m.commit_changes().unwrap();
    let r = ReadArchive::open(&path).unwrap();
    assert_eq!(r.list_files().unwrap().len(), 2);
    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 3: Implement, test, commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/archive/mode_split.rs tests/v2_api_compat.rs
git commit -m "feat(archive): ModifyArchive typed handle + try_commit_changes (D2 + R0075-0039)"
```

## Task 8.5: `Archive` becomes deprecated sum type

**Files:**
- Modify: `src/archive.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Wrap the existing struct under v2-api**

`src/archive.rs`:

```rust
#[cfg(feature = "v2-api")]
#[deprecated(since = "0.3.0", note = "use ReadArchive / WriteArchive / ModifyArchive directly")]
pub enum Archive {
    Read(ReadArchive),
    Write(WriteArchive),
    Modify(ModifyArchive),
}

#[cfg(not(feature = "v2-api"))]
pub struct Archive { /* current shape unchanged */ }

#[cfg(feature = "v2-api")]
impl Archive {
    /// Migration shim. Construct with `ReadArchive::open` instead.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Read(ReadArchive::open(path)?))
    }

    pub fn modify(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Modify(ModifyArchive::open(path)?))
    }

    // ... etc; every old method delegates by matching the variant.
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        match self {
            Self::Read(r) => r.list_files().map(|s| s.to_vec()),
            Self::Write(_) => Err(ArchiveError::operation_blocked("list_files", "Archive is in Write mode")),
            Self::Modify(m) => m.read_side.list_files().map(|s| s.to_vec()),
        }
    }
}
```

- [ ] **Step 2: Public re-exports**

`src/lib.rs`:

```rust
#[cfg(feature = "v2-api")]
pub use archive::{Archive, ReadArchive, WriteArchive, ModifyArchive};

#[cfg(not(feature = "v2-api"))]
pub use archive::Archive;
```

- [ ] **Step 3: Run the full test suite under both feature configurations**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --no-default-features 2>&1 | tail -10
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -10
TMPDIR=/Volumes/Temp/claude cargo test --features v2-api 2>&1 | tail -10
```

All pass.

- [ ] **Step 4: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/archive.rs src/lib.rs
git commit -m "feat(archive): deprecated Archive enum delegating to typed handles (v2-api)"
```

---

# PHASE 9 — Post-D2 Finalization

## Task 9.1: Retire sticky flags on the typed types

**Files:**
- Modify: `src/archive/mode_split.rs::WriteArchive` (drop `finalized`)
- Modify: `src/archive.rs::Archive` (sum-type Drop forwards to inner type)

- [ ] **Step 1: Verify the consuming `finish` already eliminates the need**

In Task 8.3 the `finish(self) -> Result<PathBuf>` already consumes `self`, so the `finalized` flag is unnecessary. Confirm Drop on `WriteArchive` only fires when `finish` was NOT called.

- [ ] **Step 2: Update OI-0075-003 status**

```markdown
- **Status:** RESOLVED 2026-04-29 (post-D2: WriteArchive::finish consumes self; sticky flags retired on typed types. Old Archive enum keeps sticky flags for migration window.)
```

- [ ] **Step 3: Commit**

```bash
git add docs/project/open-issues.md
git commit -m "docs(oi): close OI-0075-003 (typed WriteArchive::finish retires sticky flags)"
```

## Task 9.2: Tighten `ValidatedSource` constructor (R0069-0063)

**Files:**
- Modify: `src/extraction.rs::ValidatedSource`

- [ ] **Step 1: Read current shape**

```bash
grep -n "ValidatedSource\|validated_source" /Volumes/Common/QJoon/Unified-Archiver/Unified-Archiver/src/extraction.rs
```

- [ ] **Step 2: Make constructor accept `&ReadArchive` under v2-api**

`src/extraction.rs`:

```rust
#[cfg(feature = "v2-api")]
pub(crate) struct ValidatedSource<'a> {
    archive: &'a ReadArchive,
}

#[cfg(feature = "v2-api")]
impl<'a> ValidatedSource<'a> {
    pub(crate) fn from_listed_archive(archive: &'a ReadArchive) -> Self {
        Self { archive }
    }

    pub(crate) fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.archive.extract_to_memory(file_path)
    }

    pub(crate) fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> {
        self.archive.extract_to_stream(file_path)
    }
}

#[cfg(not(feature = "v2-api"))]
pub(crate) struct ValidatedSource<'a> {
    archive: &'a Archive,  // current shape
}
```

A write-mode handle can't construct a `&ReadArchive`; the loophole closes structurally.

- [ ] **Step 3: Migrate the two internal callers**

`src/modification.rs::commit_changes` and `src/inspection.rs::entry_crc32_for_digest` — both already use the token. Under v2-api they construct from `self.read_side` (in `ModifyArchive`'s case) or `&self.read_side` directly.

- [ ] **Step 4: Update OI-0069-002 status (R0069-0063)**

```markdown
R0069-0063 → RESOLVED 2026-04-29 (post-D2: ValidatedSource constructor takes &ReadArchive under v2-api)
```

- [ ] **Step 5: Run full test suite**

```bash
TMPDIR=/Volumes/Temp/claude cargo test --features v2-api 2>&1 | tail -15
```

- [ ] **Step 6: Lint + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/extraction.rs src/modification.rs src/inspection.rs docs/project/open-issues.md
git commit -m "fix(extraction): ValidatedSource constructor on &ReadArchive (R0069-0063)"
```

## Task 9.3: Final OI status sweep

**Files:**
- Modify: `docs/project/open-issues.md`

- [ ] **Step 1: Mark every closed item with resolution date and commit**

Sweep through every OI entry that landed in this plan. Update Status, Verification checkboxes, and the Resolution line. Move fully-resolved entries to `reviews/Open_Issues_Resolved.md` (per the file's header convention).

- [ ] **Step 2: Verify the design notes are referenced from each parked OI**

OI-0058-001 → references `docs/design-notes/oi-0058-001-feature-footprint.md`
OI-0065-002 → references `docs/design-notes/oi-0065-002-piz-extra-fields.md`
OI-0075-002 → references `docs/design-notes/oi-0075-002-snapshot-caching.md`

- [ ] **Step 3: Commit**

```bash
git add docs/project/open-issues.md reviews/Open_Issues_Resolved.md
git commit -m "docs(oi): final sweep — resolved entries archived, parked entries link design notes"
```

## Task 9.4: Verify the full plan landed clean

**Files:**
- (no edit — verification step)

- [ ] **Step 1: Run all profiles**

```bash
cargo fmt --all --check
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --features v2-api -- -D warnings
TMPDIR=/Volumes/Temp/claude cargo test --no-default-features 2>&1 | tail -15
TMPDIR=/Volumes/Temp/claude cargo test --all-features 2>&1 | tail -15
TMPDIR=/Volumes/Temp/claude cargo test --features v2-api 2>&1 | tail -15
```

All pass.

- [ ] **Step 2: Doc build**

```bash
cargo doc --no-deps --all-features
cargo doc --no-deps --features v2-api
```

No broken intra-doc links.

- [ ] **Step 3: Final summary commit**

```bash
git log --oneline | head -50
```

Squash-merge candidate: this plan can be split per phase if the user prefers small commits, but the per-task commits above are already finely scoped.

---

## Self-Review Checklist

After writing this plan, the author confirmed:

**1. Spec coverage:**
- ✅ OI-0058-001 → Phase 0 design note (Task 0.1)
- ✅ OI-0065-001 → skipped per user routing
- ✅ OI-0065-002 → Phase 0 design note (Task 0.2)
- ✅ OI-0069-002 R0069-0003 → Phase 7 (Task 7.1)
- ✅ OI-0069-002 R0069-0006 → Phase 2 (Task 2.1)
- ✅ OI-0069-002 R0069-0063 → Phase 9 (Task 9.2)
- ✅ OI-0069-002 R0069-0064 → Phase 2 (Task 2.2)
- ✅ OI-0075-001 → Phase 1 (Tasks 1.1–1.6)
- ✅ OI-0075-002 → Phase 0 design note (Task 0.3)
- ✅ OI-0075-003 → Phase 9 (Task 9.1) post-D2 closure
- ✅ OI-0075-004 R0075-0003 → Phase 4 (Task 4.1)
- ✅ OI-0075-004 R0075-0031 → Phase 3 (Task 3.1)
- ✅ OI-0075-004 R0075-0032 → Phase 3 (Task 3.2)
- ✅ OI-0075-004 R0075-0034 → Phase 2 (Task 2.2; same code path as R0069-0064)
- ✅ OI-0075-004 R0075-0039 → Phase 8 (Task 8.4) `try_commit_changes` on `ModifyArchive`
- ✅ OI-0075-004 R0075-0061 → Phase 4 (Task 4.2)
- ✅ OI-0075-004 R0075-0062 → Phase 4 (Task 4.2)
- ✅ OI-0075-004 R0075-0071 → Phase 4 (Task 4.3)
- ✅ OI-0075-004 R0075-0078 → Phase 5 (Task 5.1)
- ✅ OI-0075-004 R0075-0079 → Phase 3 (Task 3.3)
- ✅ OI-0075-004 R0075-0080 → Phase 5 (Task 5.2)
- ✅ OI-0075-004 R0075-0081 → Phase 6 (Task 6.1)
- ✅ OI-0075-004 R0075-0083 → Phase 5 (Task 5.3)
- ✅ OI-0075-004 R0075-0084 → Phase 5 (Task 5.4)

**2. Placeholder scan:** No "TBD", "fill in", "similar to Task N", or "appropriate error handling" remain. Code blocks present everywhere code is required.

**3. Type consistency:** `ReadArchive`, `WriteArchive`, `ModifyArchive`, `ReadArchiveBackend`, `WriteArchiveBackend`, `ExtractionPlan`, `MultipartLayout`, `ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions`, `ArchiveEntryBuilder`, `SfxStagingProgress`, `ValidatedSource`, `ZipSourceEntryView`, `ZipSourceExtras` — used consistently across phases.

**4. Dependency sequencing:**
- Phase 0 (notes): no deps.
- Phase 1 (raw_path): ADR (Task 1.1) precedes backend changes (Tasks 1.2–1.5). Independent of D1/D2.
- Phase 2 (R0069-0006/0064): independent.
- Phase 3 (small items): independent.
- Phase 4 (SFX/RAR): independent.
- Phase 5 (type-shape): R0075-0084 uses `ValidatedSource` which exists pre-D2; tightening to `&ReadArchive` happens in Phase 9.
- Phase 6 (CompressionOptions): independent.
- Phase 7 (D1 dispatch): unblocks R0069-0003.
- Phase 8 (D2 split): unblocks R0075-0039 (Task 8.4 lands it).
- Phase 9 (post-D2): R0069-0063 (Task 9.2), OI-0075-003 closure (Task 9.1).
