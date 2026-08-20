---
type: ADR
title: "AD: Wire ResultWithWarnings into extract_all (R0052-0006/R0052-0007)"
description: "Implemented"
tags: [decision, ADR-0010, R0052-0006, R0052-0007, FR-022]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Wire ResultWithWarnings into extract_all (R0052-0006/R0052-0007)

## Context and Problem Statement
Found in Review 0052 (Issues R0052-0006 and R0052-0007, Severity: High).
Location: `src/extraction.rs` — `Archive::extract_all()`

Per FR-022, symlinks and hardlinks are skipped during extraction for security. Previously, `extract_all()` returned `Result<()>` — callers had no way to know which entries were skipped. The existing `ResultWithWarnings<T>` type and `ArchiveWarning::SkippedSymlink`/`SkippedHardLink` variants existed but were not wired into the extraction return path.

## Decision Drivers
* Callers need to know what was skipped — silent data loss is unacceptable
* `ResultWithWarnings<T>` already exists in the crate's public API
* Backward compatibility: existing callers using `.unwrap()`, `.expect()`, `?`, or `.is_ok()` must continue to work

## Considered Options
1. Return `Result<ResultWithWarnings<()>>` from `extract_all()` — live warning surface
2. Side-channel warnings via a separate `get_warnings()` method (rejected — stateful, easy to forget)
3. Log warnings only (rejected — no programmatic access)

## Decision Outcome
ACCEPT (Option 1): `extract_all()` now returns `Result<ResultWithWarnings<()>>`.

All five backends (`libarchive`, `piz`, `zip`, `sevenz`, `unrar`) collect `ArchiveWarning` entries at their symlink/hardlink skip points and return `Result<Vec<ArchiveWarning>>`. The `extraction.rs` dispatch layer wraps the vector into `ResultWithWarnings`.

Status: Implemented

### Implementation
- `src/ffi/libarchive_wrapper.rs`: `extract_all_with_options` returns `Result<Vec<ArchiveWarning>>`
- `src/ffi/piz_wrapper.rs`: Same pattern — warning at `piz_entry_is_symlink` skip
- `src/ffi/zip_wrapper.rs`: Same pattern — warning at `is_symlink()` skip
- `src/ffi/sevenz_wrapper.rs`: Same pattern — warning captured in `for_each_entries` closure
- `src/ffi/wrapper.rs`: Same pattern — separate `SkippedSymlink`/`SkippedHardLink` based on `EntryType`
- `src/extraction.rs`: `extract_all()` return type changed; match block uses `?` and wraps in `ResultWithWarnings`
- `src/error.rs`: Added `#[derive(Debug)]` to `ResultWithWarnings<T>` for test compatibility
- `src/lib.rs`: Added `ArchiveWarning` to public re-exports
- `tests/integration/hardlink_skip.rs`: Updated to verify warnings are returned

### Backward Compatibility
- `Result<ResultWithWarnings<()>>` is compatible with all existing usage patterns:
  - `.unwrap()` / `.expect()` → yields `ResultWithWarnings<()>`, discarded by `;`
  - `?` in `fn -> Result<()>` → `?` unwraps to `ResultWithWarnings<()>`, discarded by `;`
  - `.is_ok()` → works on both `Result<()>` and `Result<ResultWithWarnings<()>>`
- One test (`large_zip_mmap_test.rs`) needed `Ok(())` → `Ok(_)` pattern update

## Consequences
* Good, because callers can now discover which entries were skipped during extraction
* Good, because the warning surface is the return value — no side-channel or state management
* Good, because backward compatibility is preserved for existing callers
* Bad, because callers who want to inspect warnings must destructure `ResultWithWarnings` (but can ignore it)

## Amendment (2026-08-09, Review 0001 R0001-0001/R0001-0022 — skip class widened, notification not yet)

**The set of entries extraction skips is now larger than the set it warns about.** This record's
decision driver is "callers need to know what was skipped — silent data loss is unacceptable", and
that principle is currently satisfied for links only.

DCR-010 (paired with this amendment) widened the FR-022 policy from a link denylist to a kind
allowlist: on the libarchive bulk walk only `AE_IFREG` and `AE_IFDIR` proceed, and the 7z backend
now decodes `S_IFMT` in full and maps FIFOs, sockets and device nodes to `EntryType::Other`, which
extraction refuses. Before that, a crafted archive could have `archive_write_disk` materialise a
special node inside the destination.

Those new skips emit **no warning**. `ArchiveWarning` carries `SkippedSymlink` and `SkippedHardLink`
and nothing that describes an unsupported entry kind, and reusing a link variant for a device node
would assert something false about the archive's contents to exactly the caller most likely to be
auditing it — a worse outcome than silence in a security-sensitive library. Adding a variant to a
public enum was outside the routed scope of the review that produced the fix. The ZIP backend has
skipped `EntryType::Other` silently since R0081-0077, so the three backends are at least consistent.

The gap is tracked as a TicGit follow-up (`ArchiveWarning::SkippedUnsupportedEntry`, emitted from all
three backends). Until it lands, treat this record's guarantee as: *every skipped **link** is
reported*; a skipped special entry is not. The record stays **active** — the decision is unchanged,
its coverage is incomplete.
