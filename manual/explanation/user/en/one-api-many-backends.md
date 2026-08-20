---
type: Explanation
title: One API over many backends
description: Why unified-archive puts a single Archive type over an internal backend enum, how formats map to engines, and which asymmetries the facade refuses to hide.
tags: [archive, formats, api, decision, AD-0001, AD-0002, AD-0007, DCR-009]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: ad-0001, resource: docs/records/AD-0001-single-archive-facade-with-backend-enum.md }
  - { id: ad-0002, resource: docs/records/AD-0002-hybrid-backend-selection.md }
  - { id: ad-0007, resource: docs/records/AD-0007-dual-zip-backend-strategy.md }
  - { id: dcr-009, resource: docs/records/DCR-009-collapse-dual-zip-to-single-zip-crate-backend.md }
  - { id: archive-facade, resource: src/archive.rs }
  - { id: format-capabilities, resource: src/format.rs }
  - { id: read-backend-trait, resource: src/backend.rs }
  - { id: creation-routing, resource: src/creation.rs }
synced_hash: 14dd787917cd049aa7951cef07baaa731fe1ca6a660420c67d739f151b677e37
---

# One API over many backends

## The problem the facade solves

The Rust ecosystem has good archive libraries — one format at a time. Each brings its own
model of what an archive is, and none of the models agree.

The `zip` crate hands you a seekable reader over a central directory that it materialises when
the reader is constructed. `sevenz-rust2` parses a table of contents up front. libarchive is a
one-shot forward iterator over headers: it cannot be rewound, so anything that needs a second
pass has to open the file again. The UnRAR SDK is C++ with process-wide global state and speaks
only RAR. They disagree about whether you may return to an earlier entry, whether metadata is
available before payload, whether a handle survives more than one operation, and what a "file"
in an archive even is.

A program that wants to accept whatever a user drops on it had to learn all four models and
write its own dispatch layer — the same dispatch layer, badly, in every such program. AD 0001
records the decision to absorb that work once: the public API is centred on a single `Archive`
type, and backend-specific behaviour is hidden behind an internal enum. Listing, extraction,
integrity checking and inspection have the same signatures whatever engine is doing the work.

## The shape: one type, a private enum, detection in front

An `Archive` handle carries a backend, the path it came from, the `ArchiveFormat` that was
detected, and an access mode. You never tell it which engine to use.

Detection is content-first. Magic bytes win when present, so a RAR file renamed with a `.zip`
suffix opens as RAR and is routed to UnRAR. The extension is consulted only where it is the
authoritative signal — `.tar` has no consistent magic, ISO 9660's volume descriptor sits far
past the cheap read window, and raw LZMA has no stable short marker — or to promote an ambiguous
bare-codec signature to its compound form, which is how a gzip stream named `backup.tar.gz`
becomes `TarGzip` rather than `Gzip`. Executable extensions divert to the self-extracting-archive
path. `ArchiveFormat::detect` and `Archive::extension_format` are both public, so you can
compare what the bytes say against what the name says.

One detail of that routing is worth knowing because it can surface as an error you did not ask
for. Detection and backend construction are two separate opens of the same pathname, so `open`
captures the file's identity at the first open and revalidates it after the backend exists. If
something replaced the file at that pathname in between, the call is refused rather than handing
a backend bytes that detection never saw.

## Why an enum rather than trait objects

AD 0001 weighed two alternatives and rejected both: separate public types per format, because
that fragments the API and pushes format dispatch back onto you, and a trait-object plugin
registry, because the lifetime and ownership overhead outweighed the flexibility.

The rejection is narrower than it sounds. Dynamic dispatch is used inside the crate: there is a
crate-internal `ReadBackend` trait, and the read paths borrow whichever variant is active as a
`&dyn ReadBackend` so the shared logic is written once. What the enum decides is *ownership and
closure of the set* — which backends exist, and who may add one — not whether a vtable is
acceptable.

That closure buys three things. The set of backends is exhaustively matchable, so adding a
variant makes the compiler enumerate every dispatch site that has to answer for it. Every
variant is boxed, so the handle stays pointer-sized regardless of which engine is behind it. And
there is no registration step, no plugin lifetime, and no way for a consumer to end up with a
half-configured registry.

It costs what AD 0001 wrote down as the price: the central orchestration code carries broad
responsibility and is a coupling hotspot as formats and operations accumulate. You cannot add a
backend from outside the crate. And every new operation must produce an answer for every
backend, including honest negative answers — recovery-record queries, for instance, are
meaningful only for RAR and report "no" for everything else rather than refusing the question.

## Which engine serves which format

AD 0002 settled that no single backend met the requirements, and rejected both single-engine
options: libarchive-only for its RAR constraints and metadata limits, UnRAR-only for being far
too narrow to serve as a general-purpose library. The result is a deliberate hybrid.

On the read side today: RAR and RAR5 go to UnRAR, behind the `rar-support` feature that is on by
default; ZIP goes to the `zip` crate; 7z goes to `sevenz-rust2`; the TAR family, the standalone
compressed streams (gzip, bzip2, xz, zstd, lz4, lzma), and ISO all go to libarchive.

Writing is a *different* map, not the same one run backwards. ZIP creation uses the `zip`
crate's writer. Everything else that can be created — including 7z — is written by libarchive.
So a 7z archive you create is produced by libarchive's 7z writer while a 7z archive you read is
parsed by `sevenz-rust2`: one format, two engines, chosen per direction.

Modify mode is a third map again. A handle opened for modification reads its source through
libarchive whatever the format, because modification is implemented as a rewrite through a copy
path rather than as an edit in place. Why that is so, and what it costs you in fidelity, is
covered in [Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md).

The per-format truth — what reads, what creates, what modifies, what streams — belongs in
[Format support matrix](../../../reference/user/en/format-support-matrix.md).

## Why ZIP changed engines

ZIP is the one format whose engine visibly moved, and the history is instructive.

AD 0007 originally ran two ZIP readers. `piz`, a memory-map-based reader, was the default for
plain archives because it was expected to be substantially faster on large unencrypted ZIPs; the
`zip` crate took over when a password was supplied, because `piz` cannot decrypt. The record
also wrote down the price at the time it was accepted: two read backends cost maintenance and
introduce behavioural differences that have to be tested and documented.

A benchmark commissioned to settle the speed question was added to AD 0007 as a dated amendment
rather than as a rewrite of the ruling. Reading every entry of a DEFLATE archive holding about a
gigabyte of uncompressed content, `piz` over a memory map beat the `zip` crate over a plain file
by 0.9 percent — the common case, and far below the threshold the record had set. On a
stored (uncompressed) fixture the gap was 12.2 percent — but a third arm showed that running the
`zip` crate over a cursor on the same memory map recovered essentially all of it, which means
the advantage belonged to the read substrate, not to `piz`. Against that, peak resident memory
was about the size of the archive for both mapped arms versus a few megabytes for the streaming
file arm, because reading every entry faults the whole file in.

DCR-009 executed the collapse with owner approval. `piz` is gone; the `zip` crate is the sole
ZIP backend for encrypted and unencrypted archives alike, and `Archive::open` on a ZIP now
builds exactly the backend `Archive::open_encrypted` already built, just without a password.

What that removed is more interesting than the throughput it gave up. An entire class of
"which ZIP reader am I on?" divergence disappeared, along with the memory-map machinery that
existed only to serve it — including the extraction limit that had been introduced to cap map
sizes. And it removed a failure mode that had been an open hazard: with a mapped file, another
process truncating the archive under you raises a signal rather than an I/O error, and an
in-place overwrite silently changes bytes you have already inspected. Reads over an ordinary
file surface truncation as an ordinary error.

The collapse was not free. Beyond the stored-case throughput, duplicate entry names had to be
re-homed: the `zip` crate's directory map keys by name, so byte-identical duplicates collapse
into one listing entry, where the removed reader kept both and let the single-entry guard refuse
the ambiguity. The backend now re-walks the raw central directory to tally names and refuses
by-name single-entry access to any name that occurs more than once — and, where an exotic
mixed-encoding collision cannot be attributed, refuses the whole by-name single-entry surface
for that archive rather than guessing.

Read that episode as a general warning rather than a ZIP anecdote. A second backend has to be
paid for in behaviour that must be tested, documented and reconciled, and a single-digit
throughput win does not cover the bill.

## The asymmetry the facade cannot hide

One API does not mean one capability set. The coverage narrows as you move from reading to
writing to editing, and the facade is explicit about it rather than uniform about it.

Reading is the widest surface: every format the crate knows can be listed and extracted.
Creation is narrower, and `ArchiveFormat::can_create` is the authority — notably, the standalone
single-file compressors are read-only by an explicit scope decision, and ISO is read-only
because libarchive's ISO support is. Modification is narrower still: ZIP and 7z only.
Encryption is read-only in the strong sense — encrypted RAR, RAR5, ZIP and 7z can be opened
with a password, but supplying a password at creation time is refused outright. That ruling is
recorded as MADR-0027. Pre-consolidation documents — `docs/API_REFERENCE.md` and the generated
`docs/investigation/` snapshot among them — still cite it by its old number "AD 0027", which now
resolves to no record at all; the source tree was brought onto the current id on 2026-08-06. A
2026-07-20 amendment softened the ruling from a permanent ban to "deferred behind an explicit
opt-in", and until that opt-in ships the rejection stands. Streaming is not uniform
either: only libarchive-backed formats hand you a reader that pulls from the archive as you
read, while ZIP, 7z and RAR decode the selected entry first and hand you a cursor over the
result. Multi-volume sets read end to end for RAR and RAR5 and not for split ZIP or 7z.

The streaming difference is the one most likely to surprise you at scale, and it is the subject
of [Streaming and memory behaviour](../../../explanation/developer/en/streaming-and-memory.md).
Which call to reach for in the first place is
[How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md).

A facade could have hidden all of this behind uniform methods that fail at runtime. This one
does fail at runtime where it must, with typed errors — but it also publishes the map, which is
why capability queries exist at all. `ArchiveFormat::capabilities` returns a
`FormatCapabilities` whose fields report encryption, multi-part handling and compression
separately for the read and the write direction, plus a modification field, each valued
`Support::Full`, `Support::Partial` or `Support::None`. You can ask before you act instead of
discovering the answer from an error.

A view worth holding: prefer the typed report to the boolean helpers. The booleans exist for
compatibility and they collapse `Partial` into `true`. `can_modify` is true for ZIP and 7z, but
both report `Partial`, because modification is a lossy rewrite rather than an edit.
`supports_multipart_read` is true for ZIP, but the field is `Partial`, because a first volume
header is recognised while extraction across parts is not implemented. `supports_encryption`
collapses the two directions and answers true for formats that can only decrypt. Both
`FormatCapabilities` and `Support` are marked non-exhaustive, so match on them with a fallback
arm and expect new states to appear.

## The modes a handle can be in

A handle is in exactly one of three modes for its whole life: read, write, or modify. The mode
enum itself is internal, but it is thoroughly observable, and no method converts a handle from
one mode to another — you drop it and open again.

Read mode comes from `Archive::open`, `open_encrypted`, `open_sfx`, `open_with_sfx_progress`,
and `open_at_offset`. Write mode comes from `Archive::create` and its per-format constructors
`create_zip`, `create_seven_zip` and `create_libarchive`; `finish` is the durable commit, and
dropping an unfinished write handle performs one best-effort finalize and warns. Modify mode
comes from `Archive::modify` and `modify_with_options`; it takes an advisory exclusive lock on
the file for the life of the handle, and `commit_changes` is what makes the result visible by
swapping the file in place.

Calling across the grain gives you a specific error rather than nonsense: read operations on a
write handle produce `WriteModeOnly`, and write operations on a read or modify handle produce
`ReadOnlyBackend`.

Two further properties of a handle follow from the backend design. A read handle is a *snapshot*
frozen at first observation: every read backend memoises its listing on first use, so later
calls on the same handle return the cached view even if the file is rewritten underneath. If you
need a guaranteed-fresh view, drop the handle and open the path again, because the cache is
per-handle. And an `Archive` is `Send` but not `Sync` — move handles between threads freely, one
handle per thread, and note that RAR is additionally serialised process-wide behind a mutex, so
two threads each holding a RAR handle run one at a time even on different files.

Enabling the `v2-api` feature turns the mode into a type: `v2::ReadArchive`, `v2::WriteArchive`
and `v2::ModifyArchive` are distinct handles that delegate to the same backends, so a write
handle cannot be passed to an extract site at all, and `finish` consumes the handle instead of
leaving a used one behind. The feature is additive in 0.3 and becomes the default in 0.4.
