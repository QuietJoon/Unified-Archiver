---
type: Explanation
title: How SFX detection decides
description: Why self-extracting-archive detection is staged, why its verdict is an enum with an evidence trail, and what it refuses to claim.
tags: [sfx, decision, AD-0006, AD-0015, AD-0016, AD-0040]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-16T00:00:00Z
sources:
  - { id: sfx-detection, resource: src/sfx/detection.rs }
  - { id: sfx-signatures, resource: src/sfx/signatures.rs }
  - { id: sfx-result, resource: src/sfx/result.rs }
  - { id: sfx-stub-types, resource: src/sfx/stub_types.rs }
  - { id: sfx-limits, resource: src/sfx/limits.rs }
  - { id: ad-0006, resource: docs/records/AD-0006-staged-sfx-detection-pipeline.md }
  - { id: ad-0015, resource: docs/records/AD-0015-sfx-iterate-all-candidates-demote-cd-signature.md }
  - { id: ad-0016, resource: docs/records/AD-0016-rename-shellscript-add-unknown-stub.md }
  - { id: ad-0040, resource: docs/records/AD-0040-sfx-payload-size-ceiling.md }
  - { id: madr-0014, resource: docs/records/MADR-0014-r0001-sfx-probable-confidence-clamp.md }
synced_hash: 630a9da867d338b3c287b7584a6bcf82736072dcb4dfb4934a1d9c06ebcaa260
---

# How SFX detection decides

Deciding whether a file is a self-extracting archive means answering a question
about intent from bytes alone: was this executable *built* to carry an archive,
or does it merely contain a few bytes that look like archive magic? The answer
has to be cheap, because most files fed to a detector are not SFX archives at
all, and it has to be honest, because the only way to be certain is to open the
payload — which detection deliberately does not do.

## Three stages, cheapest first

The pipeline of AD 0006 is ordered by cost, and each stage exists to make the
next one rarer.

**Stage 1 classifies the executable stub** from a 4 KiB header window and rules
out everything that is not a plausible executable: text files, data files,
archives that are simply archives, and — since the R0070-0076 change —
arbitrary binaries that happen to contain archive magic somewhere. This stage is
infallible by construction: `StubType::detect` returns a variant rather than a
`Result`, and an `Unknown` classification short-circuits to a negative verdict
*before* the signature scan is paid for. The gate is cheap and it discards the
largest class of inputs, which is exactly what you want first.

**Stage 2 scans the 1 MiB prefix for archive signatures** and rules out
recognised executables with no archive magic near the front. It is one pass over
the buffer through a first-byte dispatch table built once from the signature
table, and it returns *every* match sorted by offset rather than the first one.

**Stage 3 screens each candidate offset with a format-specific structural probe**
and rules out coincidence. The ZIP arm wants a canonical compression method, a
filename length inside a sane bound, and a declared header extent that fits in
the remaining file; the gzip arm wants the deflate method byte and clear reserved
flag bits; the bzip2 arm wants a block-size digit followed by either the
compressed-block marker or the end-of-stream marker, so that a legitimately
*empty* member is not missed; the xz arm wants the reserved stream-flag byte to
be zero. These are shape checks on a handful of header bytes — a screen, not a
parse.

The remaining three arms are weaker than that description suggests, and the gap
is worth naming. RAR, RAR5 and 7z inspect no header fields at all: they require
only that enough bytes remain in the file behind the signature — twenty for
either RAR generation, thirty-two for 7z — so for three of the seven signature
table entries stage 3 rules out nothing beyond truncation. Their false-positive
control rests entirely on the length of the magic (six to eight bytes, against
ZIP's four and gzip's two) plus the stage-1 stub gate. That is a defensible
place to stop, since a coincidental seven-byte `Rar!\x1a\x07\x00` in a stub is
far less likely than a coincidental `PK\x03\x04`, but it is a different argument
from the one the ZIP and stream arms make, and adding real header probes there
is the obvious next increment.

**Payload validation is not a stage at all.** It is deferred to whoever opens the
payload. The function that performs stage 3 was renamed from "validation" to
"heuristic screening" (R0069-0075) precisely because the earlier name
over-promised, and the honest reading of a positive verdict is "worth opening",
never "known good".

AD 0006 rejected the obvious alternative — deep-parse every candidate format for
every input — on latency grounds. In any scanning workload the non-SFX inputs
dominate, and deep parsing charges them the full price of a negative answer.

One implementation detail follows from the staging rather than motivating it: all
three stages read a single buffer. Detection performs one read of up to the scan
window plus a 32-byte tail, then slices it — stage 1 takes the leading 4 KiB,
stage 2 takes the window with the tail excluded so the advertised 1 MiB bound
stays literally true, and stage 3's probes may reach into the tail.

## The verdict is an enum with a paper trail

Detection reports `SfxConfidence` — `NotSfx`, `Probable`, or `Confirmed` — plus
an `evidence: Vec<String>` naming what was seen. It used to report an `f32`.

The float never carried the information its type implied. Only two values were
ever produced in practice: rejections returned `0.0`, and a stage-3 pass returned
a clamped value just under `1.0`. Nothing in between was reachable. That had
three costs. Callers had to compare against thresholds whose meaning was invented
at the call site. A caller could pass `1.0` into the probable-result constructor
and manufacture something `is_confirmed()` would agree with — a foot-gun closed
by clamping the input range, which is the whole content of MADR-0014-r0001 and
which amounted to guarding a scale that carried no information. (Pre-consolidation
text cites that record as "AD 0014"; do not follow the citation literally — the
current `AD-0014` is an unrelated ruling about deferred password validation.) And a score names nothing: `0.9` told a caller how sure the detector
felt, not what it had actually found.

Innovation I3 (2026-07-22) replaced the score with the tri-state enum and the
evidence list, amending AD 0006 and superseding MADR-0014-r0001. The accept/reject
decisions did not change — stage 1 or 2 rejection maps to `NotSfx`, a stage-3
pass maps to `Probable`. What changed is that the reachable states are now named,
the invariant "a probable result is never confirmed" is structural rather than
enforced by arithmetic (the probable constructor cannot set `Confirmed` at all),
and the reason for a verdict travels with it. Evidence lines record the stub
kind, the format and offset, whether the structural probe passed, and whether the
winning match was a weak two-byte one — the things detection actually knows.

`Confirmed` is a target, not a state the live pipeline produces. It is reserved
for the confirmed-detection patterns that AD 0015 deferred and OI-0080-004 still
tracks, and it is reachable today only from a test-only constructor. That is a
real wart: a public variant that never appears, and a public `is_confirmed()`
predicate that is always false. The judgement behind keeping it is that
`SfxConfidence` is not `#[non_exhaustive]`, so adding the variant later would
break every downstream exhaustive match, whereas carrying it now costs one dead
arm and buys the freedom to land confirmed detection as a non-breaking change.
A reader who wants a guaranteed answer should not look for `Confirmed`; they
should open the payload and treat the open's error as the refusal.

The result type is shaped to keep this honest from the outside too. The struct is
`#[non_exhaustive]` with `pub(crate)` fields behind accessor methods, so no
external crate can fabricate a contradictory state — a negative verdict carrying
a format, or a `Confirmed` outside the test-only path — and no downstream code
can come to depend on the field layout.

## Every candidate is examined, and one signature was deleted rather than ranked

AD 0015 fixed two related failures. Detection checked only the first signature
found, so a single false positive earlier in the file hid the real payload behind
it. And the ZIP central-directory signature was in the signature table at all —
which is worse than it sounds, because a central directory legitimately appears
*before* a local file header when a scanner walks a concatenated file, so
first-match logic could lock onto an offset that is not the start of any archive.

The fix was to iterate every candidate in offset order and take the first that
passes its probe, and to remove the central-directory signature from the table
entirely. That is a stronger move than demoting its rank: the scanner no longer
looks for those bytes, so no ordering rule can ever hand them a win. TAR's
`ustar` marker got the same treatment for a different reason — it lives at a
fixed offset inside a TAR header, which is meaningless once the header is
embedded at an arbitrary offset. The accepted cost is that TAR SFX files, which
barely exist in the wild, are no longer detected. AD 0015 also considered a
scoring or ranking system for candidates and rejected it as more machinery than
the problem needed; with hindsight that rejection aged well, since the numeric
score elsewhere in this module is exactly what I3 later removed.

One ranking rule did prove necessary, and it is not offset order. A strong-magic
candidate — any passing candidate other than the two-byte gzip magic — beats
gzip regardless of position (R0079-0031). The reasoning is
that a two-byte magic plus a method byte and a flag check is nearly free to
satisfy, so an ordinary compressed resource embedded in a stub passes the gzip
probe by construction and would otherwise shadow the genuine payload sitting
behind it. Earliest offset remains the tiebreak within a strength class, and a
lone passing gzip candidate is still reported when nothing stronger exists.

## False-positive pressure, and where the cost is bounded

An executable is megabytes of dense, near-arbitrary bytes. The ZIP local-header
signature is four of them; the gzip signature is two. Incidental hits are the
expected case, not the exceptional one, which makes false-positive control the
central design problem here rather than a polish item.

Three defences carry that load, and none of them is expensive. The first is
requiring a recognised stub before the payload: a real SFX always has one, so
demanding it costs nothing legitimate and discards binaries whose only claim is
an accidental signature. The second is making the stage-3 probes
format-specific. They replaced a single "at least 100 bytes follow" length gate,
which is precisely what used to report ordinary executables carrying incidental
gzip, bzip2, or xz magic as probable SFX archives. That catch-all arm still
exists as a fallback for a format added to the signature table without a probe,
but all seven current table entries have explicit arms — three of them, as noted
above, still length-only — so it is unreachable today and kept deliberately as
defence in depth. The third is gating each
probe's minimum length on the bytes remaining in the *file* rather than in the
buffer (R0079-0043), with the 32-byte over-read past the window so a signature
found just inside the 1 MiB boundary can still be probed. Gating on the
truncated buffer produced false *negatives* at the boundary; gating on the file
removes them without widening the scan.

The cost side is bounded by two numbers that are independent of each other, and
conflating them is the usual mistake. The scan window bounds the *prefix*:
detection is one pass over at most 1 MiB regardless of file size, so a 4 GiB
installer costs the same detection as a small one, and the probes are
constant-size reads at candidate offsets. The consequence the reader has to
accept is a coverage gap — a payload whose signature sits past 1 MiB is reported
as not an SFX, silently, because a detector that reads further to be sure would
give up the property that makes it cheap enough to run on everything.

The genuinely expensive step is not detection but staging, and AD 0040 bounds
that separately: opening a payload copies the tail to a temporary file, and the
copy is capped at 16 GiB before the tempfile is created. AD 0040 is explicit that
this is a blast-radius bound and not an integrity check — a 15 GiB payload that
is corrupt is still copied in full before a backend rejects it. Detection bounds
the prefix; staging bounds the suffix; neither number constrains the other.

## The stub taxonomy, and why `Unknown` is in it

The taxonomy is `WindowsPE`, `LinuxELF`, `MacOSMachO`, `ScriptInterpreter`, and
`Unknown`. AD 0016 produced the last two names.

`ScriptInterpreter` was called `ShellScript`, which understated what shebang
detection covers: any `#!` interpreter, so Python, Perl, and Ruby stubs as much
as the makeself-style shell case that made the original name feel right.

`Unknown` exists to make classification total. Because it exists, stub detection
can be a pure infallible function — a header matching nothing yields a value
rather than an error — and "unrecognised executable" becomes an ordinary case the
pipeline branches on instead of an exceptional one it has to plumb.

Worth noticing is how the variant's role inverted. AD 0016 landed it as
future-proofing with detection behaviour deliberately unchanged, and OI-0027-001
was opened to track the idea of scanning *through* unknown stubs — that is, being
more permissive. What actually happened is the opposite: R0070-0076 made
`Unknown` the rejection that stops arbitrary binaries with incidental magic from
being classified as SFX. The variant added to allow a looser policy is what makes
the strict policy cheap.

Classification reads header fields and nothing else. An earlier implementation
handed the 4 KiB prefix to a general object-file parser, which walks section
tables and import data whose file offsets sit far beyond that window in any real
stub; the parse failed on the truncated prefix and every native SFX stub
classified as `Unknown` (R0079-0010). The replacement asks only what the prefix
can answer: PE needs the `MZ` magic plus a new-header offset that points at the
COFF signature inside the window; ELF needs its whole fixed header — a coherent
`e_ident` (`EI_CLASS` and `EI_DATA` each in {1, 2}, `EI_VERSION` 1) plus `e_type`
an executable image (`ET_EXEC` or `ET_DYN`), `e_version` `EV_CURRENT`, and an
`e_ehsize` matching the size the class implies (52 bytes for ELFCLASS32, 64 for
ELFCLASS64), with every multi-byte field read in the byte order `EI_DATA`
declares; and Mach-O needs either a thin magic whose fixed `mach_header` /
`mach_header_64` validates — `filetype` an executable image (`MH_EXECUTE` or
`MH_DYLIB`), a nonzero `ncmds` whose eight-byte-per-command minimum fits inside
`sizeofcmds`, and a `sizeofcmds` that fits in the window behind the header, the
matched magic fixing the byte order for all of them — or the fat magic followed
by a plausible architecture count, that last check being what keeps Java class
files, which share the fat magic, from classifying as Mach-O. R0001-0066 and
R0001-0067 widened the ELF and Mach-O checks past the magic for exactly the
reason stage 1 exists: four magic bytes, or an `e_ident` alone, let incidental
prefix bytes pose as an executable stub, and a prefix too short to hold the fixed
header now classifies as `Unknown` rather than as an executable. Shebang
recognition similarly requires a newline-terminated, non-empty interpreter path,
so a binary that merely begins with the two bytes `#!` is not a script stub.

One consequence of the API shape is worth naming. Detection takes a path and
opens it by name, so its decision is about bytes it read from an inode that the
later staging or stub read has to open again. The crate closes that gap by
binding the second open to the file identity captured at detection — device,
inode, and length on Unix, length alone elsewhere — and failing rather than
handing a backend bytes that were never detected. A descriptor handed across the
detection API would be tighter, and the accepted residual is a deliberate choice
rather than an oversight.

## What the pipeline will not claim

*Product identity.* It reports a stub kind and a payload format, never a builder:
not NSIS, not InstallShield, not a WinRAR SFX module. Fingerprinting installers
means maintaining a signature database against vendors who change their stubs on
their own schedule, for an answer no extraction step needs.

*Execution.* The stub is data. It can be handed to a caller for sandboxed
analysis, and nothing in the crate runs it on any platform.

*Payload validity.* Stage 3 inspects a header shape, and for RAR, RAR5 and 7z
only a remaining length. It does not decompress, does
not walk a central directory, does not check a CRC. A crafted file that satisfies
a stub check and one structural probe produces a positive verdict, and a real
archive can be corrupt behind a perfectly plausible header. `Probable` means the
file is worth opening; the open is the experiment.

*Recovery of an out-of-window payload.* If the signature is past the scan window,
detection reports nothing rather than searching harder.

## Applying this

The caller-facing sequence — detection, reading the verdict, opening or staging
the payload, and pulling out the stub — is in
[How to detect and open a self-extracting archive](../../../how-to/user/en/handle-self-extracting-archives.md).
