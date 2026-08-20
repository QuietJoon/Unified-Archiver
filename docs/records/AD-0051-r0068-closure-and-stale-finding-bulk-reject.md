---
type: ADR
title: "AD 0051: Review 0068 closure — bulk-reject stale findings, accept the residual fixes, supersede AD 0050"
description: "Implemented for Groups A/B/C/E; Group D forward work is tracked in-repo via AD 0053 and its sub-phase records — the out-of-repo plan-file pointer is defunct (2026-08-04 amendment)."
tags: [decision, ADR-0051, ADR-0050, ADR-0014, ADR-0052, R0068-0003, R0068-0004, R0068-0021, R0068-0022, R0068-0001, R0068-0002, R0068-0005, R0068-0006]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0051: Review 0068 closure — bulk-reject stale findings, accept the residual fixes, supersede AD 0050

Status: Active (closure record). Amended 2026-08-04 — the out-of-repo plan-file pointer is defunct; the Group D forward work it carried is tracked in-repo via AD 0053 and the AD 0052/0054/0055/0056/0057 sub-phase records (see the amendment at the end).

## Context and Problem Statement

Found in Review 0068 (90 issues across Critical/High/Medium/Low).
Location: spans most of `src/` plus tests and docs.

Review 0068 was performed under static-only inspection (the reviewer's
own preface notes that `cargo clippy --all-targets --all-features` did
not complete because their workspace contained stale toolchain
artifacts). The decision gate verified each Critical/High claim against
the head-of-branch source; a large majority — 22 of the 30
"Critical"/"High" findings, plus several Mediums — turned out to be
false positives reported against code that earlier review cycles had
already fixed.

The same review surfaced a smaller residual set of legitimate gaps and
a large architectural-refactor surface. The user explicitly routed
every group during the gate's Phase 2 (single-shot routing for both
DEFER items and "too-large-for-one-session" escalation candidates). This
record captures the closure decisions: which findings the gate
discarded as stale, which it fixed inline, which were re-routed to
documentation, and how the architectural pass is sequenced.

## Decision Drivers

* Review 0068 found no new critical regressions. Every "Critical"
  finding was either a stale claim against already-fixed code or a
  design preference whose alternative was already considered in an
  earlier ADR.
* The user routed Group A residual items as **fix or document**, Group
  B as **document the limitation**, Group C as **fix all**, Group E as
  **add the tests**, and Group D (architectural refactors,
  ~22 items) as **do it after A/B/C/E, do not defer** — i.e. forward
  work, not in-session.
* Several of the routed-to-fix items had explicit prior decisions
  (notably R0068-0003 / R0068-0004 against AD 0050 "limits-only by
  design"). The user's Review 0068 routing supersedes those prior
  rejections; this ADR records the supersession so future readers do
  not relitigate.
* Some "ACCEPT & fix" items turned out — on inspection — to be already
  implemented (e.g. R0068-0021 empty-directory parity at
  `libarchive_wrapper.rs`, R0068-0022 symlink-rejection parity, several
  Group-C items). Those are noted below as "no-op verified" rather
  than re-implemented.

## Considered Options

1. Process each of the 90 findings individually: write a per-finding
   ADR, route, fix, archive. Estimated ~3 days of pure ceremony for
   what is ultimately a small substantive change set.
2. Group-batch the findings (gate's Phase 2 routing): one routing
   round, one closure ADR, inline fixes, single archive step. Fast,
   keeps the architectural-refactor work tracked separately in the
   plan file rather than inflating `docs/records/`.
3. Reject the entire review for staleness and ask for a re-run on a
   clean toolchain. Wastes the genuine residual findings the reviewer
   surfaced (raw-signature negative cases, dup-path commit tests,
   open-encrypted error precision, etc.).

## Decision Outcome

ACCEPT: option 2 — group-batch routing with this single closure ADR
and inline fixes. The forward-looking architectural work (Group D from
the gate's plan file) is documented in
`/Users/sg/.claude/plans/group-a-magical-flame.md` and stays out of
`docs/records/` until each sub-phase has its own
landed/rejected outcome.

Status: Implemented for Groups A/B/C/E. Group D forward work remains.

### Implementation

#### Stale findings — discarded (no code change)

| Finding | Why stale (head-of-branch evidence) |
|---|---|
| R0068-0001 / R0068-0002 (open_at_offset path identity) | `archive.rs` already restores `archive.path = path_ref.to_path_buf()` and retains the staged tempfile in `_backing_tempfile`. |
| R0068-0005 (SFX raw-signature validation) | `sfx/detection.rs` already runs per-format gzip/bzip2/xz structural probes; the `_ => len >= 100` arm only fires for genuinely unknown formats. |
| R0068-0006 (missing `validate_file_path`/`validate_directory_path`) | Both helpers exist at `creation.rs:42,54`. |
| R0068-0008 (`stage_suffix_for` tar-only) | Function already preserves any final extension and falls back to `.bin` only when the source has no extension. |
| R0068-0011 / R0068-0012 (commit dup-path collisions) | `modification.rs` already builds a `HashSet` of retained-plus-added paths and rejects collisions before writing the temp archive. |
| R0068-0013 (backup overwrites existing) | Backup creation already uses `OpenOptions::create_new(true)` (O_CREAT\|O_EXCL) and remaps `AlreadyExists`. |
| R0068-0016 (multipart hides dir-read errors) | `inspection.rs::detect_multipart` already propagates `read_dir` errors via `.map_err(...)`. |
| R0068-0017 (numeric split-volume boundary) | `numeric_part_boundary` already enforces `<stem>.<digits>` shape. |
| R0068-0018 (single-file extraction skips archive ratio) | `extract_file()` already calls `check_extraction_safe_with_archive`. |
| R0068-0021 (libarchive recursive drops empty dirs) | Empty leaf directories ARE emitted via `add_directory_entry`. |
| R0068-0022 (libarchive recursive silently skips symlinks) | Already returns a structured `invalid_path` error (matches ZIP loud-rejection). |
| R0068-0036 (finalize logic triplicated) | Single `finalize_write_backend` is called by both `finish()` and `Drop::drop()`; `close()` delegates to `finish()`. |
| R0068-0041 (stale `#[allow(dead_code)]` on `ArchiveMode`) | No suppression present. |
| R0068-0042 (`large_enum_variant` suppressed) | No `#[allow(clippy::large_enum_variant)]`; every backend variant is `Box<T>` per the existing module-level comment. |
| R0068-0048 (`builder()` alias on `CompressionOptions`) | No `builder()` method exists; only `new()`. |
| R0068-0049 (`Clone` silently drops progress) | `CompressionOptions` deliberately does NOT implement `Clone`; `strip_progress()` is the explicit API. |
| R0068-0050 / R0068-0089 (`compression_ratio` inverse / untested) | Method is `#[deprecated]` since 0.2.0; `compression_fraction` and `expansion_ratio` are the supported APIs and are documented as such. |
| R0068-0051 (`./file` accepted on write path) | `validate_archive_internal_path` rejects `Component::CurDir`. |
| R0068-0052 / R0068-0053 / R0068-0054 (silent misuse on `remove_entry`/`pending_operations`/`clear_operations`) | All three return `Result<...>` with structured `OperationBlocked` outside Modify mode. |
| R0068-0055 (temp name replaces extension) | `commit_changes` appends `.tmp.{pid}.{nanos}.{counter}` via `name.push(...)` — the original extension is preserved. |
| R0068-0058 (internal CRC-walking listing footgun) | `list_files_internal` no longer exists in `libarchive_wrapper.rs`. |
| R0068-0059 (probable() can return contradictory result) | `probable()` clamps confidence to `[0.5, 0.99]` per AD 0014; floor cited in the function's rustdoc. |
| R0068-0060 (`scan_for_signatures` unused `_chunk_size`) | Current signature is `scan_for_signatures(buffer: &[u8])`; no chunk_size param. |
| R0068-0062 (dead `archive_error_to_io`) | Not present in `streaming.rs`. |
| R0068-0063 (`AtomicOutputFile::create` hardcoded "extract") | Already takes `op: &'static str`. |

These discards consume no implementation budget. Future reviewers
should run `cargo clippy --all-targets --all-features` before flagging
similar items so the toolchain produces an independent verification.

#### Group A residual fixes — applied

| Finding | Change |
|---|---|
| R0068-0003 / R0068-0004 | `extract_to_memory_with_options` and `extract_to_stream_with_options` now route through `open_archive_for_extraction` when `options.password` is set — the same password path used by `extract_all`/`extract_file`/`extract_some`. **Supersedes AD 0050**, which had rejected this on the grounds that the limits-only contract was documented and the unified-options surface should wait for the post-v0.2 trait-backend refactor. |
| R0068-0007 | `ArchiveFormat::detect_from_bytes` now recognises ISO 9660 by checking `magic[32769..32774] == b"CD001"` when the buffer is large enough; rustdoc states the size requirement explicitly. |
| R0068-0009 | `Archive::modify` now wraps the encryption probe so any `Password`/encrypted-sounding `Format`/`Corruption` raised by the underlying `Archive::open` is remapped to a single `OperationBlocked(MODIFY, "Encrypted archives cannot be modified ...")` reason. Added private `looks_like_encryption` helper. |
| R0068-0020 | `check_overwrite_conflicts` now resolves output paths via `sanitize_entry_path` rather than `validate_entry_path`, so the preflight applies the same component-normalisation + symlink-ancestor escape policy that the real extractors use. |
| R0068-0023 | `ZipWriter::add_directory_recursive` now strips `dir_path.parent()` rather than `dir_path` itself, so ZIP recursive add preserves the source directory's name in archive paths — matching the libarchive backend, `tar`, `zip -r`, and `7z a -r`. Existing test was updated; new tests assert the post-refactor layout per format. |
| R0068-0065 | `AtomicOutputFile::commit` for the overwrite case now routes through a shared `rename_with_overwrite` helper. On Windows the helper uses `MoveFileExW(MOVEFILE_REPLACE_EXISTING \| MOVEFILE_WRITE_THROUGH)`; on Unix it stays on `std::fs::rename`. The previous Windows delete-then-persist path (which left a window where the original could vanish on persist failure) is gone. The same helper is now reused from `modification.rs::commit_changes` (the duplicate `rename_with_overwrite` there was removed). |
| R0068-0069 | `is_encrypted` rustdoc tightened: "best-effort metadata probe; returns `false` for header-encrypted archives that refuse listing without a password — use `open_encrypted` then check for password errors for guaranteed validation". |
| R0068-0070 | `open_encrypted` no longer returns the catch-all `"Format not yet supported"` for non-encryptable formats. TAR variants, Gzip, Bzip2, Xz, ISO each get a precise reason ("Format does not support encryption — open with `Archive::open` instead"). |

In addition to the routed items, `ZipArchive::list_files` now uses
`by_index_raw` so encrypted-ZIP listings succeed without a password —
previously they failed with `UnsupportedArchive("Password required to
decrypt file")`, which made R0068-0080's listing precondition
unreachable. This change was needed for the R0068-0003/0004 fix to
actually function and is captured here for future readers.

#### Group B documentation-only — applied

| Finding | Change |
|---|---|
| R0068-0066 | `find_rar_exe` rustdoc now states the discovery surface (PATH plus two stock install paths) and the limitation that custom installs must be exposed via PATH. USER_MANUAL § "Locating `rar.exe`" added. |
| R0068-0067 / R0068-0068 | `RarCreator::set_password` and `::create` rustdoc, plus a USER_MANUAL § "Password handling caveat", now document that `-hp{password}` exposes the password in the OS process listing while `rar.exe` runs. `SecStr` cannot mask argv on Windows. The in-process `rar-support` feature is recommended for confidential workloads. |

#### Group C residual fixes — applied

| Finding | Change |
|---|---|
| R0068-0019 | `extract_some` rustdoc now documents the selective-ratio caveat (archive-wide compressed denominator vs selected uncompressed numerator) and recommends tightening `max_total_size` for hostile CRC-less inputs. |
| R0068-0047 | `Signature`, `SIGNATURES`, `scan_for_signatures` reduced to `pub(crate)`. `find_first_signature` is `#[cfg(test)] pub(crate)` (used only by its own tests). The `pub use` re-exports in `sfx.rs` are gone. The high-level public SFX surface (`detect_sfx`, `SfxDetectionResult`, `StubType`) is unchanged. |
| R0068-0061 | `StreamingExtractor::take_bounded(self, fallback: u64) -> std::io::Take<Self>` added — clamps reads to the declared `total_size`, falling back to the supplied cap when the size is unknown. The bare `Read` impl is unchanged (would have been a behavior break for existing callers). |
| R0068-0064 | `AtomicOutputFile::inner` is now plain `NamedTempFile` rather than `Option<NamedTempFile>`. `commit` consumes `self` and destructures it directly, eliminating the two `expect("file present until commit()")` calls. |

#### Group E test coverage — added

`tests/review_0068_test.rs` (new file, 19 tests) covers R0068-0078 …
R0068-0089: per-format recursive create layout, password propagation
through the `*_with_options` paths, offset-open path identity,
multipart unreadable-directory I/O surfacing (Unix), SFX raw-signature
negative cases, libarchive empty-directory round-trip, recursive
symlink rejection parity (libarchive + ZIP), commit_changes dup-path
rejection, backup noclobber, `strip_progress` callback semantics, and
the `compression_ratio == compression_fraction` deprecation contract.
R0068-0090 (Windows external-rar-create CI) is not landed — the
repository carries no `.github/workflows/`, so the matrix entry has no
home; tracked instead in the plan file for the next CI bring-up.

#### Group D — partially landed; remainder sequenced as forward work

The gate's plan file
(`/Users/sg/.claude/plans/group-a-magical-flame.md`)
sequences Group D as ten sub-phases (D1 backend traits → D10 large-file
splits). The honest scope estimate is 3–5 weeks of focused work, much
of it touching public API. Each sub-phase lands — or is explicitly
rejected — in its own ADR; this closure ADR does NOT pre-commit the
unlanded ones. Sub-phase status as of this Review-0068 closure pass:

| Sub-phase | Status | Record |
|---|---|---|
| D1 (Backend traits / capability split) | Forward work | Plan file |
| D2 (Archive god-object split) | Forward work; will gate behind `v2-api` feature flag for one minor cycle | Plan file |
| D3 (`_unchecked` → ValidatedSelection token) | Forward work | Plan file |
| D4 (Per-backend handle reuse incl. libarchive memoisation) | Forward work | Plan file |
| D5 (Finalize dedup) | **Already done before review.** Single `finalize_write_backend` is shared by `finish()`, `close()`, and `Drop`. R0068-0036 was a stale finding — closed by AD 0051. | This ADR |
| D6 (Split `security.rs` / `options.rs` / `format.rs`) | Forward work | Plan file |
| D7 (Operations enum / R0068-0045) | **Landed in this pass.** `pub enum Operation` in `error.rs` with `Display` + `From<Operation> for String`; legacy `error::ops::*` constants are now `const` views over `Operation::*.as_str()`. Public re-export added at `unified_archive::Operation`. Existing call sites unchanged. | CHANGELOG, this ADR |
| D8 (Lazy- vs eager-open at backend `open()`) | **Landed in this pass.** Codified via AD 0052; per-backend `open` rustdoc tightened to state validation timing explicitly. Implementation deferred to D1's `validate()` opt-in. | AD 0052 |
| D9 (`LibarchiveArchive` read/write split) | Forward work; depends on D1 | Plan file |
| D10 (Large-file refactors + `#[cfg(test)]` move-out) | Forward work | Plan file |

Until the remaining sub-phases' ADRs exist, those items remain in the
plan file, not in `docs/project/open-issues.md` (per the gate's "no
auto-routing to OI" discipline).

## Consequences

* Good, because the false-positive churn is closed without inflating
  the decision-record corpus with 22 individual reject ADRs.
* Good, because the legitimate residual fixes (password propagation,
  ISO byte detection, symlink-aware preflight, root-preserving
  recursive add, atomic Windows replace, encrypted-listing without
  password, …) all land together with explicit tests.
* Good, because Group D's scope is named honestly (multi-week,
  multi-ADR) instead of accruing as silent "track in OI" debt.
* Bad, because supersession of AD 0050 means the
  `extract_to_{memory,stream}_with_options` API now has a slightly
  different shape (password participates) than the v0.1.x docs
  promised. CHANGELOG entry calls this out so v0.1.x consumers can
  audit their call sites.
* Bad, because the closure ADR is necessarily long. Future per-finding
  ADRs are still preferable when a single decision warrants its own
  trail; the bulk-reject form is reserved for stale-review situations.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — defunct plan-file pointer)

The forward-work pointer `/Users/sg/.claude/plans/group-a-magical-flame.md` (cited with its full
path twice above — in the Decision Outcome and in the Group D section's preamble — and invoked
path-lessly as "the plan file" in Considered Options option 2, in every "Plan file" cell of the
Group D status table, in the Group E note on R0068-0090, and in the Group D closing paragraph) is
**defunct**. It was a session-local planning file in a user home outside this repository: no such
file has ever been tracked here (`git log --all -- '*magical-flame*'` finds nothing; the only
history hit for the slug is this record's own text in commit `985b5f5`), and the directory it
pointed into no longer holds it. That makes every delegation above unresolvable and puts the record
at odds with the durable-on-disk rule the project has since adopted. No content is lost, because
everything this record delegated to that file was brought in-repo:

- **The Group D sequencing (D1–D10)** the plan file carried became the in-repo design baseline
  **AD 0053**, flanked by the per-sub-phase records AD 0052 (D8), AD 0054 (D4 first cut),
  AD 0055 (D3), AD 0056 (D9 deferral), and AD 0057 (D10 deferral) — all landed together as
  commit `985b5f5` ("docs(adr): R0068 D-series architectural decisions (0051-0058)").
- **Current per-sub-phase status** is reconciled in AD 0053's same-date §B amendment rather than
  restated here. In brief: D1/D2/D3 shipped (D2 additively, behind the `v2-api` cargo feature),
  D4 pivoted to the AD 0065 listing-cache baseline with the piz backend removed by DCR-009,
  D9 executed 2026-07-20 (AD 0056 amendment), D10 still deferred (AD 0057). The "Group D forward
  work remains" wording in the Decision Outcome above is therefore stale; the sub-phase records
  are now the authoritative trackers.
- **R0068-0090 (Windows external-rar-create CI)**, which the Group E section says was "tracked
  instead in the plan file for the next CI bring-up", still has no landing: the repository
  carries no `.github/` directory at all. Its in-repo homes are now OI-0065-001 Required Action 4
  ("add a Windows CI job that compiles the crate with default features" — the CI bring-up itself,
  which OI-0081-005's runtime verification also waits on) and OI-0076-006's open checklist item
  "Layout regression test on the Windows external-rar lane" (the `rar.exe` create-path coverage
  this finding actually asked for).

For readers misled by the file's "group-a" slug: the plan file this record cites carried the
R0068 **Group D** sequencing, as the surrounding text states — it is not the later R0069
"Group A" pass, which is separately recorded as AD 0062 (v0.3 API shaping) and closed under
OI-0069-001 (RESOLVED 2026-04-28). The original text stands unedited per the immutability
policy, and this record remains **active** as the Review 0068 closure record.
