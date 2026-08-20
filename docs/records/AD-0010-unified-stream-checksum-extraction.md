---
type: ADR
title: "AD: Unified Stream Checksum Extraction"
description: "Accepted"
tags: [decision, ADR-0010]
timestamp: 2026-04-12T00:00:00Z
status: active
---

# AD: Unified Stream Checksum Extraction

Status: Accepted

## Context and Problem Statement
Consumers need to verify single-file compressed formats (GZIP, BZIP2, XZ) without full decompression or archive-level metadata. Relying on libarchive verification requires reading the entire stream, which defeats the purpose of a fast integrity check.

## Decision Drivers
- Fast integrity checks for non-archive compressed containers
- No full stream read required for checksum retrieval
- Support for format-native checksums (CRC32, CRC64, etc.)

## Considered Alternatives
- **Rely solely on libarchive verification** -- rejected because it requires a full stream read, negating the speed advantage of direct checksum extraction.

## Decision Outcome
We decided to implement `stream_crc.rs` to parse trailers and headers for format-native checksums (CRC32, CRC64, etc.) because it enables fast integrity checks using internal format metadata without decompressing the full stream.

## Consequences
- Good: Enables fast integrity checks using internal format metadata without full decompression.
- Bad: Requires manual bit-level parsing of format trailers and headers; full XZ checksum extraction still requires additional parsing work beyond what is currently implemented.

## Amendment (2026-08-21, R7 module split and truncation classification)

Scope: an internal reorganisation of `src/stream_crc.rs` plus one deliberate behavioural
decision. The Decision Outcome above still stands — the module keeps parsing trailers and
headers for format-native checksums without decompressing, and nothing here supersedes it.
Status remains Accepted.

### Re-framing: three seams instead of one module

The advisory design review (`docs/architecture/decision-review-2026-07-19.md`, candidate R7)
observed that one file was carrying three unrelated concerns. The concerns were real, so the
module is now split along them, as file-as-module children:

- `src/stream_crc/digest.rs` — the **value** layer: `StreamChecksum`, `CheckType`, the
  type-gated `crc32_value`/`crc64_value` accessors, and the two search primitives that lift a
  digest out of a buffer (`find_pattern_last`, `find_bzip2_eos_crc_bitwise`).
- `src/stream_crc/codec.rs` with `codec/gzip.rs`, `codec/bzip2.rs`, `codec/xz.rs` — the
  **framing** layer: one child per format, each owning a single entry point that opens the path,
  validates the container's fixed header, and reads the trailer or stream flags.
- `src/stream_crc/detect.rs` — the **dispatch** layer: the six-byte magic probe and the
  routing table, and nothing else.

`src/stream_crc.rs` retains the module documentation and the re-exports, and holds no logic.

Two boundary choices worth recording, because a later reader will otherwise re-litigate them:

- **The bzip2 bit-level EOS scan lives in `digest.rs`, not in `codec/bzip2.rs`.** The seam that
  earns its keep is *touches the filesystem* versus *pure function of bytes*: `digest.rs` is
  entirely I/O-free and therefore testable without a fixture on disk, while every function in
  `codec/` opens a file. Splitting bzip2's framing knowledge across that line costs one extra
  hop and buys a testability boundary that the module already relied on informally.
- **Every child module is private, and all public items are re-exported from
  `stream_crc`.** The public surface is byte-for-byte what it was: `stream_crc::CheckType`,
  `stream_crc::StreamChecksum`, `stream_crc::extract_gzip_stream_crc`,
  `stream_crc::extract_bzip2_stream_crc`, `stream_crc::extract_xz_stream_check`,
  `stream_crc::extract_stream_checksum`, plus the same six names re-exported at the crate root.
  No new public module path was created, so the split is not observable from outside the crate.

### Decision: truncation is a `Format` error at every layer

When R0001-0075 landed it fixed the six-byte auto-detect probe to reserve `Format` for
`UnexpectedEof` and keep `Io` for every other error kind — but it left the per-format helpers
mapping *all* their read failures, `UnexpectedEof` included, to `ArchiveError::Io`. An identical
truncated file therefore reported a different variant depending on which layer caught it. That
was left alone on the grounds that `Io` is the safer direction.

**Decision: `UnexpectedEof` on a fixed-size framing read is `ArchiveError::Format` in every
layer of this module. Every other `io::ErrorKind` stays `ArchiveError::Io`, with its `operation`
label and its original `io::Error` source.**

Reasons, in the order they decided it:

1. Truncation is a property of the *bytes*, not of the machine. A stream that ends inside a
   structure the format mandates is malformed data by the same standard as a bad magic byte,
   which this module has always reported as `Format`.
2. `Io` is not in fact the safer direction. Callers treat `Io` as the operational,
   possibly-retryable class; classifying a permanently short file as `Io` invites a retry that
   can never succeed, which is the more damaging misreport. The genuinely conservative choice is
   the variant that tells the caller the data is wrong.
3. `Format` carries strictly more information here: the format tag (present once the container
   has been established, `None` before) and a message naming what was missing. `Io` names only
   the syscall label.
4. It makes the reported variant independent of which layer noticed — the property the split
   would otherwise have shuffled around rather than fixed.
5. The opposite direction would have to *regress* three landed decisions and fabricate error
   sources. The gzip length pre-flight (R0069-0078), the bzip2 too-small-for-EOS check, and the
   auto-detect probe (R0069-0079, R0001-0075) already report `Format` for short input, and none
   of them has an `io::Error` to attach; unifying on `Io` would mean inventing one.

Implemented as a single shared classifier, `framing_read_error` in `src/stream_crc/codec.rs`,
used by all four entry points, so the rule has exactly one definition. Its rustdoc states the
rule; the classifier itself is unit-tested for both arms.

**Observable change.** `extract_bzip2_stream_crc` on a stream shorter than its 4-byte header and
`extract_xz_stream_check` on a stream shorter than its 12-byte Stream Header now return `Format`
where they previously returned `Io`. The XZ case is the one a caller actually hits through
`extract_stream_checksum`: a file of 6 to 11 bytes passes the six-byte probe and dies inside the
Stream Header, which is precisely the window where the two layers disagreed. The gzip header and
trailer reads and the bzip2 tail read are bounded by a preceding length check, so their new
`Format` mapping is reachable only under concurrent truncation of the file being read; they are
mapped anyway so no path is left with the old classification.

**Deliberately not changed.** Only the `Io`-versus-`Format` classification was in scope. The
pre-existing format tags and messages are untouched, including the untagged (`None`)
"BZIP2 file too small to contain EOS marker" error, which by the module's own
tag-once-the-magic-matched convention could name `Bzip2`. New mapping sites do follow that
convention: `None` before the container is established, the format afterwards.

**Consequence.** `docs/investigation/codebase/stream-crc.md` describes the module as a single
file and records the old XZ behaviour ("a file between 6 and 11 bytes reaching it through
auto-detection fails as `io("read_header", …)` rather than a `Format` error") as a live
constraint. That sentence and the module-inventory entries in
`docs/investigation/codebase/lib.md`, `docs/investigation/architecture/module-structure.md`, and
`docs/implementation/module-map.md` are now stale and need a refresh pass; those documents were
outside this change's ownership.
