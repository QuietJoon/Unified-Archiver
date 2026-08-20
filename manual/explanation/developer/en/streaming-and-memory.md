---
type: Explanation
title: Streaming and memory behaviour
description: Why one entry-reader type covers backends with very different memory profiles, and why exceeding the stream's cap is an error instead of a silent end of data.
tags: [streaming, extraction, decision, DCR-006, IG-004-01]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: streaming, resource: src/streaming.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: libarchive-reader, resource: src/ffi/libarchive_wrapper/reader.rs }
  - { id: zip-backend, resource: src/ffi/zip_wrapper.rs }
  - { id: unrar-backend, resource: src/ffi/wrapper.rs }
  - { id: dcr-006, resource: docs/records/DCR-006-bounded-streaming-hard-cap.md }
  - { id: ig-004-01, resource: docs/records/IG-004-01-streaming-buffers-entire-file.md }
  - { id: ad-0035, resource: docs/records/AD-0035-oi-0057-007-sevenz-source-streaming-deferred.md }
synced_hash: 0b204c07a60e02c1a84cfe560e09c14e66df7a422592dd10e064398e07165325
---

# Streaming and memory behaviour

## What "streaming" is a claim about

The unit of streaming in this crate is one entry. `Archive::extract_to_stream`
returns a `StreamingExtractor`, which is an owned `Read` plus a byte counter and
an optional declared size. Nothing in that type says where the bytes come from,
and that is the whole difficulty: the interesting property is not "you can call
`read` in a loop" — you always can — but "resident memory tracks your buffer
rather than the entry". Only some backends can honour the second claim, and the
type cannot tell them apart.

That asymmetry is deliberate but uncomfortable, and `src/backend.rs` records the
shape of the wanted fix in its roadmap notes: a typed `streaming_mode()`
capability query that would move the distinction from prose into the type
system. Until that exists, the crate discharges the obligation with
documentation — on `Archive::extract_to_stream`, on `StreamingExtractor`, and in
[How to stream a large entry](../../../how-to/user/en/stream-a-large-entry.md).

## Why the guarantee is per-backend

libarchive is a pull API, and that is the entire reason it streams.
`LibarchiveStreamReader::open` walks headers with `archive_read_next_header`
until it reaches the entry's stable listing index (refusing a header whose name
does not match the one the listing resolved — the drift guard), then hands the
still-open handle to the reader. Each `Read::read` is one `archive_read_data`
call into the caller's buffer. The reader owns the handle exclusively, frees it
in `Drop`, and carries an `unsafe impl Send` justified by that exclusive
ownership. An owned entry reader is the natural shape here, so the crate takes
it.

The native Rust backends decode an entry at a time behind an API that lends a
reader rather than giving one away.

- **ZIP.** The `zip` crate's entry reader (`zip::read::ZipFile<'a>`) borrows the
  archive it came from. `src/ffi/zip_wrapper.rs` keeps that archive in a
  mutex-guarded cache and only touches it inside a `with_zip` closure, so an
  owned reader would have to hold both the mutex guard and the borrow — a
  self-referential value that would also pin the shared cached archive for as
  long as the caller kept reading. The wrapper's `extract_to_stream` therefore
  calls `extract_to_memory` and wraps the buffer in a `Cursor`, with the
  trade-off stated in its own rustdoc.
- **7z.** sevenz-rust2 exposes entry data only through callback-scoped
  `&mut dyn Read` or as an owned `Vec<u8>`; there is no public function that
  returns a free-standing reader for one entry. That is not an oversight
  upstream: the decoder state has to be held and mutated across entries for
  solid blocks, which is fundamentally at odds with yielding a reader back to
  the caller. AD 0035 recorded the investigation against 0.19.4, the two
  rejected workarounds, and the deferral.
- **RAR.** The UnRAR SDK's model is extraction to disk. The wrapper stages the
  entry into a `tempfile::TempDir`, reads the staged file back into a `Vec<u8>`,
  and wraps that. There is no point in the SDK's flow where an in-memory pull
  is available.

So the uniform type is a deliberate API choice — one call, one reader, whatever
the container — and the non-uniform memory profile is inherited from what the
upstream libraries can lend out. The honest summary is that on ZIP, 7z, and RAR
`extract_to_stream` buys ergonomics and a byte counter, not a memory bound.

## Where the problem has been, and where it stands

IG-004-01 filed this in January 2025 (Review 0004) and the disposition was
REJECT with reasons that still read as reasonable: true streaming needed a
refactor across every FFI backend, memory was already bounded by single-entry
size rather than archive size, and the constraint was documented. The general
gap is tracked as DEF-004 in `docs/project/stub-manifest.md`, which since the
removal of the piz ZIP backend names the three buffered backends that remain, and
AD 0035 closed the 7z half as deferred pending upstream support, with explicit
revisit triggers.

What has actually changed since is not the buffering. It is that the bounded
part of the contract became enforceable and honest, which is the subject of the
next section.

## Why over-production is an error rather than a truncation

DCR-006 records the design it replaced: AD 0062 §A.2 had the bounded public
methods return `io::Take<StreamingExtractor>`. That capped memory, and it did so
with one line of standard library. Its defect is that `io::Take` reaches EOF at
the limit and reports nothing — which is byte-for-byte what a complete, healthy
read looks like.

The class of corruption that hides in that gap is an archive whose header
under-declares an entry's decoded size. A consumer that hashed, parsed, or
re-packed what it received would treat a prefix of the payload as the payload
and succeed. The caller's only defence was to compare the observed byte count
against an expected size after the fact — which presumes an expected size, the
very thing an unknown-size entry does not provide.

DCR-006 (2026-07-17, from R0080-0007) replaced the clamp with the hard-cap
probing reader the crate already had for the `extract_to_stream_with_limit`
trait default and for manifest digests. The reader stops filling the buffer at the
cap, then probes a single byte: `Ok(0)` is a genuine end of data, and one more
available byte is `io::ErrorKind::InvalidData` naming the cap. The return type
went from `io::Take<StreamingExtractor>` back to `StreamingExtractor`; the
migration cost was borne by callers who had matched on the `io::Take` type or
relied on truncation.

The 2026-07-22 amendment (R0081-0027 / I2) finished the shape. Rather than a
bounded default method plus parallel `_unbounded` methods — which had already
drifted apart on whether a cap was propagated — the bound became a value:
`StreamBound::{DeclaredSize, Cap, Unbounded}`, interpreted by exactly one
`match` at the end of `Archive::extract_to_stream_impl`. Choosing "unbounded" in
one place, as one variant among three, is what structurally retires the drift;
it is a good illustration of preferring a value that cannot be forgotten over a
second code path that must be kept in step.

Two amendments closed the two halves that shape left open.

The 2026-08-09 amendment (R0001-0007 / R0001-0008) fixed where the bound binds
and what "declared" means. The cap had been applied only to the returned reader,
so on the backends that materialize an entry before exposing a `Read` it bounded
what the caller could *read* while the whole entry had already been staged; the
caller's `max_file_size` is now threaded into the backend step. And
`StreamBound::DeclaredSize` had been taking its size from
`StreamingExtractor::from_bytes` — the materialized buffer's own length — so on
those backends the cap equalled the payload exactly and the over-production
error was unreachable by construction. The size now comes from the authoritative
preflight listing, which is what makes the comparison a comparison at all.

The 2026-08-12 amendment (R0001-0011) finished both. `StreamBound::DeclaredSize`
became two-sided: with a listing-declared size, over-production is still
`InvalidData` and an end-of-stream *below* the declaration is now
`io::ErrorKind::UnexpectedEof` rather than a short read that a consumer would
treat as the payload — the class of corruption this page opened with, now caught
from the other side. `StreamBound::Cap(n)` stayed ceiling-only on purpose: a
caller-chosen budget is a limit, not an assertion about the entry's size. And the
backend's materialization budget became `min(bound, max_file_size,
max_total_size)`, so a `Cap(1024)` no longer lets a staging backend buffer a
multi-gigabyte entry first. The visible cost is a per-backend split in *where*
the violation appears: libarchive reports it from a read, while ZIP, 7z and RAR
report it from the extract call, because they must decide before they buffer. The
guarantee is "no silent success", not "one error shape".

One deliberate coarseness remains. Over-production and a caller-configured
ceiling overrun both surface as `InvalidData`. libarchive's extract-to-memory
path does separate them, choosing between a corruption error and an
`OperationBlocked` naming the real limit; the stream path has no equivalent, so a
caller who needs the distinction has to infer it from the bound it passed. Under
`DeclaredSize` the two are the same event anyway — the cap *is* the declaration —
and truncation is now distinguishable by kind.

## The memory profile to expect

Read this as the cost model, family by family.

- **libarchive-backed reads** (TAR family, ISO, standalone compressed streams):
  the caller's buffer plus whatever libarchive holds internally for its
  decompressor. Independent of entry size.
- **ZIP and 7z**: one entry resident as a `Vec<u8>`, then a cursor over it. Peak
  is the largest single entry you touch, not the archive.
- **RAR**: the entry is written into a private staging directory and read back
  into a `Vec<u8>`, so the peak is one entry on disk plus one in memory. The
  staging tree is owned by `tempfile::TempDir` and removed on drop, on success
  and error alike.
- **Self-extracting archives**: opening one copies the payload — everything from
  the detected offset to end of file — into a tempfile in 64 KiB chunks so a
  backend that expects offset-zero input can open it. The `Archive` holds that
  temp path for its lifetime, and the compression-ratio denominator uses the
  staged payload size rather than the executable's. The cost is disk rather than
  memory, bounded by the SFX payload ceiling (16 GiB by default) and — when the
  open came through SFX detection rather than a caller-supplied offset — by an
  identity check that refuses a source replaced between detection and staging.
- **Modification commits**: the rewrite streams retained entries through the
  writer, but an entry whose source cannot declare a size is drained into a
  tempfile in 64 KiB chunks and capped at the default `max_file_size`, so a
  streaming-only source cannot fill the disk during a commit. A 7z source
  buffers each retained entry — the consequence AD 0035 accepted.

The pattern across all of these is that input of unknown length is turned into
bounded work by staging to disk under an explicit ceiling rather than by
buffering it. The place that buffers by design is the entry payload on the three
non-libarchive backends, and that is the open item; the place where the ceiling
is thinnest is the RAR stream path, whose buffer is sized from whatever the
decoder actually wrote rather than from a caller-configured limit.

## The trade-off that is still open

Fixing the native backends is possible and nobody has argued it is not worth
having. What it costs differs sharply by backend.

ZIP looks tractable: the entry reader borrows a `ZipArchive` that the wrapper
already owns, so an extractor that takes ownership of the archive for the
reader's lifetime is a plausible design. The price is giving up the shared
cached-archive optimisation for the duration of a stream, plus a
self-referential or handle-moving construction that has to be got right once.

7z is blocked upstream, not internally. AD 0035's rejected options are the
honest menu: a background thread writing into a pipe (a real concurrency
surface, with deadlock on early reader drop, error propagation across the thread
boundary, and password lifetime to think about, plus a new dependency), or a
self-referential struct over a callback-scoped reader whose type upstream does
not even export. Neither is proportionate to a space inefficiency, and both
would be thrown away the day sevenz-rust2 exposes an owned reader — which is
exactly the recorded revisit trigger.

RAR would need the same pipe-shaped concurrency to turn a data callback into a
`Read`, for the same reasons.

Set against that is API parity, and this is where a view is worth stating: a
uniform type whose guarantee is not uniform is defended only by documentation,
and documentation is not checked at compile time. Callers who read the signature
and not the paragraph will assume a bound the crate does not always give them,
and `ExtractionLimits` plus `StreamBound` bound the damage without removing the
surprise. On that reading the cheap half of the fix — a capability query that
makes a backend say whether it truly streams, so callers can branch instead of
guess — has better value per unit of risk than any of the three rewrites, even
though it solves nothing about the buffering itself. The rewrites remain the
right end state; the visibility gap is the part that can be closed now.

Why one API sits over backends with such different characters in the first place
is discussed in
[One API over many backends](../../../explanation/user/en/one-api-many-backends.md).
