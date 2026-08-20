---
type: DCR
title: "Extraction rejects non-regular, non-directory entries on every backend"
description: "FIFOs, sockets and device nodes are no longer materialised by the libarchive disk writer and are no longer decoded under file semantics by the 7z backend. The FR-022 skip class widens from links to every unsupported entry kind; the caller-visible warning for the new class is deferred."
tags: [change, project-control, DCR-010, ADR-0010, ADR-0029, R0001-0001, R0001-0022, FR-022]
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-09T00:00:00Z
status: active
---

# DCR-010: Extraction rejects non-regular, non-directory entries on every backend

- **Date:** 2026-08-09
- **Source:** Review 0001, issues R0001-0001 (Critical) and R0001-0022 (High)
- **Affected ADRs:** MADR-0010-r052-extraction-warnings-result-with-warnings.md (amended — paired),
  AD-0029-extraction-api-redesign-extract-some.md (unchanged; its FR-022 parity requirement is
  satisfied by construction)

## What Changed

FR-022 was implemented as a **link** policy: `AE_IFLNK` entries and entries with a non-NULL
`archive_entry_hardlink` were skipped with an `ArchiveWarning`, and every other entry proceeded.
"Every other entry" included FIFOs, sockets, and character and block device nodes — a crafted
archive could ask `archive_write_disk` to create them inside the destination, and `parse_entry`
already classified them `EntryType::Other`, so the backend had the information to refuse and did
not use it.

The policy is now **kind-allowlisted rather than link-denylisted**. Only `AE_IFREG` and `AE_IFDIR`
proceed to destination and staging setup in the libarchive bulk walk; everything else is skipped
before any header is written. Independently, the 7z backend classified everything that was neither
a symlink nor a directory as `EntryType::File`, so the same special modes — reachable through the
upper 16 bits of `windows_attributes` on p7zip-created archives — were decoded under file
semantics. It now decodes `S_IFMT` in full and maps unsupported kinds to `EntryType::Other`, which
extraction refuses.

The ZIP backend already skipped `EntryType::Other` (R0081-0077), so the three backends now agree.

## Why

An archive is untrusted input. Creating a FIFO or a device node in a destination directory expands
the extraction attack surface beyond the regular files and directories every caller expects, and
nothing in the crate's contract ever promised to materialise them. Refusing is the fail-closed
reading of FR-022's intent; skipping only links was an incomplete implementation of it, not a
deliberate boundary.

## Affected Areas

- `src/ffi/libarchive_wrapper/reader.rs` — bulk extraction walk
- `src/ffi/sevenz_wrapper.rs` — `parse_entry` classification and the extraction paths that consume it
- `src/ffi/zip_wrapper.rs` — unchanged; already conformant

## Migration / Follow-up

**The caller-visible half is deliberately incomplete, and this is the part a future reader needs.**
MADR-0010's decision driver is *"callers need to know what was skipped — silent data loss is
unacceptable"*, and the new skip class is currently **silent**: `ArchiveWarning` has
`SkippedSymlink` and `SkippedHardLink` and nothing that fits a device node, and emitting a link
variant for a FIFO would hand a security-auditing caller a false statement about what the archive
contained — worse, in a security-sensitive library, than saying nothing. Adding a variant to a
public enum is a semver-visible change that was outside this review's routed scope.

So the security property is fully landed (special nodes never reach the disk writer) while the
notification property regressed in coverage relative to MADR-0010's stated principle. A follow-up
ticket tracks adding `ArchiveWarning::SkippedUnsupportedEntry` and emitting it from all three
backends; MADR-0010 carries a paired amendment recording the same gap.

Also note the knock-on to progress accounting: because skipped entries contribute declared size to
the denominator but no decoded bytes to the numerator, a successful extraction could finish below
100%. That is fixed in the same batch (R0001-0055) by building the denominator from extractable
regular-file payloads only and emitting a final completion callback.
