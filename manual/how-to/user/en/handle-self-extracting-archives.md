---
type: How-To Guide
title: How to detect and open a self-extracting archive
description: Detect an SFX executable, read the detection verdict, open or stage its embedded payload, and pull the stub bytes out for inspection.
tags: [sfx, archive, extraction]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: sfx-result, resource: src/sfx/result.rs }
  - { id: sfx-detection, resource: src/sfx/detection.rs }
  - { id: sfx-stub-types, resource: src/sfx/stub_types.rs }
  - { id: sfx-limits, resource: src/sfx/limits.rs }
  - { id: archive-facade, resource: src/archive.rs }
  - { id: options, resource: src/options.rs }
synced_hash: f8a94f11627ca16cf4ae59295088be6a406d8f42eafba3c320fd34d3e097334e
---

# How to detect and open a self-extracting archive

> **Before you start:** everything on this page needs the `sfx` Cargo feature. It is in
> `default`, but absent from the `read-minimal` profile — without it `unified_archive::sfx`
> does not exist and `SfxConfidence` / `SfxDetectionResult` / `StubType` are not re-exported.


This page covers asking whether a file is a self-extracting archive (SFX),
acting on the answer, and getting at either half of the file. Nothing here runs
the stub.

## Before you start

The types you need are re-exported at the crate root:

```rust
use unified_archive::{Archive, ArchiveError, SfxConfidence, SfxStagingProgress, StubType};
```

`Archive::open` already routes to the SFX path on its own: when magic-byte
detection fails *and* the path carries an executable extension (`.exe`, `.com`,
`.scr`, `.app`, `.run`, `.sh`, `.bash`), it retries as an SFX and reports both
failures together if that also fails. `Archive::open_encrypted` does the same
before applying your password. So reach for the calls below when you want the
verdict itself, when the file has an extension that fallback does not cover, or
when you already know the payload offset.

`cargo run --example detect_sfx -- <file>` prints the whole verdict for one
file; the source is `examples/detect_sfx.rs`.

## Detect

```rust
let detection = Archive::detect_sfx("installer.exe")?;
if !detection.is_sfx() {
    println!("{}", detection.summary()); // "Not a self-extracting archive"
    return Ok(());
}
```

`Archive::detect_sfx` reads a bounded prefix of the file and never opens the
embedded archive. It returns `Err` only for I/O problems (a missing or
unreadable path); "this is not an SFX" is a successful `Ok` result with
`is_sfx() == false`.

## Read the verdict

Prefer the packaged accessor — it hands you the three payload facts together, or
`None` if any of them is missing:

```rust
let Some((format, offset, stub)) = detection.payload_coordinates() else {
    return Ok(()); // not an SFX, or an incomplete result
};
println!("{format:?} payload at byte {offset}, behind a {}", stub.description());
```

For a log line, `summary()` renders the whole verdict in one string, e.g.
`SFX detected (probable): Windows PE executable stub, Zip archive at offset 8192`.
The remaining accessors — the individual format, offset, stub-type and
confidence getters, and the `evidence()` trail worth logging verbatim — are
listed with their types in
[Public API surface](../../../reference/user/en/public-api-surface.md).

Do not gate behaviour on `is_confirmed()`: the live detector produces only
`NotSfx` and `Probable`, so it is `false` for every real detection. The way to
confirm a payload is to open it — see below.

If you show the verdict to a person, `StubType::is_native()` tells you whether
that stub kind would run on the platform you are on right now, which is what
lets you warn them that a Windows-PE installer will not execute on their Linux
box. It says nothing about whether the *payload* can be extracted; that is
platform-independent.

## Open the payload

```rust
let archive = Archive::open_sfx("installer.exe")?;
for entry in archive.list_files()? {
    println!("{} ({:?} bytes)", entry.path, entry.size);
}
```

`Archive::open_sfx` detects, then copies the payload tail into a temporary file
and opens that through the normal pipeline. The temporary file is deleted when
the returned `Archive` is dropped. On the returned handle:

- `path()` still reports the outer path you passed in, not the staged copy.
- `format()` reports the *payload* format, while `extension_format()` inspects
  the outer path's extension and therefore usually returns `None` for a genuine
  SFX. That combination is not an extension/content mismatch.

A non-SFX input gives an `ArchiveError::Format` whose message reads `File is not
a self-extracting archive: <path>`. For an encrypted payload, call
`Archive::open_encrypted` on the outer path and let its SFX fallback stage the
payload before the password is applied.

If you already know the offset — from a previous detection you cached, from the
vendor's documentation, or from a container format this crate does not scan —
skip detection entirely:

```rust
let archive = Archive::open_at_offset("installer.exe", 8192)?;
```

`open_at_offset` trusts the number you give it: no stub classification and no
signature screening run, so the bytes at that offset must genuinely begin an
archive or the backend open fails. `offset == 0` short-circuits to
`Archive::open` and performs no copy at all. An offset at or past the end of the
file is an `ArchiveError::Format`.

## Report progress and cancel staging

Staging a multi-gigabyte installer is a visible copy. `Archive::open_with_sfx_progress`
is `open_sfx` plus a hook over that copy:

```rust
let progress = SfxStagingProgress::new(|copied| {
    eprintln!("staged {copied} bytes");
});
let archive = Archive::open_with_sfx_progress("installer.exe", Some(progress))?;
```

The callback receives the *cumulative* byte count and fires once per write
chunk. `SfxStagingProgress::new` takes an observe-only closure. To be able to
stop the copy, build the hook with `SfxStagingProgress::with_cancel` and return
`false`:

```rust
let progress = SfxStagingProgress::with_cancel(move |copied| {
    copied < 512 * 1024 * 1024 // give up past 512 MiB
});
match Archive::open_with_sfx_progress("installer.exe", Some(progress)) {
    Ok(archive) => { /* … */ }
    Err(ArchiveError::Cancelled { operation }) => {
        assert_eq!(operation, "sfx_staging");
    }
    Err(other) => return Err(other),
}
```

The partial temporary file is removed for you. Passing `None` as the hook is
exactly `open_sfx`. Cancellation shapes and the wider progress story:
[How to report progress and cancel an operation](report-progress-and-cancel.md).

## Extract the stub bytes

```rust
let stub = Archive::extract_stub("installer.exe", &detection)?;
std::fs::write("stub.bin", &stub)?;
```

You get the file's leading `data_offset` bytes verbatim — the executable prefix,
with none of the payload. For a PE, ELF, or Mach-O stub those bytes are a
complete executable image in their own right, which is exactly why the crate
will not run them and neither should you: analyse the file in a sandbox.

Two constraints on the call:

- Pass the `SfxDetectionResult` you actually obtained for that same path.
  `extract_stub` re-runs detection internally and refuses, with an
  `ArchiveError::Format`, if the fresh offset differs from the one in the result
  you handed it. A hand-built or stale result cannot steer the read to an
  offset of your choosing.
- The file must not change underneath the call. `extract_stub` checks the
  file's identity before and after the read and fails rather than return bytes
  that were never verified.

## Limits these calls will not negotiate

- **Scan window — 1 MiB.** Detection looks for payload signatures only in the
  first 1 MiB of the file. An SFX whose payload begins past that offset reports
  `is_sfx() == false`; there is no error and no warning. If you have the offset
  from another source, `open_at_offset` still works — the window bounds
  *detection*, not opening.
- **Payload staging ceiling — 16 GiB.** `open_sfx` and `open_at_offset` refuse a
  payload larger than 16 GiB with an `ArchiveError::Format` naming both the
  payload size and the maximum, and they refuse it *before* creating the
  temporary file, so nothing lands in your temp directory. The number is fixed
  in the crate for these entry points: `ExtractionLimits` carries a
  `max_sfx_payload_size` knob, but the staging path reads the built-in default
  rather than your limits, so lowering or raising it there has no effect on
  `open_sfx`. Staging also fails if the payload's size changes mid-copy, or if
  the file was replaced between detection and staging.
- **`Probable` is the strongest verdict you will get.** The embedded archive is
  never parsed during detection, so treat a successful `open_sfx` (or a
  successful `list_files` on the handle) as the only confirmation and its error
  as the refutation. What a positive verdict does and does not rule out:
  [How SFX detection decides](../../../explanation/developer/en/sfx-detection-pipeline.md).

Nothing in this pipeline tells you which installer product built the file, and
nothing anywhere in the crate executes the stub.

## Related

- Why detection is staged, what each stage rules out, and why the verdict is an
  enum rather than a score:
  [How SFX detection decides](../../../explanation/developer/en/sfx-detection-pipeline.md).
- Which formats can appear as an SFX payload and what each backend supports:
  [Format support matrix](../../../reference/user/en/format-support-matrix.md).
- Progress callbacks and cancellation across the rest of the API:
  [How to report progress and cancel an operation](report-progress-and-cancel.md).
