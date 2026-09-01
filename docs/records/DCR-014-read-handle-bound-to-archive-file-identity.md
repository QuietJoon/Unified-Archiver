---
type: DCR
title: "Read handles are bound to the archive file's identity, not to a wider per-entry fingerprint"
description: "Every read backend now records the identity of the archive file its cached listing describes — (dev, ino, len) on Unix, len alone elsewhere — and re-checks it at every by-path re-open, refusing with OperationBlocked + \"identity changed\". OI-0001-002's own acceptance criteria asked for a per-entry metadata fingerprint instead; that alternative was rejected on false-positive risk, not on coverage, and this record says why."
tags: [change, project-control, DCR-014]
status: active
---

# DCR-014: Read handles are bound to the archive file's identity, not to a wider per-entry fingerprint

- **Date:** 2026-09-01
- **Source:** OI-0001-002 (R0001-0020, Review 0001); TicGit `f84b31d1`
- **Related:** AD-0065 (backend caching baseline — frozen view at first use), AD-0052 (lazy-validation
  semantics for backend `open()`), DCR-007 (modify mode revalidates locked-file identity, and its
  2026-07-22 amendment on the fd hand-off), DCR-006 (bounded streaming errors on over-production),
  DCR-011 (digest exactness for CRC-less entries), MADR-0009 (advisory file locking for modify mode),
  OI-0081-001, OI-0001-003

## Why this needs a record at all

Two reasons, and the second is the larger one.

**The defect.** Every read backend memoises its listing once (AD-0065's frozen-view baseline) and
then re-opens the archive **by pathname** for each later operation, because none of the four native
readers offers a rewindable handle. The drift guards standing at those re-opens compared the
normalised entry *name* and nothing else. A same-name replacement between the listing and the
extraction therefore passed every guard, while the safety gate's ratio, size and entry-kind
decisions had already been taken against the stale cached listing. OI-0081-001 named those
cross-checks as the mitigation for the whole detect-then-reopen class, so the mitigation read
stronger than it was.

**The approach is not the one the issue asked for.** OI-0001-002's Required Actions asked for a
stable per-entry metadata fingerprint — type, declared size, CRC, encryption — produced identically
from the cached entry and from the live walk, applied at all five guard sites. That is not what
landed, and the difference is deliberate, owner-chosen, and argued below rather than asserted. A
later reconciliation pass reading those criteria against this tree would otherwise conclude the work
is unfinished. It is finished; it answers a different, narrower question, and the question the
criteria asked is left explicitly open rather than silently dropped.

## What changed

A new `pub(crate) FileIdentity` in `src/fs_identity.rs` — the module that already owns this crate's
single audited `MetadataExt` site — with `capture` (path `stat`, best-effort), `from_metadata`
(usable with an `fstat` of an open descriptor), `revalidate` (fail-closed), and one shared
`identity_drift` error constructor so four backends cannot grow four vocabularies for one condition.

Each read backend now records the identity of the file its cached listing describes, and re-checks
it at every by-path re-open:

- **libarchive** gained an `identity` field, captured in `open` inside the bracket shape the UnRAR
  backend already used (`stat`, native open, `stat` again), and a `bound_read_handle` that replaces
  the raw `open_read_handle` at all six read sites plus the stream reader's own open. `open` itself
  keeps the raw opener, because it is where the binding is *captured* rather than compared — a
  counter of `bound_read_handle` call sites therefore reads seven, not eight, and the one surviving
  raw call is not a missed site. Capturing at
  `open` rather than at listing population is deliberately stronger: the AD-0052 eager first-header
  probe and the AD-0065 listing snapshot are then provably bound to the same bytes. The two
  write-mode constructors set `identity: None`.
- **ZIP** gained an identity `OnceCell` and an `open_zip_bound` that captures from
  `File::metadata()` of the descriptor it is about to hand to the raw archive.
- **7z** gained an identity `OnceCell` with capture-or-compare inside `open_reader`, which every
  operation already routes through; an op label is threaded from all six callers so the refusal
  names what was refused.
- **UnRAR** gained the comparison in `fresh_handle`, which all four re-open callers inherit, and its
  pre-existing `UnrarFileIdentity` was migrated onto the shared `FileIdentity` in the same change.
  That migration is net-negative lines and confined to one file; the recovery-metadata guard it also
  serves still compares inode-only and still emits a byte-identical message.

**Every pre-existing name and cardinality guard was kept.** Nothing was removed, narrowed, or
replaced. The binding is additive, for the reason set out under "The residual" below.

## What identity consists of, and where it degrades

- **Unix:** `(dev, ino)` plus the byte length.
- **Everywhere else:** the byte length alone.

The degradation is real and is stated in the type's own rustdoc rather than left for a reader to
discover. `InodeId` is Unix-only because the stable `std` surface has no inode analogue:
`volume_serial_number()` and `file_index()` are nightly, behind `windows_by_handle`. Off Unix, a
replacement that changes the file's length is still refused; a *same-length* replacement is not
caught by identity at all.

**Timestamps were rejected as the substitute.** Folding `last_write_time` or `creation_time` in from
`std::os::windows::fs::MetadataExt` would strengthen the heuristic and would have closed most of the
practical gap. It was not done, because a timestamp is trivially forgeable and is routinely
preserved by ordinary copying tools — and selling forgeable metadata as *identity* is worse than a
documented gap. A reader who knows the Windows check is length-only can reason about it correctly; a
reader told the handle is identity-bound, when the binding rests on a field an attacker sets, cannot.
This keeps parity with the degradation the crate's open-time read guard already documents rather
than inventing a new semi-guarantee. Note the change also *improves* UnRAR off Unix, where the old
`UnrarFileIdentity` captured nothing at all and now carries the length check.

## Why `len` is part of the identity, and what that costs

`(dev, ino)` alone would be blind to the two most convenient ways to replace an archive.

1. **`std::fs::copy` onto an existing path preserves the destination inode.** It opens and truncates
   the destination rather than replacing it. Verified empirically on the dev host this session: same
   inode, different length. This is the replacement primitive a test — or an attacker with a shell —
   reaches for first, and inode comparison does not see it. The length does, whenever the byte count
   moves.
2. **Appending to a tar keeps the inode while adding entries the safety gate never saw.** The
   bulk-walk cardinality guard would catch the extra live entries, but a single-entry seek stops at
   its target index and walks no further, so it would not.

**The accepted cost, stated as a cost.** A *cooperating* writer that appends to an archive under a
live read handle now gets a refusal instead of an undefined mixed read. That is a real behaviour
change for a legitimate pattern, and it is the correct answer under MADR-0009's cooperating-process
model: this crate's own writers never mutate in place — they stage and atomically install, and
`commit_changes` consumes the handle — so the only party this refuses is one mutating an archive
under a live reader, which is precisely the shape of the attack. The refusal is loud and names the
remedy (reopen the archive) rather than degrading silently.

## The residual

**A same-inode, same-length in-place rewrite is invisible to any stat-based identity.** Open,
truncate, write the same byte count — or `pwrite` into the middle — and every field this record
compares is unchanged. mtime is deliberately not part of the comparison for the reason given above,
so nothing in `FileIdentity` sees it. Off Unix the residual is wider still: any same-length
replacement.

What covers that case is exactly what was kept:

- the per-index **name** guards at every drift site on all four backends;
- the **cardinality** guards — extra live entries, and early end-of-archive — on the bulk walks;
- the **DCR-006** declared-size exactness bound on `DeclaredSize` streams, and DCR-011's exactness
  on the digest surface, which turn a payload that no longer matches its declaration into an error
  rather than a short read;
- **CRC verification** on ZIP, 7z and RAR, where the format carries a per-entry checksum.

Neither layer subsumes the other, which is the whole reason both exist. Identity cannot see a
same-inode same-length rewrite. The name guards cannot see a whole-file swap that kept every name.

## The rejected alternative: a per-entry metadata fingerprint

This is the section the record exists for.

**What was asked.** OI-0001-002 Required Actions 1–3: define a stable per-entry fingerprint over
entry type, declared size, CRC and encryption status; establish per backend that the live walk
normalises each participating field exactly as `parse_entry` does; apply it at all five guard sites,
failing closed on any mismatch.

**Why it was rejected.** Action 2 is not a formality, and the issue's own text says so — it names
the failure mode of getting it wrong as a hard `OperationBlocked` on legitimate archives. Reading
the tree, the normalisations do not agree today:

- **Directory sizes disagree by backend.** A directory entry's declared size is `None` on ZIP, 7z
  and UnRAR, and `Some(0)` on libarchive tar. A fingerprint comparing declared size would have to
  encode that difference per backend and per entry kind before it could compare anything.
- **CRC is absent for a whole format family.** It is `None` for tar, ISO and the raw single-file
  readers — the formats with no per-entry checksum at all — and a placeholder for AE-2 ZIP, where
  the field is present in the record and deliberately not the payload's CRC. A fingerprint including
  CRC is therefore comparing "nothing" against "nothing" on exactly the formats where content
  verification is weakest, and comparing a placeholder against a placeholder on AE-2.
- Every remaining field would need the same treatment, per backend, and the agreement demonstrated
  by test rather than assumed — which is what Action 2 asks for and what makes it expensive.

A widened comparison built on those disagreements is the fail-closed-on-healthy-archives trap the
issue's own note warned about: the cost of a false positive here is refusing a legitimate extraction
of an undamaged archive, and the crate would be paying it on the formats it supports best.

**The two approaches are NOT ordered, and a future reader must not read this as the fingerprint idea
being wrong.** They cover overlapping but distinct sets:

- A fingerprint **would** catch the same-inode same-length in-place rewrite that identity misses —
  the residual named above — because it compares content-derived metadata rather than file
  attributes.
- Identity **does** catch every replacement primitive a fingerprint would have to be perfect to see:
  rename-over, `fs::copy`-over, append, truncate-and-rewrite-to-a-different-length. A fingerprint
  catches those only to the extent that its fields actually differ between the two files, and only
  at the entries it inspects; identity catches them at the file, before a single entry is read.

The choice between them was made on **false-positive risk**, not on coverage dominance. Identity is
cheap, uniform across four backends, needs no per-backend normalisation agreement, and cannot refuse
a healthy archive that nobody touched. The fingerprint is strictly more work, needs the Action 2
agreement proof before it can be trusted, and its failure mode on a healthy archive is a hard
refusal. Identity was taken first for that reason alone. If the same-inode same-length rewrite ever
needs closing, the fingerprint is the right instrument for it and this record is not an argument
against building it — it is a record that the agreement proof, not the idea, is the cost.

## Error type and the two-vocabulary rule

**Variant: `ArchiveError::OperationBlocked`, not a new variant.** `ArchiveError` is
`#[non_exhaustive]`, so a new variant was available. It was not taken: `OperationBlocked` is already
what all three pre-existing identity guards speak — the facade open-time guard (OI-0081-001), the
UnRAR recovery-metadata guard, and modify mode's locked-file revalidation (DCR-007) — and this is
the same condition caught at a later altitude. A fourth guard speaking a fifth variant would be
worse for a caller than one variant with one phrasing. The `operation` field carries the caller's op
label, so the refusal names what was refused.

**Two vocabularies, deliberately disjoint.** Identity drift is `OperationBlocked` carrying
**"identity changed"**. Name and cardinality drift stays `ArchiveError::Format` carrying **"listing
drift"**. A caller can tell "this is no longer the file you listed" from "this is the file, but the
n-th entry is not what it was" by variant *and* by message, and the two message vocabularies do not
overlap. The unit assertions pin both halves: the identity message must contain "identity changed"
and must *not* contain "listing drift".

The phrase itself is the tree's pre-existing one. Six production sites already said "identity
changed", so the new constructor was aligned to them rather than the other way round — a point that
surfaced as a failing modify-mode test asserting the old wording, and was resolved by changing the
new message, not the old test.

## Per-backend asymmetry, which is real

The four backends do not get the same guarantee, and flattening that in the prose would misstate
what ZIP has and what the other three have.

**ZIP's window is closed.** `open_zip_bound` captures from `File::metadata()` — an `fstat` of the
very descriptor every subsequent read goes through. The inode measured is by construction the inode
read; there is no stat-to-open window to race. And because every listing, extraction, streaming and
integrity path on this backend reads through the cached `RawZipArchive` or the descriptor it owns
(OI-0001-003), that single re-open is the only by-path resolution the backend ever performs.

That last sentence was almost wrong, and the way it was almost wrong is worth recording. The backend
performs no other by-path resolution, but the **facade** does: `Archive::payload_size_for_ratio`
re-`stat`s the pathname to produce the denominator of the compression-ratio gate, while the
numerator comes from the AD-0065 cached listing. Those two numbers straddled two different files
whenever the archive was replaced after the snapshot, and the direction of the bug is the dangerous
one — a *larger* replacement talks the gate **down**. A one-entry archive declaring 900 MiB inside a
100 KiB file is refused at a ratio of about 9216; drop a 2 MiB blob over the pathname, which need
not even be a valid archive because nothing parses it, and the ratio reads 450, the gate passes, and
extraction then proceeds through ZIP's cached descriptor against the original bomb. On the
path-`stat` backends the same straddle happens at gate time and is masked only because the
subsequent re-open refuses; on ZIP nothing downstream could refuse it at all, precisely *because*
the backend never re-opens. The denominator is now revalidated against the handle's binding before
it is used, which is what makes the closed-window claim above true at the altitude a caller actually
sees. It was found in review, not in the gate: every identity test passed without it.

**libarchive, 7z and UnRAR capture by path `stat` and retain a sliver.** Each hands a *pathname* to
a native opener that exposes no Rust-side descriptor, so the strongest available shape is the
bracket: `stat`, open, `stat` again. That narrows the window to the duration of the native open; it
does not close it. This is not a gap this change chose to leave — DCR-007's 2026-07-22 amendment
already evaluated the owned-fd hand-off that would close it and **rejected** it on four grounds
(iterator-shaped handles cannot be rewound, one re-handed fd shares a file offset across concurrent
handles, no portable way exists to derive an independent open-file description from an fd, and
UnRAR's SDK can never take an fd at all). The sliver is accepted-permanent for the path-only
backends, and is one of the reasons the name and cardinality guards all stay.

## UnRAR multi-volume: only the first volume is bound

For a multi-volume RAR set, the continuation volumes are opened **inside the SDK**, via the
volume-change callback; the crate never resolves their pathnames itself. Only the first volume's
path is identity-bound.

This is out of scope rather than an oversight, and the reason is structural: there is no
per-continuation-volume listing snapshot to bind a continuation volume *to*. The listing is taken
once, over the set, through the first volume's handle. Binding a continuation volume would mean
inventing a per-volume snapshot that the cache does not have and the SDK does not expose a hook to
capture at. Named here so a future reader finds it as a decision with a reason, not as an untested
assumption.

## Test impact

Three integration tests previously constructed their scenario with `std::fs::copy` onto the archive
path — which preserves the inode but changes the length, so post-change they would have tripped the
identity guard before reaching the property they exist to pin. They were reworked onto a shared
`rewrite_in_place_preserving_identity` helper in `tests/common`: open in place with `write` +
`truncate` (never a rename, never `create_new`), zero-pad the replacement up to the target's current
length, and assert the length precondition so it is visible in the test rather than assumed. For TAR
the padding is not a hack — trailing zero blocks are the end-of-archive marker, so the padded result
is a well-formed archive. The helper panics rather than truncating if the replacement is longer,
because a silently truncated archive would let the test run against a differently-damaged file.

Empirically established this session, since both were previously assumptions:

1. `std::fs::copy` onto an existing path preserves the destination inode and changes its length.
2. libarchive writing a tar to a regular file does **not** pad to the 10240-byte block factor — a
   512-byte-payload tar and a 4096-byte-payload tar have different lengths. The helper's design does
   not depend on this either way, which is why it pads explicitly instead of relying on it.

New coverage is `tests/listing_identity_test.rs`: per backend, open → `list_files` → swap a
same-name archive in → each of `extract_all`, `extract_file`, `extract_to_memory`,
`extract_to_stream` and `validate_integrity` is refused with the identity phrasing, on tar, 7z and
RAR. Plus a `cfg(unix)` test that appends a single byte to a tar — inode unchanged, length moved —
and pins the refusal, so `len`'s place in the identity is not merely argued in prose.

**ZIP has no facade-level refusal test, because ZIP cannot produce a refusal.** Once `list_files`
warms the cached descriptor there is no second by-path re-open for the guard to fire at. Asserting
a refusal there would require staging a cache-rebuild failure *and* a swap, which would prove less
about the guard than the honest test does: a descriptor-stability test asserting that ZIP keeps
serving the file its cached descriptor holds. That is ZIP's actual property, and it is asserted
rather than left as a silent hole in the matrix.

## Known risks

1. **Exotic filesystems that synthesise unstable inode numbers** (some FUSE and SMB mounts) can
   produce a false refusal. This is not a new exposure class — DCR-007's modify-mode guard,
   OI-0081-001's open-time guard and the UnRAR recovery reader already stat-compare `(dev, ino)` on
   the same paths — but it now applies at re-opens that were previously unguarded, so the surface is
   wider than before. The failure is loud and names the remedy.
2. **The cooperating-appender refusal** described above is a behaviour change for a caller who was
   getting away with it. Nothing in the type system signals it; the error message is the only notice.
3. **Off-Unix coverage is length-only**, and this record is the place that says so. A Windows reader
   who assumes "identity-bound" means what it means on Unix will overestimate the guarantee.
4. **The OI-0001-002 acceptance criteria remain formally unmet.** Actions 1–3 asked for the
   fingerprint; this change does not provide one, and the criteria's own verification boxes are not
   satisfiable by it. The residual those actions would have covered is named under "The residual"
   above and stays open work, not closed work.
