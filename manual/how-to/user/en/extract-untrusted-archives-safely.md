---
type: How-To Guide
title: How to extract an untrusted archive safely
description: Configure extraction limits, an overwrite policy, and checksum verification before extracting an archive from an untrusted source, then inspect the warnings the run returns.
tags: [security, extraction, config]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: security-rs, resource: src/security.rs }
  - { id: options-rs, resource: src/options.rs }
  - { id: extraction-rs, resource: src/extraction.rs }
  - { id: error-rs, resource: src/error.rs }
  - { id: security-md, resource: SECURITY.md }
  - { id: user-manual, resource: docs/USER_MANUAL.md }
synced_hash: e2492ef306cd56c1286decd29c0574287729df90ed88cc486ff5d121b7a89287
---

# How to extract an untrusted archive safely

You have an archive that arrived from somewhere you do not control — an upload, a
download, a mail attachment — and you want its contents on disk without letting it
choose where those contents land or how much of your disk they consume.

The default `ExtractionOptions` already applies resource ceilings and path
sanitisation, so a plain `extract_all` is not unguarded. What this recipe adds is
ceilings sized for *your* workload rather than the shipped ones, a destination
that contains the blast radius, and an explicit decision about checksum
verification.

## Preconditions

- The archive is opened read-only through `Archive::open` (or
  `Archive::open_encrypted` if it is password-protected).
- You have a destination directory you are willing to have written into and,
  on failure, to delete wholesale. Do not point extraction at a tree that already
  holds files you care about.
- You know roughly how large the legitimate content should be. Every ceiling
  below is a number you choose; the crate's defaults are a fallback, not a
  measurement of your case.

## 1. Build the limits

`ExtractionLimits` has no public fields. Start from
`ExtractionLimits::builder()`, which begins at the shipped defaults, and override
only the ceilings you want to change.

```rust
use unified_archive::{Cap, CompressionRatio, ExtractionLimits};

let limits = ExtractionLimits::builder()
    // Cumulative uncompressed bytes across every extracted entry.
    .max_total_size(2u64 * 1024 * 1024 * 1024)
    // Uncompressed bytes for any single entry.
    .max_file_size(128u64 * 1024 * 1024)
    // Number of entries in the archive.
    .max_entry_count(20_000u64)
    // Zip-bomb gate, as an exact rational.
    .max_compression_ratio(CompressionRatio::whole(200)?)
    // Ceiling on an SFX payload staged to a temporary file.
    .max_sfx_payload_size(Cap::Limited(1024 * 1024 * 1024))
    // Opt in to strict path rejection (see the caveat below).
    .reject_unsafe_paths(true)
    .build();
```

The `?` is real: `CompressionRatio::whole` is fallible, so this fragment belongs in
a function that returns `Result<_, ArchiveError>`.

Notes on the individual setters:

- `max_total_size`, `max_file_size`, `max_entry_count` and
  `max_sfx_payload_size` all take `impl Into<Cap>`, so a bare `u64` works and
  becomes `Cap::Limited`.
- `max_compression_ratio` takes a `CompressionRatio`, not a number.
  `CompressionRatio::whole(n)` builds `n : 1`; `CompressionRatio::new(num, den)`
  builds an arbitrary rational. Both return `Result` and reject a zero numerator
  or denominator with `ArchiveError::OperationBlocked`, so the `?` above is
  required.
- `reject_unsafe_paths(true)` records your intent but **does not change
  behaviour yet**. The flag has a typed home on `ExtractionLimits` and defaults
  to `false`; the strict-reject code path is still deferred, so unsafe path
  components are repaired rather than rejected either way. Setting it is
  harmless and future-proof, not protection you can rely on today.
- `max_sfx_payload_size` is likewise recorded but not yet consumed. Its default
  is 16 GiB, and every SFX staging path — `Archive::open_sfx`,
  `Archive::open_with_sfx_progress`, `Archive::open_at_offset`, and
  `Archive::open_encrypted` on an SFX — reads that built-in constant directly,
  because none of those entry points takes a limits argument.

Every ceiling has an explicit opt-out. `Cap::Unlimited` removes one ceiling
(`.max_total_size(Cap::Unlimited)`), and
`ExtractionLimitsBuilder::unlimited_compression_ratio()` disables the ratio gate
entirely. There is no public constructor that turns everything off at once — the
all-ceilings-disabled preset is crate-internal. Treat `Cap::Unlimited` as what it
is: a statement that you have another bound in place, not a convenience.

## 2. Choose an overwrite policy

`overwrite` defaults to `false`, and false is the right answer for untrusted
input: the preflight refuses to start when any entry's output path already
exists, so a hostile archive cannot quietly replace a file you already have.

`overwrite: true` replaces existing *files* only. Two cases are rejected
regardless of the flag, because "overwrite" means replacing files and never
deleting a tree:

- a file entry whose output path already exists as a directory;
- a directory entry whose output path already exists as a non-directory.

## 3. Decide about `verify_crc32`

`verify_crc32` is `false` by default and its meaning depends on the backend that
serves your format:

| Format family | Effect of `verify_crc32` |
|---|---|
| ZIP | Honoured. Per-entry CRC32 is verified during extraction. |
| 7z | CRC32 always validates; the flag changes nothing. |
| RAR / RAR5 | CRC32 always validates; the flag changes nothing. |
| Libarchive-backed (TAR family, ISO, standalone compressed streams) | `true` returns `ArchiveError::Unsupported` before any I/O. The wrapping codec's own integrity check still runs. |

So: set `verify_crc32: true` when you are extracting ZIP and want the check;
leave it `false` for the libarchive-backed formats, where `true` is a hard error
rather than a no-op. If you do not know the format up front, either branch on
`archive.format()` or leave the flag `false` and verify separately. A mismatch
surfaces as `ArchiveError::Corruption { path, details }`.

For what a CRC32 match does and does not prove, see
[What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md);
for a verification pass that works across formats, see
[How to verify an archive's integrity](verify-archive-integrity.md).

## 4. Extract into a dedicated destination

Point `destination` at a directory used for nothing else. Extraction creates it
if it is missing, resolves it once, and rejects any entry whose resolved output
path would leave it.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let destination = PathBuf::from("/var/tmp/incoming-42");

let options = ExtractionOptions {
    destination: destination.clone(),
    overwrite: false,
    verify_crc32: true,   // ZIP source in this example
    limits: limits.clone(),
    ..Default::default()
};

let archive = Archive::open("incoming/payload.zip")?;
let result = archive.extract_all(options)?;
```

`ExtractionOptions` holds boxed callbacks and is not `Clone`, and every
extraction call takes it by value, so each call needs its own value. The clones
above keep `destination` and `limits` (both `Clone`) available for the next one.

Two ordering details worth knowing when you clean up after a failure: the
resource-limit gate runs before the destination directory is created, so a
rejected bomb leaves nothing behind, while an overwrite-conflict rejection can
leave an empty destination directory in place. Deleting the destination tree on
any error is the simplest policy.

If you are extracting a subset rather than everything, prefer
`extract_by_ids` — and note that for the libarchive-backed compressed formats
(TAR.GZ, TAR.BZ2, TAR.XZ) the archive-level ratio gate uses the whole archive's
on-disk size as its denominator, so a small selection makes that gate more
permissive. Tighten `max_total_size` and `max_file_size` instead of relying on
the ratio there.
[How to pick the right extraction call](choose-an-extraction-api.md) covers the
choice between the entry points. One further trap: the bare
`Archive::extract_to_memory` always applies the *default* limits and ignores
yours. Use `extract_to_memory_with_options` (or
`extract_to_stream_with_options`) whenever the source is untrusted.

## 5. Inspect the warnings

`extract_all`, `extract_some`, `extract_files` and `extract_by_ids` return
`ResultWithWarnings<()>`. A returned `Ok` with a non-empty `warnings` vector means
the run completed *and* something was not done as the archive asked. Symbolic
links and hard links are skipped rather than recreated, and that is reported
here — not as an error.

```rust
use unified_archive::error::ArchiveWarning;

for warning in &result.warnings {
    match warning {
        ArchiveWarning::SkippedSymlink { path, target } => {
            eprintln!("symlink not created: {path} -> {target:?}");
        }
        ArchiveWarning::SkippedHardLink { path } => {
            eprintln!("hard link not created: {path}");
        }
        ArchiveWarning::OutputPathCaseCollision { first, second } => {
            eprintln!("case collision: {first} vs {second}");
        }
        other => eprintln!("{other}"),
    }
}
```

`ArchiveWarning` is `#[non_exhaustive]`, so keep the wildcard arm. Every variant
also implements `Display`, which is enough if you only want to log.

If the presence of links should change your decision *before* anything is
written, call `Archive::check_symlinks()` first — it scans the listing and
returns the link warnings (`SkippedSymlink`, `SkippedHardLink`) without
extracting. It is a strict subset: `OutputPathCaseCollision` is produced by the
overwrite-conflict preflight, so it only appears on a real extraction.

## 6. Match on the rejection

Every gate in this recipe refuses by returning an error, so a caller can
distinguish a hostile archive from a broken one:

```rust
use unified_archive::{ArchiveError, ExtractionOptions, Operation};

// The call in section 4 consumed its options, so build a fresh value here.
let options = ExtractionOptions {
    destination,
    verify_crc32: true,
    limits,
    ..Default::default()
};

match archive.extract_all(options) {
    Ok(result) => { /* ... inspect result.warnings ... */ }

    // Limit breached, or a namespace/overwrite conflict in the preflight.
    Err(ArchiveError::OperationBlocked { operation, reason }) => {
        if operation == Operation::ExtractAll.to_string() {
            eprintln!("extract_all refused: {reason}");
        } else {
            eprintln!("refused during {operation}: {reason}");
        }
    }

    // The entry's resolved output path was outside the destination, or the
    // destination path is itself a symlink.
    Err(ArchiveError::InvalidPath { path, reason }) => {
        eprintln!("rejected path {path}: {reason}");
    }

    // verify_crc32 = true against a backend that cannot honour it.
    Err(ArchiveError::Unsupported { operation, format, details }) => {
        eprintln!("{operation} unsupported for {format:?}: {details:?}");
    }

    // A CRC32 that did not match.
    Err(ArchiveError::Corruption { path, details }) => {
        eprintln!("corrupt entry {path}: {details}");
    }

    Err(other) => eprintln!("{other}"),
}
```

The `operation` field is a `String` carrying the entry point's stable label.
Compare it against `Operation::ExtractAll.to_string()` and friends, as the first
arm does, rather than against a hand-written literal; `Operation` is also
`#[non_exhaustive]`. `ArchiveError` is `#[non_exhaustive]` too, so the wildcard
arm is mandatory.

Which limit fired is in `reason`, as prose: too many entries, a file over
`max_file_size`, a cumulative total over `max_total_size`, or a compression ratio
over the bound. One shape is worth calling out because it looks like a false
positive and is not: an entry that reports zero compressed bytes but a non-zero
uncompressed size is treated as a ratio violation, because a well-formed archive
only reports zero compressed bytes for an empty payload.

## Where to go next

- Every field, its type and its shipped default:
  [Options and defaults](../../../reference/user/en/options-and-defaults.md).
- The exact error and warning shapes:
  [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).
- Why these gates exist, where they run, and what they deliberately do not
  cover: [The extraction safety model](../../../explanation/user/en/extraction-safety-model.md).
