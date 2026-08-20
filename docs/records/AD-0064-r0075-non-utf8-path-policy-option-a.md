---
type: ADR
title: "AD 0064: Non-UTF-8 path policy — preserve raw bytes (Option A)"
description: "Option A (preserve raw bytes) remains the policy; the closure claim is narrowed to its shipped scope (amended 2026-08-05, §B): listing raw_path plus PathBuf backend path fields landed, add_file_from_path rejects non-UTF-8 instead of preserving it, detect_multipart is still lossy, and 12 write/extract-path sites plus the missing by-id read surface stay open as OI-0076-001."
tags: [decision, ADR-0064, R0075-0007, R0075-0023, R0075-0055, R0075-0082, R0070-0025, OI-0075-001, OI-0065-001]
timestamp: 2026-04-29T00:00:00Z
status: active
---

# AD 0064: Non-UTF-8 path policy — preserve raw bytes (Option A)

## Context and Problem Statement

Review 0075 raised four findings (R0075-0007, R0075-0023, R0075-0055, R0075-0082)
about path-handling lossy conversions:

- `LibarchiveArchive::path: String` (lossy at construction)
- `UnrarArchive::path: String` (lossy at construction)
- `add_file_from_path` derives the archive name via `to_string_lossy()`
- `detect_multipart` lossy-converts file names + stems

OI-0075-001 routed the bundle as a single design question. User Phase-2
routing (2026-04-29) picked Option A: preserve raw bytes via `raw_path`-style
fields throughout; keep `to_string_lossy` only for display.

This ADR locks the policy. Implementation lands across every backend in
the deferred-OI-closure plan Phase 1.

## Decision Drivers

- **Symmetry.** Listing-side already preserves `raw_path` (per R0070-0025).
  Archive-level metadata being lossy while entry-level isn't is a paper cut.
- **Round-trip correctness.** A user with a non-UTF-8 archive on a
  filesystem that allows non-UTF-8 paths can today get a `Archive::path()`
  string that doesn't match the original bytes. Option A closes that.
- **Cost asymmetry.** `PathBuf` is the same size as `String` on every
  platform unified-archive supports; the runtime cost is zero.
- **Windows wide-path support.** Libarchive ships
  `archive_entry_copy_pathname_w`, so the destination side gets the
  full-fidelity path even on Windows.

## Considered Options

1. **Option A — preserve raw bytes everywhere.** Backend `path` fields
   become `PathBuf`; archive-name derivation preserves `OsStr` bytes;
   detect_multipart uses raw byte comparisons; libarchive uses the wide
   variant on Windows.
2. **Option B — reject non-UTF-8 input loudly.** Add `InvalidPath` error;
   facade refuses to open / create / modify a non-UTF-8 path.

## Decision Outcome

ACCEPT Option A. User Phase-2 routing picked it explicitly:
"preserve raw_path. but keep as open issue."

Status: Implementation in deferred-OI-closure plan Phase 1.

### Implementation

`src/ffi/libarchive_wrapper.rs`:
- `LibarchiveArchive::path: String` → `PathBuf`.
- `archive_entry_set_pathname` call site uses a `path_to_cstring`
  helper (Unix bytes via `OsStrExt`) on Unix; on Windows uses
  `archive_entry_copy_pathname_w` with the wide-char form when the
  binding is available, else falls back to lossy.

`src/ffi/wrapper.rs` (UnRAR):
- `UnrarArchive::path: String` → `PathBuf`.
- All `path.clone()` callers updated.

`src/creation.rs::add_file_from_path`:
- Archive name comes from `path.file_name().ok_or(...)?.as_encoded_bytes()`
  on Unix (`OsStrExt`) or wide-form on Windows.
- `ArchiveEntry::raw_path` populated for non-UTF-8 names.

`src/inspection.rs::detect_multipart`:
- Sibling enumeration uses `OsStr` bytes for matching patterns
  (`.part1.rar`, `.001`); when a sibling matches, the original
  `PathBuf` is kept.

### Tests

- `tests/integration/non_utf8_paths.rs` (new, `#[cfg(unix)]`):
  archive a file whose name is `b"\xff\xfe.txt"`; round-trip through
  every backend; assert `raw_path == Some(b"\xff\xfe.txt".to_vec())`.
- Windows wide-path test deferred to Windows CI lane (OI-0065-001).

## Consequences

- Good: archive-level path metadata round-trips losslessly on every
  Unix-supported filesystem.
- Good: Windows libarchive wide-pathname support lands as a side
  benefit — destination paths with non-BMP code points work.
- Good: closes the asymmetry between entry-level (raw_path) and
  archive-level (path) preservation.
- Bad: PathBuf doesn't `impl Display` — every error-message site that
  prints `archive.path` now uses `.display()`. ~30 call sites; mechanical.
- Bad: `String`-typed FFI return paths (e.g. unrar's archive name)
  still cross a UTF-8 boundary inside the FFI layer; the new `PathBuf`
  is constructed from the same bytes via `OsStr::from_encoded_bytes_unchecked`
  on Unix. Documented at the FFI layer.

## Amendment (2026-08-05, decision-review-2026-07-19 §B — closure claim narrowed to shipped scope)

The Consequences bullet **"Good: closes the asymmetry between entry-level (raw_path) and
archive-level (path) preservation"** over-claims, and two Implementation bullets describe code that
did not ship as written. What did land, verified against the current tree: `LibarchiveArchive::path`
and `UnrarArchive::path` are both `PathBuf` (`src/ffi/libarchive_wrapper.rs`, `src/ffi/wrapper.rs`),
and `tests/integration/non_utf8_paths.rs` exists. Note that the libarchive wrapper has since been
split into `reader.rs` / `writer.rs` children, so this record's `src/ffi/libarchive_wrapper.rs`
citations now resolve to those files for everything except the struct definition itself.

Two divergences from the Implementation section:

* **`src/creation.rs::add_file_from_path` shipped Option B's rejection, not Option A's byte
  preservation.** It does not derive the entry name from `as_encoded_bytes()` and does not populate
  `ArchiveEntry::raw_path`; it calls `file_name_os.to_str()` and, on failure, returns
  `ArchiveError::invalid_path` with the reason "Source filename is not valid UTF-8 — pass an
  explicit name to add_file_from_path_as instead (AD 0064)". `add_file_from_path_as` is the escape
  hatch. OI-0075-001's status line records this outcome ("add_file_from_path rejects non-UTF-8
  loudly; detect_multipart rustdoc-noted").
* **`src/inspection.rs::detect_multipart` was not migrated to `OsStr` bytes.** It still derives both
  `file_name` and `file_stem` via `to_string_lossy()`; the accepted-limitation reasoning lives in a
  code comment citing this record and R0075-0082 (same-substitution prefix matching still works for
  the common multi-volume case), with the stem-coalescing edge case deferred to OI-0075-004
  (R0075-0083) and byte-level matching still open as R0076-0092.

Review 0076 then found **12 further lossy `to_string_lossy` sites in the write and extract paths**,
all tracked OPEN as **OI-0076-001** ("Non-UTF-8 path fidelity round 2 (write + extract paths)"). Spot-verified as still
present today: the libarchive write constructor's `path_str = path.as_ref().to_string_lossy()
.to_string()` in `src/ffi/libarchive_wrapper/writer.rs` (R0076-0021); `raw_format_name` and
`is_compound_tar_extension` in `src/ffi/libarchive_wrapper/reader.rs` (R0076-0041, R0076-0042);
libarchive's staging and final extract-destination strings in the same file (R0076-0039,
R0076-0040); `src/format.rs::extension_in`, `promote_to_compound_tar`, and `format_from_extension`
(R0076-0094); UnRAR's atomic-tempfile path conversion in `src/ffi/wrapper.rs` (R0076-0067); and the
empty-directory root-name derivation in `src/ffi/zip_writer.rs::add_directory_recursive`
(R0076-0019). So the asymmetry this record set out to close is **partially** closed: listing-side
`raw_path` plus the two backend `path` fields, with `add_file_from_path` refusing rather than
preserving, and every write/extract-destination hand-off still lossy.

One qualification on OI-0076-001's own wording: R0076-0083 states there is "no extract-by-raw-bytes
surface, so callers cannot extract a non-UTF-8 entry by exact name," but a public **id-addressed**
surface does exist — `Archive::extract_by_ids` (`src/extraction.rs`), re-exposed as
`ReadArchive::extract_by_ids` (`src/archive/mode_split.rs`) — which selects entries by
`ArchiveEntry::id` and never needs the name. What is genuinely absent is a public by-id or
raw-bytes **read** surface: `extract_to_memory` / `extract_to_stream` and their `_with_options`
forms are all `&str`-keyed, and the id-based stream (`ValidatedSource::extract_to_stream_by_id`) is
`pub(crate)`. That, plus the destination-side lossy conversions above, is what remains; whether
`extract_by_ids` already satisfies OI-0076-001's Required Action 5 (its Verification checkbox is
still unchecked) is an owner call, not settled here.

**Disposition: this record stays ACTIVE.** Option A remains the governing policy and no later
record reversed it; this amendment narrows the closure claim to the shipped scope and forward-links
the open remainder to OI-0076-001. Cross-references: OI-0075-001 (RESOLVED 2026-04-29 — the scope
this record actually delivered); OI-0076-001 (OPEN — the 12-site remainder and the read-surface
gap); OI-0075-004 / R0075-0083 (`detect_multipart` typed return shape); AD 0042 (non-UTF-8 password
policy, the sibling reject-loudly ruling).
