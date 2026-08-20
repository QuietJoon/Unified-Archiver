---
type: How-To Guide
title: How to pick the right extraction call
description: Routes an extraction goal to the matching Archive method and shows the two selection choices that are easy to get wrong.
tags: [extraction, api, streaming]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: extraction, resource: src/extraction.rs }
  - { id: options, resource: src/options.rs }
  - { id: streaming, resource: src/streaming.rs }
  - { id: security, resource: src/security.rs }
  - { id: archive, resource: src/archive.rs }
  - { id: ad-0029, resource: docs/records/AD-0029-extraction-api-redesign-extract-some.md }
synced_hash: 49dac1f993f50da72911300f3a975f0bb32ab77571c1460973615dc17ad7abad
---

# How to pick the right extraction call

You have an open `Archive` and something you want out of it. Ten public extraction entry
points differ in what they select, where the bytes land, what they return, and how much of
the `ExtractionOptions` value you hand them is read. This page routes a goal to a call.

Preconditions: the handle is in read mode — from any of the `Archive::open*` read-mode
constructors (`Archive::open`, `Archive::open_encrypted`, `Archive::open_sfx`,
`Archive::open_with_sfx_progress`, `Archive::open_at_offset`) — and you have a destination
directory you are allowed to write to. Both fragments below belong in a function returning
`Result<(), ArchiveError>`.

## Route by goal

- Everything onto disk — `extract_all`.
- One known path onto disk — `extract_file`.
- A handful of known paths onto disk — `extract_files`.
- A selection that must survive duplicate paths — `extract_by_ids`.
- A selection expressed as a rule (extension, size, prefix, budget) — `extract_some`.
- The same, in code that already calls it — `extract_filtered` (an alias of `extract_some`).
- A small file as bytes in memory — `extract_to_memory`, or
  `extract_to_memory_with_options` when you need limits, a password, or CRC checking.
- A large file consumed incrementally — `extract_to_stream`, or
  `extract_to_stream_with_options` for the same three reasons. See
  [How to stream a large entry](../../../how-to/user/en/stream-a-large-entry.md) for the
  full streaming procedure.

Each call's signature, return type, and the subset of `ExtractionOptions` it actually reads
are listed in [Public API surface](../../../reference/user/en/public-api-surface.md) and
[Options and defaults](../../../reference/user/en/options-and-defaults.md). Consult them
before you assume a field takes effect: the memory and stream variants read only `limits`,
`password`, and `verify_crc32`, and `extract_file` has no warnings channel at all.

## Two rules that cut across every call

`ExtractionOptions` is not `Clone`, and the disk-writing calls take it **by value**. Build a
fresh options value for each call rather than trying to reuse one.

`verify_crc32: true` is rejected up front with `ArchiveError::Unsupported` on every
libarchive-backed format (TAR family, ISO, standalone compressed streams), because those
formats carry no per-entry CRC32. Set it only for ZIP, 7z, and RAR.

## Select entries that repeat a path: extract_by_ids

ZIP and 7z central directories are allowed to list the same path twice. `extract_file` and
`extract_files` reject such an archive with `OperationBlocked` rather than guess, and a
path-matching predicate handed to `extract_some` selects both entries and then fails the
overwrite preflight for mapping two entries onto one output path. Ids keep them apart.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.zip")?;
let ids: Vec<usize> = archive
    .list_files()?
    .iter()
    .filter(|entry| entry.is_file())
    .map(|entry| entry.id)
    .collect();

let result = archive.extract_by_ids(
    &ids,
    ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    },
)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

An id is the zero-based position of the entry in `list_files()`, and it is meaningless
outside the listing it came from. Read the listing, select, and extract with the same handle:
an out-of-range id is `OperationBlocked`, but a stale in-range id silently names a different
entry. Duplicate ids are collapsed, and an empty slice is a no-op success.

## Select entries by rule: extract_some

`extract_some` is the selective primitive the other selective calls delegate to (AD 0029):
one pass over the archive with one handle, whatever the size of the selection. The predicate
is `FnMut`, so it may carry state such as a running byte budget.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("sources.tar.gz")?;
let mut budget: u64 = 64 * 1024 * 1024;
let result = archive.extract_some(
    |entry| {
        let size = entry.size.unwrap_or(0);
        if !entry.path.ends_with(".rs") || size > budget {
            return false;
        }
        budget -= size;
        true
    },
    ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    },
)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

One safety caveat applies when the input is untrusted: the compression-ratio guard divides
the *selected* uncompressed size by the whole archive's on-disk size, so a small selection
out of a compressed container (TAR.GZ, TAR.BZ2, TAR.XZ) computes a more permissive ratio
than a true per-entry ratio would. Tighten `max_total_size` and `max_file_size` instead of
trusting the ratio; see
[How to extract an untrusted archive safely](../../../how-to/user/en/extract-untrusted-archives-safely.md).
An empty selection returns success with no warnings and does not create the destination.

## Where to look next

- Every field, default, and limit: [Options and defaults](../../../reference/user/en/options-and-defaults.md).
- Every signature and return type: [Public API surface](../../../reference/user/en/public-api-surface.md).
- What the warnings and error variants mean: [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).
- Which formats are backed by which engine, and what that costs:
  [One API over many backends](../../../explanation/user/en/one-api-many-backends.md).
- Progress reporting and cancellation, which only the bulk paths support:
  [How to report progress and cancel an operation](../../../how-to/user/en/report-progress-and-cancel.md).
