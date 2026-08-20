---
type: How-To Guide
title: How to extract a multi-part RAR set
description: Opens the right volume of a split RAR set, confirms the set with detect_multipart, and extracts it in one call.
tags: [extraction, formats, archive]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: inspection, resource: src/inspection.rs }
  - { id: format, resource: src/format.rs }
  - { id: options, resource: src/options.rs }
  - { id: creation, resource: src/creation.rs }
  - { id: unrar-wrapper, resource: src/ffi/wrapper.rs }
  - { id: limitations, resource: Limitations.md }
synced_hash: ddb894304a05a2a7e77f7a9ed41fe592cfb2f550d8450f055388f0319a65709f
---

# How to extract a multi-part RAR set

RAR and RAR5 are the only formats whose split volumes this crate reads end to end. You open
one volume, and the UnRAR backend walks the rest of the set itself — you never hand it the
volume list.

Preconditions: the default `rar-support` Cargo feature is enabled (without it, opening a
RAR fails with `ArchiveError::Unsupported` naming the feature), and every volume of the set
sits in one directory under its original name and stays readable for the whole operation.

## 1. Open the first volume

Which file is "first" depends on the naming scheme WinRAR used:

- New-style volumes (`archive.part1.rar`, `archive.part2.rar`, …): open `archive.part1.rar`.
- Old-style volumes (`archive.rar` plus `archive.r00`, `archive.r01`, …): open
  `archive.rar`.

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.part1.rar")?;
println!("{:?}", archive.format());
```

Nothing in the library checks that the file you opened is the first volume of the set. The
UnRAR SDK reports that fact through an archive flag, and the crate does not consult it, so
opening a middle volume produces no diagnostic — you simply see whichever entries that
volume's headers describe. Pick the first volume yourself.

## 2. Confirm the set and see the volumes

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.part1.rar")?;
let (is_multipart, parts) = archive.detect_multipart()?;
if is_multipart {
    println!("{} volumes:", parts.len());
    for part in &parts {
        println!("  {}", part.display());
    }
}
```

`detect_multipart` returns `(bool, Vec<PathBuf>)`. The vector is sorted in volume order —
the unnumbered main volume first, then numbered volumes ascending — and always contains the
archive you opened, so a single-volume archive comes back as `(false, [that one path])`
rather than an empty list.

Two properties matter when you use this as a preflight. Volumes are matched **by file name
only**; `detect_multipart` never opens a sibling, so a listed volume is not a validated
volume. And the directory is re-scanned on every call, so a volume that appears (or
disappears) after you opened the handle is reflected the next time you ask.

The call fails rather than guessing when it cannot read the directory: an I/O error while
scanning propagates as `ArchiveError::Io`, and calling it on a write-mode handle from
`Archive::create` is rejected outright.

## 3. Prefer the typed layout in new code

```rust
use unified_archive::{Archive, MultipartLayout};

let archive = Archive::open("backup.part1.rar")?;
match archive.multipart_layout()? {
    MultipartLayout::Single { path } => {
        println!("single volume: {}", path.display());
    }
    MultipartLayout::Multi { parts } => {
        println!("{} volumes, first is {}", parts.len(), parts[0].display());
    }
}
```

`MultipartLayout` has exactly two variants. `Single { path }` carries the archive you
opened. `Multi { parts }` carries every detected volume in volume order and is never empty.
The point of the enum is that a caller cannot forget to check the boolean and mistake a
single-volume archive for a one-volume set. `detect_multipart` remains available and
undeprecated in 0.3; version 0.4 will deprecate it formally.

## 4. Which RAR names group as one set

Grouping is deliberately narrow, so an unrelated sibling that merely shares a prefix is not
swept into the set. Matching is ASCII-case-insensitive, and the returned paths keep their
original case. Two shapes matter for picking the volume you open:

- **New-style** — `<base>.part<digits>.rar`, grouped only when the file you opened is itself
  a `.part<digits>.rar` volume with the same base. The suffix is parsed from the *end* of the
  name, so `my.part9.data.part1.rar` groups with `my.part9.data.part2.rar` and not with
  something whose base is `my`; `archive.part.rar` has no digit run and is not a volume.
- **Old-style** — `<stem>.rar` plus `<stem>.r<NN>` and `<stem>.s<NN>` with **exactly two**
  digits, grouped only when the file you opened ends in `.rar`. WinRAR rolls `.r99` over into
  `.s00`, and the sort orders the whole s-series after the r-series. `archive.r1`,
  `archive.r123`, and `archive.rab` are not volumes.

The same sibling scan also recognises ZIP `.zip` + `.zNN` sets and numeric `<stem>.<digits>`
splits, neither of which this recipe applies to — see the hard boundary below and
[Format support matrix](../../../reference/user/en/format-support-matrix.md) for what each
format's multipart capability actually promises.

## 5. Extract

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.part1.rar")?;
let result = archive.extract_all(ExtractionOptions {
    destination: PathBuf::from("out"),
    ..Default::default()
})?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

That is the whole extraction. UnRAR continues into `backup.part2.rar` and onwards on its
own, following the naming convention of the volume you opened; the `parts` list from step 2
is for your reporting and preflight, not an input to extraction. The selective calls
(`extract_some`, `extract_files`, `extract_by_ids`) work the same way on a volume set — see
[How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md).

Note that the entry ids and the listing come from a walk across the whole set, so a
selection made from `list_files()` may pull data from several volumes; that is transparent
to you.

## 6. When a volume is missing

Check for the gap yourself before extracting: because `parts` is derived from file names, a
missing volume shows up as a short or non-contiguous list. That check is cheap and gives you
a message naming the volume you expected.

If you extract anyway, the failure comes from UnRAR trying to open the next volume by name.
It surfaces as `ArchiveError::Io` with the operation `open` — deliberately not a
`NotFound`-kinded error, because the same UnRAR code covers permission failures and encoding
problems too. The path it names is **the archive you opened**, not the volume that is
missing, so include the volume list from step 2 in your own error message if your users need
to know which file to fetch. Entries already written to the destination stay on disk; there
is no rollback.

Do not delete, move, or unmount volumes while an extraction is running. There is no
recovery path for a volume that becomes unavailable mid-operation, and the failure is not
guaranteed to be a clean error.

## The hard boundary

- **ZIP split volumes are not supported end to end.** `detect_multipart` groups `.zip` +
  `.zNN` sets by name because the naming heuristic is shared, and `ArchiveFormat::Zip`
  reports its multipart read capability as `Support::Partial` for that reason — but no code
  path reads entry data across ZIP segments. Treat a detected ZIP set as information only.
- **7z numeric volumes are not supported, and are not even detected.**
  `ArchiveFormat::SevenZip` reports no multipart capability at all, so `detect_multipart`
  returns before it scans the directory: if a `.7z.001` volume opens at all, its layout is
  reported as `Single` however many siblings sit next to it.
- **Split creation does not exist.** `CompressionOptions` still has a `split_size` field,
  but `Archive::create` runs `CompressionOptions::validate_for_format`, which rejects any
  `Some(_)` value with `ArchiveError::OperationBlocked` for every format. Leave it `None`;
  the field is reserved, not functional. The typed builders
  (`ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions`) do
  not expose it at all, which is the safer way to construct creation options —
  [How to create an archive in a given format](../../../how-to/user/en/create-an-archive.md).
- RAR creation is out of scope for `Archive::create` in any case, split or not.

## Where to look next

- Per-format capabilities, including the multipart column:
  [Format support matrix](../../../reference/user/en/format-support-matrix.md).
- The `detect_multipart`, `multipart_layout`, and `MultipartLayout` signatures:
  [Public API surface](../../../reference/user/en/public-api-surface.md).
- What the warnings from `extract_all` mean:
  [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).
- Why one facade behaves differently per backend:
  [One API over many backends](../../../explanation/user/en/one-api-many-backends.md).
