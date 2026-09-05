---
type: ADR
title: "AD 0071: libarchive owns ZIP modification; the zip crate owns everything else ZIP"
description: "Accepted (2026-09-02, owner ruling) — modify mode reads ZIP through libarchive and will keep doing so. The ZIP-native modify pipeline is not deferred work; it is not coming. Every other ZIP path in the crate goes through the `zip` crate, and that asymmetry is deliberate."
tags: [decision, ADR-0071, zip, modification, libarchive, backend-split]
status: active
---

# AD 0071: libarchive owns ZIP modification; the zip crate owns everything else ZIP

## Status

Accepted (2026-09-02) — owner ruling, recorded the same day in
`docs/architecture/mvp-scope.md`'s integration-deferral table, which now reads
*"Accepted as permanent, owner ruling 2026-09-02 (was: 'Planned: use zip crate
for ZIP modification')."*

## Why this record has to exist

The ruling was made and applied to one table cell in `mvp-scope.md`, and to
`docs/project/stub-manifest.md`'s DEF-005 line. It was never written down as a
record. Under this repository's own convention every AD / MADR / DCR belongs in
`docs/records/` and is registered in `docs/records/index.yaml`, so a decision
living only in a scope table is a decision the record store does not know about.

That gap had a measurable cost. `docs/architecture/effort-and-risk.md` — a
tracked, `status: active`, hand-maintained document — kept a "Known Technical
Debt" row whose Suggested Resolution was *"Use ZIP-native read/write
pipeline"*: the exact migration this ruling killed, still prescribed as the
fix. It survived a commit that edited the row directly above it on 2026-09-03,
the day after the ruling. Anyone reading that table would have concluded the
current shape was a stopgap awaiting the migration, which is precisely the
confusion the ruling was made to end.

The other cost is process. TicGit `fdd5f381`'s first acceptance criterion is
"a recorded decision (record or amendment) naming which pipeline owns ZIP
modification". No record named it, so the ticket could not be closed on the
merits even though the decision behind it had been taken.

## The decision

**Modify mode reads ZIP through libarchive, permanently.** `Archive::modify`
binds its read backend to `LibarchiveArchive` with no per-format match — every
format takes the same route in. The write side stays native: the commit path
dispatches on `ArchiveBackend::ZipWriter`, so a ZIP produced by a modify commit
is written by the `zip` crate.

**The asymmetry is the decision, not an accident.** Every other ZIP path in the
crate — read, create, integrity, streaming — goes through the `zip` crate.
Modify is the single exception, and it stays the exception.

**This is not a deferral.** "ZIP-native modify pipeline" must not appear in any
document as planned, pending, deferred, or suggested work. A plan nobody has
committed to reads as a commitment, and that is worse than a documented split:
a reader cannot tell whether the current shape is a stopgap or the answer. It
is the answer.

## Why

Migrating means rewriting the commit path for no user-visible gain on its own.
The split is ugly rather than harmful — it costs a reader's surprise, not a
caller's correctness — and the rewrite would spend a large, risky change budget
to buy internal symmetry.

## What this does NOT settle

DEF-005's three caveats against the libarchive rewriter — ZIP64 above 4 GiB,
encrypted-ZIP re-encryption, and crash-recovery journaling. Those were
previously framed as reasons to want the migration. With the migration ruled
out they become a **standing caveat list against the pipeline that is staying**,
not questions awaiting a change that is not coming. They keep whatever
independent treatment they merit; this record neither closes nor reopens them.

## Consequences

- `mvp-scope.md`'s row is a permanent-acceptance record rather than a deferral.
- `effort-and-risk.md`'s debt row is restated to name the ruling instead of
  prescribing the dead migration.
- `fdd5f381`'s first acceptance criterion is met by this record.
- Anyone who later wants the migration is proposing to overturn an owner
  ruling, and has to supersede this record to do it. That is the intended bar.

## Related

DCR-009 (collapse of the dual ZIP backends onto the `zip` crate — the change
that created the asymmetry, and which is silent on the modify path), DEF-005
(the residual ZIP-modify caveats), `docs/architecture/mvp-scope.md`,
`docs/architecture/effort-and-risk.md`, ticgit `fdd5f381`, ticgit `7318f6ca`.

## Amendment (2026-09-05, the stated reasons were wrong; the real one is a precondition)

AD-0072 made the uniform interface this library's stated purpose and named the
reasoning shapes it inverts. This record's entire `## Why` section is two
sentences, and both are on that list:

> Migrating means rewriting the commit path for no user-visible gain on its own.
> The split is ugly rather than harmful — it costs a reader's surprise, not a
> caller's correctness — and the rewrite would spend a large, risky change budget
> to buy internal symmetry.

A re-examination against AD-0072 checked them against the tree. They are not
merely inverted reasoning; **they are false**, and the record never named the
reason that actually holds.

### What is false

**"Rewriting the commit path" is not what a migration means here.** The commit
path already dispatches on `ArchiveBackend::ZipWriter`, and a read-side swap does
not touch it. This record says so itself two paragraphs earlier — *"The write side
stays native"*.

**"A reader's surprise, not a caller's correctness"** is contradicted three ways,
all verifiable:

1. **Every ZIP commit walks the central directory twice.** The commit path reopens
   the archive with the `zip` crate to recover the comment and the per-entry
   compression method, because libarchive cannot surface them, after libarchive
   has already walked it once for the listing. The in-tree comment at that site
   says the real fix is to move the modify source to the `zip` crate so both walks
   collapse into one. That is O(entries) of extra I/O per commit, which is a
   caller's cost.
2. **A correctness guard exists only to reconcile two readers, and its strongest
   arm is dead.** `cross_check_source_listing` raises typed drift errors for name,
   size and CRC. The CRC arm reads the facade listing's `crc32` — which is the
   libarchive listing, where it is unconditionally `None`. It never fires. It
   reads as a guard against a malformed archive whose central directory disagrees
   with its local headers; it is one only for name and size.
3. **It is the crate's one documented layering violation.** `src/modification.rs`
   is the only orchestration module that imports a backend crate directly, and the
   architecture notes already record that this collapses when the ZIP modify read
   path moves.

There is also a capability gap this record never mentions, which is exactly the
case AD-0072 is about: libarchive's generic entry API cannot express the ZIP
archive comment, the per-entry compression method, `crc32`, or `compressed_size`.
The crate is already paying a code cost for that — but for a **reconciliation
layer between two readers**, rather than for the better reader.

### The reason that actually holds

The `zip` crate's ZIP read side does not stream: `extract_to_stream` loads the
whole entry into memory before wrapping it in a cursor. The retained-entry replay
calls that once per entry, so swapping the modify read path to the `zip` crate
**today** would turn a bounded-memory rewrite into one whose peak scales with the
largest retained entry. That is a real, user-visible regression, and it is the
argument this record should have made.

It is a **precondition, not a permanent answer.** It is DEF-004 — and under
AD-0072 an upstream gap of that shape is itself absorbable rather than final.

### What this record now means

- **The operative ruling stands, and stands for a better reason.** ZIP modify
  reads through libarchive, and no document should carry an unowned "planned
  ZIP-native migration" — that half of this record fixed a real drift, where
  `effort-and-risk.md` was still prescribing the dead migration.
- **Permanence becomes conditional.** The migration is not planned work *while
  DEF-004's ZIP streaming reader is open*. It becomes reconsiderable when that
  lands. "It is the answer", and the blanket ban on the phrase appearing anywhere,
  are withdrawn.
- **The cost is named rather than hidden**: a second central-directory walk per
  commit, the reconciliation layer and its dead CRC arm, and the layering
  violation. A future reader weighing this should weigh those, not "ugly rather
  than harmful".
- **`## What this does NOT settle` stands as written.** DEF-005's three caveats
  are backend-independent — ZIP64 is already the `zip` crate's own write path,
  encrypted re-encryption is a write-side hold that no reader change touches, and
  journaling is orchestration — so they are correctly out of scope here and are
  *not* examples of the AD-0072 case.

Separately, and independently of this ruling: the dead CRC arm in
`cross_check_source_listing` should be either fixed or documented as unreachable.
It currently presents as a correctness guard that cannot fire.
