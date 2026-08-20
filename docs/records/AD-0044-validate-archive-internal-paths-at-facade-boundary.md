---
type: ADR
title: "AD 0044: Validate archive-internal paths at the creation/modification facade boundary"
description: "Review 0063 (R0063-0001 through R0063-0006) flagged that the public write-path APIs — Archive::addfilefromdata, addfilefrompathas, adddirectory,…"
tags: [decision, ADR-0044, R0063-0001, R0063-0006]
timestamp: 2026-04-18T00:00:00Z
status: active
---

# AD 0044: Validate archive-internal paths at the creation/modification facade boundary

## Context and Problem Statement

Review 0063 (R0063-0001 through R0063-0006) flagged that the public write-path APIs — `Archive::add_file_from_data`, `add_file_from_path_as`, `add_directory`, `add_entry`, `add_directory_entry`, `remove_entry` — forwarded caller-supplied archive-internal names straight to the backend with no validation. The extraction side (`src/security.rs::sanitize_entry_path`) enforces a strict policy: traversal segments are stripped, absolute prefixes are dropped, paths that collapse to empty are rejected. The write path did not enforce the same policy, producing a silent asymmetry.

The concrete consequence: a caller passing `"../../secret"` to `add_file_from_data` got a successful write, and later extracting that archive with this crate's own extractor produced an entry at `secret` (traversal components stripped) rather than the caller's intended name. The archive round-trips through **different names** in and out, with no error anywhere.

## Decision Drivers

* A facade with its own opinionated error surface should not accept names that its own extractor silently rewrites. The asymmetry is a footgun for downstream authors.
* All six affected APIs already return `Result<_, ArchiveError>`; propagating `ArchiveError::InvalidPath` is free.
* The validator needs to be consistent with the extraction policy — rejecting what `sanitize_entry_path` strips — so that authored names round-trip losslessly.
* Non-goals: changing the low-level ZIP/libarchive writers (they remain permissive), and changing the extraction-side sanitize-and-continue policy (it is a security gate, not an authorship gate).

## Considered Options

1. **Keep silent-strip-on-extract asymmetry.** Rejected: silent rewrites at an API boundary are user-visible surprises.
2. **Normalize (strip) on write instead of reject.** Rejected: the write helper would accept `"../../secret"` and silently rename it to `"secret"`. Callers deserve an error, not a silent rename.
3. **Add a shared `validate_archive_internal_path` helper that rejects empty, NUL-containing, traversal, and absolute names, and wire it at the six facade sites.** Accepted.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

### `src/security.rs`

Added `pub(crate) fn validate_archive_internal_path(archive_path: &str) -> Result<()>`:

* Rejects empty strings with `ArchiveError::InvalidPath`.
* Rejects paths containing NUL (`\0`).
* Walks `Path::new(archive_path).components()`; `Component::ParentDir` and `Component::RootDir`/`Component::Prefix` both produce `InvalidPath`.
* Accepts `Component::Normal` and `Component::CurDir` (the latter so `"./file.txt"` round-trips as `"file.txt"`).

Five unit tests cover: normal names, empty, NUL, traversal (three variants), absolute.

### Facade wiring (six call sites)

* `src/creation.rs::add_file_from_data` — validate `path` before backend dispatch.
* `src/creation.rs::add_file_from_path_as` — validate `archive_path`.
* `src/creation.rs::add_directory` — validate `path`.
* `src/modification.rs::add_entry` — validate after mode check, before pushing into the modification set.
* `src/modification.rs::add_directory_entry` — same.
* `src/modification.rs::remove_entry` — same. (Validation fails fast on malformed removal targets instead of silently not-matching.)

`add_file_from_path` delegates to `add_file_from_path_as`, so it is covered transitively.

The helper remains `pub(crate)` — callers do not need to run it themselves.

## Consequences

* Callers passing names the extractor would later rewrite now get `ArchiveError::InvalidPath` up front instead of a silent-rename surprise.
* No well-formed caller breaks: benign names (`"file.txt"`, `"dir/file.txt"`, `"./file.txt"`) pass validation unchanged.
* Extraction-side `sanitize_entry_path` is unchanged; its strip-and-continue behavior is still correct defense for archives *produced by other tools*.
* The validator is intentionally stricter than `sanitize_entry_path` on the authorship side: where sanitize says "strip and accept", validate says "reject". That asymmetry is the point.

## Revisit trigger

Re-open if:

* A legitimate use case appears for writing `..`-containing archive entries (none known).
* The validator needs to become platform-aware (e.g. rejecting backslashes on Unix, reserved DOS device names on Windows) — currently deferred as out-of-scope for this review.

## Amendment (2026-08-20, archive-portable validation replaces host semantics)

The revisit trigger above has fired, and the resolution is not the one it
anticipated: the validator did not become *platform-aware*, it became
**platform-independent**. `validate_archive_internal_path` now judges the
name against an archive-portable policy by default
(`security::ArchivePathPolicy::Portable`), with an explicit opt-out for
callers who deliberately want the writing host's rules
(`security::ArchivePathPolicy::Host`, selected process-wide through
`security::set_archive_path_policy`, or per-name through
`security::validate_archive_internal_path_as`).

### Why this record's central promise read as broken

The Implementation section above states the validator "walks
`Path::new(archive_path).components()`" and accepts `Component::CurDir`
"so `"./file.txt"` round-trips as `"file.txt"`". That delegation is what
broke the Decision Drivers' promise that "authored names round-trip
losslessly", in two ways that a reader of this record could not have
predicted:

1. **`std` eats `.` segments before the check can see them.** Verified by
   experiment (rustc, 2026-08-20): `"./a/b"` yields
   `[CurDir, Normal(a), Normal(b)]`, but `"a/./b"` and `"a/b/."` both
   yield `[Normal(a), Normal(b)]` — the interior and trailing dots are
   gone. The `CurDir` arm therefore only ever fired for a *leading*
   `./`. `add_file_from_data("a/./b", …)` passed validation, was written
   verbatim by the permissive backend writer, and came back out of this
   crate's own extractor as `a/b` — precisely the "archive round-trips
   through **different names** in and out, with no error anywhere"
   footgun this record exists to close.
2. **Host path grammar leaked into a portable container.** On a Unix
   writer `Path::components()` reports `"C:/x"` as `[Normal("C:"),
   Normal(x)]` and `"n:stream"` as one `Normal` segment, so both passed.
   A Windows consumer extracting the same archive reads `C:` as a
   `Prefix` (dropped, so the entry silently becomes `x`) and `n:stream`
   as an NTFS alternate-data-stream write. `CON`, `NUL.txt`, `name.` and
   `name ` passed too, though Windows cannot create the first two at all
   and silently trims the trailing `.`/space from the others.

### What the validator now rejects

Under the default portable policy, in **every** segment position:

* empty segments (`a//b`) — with one carve-out below; `.`; `..`;
* NUL bytes; the empty name; any absolute prefix;
* a drive-letter prefix (`C:/x`, `c:\x`) and a `:` anywhere in a segment
  (NTFS alternate data streams);
* a segment ending in `.` or ` `;
* Windows reserved device names — `CON`, `PRN`, `AUX`, `NUL`,
  `COM1`–`COM9`, `LPT1`–`LPT9` — case-insensitive, with or without an
  extension (`nul.txt` is refused, `nullable.rs` is not).

Two deliberate limits. **Carve-out:** exactly one trailing `/` is the
conventional spelling of a directory entry (`add_directory("dup/")`) and
is treated as a separator artifact, not an empty segment; any other empty
segment is refused. **Not policed:** the rest of the Windows-forbidden
character set (`< > " | ? *`) and control characters are still accepted —
the portable policy covers the classes that silently *rename* or *fail*
an entry, and widening it further is a separate decision.

The name check is no longer delegated to `Path::components()` at all: the
input is normalised (`\` → `/`, as R0070-0020 already required) and split
on `/`, so each segment is judged in place on every host.

### Consequences beyond the original record

* The asymmetry this record calls "the point" (validate rejects what
  sanitize strips) is now genuinely symmetric for `.` in all positions,
  and is *stricter* than the extraction-side sanitiser for the portable
  name classes — deliberately, because sanitize defends against archives
  written by other tools while validate governs authorship.
* Rebuild paths that re-author existing entry names through the facade
  (`commit_changes` re-adds directory entries via `add_directory`) will
  now refuse a wild archive carrying a non-portable directory name. That
  is the intended reading of "callers deserve an error, not a silent
  rename", but it is a behaviour change for archives produced elsewhere;
  the host policy is the escape hatch.
* AD 0066's extraction-side counterpart landed in the same pass: the
  strict-reject opt-in (`ExtractionLimits::reject_unsafe_paths`) is now
  enforced rather than merely recorded, so a caller can have hostile
  entry names refused instead of repaired on the read side as well.
