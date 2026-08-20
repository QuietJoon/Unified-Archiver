---
type: Design Note
title: "OI-0065-002 — Piz reader 0x5455/0x000A extra-field parsing: cons / pros"
description: "Closed out 2026-08-06: OI-0065-002 was resolved 2026-04-30 and DCR-009 removed the piz reader the trade-off concerned."
tags: [design-note, ADR-0054, OI-0065-002]
timestamp: 2026-04-29T00:00:00Z
status: archived
---

# OI-0065-002 — Piz reader 0x5455/0x000A extra-field parsing: cons / pros

> **Closed out 2026-08-06.** The trade-off this note parked no longer exists: OI-0065-002 was
> resolved on 2026-04-30, and DCR-009 then removed the `piz` reader entirely (2026-07-23). The
> sole `zip`-crate backend parses the 0x5455 extended-timestamp field itself, so
> `list_files()` surfaces `accessed` / `created` without any of the options weighed below.
> Kept for the reasoning; not a live decision.

**Status:** parked design note. Surfaces a default-reader trade-off that needs user choice.

## Problem (one sentence)
The writer side of `preserve_metadata=true` now emits ZIP `0x5455` ("Universal Time") + 0x000A NTFS extra fields with `accessed`/`created` timestamps, but the default Piz-backed ZIP reader does not surface those fields back to `ArchiveEntry`, so a round-trip `Archive::open(...).list_files()` reports `accessed=None, created=None`.

## Two paths

### Option A — patch piz upstream to expose extra-field bytes
Walk the central-directory record, expose extra-field bytes on `FileMetadata`, parse 0x5455/0x000A in `src/ffi/piz_wrapper.rs::parse_entry`.

**Pros**
- Keeps Piz's mmap-based read path (fast, low-syscall, no per-entry decompress to peek metadata).
- Single-crate fix once the upstream PR lands.
- No public-API change in unified-archive — `ArchiveEntry.accessed/created` start populating without callers noticing.

**Cons**
- Upstream piz hasn't landed a release in 18+ months. PR may sit idle. Forking adds maintenance burden.
- Even if upstream accepts, semver-bumping piz across our dep tree is its own coordination cost.
- The `0x5455` flags byte (ModTime/AccessTime/CreateTime presence) needs careful parsing — bit-flag layout is in PKWARE APPNOTE, not in the extra-field bytes themselves.

### Option B — switch the default ZIP reader from Piz to the `zip` crate
The `zip` crate (`zip_rs`) already exposes `ZipFile::extra_data_fields()`. Wire `ZipArchive` (already in the codebase as the modify-side reader) as the default for `Archive::open(zip)`.

**Pros**
- Single crate move, no upstream dependency. Already validated for modify-side correctness.
- `zip` crate parses 0x5455 natively (`extra_field_universal_time`).
- Removes dual-reader divergence: today, Piz handles `Archive::open(...)` reads but `ZipArchive` handles modify-mode reads. Picking one closes the divergence.

**Cons**
- Loses Piz's mmap throughput. `zip` reads via `BufReader<File>` — slower for large central directories on big archives. Benchmark needed.
- ZipArchive currently sits behind a `Mutex<Option<RawZipArchive>>` for modify-side caching (per AD 0054). The default-reader switch needs the same caching, exported with the same Mutex semantics.
- Field-coverage cliff: callers depending on Piz-specific behaviour (mmap'd lazy reads, zero-copy slices) lose those — but unified-archive's public API never exposed those, so no API-level regression.

## Recommendation (for the user, not a decision here)
**Option B**. Reason: no upstream dependency, mmap loss is bounded by archive size (large archives are extracted not just listed), and the dual-reader divergence is the larger ongoing cost.

## Effort
- Option A: 1 day local + indefinite upstream wait (could be months). 1 day to consume upstream PR.
- Option B: 1 day to swap reader + benchmark + update tests; 1 day to backport mmap caching policy if needed.

## Why this isn't an ADR
The user routed it as "make a doc with cons/pros." The decision (A vs. B) is open and the answer depends on benchmark numbers we don't have yet on the consumer's archives. ADR will be written when the user picks.
