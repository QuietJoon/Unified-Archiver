---
type: ADR
title: "AD: Restore `Result<ResultWithWarnings<()>>` for `extract_all` per AD 0010"
description: "Implemented."
tags: [decision, ADR-0033, ADR-0010, R0059-0006, R0059-0007]
timestamp: 2026-04-17T00:00:00Z
status: active
---

# AD: Restore `Result<ResultWithWarnings<()>>` for `extract_all` per AD 0010

## Context and Problem Statement

Found in Review 0059 (Issues R0059-0006 and R0059-0007, Severity: High).
Location: `src/extraction.rs::Archive::extract_all`, `src/ffi/{libarchive_wrapper,piz_wrapper,sevenz_wrapper,zip_wrapper,wrapper}.rs`.

AD 0010 specified a `Result<ResultWithWarnings<()>>` return type for extraction
so that callers can observe non-fatal warnings — notably skipped symlinks and
hardlinks — without losing the fact that the bulk extraction succeeded. At the
time of review 0059 the public and backend signatures had silently collapsed
back to `Result<()>`, dropping the warnings channel entirely.

## Decision Drivers

* AD 0010 is a ratified architectural decision; the public surface drifted from it.
* Security/audit workflows need to observe symlink/hardlink skips (they are a
  deliberate safety gate, not a bug).
* Downstream crates depending on `ResultWithWarnings` were already importing the
  type expecting the AD 0010 shape.

## Considered Options

1. Leave `Result<()>` and amend AD 0010 to drop the warnings channel.
2. Restore `Result<ResultWithWarnings<()>>` end-to-end across all 5 backends and
   the 4 public dispatch methods (`extract_all`, `extract_filtered`,
   `extract_files`, `extract_by_ids`), plumbing `SkippedSymlink { path, target }`
   and `SkippedHardLink { path }` warnings from each backend's skip site.

## Decision Outcome

ACCEPT: We picked option 2. Reverting the AD would silently erode a safety
contract that external code already relies on; threading the warnings through
is a mechanical fix whose cost is bounded (five backends, one new FFI binding
for `archive_entry_symlink`, per-call `Vec<ArchiveWarning>` accumulation).

Status: Implemented.

### Implementation

- `src/ffi/libarchive.rs`: added `archive_entry_symlink` FFI binding.
- `src/ffi/libarchive_wrapper.rs`: `extract_all[_with_options]` now returns
  `Result<Vec<ArchiveWarning>>`, reading pathname + symlink target at the skip
  site and emitting `SkippedSymlink { path, target }` or `SkippedHardLink { path }`.
- `src/ffi/{piz,sevenz,zip}_wrapper.rs`: same shape; target is `None` where the
  backend does not expose the link target without extra I/O.
- `src/ffi/wrapper.rs` (UnRAR): split the combined symlink/hardlink skip branch
  into two, each emitting its own warning variant before `RAR_SKIP`.
- `src/extraction.rs`: `extract_all`, `extract_filtered`, `extract_files`,
  `extract_by_ids` all return `Result<ResultWithWarnings<()>>`; backend calls
  thread through `?` and accumulate into the ResultWithWarnings. The
  `ZipWriter` arm returns the appropriate `WriteModeOnly` error (unchanged
  behavior, just reshaped for the new signature).
- `src/error.rs`: `ResultWithWarnings<T>` now derives `Debug, Clone` (required
  by test `assert_eq!` / `expect()` / `format!("{:?}")` callers).
- `tests/large_zip_mmap_test.rs`: pattern match updated from `Ok(())` to `Ok(_)`.

## Consequences

* Good, because the AD 0010 contract is restored without churn in caller code
  (callers using `.value` / `.warnings` continue to work).
* Good, because symlink/hardlink skip reasons are now observable at every
  backend — previously only libarchive emitted them.
* Neutral, because `target: Option<String>` is `None` for piz/sevenz/zip where
  reading the target would require re-opening the entry; libarchive and UnRAR
  populate it when available.
* Bad, because the call sites in every backend now allocate a `Vec` even when
  no warnings are emitted. The vector is small and the extraction path is
  already O(N) in entries, so this is a rounding error.
