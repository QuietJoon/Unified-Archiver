---
type: How-To Guide
title: How to stream a large entry
description: Read one archive entry incrementally through a bounded StreamingExtractor and pick the StreamBound that matches your trust in the archive.
tags: [streaming, extraction, api]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: streaming, resource: src/streaming.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: backend-trait, resource: src/backend.rs }
  - { id: example, resource: examples/streaming_extract.rs }
  - { id: dcr-006, resource: docs/records/DCR-006-bounded-streaming-hard-cap.md }
  - { id: user-manual, resource: docs/USER_MANUAL.md }
synced_hash: 5c9bf06782303bd3aa26c1d6cd2bfe99a96ddfbedc756a6baa5ee8cfe9486eba
---

# How to stream a large entry

Use `Archive::extract_to_stream` when you want to consume one entry
incrementally through a `Read` instead of receiving the whole payload as a
`Vec<u8>`: piping it into a hasher, a socket, a decoder, or a file you write
yourself.

## Before you start

- Open the archive for reading (`Archive::open`, or `Archive::open_encrypted`
  for an encrypted one). A handle in write mode skips the safety pre-check
  described below.
- Have the entry path exactly as it appears in `list_files`. The single-entry
  gate resolves the path against a fresh listing and returns
  `ArchiveError::OperationBlocked` when the path is missing, when two entries
  carry it, or when the match is a directory, symlink, or hard link.
- Know which backend serves your format. Only libarchive-backed formats
  actually stream; see "Plan for the backend you are on" below.
- `Archive` is `Send` but not `Sync`, so keep one handle per thread.

## Read the entry in a fixed-size buffer loop

```rust
use std::io::{Read, Write};
use unified_archive::{Archive, StreamBound};

let archive = Archive::open("media.tar")?;
let mut stream = archive.extract_to_stream("video/large.bin", StreamBound::DeclaredSize)?;
let mut sink = std::fs::File::create("large.bin")?;

let mut buffer = vec![0u8; 64 * 1024];
loop {
    let n = stream.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    sink.write_all(&buffer[..n])?;
}
```

The `?` operators in that loop mix two error types: `Archive::extract_to_stream`
yields `ArchiveError`, while `File::create`, `read` and `write_all` yield
`std::io::Error`. `ArchiveError` implements no `From<std::io::Error>`, so the
fragment belongs in a function returning
`Result<(), Box<dyn std::error::Error>>` — not in one returning
`Result<(), ArchiveError>`.

The signature is
`extract_to_stream(&self, file_path: &str, bound: StreamBound) -> Result<StreamingExtractor>`.
`Read` is the only trait `StreamingExtractor` implements, so any `Read`-shaped
consumer works: `io::copy`, `BufReader`, a digest, a decoder.

Propagate the error from `read`. Do not write `while let Ok(n) = stream.read(..)`
and do not treat `Err` as end of input: a cap overrun, a decode failure, and a
real I/O fault all arrive as `Err`, and swallowing them turns a truncated or
hostile payload into apparent success.

Before returning a reader, the call rebuilds the entry listing and runs the
per-entry safety gate: declared size against `max_file_size` and
`max_total_size`, and per-entry compression ratio against
`max_compression_ratio`. A refusal is `ArchiveError::OperationBlocked` and no
bytes are read. Entries with no declared size pass the gate; the bound is what
protects you then.

## Choose the bound

`StreamBound` is a three-variant `Copy` enum. The variant you pass shapes both
the output cap on the returned reader and the budget the backend is allowed to
materialize while preparing it — the safety pre-check runs the same way for all
three.

- `StreamBound::DeclaredSize` — the default choice. Holds the stream to
  *exactly* the entry's declared uncompressed size, taken from the archive's
  listing: more bytes than that is an error, and fewer bytes than that is also
  an error. When the format declared no size, there is no declaration to hold
  the stream to, so it degrades to a ceiling-only cap at the effective entry
  limit — `min(max_file_size, max_total_size)`, 1 GiB with default limits, and
  no cap at all if you set both to `Cap::Unlimited`.
- `StreamBound::Cap(n)` — a ceiling-only budget at `n` bytes. An entry smaller
  than `n` stops cleanly; an entry longer than `n` produces an error after `n`
  bytes rather than a clean stop, and on ZIP, 7z and RAR — which must buffer the
  entry before handing you a reader — an entry whose declared size already
  exceeds `n` is refused by the `extract_to_stream` call itself. To read a
  prefix of a larger entry portably, pass `StreamBound::DeclaredSize` and wrap
  the extractor in `Read::take`.
- `StreamBound::Unbounded` — no caller-chosen output cap; decoded bytes are
  forwarded verbatim. A hostile archive that decodes to more than it declared
  can then drive your own `read_to_end` into large allocation, bounded only by
  the effective entry limit the backend still receives. Use it for trusted
  input, or wrap the result in your own budget (see the next section).

Under either cap, over-production is an error and never a silent stop. Once the
cap is reached the reader probes one more byte: a genuine end of data yields
`Ok(0)`, while another available byte yields `io::Error` of kind
`io::ErrorKind::InvalidData` whose message names the cap ("stream emitted more
than the N byte cap"). Under `DeclaredSize` with a known declared size,
truncation is reported by a different kind — `io::ErrorKind::UnexpectedEof`,
message "entry truncated: stream ended after N of the declared M bytes" — so you
can tell a truncated source (retry, re-download) from a hostile one (reject).
That verdict is sticky: retrying the read repeats the error instead of falling
through to `Ok(0)`. Read both as statements about the archive, not about your
buffer size.

## Track size and progress

`StreamingExtractor` carries three read-only accessors:

- `total_size() -> Option<u64>` — the entry size the backend reported, or
  `None` when the format never declared one. Under `StreamBound::DeclaredSize`
  with a known size it is the listing's declaration; otherwise, on the buffered
  backends, it is the length of the materialized payload.
- `bytes_read() -> u64` — bytes handed to you so far, accumulated saturating.
- `progress() -> Option<f64>` — `bytes_read / total_size`, clamped to `1.0`,
  and `None` when the size is unknown. A zero-size entry reports `1.0`.

`examples/streaming_extract.rs` drives all three: a 64 KiB chunk loop that
prints a percentage every five points. It passes `StreamBound::Unbounded` so
that the accessors are exercised on an uncapped reader — production code reading
untrusted input should keep `StreamBound::DeclaredSize`. Run it with
`cargo run --example streaming_extract -- <archive> <entry>`.

There is also `take_bounded(self, fallback: u64) -> io::Take<Self>`, which
consumes the extractor and clamps it at `total_size()` or `fallback` when the
size is unknown. It is the silent variant: `io::Take` reaches EOF at the limit
and reports no error, so use it only when you layer it on
`StreamBound::Unbounded` and are content with truncation, or when you compare
the observed byte count against the expected size yourself.

## Plan for the backend you are on

The `Read` surface is uniform; the memory profile is not.

- libarchive-backed formats (the TAR family, ISO, and standalone compressed
  streams) stream. The reader owns a libarchive handle
  positioned at the entry and pulls decompressed bytes per `read` call, so
  resident memory tracks your buffer, not the entry.
- ZIP, 7z, and RAR do not. Each of those backends materializes the entry
  first — into a `Vec<u8>` for ZIP and 7z, and via a temporary directory on
  disk for RAR — and then hands you a cursor over that buffer. Peak memory is
  the size of the single entry.

So on ZIP, 7z, and RAR, treat `extract_to_stream` as an ergonomic wrapper, not
a memory bound: check the entry's size through `find_entry` or `list_files`
first, and keep `ExtractionLimits::max_file_size` tight enough that the
buffered payload is one you can afford. For genuinely multi-gigabyte inputs,
prefer a libarchive-backed container. Why the guarantee is per-backend is
discussed in
[Streaming and memory behaviour](../../../explanation/developer/en/streaming-and-memory.md).

## Pass limits, a password, or a CRC expectation

`extract_to_stream_with_options(&self, file_path: &str, options: &ExtractionOptions, bound: StreamBound)`
returns the same `StreamingExtractor` and honours three fields of
`ExtractionOptions`:

- `limits` — used for the per-entry safety pre-check, as the ceiling on the
  backend's materialization budget (`min(max_file_size, max_total_size)`, which
  a `StreamBound` may tighten but never loosen), and as the unknown-size
  fallback ceiling for `StreamBound::DeclaredSize`.
- `password` — when `Some`, the archive is reopened through
  `Archive::open_encrypted` before the read. Formats that cannot be encrypted
  return `ArchiveError::Unsupported` rather than quietly reading plaintext, so
  do not set it for TAR, ISO, or a standalone compressed stream.
- `verify_crc32` — checked before any I/O. `true` on a libarchive-backed
  format returns `ArchiveError::Unsupported`, because those formats carry no
  per-entry CRC32. On the other backends it changes nothing about the reader
  you get back; the ZIP path, for instance, verifies the entry CRC while
  buffering whether or not you set the flag.

`destination`, `overwrite`, `preserve_permissions`, `preserve_times`, `filter`,
and `progress` describe disk writes or multi-entry traversal and are ignored on
this path — including `progress`, so drive your own progress from
`bytes_read()`.

```rust
use unified_archive::{Archive, Cap, ExtractionLimits, ExtractionOptions, StreamBound};

let options = ExtractionOptions {
    limits: ExtractionLimits::builder()
        .max_file_size(Cap::Limited(256 * 1024 * 1024))
        .build(),
    ..Default::default()
};

let archive = Archive::open("untrusted.tar.gz")?;
let stream = archive.extract_to_stream_with_options(
    "payload.bin",
    &options,
    StreamBound::DeclaredSize,
)?;
```

Every field and its default: [Options and defaults](../../../reference/user/en/options-and-defaults.md).

With the `v2-api` feature, `ReadArchive::extract_to_stream` and
`ReadArchive::extract_to_stream_with_options` take the same arguments and
delegate to these methods.

## When to reach for a different call

If you want the whole entry as bytes, `extract_to_memory` says so directly and
skips the cap question. If you want files on disk, the `extract_all` /
`extract_some` family writes them for you with progress and warnings. Picking
between them:
[How to pick the right extraction call](choose-an-extraction-api.md).
