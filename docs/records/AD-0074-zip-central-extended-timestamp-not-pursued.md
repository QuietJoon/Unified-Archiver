---
type: ADR
title: "AD 0074: the ZIP central-directory extended-timestamp convention is not pursued"
description: "Accepted (2026-09-06) — OI-0081-004 asks for an mtime-only 0x5455 block in the central directory. Neither route to it is worth its cost against a low-impact strict-reader edge, and no dependency upgrade will ever supply it. Closed with one named condition under which it becomes nearly free."
tags: [decision, ADR-0074, zip, extended-timestamp, deferral]
status: active
---

# AD 0074: the ZIP central-directory extended-timestamp convention is not pursued

## Status

Accepted (2026-09-06). Closes OI-0081-004, which had been carried since
2026-07-18 — first as "blocked by the `zip` crate API", then, after that framing
was corrected on 2026-09-02, as an open decision.

## The gap, stated accurately

`src/ffi/zip_writer.rs` attaches one `0x5455` Extended-Timestamp payload carrying
mtime + atime + ctime, and that same payload lands in both the local header and
the central directory. The Info-ZIP convention is that the central-directory
block carries ModTime only.

Impact is a strict-reader edge, and the OI entry says so itself: most readers key
off the block's `TSize` and tolerate the 13-byte central block. No reader in use
here misparses it; none has been observed to.

## Why "wait for the dependency" was wrong, and is now retired

The entry's Required Action 2 said *"reassess when the `zip` dependency exposes a
local-only extra-field channel."* That instruction should not be acted on. It was
checked against both ends of the range available on this machine and re-verified
for this record:

* In `zip` 2.4.2, `add_extra_data(header_id, data, central_only)` routes to
  `central_extra_data` when `central_only` is true and to `extra_data` otherwise.
* The central-directory header writer emits **both** `file.extra_field` and
  `file.central_extra_field`.
* So `central_only = false` — what the writer passes for the `0x5455` block —
  means local **and** central. There is no local-only channel, and none can be
  simulated: emitting a second `0x5455` centrally would be a duplicate header id,
  and putting the mtime-only payload in `extra_data` instead would drop atime and
  ctime from the local record, which is strictly worse than the current state.
* `zip` 8.2.0, the newest copy present in `CARGO_HOME` here, has identical
  semantics.

This is not a gap the upstream crate is on its way to filling. It is a shape the
API does not have.

## Decision

**Do not pursue.** The two routes that would work are both disproportionate:

1. **A raw central-directory writer of our own.** The read side already exists —
   `src/ffi/zip_wrapper/raw_directory.rs` is a central-directory *parser* — but
   there is no writer, and building one means owning central-directory emission
   for every ZIP this crate writes. That is a medium-to-large piece of work whose
   risk is concentrated in exactly the structure that decides whether an archive
   is readable at all.
2. **A second ZIP writing dependency** that can express it. AD-0072 permits
   absorbing per-format dependency cost, but explicitly on the basis that the
   feature split (OI-0058-001) is what keeps that cost off every consumer — and
   that split is unstarted. Taking a second ZIP writer now would land its weight
   on everyone, to fix a low-impact edge.

Paying either price for a strict-reader edge that no observed reader trips is a
bad trade, and leaving the item open as a standing question has its own cost: it
has been re-triaged at least four times and consumed more effort in
reconsideration than the fix would take once the prerequisite exists.

## The one condition that reopens this

If a raw central-directory writer is ever built **for another reason**, this
becomes nearly free and should be done at that time. That is the only trigger.
It is not "when the `zip` crate is upgraded", and that wording is retired here so
a future reader does not go looking for an upstream change that is not coming.

## Consequences

* OI-0081-004 closes; ticgit `96be20d6` closes with a pointer here.
* The behaviour is unchanged and stays documented where it is produced.
* `tests/integration/zip_extended_timestamps.rs` continues to pin the round-trip
  that does work. Nothing about this decision weakens it.
* If the trade is judged differently later, nothing here blocks reversing it —
  the routes are unchanged and still available.
