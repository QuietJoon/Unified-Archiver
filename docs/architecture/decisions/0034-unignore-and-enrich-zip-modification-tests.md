# AD: Unignore and enrich ZIP modification tests

## Context and Problem Statement

Found in Review 0059 (Issue R0059-0036, Severity: Medium).
Location: `tests/modification_test.rs`, `tests/integration/modification.rs`,
`src/modification.rs` (inline unit tests).

Several ZIP `commit_changes` tests carried
`#[ignore = "ZIP modification via libarchive has known issues..."]`. The ignore
markers predated the OI-025-001/OI-025-002 fix (2026-04-14), which introduced
metadata-aware add helpers on both the `ZipWriter` and `LibarchiveArchive`
backends. The ignore flag was never retracted, so the regression-protection
value of the tests had been silently lost.

The user's direction on this review item was specifically: "Remove ignore flag
and re-implement/verify the tests as workable and enrich!"

## Decision Drivers

* The root-cause for the ignore flag (ZIP modification unreliability) was
  resolved by OI-025-001/OI-025-002.
* Without enabling and exercising these tests, the `commit_changes` pipeline
  for ZIP has no regression protection.
* The user's explicit instruction to enrich, not just unignore.

## Considered Options

1. Leave the tests ignored and add a TODO in a tracking issue.
2. Remove the ignore attribute only (minimal change).
3. Remove the ignore attribute *and* enrich the integration-level coverage
   with new tests exercising byte-exact preservation, payload size transitions,
   ordering, and the independence of add/remove queues.

## Decision Outcome

ACCEPT option 3. The existing `test_add_files_to_archive`,
`test_remove_files_from_archive`, `test_replace_files_in_archive`, and
`test_combined_operations` only assert entry counts and content equality on a
small set of files. Six new tests probe behaviors that were previously
untested and surfaced one API observation worth documenting (see below).

Status: Implemented.

### Implementation

- `tests/modification_test.rs`: 4 ignore attributes removed via `replace_all`.
- `tests/integration/modification.rs`: 2 ignore attributes removed; added
  `std::fs::remove_file(&test_path).ok();` before every `Archive::create` to
  clean stale fixtures from prior failed runs; six enriched tests appended:
  - `test_modify_preserves_unchanged_content_bytes` — verifies byte-exact
    preservation (4096 bytes of 0xAB) after removing a *different* entry.
  - `test_modify_empty_archive_add_then_commit` — seed + 3 adds = 4 entries.
  - `test_modify_replace_grows_and_shrinks_payload` — 100 bytes → 8000 bytes
    → 4 bytes across two commit cycles.
  - `test_modify_remove_nonexistent_entry_errors_on_commit` — asserts the
    nonexistent-remove is surfaced and the original archive survives intact.
  - `test_modify_add_and_remove_queues_are_independent` — documents the
    observed add+remove semantics: `remove_entry` targets only source-archive
    entries, so a same-name add+remove pair results in the *added* entry
    present after commit (2 entries total, not 1).
  - `test_modify_many_entries_preserves_ordering_and_content` — 25 entries
    with remove/add/replace + byte-exact verification of 23 untouched entries.
- `src/modification.rs`: 2 ignore attributes removed
  (`test_commit_changes_add_entry_roundtrip`,
  `test_commit_changes_remove_entry_roundtrip`).

Test result: 11 passed; 0 failed; 0 ignored.

## Consequences

* Good, because the `commit_changes` ZIP path now has regression protection
  across payload-size transitions and bulk modifications.
* Good, because the API contract around add+remove queue independence is now
  encoded in an executable test rather than implicit folklore.
* Neutral, because one test
  (`test_modify_add_and_remove_queues_are_independent`) documents observed
  behavior rather than asserts a design invariant. If the semantics are
  intentionally changed in the future, that test will fail loudly and force an
  explicit decision.
