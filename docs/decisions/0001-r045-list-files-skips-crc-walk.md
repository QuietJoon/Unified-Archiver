# AD: `Archive::list_files()` skips the libarchive CRC walk

## Context and Problem Statement

Found in Review 045 (Issue R045-003, Severity: Medium).
Location: `src/inspection.rs` (route to libarchive backend) /
`src/ffi/libarchive_wrapper.rs` (`list_files`).

`LibarchiveArchive::list_files()` populates `entry.crc32` by walking
`archive_read_data_block()` on every file entry. That decompresses the entire
archive payload just to enumerate names and sizes, making `Archive::list_files()`
O(total uncompressed size) for libarchive-backed formats (TAR, TAR.GZ, TAR.BZ2,
TAR.XZ, ISO). Most callers of `list_files()` only need names/sizes/types — full
decompression for CRC is wasted I/O and CPU.

The wrapper already exposes `LibarchiveArchive::list_files_metadata_only()`,
which skips the CRC walk and is already used by `list_files_for_limits()`.

## Decision Drivers

* Listing should be cheap; integrity verification has its own dedicated entry
  point (`Archive::validate_integrity()` / backend `test_integrity()`).
* The public contract for `ArchiveEntry.crc32` already documents it as
  best-effort and may be `None` for some backends, so dropping libarchive's
  populated CRC stays within contract.
* The faster code path already exists inside the wrapper.

## Considered Options

1. Route `Archive::list_files()` through `list_files_metadata_only()` for the
   libarchive backend; leave `LibarchiveArchive::list_files()` available for
   any internal caller that explicitly wants CRCs.
2. Delete the CRC walk from `LibarchiveArchive::list_files()` entirely.
3. Status quo (continue paying the cost on every public listing).

## Decision Outcome

**ACCEPT** — Option 1.

The minimal change keeps the wrapper's documented contract intact while making
the public surface fast. Switching the route is a one-line change in
`src/inspection.rs::list_files` and avoids touching the wrapper's public API.

Status: Implemented (2026-04-14).

### Implementation

`src/inspection.rs` — `Archive::list_files()` libarchive arm now calls
`libarchive.list_files_metadata_only()` instead of `libarchive.list_files()`.

Verified by full `cargo test` suite (all suites green, including
`tests/inspection_test.rs` and `tests/integrity_comprehensive_test.rs`).

## Consequences

* Good — `Archive::list_files()` no longer decompresses payloads on libarchive
  backends; listing cost is now O(headers) instead of O(total bytes).
* Good — Removes the double-decompression on the
  `list_files()` + `validate_integrity()` path (called out as a perf opportunity
  by Review 002 as well).
* Bad — `ArchiveEntry.crc32` is now consistently `None` on libarchive backends
  for the public listing path; callers that need a CRC must use
  `validate_integrity()` (which already verifies via `test_integrity()`) or a
  dedicated archive-level integrity API
  (`calculate_archive_crc` / `calculate_manifest_digest`).
