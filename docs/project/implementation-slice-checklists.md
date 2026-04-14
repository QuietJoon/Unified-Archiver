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
- [x] Symlink/hardlink warnings during listing (partial — native backends still do not classify link entries; see OI-010-001)
- [x] Password-protected metadata access without password (RAR)
- [x] Pattern-based filtering on entry lists
- [x] Tests: format_compatibility_test, password_handling_test, crc32_verification_test, integrity_comprehensive_test (multipart detection tested inline in `src/inspection.rs`)

**Status:** Complete with known gap (link classification only works on libarchive backend; native backends do not classify symlink/hardlink entries — OI-010-001)

---

## Slice 2: Extraction (SCN-EXT-01 through SCN-EXT-05)

**Governing scenarios:** SCN-EXT-01, SCN-EXT-02, SCN-EXT-03, SCN-EXT-04, SCN-EXT-05
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/contracts/extraction.md`

- [x] `extract_all()` with destination preparation and backend dispatch
- [x] `extract_file()` single-file extraction
- [x] `extract_to_memory()` in-memory extraction — backend matrix: UnRAR extracts to temp dir then reads; libarchive reads directly into buffer; Piz reads from memory-mapped ZIP; SevenZ decodes in memory; ZipReader (buffered encrypted ZIP backend) reads via zip crate cursor
- [x] `extract_filtered()` predicate-based selective extraction
- [x] `extract_to_stream()` streaming wrapper (Cursor-based for non-libarchive; true streaming for libarchive)
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
- [ ] **Deferred (mvp-scope):** RAR creation via external WinRAR CLI — deferred per mvp-scope and intake decisions
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
- [~] **Partial (AD 0020):** ModificationOptions — `create_backup`/`backup_suffix` honored via `modify_with_options()`; `preserve_metadata` accepted but no-op pending OI-025-002
- [x] Tests: integration/modification.rs (12 tests)
- [ ] **Partial:** libarchive write API integration for commit (returns error for some paths)
- [ ] **Partial:** ZIP modification reliability (some scenarios ignore-gated — DEF-005)

**Status:** Partial (API complete, backend integration pending)

---

## Slice 5: SFX Detection (SCN-SFX-01 through SCN-SFX-08)

**Governing scenarios:** SCN-SFX-01 through SCN-SFX-08
**Design inputs:** `docs/architecture/scenario-matrix.md`, `specs/001-unified-archive/spec.md` (FR-025 through FR-031)

- [x] `detect_sfx()` 3-stage pipeline: stub type -> signature scan -> validation
- [x] `extract_stub()` extract SFX stub bytes separately
- [x] StubType detection: PE (goblin), ELF (goblin), Mach-O (goblin), ScriptInterpreter (shebang, per AD 0016)
- [x] Signature scanning: ZIP, RAR, RAR5, 7z magic bytes within first 1MB
- [x] Correct rejection of standard archives (not SFX)
- [x] Correct rejection of non-archive executables
- [x] Resolved (OI-027-001): Unknown stubs classified as `StubType::Unknown` and proceed to signature scanning
- [x] SfxDetectionResult with is_sfx, archive_format, data_offset, stub_type, confidence
- [x] Tests: tests/integration/sfx_detection.rs, SFX coverage report
- [ ] **Partial:** `open_sfx()` convenience method exists in public API but delegates to deferred `open_at_offset()`, so positive SFX detection does not yield a readable archive
- [ ] **Deferred:** `open_at_offset` for direct offset-based archive opening (DEF-001, returns `Unsupported`)

**Status:** Partial — detection pipeline complete; `open_sfx()` present but non-functional pending `open_at_offset` (DEF-001)

---

## Slice 6: Cross-Cutting Concerns

**Governing scenarios:** All
**Design inputs:** Constitution (robustness, testing, contracts)

- [ ] **Partial:** Unified error handling (ArchiveError) across all backends — error mapping is not fully normalized; backend-specific errors may surface inconsistently pending error-contract reconciliation
- [x] Thread safety (Archive is Send; concurrent operations on different files) — RAR FFI calls serialized via `UNRAR_LOCK` mutex (OI-026-004 resolved)
- [x] RAII cleanup for all resources (handles, temp dirs, writers)
- [x] Security: path sanitization, extraction limits
- [ ] **Partial:** Documentation: `docs/API_REFERENCE.md`, `docs/GETTING_STARTED.md`, `examples/`, rustdoc — documentation exists but requires reconciliation with implementation (see status.md)
- [x] Benchmarks: archive_operations, integrity_validation, sfx_detection
- [x] Property-based tests: path normalization, format consistency
- [x] Concurrency tests: parallel operations, thread safety

**Status:** Partial — unified error handling not fully normalized across backends
