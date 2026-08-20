---
type: Explanation
title: The extraction safety model
description: What unified-archive defends against when it extracts an archive, why the gates run before the first byte is written, and what the model deliberately leaves to you.
tags: [security, extraction, decision, AD-0003, AD-0066, AD-0040]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: security-md, resource: SECURITY.md }
  - { id: security-rs, resource: src/security.rs }
  - { id: extraction-rs, resource: src/extraction.rs }
  - { id: ad-0003, resource: docs/records/AD-0003-safety-gates-before-extraction.md }
  - { id: ad-0066, resource: docs/records/AD-0066-r0076-sanitize-vs-reject-policy-deferred.md }
  - { id: madr-0026, resource: docs/records/MADR-0026-r0057-sanitize-entry-path-side-effect-removal.md }
  - { id: ad-0040, resource: docs/records/AD-0040-sfx-payload-size-ceiling.md }
synced_hash: 362c8109be50e7906b06ed7e1fd4fcbdf04f2b9ab4de0a1ffa08d039d41c262e
---

# The extraction safety model

An archive is a list of names, sizes, and payloads written by someone else. An
extractor that trusts that list hands a stranger the ability to choose file paths
on your disk and the number of bytes you write. Every gate described here exists
to take one of those choices back.

## The threat model

Five hostile shapes drove the design, and they are the ones the crate is built to
refuse.

**Path traversal, or "zip slip".** An entry named `../../../etc/cron.d/backdoor`
or `/etc/passwd` turns extraction into arbitrary file write. The classic exploit
needs nothing more than a name; the archive's payload is ordinary. This is the
oldest and most damaging of the five, and it is why entry names are treated as
untrusted strings rather than paths.

**Expansion bombs.** A small archive that decompresses to an enormous tree. The
canonical example, `42.zip`, is 42 KB of input and roughly 4.5 PB of output. Disk
exhaustion is the visible failure; the interesting property is that nothing about
the archive is malformed, so a parser cannot detect it. Only a policy about
acceptable sizes and ratios can.

**Entry-count floods.** An archive carrying millions of tiny records. Even if the
total decoded size is modest, the per-entry work — allocation, path resolution,
`stat` calls, file creation — is enough to hang a process. This is a distinct
threat from size, and it needs its own ceiling.

**Link-based escapes.** A symbolic link entry pointing at `/`, followed by an
entry writing "into" that link. Recreating the link faithfully hands the archive a
way out of the destination directory even when every entry name is well-formed. A
hard link entry does the same to an existing inode. There is also the mirror
case: a symlink that already exists inside the destination, planted by something
other than the archive.

**Oversized self-extracting payloads.** Opening an SFX archive means finding an
embedded archive inside an executable and copying that tail out to a temporary
file so a normal backend can read it from offset zero. Without a ceiling, a
crafted or merely malformed stub could make that copy write tens or hundreds of
gigabytes to your temporary volume before any backend had a chance to say the
bytes are not an archive at all.

## Why the gates run first

AD 0003 settled the central structural question: the safety policy lives in the
orchestrator, and it runs to completion before any backend is asked to write
anything.

The alternative — let each backend enforce its own limits, close to where it
already knows the sizes — was considered and rejected. The reason is not
performance but drift. With four extraction backends (the `zip` crate,
`sevenz-rust2`, libarchive, and UnRAR), per-backend enforcement means four
implementations of the same policy, four places to audit, and a fifth
implementation to write and review every time a format is added. Divergence is not
hypothetical: the same review series that produced these records repeatedly found
one backend enforcing a bound that another silently skipped.

Running first, rather than during, buys a specific guarantee: a rejected archive
leaves no partial output. The bomb and ratio gates complete before the destination
directory is even created, so refusing a bomb costs you nothing to clean up. That
guarantee is what makes "refuse" a usable answer rather than a mess.

The cost is real and worth naming. Deciding before writing requires knowing the
whole entry list first, which means:

- **A listing pass.** Metadata is read and materialised before extraction begins.
  For formats whose table of contents is central and cheap (ZIP, 7z) this is
  nearly free. For streaming formats it is a genuine extra traversal.
- **Memory proportional to the entry count.** The entry list is a real `Vec` in
  memory, which is precisely the resource an entry-count flood attacks. The
  mitigation is a second, earlier gate: the listing itself is budgeted, so a
  parse can abort once it crosses the caller's entry ceiling instead of allocating
  the full list first. The residual limitation is honest and documented in the
  code: only the streaming backends (libarchive, UnRAR) can actually stop reading
  early. The `zip` and `sevenz-rust2` libraries materialise their tables of
  contents when the handle is constructed, so for those formats the budget bounds
  only the crate's own list, and the post-listing gate remains the real cap.
- **Declared sizes, not measured ones.** The preflight sums what the archive says
  each entry contains. An archive that under-reports gets through the preflight.
  This is why the ceilings are also plumbed down into the extraction loops, and
  why the coverage there is uneven — libarchive tallies decoded bytes against the
  cumulative total as it writes, while the ZIP and 7z paths bound each entry to
  its declared size, which stops a per-entry overrun but does not police a
  cross-entry total when the metadata lies.

A related consequence of the same "know before you write" stance is that overwrite
and namespace conflicts are also decided up front: an archive containing both a
file `a` and a file `a/b`, or two entries resolving to the same output path, is
refused at the preflight rather than discovered halfway through with output
already on disk.

## Why paths are repaired rather than rejected

The path pipeline strips every component that is not a plain name: `..`, a root or
drive prefix, and `.` are dropped, so `../../etc/passwd` becomes `etc/passwd` and
lands under your destination. An entry consisting of nothing but such components is
rejected outright, since there is no name left.

That is repair, not rejection, and the choice is deliberate. AD 0066 ratified it as
the v0.3 baseline after a review recommended the opposite. The argument that won
was compatibility with reality: the overwhelmingly common source of traversal
components is not an attacker but a careless `tar c ..`, and the repaired form is
still safe — the rewritten entry cannot escape the destination. Making the default
strict would break "extract this archive from the wild" for a large number of
callers in exchange for a posture improvement rather than a new guarantee.

The cost, also recorded in AD 0066, is that repair hides intent. `../../etc/passwd`
and a legitimate `etc/passwd` become indistinguishable by the time you see the
output, and if both appear in the same archive they collide — surfacing to you as a
generic duplicate-output error rather than as evidence of a hostile name. The
component-level stripping is also not a general path normaliser: `a/../b` becomes
`a/b`, not `b`, because each component is judged on its own.

A `reject_unsafe_paths` flag exists on `ExtractionLimits`, defaulting to `false`,
to opt into strict rejection. Its status should be read precisely: AD 0066's
amendment records that
the field is now a first-class validated part of the limits struct, and that the
*behaviour* remains deferred. The path normaliser does not yet branch on the flag.
So the flag is a settled interface waiting for an implementation, not a switch you
can currently rely on. That wiring is tracked as OI-0076-003 item 1.

Layered under the name rewriting is a check that does not depend on the archive's
honesty at all. Because a name can no longer contain traversal components after
normalisation, the remaining escape route is a symlink that already exists inside
the destination. So the resolved output path's deepest existing ancestor is
canonicalised and required to sit under the canonicalised destination, and the
output path itself is rejected if it turns out to be a symlink. MADR-0026 is the
record behind the shape of that check: an earlier version created parent
directories as a side effect of "sanitising" a path, which meant a failure
anywhere later left orphan directories behind. The sanitiser was made
side-effect-free, and the ancestor walk exists because canonicalisation needs
*something* that exists to resolve. That record also notes why the remaining hole
— an archive planting a symlink in a directory it creates itself — is moot: link
entries are never recreated.

## Why links are skipped, not recreated or refused

Symlink and hard-link entries produce neither a file nor an error. They are
skipped, and each one becomes an `ArchiveWarning` in the returned
`ResultWithWarnings`.

Both of the obvious alternatives are worse. **Recreating** them reintroduces the
escape the path pipeline just closed, and does so through a mechanism the path
checks cannot see: a link's target is data, not a name in the archive's namespace.
It is also not portable — link semantics differ across the archive formats and
across the operating systems the crate supports, so faithful recreation is not one
behaviour but several. **Failing the run** would mean that a single symlink inside
an otherwise ordinary source tarball makes the archive unextractable, which is
unacceptably brittle for something that appears in a large fraction of real
tarballs.

Skipping with a warning keeps the successful path successful while making the
omission inspectable. It does put the burden on you: an `Ok` result whose
`warnings` vector you never read is a result you have not fully understood. If the
presence of links should change your decision before anything is written,
`Archive::check_symlinks` reports them from the listing alone.

The single-entry APIs behave differently on purpose. `extract_file`,
`extract_to_memory` and `extract_to_stream` commit to producing one regular-file
payload, so a link named there is an error rather than a silent skip — there is no
partial success to report and no other entry the call could have meant.

One accounting detail follows from skipping: link entries are excluded from the
size and ratio sums. Counting them charged a long link-target string against the
total budget for bytes that were never going to be written, which made the
cumulative ceiling fire on archives that were not close to it.

## Where the default numbers come from

The shipped ceilings are 10 GiB total, 1 GiB for a single file, 1000:1
compression ratio, 100 000 entries, and 16 GiB for a staged SFX payload.

None of these is a property of the formats. They are policy: chosen to sit above
what ordinary archives do and below what an attack needs. The 16 GiB SFX ceiling
is the one with an explicit derivation on record — AD 0040 reasoned from the
formats themselves (RAR splits at 4 GiB, 7z is bounded in practice by solid-block
sizes, ZIP64 is the only format supporting genuinely multi-gigabyte single
payloads) and picked a number that clears every realistic case with headroom. A
1 GiB alternative was rejected for rejecting legitimate installer distributions.

The consequence of "policy, not physics" is that the defaults are wrong for
someone. A build farm unpacking 40 GiB toolchains will trip `max_total_size`
legitimately; a service accepting user uploads should be far stricter than 10 GiB.
Both cases are the same instruction: set the numbers yourself. Because "unlimited"
is a distinct state (`Cap::Unlimited`) rather than an extreme number, turning a
ceiling off is visible in the code that does it. That was a deliberate correction
— earlier versions used `u64::MAX` and `f64::MAX` as "no limit" sentinels, and the
review history contains real bugs where a sentinel was either mistaken for a
genuine bound or silently disabled a gate. The compression ratio moved from `f64`
to an exact rational compared by integer cross-multiplication for the same
reason: an `f64` ratio admitted non-finite values that disabled the gate, and lost
integer precision above 2^53, where an allow/reject decision could flip at the
boundary.

Two of the typed fields are interfaces ahead of their implementations, and it is
worth knowing which. `reject_unsafe_paths` is discussed above.
`max_sfx_payload_size` is the second: AD 0040's amendment moved the ceiling onto
`ExtractionLimits`, but the shared staging helper in `src/archive.rs` still reads
the built-in 16 GiB constant, and no SFX entry point — `Archive::open_sfx`,
`Archive::open_with_sfx_progress`, `Archive::open_at_offset` — accepts an
`ExtractionLimits` to read it from. Threading a caller-supplied value through is
named in that record as future work. Lowering the field therefore hardens
nothing today.

## What this model does not protect against

The gates bound *how much* is written and *where*. They say nothing about what the
bytes are.

**Malicious content inside a legitimately-sized file.** An archive containing one
50 MB executable with a plausible name passes every gate described here, because
it is indistinguishable from an archive containing one 50 MB executable you asked
for. Content inspection — scanning, sandboxing, refusing to execute what you
unpack — is your responsibility and cannot be delegated to a size ceiling.

**Backend parser bugs.** Every gate here runs against metadata the backend has
already parsed. A memory-safety bug in libarchive's TAR reader, in UnRAR's header
handling, or in a compression codec is reached before any of this policy applies.
The crate narrows that surface — unsafe FFI is isolated behind checked wrappers,
buffer sizes taken from archive headers are bounded before use — but it does not
eliminate it. Extraction of genuinely hostile input belongs in a process boundary
you control, not just behind a limits struct.

**Time-of-check to time-of-use on the destination tree.** The path checks resolve
against the filesystem as it is at preflight time; the writes happen afterwards.
The known window is recorded rather than hidden: only the deepest *existing*
ancestor is canonicalised, so a symlink inserted into a not-yet-existing parent
path between the check and the directory creation is not caught. That
parent-creation race, and a matching gap where a symlink check is followed by a
separate open rather than a no-follow open, are both open items under OI-0076-003.
The practical mitigation is the same one that makes the rest of the model work:
extract into a directory nothing else is writing to.

**Anything about correctness of the archive's claims.** A CRC32 that matches tells
you the bytes are the bytes the archive's author recorded. It says nothing about
who the author was.

## Applying it

- The recipe that puts this into code — building the limits, choosing an overwrite
  policy, deciding about CRC verification, reading the warnings:
  [How to extract an untrusted archive safely](../../../how-to/user/en/extract-untrusted-archives-safely.md).
- Every field, type and shipped default in one place:
  [Options and defaults](../../../reference/user/en/options-and-defaults.md).
- Why one API sits over four different backends in the first place:
  [One API over many backends](one-api-many-backends.md).
