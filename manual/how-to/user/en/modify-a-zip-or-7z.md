---
type: How-To Guide
title: How to add, replace, or remove entries in an existing archive
description: Queue additions, replacements, and removals against an existing ZIP or 7z archive and commit them as one atomic rewrite.
tags: [modification, api, config]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: modification-src, resource: src/modification.rs }
  - { id: options-src, resource: src/options.rs }
  - { id: creation-src, resource: src/creation.rs }
  - { id: security-src, resource: src/security.rs }
  - { id: ad-0020, resource: docs/records/AD-0020-additive-modification-options-extension.md }
  - { id: madr-0009, resource: docs/records/MADR-0009-r052-advisory-file-locking-modify-mode.md }
  - { id: modify-example, resource: examples/modify_archive.rs }
synced_hash: 69aa0dd754b1cd29992c571f09342af7a64f98c67ffa3f05df9a4536a796a26b
---

# How to add, replace, or remove entries in an existing archive

Modification is a session: open the archive with `Archive::modify`, queue mutations against
the handle, then commit. Nothing touches the archive on disk until `commit_changes` runs, and
the commit is a full rewrite followed by an atomic replace — see
[Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md)
for the reasoning behind that shape.

## Preconditions

- **Format is ZIP or 7z.** Those are the only two formats where
  `ArchiveFormat::can_modify` is true. `Archive::modify` on a TAR variant, ISO, standalone
  compressed stream, or RAR fails with `OperationBlocked` and a reason naming the format.
- **The archive is not encrypted.** `Archive::modify` probes for encryption before it binds a
  backend and refuses encrypted archives with
  `OperationBlocked`: "Encrypted archives cannot be modified". There is no password parameter
  on this path.
- **The archive exists and is writable.** The session opens the file read-write to take its
  lock; a missing or unwritable path fails as an `Io` error labelled `modify-lock-open`.
- **No other modify session holds the file** (see [Concurrent sessions](#concurrent-sessions)).
- **Room for a second copy.** The commit writes a complete new archive beside the original
  before replacing it, plus another full copy if you asked for a backup.

## Open a session

```rust
use unified_archive::Archive;

let mut archive = Archive::modify("release.zip")?;
```

The handle is a read handle as well: `list_files`, `find_entry`, `find_entries`,
`entry_count`, and `is_solid` all work on it. They describe the archive **as it is on disk** —
the entry listing is a snapshot taken on first use and queued mutations never appear in it.

Dropping the handle without committing leaves the archive untouched. Calling `finish` or
`close` on a modify handle that still has queued operations is refused with
`OperationBlocked` rather than silently discarding them; a handle with an empty queue closes
quietly.

## Open with options

`ModificationOptions` carries the session's opt-in behaviour and is passed at open time, not
at commit time (AD 0020 chose the sibling constructor over changing `modify`'s signature).

```rust
use unified_archive::{Archive, ModificationOptions};

let options = ModificationOptions::new().with_backup(".bak");
let mut archive = Archive::modify_with_options("release.zip", options)?;
```

Defaults from `ModificationOptions::new` (identical to `Default`): `preserve_metadata: true`,
`create_backup: false`, `backup_suffix: ".bak"`, `compression: None`.

- **`with_backup(suffix)`** turns backup on and sets the suffix. The backup is always the
  sibling file `<archive path><suffix>`; a suffix without a leading dot gets one added. An
  empty suffix, or one containing `/` or `\`, is discarded and `.bak` is used instead — in
  debug builds a `debug_assert` fires, in release builds the substitution is silent, and the
  same sanitising runs again at commit in case you assigned the public field directly.
- **`without_metadata_preservation()`** clears `preserve_metadata`.
- **`compression`** is a public field with no builder. Assign a `CompressionOptions` whose
  `format` equals the archive's own format; `level` and `progress` are honoured by the
  rewrite. A `format` that disagrees is rejected at commit — the rewrite cannot change the
  container. `password` and `split_size` are rejected by the same create-time gate the
  `Archive::create` facade uses.
- `ModificationOptions` is not `Clone`, because `compression` may hold a progress callback.
  Build a fresh value per session.

### What metadata preservation actually restores

With `preserve_metadata: true`, retained **regular-file** entries are re-emitted with the
modification, access, and creation times the source listing exposed, plus their Unix
permission bits. The ZIP writer records access and creation times in a `0x5455` extended
timestamp extra field; the libarchive writer (used for 7z) sets them on the entry directly.
Entries queued with `add_entry_from_path` pick up their times and permissions from the
source file on disk.

Two things fall back to backend defaults regardless of the flag:

- **Retained directory entries** get the writer's default permissions (`0o755` on the
  libarchive path) and the rewrite's wall-clock time.
- **Entries queued from bytes or from a reader** have no source metadata to carry, so the
  writer's defaults apply.

The archive *file* itself is a separate matter — see [What the commit does not carry
across](#what-the-commit-does-not-carry-across).

## Queue the changes

Every one of these calls only records an intent. They validate the archive-internal path
immediately and return `InvalidPath` for an empty name, a name containing NUL, a leading
`./`, a `..` segment anywhere, or an absolute or drive-prefixed path (backslashes are
normalised to forward slashes before that check, so `..\..\evil` is rejected on Unix too).
The check reads the name as `std::path::Components`, which normalizes away every `.` except
a leading one, so `a/./b` passes and is stored as written.

Additions:

- `add_entry(path, data)` — snapshots the bytes into the queue. You may drop or mutate your
  buffer immediately.
- `add_entry_from_path(archive_path, fs_path)` — records the filesystem path only. Commit
  opens it then and archives whatever it names at that moment, so a same-path replacement
  written before the commit is picked up silently. This is the call to use for large files:
  the payload never buffers in memory.
- `add_entry_from_reader(archive_path, reader, size)` — takes an owned
  `Read + Send + 'static` source, so a reader borrowing from a local buffer will not compile.
  With `Some(n)` the length is declared up front and a reader that delivers a different
  number of bytes is rejected at commit. With `None` the reader is drained into a temporary
  file first to learn its length; that drain is capped at the default single-file extraction
  limit (1 GiB) and a source exceeding it fails with `OperationBlocked`.
- `add_directory_entry(path)` — queues a directory entry with no contents. Duplicate
  directory paths are collapsed to one emitted record.

Removals:

- `remove_entry(path)` resolves the path against the source listing and marks **every**
  matching entry, returning how many matched. ZIP and 7z both permit duplicate paths, so this
  can be more than one. A path that matches nothing returns `Ok(0)` and is not an error —
  check the count if you need strict removal.
- `remove_entry_by_id(id)` marks exactly one entry by its positional index in `list_files()`
  order. That is the precise tool when duplicate paths exist. An out-of-range id is
  `OperationBlocked`.

Replacements are the composition of the two, with the same argument shapes as the adds:
`replace_entry(path, data)`, `replace_entry_from_path(path, fs_path)`, and
`replace_entry_from_reader(path, reader, size)`. Each removes every source entry at that path
and queues one new entry. If the path was not in the source archive the removal matches
nothing and the add still happens — the result is an ordinary add, not an error. Call
`remove_entry` yourself and inspect the count if you need replace-or-fail.

Note that removals only ever target *source* entries. Queueing an add and then removing the
same path in one session does not cancel the add.

## Inspect and discard the queue

```rust
let queued = archive.pending_operations()?;   // adds + removals + directory entries
if queued > 0 {
    archive.clear_operations()?;              // drop all three queues
}
```

`clear_operations` is the only way to abandon a queued reader source without materialising
it, and it must be used *before* the commit — `commit_changes` takes the handle by value, so
after a failed commit there is nothing left to clear.

Both calls return `OperationBlocked` on a handle that is not in modify mode.

## Commit

```rust
archive.commit_changes()?;
```

`commit_changes` consumes the handle. In order, it:

1. Re-runs the pre-write gates: the compression-override checks, a re-validation of every
   retained entry's path, a namespace check that rejects duplicate output paths and
   file-versus-directory collisions (`a` against `a/b`), and for ZIP a cross-check of the
   source listing against the central directory.
2. Creates a fresh archive at `<archive path>.tmp.<pid>.<nanos>.<counter>` in the same
   directory, in the same format.
3. Copies every retained entry into it, then the queued directory entries, then the queued
   additions.
4. For ZIP, carries the archive-level comment and each retained entry's stored/deflated
   compression method across.
5. Copies the original archive's Unix permission bits onto the temporary file so the swap
   cannot widen access.
6. If a backup was requested, copies the original aside. The backup target is claimed
   exclusively, so an existing `<path>.bak` fails the commit with "backup target already
   exists" instead of being overwritten; the copy is flushed to disk before the swap.
7. Renames the temporary file over the original, then best-effort syncs the parent directory.

An empty queue short-circuits: `commit_changes` returns `Ok(())` without rewriting anything
and without writing a backup.

On any failure the temporary file is removed and the original archive is left exactly as it
was — but the handle is gone, including for recoverable rejections such as a duplicate output
path or a bad backup suffix. To retry, open a fresh `Archive::modify` session and re-queue;
the previous session's lock was released when its handle dropped. If you want to test a queue
without risking the handle, the `v2-api` feature's `ModifyArchive` exposes
`try_commit_changes`, which runs the same gates against `&mut self`. Treat its success as
snapshot-time only: it leases nothing, so the real commit re-runs every gate.

## Concurrent sessions

`Archive::modify` takes an exclusive advisory lock on the archive before it does anything
else — before format detection, before the capability check, before the encryption probe. A
second `Archive::modify` on the same path while the first handle is alive fails immediately:

```text
OperationBlocked { operation: "modify", reason: "Another Modify session holds the advisory lock on release.zip" }
```

That failure is the intended behaviour, not a limitation to work around. Two concurrent
rewrite-and-replace sessions on one file would each build a complete new archive from the
same source and then race to rename over it; the loser's changes would vanish with no error
anywhere. Failing the second `modify` call converts a silent lost update into an error you
can handle, and the non-blocking acquisition means the caller decides whether to wait, skip,
or report — there is no deadlock risk (MADR-0009). The lock releases when the handle drops,
so a retry loop around `modify` is a reasonable pattern.

The lock is **advisory**: it coordinates processes that use this library. Any other process
writing the same path is unaffected. The session additionally pins the archive's inode
identity at open and re-checks it before each step that resolves the path again, so a
non-cooperating writer that swaps the pathname is caught rather than followed — the
[explanation page](../../../explanation/developer/en/modification-is-a-rewrite.md) covers
what that does and does not guarantee.

## What the commit does not carry across

Because the rewrite installs a **new inode**, only what the commit explicitly re-applies
survives on the archive file. That is the Unix permission bits and nothing else. Ownership
(uid/gid), POSIX ACLs, extended attributes and security labels, and the archive file's own
timestamps are all lost — the replacement carries the rewrite's wall-clock times. A backup
created with `with_backup` follows the same contract. Re-apply anything your workflow depends
on after the commit returns.

Inside the archive, the rewrite is deliberately lossy in two ways:

- **Symlink, hard link, and other special entries in the source are dropped.** They are not
  copied into the new container and no error is raised; the resulting archive simply has
  fewer entries. Because such an entry claims no name in the output, you may legitimately add
  a regular file at a dropped link's path in the same session.
- **7z solid-block layout is not reproduced.** The new archive is written by the ordinary
  creation writer, so block grouping and 7z-specific metadata are regenerated. Both formats
  report `Support::Partial` for modification precisely to advertise this.

## Other boundaries

- **A filesystem symlink cannot be queued as content.** `add_entry_from_path` accepts the
  path without complaint, but at commit the writer refuses to archive a symlink source with
  `OperationBlocked`. Resolve the link yourself and pass the target.
- **No re-encryption.** Encrypted archives are rejected at `modify` time, so the question of
  preserving encryption across a rewrite never arises. There is no way to produce an
  encrypted archive from a modify session.
- **The container format is fixed.** A compression override whose `format` differs from the
  archive's is refused at commit rather than silently writing a different container over the
  path.
- **No crash-recovery journal.** Durability rests on the temporary file plus atomic rename. A
  crash mid-rewrite leaves the original intact and a recognisable `.tmp.<pid>.…` file beside
  it, which nothing cleans up for you.

`examples/modify_archive.rs` is a runnable end-to-end version of the add, remove, and replace
flows.

## Related pages

- Every option, field, and default: [Options and defaults](../../../reference/user/en/options-and-defaults.md).
- Which error you will actually see: [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).
- Why the commit is a rewrite at all: [Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md).
