# AD: Raw format test verification and entry name normalization

## Context and Problem Statement
Found in Review 0003 (Issue R0003-007, Severity: Medium).
Location: `tests/format_compatibility_test.rs:332-445`, `src/ffi/libarchive_wrapper.rs`

Ten standalone raw-format compatibility tests (GZIP, BZIP2, XZ) were
`#[ignore]`-gated with the message "format_raw binding exists; kept ignored
pending fixture and end-to-end verification." The underlying feature
(OI-026-003) was resolved and fixtures existed, but the tests were never
un-gated. When un-ignored, 3 extraction tests failed: the entry name
returned by `list_files()` ("test") did not match the name seen during
`extract_to_memory()` ("data") because the raw-format name normalization
was only applied in the listing path.

## Decision Drivers
* Standalone raw-format support was advertised as delivered but unverified
* Fixtures (`test.gz`, `test.bz2`, `test.xz`) already existed in the repo
* The entry name "data" → filename-stem normalization was inconsistent across
  libarchive code paths

## Considered Options
1. Un-ignore the tests and fix the normalization bug
2. Keep tests ignored and narrow the support claims in docs

## Decision Outcome
ACCEPT (Option 1): Un-ignore all 10 tests and fix the normalization bug.

Status: Implemented

### Implementation
- `tests/format_compatibility_test.rs`: Removed all 10 `#[ignore]` attributes
  and updated section comments
- `src/ffi/libarchive_wrapper.rs`: Extracted `normalize_raw_entry_name()`
  helper that replaces "data" with the archive filename stem (sans compression
  extension). Applied at four call sites:
  1. `list_files_internal()` — entry listing
  2. `extract_all_with_options()` — bulk extraction path renaming
  3. `extract_to_memory()` — single-file extraction path matching
  4. `LibarchiveStreamReader::open()` — streaming extraction path matching
- 17 doc/spec files updated to note "R0003-007 verified" for raw format support

### Bug Details
The raw format handler in libarchive returns "data" as the entry name for all
standalone compressed files. `list_files()` normalized this to the filename
stem (e.g., "test" for "test.bz2"), but `extract_to_memory()` and
`extract_to_stream()` compared the raw "data" name against the caller's
normalized path — always mismatching for BZIP2 and XZ. GZIP worked by
coincidence because libarchive returned the correct name for `.gz` files.

## Consequences
* Good, because all 10 raw-format tests now pass and run in CI
* Good, because the entry name normalization is consistent across all code paths
* Good, because the raw-format support story is fully verified end-to-end
* Bad, because the bug existed since OI-026-003 was resolved (2026-04-14) —
  any caller using `extract_to_memory()` on standalone `.bz2`/`.xz` files
  would have received a "not found" error
