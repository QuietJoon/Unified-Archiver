---
type: ADR
title: "AD: Eliminate redundant middle open in modify()"
description: "Implemented at the time; the eliminated middle open was later reintroduced as a format-native encryption probe — see the 2026-08-04 amendment."
tags: [decision, ADR-0016, R0002-0009]
timestamp: 2026-04-16T00:00:00Z
status: active
---

# AD: Eliminate redundant middle open in modify()

## Context and Problem Statement
Found in Review 0002 (Issue R0002-0009, Severity: Medium).
Location: `src/modification.rs:108`

`Archive::modify()` opened the archive three times:
1. `ArchiveFormat::detect()` — format detection
2. `Archive::open()` — encryption preflight check (used native backends: Piz/SevenZ)
3. `LibarchiveArchive::open()` — actual modify handle (libarchive only)

The middle open (#2) used a different backend than the modify handle (#3),
doubling I/O and creating backend divergence before any rewrite begins.

## Decision Drivers
* The encryption check only needs `list_files_metadata_only()` to scan `is_encrypted` flags
* The libarchive backend (opened in step #3) can perform the same check
* Eliminating the middle open removes one full archive parse and reduces I/O

## Considered Options
1. Check encryption via the libarchive handle that will be used for modification
2. Keep the separate `Archive::open()` for encryption check (separation of concerns)

## Decision Outcome
ACCEPT (Option 1): Check encryption on the libarchive handle directly.

Status: Implemented

### Implementation
- `src/modification.rs`: Removed `Archive::open()` encryption preflight block.
  After opening `LibarchiveArchive`, call `list_files_metadata_only()` on the
  same handle and check `entries.iter().any(|e| e.is_encrypted)`.
- The advisory file lock acquisition was moved before the libarchive open
  (previously it was between the two opens).

## Consequences
* Good, because modify() now opens the archive twice instead of three times
* Good, because the encryption check uses the same backend as the modification
* Good, because format detection (#1) + libarchive open (#2) is the minimum required

## Amendment (2026-08-04, decision-review-2026-07-19 §B — the eliminated open was reintroduced)

The "opens the archive twice" claim no longer describes `Archive::modify()`. The separate
native-backend encryption preflight this record eliminated is back: `modify()` today calls
`Archive::open_as_format(&path_buf, format)` and checks `is_encrypted()` on that probe handle
**before** the libarchive modify backend is ever opened (`src/modification.rs::modify`), rather
than scanning `is_encrypted` flags on the modify handle's own `list_files_metadata_only()`
listing as recorded above. The modify path therefore opens the pathname **three times** again —
(1) the advisory-lock descriptor (fs4 exclusive lock + inode-identity capture), (2) the
`open_as_format` encryption probe (format-native backend for the modifiable formats: ZIP via the
`zip` crate per the AD 0007 collapse / DCR-009, 7z via sevenz), (3) `LibarchiveArchive::open` as
the modify read backend. That is the same open count as the pre-record shape, composed
differently: format detection no longer costs its own open (`detect_format_from_locked` reads the
magic prefix from a clone of the locked descriptor, R0070-0003), and the reinstated middle open
now carries the encryption check.

The change that reinstated the probe predates the root of the surviving (squashed) git history
and has no dedicated decision record; the earliest surviving trail already treats the probe as
established and hardens it rather than re-removing it:

- the v0.2.0 CHANGELOG "Known limitations" defers R0066-0009 ("modify encryption probe") into the
  modify-architecture cluster;
- AD 0051 (R0068-0009) remaps the probe's `Password`-flavoured failures to a clean
  `OperationBlocked(MODIFY, …)`;
- AD 0059 (R0069-0056) moves the advisory lock ahead of the probe;
- AD 0060 (R0070-0004) makes the probe propagate listing failures instead of swallowing them;
- DCR-007 (R0080-0041) revalidates the locked inode identity immediately before the probe's
  pathname reopen.

Why the libarchive-handle check this record adopted was abandoned is likewise not recorded; the
call-site comments document what the probe provides today — header-encrypted formats refuse to
surface a listing without a password, and the format-native open raises a typed
`ArchiveError::Password` that `looks_like_encryption` remaps to the uniform modify-blocked
reason. No later record reverses this record's ruling, which stands as a correct point-in-time
elimination of a then-redundant open, so the record stays **active** with this factual refresh.
Whether the probe should be re-collapsed into the modify backend is an open question belonging to
the deferred modify-via-native-backends cluster (R0066-0009/0014); this amendment does not decide
it.
