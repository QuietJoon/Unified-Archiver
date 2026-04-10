# Design Baseline

## Baseline Metadata

- **Baseline ID:** BL-001-retroactive
- **Date:** 2026-04-10
- **Status:** approved
- **Replaces:** (none — initial baseline)
- **Stub manifest version:** 1

## Scope

### In-Scope
- Archive inspection across all supported formats (ZIP, 7z, RAR, RAR5, TAR variants, GZIP, BZIP2, XZ, ISO)
- Archive extraction with progress, passwords, parallel execution, CRC verification
- Archive creation (ZIP, 7z, TAR variants) with compression levels and encryption
- Archive modification (add, remove, replace entries) via copy-on-write
- SFX detection (PE, ELF, Mach-O, Script stubs) with 3-stage pipeline

### Out-of-Scope
- ISO creation, `open_at_offset`, SecStr migration, split creation, true streaming, backend trait abstraction, interactive prompts, Windows/Linux production support

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
| SCN-CRE-04 | Create from large dataset with progress | No | No |
| SCN-CRE-05 | Create with compression level control | No | No |
| SCN-MOD-01 | Add files to existing archive | Yes | No |
| SCN-MOD-02 | Remove entries from archive | Yes | No |
| SCN-MOD-03 | Replace file in archive | No | No |
| SCN-MOD-04 | Modify large archive efficiently | No | No |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | Yes | No |
| SCN-SFX-02 | Detect WinRAR SFX | Yes | No |
| SCN-SFX-03 | Detect 7-Zip SFX | Yes | No |
| SCN-SFX-04 | Detect Linux ELF SFX | Yes | No |
| SCN-SFX-05 | Detect shell script SFX | Yes | No |
| SCN-SFX-06 | Reject standard archive (not SFX) | Yes | No |
| SCN-SFX-07 | Reject non-archive executable | Yes | No |
| SCN-SFX-08 | Detect SFX with unknown stub | No | No |

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
| `specs/001-unified-archive/contracts/archive.md` | Archive lifecycle and core operations |
| `specs/001-unified-archive/contracts/inspection.md` | Inspection operations |
| `specs/001-unified-archive/contracts/extraction.md` | Extraction operations |
| `specs/001-unified-archive/contracts/creation.md` | Creation operations |
| `specs/001-unified-archive/contracts/progress.md` | Progress callback contract |
| `specs/001-unified-archive/contracts/streaming.md` | Streaming extraction contract |
| `specs/001-unified-archive/contracts/errors.md` | Error handling contract |

### Internal Interface Inventory
| Interface | Location | Purpose |
|---|---|---|
| `ArchiveBackend` enum dispatch | `src/archive.rs` | Route operations to correct backend |
| Safe wrapper layer | `src/ffi/*_wrapper.rs` | Isolate unsafe FFI behind safe Rust API |
| Security preflight | `src/security.rs` | Path sanitization and extraction limits |

## Placeholder Policy

- **STUB entries expected:** 0 (implementation complete for all in-scope scenarios)
- **DEFERRED entries allowed:** 5 (see `docs/project/stub-manifest.md`)
- **Why DEFERRED exists:** These features are explicitly out of MVP scope. The library is functional without them, workarounds exist, and they represent future enhancement opportunities.

## Handoff Readiness Checklist

- [x] Structural skeleton compiles and tests pass (`cargo test`)
- [x] Local smoke path runs (full test suite, 82+ tests)
- [x] Examples exist and run (`examples/` — 11 examples)
- [x] Benchmarks exist (`benches/` — 3 benchmark suites)
- [x] API documentation available (`cargo doc --open`)
- [x] Contract documents exist (7 files at `specs/001-unified-archive/contracts/`)
- [x] Stub manifest complete (5 DEFERRED entries documented)
- [x] Implementation slice checklists cover all mandatory scenarios
- [x] Phase-state and status synchronized to this baseline

## Linked Design Change Records

None (initial baseline).
