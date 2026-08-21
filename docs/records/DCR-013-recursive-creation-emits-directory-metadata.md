---
type: DCR
title: "Recursive creation emits every directory once, carrying source metadata, on both writers"
description: "add_directory_recursive emits an entry for every directory the walk observed — not only for empty leaves — carrying the mtime and unix mode of the source. Landed in two halves: the libarchive writers with the single-walk manifest (OI-0080-005), the ZIP writer here. Changes the entry count and the byte content of every recursively created archive, so a stored checksum over one must be recomputed."
tags: [change, project-control, DCR-013]
status: active
---

# DCR-013: Recursive creation emits every directory once, carrying source metadata, on both writers

- **Date:** 2026-08-21
- **Source:** Review 0001, R0001-0074 / OI-0001-010; TicGit `9cc3d714`
- **Related:** OI-0080-005 / AD-0057-adjacent single-walk manifest (`src/creation/manifest.rs`),
  AD-0044 (archive-internal path fidelity), DCR-010 (extraction rejects special entries)

## Why this needs a record at all

OI-0001-010's own framing is the reason: emitting every directory once, before its children, with
source metadata *"changes the entry count and byte content of every recursively created archive —
moving existing test expectations and invalidating consumers' stored checksums."* It also observed
that the accepted directory-metadata caveat already on record covers **modification** mode, not
creation, so creation had no covering decision. The change has now shipped in both halves. This
record is the covering decision, written after the fact rather than before it, which is stated here
rather than implied.

## What changed

**Before.** Each writer walked the source tree itself and emitted a directory entry only where the
walker reported `DirWalkEntry::is_leaf_dir`. Three consequences followed:

- a **non-empty** directory contributed no entry at all, so its mtime and mode were not merely
  defaulted — they were absent, and extraction recreated the directory with whatever the extractor
  chose;
- ZIP emitted its leaf directories through the writer's shared default `FileOptions`, so even the
  directories that *did* get an entry carried no source metadata;
- an empty source root went through a separate fallback that used backend defaults.

**Half one — the libarchive writers (landed with OI-0080-005).** `Manifest::write_into` emits every
recorded directory in walk order, so a directory always precedes its contents, and hands
`add_directory_entry_with_metadata` the mtime and unix mode the single walk observed. The empty-root
fallback is gone: an empty source directory yields exactly one entry without a special case, because
the root is itself a recorded entry. Namespace suppression still fires exactly once per entry.

**Half two — the ZIP writer (this change).** The manifest already carried `mtime` and `mode` and
already handed both to the libarchive arm. The ZIP arm called `add_directory_entry(&archive_path)`,
which takes a path and nothing else, so both were dropped on the floor. `ZipWriter` now has
`add_directory_entry_with_metadata(archive_path, mtime, mode)` mirroring its libarchive counterpart,
the manifest's ZIP arm calls it, and the metadata-free `add_directory_entry` delegates to it with
`None, None`.

## Observable behaviour change

Two distinct effects, worth separating because a consumer can be exposed to one and not the other.

**Entry count.** Every non-empty directory in a recursively added tree now contributes an entry where
it contributed none. A test or a consumer that counted entries in a recursively created archive sees
a larger number.

**Byte content, ZIP specifically.** A ZIP directory record now carries a real DOS timestamp and, on
Unix, external attributes holding the source mode. Before this change it carried the default: the
DOS epoch. That is not a guess — reverting the manifest's ZIP arm makes the new round-trip test fail
with `stored SystemTime { tv_sec: 315532800 }`, which is 1980-01-01, against a source stamp of
`tv_sec: 1787310585`. So the old directory records were not approximately right; they were the
epoch.

**Migration.** A checksum stored over a recursively created archive will not match a freshly created
one and must be recomputed. Archives created entry-by-entry through `add_file_from_data` or
`add_file_from_path` are unaffected, because nothing about the file path changed.

## What deliberately did *not* change

`Archive::add_directory(path)` — the single-directory facade call — still emits a metadata-free
entry, and that is by construction rather than by omission. Its caller names an *archive* path; there
is no filesystem entry behind it to read an mtime or a mode from. Giving it metadata would mean
inventing some, which is the opposite of what this record is for. `ZipWriter::add_directory_entry`
therefore keeps its signature and delegates with `None, None`.

## Precision is a format property, not test slack

The round-trip assertions use different tolerances per format and the difference is not laxity:

| format | stored mtime resolution | tolerance in the test |
|---|---|---|
| tar | whole seconds | 1s |
| ZIP | DOS timestamp, **2-second** granularity | 2s |

Demanding bit equality with the filesystem stamp would fail against a correct writer in both cases.
A future reader tightening the ZIP tolerance to 1s will get an intermittent failure that looks like a
race and is not one.

The unix mode is applied only under `#[cfg(unix)]`, for two reasons that happen to agree: the
manifest records `mode: None` off Unix, so there is nothing to apply; and ZIP external attributes are
only meaningful with a Unix host indicator, so writing a mode from a non-Unix host would be
fabricating a claim the format cannot honour.

## Evidence

- `test_add_directory_recursive_preserves_nonempty_directory_metadata` (tar) and
  `test_add_directory_recursive_preserves_nonempty_zip_directory_metadata` (ZIP) share one helper,
  `assert_nonempty_directory_metadata_survives(format, extension, mtime_tolerance_secs)`, so the two
  formats cannot drift apart in what they assert.
- `test_add_directory_recursive_emits_nonleaf_zip_directory` continues to pin that the *entry*
  exists; the new test covers what it carries. The two are complementary, not redundant.
- **Non-vacuity, by revert-and-observe.** Reverting the manifest's ZIP arm to the metadata-dropping
  `add_directory_entry` call makes the ZIP test fail on the DOS-epoch mtime quoted above, while the
  tar test keeps passing — so the revert isolated the ZIP arm, and the new test is measuring the
  thing this record claims. `src/creation/manifest.rs` was then restored and compared
  byte-for-byte against the pre-revert copy.

## Known risks

1. **A consumer with stored checksums over recursively created archives is silently affected.**
   Nothing in the API signals this — the change is in the bytes, not the types — so the only warning
   is `CHANGELOG.md`. This is the same exposure class DCR-012 recorded for the content digest, and it
   has the same mitigation: the consumer must recompute.
2. **The two halves shipped in different change sets.** An archive created between them has
   libarchive directory metadata and ZIP directory records at the DOS epoch. There is no way to tell
   from such an archive which half was in effect, so a bisect over archive bytes cannot distinguish
   "created before half two" from "created by a writer that lost the metadata".
3. **The superseded leaf-only emit is still in the tree, with no caller.** Each writer keeps its own
   `add_directory_recursive` — `ZipWriter::add_directory_recursive` and the libarchive writer's
   equivalent — and both still branch on `DirWalkEntry::is_leaf_dir`, which is the exact strategy
   this record replaces. Nothing in the crate calls either one: `Archive::add_directory_recursive`
   goes through `SourceManifest::build` and `Manifest::write_into`, and the only other apparent
   call site is a `WriteArchive` test that routes through that same facade. `is_leaf_dir` survives
   in `src/ffi/common.rs` to serve those two methods and nothing else.

   They are not flagged as dead code because they are `pub` inside `#[doc(hidden)] pub mod ffi`, so
   the compiler sees a reachable public item and a downstream crate could in principle call one and
   get the old behaviour. The risk is a future reader treating them as the live implementation, or
   copying from them — they are the *before* half of this record, left compiling. Removing them, or
   pointing them at the manifest, is not attempted here: it is a public-surface change on a hidden
   module and belongs with the AD 0058 / OI-0058-001 work that decides what `ffi` exposes at all.
