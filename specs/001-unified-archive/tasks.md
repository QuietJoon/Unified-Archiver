# Tasks: unified-archive

**Input**: Design documents from `/specs/001-unified-archive/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: The project now has broad unit, integration, contract, and property-based test coverage.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/`, `benches/`, `examples/` at repository root
- Paths shown below are for the root crate `unified-archive/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [X] T001 Create Cargo.toml with package metadata (name: unified-archive, version: 0.1.0, edition: 2024, rust-version: 1.85)
- [X] T002 Create build.rs for FFI linking configuration with pkg-config for libarchive
- [X] T003 [P] Create .gitignore with Rust patterns (target/, Cargo.lock, *.rs.bk, .idea/, *.log, .env*)
- [X] T004 [P] Create LICENSE file (MIT)
- [X] T005 [P] Create README.md with project overview, build requirements, and basic usage
- [X] T006 Create src/ directory structure (lib.rs, archive.rs, entry.rs, format.rs, error.rs, options.rs, etc.) **(superseded — actual files differ from plan; progress callbacks live in src/options.rs)**
- [X] T007 Create src/ffi/ subdirectory **(superseded — actual layout: common.rs, wrapper.rs, piz_wrapper.rs, zip_wrapper.rs, sevenz_wrapper.rs, libarchive_wrapper.rs, native/unrar/)**
- [X] T008 [P] Create tests/ directory structure (contract/, integration/, fixtures/)
- [X] T009 [P] Create benches/ directory for criterion benchmarks
- [X] T010 [P] Create examples/ directory for usage demonstrations

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T011 Implement ArchiveError enum in src/error.rs with all variants (Io, Format, Corruption, Password, Unsupported, InvalidPath) — **Superseded:** Current enum also includes `CodecUnavailable` and `UnsupportedOperation`.
- [X] T012 [P] Implement Display and std::error::Error traits for ArchiveError in src/error.rs
- [X] T012a [P] Implement codec-specific error messages in src/error.rs with installation instructions (FR-024): ArchiveError::CodecUnavailable with platform-specific installation instructions ✅
- [X] T013 Implement ArchiveFormat enum in src/format.rs with all supported formats (SevenZip, Zip, Rar, Rar5, Tar, TarGzip, TarBzip2, TarXz, Gzip, Bzip2, Xz, Iso)
- [X] T014 Implement ArchiveFormat::detect() method in src/format.rs for magic byte detection (complete - not stubbed) ✅
- [X] T015 [P] Implement ArchiveFormat capability methods in src/format.rs (supports_compression, supports_encryption, supports_multipart, can_modify)
- [X] T016 Implement ArchiveEntry struct in src/entry.rs with all fields (path, size, compressed_size, modified, created, accessed, crc32, entry_type, permissions, is_encrypted, comment, attributes, id)
- [X] T017 Implement EntryType enum in src/entry.rs (File, Directory, Symlink, HardLink, Other)
- [X] T018 Implement ExtractionOptions struct in src/options.rs with Default impl
- [X] T019 [P] Implement CompressionOptions struct and CompressionLevel enum in src/options.rs with Default impl
- [X] T020 Implement ProgressCallback trait in src/options.rs with FnMut impl
- [X] T021 Setup UnRAR FFI bindings in src/ffi/unrar.rs (manual bindings, v7.13.0) - ✅ MANDATORY for RAR/RAR5 CRC32
- [X] T021b Configure build.rs to compile UnRAR library and link libarchive (Homebrew macOS support)
- [X] T021c Fix wchar_t platform differences (UTF-32 on macOS, UTF-16 on Windows) - ✅ Critical for cross-platform
- [X] T022 Implement safe wrapper types in src/ffi/wrapper.rs for UnRAR handles (RAII patterns, CRC32 extraction working)
- [X] T023 Create test fixtures in tests/fixtures/ (test.rar, test_rar5.rar with known CRC32: 0x054607BC) ✅ All tests passing!

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Archive Inspection (Priority: P1) 🎯 MVP

**Goal**: Implement unified archive inspection API that works identically across all formats (ZIP, 7z, RAR, TAR.GZ, etc.)

**Independent Test**: List files from archives of different formats using identical code, verify consistent metadata structure

### Implementation for User Story 1

- [X] T024 [P] [US1] Implement Archive struct skeleton in src/archive.rs with backend (ArchiveBackend enum), format (eager), entry_cache (OnceCell), path, mode
- [X] T025 [P] [US1] Implement ArchiveMode enum in src/archive.rs (Read, Write, Modify)
- [X] T026 [US1] Implement Archive::open() in src/archive.rs with format detection, file validation, returns Read mode
- [X] T027 [US1] Implement Archive::open_encrypted() in src/archive.rs with password storage
- [X] T028 [US1] Implement Archive::format() in src/archive.rs — superseded by eager detection in Archive::open()
- [X] T029 [US1] Implement Archive::is_encrypted() in src/archive.rs scanning entry metadata
- [X] T030 [US1] Implement Archive::path() getter in src/archive.rs
- [X] T031 [US1] Implement Archive::close() in src/archive.rs consuming self, flushing FFI handles
- [X] T032 [US1] Implement Drop trait for Archive in src/archive.rs calling close internally
- [X] T032a [P] [US1] Implement Send bounds for Archive struct in src/archive.rs and verify thread-safety for FR-020/FR-021 (concurrent operations on different files; RAR FFI serialized via UNRAR_LOCK mutex, OI-026-004 resolved) ✅
- [X] T033 [US1] Implement Archive::list_files() in src/inspection.rs, return &[ArchiveEntry] — **Superseded:** Inspection now routes across all backends (Piz, SevenZ, Libarchive, UnRAR), not libarchive-only.
- [X] T033a [US1] Implement multi-part archive detection in src/inspection.rs for FR-019 (detect .z01, .001, .part files and aggregate entries) ✅
- [X] T034 [US1] Implement Archive::entry_count() in src/inspection.rs — delegates to list_files().len() (cached)
- [X] T035 [US1] Implement Archive::find_entry() in src/inspection.rs with linear search by path
- [X] T036 [US1] Implement ValidationReport struct in src/inspection.rs (total_entries, validated, failed)
- [X] T037 [US1] Implement Archive::validate_integrity() in src/inspection.rs with CRC32 checking
- [X] T038 [US1] Wire up inspection.rs in src/lib.rs re-exports (pub use)
- [X] T039 [US1] Create example in examples/inspect_archive.rs demonstrating list_files across multiple formats
- [X] T040 [US1] Create integration test in tests/integration/format_compatibility.rs verifying same API works for ZIP, 7z, TAR.GZ

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently

---

## Phase 4: User Story 2 - Archive Extraction (Priority: P2, MVP)

**Goal**: Implement unified extraction API that works identically for all formats with progress tracking

**Independent Test**: Extract archives of different formats using identical code, verify extracted content matches originals

### Implementation for User Story 2

- [X] T041 [P] [US2] Implement Archive::extract_all() in src/extraction.rs dispatching across all backends (Piz, ZipReader, SevenZ, Libarchive, UnRAR), ExtractionOptions processing
- [X] T041a [US2] Implement multi-part RAR archive reading in src/extraction.rs for FR-019 (seamlessly extract from .partNN.rar files; other multi-part formats deferred) ✅
- [X] T042 [P] [US2] Implement Archive::extract_file() in src/extraction.rs for single file extraction by path
- [X] T043 [US2] Implement Archive::extract_to_memory() in src/extraction.rs returning Vec<u8> — **Note:** UnRAR backend may use temp extraction internally.
- [X] T044 [US2] Implement Archive::extract_filtered() in src/extraction.rs with predicate function for selective extraction
- [X] T045 [US2] Implement streaming extraction logic in src/extraction.rs — mixed: libarchive truly streams; ZIP/7z/RAR buffer full entry in memory before wrapping in StreamingExtractor. **Qualified:** <100MB applies to libarchive-backed formats; Piz/SevenZ/UnRAR buffer full entries.
- [X] T045a [US2] Implement symlink detection and warning emission in src/extraction.rs with ArchiveWarning::SkippedSymlink for FR-022 compliance ✅
- [X] T045b [P] [US2] Create integration test in tests/integration/hardlink_skip.rs verifying hard links are skipped with ArchiveWarning::SkippedHardLink (FR-022, R010-001 fix) ✅ Implemented
- [X] T046 [US2] Implement progress callback integration in src/extraction.rs — rate-limited to ~60 updates/sec maximum; actual frequency varies by backend.
- [X] T047 [US2] Implement password authentication in src/extraction.rs using stored password from Archive::open_encrypted()
- [X] T047b [US2] Implement consistent password authentication failure errors across all formats (RAR/ZIP/7z) with clear error messages (FR-014)
- [X] T048 [US2] Implement file metadata preservation in src/extraction.rs (timestamps, permissions) per ExtractionOptions
- [X] T049 [US2] Implement overwrite handling in src/extraction.rs per ExtractionOptions.overwrite flag
- [X] T050 [US2] Wire up extraction.rs in src/lib.rs re-exports
- [X] T051 [US2] Create example in examples/extract_archive.rs demonstrating extraction with progress across formats
- [X] T052 [US2] Create integration test in tests/integration/extraction.rs verifying extraction works for all supported formats

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently (MVP complete)

---

## Phase 5: User Story 3 - Archive Creation (Priority: P3) - **IMPLEMENTED**

**Goal**: Implement unified archive creation API supporting multiple output formats with compression options

**Independent Test**: Create archives in different formats using identical code, verify they can be extracted by standard tools

**✅ STATUS**: Creation IMPLEMENTED (T053-T057, T060-T064 complete). Archive creation works for ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ with password encryption and all compression levels. T058 (multi-part) deferred to v0.2.0. T059 (progress callbacks) resolved: per-entry creation progress wired for both ZIP and libarchive backends (AD 0021 / OI-025-003). OI-022-002 metadata loss resolved: `add_file_from_path` now preserves timestamps and Unix permissions.

### Implementation for User Story 3

- [X] T053 [P] [US3] Implement Archive::create() in src/creation.rs with CompressionOptions, returns Write mode
- [X] T054 [P] [US3] Implement archive entry addition in src/creation.rs via `add_file_from_data`, `add_file_from_path`, `add_file_from_path_as`
- [X] T055 [US3] Implement Archive::add_directory() in src/creation.rs for recursive directory addition
- [X] T056 [US3] Implement compression level mapping in src/creation.rs (CompressionLevel → format-specific settings)
- [X] T057 [US3] Implement password encryption in src/creation.rs using CompressionOptions.password
- [ ] T058 [US3] Implement multi-part archive support in src/creation.rs per CompressionOptions.split_size (DEFERRED to v0.2.0 - non-critical feature)
- [X] T059 [US3] Implement progress callback integration in src/creation.rs for compression progress — **Resolved:** per-entry creation progress wired for ZIP and libarchive backends (AD 0021 / OI-025-003)
- [X] T060 [US3] Implement file metadata preservation in src/creation.rs (timestamps, permissions) per CompressionOptions — **Resolved:** OI-022-002 fixed; ZipWriter preserves timestamps and Unix permissions via `add_file_from_path`
- [X] T061 [US3] Implement Archive finalization in src/creation.rs on close() for Write mode
- [X] T062 [US3] Wire up creation.rs in src/lib.rs re-exports
- [X] T063 [US3] Create example in examples/create_archive.rs demonstrating creation for ZIP and 7z formats
- [X] T064 [US3] Create integration test in tests/creation_roundtrip_test.rs verifying created archives are valid

**Checkpoint**: All user stories 1, 2, and 3 should now be independently functional

---

## Phase 6: User Story 4 - Archive Modification (Priority: P4) - **IMPLEMENTED**

**Goal**: Implement unified modification API for supported formats (ZIP, 7z) with add/remove operations

**Independent Test**: Modify archives by adding/removing files using identical code, verify changes persist correctly

**✅ STATUS**: Archive modification IMPLEMENTED. Copy-on-write modification strategy implemented in src/modification.rs via commit_changes(). Core operations (add_entry, remove_entry, replace_entry, commit_changes) functional. OI-025-001 resolved: `ModificationOptions.compression` configures recreated archive settings. OI-025-002 resolved: `commit_changes()` preserves timestamps and permissions via `add_file_from_data_with_metadata`. OI-025-003 (creation progress) resolved. `ModificationOptions` backup settings wired via `modify_with_options()` (AD 0020). RAR archives correctly rejected.

### Implementation for User Story 4

- [X] T065 [P] [US4] Implement Archive::modify() in src/archive.rs opening existing archive in Modify mode ✅
- [X] T066 [P] [US4] Implement Archive::add_entry() in src/archive.rs for Modify mode ✅
- [X] T067 [US4] Implement Archive::remove_entry() in src/archive.rs by path ✅
- [X] T068 [US4] Implement Archive::replace_entry() in src/archive.rs combining remove and add ✅
- [X] T069 [US4] Implement format capability check in src/archive.rs (error if format doesn't support modification) ✅
- [X] T070 [US4] Implement copy-on-write modification in src/archive.rs with commit_changes() ✅
- [X] T071 [US4] Modification logic in src/modification.rs (copy-on-write via commit_changes()) ✅
- [X] T072 [US4] Create example in examples/modify_archive.rs demonstrating add/remove/replace operations - all scenarios tested successfully ✅
- [X] T073 [US4] Create integration test in tests/modification_test.rs - 5 tests passing (add, remove, replace, combined, error handling) ✅

**Checkpoint**: All user stories functionally present. OI-025-001/002/003 all resolved (compression settings, metadata preservation, creation progress).

---

## Phase 7: User Story 5 - SFX Detection (Priority: P5)

**Goal**: Implement cross-platform SFX (Self-Extracting Archive) detection that identifies embedded archives in executable files (Windows PE, Linux ELF, macOS Mach-O, Unix shell scripts via ScriptInterpreter)

**Independent Test**: Detect SFX files using synthetic test fixtures across platforms. Official-tool samples (7-Zip, WinRAR, makeself) and large negative corpus are planned but not yet implemented.

### Implementation for User Story 5

- [X] T091 [P] [US5] Add goblin dependency (v0.9+) to Cargo.toml for PE/ELF/Mach-O parsing
- [X] T092 [P] [US5] Create SFX module: src/sfx.rs (public module root) + src/sfx/ directory (detection.rs, signatures.rs, stub_types.rs, result.rs)
- [X] T093 [P] [US5] Implement StubType enum in src/sfx/stub_types.rs (WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown)
- [X] T094 [P] [US5] Implement SfxDetectionResult struct in src/sfx/result.rs with all fields (is_sfx, archive_format, data_offset, stub_type, confidence: f32 (0.0-1.0))
- [X] T095 [US5] Implement SfxDetectionResult::not_sfx() helper in src/sfx/result.rs
- [X] T096 [P] [US5] Implement archive signature constants in src/sfx/signatures.rs (ZIP, RAR, RAR5, 7z — per AD 0015, TAR/gzip/bzip2/xz signatures removed from SFX detection)
- [X] T097 [US5] Implement detect_executable_format() in src/sfx/stub_types.rs using goblin Object::parse()
- [X] T098 [US5] Implement scan_for_signatures() in src/sfx/signatures.rs — bounded heuristic scan of first 1MB for archive signatures
- [X] T099 [US5] ~~Implement validate_archive_at_offset()~~ (removed: validation integrated into detect_sfx pipeline)
- [X] T100 [US5] Implement detect_sfx() in src/sfx/detection.rs orchestrating 3-stage pipeline (Stage 1: exe validation, Stage 2: signature scan, Stage 3: archive validation)
- [X] T101 [US5] Implement Archive::detect_sfx() in src/sfx.rs (module root) as public API
- [~] T102 [US5] Implement Archive::open_sfx() in src/archive.rs as convenience method (detect + open_at_offset) — **partial**: detection works but open_at_offset returns `ArchiveError::Unsupported` (FR-029 deferred; depends on T103)
- [ ] T103 [US5] Implement Archive::open_at_offset() in src/archive.rs for opening archive from specific byte offset — **deferred:** returns `ArchiveError::Unsupported`
- [X] T104 [US5] Implement extract_stub() in src/sfx.rs (module root) for FR-031 (extract stub separately for security analysis) - Returns Vec<u8> containing executable stub bytes from offset 0 to data_offset
- [X] T105 [US5] Wire up sfx module in src/lib.rs re-exports (pub use sfx::*)
- [X] T106 [US5] Create example in examples/detect_sfx.rs demonstrating detection and extraction
- [X] T107 [US5] Create SFX test fixtures — no dedicated directory; synthetic test cases created programmatically in integration tests
- [X] T108 [P] [US5] Create integration test in tests/integration/sfx_detection.rs verifying synthetic detection coverage (SC-016) - 10 tests passing
- [X] T109 [P] [US5] Create integration test in tests/integration/sfx_false_positives.rs verifying zero false positives on synthetic corpus (SC-018) - 20 tests passing
- [X] T110 [P] [US5] Create benchmark in benches/sfx_detection.rs for SFX detection latency measurement (SC-017 target: <100ms)
- [ ] T110a [P] [US5] Verify >90% test coverage for src/sfx/ module using cargo-tarpaulin or similar tool (Constitution Principle IV requirement) - ⚠️ Coverage report created in docs/sfx_coverage_report.md: Current ~40-50% (far below >90% gate); pending T110c-T110g completion and official-tool SFX samples. **Note:** Other phases claim testing-gate passage at lower thresholds; this inconsistency should be reconciled before v0.1.0 release.
- [X] T110b [P] [US5] Create integration test verifying accurate data_offset values (SC-020) - Validated through numeric offset and stub content assertions in existing SFX extraction tests (tests/integration/sfx_detection.rs)
- [X] T110c [P] [US5] Add unit tests for src/sfx/signatures.rs edge cases (empty signatures, malformed bytes, boundary conditions) ✅ 15 new tests added
- [X] T110d [P] [US5] Add unit tests for src/sfx/stub_types.rs error paths (invalid executables, truncated headers, unsupported formats) ✅ 18 new tests added
- [X] T110e [P] [US5] Add unit tests for src/sfx/result.rs constructors and helper methods ✅ 18 new tests added
- [X] T110f [P] [US5] Add unit tests for src/sfx/detection.rs validation logic (offset bounds, file size limits, concurrent access) ✅ 12 new tests added
- [ ] T110g [P] [US5] Re-run coverage verification after T110c-T110f to confirm >90% target met - 99 SFX tests now passing

**Checkpoint**: SFX detection (US5) is partially functional. Remaining exclusions: T103 open_at_offset deferred (returns Unsupported), T102 open_sfx therefore partial, T110a coverage ~40-50% (below >90% target), T110g coverage re-verification pending, official-tool SFX samples (7-Zip, WinRAR, makeself) not yet collected.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T111 [P] Add comprehensive documentation comments (///) to all public types and methods in src/lib.rs ✅ (lib.rs already comprehensive)
- [ ] T112 [P] Add inline documentation examples to Archive methods in src/archive.rs using doc tests (deferred - existing docs sufficient for v0.1.0)
- [ ] T113 [P] Contract tests for archive API — existing in tests/contract/archive_contract.rs (deferred expansion to v0.2.0 priority)
- [ ] T114 [P] Contract tests for inspection API — existing in tests/contract/inspection_contract.rs (deferred expansion)
- [ ] T115 [P] Contract tests for extraction API — existing in tests/contract/extraction_contract.rs (deferred expansion)
- [ ] T116 [P] Create criterion benchmark in benches/inspection.rs for 10k file archive (target <1 second) (deferred - performance acceptable)
- [ ] T117 [P] Create criterion benchmark in benches/extraction.rs for 10GB archive (target <100MB memory) (deferred - v0.2.0)
- [X] T117a [P] Create lightweight streaming smoke test in tests/integration/streaming_memory.rs (10MB archive, verify <20MB memory delta during extraction) — smoke test only, not full FR-012 (10GB/<100MB) verification ✅ Implemented
- [ ] T118 [P] Add integration test in tests/integration/large_archives.rs for 10GB streaming validation (deferred - v0.2.0; file not yet created — placeholder for deferred large-archive integration tests)
- [ ] T119 [P] Add integration test in tests/integration/format_compatibility.rs for cross-format consistency (deferred)
- [X] T120 [P] Create tests/integration/concurrency.rs with test harness for thread-safety validation (FR-020, FR-021) ✅ Implemented with 7 tests
- [X] T120a [P] Test concurrent Archive::open() calls on different files from multiple threads (FR-020) - Required for thread-safety verification ✅ Implemented
- [X] T120b [P] Test multiple archive handles used concurrently without blocking (FR-021) ✅ Tests exist in tests/integration/concurrency.rs
- [X] T120c [P] Test concurrent extraction operations on different archives ✅ Tests exist in tests/integration/concurrency.rs
- [X] T121 Update README.md with installation instructions, API examples, format support matrix, and SFX detection usage ✅
- [X] T122 [P] Add CONTRIBUTING.md with development setup, testing guidelines, and PR process ✅
- [X] T123 [P] Create CHANGELOG.md documenting version 0.1.0 features including archive creation, modification, and SFX detection ✅
- [X] T124 Run `cargo clippy -- -D warnings` and fix all lints ✅ (all 8 linting issues fixed)
- [X] T125 Run `cargo fmt` to ensure consistent code formatting ✅
- [X] T126 Run `cargo test` and ensure all tests pass ✅ (test count varies as suite grows; run `cargo test -- --list` for current inventory)
- [ ] T127 Run `cargo bench` and verify performance targets met (inspection <1s, extraction <20% vs native, SFX <100ms) (deferred - v0.2.0)
- [X] T127a [P] Verify zero JVM dependencies for SC-015 ✅ (Verified: pure Rust + libarchive/UnRAR FFI; optional `external-rar-create` feature uses WinRAR CLI)
- [X] T127b [P] Create basic extraction throughput comparison test in tests/integration/performance_baseline.rs (compare unified-archive vs native 7z CLI on 100MB archive, document ratio for SC-010 verification) ✅ Implemented

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (Phase 4)**: Depends on Foundational (Phase 2) - Can work with US1 but independently testable
- **User Story 3 (Phase 5)**: Depends on Foundational (Phase 2) - Independently testable
- **User Story 4 (Phase 6)**: Depends on Foundational (Phase 2) - May reuse US3 logic but independently testable
- **User Story 5 (Phase 7)**: Depends on Foundational (Phase 2) - Uses existing format detection, independently testable
- **Polish (Phase 8)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Benefits from US1 Archive::open() but independently testable
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - Independently testable
- **User Story 4 (P4)**: Can start after Foundational (Phase 2) - May reuse US3 add logic but independently testable
- **User Story 5 (P5)**: Can start after Foundational (Phase 2) - Uses existing format detection (T014), independently testable

### Within Each User Story

- FFI wrappers before high-level APIs
- Core structs (Archive, ArchiveEntry) before operations
- Basic operations before advanced features
- Examples after implementation
- Integration tests after examples

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel (T003, T004, T005, T008, T009, T010)
- All Foundational tasks marked [P] can run in parallel within Phase 2 (T012, T015, T019, T022)
- Once Foundational phase completes, user stories can start in parallel (if team capacity allows)
- Within each user story, tasks marked [P] can run in parallel
- All Polish tasks marked [P] can run in parallel (T111-T127b)

---

## Parallel Example: User Story 1

```bash
# After Foundational complete, launch User Story 1 core structs in parallel:
Task T024: "Implement Archive struct skeleton in src/archive.rs"
Task T025: "Implement ArchiveMode enum in src/archive.rs"
# Both operate on same file, run sequentially

# Then operations can start:
Task T026: "Implement Archive::open()"
Task T027: "Implement Archive::open_encrypted()"
# Sequential (same file)

# Later parallel work:
Task T039: "Create example in examples/inspect_archive.rs"
Task T040: "Create integration test in tests/integration/format_compatibility.rs"
# Different files, can run in parallel
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1 (Archive Inspection)
4. **STOP and VALIDATE**: Test User Story 1 independently
   - Run examples/inspect_archive.rs on various formats
   - Verify metadata consistency across formats
   - Check performance (<1s for 10k files)
5. Deploy/demo if ready

### Incremental Delivery (MVP + Extraction)

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 → Test independently → Demo/Document (Inspection MVP)
3. Add User Story 2 → Test independently → Demo/Document (Full read capability MVP)
4. Add User Story 3 → Test independently → Deploy/Demo (Read/Write complete)
5. Add User Story 4 → Test independently → Deploy/Demo (Full archive management)
6. Add User Story 5 → Test independently → Deploy/Demo (Security use case: SFX detection)
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (Inspection)
   - Developer B: User Story 2 (Extraction) - can start, may need to sync on Archive::open()
   - Developer C: User Story 3 (Creation)
   - Developer D: User Story 5 (SFX Detection) - independent, uses format detection from foundational
3. Stories complete and integrate independently
4. User Story 4 (Modification) can start once US3 is complete (reuses add logic)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
- FFI complexity: libarchive bindings (TAR family, creation) and UnRAR bindings (RAR/RAR5) are foundational; ZIP uses pure-Rust Piz/zip crates, 7z uses sevenz-rust2
- Performance targets: Validate in Phase 8 benchmarks (T116-T117, T110 for SFX)
- Format auto-detection is critical to unified interface (T014)
- Streaming is critical for memory bounds (T045)
- SFX detection (US5) is independent and can be developed in parallel with other stories after foundational phase
- goblin dependency (T091) adds binary parsing capability for SFX detection with zero FFI overhead (pure Rust)

---

## Task Count Summary

> **Note**: This summary is a historical snapshot. Task completion status and test counts evolve over time. Consult the task list above and `cargo test -- --list` for current state.

- **Phase 1 (Setup)**: 10 tasks ✅ Complete
- **Phase 2 (Foundational)**: 16 tasks ✅ Complete
- **Phase 3 (US1 - Inspection)**: 19 tasks ✅ Complete
- **Phase 4 (US2 - Extraction)**: 16 tasks ✅ Complete
- **Phase 5 (US3 - Creation)**: 12 tasks (9 complete, 1 partial, 2 deferred)
- **Phase 6 (US4 - Modification)**: 9 tasks ✅ Complete (OI-025-001/002/003 all resolved)
- **Phase 7 (US5 - SFX Detection)**: 27 tasks (23 complete, 2 partial [T102, T103 deferred], 2 pending [T110a, T110g])
- **Phase 8 (Polish)**: 23 tasks (14 complete, 9 deferred)

**Total**: 132 tasks

**Implementation Status (last updated 2026-04-12)**: Active development continues with review-driven improvements.
