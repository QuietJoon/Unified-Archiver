# AD 0029: Extraction API redesign — `extract_some()` as core selective primitive

## Context and Problem Statement
Found in Review 050 (Issues R050-002, R050-007–009, R050-014, Severity: High/Medium),
resolved in Review 0004 (Issues R0004-001, R0004-002, R0004-003).

Multiple extraction API issues converged on a single architectural root cause:
1. `ExtractionOptions.filter` was dead public API (R050-002) — never read by any extraction path.
2. Three selective extraction methods (`extract_filtered`, `extract_files`, `extract_by_ids`) each reopened the archive per entry via `extract_single_entry()` — O(N) format detections for N files.
3. Link entries caused hard errors in selective extraction, violating FR-022 (which requires `ArchiveWarning`s matching `extract_all()` behavior).

## Decision Drivers

* Per-entry reopen is the dominant performance cost for selective extraction
* FR-022 compliance: link entries must produce warnings, not errors, in all bulk extraction paths
* Existing methods (`extract_filtered`, `extract_files`, `extract_by_ids`) are established API — deprecation would break callers

## Considered Options

1. Keep the current API, fix the per-entry reopen only for the sequential path (quick win).
2. Redesign around `extract_all` / `extract_some` / `extract_one`, deprecating the three existing methods.
3. Add `extract_some()` as core primitive; rewrite existing methods as thin delegation wrappers (no deprecation).

## Decision Outcome

ACCEPT option 3. The implementation provides:
- **`extract_some(predicate, options)`** — new core primitive; single-pass through the archive with a shared handle, warning-aware (FR-022). Takes `Fn(&ArchiveEntry) -> bool` predicate.
- **`extract_filtered()`**, **`extract_files()`**, **`extract_by_ids()`** — retained as thin wrappers delegating to `extract_some()`. Return type changed from `Result<()>` to `Result<ResultWithWarnings<()>>`.
- **`extract_single_entry()`** — deleted (O(N) reopen function no longer needed).

Each backend gained an `extract_core()` private method with `selection: Option<&HashSet<String>>` parameter, shared by both `extract_all_with_options(None)` and `extract_selected_with_options(Some(sel))`. This avoids code duplication — the extraction loop exists in one place per backend.

Status: Implemented.

### Implementation

Backend changes (Phase 1):
- All 5 backends (Libarchive, Piz, ZipReader, SevenZ, UnRAR) refactored to `extract_core()` pattern
- Selection filter applied after path read, before link checks
- Non-selected entries skipped efficiently (backend-specific: `archive_read_data_skip`, `RAR_SKIP`, `continue`, `return Ok(true)`)

Public API changes (Phase 2):
- `extract_some()` added to `src/extraction.rs`
- Three wrappers rewritten as delegation (validation + `self.extract_some(...)`)
- `extract_files()` preserves path-existence validation before delegation
- `extract_by_ids()` preserves ID-range validation before delegation

## Consequences

- Good: single-pass extraction eliminates O(N) archive reopens
- Good: FR-022 compliance — all selective extraction methods now return link warnings
- Good: no breaking API changes — existing methods retained with compatible signatures
- Good: warning-aware return types enable callers to handle link entries gracefully
- Neutral: return type widened from `Result<()>` to `Result<ResultWithWarnings<()>>`; callers using `.unwrap()` or `?` are unaffected

## References

- R050-002, R050-007, R050-008, R050-009, R050-014
- R0004-001, R0004-002, R0004-003 (FR-022 violations fixed)
- OI-050-014 (resolved by this implementation)
