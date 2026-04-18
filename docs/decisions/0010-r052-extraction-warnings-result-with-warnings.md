# AD: Wire ResultWithWarnings into extract_all (R052-006/R052-007)

## Context and Problem Statement
Found in Review 052 (Issues R052-006 and R052-007, Severity: High).
Location: `src/extraction.rs` — `Archive::extract_all()`

Per FR-022, symlinks and hardlinks are skipped during extraction for security. Previously, `extract_all()` returned `Result<()>` — callers had no way to know which entries were skipped. The existing `ResultWithWarnings<T>` type and `ArchiveWarning::SkippedSymlink`/`SkippedHardLink` variants existed but were not wired into the extraction return path.

## Decision Drivers
* Callers need to know what was skipped — silent data loss is unacceptable
* `ResultWithWarnings<T>` already exists in the crate's public API
* Backward compatibility: existing callers using `.unwrap()`, `.expect()`, `?`, or `.is_ok()` must continue to work

## Considered Options
1. Return `Result<ResultWithWarnings<()>>` from `extract_all()` — live warning surface
2. Side-channel warnings via a separate `get_warnings()` method (rejected — stateful, easy to forget)
3. Log warnings only (rejected — no programmatic access)

## Decision Outcome
ACCEPT (Option 1): `extract_all()` now returns `Result<ResultWithWarnings<()>>`.

All five backends (`libarchive`, `piz`, `zip`, `sevenz`, `unrar`) collect `ArchiveWarning` entries at their symlink/hardlink skip points and return `Result<Vec<ArchiveWarning>>`. The `extraction.rs` dispatch layer wraps the vector into `ResultWithWarnings`.

Status: Implemented

### Implementation
- `src/ffi/libarchive_wrapper.rs`: `extract_all_with_options` returns `Result<Vec<ArchiveWarning>>`
- `src/ffi/piz_wrapper.rs`: Same pattern — warning at `piz_entry_is_symlink` skip
- `src/ffi/zip_wrapper.rs`: Same pattern — warning at `is_symlink()` skip
- `src/ffi/sevenz_wrapper.rs`: Same pattern — warning captured in `for_each_entries` closure
- `src/ffi/wrapper.rs`: Same pattern — separate `SkippedSymlink`/`SkippedHardLink` based on `EntryType`
- `src/extraction.rs`: `extract_all()` return type changed; match block uses `?` and wraps in `ResultWithWarnings`
- `src/error.rs`: Added `#[derive(Debug)]` to `ResultWithWarnings<T>` for test compatibility
- `src/lib.rs`: Added `ArchiveWarning` to public re-exports
- `tests/integration/hardlink_skip.rs`: Updated to verify warnings are returned

### Backward Compatibility
- `Result<ResultWithWarnings<()>>` is compatible with all existing usage patterns:
  - `.unwrap()` / `.expect()` → yields `ResultWithWarnings<()>`, discarded by `;`
  - `?` in `fn -> Result<()>` → `?` unwraps to `ResultWithWarnings<()>`, discarded by `;`
  - `.is_ok()` → works on both `Result<()>` and `Result<ResultWithWarnings<()>>`
- One test (`large_zip_mmap_test.rs`) needed `Ok(())` → `Ok(_)` pattern update

## Consequences
* Good, because callers can now discover which entries were skipped during extraction
* Good, because the warning surface is the return value — no side-channel or state management
* Good, because backward compatibility is preserved for existing callers
* Bad, because callers who want to inspect warnings must destructure `ResultWithWarnings` (but can ignore it)
