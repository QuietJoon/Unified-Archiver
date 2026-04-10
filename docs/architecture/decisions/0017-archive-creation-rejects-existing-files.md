# AD: Archive creation rejects existing files

## Context and Problem Statement
Found in Review 026 (Issue R026-001, Severity: HIGH).
Location: `src/creation.rs`

`Archive::create()` would silently overwrite existing files because both the ZipWriter and libarchive backends truncate on open. This could lead to accidental data loss.

## Decision Drivers
* Data safety: accidental overwrites are a common source of user data loss
* Principle of least surprise: creation should not destroy existing data
* An explicit overwrite flag can be added later if needed

## Considered Options
1. Reject existing files with an `AlreadyExists` error (fail-safe)
2. Add an `overwrite: bool` parameter to `create()`
3. Keep current behavior (implicit overwrite)

## Decision Outcome
ACCEPT: Option 1 — `create()` now checks `path.exists()` before routing to any backend and returns an `AlreadyExists` I/O error. An optional overwrite flag may be added later (tracked as OI-026-001).

Status: Implemented

### Implementation
- `src/creation.rs`: Added `path_buf.exists()` check at the top of `create()`, before backend routing
- Returns `ArchiveError::io("create", &path_buf, IoError::new(AlreadyExists, ...))`

## Consequences
* Good, because existing files are never silently overwritten
* Good, because the check is centralized (covers both ZipWriter and libarchive backends)
* Bad, because users who want to overwrite must delete the file first (minor friction)
