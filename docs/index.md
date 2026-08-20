---
okf_version: "0.1"
---
# Documentation Index

## Project control
* [Project Index — Start Here](project-index.md) - Curated entry point for the unified-archive written record.
* [Project Records](project/README.md) - This directory contains planning, intake, gap-tracking, and implementation-status material used during development of unified-archive.
* [Design Baseline](project/design-baseline.md) - Baseline ID: BL-001-retroactive
* [Implementation Impact Report](project/implementation-impact-report.md) - Related DCR(s): none (this is the initial impact report for the retroactive baseline)
* [Implementation Slice Checklists (Retrospective)](project/implementation-slice-checklists.md) - This is a retrospective record.
* [MVP Intake: unified-archive](project/intake.md) - Retroactive intake record.
* [Open Issues](project/open-issues.md) - ACCEPT (explicit user request to track the plan in Open Issues)
* [Open Issues — Resolved Archive](project/open-issues-resolved.md) - Audit-trail archive of resolved OI entries, retired from the active open-issues ledger.
* [Structural Skeleton Plan (Retrospective)](project/skeleton-plan.md) - This is a retrospective record.
* [Project Status](project/status.md) - Design baseline ID: BL-001-retroactive
* [Stub Manifest (Retrospective)](project/stub-manifest.md) - This is a retrospective record.
* [Documentation Taxonomy](project/taxonomy.md) - Approved vocabulary and directory governance for the staged OKF v0.2 migration.
* [Design Change Record](records/DCR-001-in-flight-gap-remediation.md) - Approved.
* [DCR-002: Review 0076 inline fixes — ratio guard hardening, libarchive return-code extension, public-API non-exhaustive markers](records/DCR-002-review-0076-inline-fixes.md) - Multiple ADR-tracked decisions had small steady-state extensions applied
* [DCR-003: Extend `Archive::open` extension-fallback set to LZMA / TAR.LZMA](records/DCR-003-review-0078-lzma-extension-fallback.md) - Archive::open's post-magic-byte fallback consulted only .tar and
* [DCR-004: v2-api typed-handle full parity + unified Drop ownership](records/DCR-004-oi-0076-004-v2-api-parity-and-drop-ownership.md) - The v2-api typed handles (ReadArchive / WriteArchive /
* [DCR-005: UnRAR listing memoised via fresh-handle walk (AD 0065 correction)](records/DCR-005-unrar-listing-memoisation.md) - AD 0065 recorded that UnRAR "already satisfied" the frozen-view caching
* [Bounded streaming errors on over-production instead of silently truncating](records/DCR-006-bounded-streaming-hard-cap.md) - Public extract_to_stream[_with_options] now return a hard-capped StreamingExtractor; io::Take silent truncation retired.
* [Modify mode revalidates locked-file identity before every pathname use](records/DCR-007-modify-identity-revalidation.md) - AD 0009's advisory lock is now paired with dev/ino capture + revalidation so commit cannot touch an unrelated replacement inode.
* [UnRAR lock gains a same-thread re-entrancy sentinel](records/DCR-008-unrar-reentrancy-sentinel.md) - The process-global UnRAR lock now rejects same-thread re-entry with a typed error instead of deadlocking, after the UCM_PROCESSDATA trampoline began running user callbacks under the lock.
* [Dual ZIP backend collapsed to a single `zip`-crate backend (piz removed)](records/DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md) - The piz mmap ZIP reader is removed; the `zip` crate becomes the sole ZIP backend for both encrypted and unencrypted archives. Duplicate-name rejection is re-homed into the ZIP backend and OI-0080-002 (mmap SIGBUS) is eliminated by…
* [Extraction rejects non-regular, non-directory entries on every backend](records/DCR-010-extraction-rejects-special-entries.md) - FIFOs, sockets and device nodes are no longer materialised by the libarchive disk writer and are no longer decoded under file semantics by the 7z backend. The FR-022 skip class widens from links to every unsupported entry kind; the caller-visible warning for the new class is deferred.
* [Digest methods hold CRC-less entries to their declared size exactly](records/DCR-011-digest-exactness-for-crc-less-entries.md) - calculate_content_multiset_digest_and_size and its shims return Corruption for a truncated CRC-less entry instead of digesting the short payload.
* [Deferred OI Closure — Review-0075 Phase 2 Routing Implementation Plan](superpowers/plans/2026-04-29-deferred-oi-closure.md) - Implementation in deferred-OI-closure plan Phase 1.
* `project/phase-state.yaml` - Machine-readable design/implementation state (not an OKF concept).

## Architecture
* [Architecture: unified-archive](architecture/README.md) - Maintainer note: this directory is design and implementation material, not the release-facing contract.
* [ADR / DCR corpus design review — revert, improve, innovate](architecture/adr-dcr-design-review-2026-07-19.md) - Full-corpus critique of all 93 decision records (55 architecture ADs, 30 review-gate MADRs, 8 DCRs) at small and big focus; revert candidates prioritized.
* [ADR / DCR design review — revert / improve / innovate](architecture/decision-review-2026-07-19.md) - Cross-cutting critique of all 93 decision records (57 architecture ADs + 30 review-gate MADRs + 8 DCRs), prioritising reversible decisions. Design-only; no code changes made.
* [Bootstrap Config](architecture/bootstrap-config.md) - This document describes how the project is run locally, how processes are started, and where environment / configuration is sourced from.
* [Config Surface](architecture/config-surface.md) - Configuration categories, ownership, and usage.
* [Dictionary](architecture/dictionary.md) - Project terminology for unified-archive.
* [Effort and Risk](architecture/effort-and-risk.md) - Dependency-ordered implementation slices with risk notes.
* [MVP Scope: unified-archive v0.1.0](architecture/mvp-scope.md) - List archive contents with metadata via unified API (ZIP, RAR/RAR5, 7z, TAR variants, ISO)
* [Persistence and Files](architecture/persistence-and-files.md) - State persistence and file lifecycle design for unified-archive.
* [Rough Schema](architecture/rough-schema.md) - In-memory entity structure for unified-archive.
* [Scenario Matrix](architecture/scenario-matrix.md) - Scenario-to-system mapping for all mandatory MVP scenarios.
* [Verification Matrix](architecture/verification-matrix.md) - Scenario-to-verification mapping.
* [Walkthroughs](architecture/walkthroughs.md) - End-to-end architectural walkthroughs for mandatory scenarios.
* [ZIP backend comparison: piz vs the `zip` crate (dual-backend revert input)](architecture/zip-piz-backend-comparison.md) - Advisory. Read-only investigation feeding the AD 0007 dual-ZIP-backend revert decision.

## Implementation design
* [Module Map](implementation/module-map.md) - Ownership and placement of key structural pieces.
* [Workspace Topology](implementation/workspace-topology.md) - Repo layout, package members, and structural conventions.

## Decisions (ADRs)
* [AD: Single Archive Facade with Backend Enum](records/AD-0001-single-archive-facade-with-backend-enum.md) - Accepted
* [AD: Hybrid Backend Selection](records/AD-0002-hybrid-backend-selection.md) - Accepted
* [AD: Safety Gates Before Extraction](records/AD-0003-safety-gates-before-extraction.md) - Accepted
* [AD: Entry Metadata Caching](records/AD-0004-entry-metadata-caching.md) - Accepted
* [AD: Per-Entry Reopen for Parallel Extraction](records/AD-0005-per-entry-reopen-for-parallel-extraction.md) - Accepted
* [AD: Staged SFX Detection Pipeline](records/AD-0006-staged-sfx-detection-pipeline.md) - Accepted
* [AD: Dual ZIP Backend Strategy](records/AD-0007-dual-zip-backend-strategy.md) - Superseded by its own 2026-07-23 collapse amendment (DCR-009) — the dual piz/zip strategy is retired and the zip crate is the sole ZIP backend.
* [AD: Split Archive Behavior Across Modules](records/AD-0008-split-archive-behavior-across-modules.md) - Accepted
* [AD: Isolate Unsafe Behind Safe Wrappers](records/AD-0009-isolate-unsafe-behind-safe-wrappers.md) - Accepted
* [AD: Unified Stream Checksum Extraction](records/AD-0010-unified-stream-checksum-extraction.md) - Accepted
* [AD: Fix solid-archive parallelism check to use password-aware handle](records/AD-0011-fix-solid-archive-parallelism-check.md) - Superseded by AD 0029 (its own 2026-07-20 amendment records the supersession) — AD 0029's single-pass selective extraction removed the internal parallel branch this fix gated.
* [AD: Treat CRC32 value zero as valid, not absent](records/AD-0012-crc32-zero-is-valid.md) - Implemented
* [AD: Plumb compression levels through to ZIP and 7z creation backends](records/AD-0013-plumb-compression-levels-to-backends.md) - Implemented
* [AD: Reject proposal to validate passwords at open time](records/AD-0014-reject-deferred-password-validation.md) - No change needed
* [AD: SFX iterate all candidate signatures and demote CD signature](records/AD-0015-sfx-iterate-all-candidates-demote-cd-signature.md) - Implemented
* [AD: Rename ShellScript to ScriptInterpreter and add Unknown stub variant](records/AD-0016-rename-shellscript-add-unknown-stub.md) - Implemented
* [AD: Archive creation rejects existing files](records/AD-0017-archive-creation-rejects-existing-files.md) - Implemented
* [AD: Remove standalone gz/bz2/xz support claims](records/AD-0018-remove-standalone-format-claims.md) - Implemented (documentation), tracked (feature)
* [AD: UnRAR FFI calls serialized behind a process-wide mutex](records/AD-0019-unrar-process-wide-mutex.md) - Implemented (Phase A.2 of the in-flight remediation plan).
* [AD: `Archive::modify_with_options` is an additive extension, not a replacement](records/AD-0020-additive-modification-options-extension.md) - Implemented (Phase B.2 of the in-flight remediation plan).
* [AD: Creation-side progress callbacks fire per entry, not per byte](records/AD-0021-per-entry-creation-progress.md) - Implemented (Phase B.1 of the in-flight remediation plan).
* [AD 0029: Extraction API redesign — `extract_some()` as core selective primitive](records/AD-0029-extraction-api-redesign-extract-some.md) - Implemented.
* [AD 0030: Remove `clear_entries()` — dead API surface](records/AD-0030-remove-clear-entries.md) - Implemented; archived 2026-08-07 — clear_entries() is gone from the entire tree, the ruling is discharged and cannot recur as written.
* [AD 0031: Reject cross-document caveat deduplication](records/AD-0031-reject-cross-doc-caveat-deduplication.md) - Implemented.
* [AD: Restore `Result<ResultWithWarnings<()>>` for `extract_all` per AD 0010](records/AD-0033-restore-result-with-warnings-dispatch.md) - Implemented.
* [AD 0035: Defer SevenZ source-side streaming during commit_changes](records/AD-0035-oi-0057-007-sevenz-source-streaming-deferred.md) - OI-0057-007 (in docs/project/open-issues.md) flagged that Archive::commitchanges on a 7z source materialises each retained entry to Vec<u8> before…
* [AD 0036: Reject tempfile::tempdir migration for test scratch paths](records/AD-0036-reject-tempfile-tempdir-for-test-scratch-paths.md) - Review 0061 raised 30 findings (R0061-0094 through R0061-0123) recommending that every std::env::tempdir()-based scratch path across tests/ be…
* [AD 0037: Test-suite consolidation — integration suite is canonical](records/AD-0037-test-suite-consolidation-integration-canonical.md) - Review 0061 raised 11 findings (R0061-0013 through R0061-0023) identifying duplicated tests between three per-format test files…
* [AD 0038: Tighten progress-callback and overwrite contracts](records/AD-0038-tighten-progress-and-overwrite-contracts.md) - Review 0061 raised four findings (R0061-0004, R0061-0005, R0061-0006, R0061-0007) pointing at assertion sites that read "may or may not" — patterns…
* [AD 0039: Hermetic UnRAR build and purge of checked-in build artifacts](records/AD-0039-hermetic-unrar-build-and-artifact-purge.md) - Review 0062 raised three related findings about build.rs and the UnRAR vendored source tree:
* [AD 0040: Cap SFX payload size before copying to disk](records/AD-0040-sfx-payload-size-ceiling.md) - Review 0062 R0062-0004 flagged that Archive::openatoffset() copied the archive payload tail from the SFX file into a temp file before handing the…
* [AD 0041: Remove hardcoded `/Volumes/Temp/claude` from library and examples](records/AD-0041-remove-hardcoded-temp-path-from-library.md) - Review 0062 raised three findings about a host-specific path leaking into shipped artefacts:
* [AD 0042: `password_as_str` returns `Result`; reject non-UTF-8 passwords](records/AD-0042-password-as-str-returns-result-on-non-utf8.md) - Review 0062 R0062-0005 flagged that options::passwordasstr() — the helper that converts Option<SecStr> into Option<&str> for backend FFI calls —…
* [AD 0044: Validate archive-internal paths at the creation/modification facade boundary](records/AD-0044-validate-archive-internal-paths-at-facade-boundary.md) - Review 0063 (R0063-0001 through R0063-0006) flagged that the public write-path APIs — Archive::addfilefromdata, addfilefrompathas, adddirectory,…
* [AD 0046: Reject R0064 Windows-support messaging downgrade cluster](records/AD-0046-reject-r0064-windows-support-messaging-downgrade.md) - Review 0064 raised six Medium findings (R0064-0034 through R0064-0039) asking to downgrade every user-facing mention of Windows support from "present…
* [AD 0047: Content-based manifest_digest on CRC-less formats](records/AD-0047-content-based-manifest-digest-on-crc-less-formats.md) - Archive::calculatemanifestdigest returns a stable, content-identity digest over an archive's entries.
* [AD: Reject R0065-0017 — Windows rename locked-file retry test](records/AD-0049-reject-r0065-windows-rename-retry-test.md) - Closed
* [AD 0050: Reject R0066-0003/0004 — `extract_to_{memory,stream}_with_options` options-ignored is documented](records/AD-0050-reject-r0066-extract-with-options-narrow-parameter.md) - Superseded by AD 0051 — the Review 0068 closure pass wired options.password through extract_to_{memory,stream}_with_options (R0068-0003/0004, R0072-0002), reversing this record's rejection of Option 1; the quoted limits-only rustdoc no longer exists. The narrowing rejection (R0066-0004) is unreversed.
* [AD 0051: Review 0068 closure — bulk-reject stale findings, accept the residual fixes, supersede AD 0050](records/AD-0051-r0068-closure-and-stale-finding-bulk-reject.md) - Implemented for Groups A/B/C/E.
* [AD 0052: Codify lazy-validation semantics for backend `open()` (Review 0068 D8)](records/AD-0052-r0068-d8-codify-lazy-backend-open-validation.md) - Documented; harmonisation deferred to D1.
* [AD 0053: Review 0068 Group D — design baseline for the remaining architectural pass](records/AD-0053-r0068-group-d-architectural-pass-design-baseline.md) - Design baseline.
* [AD 0054: Cache Piz mmap + ZIP `RawZipArchive` handle (D4 first cut)](records/AD-0054-r0068-d4-piz-and-zip-handle-caching.md) - Implemented for Piz and ZIP.
* [AD 0055: `ValidatedSource` token replaces `_unchecked` extraction (D3)](records/AD-0055-r0068-d3-validated-source-token.md) - Implemented.
* [AD 0056: Defer the LibarchiveArchive read/write struct split (D9 deferred)](records/AD-0056-r0068-d9-libarchive-reader-writer-split-deferred.md) - Deferred.
* [AD 0057: Defer D10 large-file refactors and `#[cfg(test)]` move-out](records/AD-0057-r0068-d10-large-file-refactor-deferred.md) - Deferred.
* [AD 0058: Feature-first footprint split and facade crates](records/AD-0058-feature-first-footprint-split-and-facade-crates.md) - Planned
* [AD 0059: Review 0069 closure — accept wide modular-design pass, route by group](records/AD-0059-r0069-wide-modular-design-closure.md) - Implemented for the inline-fix subset.
* [AD 0060: Review 0070 closure — broad modular triage, fix all 92 findings](records/AD-0060-r0070-broad-modular-triage-closure.md) - Implemented inline; Group-A items appended to OI-0069-001 /
* [AD 0061: Review 0071 closure — narrow correctness pass, all 21 findings landed inline](records/AD-0061-r0071-narrow-correctness-closure.md) - Implemented inline.
* [AD 0062: Review 0069 Group A — v0.3 API-shaping plan](records/AD-0062-r0069-group-a-v0.3-api-shaping.md) - Planned (v0.3)
* [AD 0063: Review 0075 — closure record + design positions](records/AD-0063-r0075-closure-and-design-positions.md) - Implemented in Review 0075.
* [AD 0064: Non-UTF-8 path policy — preserve raw bytes (Option A)](records/AD-0064-r0075-non-utf8-path-policy-option-a.md) - Implementation in deferred-OI-closure plan Phase 1.
* [AD 0065: Backend caching baseline — frozen-view at first use (Option A)](records/AD-0065-backend-caching-baseline-option-a.md) - Accepted (2026-04-30) — closes OI-0075-002 R0075-0001 caching contract gap.
* [AD 0066: Path sanitization policy — preserve current "lossy repair" baseline, queue strict-reject opt-in](records/AD-0066-r0076-sanitize-vs-reject-policy-deferred.md) - Accepted (2026-05-01) — closes Review 0076 R0076-0004 routing question.
* [AD 0067: Review 0076 closure and routing record](records/AD-0067-r0076-closure-and-routing-record.md) - Accepted (2026-05-01) — closes the gate workflow for Review 0076
* [Architecture Decisions](architecture/decisions/README.md) - This directory contains architecture decision records for unified-archive.
* [AD 0023: Reject path-reference rewrites inside archived review documents](records/AD-0023-reject-archived-review-path-rewrites.md) - Implemented (no changes required); archived — imported from the pre-consolidation decisions/archive/ directory, no successor exists. Its ruling (append-only corrections to archived documents) still informs store governance.
* [AD 0045: Reject R0063 Low-cluster doc-drift sweep; already covered by OI-0057-009 + post-v0.1.0 banners](records/AD-0045-reject-r0063-low-cluster-doc-drift-already-covered.md) - Archived — imported from the pre-consolidation decisions/archive/ directory, no successor exists. Rejects the R0063 Low-cluster doc-drift sweep (R0063-0011..0121) as already covered by OI-0057-009 and the post-v0.1.0 banners; reviewers still cite it as the governing carve-out.
* [AD 0068: Reject Review 0067 as byte-identical duplicate of Review 0066](records/AD-0068-reject-review-0067-duplicate-of-0066.md) - Implemented (both reviews archived to reviews/reviewed/); archived 2026-08-07 — the ruling is discharged and nothing ever superseded it; the 2026-07-23 migration had mislabelled it superseded. Only in-repo explanation of why the review series skips 0067.
* [AD: `Archive::list_files()` skips the libarchive CRC walk](records/MADR-0001-r045-list-files-skips-crc-walk.md) - Implemented (2026-04-14).
* [AD: RAR extraction progress pre-scan uses an isolated handle](records/MADR-0002-r045-rar-progress-prescan-isolation.md) - Implemented (2026-04-14).
* [AD: Reject bulk per-wrapper documentation entries; document the boundary instead](records/MADR-0003-r045-reject-bulk-internal-type-doc-entries.md) - Implemented (2026-04-14).
* [AD: Reject the 39 "missing source file" issues — reviewer false positives](records/MADR-0004-r045-reject-src-prefix-false-positives.md) - Implemented (2026-04-14, no source changes — decision-only).
* [AD: Early link rejection in modify pipeline](records/MADR-0005-r051-early-link-rejection-in-modify.md) - Archived 2026-08-07 — the ruling was later inverted: the FR-022 policy silently drops link/special entries during the modify rewrite (Option 2, the option this record rejected). See the 2026-08-07 amendment.
* [AD: Manifest digest computes CRC32 from entry content for CRC-less formats](records/MADR-0006-r051-manifest-digest-computes-crc32-from-content.md) - Superseded by AD 0047, which owns content-based CRC32 on CRC-less formats today; the have_metadata_digest() surface recorded here never shipped. See the 2026-08-07 amendment.
* [AD: ZIP AES-256 creation encryption](records/MADR-0007-r051-zip-aes256-creation-encryption.md) - Superseded by MADR-0027 (as amended 2026-07-20) — the claimed implementation never shipped; creation passwords are rejected today. Successor tracking: OI-0081-006.
* [AD: Manifest Digest Error Propagation (R0052-0001)](records/MADR-0008-r052-manifest-digest-error-propagation.md) - Superseded by AD 0047, which owns the no-silent-substitution digest contract today; the have_metadata_digest() extension recorded here never shipped. See the 2026-08-07 amendment.
* [AD: Advisory File Locking for Modify Mode (R0052-0004)](records/MADR-0009-r052-advisory-file-locking-modify-mode.md) - Implemented
* [AD: Wire ResultWithWarnings into extract_all (R0052-0006/R0052-0007)](records/MADR-0010-r052-extraction-warnings-result-with-warnings.md) - Implemented
* [AD: UnsupportedOperation display text — \"cannot be performed\" replaces \"not implemented\"](records/MADR-0011-r0001-unsupported-operation-display-text.md) - Superseded — the Option 2 variant split this record rejected landed via OI-0001-003 (commit c9b6685, 2026-04-17); ArchiveError::UnsupportedOperation and the single Display text this record ruled on no longer exist.
* [AD: Reject non-ZIP creation password at facade boundary](records/MADR-0012-r0001-reject-non-zip-creation-password.md) - Superseded by MADR-0024 — the UnsupportedOperation guard recorded here never shipped; creation passwords are rejected today with OperationBlocked at three layers, and MADR-0027 (as amended 2026-07-20, OI-0081-006) governs the encrypted-creation question overall.
* [AD: Multipart detection heuristic tightening + ZIP sort fix](records/MADR-0013-r0001-multipart-detection-heuristic-tightening.md) - Implemented
* [AD: SfxDetectionResult probable() confidence clamped to [0.0, 0.99]](records/MADR-0014-r0001-sfx-probable-confidence-clamp.md) - Superseded by I3 — confidence clamp retired with the f32 score
* [AD: SFX confirmed detection (confidence 1.0) — known patterns](records/MADR-0015-r0002-sfx-confirmed-detection-patterns.md) - Documented — implementation deferred to future work
* [AD: Eliminate redundant middle open in modify()](records/MADR-0016-r0002-modify-eliminate-redundant-open.md) - Implemented
* [AD: Bit-level BZIP2 EOS marker scanning](records/MADR-0017-r0002-bzip2-bit-level-eos-scanning.md) - Implemented
* [AD: Clippy lint baseline restoration and dead code strategy](records/MADR-0018-r0003-clippy-lint-baseline-and-dead-code-strategy.md) - Implemented
* [AD: Raw format test verification and entry name normalization](records/MADR-0019-r0003-raw-format-verification-and-entry-name-normalization.md) - Implemented
* [AD: Feature-gate UnRAR build behind `rar-support` Cargo feature](records/MADR-0020-r0056-feature-gate-rar-build.md) - Implemented
* [AD: Reject symlinks in ZIP recursive creation](records/MADR-0021-r0056-zip-creation-reject-symlinks.md) - Implemented
* [AD: AD 0007 ZIP-AES Creation Regression — Track as Open Issue](records/MADR-0022-r0057-zip-aes-creation-regression.md) - Superseded by MADR-0027 (as amended 2026-07-20) — OI-0057-001 resolved 2026-04-18; successor tracking: OI-0081-006.
* [AD: `Archive::open_at_offset` Returns `NotImplemented`, Not a Forged-Format `Unsupported`](records/MADR-0023-r0057-open-at-offset-format-agnostic-error.md) - Superseded — Archive::open_at_offset shipped 2026-04-18 (DEF-001 closure); the NotImplemented return this record mandated no longer exists.
* [AD: Non-ZIP Creation Rejects Passwords With `OperationBlocked`](records/MADR-0024-r0057-libarchive-non-zip-passwords-hard-fail.md) - Superseded by MADR-0027, which generalised the rejection to every format including ZIP; the rejection site itself survives. See the 2026-08-07 amendment.
* [AD: Review 0058 Archived as Duplicate of Review 0057](records/MADR-0025-r0057-review-0058-duplicate-of-0057.md) - Implemented.
* [AD: `sanitize_entry_path` No Longer Creates Directories](records/MADR-0026-r0057-sanitize-entry-path-side-effect-removal.md) - Implemented.
* [AD: This Library Does Not Produce Encrypted Archives](records/MADR-0027-reject-encrypted-archive-creation.md) - Implemented.
* [AD: Reject Review 0060 items already tracked by DEF-* / OI-* entries](records/MADR-0028-r0060-reject-already-tracked-gaps.md) - Implemented (review 0060 decisions applied and archived).
* [AD: Reject blanket portability sweep over `/Volumes/Temp/claude/` scratch paths](records/MADR-0029-r0060-reject-scratch-path-portability-sweep.md) - REJECT reversed (2026-07-20 owner reversal); superseded by the consolidated scratch-path policy in AD 0036 (Option 3) and AD 0041 — test scratch literals now route through temp_test_dir().
* [AD: NULL-pathname libarchive headers are uniformly omitted, not format errors](records/MADR-0030-r0080-reject-null-header-format-error.md) - Rejected R0080-0017's recommendation to error on nameless headers — it would reverse the R0079-0022 bijectivity design.
* [AD: Files too small to carry magic bytes fall back to any extension mapping](records/MADR-0031-r0001-tiny-file-extension-precedence.md) - Rejected R0001-0035's recommendation to apply the `is_extension_fallback` whitelist to the sub-four-byte branch of format detection; the tiny-file branch and the post-magic fallback answer different questions and are deliberately governed by different rules.
* [AD: For selective extraction the archive-ratio guard is informational; the byte caps bind](records/MADR-0032-r0001-selective-ratio-denominator.md) - Disposes of R0001-0041: `extract_some` divides the selected uncompressed sum by the whole archive's on-disk size, which is more permissive than a true per-entry ratio on CRC-less formats. The decoded-byte caps are the binding check for subsets; the ratio result is advisory there.
* [Review Decision Records](decisions/README.md) - This directory contains review-driven decision records captured while the library was being stabilized for v0.1.0.
* [Decision records](records/README.md) - Single, unified store for all project decision records (AD / MADR / DCR / legacy DD-IG); index.yaml is the authoritative machine-readable catalogue.
* [DD-003: Review 0003 Improvements](records/DD-003-review-0003-improvements.md) - **Partial Extraction:** Enforced password/limits on partial extraction. **Libarchive Checks:** Added return code checks. **Piz Mmap:** Added platform-specific mmap size limits (4GB/100MB). **Options:** Enforced…
* [DD-004: Review 0004 Improvements](records/DD-004-review-0004-improvements.md) - **Zip Walkdir:** Fixed silent error dropping in Zip writer. **Piz Alloc:** Added allocation guards to prevent OOM on corrupted ZIP headers. **Timestamp Panic:** Fixed panic on pre-epoch timestamps in libarchive.
* [DD-005: Review 0005 Improvements](records/DD-005-review-0005-improvements.md) - Addressed multiple correctness and safety issues: **Windows Rename:** `std::fs::rename` doesn't overwrite on Windows; switched to `MoveFileExW`. **Symlinks:** Enforced consistent skipping of symlinks across backends.…
* [DD-009-001: Zip Path Traversal](records/DD-009-001-zip-path-traversal.md) - The `ZipArchive` backend was not using `sanitize_entry_path` for extracted files, allowing potential path traversal attacks.
* [DD-010-001: Hard Link Skipping](records/DD-010-001-hard-link-skipping.md) - Documentation stated hard links were skipped for security (FR-022), but the implementation only checked for symlinks.
* [DD-010-003: Extract All Finish Entry Errors](records/DD-010-003-extract-all-finish-entry-errors.md) - `extract_all` was ignoring return codes from `archive_write_finish_entry`, potentially masking write failures.
* [DD-011-001: Piz Backend 4GB Limit](records/DD-011-001-piz-backend-4gb-limit.md) - The Piz backend had a hardcoded 4GB limit for memory mapping, which prevented opening large ZIP files on 64-bit systems where virtual address space is plentiful.
* [DD-011-002: Libarchive Walkdir Errors](records/DD-011-002-libarchive-walkdir-errors.md) - `add_directory_recursive` was silently dropping files if `walkdir` encountered an error (e.g., permission denied), leading to incomplete archives without warning.
* [DD-012-001: Libarchive Memory Inefficiency](records/DD-012-001-libarchive-memory-inefficiency.md) - The `add_file_from_path` implementation was reading entire files into memory before writing to the archive, causing O(N) memory usage.
* [DD-012-002: SFX Detection Flaws](records/DD-012-002-sfx-detection-flaws.md) - The SFX detection logic used `read` which could return short reads, and validated offsets against buffer size rather than file size, leading to potential false negatives.
* [DD-012-003: Temp Dir Race Condition](records/DD-012-003-temp-dir-race-condition.md) - The theoretical race condition in temporary directory naming (using `SystemTime`) was deemed extremely unlikely to occur in practice.
* [DD-012-004: Libarchive Creation Timestamp Precision](records/DD-012-004-libarchive-creation-timestamp-precision.md) - Archive creation was truncating timestamps to seconds, losing nanosecond precision provided by the filesystem.
* [DD-013-001: UnRAR Compile Fix](records/DD-013-001-unrar-compile-fix.md) - Fixed a compilation error on non-macOS platforms where `.into_owned()` was called on a `String` (which does not implement it, as it's a `Cow` method).
* [DD-013-002: SevenZ Encryption Flag](records/DD-013-002-sevenz-encryption-flag.md) - The current implementation incorrectly ties the `is_encrypted` flag to the presence of a password in `ExtractionOptions`, rather than checking the actual archive metadata.
* [DD-014: Refactoring Suggestions](records/DD-014-refactoring-suggestions.md) - Multiple refactoring suggestions were proposed (backend traits, deduplication) without functional changes or patches.
* [DD-017-001: ZipWriter OOM Risk](records/DD-017-001-zipwriter-oom-risk.md) - The `add_file_from_path` method in ZipWriter was reading entire files into memory via `std::fs::read()` before writing to the archive.
* [DD-018-001: RAR5 decode_vint Panic Fix](records/DD-018-001-rar5-decode-vint-panic-fix.md) - Critical panic vulnerability in RAR5 variable-length integer decoding.
* [DD-019-001: UnRAR test_integrity Password Fix](records/DD-019-001-unrar-test-integrity-password-fix.md) - The `test_integrity()` method in UnrarArchive was opening a fresh handle using `Self::open()` directly, which does not preserve the password.
* [DD-020-001: RAR5 vint Decoding Algorithm](records/DD-020-001-rar5-vint-decoding-algorithm.md) - Critical algorithm error - the custom "leading bits" implementation was incompatible with RAR5's actual LEB128 encoding.
* [DD-020-002: UnRAR FILETIME Epoch Offset](records/DD-020-002-unrar-filetime-epoch-offset.md) - High-resolution timestamps (mtime/ctime/atime) in RAR archives use Windows FILETIME format based on 1601-01-01.
* [DD-021: Extract-to-Memory OOM Protection](records/DD-021-extract-to-memory-oom-protection.md) - All `extract_to_memory` implementations lacked size validation before allocating buffers.
* [DD-023-001: Libarchive Write-Mode Data Integrity](records/DD-023-001-libarchive-write-mode-data-integrity.md) - Archive creation could report success while writing incomplete entries.
* [DD-024-001: RAR5 parse_rar5_recovery Panic Fix](records/DD-024-001-rar5-parse-rar5-recovery-panic-fix.md) - Critical DoS vulnerability.
* [DD-024-003: OOM Protection for Libarchive and Zip extract_to_memory](records/DD-024-003-oom-protection-for-libarchive-and-zip-extract-to-memory.md) - DD-021 claimed to fix OOM in all extract_to_memory backends but missed the libarchive and native ZIP backends.
* [IG-004-01: Streaming buffers entire file](records/IG-004-01-streaming-buffers-entire-file.md) - Streaming extraction APIs currently read full entries into memory before providing a stream.
* [IG-004-05: bzip2 full file read](records/IG-004-05-bzip2-full-file-read.md) - `extract_bzip2_stream_crc` reads the entire file into memory to find the CRC at the end.
* [IG-005-01: SFX open_at_offset unimplemented](records/IG-005-01-sfx-open-at-offset-unimplemented.md) - `open_sfx` always fails because the underlying `open_at_offset` is unimplemented.
* [IG-005-08: split_size unused](records/IG-005-08-split-size-unused.md) - `CompressionOptions.split_size` is defined but ignored by archive creation.
* [IG-0061-0094..0123: tempfile::tempdir() migration for test scratch paths](records/IG-0061-0094-0123-tempfile-tempdir-migration-for-test-scratch-paths.md) - Reviewer recommended replacing every `std::env::temp_dir()`-based scratch path with `tempfile::tempdir()` for automatic cleanup and isolation.
* [IG-0063-0011..0121: Drifted non-canonical artifact content sweep](records/IG-0063-0011-0121-drifted-non-canonical-artifact-content-sweep.md) - Reviewer flagged every line in the historical planning/spec tree that still describes pre-0.1.0 behavior (e.g. passwords as `Option<String>`, `open_at_offset` deferred, unsupported-operation error names).
* [IG-0064-0034..0039: Windows-support messaging downgrade cluster](records/IG-0064-0034-0039-windows-support-messaging-downgrade-cluster.md) - Reviewer asked to downgrade every user-facing Windows mention from "present but not release-verified" to "not currently supported" or "incomplete," citing two build-script artifacts: `build.rs:52` still prints `Windows…
* [IG-0065-0017: Windows rename locked-file retry test](records/IG-0065-0017-windows-rename-locked-file-retry-test.md) - Reviewer recommended a Windows-only regression test that exercises the locked-destination case on `rename_with_overwrite`.
* [IG-0081-0010: ISO detection requires the primary volume descriptor at sector 16](records/IG-0081-0010-iso-detection-requires-the-primary-volume-descriptor-at-sector.md) - The ISO detector checks the descriptor type/version + `CD001` at sector 16 and does not walk subsequent volume descriptors, so a boot-descriptor-first image with a non-`.iso` name is missed by content detection.
* [IG-0081-0013: `PK\x07\x08` split-ZIP spanning marker rejected at offset zero](records/IG-0081-0013-pk-x07-x08-split-zip-spanning-marker-rejected-at-offset-zero.md) - `PK\x07\x08` is excluded from ZIP start-of-file detection; it is also the spanning marker at the start of a split archive's first segment.
* [IG-0081-0050: ZIP Extended-Timestamp uses signed 32-bit seconds (2038 ceiling)](records/IG-0081-0050-zip-extended-timestamp-uses-signed-32-bit-seconds-2038-ceiling.md) - The finding claims the `0x5455` seconds field is unsigned and that valid post-2038 dates are being wrongly discarded by the signed-`i32` handling.
* [IG-0081-0088: BSD targets omit the C++ runtime for the vendored UnRAR build](records/IG-0081-0088-bsd-targets-omit-the-c-plus-plus-runtime-for-the-vendored.md) - C++ runtime linkage has branches only for macOS (`-lc++`), Linux (`-lstdc++`), and Windows; a native BSD/DragonFly build links no C++ runtime and fails at link time.
* [IG-009-002: ExtractionOptions.filter unused](records/IG-009-002-extractionoptions-filter-unused.md) - `ExtractionOptions.filter` is defined but ignored by all extraction methods.
* [IG-009-003: Libarchive CRC32 partial hashes](records/IG-009-003-libarchive-crc32-partial-hashes.md) - CRC32 listing can return partial hashes if a read error occurs mid-file during listing.
* [IG-010-002: Path sanitization creates directories before check](records/IG-010-002-path-sanitization-creates-directories-before-check.md) - `sanitize_entry_path` calls `create_dir_all(parent)` before verifying the path is within the destination directory.
* [IG-011-003: Empty directories not preserved](records/IG-011-003-empty-directories-not-preserved.md) - Empty directories are not preserved when creating archives using libarchive backend.
* [IG-011-004: extract_to_memory inefficiency](records/IG-011-004-extract-to-memory-inefficiency.md) - Libarchive backend extracts to disk then reads back to memory instead of extracting directly to memory.
* [IG-011-005: sanitize_entry_path side-effects](records/IG-011-005-sanitize-entry-path-side-effects.md) - Potential creation of empty directories outside destination if extraction traverses a symlink.
* [IG-012-003: Temp dir race in extract_to_memory](records/IG-012-003-temp-dir-race-in-extract-to-memory.md) - Potential race condition in temp directory naming using `SystemTime::now()` if multiple calls happen in the same nanosecond.
* [IG-014-001: Manual dispatch in Archive methods](records/IG-014-001-manual-dispatch-in-archive-methods.md) - Code duplication in `Archive` methods (`extract_to_memory`, `extract_to_stream`) which manually match on `backend` enum instead of using a trait abstraction.
* [IG-014-002: Inconsistent extract_to_memory implementations](records/IG-014-002-inconsistent-extract-to-memory-implementations.md) - UnRAR and Libarchive backends implement `extract_to_memory` by extracting to a temp file and reading it back, whereas Piz and SevenZ extract directly to memory.
* [IG-014-003: Redundant extract_to_stream implementation](records/IG-014-003-redundant-extract-to-stream-implementation.md) - `extract_to_stream` implies streaming but currently just wraps `extract_to_memory` result in a Cursor.
* [IG-014-004: ensure_destination utility duplication](records/IG-014-004-ensure-destination-utility-duplication.md) - `ensure_destination` is defined in `src/extraction.rs` but also effectively re-implemented or checked in individual backends (e.g., `src/ffi/libarchive_wrapper.rs` does `create_dir_all` again inside `extract_all`).
* [IG-020-003: UnRAR FFI Struct Layout on Linux](records/IG-020-003-unrar-ffi-struct-layout-on-linux.md) - The code defines `wchar_t` as 2 bytes (UTF-16) for non-macOS platforms.

## Other
* [libzstd-rs-sys (Trifecta Tech Foundation) — adoption evaluation](design-notes/libzstd-rs-sys-evaluation.md) - Status: research note, evaluated 2026-06-10.
* [OI-0058-001 — Feature-first footprint split & facade crates: cons / pros](design-notes/oi-0058-001-feature-footprint.md) - Status: parked design note (not an ADR).
* [OI-0065-002 — Piz reader 0x5455/0x000A extra-field parsing: cons / pros](design-notes/oi-0065-002-piz-extra-fields.md) - Status: parked design note.
* [OI-0075-002 — Snapshot semantics & per-backend caching baseline: cons / pros](design-notes/oi-0075-002-snapshot-caching.md) - Status: parked design note.

* [API Reference](API_REFERENCE.md) - API reference for the unified-archive library.
* [Getting Started with unified-archive](GETTING_STARTED.md) - This guide will walk you through installing and using unified-archive for the first time.
* [unified-archive Documentation](README.md) - Public documentation for unified-archive v0.4.0.
* [Stream Checksums and Archive Integrity](STREAM_CRC32.md) - Comprehensive guide to understanding checksums in compression and archive formats.
* [unified-archive User Manual](USER_MANUAL.md) - This manual is the practical guide for using unified-archive v0.4.0.
* [SFX Module Test Coverage Report](sfx_coverage_report.md) - Date: 2025-11-13
* [Backlog](backlog.md) - Persistent record of WIP / not-yet-implemented / deferred work, derived from docs/project/open-issues.md and the TicGit follow-up backlog. Regenerated by `/reopen`.
