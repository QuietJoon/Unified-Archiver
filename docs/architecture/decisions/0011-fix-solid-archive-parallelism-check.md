# AD: Fix solid-archive parallelism check to use password-aware handle

## Context and Problem Statement
Found in Review 025 (Issues R025-006, R025-007, R025-008, Severity: HIGH).
Location: `src/extraction.rs` — `extract_filtered()`, `extract_files()`, `extract_by_ids()`

Three extraction methods called `self.is_solid()` to decide whether parallel extraction was safe. However, `self` uses the original archive handle which may lack a password. When an encrypted solid archive is opened with `open_encrypted()`, the password-aware `archive` handle (created locally in each method) was ignored for the solidity check, causing `is_solid()` to fail silently and default to allowing parallel extraction on solid archives.

## Decision Drivers
* Correctness: solid archives require sequential decompression
* The password-aware `archive` handle already exists in each method
* Silent failure (defaulting to parallel) could produce corrupted output

## Considered Options
1. Change `self.is_solid()` to `archive.is_solid()` in all three methods
2. Cache solidity at open time — rejected because encrypted archives can't be probed without a password

## Decision Outcome
ACCEPT: Changed all three call sites to use the local `archive` handle.

Status: Implemented

### Implementation
- `extract_filtered()`: `self.is_solid()` -> `archive.is_solid()`
- `extract_files()`: same
- `extract_by_ids()`: same

## Consequences
* Good, because encrypted solid archives are now correctly detected and extracted sequentially
* Good, because the fix is minimal (3 line changes) with no API impact
