---
type: ADR
title: "AD: Dual ZIP Backend Strategy"
description: "Superseded by its own 2026-07-23 collapse amendment (DCR-009) — the dual piz/zip strategy is retired and the zip crate is the sole ZIP backend."
tags: [decision, ADR-0007]
timestamp: 2026-04-12T00:00:00Z
status: superseded
---

# AD: Dual ZIP Backend Strategy

Status: Superseded by the 2026-07-23 collapse amendment — the dual strategy is retired; the `zip` crate is the sole ZIP backend for both encrypted and unencrypted archives.

## Context and Problem Statement
`piz` (mmap-based) provides fast parallel ZIP reading with CRC32 metadata but cannot decrypt. The `zip` crate supports AES and ZipCrypto decryption. A single backend cannot satisfy both the performance requirements for unencrypted archives and the decryption requirements for encrypted ones.

## Decision Drivers
- Best performance for the common unencrypted ZIP case
- Encrypted ZIP support without requiring API changes for consumers
- Seamless backend selection based on archive properties

## Considered Alternatives
- **Replace piz entirely with zip crate** -- rejected because piz's mmap-based reading is substantially faster for large unencrypted ZIP archives.
- **Add decryption to piz upstream** -- blocked by upstream API limitations that make this impractical.

## Decision Outcome
We decided to use `piz` as the default ZIP backend via `Archive::open()` and switch to the `zip` crate via `ZipReader` when `open_encrypted()` is called for ZIP, because it delivers the best performance for the common case while providing seamless encrypted ZIP support.

## Consequences
- Good: Best performance for the common unencrypted case; encrypted ZIP support is seamless to consumers.
- Bad: Two ZIP read backends add maintenance cost and introduce slight behavioral differences that must be tested and documented.

## Amendment (2026-07-22, R4 benchmark)

This amendment records the outcome of the "one benchmark that settles it"
(`docs/architecture/zip-piz-backend-comparison.md`), commissioned to decide whether piz's mmap
read path materially beats the `zip` crate over `File`. It does **not** rewrite the Decision
Outcome above (immutability policy, per AD 0019 / AD 0065).

**Benchmark:** `benches/zip_backend_read.rs` (criterion; single-threaded; each arm opens the
archive cold and reads *every* entry's full decompressed bytes into `io::sink()`; `sample_size=10`).
Machine: macOS (darwin), 64 GB RAM. Fixtures were built with a throwaway `zip`-crate streaming
generator (content generated on the fly, not committed to the repo):

- **DEFLATE (common case)** — 500 entries, 1,048,576,000 B uncompressed, 571 MiB archive (~1.75:1).
- **STORED (zero-copy edge)** — 8 × 80 MiB entries, 640 MiB archive, uncompressed.

Three arms: (1) piz over `Mmap`, (2) the `zip` crate over `std::fs::File`, (3) the `zip` crate over
`Cursor<Mmap>` (informational — how much of any gap is recoverable *without* piz).

Wall-clock (criterion median):

| Arm | DEFLATE (~1 GiB) | STORED (640 MiB) |
|---|---|---|
| 1 · piz + `Mmap` | 3.3023 s (302.8 MiB/s) | 106.41 ms (5.87 GiB/s) |
| 2 · zip + `File` | 3.3336 s (300.0 MiB/s) | 121.25 ms (5.15 GiB/s) |
| 3 · zip + `Cursor<Mmap>` | 3.3225 s (301.0 MiB/s) | 106.45 ms (5.87 GiB/s) |

piz (arm 1) vs zip/File (arm 2): **0.9 % faster on DEFLATE**, **12.2 % faster on STORED**. Arm 3
(`Cursor<Mmap>`, no piz) recovers essentially the entire STORED gap (121.25 → 106.45 ms, 12.2 %),
landing within noise of arm 1 — so the mmap advantage is a property of the read *substrate*, not of
piz.

Peak RSS (`getrusage` `ru_maxrss`, cross-checked against `/usr/bin/time -l`, which agreed to the
byte):

| Arm | DEFLATE | STORED |
|---|---|---|
| 1 · piz + `Mmap` | 576.7 MiB | 645.8 MiB |
| 2 · zip + `File` | 6.3 MiB | 5.8 MiB |
| 3 · zip + `Cursor<Mmap>` | 576.9 MiB | 645.9 MiB |

Reading every entry faults in the whole file, so both mmap arms sit at ≈ archive size while
zip/File streams at ≈ 6 MiB. Those mapped pages are file-backed and evictable (RSS overstates
committed RAM), but the whole-file-resident profile is real for archives near/over RAM.

### Decision-rule outcome (from the comparison doc)

- **Common case (DEFLATE):** piz beats zip/File by **< 1 %**, far below the ~15–20 % threshold →
  the mmap edge does **not** justify a second backend on speed grounds.
- **STORED:** piz's 12.2 % edge is below the threshold **and** fully recoverable via `Cursor<Mmap>`
  without piz (arm 3) → the second crate buys nothing a `Cursor<Mmap>` could not.
- **Over-RAM / RSS story:** not exercisable on this hardware (64 GB RAM ≫ the < 1 GB fixtures the
  small temp volume allowed). It only bites for archives near/over RAM, which the mmap arms would
  force fully resident. Reported as a hardware limitation, not a null result.

### Resulting decision

**piz does NOT win.** Per the AD 0007 owner's decision rule this puts the collapse-to-`zip` option
(Option A / C in the comparison doc) on the table — but the collapse is a **separate follow-up
requiring explicit owner approval and is NOT performed by this benchmark task.** No backend is
removed here; the dual-backend Decision Outcome above stands until the owner acts. Recommendation
carried forward: collapse to the `zip` crate over `File` (Option A), keeping `Cursor<Mmap>` behind
an opt-in feature (Option C) only if a future over-RAM workload shows a large win the common case
did not.

### OI-0080-002 (piz mmap SIGBUS / silent-mutation) consequence

While both backends are kept, this hazard remains **OPEN** and must still be addressed. A future
collapse to `zip`-over-`File` eliminates it (File reads surface truncation as an ordinary I/O
error, not a signal). Note the benchmark's caution: recovering piz's STORED-case speed via
`Cursor<Mmap>` would re-import **both** the whole-archive RSS profile *and* the SIGBUS hazard — so
the fast path is not free even without piz.

Cross-refs: `docs/architecture/zip-piz-backend-comparison.md`; OI-0080-002 (mmap SIGBUS);
R0079-0026 (duplicate-name asymmetry a collapse must consciously re-preserve).

## Amendment (2026-07-23, R4 collapse to a single `zip`-crate backend)

Owner-approved execution of the collapse the R4 benchmark amendment put on the table (Option A).
This amendment does **not** rewrite the original Decision Outcome (immutability policy); it records
that the dual strategy is retired. Paired design-change record: **DCR-009**.

**Decision:** Remove the `piz` backend entirely. The `zip` crate becomes the sole ZIP backend for
BOTH encrypted and unencrypted archives. `Archive::open` on a ZIP now constructs
`ArchiveBackend::ZipReader` (the `zip`-crate `ZipArchive`) exactly as `Archive::open_encrypted`
already did — the plain path is the same backend, just built without a password. The
`ArchiveBackend::Piz` variant, `src/ffi/piz_wrapper.rs`, the `piz` and `memmap2` dependencies, and
the `benches/zip_backend_read.rs` decision benchmark are deleted.

**Why now:** The benchmark showed piz does not win — 0.9 % on DEFLATE (the common case) and 12.2 %
on STORED, the latter fully recoverable without piz via `Cursor<Mmap>`. Against that near-zero
upside, piz cost ≈ archive-size resident RSS and carried the OI-0080-002 mmap SIGBUS / silent-
mutation hazard. One backend also erases the dual-backend behavioural-divergence class that
R0079-0026 and OI-0065-002 had to reconcile.

**Consequences of the collapse:**
- Good: OI-0080-002 is eliminated by construction — `File` reads surface truncation as an ordinary
  I/O error, never a `SIGBUS`. RSS for ZIP reads drops from ≈ archive size to ≈ streaming buffers.
  No more piz-vs-zip divergence to test or document.
- Good: the memory-map machinery that existed only for piz is gone — `ExtractionLimits::max_mmap_size`
  / `platform_default_mmap_size`, `get_max_mmap_size`, `DEFAULT_MAX_MMAP_SIZE`, the
  `ExtractionPlan.max_mmap_size` field, and the `ReadBackend::extract_to_memory_with_caps` /
  `extract_to_stream_with_caps` trait methods were all removed (public-API-breaking, allowed
  pre-1.0). The mmap-enforcement tests (`tests/large_zip_mmap_test.rs`,
  `test_extract_all_respects_max_mmap_size_limit`) were dropped because they asserted a path that no
  longer exists.
- Neutral/known cost: the `zip` crate's central-directory map dedupes byte-identical entry names
  (last record wins), whereas piz kept every record and let the single-entry gate refuse the
  ambiguity. R0079-0026 duplicate-name rejection is therefore **re-homed** into the ZIP backend: a
  raw central-directory re-scan (`ZipArchive::scan_duplicate_names`) records normalized names with
  raw multiplicity > 1, and the by-name single-entry paths refuse them with the same
  "Multiple entries match" error `validate_single_entry` raises. Residual: an exotic mixed-encoding
  collision the crate deduped but a UTF-8-lossy re-decode cannot attribute is caught by a
  raw-count-vs-deduped-count mismatch that refuses the whole by-name single-entry surface (safe
  over-rejection). See DCR-009 and `ZipArchive`'s rustdoc.
- The lost STORED-case throughput and the over-RAM `Cursor<Mmap>` option remain available as a
  future opt-in feature if a real workload ever shows the win the common case did not (Option C);
  it is explicitly **not** implemented here because it would re-import the RSS profile and the
  SIGBUS hazard the collapse just removed.

## Amendment (2026-08-12, Review 0001 R0001-0029 — the re-homed guard no longer re-decodes)

The collapse amendment above describes the re-homed duplicate-name guard as attributing by a
"UTF-8-lossy re-decode". That is no longer how it works: `ZipArchive::scan_duplicate_names` keys on
the raw central-directory name bytes and joins them to the crate's listing through
`ZipFile::name_raw()`, because two byte-identical non-UTF-8 records lossy-decode to one key and were
therefore reported as fully attributed while remaining reachable by name. The
raw-count-vs-deduped-count backstop survives and is now precise duplicate-record accounting rather
than an `is_empty()` heuristic. Full detail, and the residual that genuinely remains, are in DCR-009's
2026-08-12 amendment. The dual-backend ruling this record carries is unaffected.
