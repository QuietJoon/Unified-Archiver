# Clarifications Log: unified-archive

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Purpose**: Document ambiguity resolutions and design decisions

## Overview

This document records clarification questions asked during the specification review phase and the decisions made to resolve ambiguities.

> **Historical context.** The decisions below were made during the pre-implementation review (2025-10-30). Several have since been superseded by implementation changes. Superseded decisions are marked with inline notes, and a consolidated summary appears in the "Updated Entities" section. The original decision text is preserved for traceability.
>
> Key supersessions:
> - `CodecUnavailable` was split out from `Unsupported` for missing-codec errors; `Unsupported` retains its `details: Option<String>` field for non-codec cases.
> - `EntryType` now includes `Symlink`, `HardLink`, and `Other` variants for metadata representation, while extraction still skips symlinks/hardlinks per FR-022.

## Clarification Session 1: 2025-10-30

### Context

Pre-implementation review identified 3 critical ambiguities in edge case handling that required explicit decisions before implementation could begin.

### Q1: Symbolic Link & Hard Link Handling

**Question**: How should the library handle symbolic links and hard links during extraction and creation across different operating systems?

**Options Evaluated**:
1. Follow symlinks - Dereference and include target content
2. Store symlink metadata - Preserve as symlink where format supports
3. Skip symlinks - Ignore with warning (SELECTED)
4. Configurable via options - Developer choice

**Decision**: Option 3 - Skip symbolic links and hard links with warnings

**Rationale**:
- Cross-platform symlink handling is unreliable (Windows vs Unix semantics differ)
- Security concerns (symlink attacks during extraction)
- Not all archive formats support symlink metadata preservation
- Simplifies implementation and reduces attack surface
- Aligns with robustness principle from constitution

**Implementation Impact**:

*Representation (data model):* `EntryType` includes `Symlink`, `HardLink`, and `Other` variants so that archive metadata can be inspected without loss. These variants exist for listing and inspection purposes.

*Operation policy (extraction/creation):* Symlinks and hardlinks are skipped with warnings during extraction (FR-022). This is enforced as an operation-level policy, independent of the data model.

- Added FR-022: Library MUST skip symbolic links and hard links with warnings
- Updated `ArchiveEntry` invariants to document skip behavior
- Edge case marked as [RESOLVED]

**Files Updated**:
- `spec.md` - Added FR-022
- `data-model.md` - Updated `EntryType`. **Final state:** `EntryType` includes `File`, `Directory`, `Symlink`, `HardLink`, and `Other`. Symlinks/hardlinks are represented in metadata for inspection but skipped during extraction per FR-022.
- Edge cases section updated

---

### Q2: Overwrite Behavior During Extraction

**Question**: What should be the default behavior when extracting files that would overwrite existing files at the destination?

**Options Evaluated**:
1. Error and abort - Fail with clear error (SELECTED)
2. Skip existing files - Only extract non-existent files
3. Always overwrite - Replace silently
4. Configurable via ExtractionOptions.overwrite

**Decision**: Option 1 - Error and abort by default, configurable via `overwrite` flag

**Rationale**:
- Prevents accidental data loss (robustness principle)
- Makes destructive operations explicit (user must set `overwrite: true`)
- Clear error messages guide users to resolution
- Matches constitution requirement for explicit error handling
- Default behavior is safe, opt-in to overwrite

**Implementation Impact**:
- Added FR-023: Library MUST fail extraction when files would overwrite existing files unless `ExtractionOptions.overwrite` is true
- `ExtractionOptions.overwrite` default remains `false`
- Updated contract preconditions and error conditions
- Edge case marked as [RESOLVED]

**Files Updated**:
- `spec.md` - Added FR-023
- `data-model.md` - Added documentation to `overwrite` field and invariants
- `contracts/extraction.md` - Updated preconditions and error conditions
- `quickstart.md` - Added comment showing default behavior

---

### Q3: Missing Codec Handling

**Question**: What should happen when an archive requires a compression codec not available on the system?

**Options Evaluated**:
1. Error with codec name and installation instructions (SELECTED)
2. Attempt extraction anyway with available codecs
3. Partial extraction - Extract compatible files, skip others
4. Bundled codecs - Include all codecs in library

**Decision**: Option 1 - Return error with codec name and installation instructions

> **Superseded:** Missing codecs now use `ArchiveError::CodecUnavailable { codec, format, install_instructions }`, not `Unsupported`. Overwrite conflicts use `ArchiveError::Io` with `AlreadyExists` kind.

**Rationale**:
- Actionable error messages (constitution Principle V: Clear Contracts)
- User knows exactly what's missing and how to fix it
- Prevents silent failures or corrupted extraction attempts
- Maintains minimal dependencies (constitution Principle III)
- Aligns with explicit error handling requirement (FR-010)

**Implementation Impact**:
- Added FR-024: Library MUST return error with codec name and instructions
- **Superseded:** `ArchiveError::CodecUnavailable { codec, format, install_instructions }` now handles this case instead of `Unsupported` with `details`
- Updated extraction contract error conditions
- Edge case marked as [RESOLVED]

**Files Updated**:
- `spec.md` - Added FR-024
- `contracts/errors.md` - Added `CodecUnavailable` variant (supersedes original `Unsupported` enhancement)
- `contracts/extraction.md` - Added codec error to error conditions

---

## Summary of Changes

### New Functional Requirements

- **FR-022**: Skip symbolic links and hard links with warnings
- **FR-023**: Fail extraction on file collision unless overwrite=true
- **FR-024**: Return actionable error for missing codecs

### Updated Entities

**ArchiveError — Codec handling** (superseded):
```rust
// Original decision: add details to Unsupported
// Unsupported { operation, format, details: Option<String> }

// Shipped implementation: dedicated CodecUnavailable variant
CodecUnavailable {
    codec: String,
    format: ArchiveFormat,
    install_instructions: String,
}
// Unsupported variant retained for non-codec cases, still carries details:
// Unsupported { operation: String, format: ArchiveFormat, details: Option<String> }
```

**EntryType**:
```rust
// Before
pub enum EntryType {
    File,
    Directory,
    SymbolicLink { target: String },
}

// After (original clarification decision)
pub enum EntryType {
    File,
    Directory,
    // Note: Symbolic links skipped (FR-022)
}
// **Superseded:** Current enum: File, Directory, Symlink, HardLink, Other
// Symlinks/hardlinks are still skipped during extraction (FR-022) but represented in metadata.
```

**ExtractionOptions.overwrite** (clarified):
- Default: `false` (fail on collision)
- Behavior: Extraction aborts with `ArchiveError::Io` with `AlreadyExists` kind if files exist (per AD 0017)

### Edge Cases Resolved

3 of the original 10 edge cases were resolved by this clarification session (the spec has since grown to 14 edge cases, all marked resolved or deferred at the implementation level; documentation across contract files may still contain inconsistencies on cancellation, memory bounds, and comments that require separate reconciliation):
- ✅ Symbolic links and hard links → Skip with warnings
- ✅ Overwrite behavior → Error by default, configurable
- ✅ Missing codecs → Clear error with installation instructions

---

## Constitutional Compliance

All clarification decisions align with project constitution:

| Principle | Compliance | Evidence |
|-----------|------------|----------|
| I. Robustness & Stability | ✅ | Error-first defaults (FR-023), explicit skip behavior (FR-022) |
| II. Performance First | ✅ | No performance impact from these decisions |
| III. Minimal Dependencies | ✅ | No codec bundling (FR-024), system-provided codecs |
| IV. Comprehensive Testing | ⚠️ | Decisions create testable behaviors (error conditions, skip patterns); testability is confirmed but full verification coverage has not yet been independently assessed |
| V. Clear Contracts | ✅ | Actionable error messages (FR-024), explicit defaults documented |

---

## Next Steps

**Historical note:** Implementation is complete. This clarification record is preserved as design history.
