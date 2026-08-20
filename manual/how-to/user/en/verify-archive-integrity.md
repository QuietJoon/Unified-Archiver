---
type: How-To Guide
title: How to verify an archive's integrity
description: Recipes for validating an archive with validate_integrity, verify_crc32, the standalone stream-checksum helpers, and the content digests.
tags: [integrity, extraction, streaming, api]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-16T18:53:21Z
sources:
  - { id: inspection, resource: src/inspection.rs }
  - { id: stream-crc, resource: src/stream_crc.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: options, resource: src/options.rs }
  - { id: zip-backend, resource: src/ffi/zip_wrapper.rs }
  - { id: example-stream-checksum, resource: examples/stream_checksum.rs }
  - { id: backend-trait, resource: src/backend.rs }
  - { id: dcr-012, resource: docs/records/DCR-012-content-digest-drops-per-path-occurrence-ordinal.md }
synced_hash: 927bf23a7e1afbfee075bde623bafaa58f18fb04d0008d5068b917a62a1a02e4
---

# How to verify an archive's integrity

There is no single "verify" call. The crate offers five distinct integrity
operations, each answering a different question, and each with different
per-backend behaviour. Pick the one that answers your question and read its
result with the caveats below.

The four `Archive` operations below — `validate_integrity`, extraction with
`verify_crc32`, `calculate_manifest_digest`, and `calculate_archive_crc` — need a
read-mode handle (`Archive::open`, or `Archive::open_encrypted` for an encrypted
archive); a write-mode handle fails them with `ArchiveError::WriteModeOnly`.
`Archive` is `Send` but not `Sync`, so verify from one thread per handle. The
standalone stream-checksum helpers take a path and need no handle at all.

Every error variant named here is described in
[Errors and warnings](../../../reference/user/en/errors-and-warnings.md). For the
signatures and re-export paths, see
[Public API surface](../../../reference/user/en/public-api-surface.md). For why
these guarantees stop where they do, see
[What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md).

## Test every file entry in the archive

`Archive::validate_integrity` walks the archive, decodes each regular file
entry's payload, and reports which entries did not survive the walk. Nothing is
written to disk.

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.zip")?;
let report = archive.validate_integrity()?;

if report.failed.is_empty() {
    println!("{} of {} file entries verified", report.validated, report.total_files);
} else {
    for path in &report.failed {
        eprintln!("damaged: {path}");
    }
}
```

Branch on `failed`, but test `total_files` before you treat a clean report as a
pass: `validated` is computed as `total_files - failed.len()`, so an empty
`failed` on an archive with no file entries means nothing was checked. Only
entries whose type is `File` are candidates, so `total_entries - total_files`
counts the directories, symlinks, and hard links every backend skips. All four
fields and their types are listed in
[Public API surface](../../../reference/user/en/public-api-surface.md).

Two things to settle before you act on a result:

- **`Err` and a non-empty `failed` are different answers.** A non-empty `failed`
  means those entries are damaged. An `Err` means the check could not be
  completed and there is no per-entry verdict: an I/O failure on the archive file
  itself, a missing or wrong RAR password, or a backend reporting more failures
  than there are file entries (which returns `ArchiveError::Corruption` rather
  than saturating the subtraction).
- **What a `failed` entry proves depends on the format.** ZIP compares each
  entry's computed CRC32 against the stored central-directory value; 7z and
  RAR/RAR5 let their own decoders check integrity as they decode (UnRAR through
  its test mode, so nothing is written to disk); libarchive-backed formats carry
  no per-entry CRC32 at all, so a failure there means the decompressor rejected
  the bytes rather than that a stored checksum did not match. AE-2 AES ZIP
  entries are exempt from the CRC comparison — their stored value is `0` by
  specification — but are still streamed, so the AES authentication check runs
  and a failure shows up as a failed entry. What each of these mechanisms does
  and does not prove is discussed in
  [What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md).

## Ask for per-entry CRC32 during extraction

`ExtractionOptions::verify_crc32` defaults to `false`. Setting it does *not*
uniformly turn verification on and off — read the per-backend truth before you
rely on it.

```rust
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("release.zip")?;
archive.extract_all(ExtractionOptions {
    destination: "out".into(),
    verify_crc32: true,
    ..Default::default()
})?;
```

Two constraints decide whether you can set it at all:

- On libarchive-backed formats (TAR family, ISO, raw compressed streams) `true`
  is rejected with `ArchiveError::Unsupported` before any I/O runs, so pass
  `false` there. The wrapping codec's own check still runs during extraction.
- On 7z and RAR/RAR5 the decoders verify CRC32 whether or not you ask, so `false`
  switches nothing off. ZIP is the only backend the flag changes, and only on the
  disk-writing paths: ZIP's `extract_to_memory` and `extract_to_stream` compare
  against the stored value regardless of the flag (AE-2 placeholder `0` values
  excepted either way).

The flag is validated up front on every extraction entry point, so an unsupported
request fails before anything is written. The per-backend table is in
[Options and defaults](../../../reference/user/en/options-and-defaults.md).

A mismatch surfaces as `ArchiveError::Corruption` whose details read
`CRC32 mismatch: expected <hex>, got <hex>`, naming the entry path. Extraction
stops there; already-written entries stay on disk.

## Read the container checksum of a standalone .gz, .bz2, or .xz

For a single-file compressed stream you can read the checksum the format stores
in its own header or trailer, without decompressing and without opening the file
as an `Archive`. Use `extract_stream_checksum` to dispatch on content, or call
the format-specific helper directly.

```rust
use unified_archive::stream_crc::{CheckType, extract_stream_checksum};

let sum = extract_stream_checksum("backup.sql.gz")?;
match sum.check_type {
    CheckType::Crc32 => println!("stored CRC32: {:08X}", sum.crc32_value().unwrap()),
    CheckType::Crc64 => println!("stream declares CRC64"),
    CheckType::Sha256 => println!("stream declares SHA-256"),
    CheckType::None => println!("stream declares no check"),
    CheckType::Unknown => println!("unrecognised check id"),
}
```

`extract_stream_checksum` detects the format from magic bytes only; the file
extension is never consulted, so a renamed file is routed correctly and a file
whose magic matches nothing is rejected with a format error. It accepts gzip,
bzip2, and xz; anything else is an error, including a file too short to sample
six magic bytes.

Prefer the accessors over the raw fields. `StreamChecksum::crc32_value` returns
the CRC32 only when `check_type` is `Crc32`, and `crc64_value` only when it is
`Crc64`; the plain `crc32` / `crc64` / `uncompressed_size` fields are an option
bag that can hold a value the declared check type does not vouch for.

What each helper actually retrieves:

- `extract_gzip_stream_crc` validates the fixed header (magic, deflate method,
  reserved flag bits clear), then reads the 8-byte trailer: `check_type` is
  `Crc32`, `crc32` is the stored value, and `uncompressed_size` is the trailer
  `ISIZE` — the size **modulo 2^32**, so exact only below 4 GiB. Only the
  trailer at the end of the file is read, so on a multi-member gzip (from
  `pigz`, `bgzip`, or `cat a.gz b.gz`) the values describe the **last member
  only**.
- `extract_bzip2_stream_crc` validates the `BZh` header and its block-size
  digit, then searches the last kilobyte for the end-of-stream marker, first
  byte-aligned and then at every bit offset, because bzip2 blocks are bit-packed
  with no alignment padding. `check_type` is `Crc32` and `crc32` is the combined
  stream CRC read big-endian from just after the marker. `uncompressed_size` is
  always `None` — bzip2 does not record it. On concatenated streams the value
  comes from the **last** marker found.
- `extract_xz_stream_check` reads only the 12-byte stream header, rejecting it
  unless the reserved flag bits are clear and the stored flags CRC32 matches.
  It reports the declared check type — `None` (id 0x00), `Crc32` (0x01),
  `Crc64` (0x04), `Sha256` (0x0A), or `Unknown` for any other id — and returns
  `crc32`, `crc64`, and `uncompressed_size` all as `None`. **No check value is
  extracted for xz at all**, not even when the type is `Crc32`; obtaining it
  would require parsing blocks and the index. Because only the first stream
  header is read, a multi-stream `.xz` reports the **first stream's** type only.

None of these calls recompute anything. They tell you what the file claims about
itself, which is useful for comparing against a value you recorded earlier, and
useless for detecting a consistently rewritten file. To have the bytes actually
checked, decode them: open the file as an `Archive` and call
`validate_integrity`, which makes libarchive verify the container check while it
decompresses.

## Compare two archives by content

`Archive::calculate_manifest_digest` returns an 8-character lowercase hex digest
over the archive's per-entry CRC32 multiset. Two archives with the same file
contents produce the same digest even in different formats, with different
compression, different entry order, and different file names — the digest carries
no path, size, timestamp, or permission data at all, and no exception to that
remains.

```rust
use unified_archive::Archive;

let a = Archive::open("backup.zip")?;
let b = Archive::open("backup.7z")?;
if a.calculate_manifest_digest()? == b.calculate_manifest_digest()? {
    println!("same file contents");
}
```

Points to handle in your code:

- An archive with no file entries returns the **empty string**, not a digest of
  nothing. Compare for emptiness before treating two results as equal.
- On formats that store no per-entry CRC32 (TAR and its gzip/bzip2/xz wrappers,
  ISO), each file entry's payload is streamed through a CRC32 hasher so the
  digest stays a content hash rather than a path-and-size surrogate. Entries are
  streamed by their stable listing id, so an archive that legitimately repeats a
  path hashes each occurrence's own payload.
- That streaming has a cost, though less than it once did. The libarchive
  backend resolves every CRC-less entry in a single traversal, so a compressed
  TAR is decompressed once rather than once per member — linear in the archive's
  bytes, not quadratic in its entry count. It still reads every payload and still
  reports no progress, so use `calculate_archive_crc` when a cheap fingerprint
  suffices.
- Each streamed entry is bounded by a hard cap: the listing's declared size, or
  1 GiB when the listing declares none. A decoder that emits more than that
  fails the call rather than silently feeding extra bytes into the digest.
- Any per-entry read error propagates. There is no fallback digest — you get a
  trustworthy value or an error, never a degraded one. In particular, a
  CRC-less encrypted entry cannot be digested without a password.
- Order-independence is unconditional. A repeated entry path contributes one
  ordinary element per occurrence — no ordinal, no suffix, nothing derived from
  the path — so re-listing one archive in a different order cannot move its
  digest. Multiplicity still counts: two copies of one payload do not digest as
  one copy.
- **Digest values for duplicate-path archives changed once (DCR-012).** The
  hashed input used to carry a zero-based per-path occurrence ordinal
  (`<crc32-hex>#<ordinal>`) from the second occurrence of a repeated path on.
  Unique-path archives are byte-for-byte unaffected, because ordinal 0 already
  emitted the bare hex; a baseline you recorded for an archive that repeats a
  path has to be recomputed against the current code.
- **An AES-encrypted ZIP needs its password to be digested at all.** AE-2 entries
  store `0` in the CRC32 field by specification, so the listing reports no CRC32
  for them and the digest has to decrypt and read each payload — it is no longer
  a metadata-only walk on those archives. Open the handle with
  `Archive::open_encrypted`. On a handle opened without a password the call fails
  with `ArchiveError::Format` (format `Zip`) whose message ends
  `Password required to decrypt file`; that variant is the wrong one — a missing
  password should raise `ArchiveError::Password` — and it is a known mislabel on
  the ZIP no-password read path, not a statement about the archive. Digests you
  recorded for AES ZIPs before this change were folds over the placeholder `0`
  and will not match either.

  An AES ZIP is also the one case where a CRC-carrying format pays payload
  cost: those entries are decrypted and read in full, by random-access seek into
  the central directory rather than by a sequential walk. Duplicate paths are
  handled — an AES ZIP holding two names that normalise to one path (`sub/a.txt`
  and `sub\a.txt`) hashes each occurrence's own payload, because the read is
  addressed by listing id and not by name.

If you also want the total uncompressed size, call
`calculate_content_multiset_digest_and_size`, which returns
`(String, u64)` from a single walk instead of two.
`calculate_manifest_digest` and `calculate_manifest_summary` are thin shims over
it; the "manifest" name is historical and misleading, since no manifest data
participates.

## Get the cheap archive-level CRC

`Archive::calculate_archive_crc` is the wrapping arithmetic sum of every entry's
already-stored CRC32 — the value 7-Zip shows as the archive CRC. It reads
listing metadata only and decodes nothing.

```rust
use unified_archive::Archive;

let archive = Archive::open("file.7z")?;
println!("archive CRC: {:08X}", archive.calculate_archive_crc()?);
```

Entries whose listing carries no CRC32 contribute nothing to the sum, so a ZIP
and a TAR.GZ holding identical files do not agree. Comparing two archives with
this value is only meaningful when every entry of both exposes a stored CRC32 —
true for ZIP, 7z, and RAR, with AE-2 AES ZIP entries as the exception: their
stored `0` is a placeholder, the listing reports no CRC32 for them, and an
AES ZIP therefore sums only whatever plaintext entries it holds.

A returned `0` is ambiguous three ways and the call cannot distinguish them: the
archive is empty; no entry exposed a CRC32 so nothing was summed; or the sum
genuinely wrapped to `0`, which is a valid CRC32 and a valid sum. Use
`calculate_manifest_digest` when you need to tell "nothing checksummable" apart
from a real zero. Note also that addition is lossy — swapping two entries' CRC32
values leaves the sum unchanged.

## Before you report a result as "verified"

None of these calls prove authorship. A checksum that matches only tells you the
bytes match the value stored next to them, and anyone who can rewrite the bytes
can rewrite the checksum. Which call proves what, which kinds of damage each one
catches, and where every one of these guarantees stops is set out in
[What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md).
Read that before you promise a caller that an archive is intact.
