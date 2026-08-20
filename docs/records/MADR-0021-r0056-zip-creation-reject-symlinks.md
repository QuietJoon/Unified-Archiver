---
type: ADR
title: "AD: Reject symlinks in ZIP recursive creation"
description: "Implemented; still active, with the 2026-08-05 §B amendment correcting the false \"ZIP has no symlinks\" premise (ZIP stores them via S_IFLNK in the external attributes) and re-basing the reject-on-creation ruling on FR-022 link policy."
tags: [decision, ADR-0021, R0056-0027, FR-022]
timestamp: 2026-04-16T00:00:00Z
status: active
---

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

## Amendment (2026-08-05, decision-review-2026-07-19 §B — false format premise corrected, ruling re-based on FR-022)

The review found one of this record's Decision Drivers factually **false**: "ZIP format does not
natively support symlinks", restated in the Decision Outcome as an error that "explains that ZIP
format does not support symlinks". ZIP *does* represent symlinks — the Unix mode word carried in an
entry's external-attributes field holds `S_IFLNK`, and this crate's own ZIP reader relies on that:
`classify_zip_entry_type` (`src/ffi/zip_wrapper.rs`) tests `S_IFLNK` before `S_IFDIR` on the `zip`
crate's `unix_mode()` and returns `EntryType::Symlink` (with its own local `S_IFMT` / `S_IFLNK`
constants — `src/ffi/common.rs::unix_mode_is_symlink` reads the same bits but is called only by the
7z backend, whatever its "shared by the ZIP and 7z backends" rustdoc says), and
`tests/integration/link_skip_single_file.rs::build_zip_with_symlink` builds a real symlink-bearing
ZIP with `zip --symlinks`, which its own comment calls "the authoritative reference for the
S_IFLNK-on-external-attributes layout". Symlink entries in a ZIP are therefore read, classified, and
*policy*-skipped, not unrepresentable: bulk ZIP extraction pushes `ArchiveWarning::SkippedSymlink`
and continues, and the single-entry paths refuse with the `OperationBlocked` built by
`src/error.rs::link_extract_blocked` ("refusing to materialize per FR-022 link-skip policy").

**The ruling stands and the record stays ACTIVE, re-based on the security policy rather than on
format capability.** The governing requirement is FR-022 in `specs/001-unified-archive/spec.md`
(Functional Requirements): "Library MUST skip symbolic links and hard links during archive
operations with a warning, as cross-platform symlink handling is not reliably supported across all
archive formats and operating systems" — a portability/security policy that holds precisely *because*
formats can carry links that hosts cannot reliably materialize. This record's other two drivers —
"FR-022 establishes link rejection as a security invariant across the codebase" and "Silent data loss
is worse than an explicit error" — are untouched by the correction and carry the decision on their
own.

The false premise never reached the shipped error text: it lives in this record's own paraphrase,
while the code has said "not supported for ZIP creation" since the check first appeared (commit
`026f4b2`, 2026-04-18) — the phrase "ZIP format does not support symlinks" has no history in `src/`
at all. The check this record
introduced still sits in `ZipWriter::add_directory_recursive` (`src/ffi/zip_writer.rs`), where
`DirWalkKind::Special { is_symlink: true }` raises `operation_blocked("add_directory_recursive",
"Refusing to archive symlink '{}': symlinks are not supported for ZIP creation")` — scoped to ZIP
*creation*, not to the format — and `ZipWriter::add_file_from_path` calls
`src/ffi/common.rs::reject_symlink_path` ("symlinks are not supported for archive creation"). The
policy is additionally enforced **format-agnostically at the facade**:
`src/creation.rs::validate_file_path` / `validate_directory_path` use `symlink_metadata` so a
symlinked file or root is refused without being followed (R0070-0022), and
`Archive::add_directory_recursive` (`src/creation.rs`) preflights the whole tree through
`prewalk_into_tracker`, whose `walk_directory_tree` runs with `follow_links(false)` and rejects via
`reject_special_entry` before any backend writes an entry (R0080-0033); the ZIP-writer checks remain
defence in depth. That generalisation is only coherent on the policy reading — a ZIP-format-capability
premise could never have covered the libarchive creation backends.

Two notes for future readers. First, record-id hygiene: the comment above the ZIP-writer check reads
"AD 0021: symlinks are rejected at creation time to avoid silent data loss and to keep creation-side
link policy aligned with extraction rejection" — that "AD 0021" is **this** record (legacy gate-0056 /
MADR-0021), whereas the "AD 0021" cited in `specs/001-unified-archive/spec.md`'s FR-017 note on
per-entry creation progress is today's unrelated `AD-0021`. Second, this record's own Decision
Outcome left the door open ("Option 3 (skip with warning) could be added later behind an option
flag"); that option has **not** been taken, and whether creation should ever offer a
skip-with-warning or store-the-link mode remains an open owner question, not settled here. The live
follow-up on the mechanism (not the policy) is OI-0076-003 item 2 / R0076-0014: replacing the
creation-time `reject_symlink_path` + `File::open` pair with a no-follow open to close the TOCTOU
window.
