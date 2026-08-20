---
type: Explanation
title: What archive checksums actually prove
description: Discussion of the three kinds of checksum this crate exposes, what each one detects, and where the guarantees stop.
tags: [integrity, formats, decision, AD-0010, AD-0012, AD-0047, DCR-012]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-16T18:53:21Z
sources:
  - { id: stream-crc, resource: src/stream_crc.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: zip-backend, resource: src/ffi/zip_wrapper.rs }
  - { id: backend-trait, resource: src/backend.rs }
  - { id: stream-crc-guide, resource: docs/STREAM_CRC32.md }
  - { id: ad-0010, resource: docs/records/AD-0010-unified-stream-checksum-extraction.md }
  - { id: ad-0012, resource: docs/records/AD-0012-crc32-zero-is-valid.md }
  - { id: ad-0047, resource: docs/records/AD-0047-content-based-manifest-digest-on-crc-less-formats.md }
  - { id: madr-0006, resource: docs/records/MADR-0006-r051-manifest-digest-computes-crc32-from-content.md }
  - { id: dcr-012, resource: docs/records/DCR-012-content-digest-drops-per-path-occurrence-ordinal.md }
synced_hash: 3a6bfb4581b17cdff2f6b37729a26e60299feb0d5d7243280c70b2f0a2a30010
---

# What archive checksums actually prove

Three unrelated mechanisms in this crate get called "the checksum", and they
answer three different questions. Conflating them is the most common way to
believe an archive has been verified when it has not.

## Three different things

**The per-entry content CRC32.** ZIP, 7z, and RAR store a 32-bit CRC of each
entry's *uncompressed* content in their own metadata — the ZIP central
directory, the 7z files header, the RAR file header. The crate surfaces it as
`ArchiveEntry::crc32`, read straight out of that metadata. Nothing is
decompressed to produce it, which means a listing tells you what the creating
tool claimed, not what the payload currently is. The value only becomes
evidence when something decodes the entry and compares.

**The container-level stream check.** gzip, bzip2, and xz compress one stream
and record one check over the whole uncompressed payload. gzip puts a CRC32 and
an `ISIZE` in an 8-byte trailer; bzip2 folds its per-block CRCs into a stream
CRC after the end-of-stream marker; xz declares its check in the stream header
and can use CRC32, CRC64, SHA-256, or nothing at all. This is a per-container
check, not a per-file one — these formats have no notion of multiple entries,
which is why a `.tar.gz` gets one check for the whole tarball and none for the
files inside it.

**The manifest digest this library computes.** Neither of the above answers "are
these two archives the same?", so the crate derives a value that does:
`Archive::calculate_manifest_digest` collects one CRC32 per file entry, sorts
the hex encodings, joins them, and hashes the result. It is computed on demand
and stored nowhere. Its input is deliberately the *content multiset* and nothing
else — no paths, no sizes, no timestamps, no permissions, no compression method,
and no exceptions. Every file entry contributes exactly eight lowercase hex
characters, first occurrence or twentieth, and multiplicity is carried by
repeating that element, so two copies of one payload still do not digest as one
copy. That is why the historical name is misleading: a manifest is very nearly
what it does not cover. `calculate_archive_crc` is a cheaper cousin, the
wrapping arithmetic sum of the stored CRC32 values, which is what 7-Zip displays
as the archive CRC.

There used to be one exception, and removing it is the more instructive story.
When the same entry path appeared more than once, every occurrence from the
second onward carried its zero-based occurrence ordinal into the hashed input as
`<crc32-hex>#<ordinal>` (review item R0079-0028). That ordinal was never a
feature; it was a compensating control. The resolver that materialised CRC32
values for CRC-less formats walked *by path*, so it re-hashed the first
occurrence once for every duplicate, and the ordinal at least kept the resulting
digests from colliding. Once that resolver was re-keyed to each entry's stable
listing id — so every occurrence hashes its own payload — the ordinal encoded
archive *layout* and nothing else, which is the one thing this digest exists not
to carry. DCR-012 removed it. Unique-path archives are unaffected, because
ordinal 0 already emitted the bare hex; duplicate-path archives changed digest
value once, which is the practical consequence
[How to verify an archive's integrity](../../../how-to/user/en/verify-archive-integrity.md)
spells out.

Which formats carry which of these is tabulated in
[Format support matrix](../../../reference/user/en/format-support-matrix.md). The
calls themselves are covered in
[How to verify an archive's integrity](../../../how-to/user/en/verify-archive-integrity.md).

## What each one actually detects

A per-entry CRC32 detects that one entry's decoded bytes differ from what the
creating tool measured. That is a genuinely useful guarantee: bit rot on disk,
a truncated transfer, a corrupted compressed block. It is checked entry by
entry, so a report can say *which* files went bad. It is also only 32 bits and
not cryptographic — random corruption slips through roughly once in four
billion, and deliberate corruption slips through whenever the attacker bothers.

A container-level check covers the whole payload of one stream in one value. It
catches the same class of accidental damage, at whole-file granularity — you
learn the stream is bad, not which part of it. And there is a subtlety specific
to how this crate exposes it. AD 0010 chose to *read the stored check out of the
header or trailer* rather than decode the stream, explicitly rejecting the
alternative of leaning on libarchive's verification because a full stream read
negates the entire point of a fast check. So `extract_stream_checksum` and its
per-format siblings retrieve a claim. Comparing that claim against a value you
recorded earlier is meaningful; comparing it against itself is not. Actual
verification requires decoding the bytes, which in this crate means opening the
file as an `Archive` and letting `validate_integrity` push it through the codec.

AD 0010's other consequence is visible in the xz helper: parsing trailers by
hand was accepted as the cost, and extracting xz's real check value — which
lives in the blocks and index rather than the header — was left undone. The
helper therefore reports which check type an xz stream *declares* and returns no
value at all, even when the declared type is CRC32. A declaration is not a
measurement, and for the `None` check type there is not even anything to
declare.

The manifest digest detects that two archives hold a different set of file
contents. Because it sorts, it is indifferent to entry order whenever every
entry path is distinct; because it hashes per-entry identities rather than
summing them, it survives the swap that defeats `calculate_archive_crc` —
exchanging two entries' CRC32 values leaves an arithmetic sum untouched.

## What none of them detect

**Reordering.** For the digest this is by design, and since DCR-012 it is
unconditional: order-independence is the feature that lets a ZIP and a 7z of the
same files match, and it now holds for duplicate-path archives too. Re-listing
one archive in a different order cannot move its digest, whatever its paths look
like, because the sort sees CRC32 values and nothing else. "Same digest"
therefore cannot mean "same archive". For `calculate_archive_crc` the
insensitivity is worse, because addition also loses which value belonged to which
entry.

**Metadata drift.** Paths, names, directory structure, permissions, modification
times, and archive comments are outside every mechanism here. An archive whose
files have all been renamed digests identically to the original. A gzip member
whose stored filename, timestamp, and comment are rewritten keeps its trailer
CRC32 unchanged, because the trailer covers the decompressed payload and not
the header that describes it.

**Partial coverage of concatenated containers.** gzip files may be a sequence of
members and xz files a sequence of streams. The gzip helper reads only the
trailer at the end of the file, so on a multi-member input it describes the last
member; the xz helper reads only the first stream header, so it describes the
first stream's check type. Neither limitation is detectable from the returned
value — it looks like an answer about the whole file.

**Deliberate tampering.** CRC32 is an error-detecting code designed against
noise. It is unkeyed and linear, and it sits inside the same file as the data it
covers. Anyone who can edit a payload can recompute its CRC in the same pass,
and anyone who can rewrite a gzip trailer can make a corrupt stream internally
consistent. The digest inherits this completely: it is a function of values an
attacker controls.

**A blind spot that was closed, and what closing it cost.** AE-2 AES-encrypted
ZIP entries store `0` in the CRC32 field by specification, relying on the AES
authentication tag for integrity instead. Extraction and integrity testing always
handled that by exempting such entries from the CRC comparison while still
streaming them, so the authentication check runs. The digest path did not: the
listing reported the stored `0`, so every AE-2 entry contributed the CRC32 of
nothing and two AES-encrypted ZIPs with entirely different contents produced the
same digest. The listing now reports no CRC32 for those entries at all. The gate
is a three-way conjunction — the entry is encrypted, its stored CRC32 is `0`, and
its WinZip-AES extra field declares vendor version AE-2 — never the value on its
own, so AD 0012's rule survives untouched. Each conjunct earns its place: without
the encryption flag a plaintext empty file would be swept in, and without the
AE-2 vendor check so would an *encrypted* empty file, whose stored `CRC32(b"")`
of `0` is a genuine checksum the archive really carried. Both still list
`Some(0)`. Only the AE-2 placeholder — a zero that never described any content —
is treated as absent, and the digest walk then streams that entry's decrypted
payload and folds its real CRC32.

Two consequences follow from that, and both are new. Digesting an AES ZIP is no
longer a metadata-only walk: it decrypts and reads every entry, so it needs the
password and costs a full pass over the payload, and a handle opened without one
now fails the call instead of returning a worthless answer. And any digest
recorded for an AES ZIP before the change was a fold over placeholders, so it
will not match the value computed now.

Making those entries stream had one non-obvious requirement. A ZIP whose
central directory holds two names that normalise to one path — `sub/a.txt` and
`sub\a.txt` — must still hash each occurrence's own payload, which a walk keyed
on the *path* cannot do: it would find the first match twice. The ZIP backend
therefore addresses these reads by the entry's stable listing id, the same
mechanism the CRC-less formats use, and the by-path route it would otherwise
have fallen back to refuses an ambiguous name outright rather than aliasing.
That is the general shape of the design: where a name cannot identify a payload,
the code either seeks by id or refuses — it never guesses.

## Why a stored CRC32 of zero had to be a real value

CRC32 of the empty string is `0x00000000`. All three native backends — ZIP, 7z,
and UnRAR — independently treated a stored `0` as "no checksum available" and
set `crc32` to `None`. AD 0012 records what that cost: empty files vanished from
manifest digests, integrity checks skipped them entirely, and the ZIP integrity
walk skipped any entry whose stored CRC happened to be zero. Every empty file in
every archive was silently unverified, and any pair of archives differing only
in empty files digested the same.

Two alternatives were weighed and rejected. A sentinel value such as
`u32::MAX` for "absent" was judged fragile — it re-creates the same collision
one value over. An `Option<Option<u32>>` encoding of unknown / absent / present
was judged over-engineered for what the formats actually need. The accepted rule
is blunt: a file entry always carries the CRC32 its metadata holds, and only
directory entries get `None`.

The same principle governs the derived values. A `calculate_archive_crc` result
of `0` is a valid sum, not a sentinel, which is precisely why the crate
documents it as three-way ambiguous — empty archive, no CRC-bearing entries, or
a sum that genuinely wrapped to zero — instead of pretending zero means
"nothing here".

There is a counter-pressure that arrived later and is worth stating, because it
looks like a reversal and is not. On 7z the per-entry digest is *optional*: the
underlying reader's `crc` field defaults to `0` when the archive omits it, so
reading the value unconditionally produced `Some(0)` for entries that genuinely
have no checksum — directories and no-stream empty files — and made verified
extraction fail on valid data. The fix was to gate on the format's presence
flag rather than on the value. That is the durable lesson of AD 0012: absence
must be learned from a presence bit in the format, never inferred from the
number being zero.

## Why CRC-less formats get a content-derived digest

TAR and its gzip/bzip2/xz wrappers, and ISO, expose no per-entry CRC32 through
libarchive. The digest originally handled them with a `"{path}:{size}"`
surrogate, and AD 0047 spells out why that was untenable: rebuild a TAR with an
identical file list and edit one byte inside one file, and the digest does not
move. The method advertises content identity and was silently delivering layout
identity on the formats where the fallback always fired.

Three options were on the table. Keeping the fallback was rejected as failing
the contract on the most common CRC-less formats. Excluding those formats from
digest support at all — considered in MADR-0006 — would have made the digest
useless for exactly the archives people most want to deduplicate. Upgrading to
SHA-256 per entry was rejected for the version in question as more expensive
than the job requires, and as a separable decision. The accepted option streams
each CRC-less entry through a CRC32 hasher, one entry at a time, so the memory
profile stays bounded and no format-specific branching is needed.

The cost was real and was accepted with open eyes. Under libarchive each entry
was originally reached by reopening the container and decompressing everything
before it, so digesting a compressed TAR was quadratic in entry count; AD 0047's
own consequence section says so. That much has since been repaid: the libarchive
backend now resolves every CRC-less entry in a single traversal (OI-0001-009), so
a compressed TAR is decompressed once rather than once per member. What does not
go away is the trade-off in one line — correct content identity on CRC-less
formats, paid for in decompression. Every payload is still read, there is still
no progress signal, and `calculate_archive_crc` is still the cheap answer when a
fingerprint will do.

Two refinements followed. MADR-0006 accepted a helper so callers could ask in
advance whether the digest would be metadata-only or would need to read
payloads; that predicate never actually shipped (MADR-0006's 2026-08-07
amendment records the evidence), so today the cost has to
be inferred from the format. MADR-0008 removed the last silent fallback: when a
per-entry read fails, the error propagates instead of substituting a surrogate.
The reasoning generalises well beyond digests — a value that can silently
degrade to a different semantics is worse than an error, because no caller can
tell which of the two they received. Present-day behaviour follows that rule
strictly, including a hard byte cap per entry so a hostile or buggy decoder
cannot feed unbounded bytes into the hasher and pass it off as content.

Perspective: the decision to stay with CRC32 rather than move to a
cryptographic hash is the right call for what the digest is *for* — cheap
equality matching across formats for deduplication — and it is also exactly
what disqualifies the digest as a tamper check. It is worth being blunt about
that rather than letting the word "digest" imply strength it does not have.

## Why none of this substitutes for a signature

Every mechanism described here lives inside the file it describes, and none of
them is keyed. That is the whole argument, and it does not depend on hash
strength. A CRC32 is trivially recomputed by whoever edits the data. Even xz's
strongest option, SHA-256, is an unkeyed digest of the payload: it detects
modification only if you obtained the expected value through a channel the
modifier does not control. RAR5's optional BLAKE2sp-256 per-file hash is
cryptographically sound and equally powerless on its own, for the same reason.

So the honest reading of a successful integrity check is narrow: the bytes match
the values stored alongside them, and the archive is internally consistent. That
rules out disk corruption, truncated downloads, and bad media. It says nothing
about who produced the archive or whether its contents are what you asked for.
This crate does not sign archives and does not verify signatures. If provenance
matters, sign the archive file itself out of band and verify that signature
before you trust anything the archive tells you about itself — including its own
checksums.
