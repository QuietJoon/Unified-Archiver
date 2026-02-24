# Implementation Plan: unified-archive

**Branch**: `001-unified-archive` | **Date**: 2025-11-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-unified-archive/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This project implements a unified archive library for Rust that provides a single, format-agnostic API for handling multiple archive formats (7z, RAR, RAR5, ZIP, TAR, GZIP, BZIP2, XZ, ISO). The library recently added **SFX (Self-Extracting Archive) detection** capabilities to identify and extract embedded archives from executable files across multiple platforms (Windows PE, Linux/macOS ELF, Unix shell scripts).

**Primary Requirements**:
1. Unified interface for archive operations (inspection, extraction, creation, modification) across all formats
2. Automatic format detection from file content (magic bytes)
3. SFX detection with platform-agnostic API supporting Windows, Linux, macOS executables and shell scripts
4. Cross-platform compatibility (Windows, macOS, Linux) without JVM dependency
5. RAR/RAR5 format support through UnRAR SDK
6. Performance within 20% of native 7zip tools

**Technical Approach**:
- FFI bindings to proven native libraries (UnRAR SDK, libarchive)
- Rust 2024 edition with enhanced unsafe code requirements
- Pattern matching on file signatures for format and SFX detection
- Streaming APIs for memory-efficient large archive handling
- Platform-specific optimizations with unified public API

## Technical Context

**Language/Version**: Rust 2024 edition (minimum version 1.85)
**Primary Dependencies**:
- Core: once_cell 1.20+, crc32fast 1.4+, rayon 1.8+, secstr 0.5+, walkdir 2.4+
- FFI: UnRAR SDK (RAR/RAR5), libarchive (ZIP/7z/TAR/etc)
- Native Rust Backends: piz 0.5+ (ZIP extraction with parallel decompression), sevenz-rust2 0.19+ (7z extraction), zip 2.2+ (ZIP creation), memmap2 0.9+ (memory-mapped file access for piz)
- SFX Detection: goblin 0.9+ (PE/ELF/Mach-O parsing, fuzzed, pure Rust)

**Storage**: Files (archive files on disk, streaming for large archives)
**Testing**: cargo test, integration tests with real archive samples, property-based tests for invariants
**Target Platform**: Cross-platform library (Windows 10+, macOS 11+, Linux kernel 4.4+)
**Project Type**: Single library crate with FFI bindings

**Performance Goals**:
- Archive inspection: <1s for 10,000 files
- Extraction: within 20% of native 7zip performance (measured by extraction throughput MB/s on identical hardware)
- SFX detection: <100ms for files up to 10MB
- Memory: 64KB read buffers with <50MB total library overhead, enabling 10GB+ archives in <100MB total process memory (streaming)

**Per-Module Memory Budgets** (Constitution requirement):
- `src/extraction.rs`: 35MB max (read buffers + decompression state)
- `src/inspection.rs`: 10MB max (entry metadata caching)
- `src/sfx/`: 5MB max (signature scanning + executable parsing)
- `src/creation.rs`: 30MB max (compression buffers + write state)
- `src/modification.rs`: 40MB max (modification tracking + temp buffers)
- Remaining modules: 10MB combined (error handling, format detection, etc.)

**Constraints**:
- Zero JVM dependency (pure Rust + FFI to native libs)
- Zero false positives for SFX detection on 100+ non-SFX executables
- 100% detection rate for official SFX tools (7-Zip, WinRAR, makeself)
- Thread-safe for concurrent archive operations on different files

**Scale/Scope**:
- Support 10+ archive formats (7z, RAR, RAR5, ZIP, TAR, GZIP, BZIP2, XZ, ISO)
- Handle archives up to 10GB efficiently
- Process up to 10,000 files per archive
- Support 4 SFX stub types (PE, ELF, Mach-O, shell script)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Robustness & Stability (NON-NEGOTIABLE)

**Status**: ✅ PASS

- All error conditions explicitly handled (FR-010: distinguishes I/O, format, corruption errors)
- Input validation at boundaries (FR-030: graceful false positive handling for SFX detection)
- Graceful recovery (FR-011: CRC integrity validation, FR-014: consistent password failures)
- Clear error messages with context (FR-024: codec name + installation instructions)
- Invariants maintained (FR-023: overwrite protection unless explicitly enabled)

### II. Pragmatic Performance

**Status**: ✅ PASS

**Planned Optimizations**:
- SFX detection scans only first 1MB with 512-byte aligned chunks (FR-025) - **Small change, significant gain** (eliminates full file scans)
- CRC32 validation uses crc32fast crate with SIMD acceleration - **Small change, 10x+ speedup**
- Parallel extraction with rayon work-stealing - **Medium change, multi-core scaling**
- Streaming APIs for large archives (FR-012) - **Medium change, enables 10GB+ archives in <100MB memory**

All optimizations measured against benchmarks, targeting 20% of native 7zip performance (SC-010).

### III. Unified Interface with Platform-Optimized Backends

**Status**: ✅ PASS

- Unified public API across all platforms (FR-001, FR-008)
- Multiple backend libraries used (UnRAR SDK for RAR, libarchive for others)
- Backend selection transparent to consumers (FR-004)
- Format auto-detection eliminates explicit format specification (FR-002)

**Dependency Justification**: See spec.md "Technology Stack" section (lines 197-213) for detailed dependency justifications including UnRAR SDK, libarchive, once_cell, crc32fast, rayon, and secstr.

**SFX Detection**: goblin 0.9+ - Battle-tested with extensive fuzzing coverage (https://github.com/m4b/goblin - 100M+ inputs via cargo-fuzz, documented in repo CI), pure Rust, PE/ELF/Mach-O support, Rust 2024 compatible

### IV. Comprehensive Testing

**Status**: ✅ PASS

Testing strategy defined:
- Unit tests for SFX detection logic (>90% coverage target)
- Integration tests with real SFX samples from official tools (SC-016: 100% detection)
- Contract tests for public API (SfxDetectionResult structure, FR-026)
- Property-based tests for SFX detection invariants (no false positives, SC-018)
- Performance regression tests for detection latency (SC-017: <100ms for 10MB files)

### V. Clear Contracts & Documentation

**Status**: ✅ PASS

Contracts defined for SFX detection:
- Input: File path or byte stream
- Output: SfxDetectionResult (is_sfx, archive_format, data_offset, stub_type) (FR-026)
- Error conditions: Corrupted files, invalid formats, I/O errors (FR-030)
- Performance guarantees: <100ms for files up to 10MB (SC-017)
- Thread-safety: Concurrent detection on different files (FR-020)
- Usage examples planned in quickstart.md

**Overall Gate Status**: ✅ PASS - All clarifications resolved (goblin 0.9 selected for SFX detection)

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── lib.rs                    # Public API exports, unified interface
├── archive.rs                # Archive handle, format detection
├── entry.rs                  # ArchiveEntry, file metadata
├── error.rs                  # Error types (ArchiveError)
├── format.rs                 # ArchiveFormat enum, detection logic
├── compression.rs            # CompressionOptions, CompressionLevel
├── extraction.rs             # Extraction APIs (stub)
├── creation.rs               # Creation APIs (stub)
├── modification.rs           # Modification APIs (stub)
├── inspection.rs             # Inspection APIs (stub)
├── sfx/                      # NEW: SFX detection module
│   ├── mod.rs               # Public SFX API
│   ├── detection.rs         # Core detection logic
│   ├── signatures.rs        # Archive signatures (PK, Rar!, 7z)
│   ├── stub_types.rs        # Executable format detection (PE/ELF/Mach-O)
│   └── result.rs            # SfxDetectionResult structure
├── ffi/                      # FFI bindings
│   ├── mod.rs
│   ├── unrar.rs             # UnRAR SDK bindings
│   ├── libarchive.rs        # libarchive bindings
│   └── bindings.rs          # Auto-generated bindings (stub)
└── utils/                    # Utilities
    ├── crc32.rs             # CRC32 validation
    └── streaming.rs         # Streaming APIs

tests/
├── integration/
│   ├── mod.rs
│   ├── inspection.rs        # Archive inspection tests
│   ├── extraction.rs        # Extraction tests
│   ├── creation.rs          # Creation tests
│   ├── modification.rs      # Modification tests
│   ├── sfx_detection.rs     # NEW: SFX detection integration tests
│   ├── sfx_false_positives.rs # NEW: SFX false positive tests
│   └── concurrency.rs       # NEW: Thread-safety tests (T120-T120c)
└── fixtures/
    ├── test.rar             # Test archives
    ├── test_rar5.rar
    └── sfx/                 # NEW: SFX test fixtures

examples/
├── list_archive.rs          # Inspection example
├── extract_archive.rs       # Extraction example
├── create_archive.rs        # Creation example
└── detect_sfx.rs            # NEW: SFX detection example
```

**Structure Decision**: Single library crate structure. The project is a Rust library providing archive manipulation capabilities. The `src/sfx/` module is newly added for SFX detection, containing detection logic, signature matching, executable format identification, and result structures. The FFI bindings to UnRAR SDK and libarchive are isolated in `src/ffi/` for safety and maintainability.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitutional violations. All complexity is justified:
- Multiple backend libraries (UnRAR SDK, libarchive) justified by format complexity and proven stability
- FFI complexity justified by performance requirements (20% of native 7zip)
- SFX detection complexity justified by security use case and cross-platform requirements
