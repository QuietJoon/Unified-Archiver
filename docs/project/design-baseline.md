# Design Baseline

## Baseline Metadata

- **Baseline ID:** BL-001-retroactive
- **Date:** 2026-04-10
- **Status:** approved
- **Replaces:** (none — initial baseline)
- **Stub manifest version:** 1

## Scope

### In-Scope
- Archive inspection across all supported formats (ZIP, 7z, RAR, RAR5, TAR variants including .tar.gz/.tar.bz2/.tar.xz, ISO). Standalone GZIP, BZIP2, XZ are not supported; these compression formats are only handled as TAR compound variants per AD 0018.
- Archive extraction with progress, passwords, parallel execution, CRC verification
- Archive creation (ZIP, 7z, TAR variants) with compression levels and encryption (ZIP-only; 7z/TAR do not support creation encryption)
- Archive modification (add, remove, replace entries) via copy-on-write — partial: `commit_changes()` uses full rewrite and loses metadata/settings in some flows (OI-025-001, OI-025-002)
- SFX detection (PE, ELF, Mach-O, ScriptInterpreter per AD 0016) with 3-stage pipeline

### Out-of-Scope
- ISO creation, `open_at_offset` (deferred, non-functional on positive SFX matches), SecStr migration (not integrated in runtime API; passwords use `Option<String>`), split creation, true streaming, backend trait abstraction, interactive prompts, Windows/Linux production support

## Mandatory Scenarios

| ID | Name | Core Verification? | Critical Negative Path? |
|---|---|---|---|
| SCN-INS-01 | Inspect archive contents (unified API) | Yes | No |
| SCN-INS-02 | Inspect password-protected metadata | Yes | Yes |
| SCN-INS-03 | Validate integrity via CRC32 | Yes | Yes |
| SCN-INS-04 | Inspect multi-part archive | No | No |
| SCN-INS-05 | Filter large archive listings | No | No |
| SCN-EXT-01 | Extract all files (unified API) | Yes | No |
| SCN-EXT-02 | Extract password-protected archive | Yes | Yes |
| SCN-EXT-03 | Extract multi-layer compressed | Yes | No |
| SCN-EXT-04 | Handle corrupted archive | Yes | Yes |
| SCN-EXT-05 | Monitor extraction progress | No | No |
| SCN-CRE-01 | Create archive in multiple formats | Yes | No |
| SCN-CRE-02 | Create with max compression | No | No |
| SCN-CRE-03 | Create password-protected archive | No | No |
| SCN-CRE-04 | Create from large dataset with progress | No (RESOLVED -- OI-025-003: per-entry progress with `total=None`, AD 0021) | No |
| SCN-CRE-05 | Create with compression level control | No | No |
| SCN-MOD-01 | Add files to existing archive | Yes | No |
| SCN-MOD-02 | Remove entries from archive | Yes | No |
| SCN-MOD-03 | Replace file in archive | No | No |
| SCN-MOD-04 | Modify large archive efficiently | No | No |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | Yes | No |
| SCN-SFX-02 | Detect WinRAR SFX | Yes | No |
| SCN-SFX-03 | Detect 7-Zip SFX | Yes | No |
| SCN-SFX-04 | Detect Linux ELF SFX | Yes | No |
| SCN-SFX-05 | Detect ScriptInterpreter SFX (per AD 0016) | Yes | No |
| SCN-SFX-06 | Reject standard archive (not SFX) | Yes | No |
| SCN-SFX-07 | Reject non-archive executable | Yes | No |
| SCN-SFX-08 | Detect SFX with unknown stub | No (RESOLVED -- OI-027-001: unknown stubs proceed to signature scanning) | No |

## System Shape

- **Type:** Single library crate (layered modular monolith with backend strategy/facade)
- **Key binaries/processes:** None — library only
- **Communication style:** Direct Rust function calls

## Ownership and Source of Truth

- **Module map:** `docs/implementation/module-map.md`
- **Rough schema:** `docs/architecture/rough-schema.md`
- **Persistence and files:** `docs/architecture/persistence-and-files.md`
- **Source-of-truth table:** N/A (single-crate library, no shared data between processes)

## Contracts and Boundaries

### Contract Inventory
| Location | Content |
|---|---|
| `specs/001-unified-archive/contracts/archive.md` | Archive lifecycle and core operations (includes creation) |
| `specs/001-unified-archive/contracts/inspection.md` | Inspection operations |
| `specs/001-unified-archive/contracts/extraction.md` | Extraction operations |
| `specs/001-unified-archive/contracts/progress.md` | Progress callback contract |
| `specs/001-unified-archive/contracts/streaming.md` | Streaming extraction contract |
| `specs/001-unified-archive/contracts/errors.md` | Error handling contract |

> **Note:** Public APIs `calculate_archive_crc()` and `calculate_manifest_digest()` are not covered by any contract document. These should be added to the inspection or archive contract, or documented as uncovered utility functions.

### Internal Interface Inventory
| Interface | Location | Purpose |
|---|---|---|
| `ArchiveBackend` enum dispatch | `src/archive.rs` | Route operations to correct backend |
| Safe wrapper layer | `src/ffi/*_wrapper.rs` | Isolate unsafe FFI behind safe Rust API |
| Security preflight | `src/security.rs` | Path sanitization and extraction limits |

## Placeholder Policy

- **STUB entries expected:** 0 (implementation complete for in-scope scenarios; SCN-CRE-04 and SCN-SFX-08 are deferred)
- **DEFERRED entries allowed:** 8 (see `docs/project/stub-manifest.md`)
- **Why DEFERRED exists:** These features are explicitly out of MVP scope. Item-by-item status:
  - `open_at_offset` -- workaround: `extract_stub()` returns the executable prefix bytes; callers can use the detected offset to slice the archive payload manually. No convenience API for opening the embedded archive yet.
  - `split_size` -- split archive creation not yet implemented; field exists but no writer honors it.
  - SecStr migration -- passwords use `Option<String>`; secstr not integrated in runtime API.
  - True streaming -- non-libarchive backends buffer then wrap in Cursor.
  - ZIP modification reliability -- `commit_changes()` loses metadata/settings (OI-025-001, OI-025-002).
  - Creation progress callbacks (OI-025-003) -- RESOLVED: per-entry progress invoked with `total=None` (AD 0021).
  - Unknown-stub SFX scanning (OI-027-001) -- RESOLVED: unknown stubs proceed to signature scanning.
  - `ModificationOptions` -- `create_backup`/`backup_suffix` honored via `modify_with_options()` (AD 0020); `preserve_metadata` no-op pending OI-025-002.

## Handoff Readiness Checklist

- [x] Structural skeleton compiles and tests pass (`cargo test`)
- [x] Local smoke path runs (full test suite, 867+ tests)
- [x] Examples exist (`examples/` -- 9 examples)
- [x] Benchmarks exist (`benches/` -- 5 benchmark suites)
- [x] API documentation available (`cargo doc --open`)
- [x] Contract documents exist (6 files at `specs/001-unified-archive/contracts/`: archive, inspection, extraction, progress, streaming, errors). Archive-level integrity coverage added 2026-04-13 (`calculate_archive_crc`, `calculate_manifest_digest`, `calculate_manifest_summary`); other content reconciled in IIR-001.
- [x] Stub manifest complete (8 DEFERRED entries documented; DEF-007 closed 2026-04-13)
- [x] Implementation slice checklists cover all mandatory scenarios
- [x] Phase-state and status synchronized to this baseline (synced 2026-04-13 — see IIR-001 closure notes)

## Linked Design Change Records

None (initial baseline).
