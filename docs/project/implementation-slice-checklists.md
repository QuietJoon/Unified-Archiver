# Implementation Slice Checklists (Retrospective)

> This is a retrospective record. Items are marked as completed or deferred based on actual implementation status.

## Slice 1: Inspection (SCN-INS-01 through SCN-INS-05)

**Governing scenarios:** SCN-INS-01, SCN-INS-02, SCN-INS-03, SCN-INS-04, SCN-INS-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/inspection.md`

- [x] `Archive::open` with format auto-detection via magic bytes
- [x] `list_files()` returns Vec<ArchiveEntry> with normalized metadata across all formats
- [x] Entry caching via OnceCell for repeated reads
- [x] `list_files_for_limits()` cheaper path without CRC computation
- [x] `find_entry()` by path lookup
- [x] `validate_integrity()` CRC32-based integrity check
- [x] `is_encrypted()` encryption detection
- [x] `detect_multipart()` multi-part archive detection
- [x] `is_solid()` solid compression detection (RAR)
- [x] `has_recovery_record()` and `recovery_percentage()` (RAR)
- [x] Symlink/hardlink warnings during listing
- [x] Password-protected metadata access without password (RAR)
- [x] Pattern-based filtering on entry lists
- [x] Tests: format_compatibility_test, password_handling_test, crc32_verification_test, multipart_test, integrity_comprehensive_test

**Status:** Complete

---

## Slice 2: Extraction (SCN-EXT-01 through SCN-EXT-05)

**Governing scenarios:** SCN-EXT-01, SCN-EXT-02, SCN-EXT-03, SCN-EXT-04, SCN-EXT-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/extraction.md`

- [x] `extract_all()` with destination preparation and backend dispatch
- [x] `extract_file()` single-file extraction
- [x] `extract_to_memory()` in-memory extraction (via temp files for UnRAR/libarchive)
- [x] `extract_filtered()` predicate-based selective extraction
- [x] `extract_to_stream()` streaming wrapper (Cursor-based)
- [x] Parallel extraction via rayon for 4+ files
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
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/creation.md`

- [x] `Archive::create()` with format selection
- [x] `add_file_from_path()` add file from filesystem
- [x] `add_file_from_data()` add file from memory
- [x] `add_directory_recursive()` recursive directory addition via walkdir
- [x] `finish()` finalize and close archive
- [x] Compression levels: Store, Fastest, Fast, Normal, Maximum, Ultra
- [x] Format support: ZIP (zip crate), 7z (libarchive), TAR/TAR.GZ/TAR.BZ2/TAR.XZ (libarchive)
- [x] Password encryption for ZIP
- [x] Progress callbacks during creation
- [x] Drop impl ensures cleanup if finish() not called
- [x] Optional RAR creation via external WinRAR CLI (feature-gated, Windows only)
- [x] Tests: integration/creation.rs (13 tests)

**Status:** Complete
**Deferred:** split_size (DEF-002), GZIP/BZIP2/XZ standalone creation

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
- [x] ModificationOptions (backup, metadata preservation)
- [x] Tests: integration/modification.rs (12 tests)
- [ ] **Partial:** libarchive write API integration for commit (returns error for some paths)
- [ ] **Partial:** ZIP modification reliability (some scenarios ignore-gated — DEF-005)
- [ ] **Partial:** ModificationOptions fields not fully applied by commit flow

**Status:** Partial (API complete, backend integration pending)

---

## Slice 5: SFX Detection (SCN-SFX-01 through SCN-SFX-08)

**Governing scenarios:** SCN-SFX-01 through SCN-SFX-08
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/spec.md` (FR-025 through FR-031)

- [x] `detect_sfx()` 3-stage pipeline: stub type -> signature scan -> validation
- [x] `extract_stub()` extract SFX stub bytes separately
- [x] StubType detection: PE (goblin), ELF (goblin), Mach-O (goblin), Script (shebang)
- [x] Signature scanning: ZIP, RAR, RAR5, 7z magic bytes within first 1MB
- [x] Correct rejection of standard archives (not SFX)
- [x] Correct rejection of non-archive executables
- [x] Heuristic scanning for unknown stubs
- [x] SfxDetectionResult with is_sfx, archive_format, data_offset, stub_type
- [x] Tests: sfx_detection_test.rs, SFX coverage report
- [ ] **Deferred:** `open_at_offset` / `open_sfx` for direct SFX archive opening (DEF-001)

**Status:** Complete (detection); open_at_offset deferred

---

## Slice 6: Cross-Cutting Concerns

**Governing scenarios:** All
**Design inputs:** Constitution (robustness, testing, contracts)

- [x] Unified error handling (ArchiveError) across all backends
- [x] Thread safety (Archive is Send; concurrent operations on different files)
- [x] RAII cleanup for all resources (handles, temp dirs, writers)
- [x] Security: path sanitization, extraction limits
- [x] Documentation: API_REFERENCE.md, GETTING_STARTED.md, examples/, rustdoc
- [x] Benchmarks: archive_operations, integrity_validation, sfx_detection
- [x] Property-based tests: path normalization, format consistency
- [x] Concurrency tests: parallel operations, thread safety

**Status:** Complete
