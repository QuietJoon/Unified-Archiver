---
type: ADR
title: "AD 0073: an abandoned integrity scan is a third outcome class, not a corruption verdict"
description: "Accepted (2026-09-06, owner ruling on OI-0080-007 item 2) — `ArchiveError::IntegrityScanAborted` expresses \"the scan could not trust itself past this point\", which neither of the two existing classes could say. Records the measurement that answered Required Action 3 and narrowed why the guard matters."
tags: [decision, ADR-0073, sevenz, integrity, error-taxonomy]
status: active
---

# AD 0073: an abandoned integrity scan is a third outcome class, not a corruption verdict

## Status

Accepted (2026-09-06) — owner ruling on OI-0080-007 item 2, which had been
carried as "needs decision" since 2026-08-07 and was re-examined twice.

## Context

`test_integrity` had two outcome classes and no way to express a third.

* **Per-entry corruption is recorded, and the walk continues.** One damaged
  entry says nothing about the others, so the scan keeps going and returns the
  list of paths that failed.
* **An archive-level I/O failure propagates.** Nothing can be read at all, so
  there is no report to give.

The condition with no home is *"the scan read enough to know it can no longer
trust itself"*: after a corrupt entry in a solid block, if the shared cursor
cannot be put back where the table of contents says it is, every later entry
would be decoded from the wrong offset and its verdict recorded against the
wrong name.

Both existing classes are wrong for it. Recording it as corruption asserts a
per-entry verdict the scan cannot support. Propagating it as I/O claims the
archive is unreadable when most of it was read fine. Continuing — which is what
the code did — produces the worst answer available: a report that blames healthy
entries and is indistinguishable from a real one.

## Decision

Add `ArchiveError::IntegrityScanAborted { path, entry, reason,
failed_before_abort }`.

`ArchiveError` is `#[non_exhaustive]` (R0076-0076), so this is not a breaking
change. `failed_before_abort` carries the verdicts the scan *did* reach rather
than discarding them: those are trustworthy, and a caller acting on known damage
should not have to re-run a scan that already found it. The `Display` text leads
with the incompleteness, because the failure mode this record exists to prevent
is a caller reading a partial report as a whole one — entries after the abort
point are **untested, not passed**.

## Required Action 3, answered by measurement

The OI entry made this half conditional: *"first verify whether `sevenz-rust2`'s
`for_each_entries` re-syncs the solid cursor after a short callback read. If it
does, the mis-blame is unreachable and this half closes as moot."* It was never
verified, in either of the two re-examinations.

It was measured on 2026-09-06. Two 30 KB COPY-stored entries in one archive; the
callback reads **10 of `a.txt`'s 30,000 bytes** and returns. `b.txt` then comes
back complete and byte-for-byte correct.

**Upstream does re-sync.** So the mis-attribution as the finding originally
described it — short callback read leaves the cursor mid-entry — is *not
reachable by that route*, and this record says so plainly rather than leaving
the original motivation standing unexamined.

What that does **not** establish is the behaviour after a decode *error*, which
is a different state from a short read, and which could not be measured: the
COPY route reaches its CRC comparison only once the byte budget is spent, so the
drain that follows sees a clean EOF and never errors, and the LZMA2 route is
unusable for the reason below. So the guard is kept — narrowed from "this fixes
a known mis-blame" to "this refuses to guess in the one state we cannot
observe". That is a weaker claim than the finding made, and it is the accurate
one.

## Two unbounded loops, found while implementing this

Both read loops in `test_integrity` were unbounded — they ran until the reader
returned `Ok(0)` or an error, with nothing tying them to the entry's declared
size. Both are now bounded by the 7z table of contents' unpacked size, which
this backend already treats as authoritative (R0075-0051), plus one byte so an
over-producing decoder is detected rather than looped on. Each iteration now
either terminates or advances by at least one byte, so both loops are provably
finite.

Over-production is folded into this record's class rather than treated as plain
corruption: an entry that decodes past its declared size has moved the shared
cursor beyond the next entry's start, which is the same untrustworthiness by a
different route.

## What this does not fix, stated so it is not assumed

A 7z archive with a damaged **LZMA2** region hangs `test_integrity` forever, and
this record does not fix that. The hang is inside sevenz-rust2's decoder, in a
single `read` call that never returns — measured with an atomic counter at the
call site, which printed exactly one entry and never a second, and confirmed by
`sample(1)` showing every sample at that call. It reproduces on unmodified HEAD.
No bound in this crate can interrupt a call that does not return. Tracked as
ticgit `f8c4b2`; the possibility that the extraction paths share it is noted
there and not yet measured.

## Consequences

* `test_integrity` gains a third documented outcome, and its rustdoc states both
  it and the LZMA2 hazard.
* The drain logic is extracted as `drain_to_realign`, taking a `Read`. Neither
  abort branch can be produced by any 7z archive buildable and readable on this
  host — the COPY route cannot error mid-drain and the LZMA2 route hangs first —
  so a `Read` seam is what makes them testable at all. Both branches are covered
  and both were mutation-checked.
* `test_sevenz_integrity_records_corrupt_payload` is unchanged and still passes:
  a corrupt payload remains a recorded failure, because the new class is a third
  branch and not a reclassification of that one.
