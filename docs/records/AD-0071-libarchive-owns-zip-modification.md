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
