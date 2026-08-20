---
type: How-To Guide
title: How to report progress and cancel an operation
description: Wire a ProgressCallback into extraction, creation, and SFX staging, throttle it, and understand what a cancellation leaves on disk.
tags: [api, extraction, creation, sfx, AD-0021]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: options, resource: src/options.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: creation, resource: src/creation.rs }
  - { id: archive-facade, resource: src/archive.rs }
  - { id: ffi-common, resource: src/ffi/common.rs }
  - { id: ad-0021, resource: docs/records/AD-0021-per-entry-creation-progress.md }
synced_hash: cc81ca38887f280819ccf43434a986c004bb5efc7df24aceeb8473395a1a26d0
---

# How to report progress and cancel an operation

Progress reporting and cancellation are the same mechanism: one callback that
returns `ControlFlow`, where `Break` means stop. This page wires it into
extraction, creation, and SFX staging, and states what a cancelled operation
leaves behind.

## Write the callback

A closure is enough for most reporting: anything that is
`FnMut(u64, Option<u64>) -> ControlFlow<()> + Send` already is a
`ProgressCallback`.

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;

let cb: Box<dyn ProgressCallback> = Box::new(|processed: u64, total: Option<u64>| {
    eprintln!("{processed} / {total:?}");
    ControlFlow::Continue(())
});
```

Annotate the binding. `ControlFlow` carries a break type as well as a continue type, and
`Continue(())` alone does not pin it, so a bare `let cb = Box::new(…)` fails to compile with
`type annotations needed`. Assigning straight into `options.progress`, as the next section
does, supplies the same information and needs no annotation.

Implement the trait by hand when the callback needs state of its own:

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;

struct Bar {
    last_percent: u64,
}

impl ProgressCallback for Bar {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()> {
        if let Some(total) = total.filter(|t| *t > 0) {
            let percent = processed * 100 / total;
            if percent != self.last_percent {
                eprintln!("{percent}%");
                self.last_percent = percent;
            }
        } else {
            eprintln!("{processed} bytes");
        }
        ControlFlow::Continue(())
    }
}
```

Either way the value ends up boxed, because both option structs store
`Option<Box<dyn ProgressCallback>>`. The callback is invoked through `&mut self`
from a single thread per operation, so captures such as `Cell` or `RefCell` are
fine and need no mutex. The trait declaration and its bounds:
[Options and defaults](../../../reference/user/en/options-and-defaults.md).

One hard rule: do not perform an archive operation from inside the callback.
Backends invoke it synchronously while holding backend-internal state, and
during a RAR operation the process-wide UnRAR lock is held. Re-entering on that
thread is detected and returns `ArchiveError::OperationBlocked`
("re-entrant UnRAR access detected") instead of deadlocking the process. Record
what you need and act on it after the call returns.

## Attach it to extraction

```rust
use std::ops::ControlFlow;
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("release.tar.zst")?;
let mut options = ExtractionOptions {
    destination: PathBuf::from("./out"),
    ..Default::default()
};
options.progress = Some(Box::new(|processed: u64, total: Option<u64>| {
    if let Some(total) = total {
        eprintln!("{processed}/{total}");
    }
    ControlFlow::Continue(())
}));
let result = archive.extract_all(options)?;
```

Assign the field: `ExtractionOptions` has a builder for `password` only.

On extraction `total` is `Some(denominator)`: the sum of declared uncompressed
sizes over the selected entries, computed only when a callback is present. The
ZIP and 7z backends leave out the entries extraction skips anyway (directories
and links, plus special Unix modes on ZIP); the libarchive and UnRAR backends
sum every selected entry that declared a size. An entry whose size the format
never declared appears in neither the denominator nor the numerator, so it
advances the internal byte budget without moving the bar, and `processed` stays
monotonic and never exceeds the total. Each backend also makes a final call
after the walk — the completed total, or for libarchive its real processed count
clamped to the denominator — and a `Break` returned there is honoured as well.

Which calls honour the field is not uniform:

- Honoured: `extract_all`, `extract_some` / `extract_filtered`,
  `extract_files`, `extract_by_ids`.
- Ignored: `extract_file`, `extract_to_memory` and
  `extract_to_memory_with_options`, `extract_to_stream` and
  `extract_to_stream_with_options`. The single-entry and in-memory paths hand
  the work straight to the backend without the progress plan; for a stream,
  drive reporting from `StreamingExtractor::bytes_read` instead.

Cadence is per entry boundary plus once per 64 KiB chunk within an entry. The
libarchive and UnRAR paths throttle themselves to roughly sixty calls per second
using the same rate limiter described below; the ZIP and 7z paths do not
throttle at all, so a multi-gigabyte entry means one call per chunk.

## Cancel, and know what survives

Return `ControlFlow::Break(())`. The operation stops and the call returns
`ArchiveError::Cancelled { operation }`, one typed variant for every
cancellation surface, so you match a variant rather than sniffing message text.

```rust
use unified_archive::ArchiveError;

match archive.extract_all(options) {
    Ok(result) => { /* result.warnings */ }
    Err(ArchiveError::Cancelled { operation }) => {
        eprintln!("cancelled during {operation}");
    }
    Err(other) => return Err(other),
}
```

For every disk-extraction entry point the label is `"extract_all"`, including
cancellations raised from `extract_files`, `extract_by_ids`, and
`extract_some` — the backends pass one shared label. Creation reports
`"create"`, and SFX staging reports `"sfx_staging"`.

Cancelling means "stop future work", never "undo finished work":

- **Extraction.** Each regular file is written to a staging file beside its
  destination and renamed into place only after it completes, so the entry that
  was in flight leaves nothing at its destination path. Entries already renamed
  stay, and so do the destination directory and any parent directories created
  along the way. There is no rollback.
- **Creation.** The callback fires after the bytes reach the writer, so a
  `Break` cannot un-write them. The error surfaces from the `add_*` call, the
  entry in flight is left partial and is not counted in the writer's entry
  count, and the writer poisons itself: further `add_*` calls return
  `OperationBlocked` naming the poison. `finish` (or `Drop`) still drains the
  writer to a structurally valid archive containing every entry that completed
  before the `Break`. If you need all-or-nothing, create into a temporary path
  and rename it yourself on success.
- **SFX staging.** The partial temporary file is removed for you.

## Attach it to creation

```rust
use std::ops::ControlFlow;
use unified_archive::{Archive, ArchiveFormat, CompressionOptions};

let mut options = CompressionOptions::new(ArchiveFormat::TarGzip);
options.progress = Some(Box::new(|written: u64, _total: Option<u64>| {
    eprintln!("{written} bytes written");
    ControlFlow::Continue(())
}));

let mut archive = Archive::create("out.tar.gz", options)?;
archive.add_directory_recursive("./payload")?;
archive.finish()?;
```

`CompressionOptions` has no `progress` setter, so assign the field; the typed
per-format builders `ZipCompressionOptions`, `SevenZCompressionOptions`, and
`LibarchiveCompressionOptions` do have `progress(Box<dyn ProgressCallback>)`.
`Archive::create` takes the options by value and moves the callback into the
chosen writer.

Creation reports differently from extraction, by design:

- `total` is always `None`. Creation streams are not pre-sized: the writer reads
  from a slice or walks a directory as it goes, so there is no denominator to
  report (AD 0021). A percentage needs a total you estimate yourself.
- `processed` is the cumulative count of payload bytes handed to the writer,
  accumulated saturating across the whole archive.
- Cadence is per entry rather than per byte in the sense AD 0021 settled: you
  get at least one call per `add_*` call, including a zero-byte call for an
  empty entry. Large entries do report inside the entry, though — the ZIP writer
  chunks every payload at 64 KiB and calls back per chunk, and the libarchive
  writer does the same for entries streamed from a path, while an entry added
  from a byte slice notifies once with its full length. AD 0021's text still
  describes the strict once-per-entry form it originally landed with.

Rewrites report through the same channel: `ModificationOptions::compression`
carries a `CompressionOptions`, and `commit_changes` moves it — callback
included — into the writer that rebuilds the archive.

## Throttle with RateLimiter

Because several backends call you per chunk, keep the callback body cheap or
gate it. `RateLimiter` is the crate's own throttle, reachable as
`unified_archive::options::RateLimiter` (it is not among the crate-root
re-exports); the default interval is 16 ms, about sixty updates per second:

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;
use unified_archive::options::RateLimiter;

let mut limiter = RateLimiter::new();
let cb: Box<dyn ProgressCallback> = Box::new(move |processed: u64, total: Option<u64>| {
    if limiter.should_update() {
        eprintln!("{processed} / {total:?}");
    }
    ControlFlow::Continue(())
});
```

It throttles your reporting, not the callback itself: the backend still calls
you per chunk, and a `Break` you return still cancels immediately. Keep the
cancellation decision outside the `should_update` branch if you want it checked
on every call. The other constructor, the first-call rule, and the full method
list: [Options and defaults](../../../reference/user/en/options-and-defaults.md).

## Observe SFX payload staging

Opening a self-extracting archive copies the embedded payload into a temporary
file before a normal backend can read it, and that copy is visible work for a
multi-gigabyte installer. It has its own hook, because it happens before any
`ExtractionOptions` exists:

```rust
use unified_archive::{Archive, SfxStagingProgress};

let progress = SfxStagingProgress::new(|copied| eprintln!("staged {copied} bytes"));
let archive = Archive::open_with_sfx_progress("installer.exe", Some(progress))?;
```

`SfxStagingProgress::new(impl FnMut(u64) + Send + 'static)` observes only. The
cancelling variant is
`SfxStagingProgress::with_cancel(impl FnMut(u64) -> bool + Send + 'static)`,
where returning `false` aborts the copy and `Archive::open_with_sfx_progress`
returns `ArchiveError::Cancelled { operation: "sfx_staging" }`. Either way the
closure receives the cumulative byte count and is called once per 64 KiB chunk,
with no internal throttling. Passing `None` as the hook is exactly
`Archive::open_sfx`. More on that path:
[How to detect and open a self-extracting archive](handle-self-extracting-archives.md).

## Reuse the options without the callback

`CompressionOptions` does not implement `Clone`, because its `progress` field is
a trait object. To hand the same configuration to another operation without the
callback, take a stripped copy:

```rust
let plain = options.strip_progress();
```

The original keeps its callback unless you assign the result back, so the intent
— this operation gets no progress events — is written down at the call site.

What `strip_progress` copies, why `Clone` was removed, and every option field
with its type and default:
[Options and defaults](../../../reference/user/en/options-and-defaults.md).
