# Decision records — index

Generated view; the authoritative catalogue is [`index.yaml`](index.yaml). See [`README.md`](README.md) for the scheme.

## Architecture Decisions (AD)

- [AD-0001](AD-0001-single-archive-facade-with-backend-enum.md) — AD: Single Archive Facade with Backend Enum
- [AD-0002](AD-0002-hybrid-backend-selection.md) — AD: Hybrid Backend Selection
- [AD-0003](AD-0003-safety-gates-before-extraction.md) — AD: Safety Gates Before Extraction
- [AD-0004](AD-0004-entry-metadata-caching.md) — AD: Entry Metadata Caching
- [AD-0005](AD-0005-per-entry-reopen-for-parallel-extraction.md) — AD: Per-Entry Reopen for Parallel Extraction
- [AD-0006](AD-0006-staged-sfx-detection-pipeline.md) — AD: Staged SFX Detection Pipeline
- [AD-0007](AD-0007-dual-zip-backend-strategy.md) — AD: Dual ZIP Backend Strategy — _superseded_
- [AD-0008](AD-0008-split-archive-behavior-across-modules.md) — AD: Split Archive Behavior Across Modules
- [AD-0009](AD-0009-isolate-unsafe-behind-safe-wrappers.md) — AD: Isolate Unsafe Behind Safe Wrappers
- [AD-0010](AD-0010-unified-stream-checksum-extraction.md) — AD: Unified Stream Checksum Extraction
- [AD-0011](AD-0011-fix-solid-archive-parallelism-check.md) — AD: Fix solid-archive parallelism check to use password-aware handle — _superseded_
- [AD-0012](AD-0012-crc32-zero-is-valid.md) — AD: Treat CRC32 value zero as valid, not absent
- [AD-0013](AD-0013-plumb-compression-levels-to-backends.md) — AD: Plumb compression levels through to ZIP and 7z creation backends
- [AD-0014](AD-0014-reject-deferred-password-validation.md) — AD: Reject proposal to validate passwords at open time
- [AD-0015](AD-0015-sfx-iterate-all-candidates-demote-cd-signature.md) — AD: SFX iterate all candidate signatures and demote CD signature
- [AD-0016](AD-0016-rename-shellscript-add-unknown-stub.md) — AD: Rename ShellScript to ScriptInterpreter and add Unknown stub variant
- [AD-0017](AD-0017-archive-creation-rejects-existing-files.md) — AD: Archive creation rejects existing files
- [AD-0018](AD-0018-remove-standalone-format-claims.md) — AD: Remove standalone gz/bz2/xz support claims
- [AD-0019](AD-0019-unrar-process-wide-mutex.md) — AD: UnRAR FFI calls serialized behind a process-wide mutex
- [AD-0020](AD-0020-additive-modification-options-extension.md) — AD: `Archive::modify_with_options` is an additive extension, not a replacement
- [AD-0021](AD-0021-per-entry-creation-progress.md) — AD: Creation-side progress callbacks fire per entry, not per byte
- [AD-0023](AD-0023-reject-archived-review-path-rewrites.md) — AD 0023: Reject path-reference rewrites inside archived review documents — _archived_
- [AD-0029](AD-0029-extraction-api-redesign-extract-some.md) — AD 0029: Extraction API redesign — `extract_some()` as core selective primitive
- [AD-0030](AD-0030-remove-clear-entries.md) — AD 0030: Remove `clear_entries()` — dead API surface — _archived_
- [AD-0031](AD-0031-reject-cross-doc-caveat-deduplication.md) — AD 0031: Reject cross-document caveat deduplication
- [AD-0033](AD-0033-restore-result-with-warnings-dispatch.md) — AD: Restore `Result<ResultWithWarnings<()>>` for `extract_all` per AD 0010
- [AD-0035](AD-0035-oi-0057-007-sevenz-source-streaming-deferred.md) — AD 0035: Defer SevenZ source-side streaming during commit_changes
- [AD-0036](AD-0036-reject-tempfile-tempdir-for-test-scratch-paths.md) — AD 0036: Reject tempfile::tempdir migration for test scratch paths
- [AD-0037](AD-0037-test-suite-consolidation-integration-canonical.md) — AD 0037: Test-suite consolidation — integration suite is canonical
- [AD-0038](AD-0038-tighten-progress-and-overwrite-contracts.md) — AD 0038: Tighten progress-callback and overwrite contracts
- [AD-0039](AD-0039-hermetic-unrar-build-and-artifact-purge.md) — AD 0039: Hermetic UnRAR build and purge of checked-in build artifacts
- [AD-0040](AD-0040-sfx-payload-size-ceiling.md) — AD 0040: Cap SFX payload size before copying to disk
- [AD-0041](AD-0041-remove-hardcoded-temp-path-from-library.md) — AD 0041: Remove hardcoded `/Volumes/Temp/claude` from library and examples
- [AD-0042](AD-0042-password-as-str-returns-result-on-non-utf8.md) — AD 0042: `password_as_str` returns `Result`; reject non-UTF-8 passwords
- [AD-0044](AD-0044-validate-archive-internal-paths-at-facade-boundary.md) — AD 0044: Validate archive-internal paths at the creation/modification facade boundary
- [AD-0045](AD-0045-reject-r0063-low-cluster-doc-drift-already-covered.md) — AD 0045: Reject R0063 Low-cluster doc-drift sweep; already covered by OI-0057-009 + post-v0.1.0 banners — _archived_
- [AD-0046](AD-0046-reject-r0064-windows-support-messaging-downgrade.md) — AD 0046: Reject R0064 Windows-support messaging downgrade cluster
- [AD-0047](AD-0047-content-based-manifest-digest-on-crc-less-formats.md) — AD 0047: Content-based manifest_digest on CRC-less formats
- [AD-0049](AD-0049-reject-r0065-windows-rename-retry-test.md) — AD: Reject R0065-0017 — Windows rename locked-file retry test
- [AD-0050](AD-0050-reject-r0066-extract-with-options-narrow-parameter.md) — AD 0050: Reject R0066-0003/0004 — `extract_to_{memory,stream}_with_options` options-ignored is documented — _superseded_
- [AD-0051](AD-0051-r0068-closure-and-stale-finding-bulk-reject.md) — AD 0051: Review 0068 closure — bulk-reject stale findings, accept the residual fixes, supersede AD 0050
- [AD-0052](AD-0052-r0068-d8-codify-lazy-backend-open-validation.md) — AD 0052: Codify lazy-validation semantics for backend `open()` (Review 0068 D8)
- [AD-0053](AD-0053-r0068-group-d-architectural-pass-design-baseline.md) — AD 0053: Review 0068 Group D — design baseline for the remaining architectural pass
- [AD-0054](AD-0054-r0068-d4-piz-and-zip-handle-caching.md) — AD 0054: Cache Piz mmap + ZIP `RawZipArchive` handle (D4 first cut)
- [AD-0055](AD-0055-r0068-d3-validated-source-token.md) — AD 0055: `ValidatedSource` token replaces `_unchecked` extraction (D3)
- [AD-0056](AD-0056-r0068-d9-libarchive-reader-writer-split-deferred.md) — AD 0056: Defer the LibarchiveArchive read/write struct split (D9 deferred)
- [AD-0057](AD-0057-r0068-d10-large-file-refactor-deferred.md) — AD 0057: Defer D10 large-file refactors and `#[cfg(test)]` move-out
- [AD-0058](AD-0058-feature-first-footprint-split-and-facade-crates.md) — AD 0058: Feature-first footprint split and facade crates
- [AD-0059](AD-0059-r0069-wide-modular-design-closure.md) — AD 0059: Review 0069 closure — accept wide modular-design pass, route by group
- [AD-0060](AD-0060-r0070-broad-modular-triage-closure.md) — AD 0060: Review 0070 closure — broad modular triage, fix all 92 findings
- [AD-0061](AD-0061-r0071-narrow-correctness-closure.md) — AD 0061: Review 0071 closure — narrow correctness pass, all 21 findings landed inline
- [AD-0062](AD-0062-r0069-group-a-v0.3-api-shaping.md) — AD 0062: Review 0069 Group A — v0.3 API-shaping plan
- [AD-0063](AD-0063-r0075-closure-and-design-positions.md) — AD 0063: Review 0075 — closure record + design positions
- [AD-0064](AD-0064-r0075-non-utf8-path-policy-option-a.md) — AD 0064: Non-UTF-8 path policy — preserve raw bytes (Option A)
- [AD-0065](AD-0065-backend-caching-baseline-option-a.md) — AD 0065: Backend caching baseline — frozen-view at first use (Option A)
- [AD-0066](AD-0066-r0076-sanitize-vs-reject-policy-deferred.md) — AD 0066: Path sanitization policy — preserve current "lossy repair" baseline, queue strict-reject opt-in
- [AD-0067](AD-0067-r0076-closure-and-routing-record.md) — AD 0067: Review 0076 closure and routing record
- [AD-0068](AD-0068-reject-review-0067-duplicate-of-0066.md) — AD 0068: Reject Review 0067 as byte-identical duplicate of Review 0066 — _archived_
- [AD-0069](AD-0069-focused-closure-records-replace-omnibus-routing-tables.md) — AD 0069: Review closure is recorded in focused records, not in one omnibus routing table
- [AD-0070](AD-0070-ci-means-recorded-per-platform-verification.md) — AD 0070: CI means recorded per-platform verification, not a hosted service
- [AD-0071](AD-0071-libarchive-owns-zip-modification.md) — AD 0071: libarchive owns ZIP modification; the zip crate owns everything else ZIP
- [AD-0072](AD-0072-the-uniform-interface-is-the-product.md) — AD 0072: The uniform interface is the product; the crate absorbs dependency gaps
- [AD-0073](AD-0073-integrity-scan-aborted-is-a-third-class.md) — AD 0073: an abandoned integrity scan is a third outcome class, not a corruption verdict
- [AD-0074](AD-0074-zip-central-extended-timestamp-not-pursued.md) — AD 0074: the ZIP central-directory extended-timestamp convention is not pursued

## Review-gate Decisions (MADR)

- [MADR-0001](MADR-0001-r045-list-files-skips-crc-walk.md) — AD: `Archive::list_files()` skips the libarchive CRC walk
- [MADR-0002](MADR-0002-r045-rar-progress-prescan-isolation.md) — AD: RAR extraction progress pre-scan uses an isolated handle
- [MADR-0003](MADR-0003-r045-reject-bulk-internal-type-doc-entries.md) — AD: Reject bulk per-wrapper documentation entries; document the boundary instead
- [MADR-0004](MADR-0004-r045-reject-src-prefix-false-positives.md) — AD: Reject the 39 "missing source file" issues — reviewer false positives
- [MADR-0005](MADR-0005-r051-early-link-rejection-in-modify.md) — AD: Early link rejection in modify pipeline — _archived_
- [MADR-0006](MADR-0006-r051-manifest-digest-computes-crc32-from-content.md) — AD: Manifest digest computes CRC32 from entry content for CRC-less formats — _superseded_
- [MADR-0007](MADR-0007-r051-zip-aes256-creation-encryption.md) — AD: ZIP AES-256 creation encryption — _superseded_
- [MADR-0008](MADR-0008-r052-manifest-digest-error-propagation.md) — AD: Manifest Digest Error Propagation (R0052-0001) — _superseded_
- [MADR-0009](MADR-0009-r052-advisory-file-locking-modify-mode.md) — AD: Advisory File Locking for Modify Mode (R0052-0004)
- [MADR-0010](MADR-0010-r052-extraction-warnings-result-with-warnings.md) — AD: Wire ResultWithWarnings into extract_all (R0052-0006/R0052-0007)
- [MADR-0011](MADR-0011-r0001-unsupported-operation-display-text.md) — AD: UnsupportedOperation display text — "cannot be performed" replaces "not implemented" — _superseded_
- [MADR-0012](MADR-0012-r0001-reject-non-zip-creation-password.md) — AD: Reject non-ZIP creation password at facade boundary — _superseded_
- [MADR-0013](MADR-0013-r0001-multipart-detection-heuristic-tightening.md) — AD: Multipart detection heuristic tightening + ZIP sort fix
- [MADR-0014](MADR-0014-r0001-sfx-probable-confidence-clamp.md) — AD: SfxDetectionResult probable() confidence clamped to [0.0, 0.99] — _superseded_
- [MADR-0015](MADR-0015-r0002-sfx-confirmed-detection-patterns.md) — AD: SFX confirmed detection (confidence 1.0) — known patterns
- [MADR-0016](MADR-0016-r0002-modify-eliminate-redundant-open.md) — AD: Eliminate redundant middle open in modify()
- [MADR-0017](MADR-0017-r0002-bzip2-bit-level-eos-scanning.md) — AD: Bit-level BZIP2 EOS marker scanning
- [MADR-0018](MADR-0018-r0003-clippy-lint-baseline-and-dead-code-strategy.md) — AD: Clippy lint baseline restoration and dead code strategy
- [MADR-0019](MADR-0019-r0003-raw-format-verification-and-entry-name-normalization.md) — AD: Raw format test verification and entry name normalization
- [MADR-0020](MADR-0020-r0056-feature-gate-rar-build.md) — AD: Feature-gate UnRAR build behind `rar-support` Cargo feature
- [MADR-0021](MADR-0021-r0056-zip-creation-reject-symlinks.md) — AD: Reject symlinks in ZIP recursive creation
- [MADR-0022](MADR-0022-r0057-zip-aes-creation-regression.md) — AD: AD 0007 ZIP-AES Creation Regression — Track as Open Issue — _superseded_
- [MADR-0023](MADR-0023-r0057-open-at-offset-format-agnostic-error.md) — AD: `Archive::open_at_offset` Returns `NotImplemented`, Not a Forged-Format `Unsupported` — _superseded_
- [MADR-0024](MADR-0024-r0057-libarchive-non-zip-passwords-hard-fail.md) — AD: Non-ZIP Creation Rejects Passwords With `OperationBlocked` — _superseded_
- [MADR-0025](MADR-0025-r0057-review-0058-duplicate-of-0057.md) — AD: Review 0058 Archived as Duplicate of Review 0057
- [MADR-0026](MADR-0026-r0057-sanitize-entry-path-side-effect-removal.md) — AD: `sanitize_entry_path` No Longer Creates Directories
- [MADR-0027](MADR-0027-reject-encrypted-archive-creation.md) — AD: This Library Does Not Produce Encrypted Archives
- [MADR-0028](MADR-0028-r0060-reject-already-tracked-gaps.md) — AD: Reject Review 0060 items already tracked by DEF-* / OI-* entries
- [MADR-0029](MADR-0029-r0060-reject-scratch-path-portability-sweep.md) — AD: Reject blanket portability sweep over `/Volumes/Temp/claude/` scratch paths — _superseded_
- [MADR-0030](MADR-0030-r0080-reject-null-header-format-error.md) — AD: NULL-pathname libarchive headers are uniformly omitted, not format errors
- [MADR-0031](MADR-0031-r0001-tiny-file-extension-precedence.md) — AD: Files too small to carry magic bytes fall back to any extension mapping
- [MADR-0032](MADR-0032-r0001-selective-ratio-denominator.md) — AD: For selective extraction the archive-ratio guard is informational; the byte caps bind

## Design Change Records (DCR)

- [DCR-001](DCR-001-in-flight-gap-remediation.md) — Design Change Record
- [DCR-002](DCR-002-review-0076-inline-fixes.md) — DCR-002: Review 0076 inline fixes — ratio guard hardening, libarchive return-code extension, public-API non-exhaustive markers
- [DCR-003](DCR-003-review-0078-lzma-extension-fallback.md) — DCR-003: Extend `Archive::open` extension-fallback set to LZMA / TAR.LZMA
- [DCR-004](DCR-004-oi-0076-004-v2-api-parity-and-drop-ownership.md) — DCR-004: v2-api typed-handle full parity + unified Drop ownership
- [DCR-005](DCR-005-unrar-listing-memoisation.md) — DCR-005: UnRAR listing memoised via fresh-handle walk (AD 0065 correction)
- [DCR-006](DCR-006-bounded-streaming-hard-cap.md) — Bounded streaming errors on over-production instead of silently truncating
- [DCR-007](DCR-007-modify-identity-revalidation.md) — Modify mode revalidates locked-file identity before every pathname use
- [DCR-008](DCR-008-unrar-reentrancy-sentinel.md) — UnRAR lock gains a same-thread re-entrancy sentinel
- [DCR-009](DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md) — Dual ZIP backend collapsed to a single `zip`-crate backend (piz removed)
- [DCR-010](DCR-010-extraction-rejects-special-entries.md) — Extraction rejects non-regular, non-directory entries on every backend
- [DCR-011](DCR-011-digest-exactness-for-crc-less-entries.md) — Digest methods hold CRC-less entries to their declared size exactly
- [DCR-012](DCR-012-content-digest-drops-per-path-occurrence-ordinal.md) — Content-multiset digest drops the per-path occurrence ordinal
- [DCR-013](DCR-013-recursive-creation-emits-directory-metadata.md) — Recursive creation emits every directory once, carrying source metadata, on both writers
- [DCR-014](DCR-014-read-handle-bound-to-archive-file-identity.md) — Read handles are bound to the archive file's identity, not to a wider per-entry fingerprint
- [DCR-015](DCR-015-in-place-offset-opens-for-rar-and-sevenz.md) — In-place offset opens extended to RAR and 7z; libarchive deferred on gate strength, not on FFI cost

## Legacy review dispositions (DD / IG)

Split out of the retired `legacy/{Decisions,Ignores}.md` write-path logs on 2026-08-04,
one record file per entry. Bodies are unchanged; see `redirects.yaml` for id resolution.

### Accepted review decisions (DD)

- [DD-024-001](DD-024-001-rar5-parse-rar5-recovery-panic-fix.md) — RAR5 parse_rar5_recovery Panic Fix
- [DD-024-003](DD-024-003-oom-protection-for-libarchive-and-zip-extract-to-memory.md) — OOM Protection for Libarchive and Zip extract_to_memory
- [DD-023-001](DD-023-001-libarchive-write-mode-data-integrity.md) — Libarchive Write-Mode Data Integrity
- [DD-021](DD-021-extract-to-memory-oom-protection.md) — Extract-to-Memory OOM Protection
- [DD-020-001](DD-020-001-rar5-vint-decoding-algorithm.md) — RAR5 vint Decoding Algorithm
- [DD-020-002](DD-020-002-unrar-filetime-epoch-offset.md) — UnRAR FILETIME Epoch Offset
- [DD-019-001](DD-019-001-unrar-test-integrity-password-fix.md) — UnRAR test_integrity Password Fix
- [DD-018-001](DD-018-001-rar5-decode-vint-panic-fix.md) — RAR5 decode_vint Panic Fix
- [DD-017-001](DD-017-001-zipwriter-oom-risk.md) — ZipWriter OOM Risk
- [DD-014](DD-014-refactoring-suggestions.md) — Refactoring Suggestions
- [DD-013-002](DD-013-002-sevenz-encryption-flag.md) — SevenZ Encryption Flag
- [DD-013-001](DD-013-001-unrar-compile-fix.md) — UnRAR Compile Fix
- [DD-012-004](DD-012-004-libarchive-creation-timestamp-precision.md) — Libarchive Creation Timestamp Precision
- [DD-012-003](DD-012-003-temp-dir-race-condition.md) — Temp Dir Race Condition
- [DD-012-002](DD-012-002-sfx-detection-flaws.md) — SFX Detection Flaws
- [DD-012-001](DD-012-001-libarchive-memory-inefficiency.md) — Libarchive Memory Inefficiency
- [DD-011-001](DD-011-001-piz-backend-4gb-limit.md) — Piz Backend 4GB Limit
- [DD-011-002](DD-011-002-libarchive-walkdir-errors.md) — Libarchive Walkdir Errors
- [DD-010-003](DD-010-003-extract-all-finish-entry-errors.md) — Extract All Finish Entry Errors
- [DD-010-001](DD-010-001-hard-link-skipping.md) — Hard Link Skipping
- [DD-009-001](DD-009-001-zip-path-traversal.md) — Zip Path Traversal
- [DD-005](DD-005-review-0005-improvements.md) — Review 0005 Improvements
- [DD-004](DD-004-review-0004-improvements.md) — Review 0004 Improvements
- [DD-003](DD-003-review-0003-improvements.md) — Review 0003 Improvements

### Rejected / ignored findings (IG)

- [IG-020-003](IG-020-003-unrar-ffi-struct-layout-on-linux.md) — UnRAR FFI Struct Layout on Linux — _superseded (reopened and fixed in Review 0080)_
- [IG-014-004](IG-014-004-ensure-destination-utility-duplication.md) — ensure_destination utility duplication
- [IG-014-003](IG-014-003-redundant-extract-to-stream-implementation.md) — Redundant extract_to_stream implementation
- [IG-014-002](IG-014-002-inconsistent-extract-to-memory-implementations.md) — Inconsistent extract_to_memory implementations
- [IG-014-001](IG-014-001-manual-dispatch-in-archive-methods.md) — Manual dispatch in Archive methods
- [IG-012-003](IG-012-003-temp-dir-race-in-extract-to-memory.md) — Temp dir race in extract_to_memory
- [IG-011-005](IG-011-005-sanitize-entry-path-side-effects.md) — sanitize_entry_path side-effects
- [IG-011-004](IG-011-004-extract-to-memory-inefficiency.md) — extract_to_memory inefficiency
- [IG-011-003](IG-011-003-empty-directories-not-preserved.md) — Empty directories not preserved
- [IG-010-002](IG-010-002-path-sanitization-creates-directories-before-check.md) — Path sanitization creates directories before check
- [IG-009-003](IG-009-003-libarchive-crc32-partial-hashes.md) — Libarchive CRC32 partial hashes
- [IG-009-002](IG-009-002-extractionoptions-filter-unused.md) — ExtractionOptions.filter unused
- [IG-005-08](IG-005-08-split-size-unused.md) — split_size unused
- [IG-005-01](IG-005-01-sfx-open-at-offset-unimplemented.md) — SFX open_at_offset unimplemented
- [IG-004-05](IG-004-05-bzip2-full-file-read.md) — bzip2 full file read
- [IG-004-01](IG-004-01-streaming-buffers-entire-file.md) — Streaming buffers entire file
- [IG-0061-0094..0123](IG-0061-0094-0123-tempfile-tempdir-migration-for-test-scratch-paths.md) — tempfile::tempdir() migration for test scratch paths
- [IG-0063-0011..0121](IG-0063-0011-0121-drifted-non-canonical-artifact-content-sweep.md) — Drifted non-canonical artifact content sweep
- [IG-0064-0034..0039](IG-0064-0034-0039-windows-support-messaging-downgrade-cluster.md) — Windows-support messaging downgrade cluster
- [IG-0065-0017](IG-0065-0017-windows-rename-locked-file-retry-test.md) — Windows rename locked-file retry test
- [IG-0081-0010](IG-0081-0010-iso-detection-requires-the-primary-volume-descriptor-at-sector.md) — ISO detection requires the primary volume descriptor at sector 16
- [IG-0081-0013](IG-0081-0013-pk-x07-x08-split-zip-spanning-marker-rejected-at-offset-zero.md) — `PK\x07\x08` split-ZIP spanning marker rejected at offset zero
- [IG-0081-0050](IG-0081-0050-zip-extended-timestamp-uses-signed-32-bit-seconds-2038-ceiling.md) — ZIP Extended-Timestamp uses signed 32-bit seconds (2038 ceiling)
- [IG-0081-0088](IG-0081-0088-bsd-targets-omit-the-c-plus-plus-runtime-for-the-vendored.md) — BSD targets omit the C++ runtime for the vendored UnRAR build
