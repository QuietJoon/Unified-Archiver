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
