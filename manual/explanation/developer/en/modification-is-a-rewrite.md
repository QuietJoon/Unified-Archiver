---
type: Explanation
title: Why modification rewrites the archive
description: The reasoning behind copy-on-write modification, the advisory lock and inode-identity guard that protect it, and why only ZIP and 7z are in scope.
tags: [modification, api, decision, MADR-0009, MADR-0016, DCR-007, AD-0020]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: modification-src, resource: src/modification.rs }
  - { id: format-src, resource: src/format.rs }
  - { id: fs-identity-src, resource: src/fs_identity.rs }
  - { id: madr-0009, resource: docs/records/MADR-0009-r052-advisory-file-locking-modify-mode.md }
  - { id: madr-0016, resource: docs/records/MADR-0016-r0002-modify-eliminate-redundant-open.md }
  - { id: dcr-007, resource: docs/records/DCR-007-modify-identity-revalidation.md }
  - { id: ad-0020, resource: docs/records/AD-0020-additive-modification-options-extension.md }
synced_hash: 149becd28793a4c2ea552a09d1b4055775c361961031f6a6c2862fd92d2af5da
---

# Why modification rewrites the archive

`Archive::modify` looks like an editing session — queue an add, queue a removal, commit — but
the commit does not edit anything. It builds a complete new archive from the old one and
renames it over the original. That choice shapes everything else about modify mode: its cost,
its concurrency rules, its lossiness, and the short list of formats it supports. This page
explains why the design landed there.

The applied side of this — the calls, the options, the error you get — is
[How to add, replace, or remove entries in an existing archive](../../../how-to/user/en/modify-a-zip-or-7z.md).

## There is no in-place edit to implement

The obvious alternative would be to patch the container: overwrite one entry's bytes, adjust
the directory, done. Every part of that is blocked by the formats themselves.

**The containers are not designed for it.** A ZIP's central directory is a trailing index of
byte offsets into the file. Replacing an entry with content of a different compressed length
shifts every subsequent local header, invalidating every offset after it as well as the
end-of-central-directory record. Deleting an entry leaves a hole that only a rewrite can close.
A 7z file is harder still: its header describes packed streams by position and length, and the
header carries its own CRC, so a payload edit invalidates a checksum stored elsewhere in the
file.

**Solid compression removes the notion of a per-entry edit.** In a solid 7z archive, many
files share one compression stream; the bytes for one member are not separable from their
neighbours' without decoding the whole block. "Replace file 3" means "decode the block,
substitute, re-encode" — which is a rewrite of that block at minimum, and in practice of the
archive, since block boundaries are chosen by the encoder.

**Checksums are recorded, not derived.** Both formats store per-entry CRCs and, for 7z, header
CRCs. Any byte-level patch obliges the writer to recompute and rewrite them in place. Getting
that wrong produces a file that opens and then fails halfway through extraction — the worst
possible failure mode for an archiver.

So an in-place editor is not a smaller version of the rewrite. It is a second, format-specific
writer implementation with a much narrower correctness margin, for the benefit of a workflow
(edit one entry in a large archive) that a rewrite already handles correctly.

## The rewrite pipeline, conceptually

`commit_changes` runs four stages, the last of which is the swap.

**Enumerate.** The source archive's listing is read and each entry is classified: retained,
removed, or dropped. Removal is keyed by entry id — the positional index in the listing — not
by path, because both ZIP and 7z legitimately contain duplicate paths and identity has to
survive that.

**Stage and gate.** Before any file is created, the plan is validated: the compression
override must not change the container format, every retained path must still satisfy the
write-side path policy, and the set of output names must be free of duplicates and of
file-versus-directory collisions. Failing here costs nothing but the listing read; failing
halfway through a write costs the whole rewrite. That gate is shared verbatim with the
non-consuming `try_commit_changes` on the `v2-api` handle, so a dry run and a real commit
cannot drift apart.

**Write a fresh container.** A new archive is created beside the original through the ordinary
creation facade — the same writer path `Archive::create` uses. Retained entries are streamed
from the source into it one at a time; queued entries follow. Nothing is copied at the byte
level: every entry is decompressed out of the old container and recompressed into the new one.
Sources whose length is not known up front — an added reader with no declared size, or a
retained entry from a streaming-only source — are drained into a bounded temporary file first,
because the libarchive writer needs a size in the entry header. That drain is capped at the
default single-file extraction limit so a hostile or endless source cannot fill the disk during
a commit.

**Replace atomically.** The temporary file's permission bits are set from the original, an
optional backup copy is taken, and a single rename installs the new file at the old path.
Readers either see the old archive or the new one, never a partial state.

### What that costs

Two costs are inherent and worth stating plainly, because callers are often surprised by them.

**Disk.** Peak usage is the original plus the full temporary copy, and the original again if a
backup was requested — roughly 3x the archive size for a backed-up commit. The temporary file
must live in the archive's own directory, because a rename across filesystems is not atomic;
you cannot redirect it to a larger volume.

**Time.** The rewrite is proportional to the *whole archive*, not to the size of the change.
Adding one small file to a 4 GB archive re-reads and re-compresses 4 GB. There is no cheap
path for a small edit, and none of the queueing calls hint otherwise — the cost lands entirely
at `commit_changes`.

The compression override exists partly to make that cost tunable: `level` and `progress` are
honoured by the rewrite, so a caller can drop to a faster level for a bulk edit and can report
progress over what is, unavoidably, a long operation.

## Concurrency and identity

A rewrite-and-replace commit is far more dangerous under concurrency than an in-place patch
would be, and the design has been hardened twice in response.

**The first hazard is the lost update.** Two sessions that both read the same source, both
build a full replacement, and both rename over the path do not corrupt anything — which is the
problem. The second rename wins and the first session's changes disappear with no error
raised anywhere. MADR-0009 answers this with an exclusive advisory lock taken on the archive
before anything else happens: before format detection, before the capability check, before the
encryption probe. Acquisition is non-blocking, so contention surfaces as an immediate
`OperationBlocked` instead of a deadlock, and the lock is held by a `File` in the handle so
RAII releases it. The decision was explicit that this is a cooperation mechanism, not a
security boundary: processes that do not use this library are unaffected. Locking *first*
rather than after the format checks closed a smaller window of the same shape, where a
concurrent writer could swap the file between the capability check and the backend's first
read.

**The second hazard is the replacement inode.** The lock binds to an inode; every later step
of the session re-resolved `self.path` by name. A non-cooperating process could rename the
archive aside and drop an unrelated file at that pathname, and the session's later steps —
encryption probe, read backend, ZIP central-directory walk, retained-entry replay, permission
cloning, backup copy, final replace — would operate on the substitute. Note what that
escalates to: MADR-0009 had accepted "a non-cooperating writer can corrupt the archive being
modified", but not "modify can back up, permission-clone, or atomically clobber a third file
the caller never named". DCR-007 closes the escalation. `modify` captures the locked file's
identity as Unix `(dev, ino)` read from the lock descriptor itself — an `fstat` of a
descriptor this session holds, immune to a path swap — and re-checks a fresh `stat` of the
pathname immediately before each by-name use, aborting with a typed identity-drift
`OperationBlocked` on mismatch. Two of the listed steps were closed differently, by removing
the by-name resolution altogether: the permission bits copied onto the replacement and the
bytes copied into a backup both come from the lock descriptor itself rather than from a
reopened pathname.

**What remains, and why it stays.** Revalidate-then-use narrows a check-to-use window; it does
not close one. A writer that ignores the advisory lock can still swap the pathname in the gap
between the check and the following syscall. The stronger fix — capture one identity-verified
descriptor at open and hand *that* to the backend so no pathname is ever re-resolved — was
evaluated and rejected on feasibility, and the reasoning is recorded in `src/fs_identity.rs`
so it is not re-litigated:

- libarchive's read handle is iterator-shaped and cannot be rewound, so the backend reopens
  the file once per operation. There is no single descriptor to hold for the handle's lifetime.
- Re-handing one owned descriptor per operation shares a single file offset across all
  reopens. The public API returns `StreamingExtractor` values that outlive the call and permits
  reads in modify mode, so two live libarchive handles on one offset would corrupt each other;
  the path-based reopen gives each its own offset.
- No portable way exists to derive an *independent* open-file description from an existing
  descriptor. `dup` shares the offset by definition; on macOS `/dev/fd/N` shares it as well.
  Only Linux `/proc/self/fd/N` yields a fresh description, and macOS and Windows are
  first-class targets.
- The one descriptor-anchored reopen that keeps independent offsets — open by name, then
  `fstat` the fresh descriptor against the pinned identity — *is* this revalidation.

So the residual window is accepted-permanent for this architecture rather than a pending
follow-up. What did land from that investigation was consolidation: three duplicated
`(dev, ino)` capture sites (modify, read-side open, the UnRAR recovery reader) collapsed into
one audited primitive, `crate::fs_identity::InodeId`, with each call site keeping its own
comparison semantics and diagnostic. On non-Unix the identity is not captured and revalidation
is skipped entirely: Windows handle-based identity is a tracked follow-up, so the
replacement-inode guard described above is simply absent there.

A related consequence worth naming: because a commit's failure modes include "the file you
locked is no longer at that path", `commit_changes` cannot offer a clean retry. It consumes
the handle on every outcome, and recovery means opening a fresh session. That is a deliberate
trade — the alternative is a handle whose backend state after a mid-rewrite failure is
undefined.

## How many times the file is opened

MADR-0016 is worth reading as a lesson in how a record can be correct and stale at once. It
observed that `modify` opened the archive three times — format detection, a native-backend
encryption preflight, and the libarchive modify handle — and eliminated the middle open by
checking encryption on the modify handle's own listing.

Today the path opens the pathname three times again, composed differently:

1. the advisory-lock descriptor, which also supplies the inode identity and, via a clone
   seeked to zero, the magic bytes for format detection;
2. an `open_as_format` encryption probe using the format-native backend;
3. `LibarchiveArchive::open` as the modify read backend.

Format detection no longer costs its own open — `detect_format_from_locked` reads the same
512-byte magic prefix from the locked descriptor, which also keeps detection inside the lock
window. But the native-backend encryption probe came back, because header-encrypted archives
refuse to surface a listing at all without a password: a probe that only inspects
per-entry flags on a listing cannot see them. The reinstated probe has been hardened rather
than re-removed (the lock moved ahead of it, its listing failures propagate instead of being
swallowed, its password-flavoured errors are remapped to one clean modify-blocked reason, and
DCR-007 revalidates identity before it). MADR-0016's ruling stands as a correct
point-in-time elimination; the record now carries an amendment stating the current open count.
Whether the probe should be re-collapsed into the modify backend belongs to the deferred
modify-via-native-backends cluster, and is not settled.

## Why only ZIP and 7z

`ArchiveFormat::capabilities` reports `modification: Support::Partial` for ZIP and 7z and
`Support::None` for everything else, and `Archive::modify` consults `can_modify` before it
does anything expensive. The gate is not arbitrary; it follows the write side.

The rewrite composes an existing reader with an existing *writer*. A format can therefore be
modified only if the crate can create it and can read individual entries out of the source
faithfully enough to re-emit them. RAR fails the first test — creation through the main facade
does not exist, and the optional Windows WinRAR bridge is a separate integration, not a
backend the rewrite can call. ISO fails it too: libarchive's ISO support is read-only.
Standalone compressed streams (`.gz`, `.bz2`, `.xz`, and friends) have no entry namespace to
add to or remove from, and their creation is out of scope by decision. The TAR family is the
interesting case: the crate can both read and write it, so nothing structural blocks a TAR
rewrite, and the rustdoc on `Archive::modify` says only "not yet implemented". Reading that
against the code, what is missing is the surrounding work rather than a blocker — the
retained-entry replay would need TAR's metadata model, and the round-trip contract the ZIP and
7z paths are tested against would have to be restated for it. Until that exists, `None` is the
honest capability answer rather than an untested `Partial`.

Note that even the two supported formats report `Partial`, and that downgrade from `Full` was
deliberate. It advertises the lossiness rather than hiding it behind a capability flag that
promises fidelity the rewrite cannot deliver.

## What survives the rewrite, and what is regenerated

Two levels have to be distinguished, and conflating them is the most common misunderstanding
of modify mode.

**Inside the archive**, the rewrite preserves entry content, entry paths, and — with
`preserve_metadata` on, which is the default — the modification, access, and creation times
plus Unix permissions of retained regular files. AD 0020 is what made those options actually
reach the commit; before it, `ModificationOptions` was a struct in the public surface that no
code read. ZIP rewrites additionally carry the archive-level comment and each retained entry's
stored-versus-deflated method, which the modify path obtains through a side-car read of the
source central directory because the libarchive-backed source reader does not surface them.

Regenerated or dropped, regardless of the flag: retained *directory* entries get the writer's
default permissions and the rewrite's wall-clock time (metadata-aware directory helpers are
tracked separately); symlink, hard link, and other special entries are dropped entirely; 7z
solid-block grouping and 7z-specific metadata are whatever the creation writer produces; and
every byte offset and the physical layout belong to the new writer. Entry order is not
preserved so much as reconstructed: retained entries are replayed in source-listing order, then
queued directory entries, then queued additions.

**The archive file itself** is a new inode. Only what the commit explicitly re-applies
survives, and the contract is deliberately narrow: the original's Unix permission bits, copied
onto the replacement before the rename. That single guarantee exists for a security reason —
without it a `0600` archive would resurface as `0644` under the process umask, and the same
copy runs on a requested backup for the same reason. It is an anti-widening rule, not a claim
of metadata faithfulness. Ownership, POSIX ACLs, extended attributes and security labels, and
the archive file's own timestamps are all lost; the replacement carries the rewrite's times.

That asymmetry is the honest summary of the whole design. A rewrite can be made faithful about
what is *inside* an archive, because the writer controls those bytes. It cannot be made
faithful about the file's own identity in the filesystem, because atomic replacement means a
different inode — and atomic replacement is the property that makes the commit safe to
interrupt.

## Related pages

- Doing it: [How to add, replace, or remove entries in an existing archive](../../../how-to/user/en/modify-a-zip-or-7z.md).
- Why one facade sits over several backends at all: [One API over many backends](../../../explanation/user/en/one-api-many-backends.md).
