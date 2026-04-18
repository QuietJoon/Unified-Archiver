# AD: Eliminate redundant middle open in modify()

## Context and Problem Statement
Found in Review 0002 (Issue R0002-0009, Severity: Medium).
Location: `src/modification.rs:108`

`Archive::modify()` opened the archive three times:
1. `ArchiveFormat::detect()` — format detection
2. `Archive::open()` — encryption preflight check (used native backends: Piz/SevenZ)
3. `LibarchiveArchive::open()` — actual modify handle (libarchive only)

The middle open (#2) used a different backend than the modify handle (#3),
doubling I/O and creating backend divergence before any rewrite begins.

## Decision Drivers
* The encryption check only needs `list_files_metadata_only()` to scan `is_encrypted` flags
* The libarchive backend (opened in step #3) can perform the same check
* Eliminating the middle open removes one full archive parse and reduces I/O

## Considered Options
1. Check encryption via the libarchive handle that will be used for modification
2. Keep the separate `Archive::open()` for encryption check (separation of concerns)

## Decision Outcome
ACCEPT (Option 1): Check encryption on the libarchive handle directly.

Status: Implemented

### Implementation
- `src/modification.rs`: Removed `Archive::open()` encryption preflight block.
  After opening `LibarchiveArchive`, call `list_files_metadata_only()` on the
  same handle and check `entries.iter().any(|e| e.is_encrypted)`.
- The advisory file lock acquisition was moved before the libarchive open
  (previously it was between the two opens).

## Consequences
* Good, because modify() now opens the archive twice instead of three times
* Good, because the encryption check uses the same backend as the modification
* Good, because format detection (#1) + libarchive open (#2) is the minimum required
