# Specification Quality Checklist: unified-archive

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2025-10-30
**Feature**: [spec.md](../spec.md)

> **Post-v0.1.0 reality check (2026-04-18):** Checklist rows below are stale — they were last audited before 0.1.0 shipped. Canonical current behavior lives in the source tree and `docs/architecture/decisions/*.md`. Known shifts that affect several rows: (1) FR-029 / SC-019 / `open_at_offset()` — the method is **implemented** (temp-file-backed) and no longer returns `Unsupported`; (2) AD 0019 supersedes AD 0018 for *read/extract* of standalone `.gz` / `.bz2` / `.xz` — only stream *creation* remains out of scope.

## Content Quality

- [~] No implementation details (languages, frameworks, APIs)
  - **Status**: PARTIAL — spec now contains implementation-specific details (crate names, buffer sizes, backend choices) alongside requirements
- [x] Focused on user value and business needs
  - **Status**: PASS - All user stories describe developer needs and value propositions, emphasizing unified interface benefits
- [~] Written for non-technical stakeholders
  - **Status**: PARTIAL — Earlier revisions were stakeholder-friendly; post-implementation updates introduced crate names, buffer sizes, and algorithmic detail (e.g. 1MB scan limit in FR-025) that assume implementation context
- [x] All mandatory sections completed
  - **Status**: PASS - User Scenarios, Requirements, Success Criteria, Reference Projects all present and complete

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
  - **Status**: PASS - Zero clarification markers, all decisions made with documented assumptions including unified interface approach
- [~] Requirements are testable and unambiguous
  - **Status**: PARTIAL — Most of the 31 functional requirements are specific and verifiable, but known exceptions remain:
    - FR-029 (`open_at_offset`): deferred, returns `ArchiveError::Unsupported`
    - FR-027 unknown/custom SFX handling: resolved (OI-027-001)
    - `CompressionOptions.progress` (creation progress): resolved per-entry (OI-025-003, AD 0021)
  - **Note**: Original spec had 21 FRs; expanded to 31 after SFX addition (Update 3).
- [~] Success criteria are measurable
  - **Status**: PARTIAL — Most of the 20 success criteria include specific metrics; however SC-019 (`open_at_offset`) lacks a reproducible measurement because the feature returns `Unsupported`, and SC-020 offset-accuracy is validated against synthetic files only
  - **Note**: Original spec had 15 SCs; expanded to 20 after SFX addition (Update 3).
- [~] Success criteria are technology-agnostic (no implementation details)
  - **Status**: PARTIAL — High-level criteria (SC-001 to SC-004) are outcome-focused, but backend-specific notes on streaming behaviour, error shapes, and metadata population tie several criteria to implementation details
- [~] All acceptance scenarios are defined
  - **Status**: PARTIAL — Each user story has 4-5 concrete acceptance scenarios; unknown/custom SFX handling is now resolved (OI-027-001) with synthetic test coverage
- [x] Edge cases are identified
  - **Status**: PASS - 14 edge cases covering Unicode, memory limits, permissions, errors, concurrency, plus 4 SFX-specific cases
- [~] Scope is clearly bounded
  - **Status**: PARTIAL — Clear priorities P1-P5, MVP focus on inspection+extraction; however active exclusions exist: standalone gzip/bzip2/xz (TAR compounds only per AD 0018), `open_at_offset()` deferred. Creation progress (OI-025-003) and unknown/custom SFX (OI-027-001) are resolved.
- [x] Dependencies and assumptions identified
  - **Status**: PASS - 20 documented assumptions covering 7zip-JBinding model, format auto-detection, reference projects, FFI approach, plus 6 SFX-specific assumptions

## Feature Readiness

- [~] All functional requirements have clear acceptance criteria
  - **Status**: PARTIAL — Most requirements map to user story acceptance scenarios; FR-029 (`open_at_offset`) has no exercisable acceptance path (returns `Unsupported`), and traceability for FR-029 is incomplete
- [x] User scenarios cover primary flows
  - **Status**: PASS - 5 user stories cover inspect, extract, create, modify, and SFX detection with clear priorities (P1-P5)
- [~] Feature meets measurable outcomes defined in Success Criteria
  - **Status**: PARTIAL — Most success criteria align with user stories and functional requirements; unmet criteria:
    - SC-019: `open_at_offset()` returns `Unsupported` (deferred)
    - SC-016: Detection coverage validated on synthetic samples only; official-tool SFX samples still planned
    - SC-018: False-positive corpus is narrow (synthetic plain archives, text, random data, small executable set)
    - `CompressionOptions.progress`: resolved per-entry (OI-025-003, AD 0021)
- [~] No implementation details leak into specification
  - **Status**: PARTIAL — see Content Quality item above; spec now references crate names and backend details

## Validation Summary

**Overall Status**: HISTORICAL SNAPSHOT (PARTIAL) — Feature has been implemented. This checklist records the original requirements review with post-implementation corrections.

Most checklist items passed on first validation. Subsequent review identified additional gaps: deferred functionality (`open_at_offset`), synthetic-only SFX validation, backend-specific details leaking into criteria, and algorithmic detail in the spec. Unknown SFX (OI-027-001) and creation progress (OI-025-003) are now resolved. Items previously marked PASS have been downgraded to PARTIAL where evidence warrants.

### Strengths

1. **Unified interface focus**: All user stories, requirements, and success criteria emphasize a shared API shape with documented backend/format caveats across 7z, RAR, RAR5, ZIP, TAR variants, and ISO — matching 7zip-JBinding's proven design (standalone GZIP/BZIP2/XZ only as TAR compounds per AD 0018)
2. **Excellent prioritization**: P1-P4 priorities enable incremental delivery with clear MVP (inspection + extraction as essential read operations)
3. **Comprehensive requirements**: 31 functional requirements organized by category (Unified Interface, Format Support, Platform, Error Handling, Security, Metadata, Compression, Concurrency, SFX Detection)
4. **Measurable success**: 20 quantified success criteria with dedicated unified interface metrics (SC-001 to SC-004) and SFX criteria (SC-016 to SC-020)
5. **Reference projects documented**: 7zip-JBinding as primary reference, archive-reader and compress-tools as Rust implementation references
6. **Well-defined assumptions**: 20 assumptions document decisions including unified interface model, format auto-detection, FFI approach, RAR5 support, and SFX-specific constraints
7. **Rich edge cases**: 14 edge cases anticipate real-world complexity (Unicode, permissions, concurrency, SFX scan limits, multiple embedded archives)
8. **Metadata focus**: P1 inspection emphasizes critical metadata (modification date, size, CRC32) for validation workflows
9. **Format auto-detection**: Explicit requirement (FR-002) and success criteria (SC-002) for automatic format detection without developer specification

### Historical Note

This specification has been implemented. The checklist above records the original requirements review state.

## Notes

- Spec originally avoided implementation details; post-implementation updates introduced crate names and backend specifics (marked PARTIAL above)
- Assumptions section effectively documents decisions that would otherwise need clarification
- User stories are independently testable, supporting incremental delivery
- Success criteria are mostly measurable without implementation knowledge, though backend-specific streaming, error shapes, and metadata population introduce dependencies on implementation detail

## Update History

**2025-11-11 (Update 3)**: SFX detection feature addition
- **MAJOR ADDITION**: Added User Story 5 - Self-Extracting Archive (SFX) Detection (Priority P5)
- Added 8 acceptance scenarios covering:
  - Windows PE executables (ZIP SFX, WinRAR SFX, 7-Zip SFX)
  - Linux ELF and macOS Mach-O executables with embedded archives
  - Unix ScriptInterpreter-based SFX archives (shell script stubs)
  - Regular archives and non-archive executables (negative cases)
  - Custom/unknown stubs (resolved per OI-027-001)
- Added 7 new functional requirements (FR-025 to FR-031):
  - FR-025: Signature scanning within first 1MB
  - FR-026: SfxDetectionResult structure with is_sfx, archive_format, data_offset, stub_type, confidence
  - FR-027: Cross-platform detection (PE/ELF/Mach-O/Script)
  - FR-028: Embedded archive format identification
  - FR-029: Reports data_offset only; direct opening deferred
  - FR-030: Graceful false positive handling
  - FR-031: Separate stub extraction for security analysis
- Added 5 new success criteria (SC-016 to SC-020):
  - SC-016: PARTIAL — Synthetic detection coverage for PE, ELF, Mach-O, and script stubs only. Official-tool sample testing is still planned; validation is not yet considered complete.
  - SC-017: <100ms detection time for files up to 10MB
  - SC-018: PARTIAL — False-positive testing covers a narrow synthetic corpus (plain archives, text, random data, small set of executables); broader negative-corpus evidence needed
  - SC-019: PARTIAL — Detection works but `open_at_offset()` returns `ArchiveError::Unsupported`; this gap affects the top-level Feature Readiness assessment
  - SC-020: PARTIAL (synthetic-only) — Offset accuracy validated via numeric assertions and stub content checks on synthetic files; no official-tool or real-world SFX samples tested yet
- Added 6 new assumptions (15-20) covering:
  - SFX stub size limits (<1MB typical)
  - Archive signature reliability and uniqueness
  - Platform executable format detection
  - SFX creation tool coverage (7-Zip, WinRAR, makeself)
  - Security scanning use case requirements
  - Cross-platform detection vs. execution limitations
- Added 4 new edge cases for SFX:
  - Scan limits and larger stub handling
  - Multiple embedded archives — design intent: detection iterates all candidates and returns first that validates (per AD 0015); behaviour in edge cases is not fully settled
  - Corrupted SFX or damaged embedded archives
  - Platform-specific executable format handling
- Added new key entity: **SfxDetectionResult** with 5 attributes

**Validation Status**: PARTIAL — 14 checklist items validated for SFX additions; multiple items downgraded to PARTIAL after review
- Detection was originally described behaviorally. The spec now includes algorithmic detail (1MB scan limit per FR-025), conflicting with the Content Quality requirement for no implementation details. This is acknowledged and tracked.
- Clear user value (security scanning, malware analysis, automated processing)
- 8 acceptance scenarios defined; unknown/custom SFX resolved (OI-027-001)
- 5 success criteria: SC-016 and SC-018 partial (synthetic-only), SC-019 deferred (`open_at_offset`), SC-020 synthetic-only
- Comprehensive edge cases and assumptions

**2025-10-30 (Update 2)**: Unified interface emphasis based on user feedback
- **MAJOR ADDITION**: Emphasized unified interface requirement across all user stories (P1-P4)
- All user stories now explicitly state "using a unified interface" that works identically for all formats
- Added 4 critical unified interface functional requirements (FR-001 to FR-004):
  - Format-agnostic API for all operations
  - Automatic format detection from file content
  - Consistent metadata structures across formats
  - Transparent handling of format-specific features
- Reorganized functional requirements into 7 categories for clarity (21 at this point; later expanded to 31 with SFX)
- Added "Reference Projects & Prior Art" section documenting:
  - 7zip-JBinding as primary reference for unified interface design
  - archive-reader and compress-tools as Rust implementation references
  - API patterns to replicate from 7zip-JBinding
- Expanded success criteria from 11 to 15, adding dedicated unified interface criteria:
  - SC-001 to SC-004 specifically verify API uniformity across formats
  - SC-007 verifies code portability across formats (change only file path, not API)
- Expanded assumptions from 10 to 14, organized into 4 categories:
  - Unified Interface Assumptions (7zip-JBinding model, format auto-detection, reference projects)
  - Archive Format & Compatibility (RAR5 support added as explicit requirement)
  - Performance & Platform (FFI approach documented)
  - Development & API assumptions
- Added explicit RAR5 support requirement (FR-007)
- All acceptance scenarios now emphasize format-agnostic API usage

**2025-10-30 (Update 1)**: Priority adjustment based on user feedback
- User Story priorities reordered: Inspection (P1, MVP) → Extraction (P2, MVP) → Creation (P3) → Modification (P4)
- Rationale updated: Inspection is foundational - must understand archive contents before extraction
- Both P1 and P2 marked as MVP features (essential "read" operations)
- Success Criteria reordered to prioritize inspection (SC-001, SC-002 for inspection, SC-003+ for extraction/other)
- Added emphasis on metadata retrieval: modification date, file size, CRC32 for each file
