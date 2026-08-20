# Implementation Plan: unified-archive

**Branch**: `001-unified-archive` | **Date**: 2025-11-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-unified-archive/spec.md`

**Note**: Generated from speckit template (no longer present in repo).

> **Post-v0.1.0 reality check (2026-04-18):** Plan text below was written before 0.1.0 shipped and has not been line-edited. Canonical behavior is in the source tree, `docs/API_REFERENCE.md`, and the `docs/records/*.md` records. Known deltas still readable below: (1) `Archive::open_at_offset()` / `Archive::open_sfx()` are **shipped** (temp-file-backed), not deferred placeholders returning `Unsupported`; (2) passwords are `Option<SecStr>` (`secstr` is integrated); (3) standalone `.gz` / `.bz2` / `.xz` read/extract are supported per MADR-0019 (only *creation* of standalone streams is excluded per AD 0018).

## Summary

This project implements a unified archive library for Rust that provides a single, format-agnostic API for handling multiple archive formats (7z, RAR, RAR5, ZIP, TAR compound formats, ISO). Standalone gz/bz2/xz streams are not supported as independent formats — only their TAR compound variants (TAR.GZ, TAR.BZ2, TAR.XZ) are handled (per AD 0018). The library includes **SFX (Self-Extracting Archive) detection** capabilities to identify embedded archives and report detection metadata (format, offset, confidence) from executable files across multiple platforms (Windows PE, Linux ELF, macOS Mach-O, Unix shell scripts).

**Primary Requirements**:
1. Unified interface for archive operations (inspection, extraction, creation, modification) across supported formats (standalone gz/bz2/xz excluded per AD 0018; documented caveats per format)
2. Automatic format detection from file content (magic bytes)
3. SFX detection with platform-agnostic API supporting Windows, Linux, macOS executables and shell scripts
4. Cross-platform compatibility (Windows, macOS, Linux) without JVM dependency
5. RAR/RAR5 format support through UnRAR SDK
6. Performance target: within 20% of native 7zip tools (under verification, not yet a settled guarantee)

**Technical Approach**:
- FFI bindings to proven native libraries (UnRAR SDK, libarchive)
- Rust 2024 edition with enhanced unsafe code requirements
- Pattern matching on file signatures for format and SFX detection
- Streaming APIs for memory-efficient large archive handling (libarchive-backed formats; Piz, ZipReader, SevenZ, and UnRAR backends buffer entries)
- Platform-specific optimizations with unified public API

## Technical Context

**Language/Version**: Rust 2024 edition (minimum version 1.85)
**Primary Dependencies** (split-backend architecture):
- Core: once_cell 1.20+, crc32fast 1.4+, rayon 1.8+, walkdir 2.4+ (secstr deferred — passwords use `Option<String>`)
- Piz 0.5+ (ZIP extraction with parallel decompression), memmap2 0.9+ (memory-mapped file access for Piz)
- zip 2.2+ (encrypted ZIP extraction, ZIP creation) — referred to as ZipReader backend
- sevenz-rust2 0.19+ (7z extraction) — SevenZ backend
- libarchive (TAR family: TAR, TAR.GZ, TAR.BZ2, TAR.XZ; ISO support; archive creation) — Libarchive backend
- UnRAR SDK (RAR/RAR5 extraction) — UnRAR backend
- SFX Detection: goblin 0.9+ (PE/ELF/Mach-O parsing, fuzzed, pure Rust)

**Storage**: Files (archive files on disk, streaming for large archives)
**Testing**: cargo test, integration tests with archive samples (synthetic only for SFX; official-tool samples planned but not yet available — SFX testing gates are incomplete), property-based tests for invariants
**Target Platform**: Cross-platform library (Windows 10+, macOS 11+, Linux kernel 4.4+). **Note**: Primary development is on macOS; cross-platform verification is a target (SC-011).
**Project Type**: Single library crate with FFI bindings

**Performance Goals** (targets under verification — not yet settled as guarantees):
- Archive inspection: <1s for 10,000 files
- Extraction: within 20% of native 7zip performance (measured by extraction throughput MB/s on identical hardware)
- SFX detection: <100ms for files up to 10MB
- Memory usage is backend-dependent. Libarchive truly streams; Piz/SevenZ/UnRAR buffer entries in memory. See per-module notes below.

**Per-Module Memory Notes**:
- `src/extraction.rs`: Libarchive streams with small read buffers; Piz/SevenZ/UnRAR buffer full entries
- `src/inspection.rs`: Entry metadata cached after first list call
- `src/sfx.rs` + `src/sfx/`: Module root re-exports; detection scans first 1MB only; minimal memory footprint
- `src/creation.rs`: Libarchive handles compression streaming; buffer sizes vary by format
- `src/modification.rs`: Copy-on-write strategy via commit_changes(); temp file overhead proportional to archive size

**Constraints**:
- Zero JVM dependency (pure Rust + FFI to native libs)
- Low false-positive rate for SFX detection on synthetic test corpus (larger negative corpus planned, SC-018)
- Synthetic SFX detection coverage for all stub types (PE, ELF, Mach-O, ScriptInterpreter, Unknown); unknown/custom stubs proceed to signature scanning (OI-027-001 resolved)
- Thread-safe for concurrent archive operations on different files. RAR FFI calls are serialized via a process-wide mutex (`UNRAR_LOCK`); concurrent caller access is safe but RAR operations execute sequentially (OI-026-004 resolved).

**Scale/Scope**:
- Support 9 openable archive formats (7z, RAR, RAR5, ZIP, TAR.GZ, TAR.BZ2, TAR.XZ, TAR, ISO). Standalone GZIP/BZIP2/XZ enum variants exist but are not supported through `Archive::open()` (AD 0018).
- Target: handle archives up to 10GB (streaming extraction via libarchive is memory-efficient; Piz/SevenZ/UnRAR buffer entries and may require proportional memory)
- Process up to 10,000 files per archive
- 5 stub types enumerated (WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown); all 5 reach signature scanning — Unknown stubs proceed to Stage 2 and return `probable()` with confidence 0.9 when an archive signature is found (OI-027-001 resolved)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Robustness & Stability (NON-NEGOTIABLE)

**Status**: ⚠️ PARTIAL — tracked exceptions: open_at_offset deferred (returns Unsupported placeholder). Creation progress (OI-025-003) and unknown-stub SFX (OI-027-001) are resolved.

- All error conditions explicitly handled (FR-010: distinguishes I/O, format, corruption errors)
- Input validation at boundaries (FR-030: graceful false positive handling for SFX detection)
- Graceful recovery (FR-011: CRC integrity validation, FR-014: consistent password failures)
- Clear error messages with context (FR-024: codec name + installation instructions)
- Invariants maintained (FR-023: overwrite protection unless explicitly enabled)

### II. Pragmatic Performance

**Status**: ✅ PASS (conditional) — tracked exceptions: performance targets under verification. Creation progress is now wired per-entry (OI-025-003 resolved).

**Planned Optimizations**:
- SFX detection scans only first 1MB for archive signatures (FR-025) - **Small change, significant gain** (eliminates full file scans)
- CRC32 validation uses crc32fast crate with SIMD acceleration - **Small change, 10x+ speedup**
- Parallel extraction with rayon work-stealing - **Medium change, multi-core scaling**
- Streaming APIs for large archives (FR-012) - **Medium change, targets <100MB memory for 10GB+ archives via libarchive streaming** (ZIP/7z/RAR backends buffer entries in memory; overall target under verification)

All optimizations measured against benchmarks, targeting within 20% of native 7zip performance (SC-010, under verification).

### III. Unified Interface with Platform-Optimized Backends

**Status**: ✅ PASS (conditional) — standalone gz/bz2/xz excluded (AD 0018), RAR concurrency caveated, SFX offset opening deferred.

- Unified public API across all platforms (FR-001, FR-008)
- Multiple backend libraries used: Piz for ZIP extraction (parallel decompression), ZipReader (zip crate) for encrypted ZIP extraction, SevenZ (sevenz-rust2) for 7z extraction, UnRAR SDK for RAR/RAR5 extraction, libarchive for TAR family/ISO and all creation
- Backend selection transparent to consumers (FR-004)
- Format auto-detection eliminates explicit format specification (FR-002)

**Dependency Justification**: See spec.md "Technology Stack" section for detailed dependency justifications including UnRAR SDK, libarchive, once_cell, crc32fast, and rayon. Note: secstr is deferred; passwords currently use `Option<String>`.

**SFX Detection**: goblin 0.9+ - Battle-tested with extensive fuzzing coverage (https://github.com/m4b/goblin - 100M+ inputs via cargo-fuzz, documented in repo CI), pure Rust, PE/ELF/Mach-O support, Rust 2024 compatible

### IV. Comprehensive Testing

**Status**: ⚠️ PARTIAL — SFX tests are synthetic only (official-tool samples planned but not available), coverage verification pending. Creation progress is now wired per-entry (OI-025-003 resolved).

Testing strategy defined:
- Unit tests for SFX detection logic (>90% coverage target)
- Integration tests with synthetic SFX samples (real official-tool samples planned)
- Contract tests for public API (SfxDetectionResult structure, FR-026)
- Property-based tests for SFX detection invariants (no false positives, SC-018)
- Performance regression tests for detection latency (SC-017: <100ms for 10MB files)

### V. Clear Contracts & Documentation

**Status**: ⚠️ PARTIAL — SFX output contract includes confidence; usage examples in quickstart.md. Performance target pending formal benchmark verification; open_at_offset contract deferred.

Contracts defined for SFX detection:
- Input: File path
- Output: SfxDetectionResult (is_sfx, archive_format, data_offset, stub_type, confidence) (FR-026)
- Error conditions: Corrupted files, invalid formats, I/O errors (FR-030)
- Performance target: <100ms for files up to 10MB (SC-017, pending formal benchmark verification)
- Thread-safety: Concurrent detection on different files (FR-020)
- Usage examples available in quickstart.md

**Overall Gate Status**: ⚠️ PARTIAL with tracked exceptions (open_at_offset deferred, performance targets under verification, standalone formats excluded per AD 0018, SFX testing synthetic only). OI-025-003, OI-026-004, and OI-027-001 are resolved.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file
├── research.md          # Phase 0 research and decisions
├── data-model.md        # Core entity definitions
├── quickstart.md        # Usage examples and getting started
├── contracts/           # API contracts per operation area
└── tasks.md             # Implementation task ledger
```

### Source Code (repository root)

```text
src/
├── lib.rs                    # Public API exports, unified interface
├── archive.rs                # Archive handle, format detection
├── entry.rs                  # ArchiveEntry, file metadata
├── error.rs                  # Error types (ArchiveError)
├── format.rs                 # ArchiveFormat enum, detection logic
├── options.rs                # ExtractionOptions, CompressionOptions, CompressionLevel
├── stream_crc.rs             # CRC32 streaming validation (root; split into
│                             #   stream_crc/{digest,codec,detect}.rs — AD 0010 amendment 2026-08-21)
├── streaming.rs              # Streaming extraction APIs
├── extraction.rs             # Extraction APIs
├── creation.rs               # Creation APIs
├── modification.rs           # Modification APIs (copy-on-write via commit_changes())
├── inspection.rs             # Inspection APIs
├── sfx.rs                    # SFX module root (re-exports)
├── sfx/                      # SFX detection module
│   ├── detection.rs         # Core detection logic (3-stage pipeline)
│   ├── signatures.rs        # Archive signatures (PK, Rar!, 7z)
│   ├── stub_types.rs        # Executable format detection (PE/ELF/Mach-O)
│   └── result.rs            # SfxDetectionResult structure
├── ffi/                      # FFI bindings
│   ├── mod.rs
│   ├── common.rs            # Shared FFI utilities
│   ├── unrar.rs             # UnRAR SDK bindings
│   ├── libarchive.rs        # libarchive bindings
│   ├── wrapper.rs           # Safe wrapper types (RAII)
│   ├── piz_wrapper.rs       # Piz backend wrapper
│   ├── sevenz_wrapper.rs    # SevenZ backend wrapper
│   ├── zip_wrapper.rs       # ZipReader backend wrapper
│   ├── zip_writer.rs        # ZIP creation wrapper
│   └── libarchive_wrapper.rs # libarchive safe wrapper
├── security.rs               # Security checks (path traversal, zip bomb, CRC32)

tests/
├── integration/              # Integration test modules
│   ├── sfx_detection.rs     # SFX detection integration tests
│   ├── sfx_false_positives.rs # SFX false positive tests
│   ├── concurrency.rs       # Thread-safety tests (T120-T120c)
│   └── ...                  # Additional integration tests
├── contract/                 # Contract tests
├── creation_roundtrip_test.rs # Creation roundtrip validation
└── fixtures/
    ├── test.rar             # Test archives
    └── test_rar5.rar        # (SFX tests use synthetic in-memory fixtures, no sfx/ directory)

examples/
├── inspect_archive.rs       # Inspection example
├── extract_archive.rs       # Extraction example
├── streaming_extract.rs     # Streaming extraction example
├── create_archive.rs        # Creation example
└── detect_sfx.rs            # SFX detection example
```

**Structure Decision**: Single library crate structure. The project is a Rust library providing archive manipulation capabilities. SFX detection is split between `src/sfx.rs` (module root, re-exports) and `src/sfx/` (detection logic, signature matching, executable format identification, result structures). The FFI layer in `src/ffi/` contains multiple per-backend wrappers (piz_wrapper.rs, sevenz_wrapper.rs, zip_wrapper.rs, zip_writer.rs, libarchive_wrapper.rs) plus raw bindings (unrar.rs, libarchive.rs) and shared utilities (common.rs, wrapper.rs). Modification logic lives in `src/modification.rs` using a copy-on-write strategy via `commit_changes()`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No untracked constitutional violations. All complexity is justified (tracked exceptions in constitution gates above):
- Multiple backend libraries (Piz, ZipReader/zip, sevenz-rust2, UnRAR SDK, libarchive) justified by format complexity and proven stability
- FFI complexity justified by performance requirements (20% of native 7zip)
- SFX detection complexity justified by security use case and cross-platform requirements
