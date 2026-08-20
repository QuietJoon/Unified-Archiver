---
type: Slice Checklists
title: "Implementation Slice Checklists (Retrospective)"
description: "This is a retrospective record."
tags: [project-control, ADR-0021, ADR-0018, ADR-0020, ADR-0016, FR-025, FR-031]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# Implementation Slice Checklists (Retrospective)

> This is a retrospective record. Items are marked as completed or deferred based on actual implementation status.

## Slice 1: Inspection (SCN-INS-01 through SCN-INS-05)

**Governing scenarios:** SCN-INS-01, SCN-INS-02, SCN-INS-03, SCN-INS-04, SCN-INS-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/inspection.md`

- [x] `Archive::open` with format auto-detection via magic bytes
- [x] `list_files()` returns `&[ArchiveEntry]` with normalized metadata across all formats
- [x] Entry caching via OnceCell for repeated reads
- [x] `list_files_for_limits()` cheaper path without CRC computation
- [x] `find_entry()` by path lookup
- [x] `validate_integrity()` CRC32-based integrity check
- [x] `is_encrypted()` encryption detection
- [x] `detect_multipart()` multi-part archive detection
- [x] `is_solid()` solid compression detection (RAR and 7z)
- [x] `has_recovery_record()` and `recovery_percentage()` (RAR)
- [x] Symlink/hardlink detection and warnings during listing (all backends classify links — OI-010-001 resolved)
- [x] Password-protected metadata access without password (RAR)
- [x] Pattern-based filtering on entry lists
- [x] Tests: format_compatibility_test, password_handling_test, crc32_verification_test, integrity_comprehensive_test (multipart detection tested inline in `src/inspection.rs`)

**Status:** Complete (link classification works on all backends — OI-010-001 resolved)

---

## Slice 2: Extraction (SCN-EXT-01 through SCN-EXT-05)

**Governing scenarios:** SCN-EXT-01, SCN-EXT-02, SCN-EXT-03, SCN-EXT-04, SCN-EXT-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/extraction.md`

- [x] `extract_all()` with destination preparation and backend dispatch
- [x] `extract_file()` single-file extraction
- [x] `extract_to_memory()` in-memory extraction — backend matrix: UnRAR extracts to temp dir then reads; libarchive reads directly into buffer; SevenZ decodes in memory; ZipReader (sole ZIP backend since DCR-009 removed piz) reads via zip crate over `std::fs::File`
- [x] `extract_filtered()` predicate-based selective extraction
- [x] `extract_to_stream()` streaming wrapper (Cursor-based for non-libarchive; true streaming for libarchive)
- [x] Selective extraction via predicate (sequential)
- [x] Password-aware reopen for encrypted archives
- [x] Path sanitization via `sanitize_entry_path` (traversal prevention)
- [x] Overwrite policy enforcement (fail on conflict by default)
- [x] ExtractionLimits preflight checks (zip bomb guard)
- [x] Progress callbacks with RateLimiter (~60 Hz)
- [x] CRC32 verification during extraction (automatic in UnRAR/libarchive)
- [x] Multi-layer decompression (TAR.GZ, TAR.BZ2, TAR.XZ) transparent
- [x] Corrupted archive detection with ArchiveError::Corruption
- [x] Tests: integration tests, streaming_test, progress_callback_test, performance_test

**Status:** Complete
**Deferred:** True streaming (DEF-004), true in-memory extraction for UnRAR/libarchive

---

## Slice 3: Creation (SCN-CRE-01 through SCN-CRE-05)

**Governing scenarios:** SCN-CRE-01, SCN-CRE-02, SCN-CRE-03, SCN-CRE-04, SCN-CRE-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, Creation API defined in `specs/001-unified-archive/contracts/archive.md` and `src/creation.rs`

- [x] `Archive::create()` with format selection
- [x] `add_file_from_path()` add file from filesystem
- [x] `add_file_from_data()` add file from memory
- [x] `add_directory_recursive()` recursive directory addition via walkdir
- [x] `finish()` finalize and close archive
- [x] Compression levels: Store, Fastest, Fast, Normal, Maximum, Ultra
- [x] Format support: ZIP (zip crate), 7z creation (libarchive; note: 7z read uses native SevenZ backend), TAR/TAR.GZ/TAR.BZ2/TAR.XZ (libarchive)
- [x] Password encryption for ZIP
- [x] Resolved (OI-025-003, AD 0021): Progress callbacks during creation — per-entry invocation with `total=None` by both ZIP and libarchive backends
- [x] Drop impl ensures cleanup if finish() not called
- [x] **Resolved (2026-08-04 re-verification):** RAR creation via external WinRAR CLI — implemented behind the optional `external-rar-create` feature (`src/external/rar.rs`, `RarCreator`, Windows-only argv-guarded wrapper). Two design holes remain tracked as OI-0076-006; the capability itself shipped.
- [x] Tests: integration/creation.rs (13 tests)

**Status:** Complete with deferred gaps (split_size, RAR creation). Creation progress resolved (OI-025-003, AD 0021).
**Deferred:** split_size (DEF-002), RAR creation via WinRAR CLI (mvp-scope). Standalone GZIP/BZIP2/XZ creation is out-of-scope per AD 0018 (these compressors are only supported as TAR compound formats).

---

## Slice 4: Modification (SCN-MOD-01 through SCN-MOD-04)

**Governing scenarios:** SCN-MOD-01, SCN-MOD-02, SCN-MOD-03, SCN-MOD-04
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/archive.md`

- [x] `Archive::modify()` opens archive in modify mode
- [x] `add_entry()` queue file addition
- [x] `remove_entry()` queue file removal
- [x] `replace_entry()` queue file replacement
- [x] `commit_changes()` copy-on-write rewrite with atomic rename
- [x] Format validation (RAR correctly rejected as read-only)
- [x] **Resolved (AD 0020, OI-025-001/002 2026-04-14):** ModificationOptions — `create_backup`/`backup_suffix` honored via `modify_with_options()`; `preserve_metadata` preserves timestamps and Unix permissions via metadata-aware add helpers; `compression` overrides recreation settings.
- [x] Tests: integration/modification.rs (12 tests)
- [x] **Resolved (2026-08-04 re-verification):** libarchive write API integration for commit — `commit_changes()` routes source reads and 7z/TAR-side writes through libarchive with retained-entry replay, metadata preservation, and the R0069-0065 known-size tempfile spool; modification is ZIP/7z-only **by design** (`Limitations.md` §2), not by missing integration. The 12 `tests/integration/modification.rs` tests run un-gated.
- [ ] **Partial:** residual ZIP-modify caveats (DEF-005) — no scenarios are ignore-gated anymore (2026-08-04 re-verification: zero `#[ignore]` in the modification suite); the open remainder is exactly DEF-005's three caveats: ZIP64 >4 GiB boundary coverage, encrypted-ZIP re-encryption (rejected; couples to OI-0081-006), and crash-recovery journaling.

**Status:** Shipped for the ZIP/7z scope; DEF-005 caveats tracked

---

## Slice 5: SFX Detection (SCN-SFX-01 through SCN-SFX-08)

**Governing scenarios:** SCN-SFX-01 through SCN-SFX-08
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/spec.md` (FR-025 through FR-031)

- [x] `detect_sfx()` 3-stage pipeline: stub type -> signature scan -> validation
- [x] `extract_stub()` extract SFX stub bytes separately
- [x] StubType detection: PE, ELF and Mach-O classified from fixed header fields in the 4 KiB prefix (R0079-0010 replaced the `goblin` whole-file parse, which failed on the truncated prefix; R0001-0066 / R0001-0067 widened the ELF and Mach-O checks past the magic), ScriptInterpreter via a newline-terminated shebang (AD 0016, R0075-0069)
- [x] Signature scanning: ZIP, RAR, RAR5, 7z magic bytes within first 1MB
- [x] Correct rejection of standard archives (not SFX)
- [x] Correct rejection of non-archive executables
- [x] Resolved (OI-027-001): unrecognised executables classify as `StubType::Unknown`. **Superseded 2026-08-16 (R0070-0076):** an `Unknown` stub is now *rejected* at Stage 1 and `detect_sfx` returns `not_sfx()` before the Stage-2 signature scan. See SCN-SFX-08 in `docs/architecture/scenario-matrix.md` and `docs/records/AD-0060-r0070-broad-modular-triage-closure.md`.
- [x] SfxDetectionResult with is_sfx, archive_format, data_offset, stub_type, confidence
- [x] Tests: tests/integration/sfx_detection.rs, SFX coverage report
- [x] `open_sfx()` convenience method opens the detected archive payload (temp-file-backed); integration coverage at `tests/integration/sfx_detection.rs::test_open_sfx_with_real_zip_payload`
- [x] `open_at_offset` for direct offset-based archive opening (temp-file-backed implementation in `src/archive.rs`; caveat: copies the payload tail to a temp file before dispatching to the backend)

**Status:** Shipped — detection pipeline and offset-based opening both implemented; `open_sfx()` returns a working `Archive` handle.

---

## Slice 6: Cross-Cutting Concerns

**Governing scenarios:** All
**Design inputs:** Constitution (robustness, testing, contracts)

- [x] **Resolved (2026-08-04 re-verification):** Unified error handling (ArchiveError) across all backends — the error contract was reconciled after this row was written: typed `Operation` labels (`error::ops`) replaced free-form operation strings, the catch-all "Format not yet supported" mapping was removed (R0068-0070), and warning-carrying results (`ResultWithWarnings`, AD 0033) normalized the non-fatal channel. No error-contract items remain open in `docs/project/open-issues.md`.
- [x] Thread safety (Archive is Send; concurrent operations on different files) — RAR FFI calls serialized via `UNRAR_LOCK` mutex
- [x] RAII cleanup for all resources (handles, temp dirs, writers)
- [x] Security: path sanitization, extraction limits
- [x] **Resolved (2026-08-04 re-verification):** Documentation: `docs/API_REFERENCE.md`, `docs/GETTING_STARTED.md`, `docs/USER_MANUAL.md`, `examples/`, rustdoc — the April reconciliation gap has since been closed by repeated review-gate doc passes (doc-drift sweeps in Reviews 0061–0064, the post-DCR-009 backend rewrite, and the 2026-07-23 record renumbering); `Limitations.md` and the README format matrix are the accuracy-checked surfaces reviewers now ground against.
- [x] Benchmarks: archive_operations, integrity_validation, sfx_detection
- [x] Property-based tests: path normalization, format consistency
- [x] Concurrency tests: parallel operations, thread safety

**Status:** Shipped — error contract normalized (typed `error::ops` labels, `ResultWithWarnings`), docs reconciled through the review-gate cycles
