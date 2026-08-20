---
type: ADR
title: "AD: Archive creation rejects existing files"
description: "Implemented; ruling unchanged and now enforced atomically — the racy path.exists() pre-check was replaced by OpenOptions::create_new(true) in both writer backends (amended 2026-08-05, §B)."
tags: [decision, ADR-0017, R0026-0001, OI-0026-001]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Archive creation rejects existing files

## Context and Problem Statement
Found in Review 0026 (Issue R0026-0001, Severity: HIGH).
Location: `src/creation.rs`

`Archive::create()` would silently overwrite existing files because both the ZipWriter and libarchive backends truncate on open. This could lead to accidental data loss.

## Decision Drivers
* Data safety: accidental overwrites are a common source of user data loss
* Principle of least surprise: creation should not destroy existing data
* An explicit overwrite flag can be added later if needed

## Considered Options
1. Reject existing files with an `AlreadyExists` error (fail-safe)
2. Add an `overwrite: bool` parameter to `create()`
3. Keep current behavior (implicit overwrite)

## Decision Outcome
ACCEPT: Option 1 — `create()` now checks `path.exists()` before routing to any backend and returns an `AlreadyExists` I/O error. An optional overwrite flag may be added later (tracked as OI-0026-001).

Status: Implemented

### Implementation
- `src/creation.rs`: Added `path_buf.exists()` check at the top of `create()`, before backend routing
- Returns `ArchiveError::io("create", &path_buf, IoError::new(AlreadyExists, ...))`

## Consequences
* Good, because existing files are never silently overwritten
* Good, because the check is centralized (covers both ZipWriter and libarchive backends)
* Bad, because users who want to overwrite must delete the file first (minor friction)

## Amendment (2026-08-05, decision-review-2026-07-19 §B — ruling unchanged, mechanism now atomic)

The 2026-07-19 review flagged this record's *mechanism* as racy, and it was: a
time-of-check/time-of-use window sat between `create()`'s existence test and the writer's own
`open`. **The ruling is unchanged — creation still refuses an existing destination — but the
`path.exists()` pre-check this record's Implementation section describes is historical.** Rejection
is now performed atomically by the two writer constructors with
`OpenOptions::new().write(true).create_new(true)` — `O_CREAT|O_EXCL` on Unix, `CREATE_NEW` on
Windows — so the kernel, not the library, decides who wins the destination inode:

- `src/ffi/zip_writer.rs::ZipWriter::create` opens the final path exclusively before handing it to
  `RawZipWriter`; the comment there cites **R0069-0050** (Review 0069, recorded in AD 0059's
  closure list: *"`zip_writer::create` opens the destination via
  `OpenOptions::new().write(true).create_new(true)` to atomically reject existing files"*).
- `src/ffi/libarchive_wrapper/writer.rs::LibarchiveArchive::create` opens the output exclusively and
  attaches the descriptor via `archive_write_open_fd` instead of letting libarchive open by name;
  the comment there cites **R0072-0004**.

Both exclusive opens landed together on 2026-04-27 (commits `e58caf7` for ZIP, `e4f0cb9` for
libarchive); the now-redundant facade pre-check was deleted two days later by the **Review 0075
archive-layer hardening pass** (commit `b51ce8f`, 2026-04-29). AD 0063's R0075 closure tables record
no finding id for that removal, so it is attributable to that pass rather than to a specific
numbered finding. Today `Archive::create` in `src/creation.rs` carries only a comment in its place — *"Pre-existence is
detected by the writer constructors via `OpenOptions::create_new(true)` (O_CREAT|O_EXCL on Unix,
CREATE_NEW on Windows), which surfaces `AlreadyExists` atomically — no facade-level `exists()` race
window."* Note that neither `R0076-0017` nor `R0081-0036` belongs to this story: the former is ZIP
`finish()` durability (`sync_data` + parent fsync), the latter is the modify-mode `<path>.bak`
backup claim.

Two consequences of this record read differently now. The *"check is centralized"* Good is
formally inverted — the gate lives in each writer constructor, which is precisely what makes it
atomic — but its substance holds: there are still exactly two write backends, every typed
entrypoint (`create_zip`, `create_seven_zip`, `create_libarchive`) delegates to `Archive::create`,
and both constructors use `create_new(true)`, so formats made creatable later inherit the gate for
free — including TAR.ZST / TAR.LZ4 / TAR.LZMA, added to `ArchiveFormat::can_create` on 2026-08-04.
The error shape drifted: this record promised
`ArchiveError::io("create", &path_buf, IoError::new(AlreadyExists, ...))`, and the original
implementation filled that ellipsis with the advisory *"Output file already exists; remove it first
or use a different path"* (added in commit `582a0c2`, removed with the facade pre-check in
`b51ce8f` — the advisory was never part of this record's own text), whereas callers now receive the
OS `AlreadyExists` wrapped under
`ops::CREATE` (ZIP) or under the `"write_open"` label (libarchive), without that advisory text;
`tests/integration/creation.rs::test_existing_file_rejected` asserts only `result.is_err()`, so the
message change is untested either way. Separately, the escape hatch this record deferred is no longer
tracked: OI-0026-001 ("Archive creation overwrite flag") is filed in
`docs/project/open-issues-resolved.md` as **RESOLVED 2026-04-10** with the resolution *"`create()`
now rejects existing files with `AlreadyExists` error"* — i.e. closed by this very decision rather
than by shipping the flag, and `CompressionOptions` (`src/options.rs`) still has no `overwrite`
field (the `overwrite` flag there belongs to `ExtractionOptions`). Whether the creation overwrite
flag should be re-opened as its own OI or is intentionally out of scope is an **open question** this
amendment does not decide.

One honest residual, outside this record's original scope (`src/creation.rs`): the Windows-only,
`external-rar-create`-gated `src/external/rar.rs::RarCreator::assemble` still rejects via
`output_path.exists()`. That path cannot hold an exclusive descriptor because `rar.exe` creates the
archive itself, so whether to close that window (or accept it) is an **open question** this
amendment does not decide.

**Disposition: ACTIVE.** The decision governs unchanged and is now enforced more strongly than when
it was written. Cross-refs: AD 0059 (R0069-0050 landing note); AD 0063 (R0075 closure table);
AD 0067 (R0076-0017 routing, for contrast); OI-0026-001.
