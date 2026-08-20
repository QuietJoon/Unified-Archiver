---
type: DCR
title: "Modify mode revalidates locked-file identity before every pathname use"
description: "AD 0009's advisory lock is now paired with dev/ino capture + revalidation so commit cannot touch an unrelated replacement inode."
tags: [change, project-control, DCR-007]
status: active
---

# DCR-007: Modify mode revalidates locked-file identity before every pathname use

- **Date:** 2026-07-17
- **Source:** Review 0080, Issues R0080-0005, R0080-0041..0046 (and R0080-0061 for the UnRAR recovery reader)
- **Affected ADRs:** MADR-0009-r052-advisory-file-locking-modify-mode.md (updated — amendment)

## What Changed

AD 0009's advisory lock binds to the inode opened by `Archive::modify()`, but every later
operation re-resolved `self.path` by name — a non-cooperating process could rename the
archive away, drop a different file at the pathname, and have the encryption probe, read
backend, ZIP-extras load, retained-entry replay, permission preservation, backup, or the
final commit replace operate on that unrelated file. `modify()` now captures the locked
file's identity (Unix `dev`/`ino` from the lock descriptor; non-Unix: not captured, tracked
follow-up) and revalidates it immediately before each pathname-based use, aborting with a
typed `OperationBlocked` identity-drift error on mismatch. The UnRAR recovery-percentage
reader gained the same construction-time capture + pre-parse revalidation.

## Why

AD 0009 accepted "the archive being modified can be corrupted" by non-cooperating writers —
not the escalation of destroying, backing up, or permission-cloning an unrelated third file.
A small check-to-use window remains (documented in the `modify()` rustdoc); closing it fully
needs descriptor-relative I/O, which libarchive's path-based API does not offer today.

## Affected Areas

- src/modification.rs (`LockedFileIdentity`, capture + seven revalidation sites)
- src/ffi/wrapper.rs (recovery reader identity check)

## Migration / Follow-up

- Windows identity capture (file index via handle info) is a follow-up under the same
  design; non-Unix currently skips revalidation with a comment.
- OI-0080-002 tracks the analogous read-side (mmap) hardening question.

## Amendment (2026-07-22, R0081 I6)

**Feasibility outcome for the owned-fd hand-off named in "Why".** "Why" says closing
the residual window fully "needs descriptor-relative I/O, which libarchive's path-based
API does not offer today." That is not quite right: `archive_read_open_fd` *does* exist
and links against the system libarchive (3.8.8). It was evaluated this pass as a way to
retire the by-name revalidation entirely (hand the identity-verified descriptor to the
backend so no pathname is re-resolved) and **rejected** — it is not a safe replacement
for the per-operation by-name reopen in this crate's architecture:

1. libarchive's read handle is iterator-shaped and cannot be rewound; the backend
   reopens the file once per operation (list, extract, integrity). There is no single
   fd to hold for the handle's lifetime — the fd must be re-handed per operation.
2. One owned fd re-handed per operation shares a single file offset across every reopen.
   The public API returns `StreamingExtractor` handles that outlive the call and permits
   reads in modify mode, so two live libarchive handles reading from one offset corrupt
   each other. The current path-based reopen gives each handle its own offset.
3. There is no portable way to get an *independent* open-file description from an
   existing fd: `dup(2)` shares the offset, and on macOS `/dev/fd/N` shares it too
   (verified empirically on the dev host). Only Linux `/proc/self/fd/N` yields a fresh
   description, and macOS + Windows are first-class targets.
4. The only fd-anchored reopen that keeps independent offsets — open by name, then
   `fstat` the fresh fd against the pinned identity — *is* this revalidation. The capture
   here already reads the authoritative advisory-lock descriptor (`File::metadata` is an
   `fstat`), so the baseline is already as strong as an fd hand-off; only the re-resolution
   window remains, and it is within AD 0009's cooperating-process threat model.

**Superseding status.** The by-name revalidation pattern is *retained*, not superseded —
for this architecture it is the strongest guard the platform allows. The seven sites keep
their guards, split by why each cannot use a descriptor hand-off: the read-backend bind,
retained-entry replay, permission preservation, and final replace re-resolve `self.path`
by name (libarchive can't consume the lock fd safely per above; the final replace is a
`rename`/backup that is inherently path-based); the encryption probe goes through the
generic `open_as_format` facade (backend chosen by format — piz/sevenz for ZIP/7z, which
are also path-only or mmap-by-path); the ZIP source-extras walk opens by name via the
`zip` crate. UnRAR's recovery reader (`src/ffi/wrapper.rs`) is a path-only SDK
(`RAROpenArchiveEx` takes a pathname) and can never take an fd.

**What landed instead (the sanctioned tighten/dedupe).** The three previously-duplicated
`(dev, ino)` capture implementations — `modification::LockedFileIdentity`,
`archive::ReadFileIdentity`, and `ffi::wrapper::UnrarFileIdentity` — now share one audited
Unix capture primitive, `crate::fs_identity::InodeId` (single `MetadataExt` site). Each
guard keeps its own comparison semantics (read also compares length; modify/UnRAR compare
`(dev, ino)` only and skip off Unix) and its own diagnostic message. The
`fs_identity` module documents the fd-infeasibility finding so it is not re-litigated.
Cross-refs: AD 0009 amendment of the same date; OI-0081-001 / OI-0081-005 notes.
