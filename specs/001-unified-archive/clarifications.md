# Clarifications Log: unified-archive

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Purpose**: Document ambiguity resolutions and design decisions

## Overview

This document records clarification questions asked during the specification review phase and the decisions made to resolve ambiguities. All decisions have been encoded back into the specification and related design documents.

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
- `EntryType` enum simplified (removed `SymbolicLink` variant)
- Added FR-022: Library MUST skip symbolic links and hard links with warnings
- Updated `ArchiveEntry` invariants to document skip behavior
- Edge case marked as [RESOLVED]

**Files Updated**:
- `spec.md` - Added FR-022
- `data-model.md` - Updated `EntryType`, removed symlink handling
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

**Decision**: Option 1 - Return `ArchiveError::Unsupported` with codec name and installation instructions

**Rationale**:
- Actionable error messages (constitution Principle V: Clear Contracts)
- User knows exactly what's missing and how to fix it
- Prevents silent failures or corrupted extraction attempts
- Maintains minimal dependencies (constitution Principle III)
- Aligns with explicit error handling requirement (FR-010)

**Implementation Impact**:
- Added FR-024: Library MUST return `ArchiveError::Unsupported` with codec name and instructions
- Enhanced `ArchiveError::Unsupported` variant with `details: Option<String>` field
- Updated Display implementation to show codec details
- Updated extraction contract error conditions
- Edge case marked as [RESOLVED]

**Files Updated**:
- `spec.md` - Added FR-024
- `contracts/errors.md` - Enhanced `Unsupported` variant, updated Display impl
- `contracts/extraction.md` - Added codec error to error conditions

---

## Summary of Changes

### New Functional Requirements

- **FR-022**: Skip symbolic links and hard links with warnings
- **FR-023**: Fail extraction on file collision unless overwrite=true
- **FR-024**: Return actionable error for missing codecs

### Updated Entities

**ArchiveError::Unsupported**:
```rust
// Before
Unsupported {
    operation: String,
    format: ArchiveFormat,
}

// After
Unsupported {
    operation: String,
    format: ArchiveFormat,
    details: Option<String>, // Codec name, installation instructions
}
```

**EntryType**:
```rust
// Before
pub enum EntryType {
    File,
    Directory,
    SymbolicLink { target: String },
}

// After
pub enum EntryType {
    File,
    Directory,
    // Note: Symbolic links skipped (FR-022)
}
```

**ExtractionOptions.overwrite** (clarified):
- Default: `false` (fail on collision)
- Behavior: Extraction aborts with `ArchiveError::Io` if files exist

### Edge Cases Resolved

3 out of 10 edge cases resolved:
- ✅ Symbolic links and hard links → Skip with warnings
- ✅ Overwrite behavior → Error by default, configurable
- ✅ Missing codecs → Clear error with installation instructions

7 edge cases remain for future clarification or implementation decisions.

---

## Constitutional Compliance

All clarification decisions align with project constitution:

| Principle | Compliance | Evidence |
|-----------|------------|----------|
| I. Robustness & Stability | ✅ | Error-first defaults (FR-023), explicit skip behavior (FR-022) |
| II. Performance First | ✅ | No performance impact from these decisions |
| III. Minimal Dependencies | ✅ | No codec bundling (FR-024), system-provided codecs |
| IV. Comprehensive Testing | ✅ | Decisions create testable behaviors (error conditions, skip patterns) |
| V. Clear Contracts | ✅ | Actionable error messages (FR-024), explicit defaults documented |

---

## Next Steps

With these clarifications resolved, the specification is ready for implementation:

1. ✅ Critical edge cases resolved
2. ✅ All decisions encoded in specification
3. ✅ API contracts updated
4. ✅ Data model aligned with decisions
5. **Ready for**: `/speckit.implement` to begin Phase 1 (Setup)

**Remaining Edge Cases** (can be addressed during implementation):
- Unicode/special character handling in filenames
- Insufficient disk space handling
- Concurrent archive access patterns
- Cross-platform permission preservation
- Password error handling specifics
- Non-standard extension handling

These remaining edge cases are either already covered by existing FRs or can be handled with standard error patterns during implementation.
