# AD 0037: Test-suite consolidation — integration suite is canonical

## Context and Problem Statement

Review 0061 raised 11 findings (R0061-0013 through R0061-0023) identifying duplicated tests between three per-format test files (`tests/modification_test.rs`, `tests/extraction_test.rs`, `tests/zip_7z_test.rs`) and the cross-format integration suite (`tests/integration/modification.rs`, `tests/integration/extraction.rs`).

The duplicates had drifted over time: in several cases only the integration version asserted a specific invariant, and the per-format version had gone stale. Keeping both copies in sync on every change was uncompensated maintenance work.

## Decision Drivers

* Maintenance cost of keeping two near-identical assertions synchronised across PRs.
* Risk of silent drift: a fix landing only in the per-format version (or vice versa) leaves the other copy misleading.
* The integration suite already exercises the public cross-format API uniformly — format-specific tests should only add value when they cover a behaviour the cross-format loop cannot.

## Considered Options

1. **Delete integration copies, keep per-format files.** Rejected: the integration suite is where regressions are most likely to be caught (tests run across every supported format), so the per-format files are the ones that add marginal value.
2. **Keep both copies and enforce sync via a checklist.** Rejected: process-based gates routinely fail during refactors; this is what got us here in the first place.
3. **Promote the integration suite to canonical; per-format files keep only format-specific cases.** Accepted.

## Decision Outcome

ACCEPT option 3.

Rationale:

* The integration suite asserts the public API contract across all formats — that is the contract we actually promise consumers.
* Per-format files retain genuine value *only* where they exercise a branch the cross-format loop cannot reach (e.g. ZIP-specific `zip` crate internals like archive comments and per-entry compression methods; 7z-specific listing; RAR-specific extract-to-memory failure path).
* Every deletion in this sweep was verified: the deleted assertion either already exists in the integration version, or was folded in before deletion.

## Implementation

Deleted from `tests/modification_test.rs`:
- `test_add_files_to_archive` (R0061-0013)
- `test_remove_files_from_archive` (R0061-0014)
- `test_replace_files_in_archive` (R0061-0015)
- `test_combined_operations` (R0061-0016)
- `test_open_nonexistent_archive` (R0061-0017)

`tests/modification_test.rs` now contains only ZIP-specific metadata preservation tests (archive comment, per-entry compression methods) that require the `zip` crate directly.

Deleted from `tests/extraction_test.rs`:
- `test_extract_all_rar5` (R0061-0018) — covered by `tests/integration/extraction.rs::test_extract_all_rar5`
- `test_extract_to_memory` positive path (R0061-0019) — covered by `test_extract_to_memory_multiple_formats`

The negative paths (`test_extract_to_memory_nonexistent`, `test_extract_nonexistent_file`), filter-based tests, and RAR-specific single-file extraction remain in the per-format file.

Deleted from `tests/zip_7z_test.rs`:
- `test_zip_format_detection` (R0061-0020) — integration tests assert `archive.format()` directly
- `test_7z_format_detection` (R0061-0021) — merged into `test_7z_list_files` so the dedicated 7z check still exists
- `test_zip_list_files` (R0061-0022) — covered by cross-format list loops
- `test_zip_entry_count` (R0061-0023) — covered by `tests/load_test.rs`

The remaining `test_7z_list_files` retains an explicit `archive.format()` assertion so 7z listing regressions do not hide behind the cross-format loops.

## Consequences

* `cargo test --all-features` continues to pass with the same cross-format coverage.
* Per-format test files shrink significantly and their docstrings now explicitly name which format-specific branch they cover.
* Future reviewers should only add to a per-format file when the case cannot be expressed inside the cross-format loop.

## Revisit trigger

Re-open this decision if:

* The integration suite grows large enough to be slow, justifying splitting some cross-format coverage back into per-format files that run in parallel.
* A format is added that cannot be exercised via the current integration-suite pattern (e.g. an exotic archive type requiring special fixture generation).
