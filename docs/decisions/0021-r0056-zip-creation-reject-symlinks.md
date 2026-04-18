# AD: Reject symlinks in ZIP recursive creation

## Context and Problem Statement
Found in Review 0056 (Issue R0056-0027, Severity: High).
Location: `src/ffi/zip_writer.rs:291`

`add_directory_recursive()` only handled files and directories in the
`walkdir` loop. Symlink entries fell through silently — the caller had no
indication that filesystem objects were being dropped from the archive.

## Decision Drivers
* FR-022 establishes link rejection as a security invariant across the codebase
* Silent data loss is worse than an explicit error
* ZIP format does not natively support symlinks

## Considered Options
1. Return an error when a symlink is encountered (consistent with FR-022)
2. Follow symlinks and archive the target content
3. Skip symlinks with a warning (like extraction does)

## Decision Outcome
ACCEPT (Option 1): Emit an `ArchiveError` that names the symlink path and
explains that ZIP format does not support symlinks. This is consistent with
FR-022's reject-links-by-default policy and gives callers an immediate, actionable
signal. Option 3 (skip with warning) could be added later behind an option flag.

Status: Implemented

### Implementation
- Added symlink check before file/directory handling in `add_directory_recursive()`
- Error message includes the symlink path for debuggability

## Consequences
* Good, because callers no longer silently lose filesystem entries
* Good, because the behavior is consistent with extraction-side FR-022 policy
* Bad, because callers with trees containing symlinks must now handle or filter them
