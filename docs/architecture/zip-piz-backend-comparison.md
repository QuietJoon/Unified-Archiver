---
type: Design Analysis
title: "ZIP backend comparison: piz vs the `zip` crate (dual-backend revert input)"
description: "Historical. Read-only investigation that fed the AD 0007 dual-ZIP-backend revert decision; the collapse landed 2026-07-23 as DCR-009, so the dual-backend tree it describes no longer exists."
tags: [design-analysis, architecture, zip, piz, ADR-0007, ADR-0027, R0079-0026, R0081-0076, R0081-0077, OI-0080-002, OI-0076-002]
timestamp: 2026-07-20T00:00:00Z
status: advisory
---

# ZIP backend comparison: `piz` vs the `zip` crate

> **Outcome recorded 2026-08-06.** The question this document was written to inform has been
> answered: the owner approved the R4 collapse on 2026-07-23 (AD 0007's collapse amendment,
> executed as DCR-009). The `piz` backend, the `memmap2` dependency and the mmap size cap are
> gone; the `zip` crate is the sole ZIP reader for encrypted and unencrypted archives alike.
> Everything below describes the pre-collapse tree and is kept as the analysis behind that
> decision — read it as history, not as a description of the current backend layout.

**Status: advisory (design input, not a decision).** This document is the detailed
analysis behind revert-candidate **R4** in `docs/architecture/decision-review-2026-07-19.md`
("AD 0007: dual ZIP backend strategy — benchmark-gated revert"). It does **not** change any
code or record; it exists so the AD 0007 owner can decide whether a *single* crate can replace
the current dual-backend split without a divergence-class regression.

## The question

AD 0007 (`docs/records/AD-0007-dual-zip-backend-strategy.md`) established two ZIP
read backends:

- **`piz`** (mmap-based) — the default for unencrypted ZIP, reached via `Archive::open()` →
  `PizArchive` (`src/ffi/piz_wrapper.rs`).
- **the `zip` crate** (zip-rs/zip2, `ZipArchive` over `std::fs::File`) — used when
  `open_encrypted()` is called, reached via `ZipArchive::open_with_password`
  (`src/ffi/zip_wrapper.rs`).

AD 0007 recorded the downside as "*slight* behavioral differences." That has since grown into a
**divergence class** — three distinct bug families that all trace back to running one format
through two independent parsers:

| Divergence | Record | Status |
|---|---|---|
| Duplicate-name handling (zip: last-record-wins dedup; piz: keeps all records → gate rejects) | R0079-0026 | live asymmetry, documented |
| Special-type entry mapping (FIFO/device/socket/symlink split between backends) | R0081-0076 / R0081-0077 | fixed via shared `classify_zip_entry_type` |
| mmap SIGBUS / silent-mutation hazard (piz only; File reads are immune) | OI-0080-002 | OPEN |

Plus a structural cost: two backends double the single-entry defense surface (OI-0076-002).

The task: can we collapse to one crate, and if so which?

## Pinned versions and enabled features (from `Cargo.toml` / `Cargo.lock`)

- `piz = "0.5"` → resolved **0.5.1**. Cargo.toml comment: *"Native Rust ZIP inspection/extraction
  backend with CRC32 metadata; **extraction is sequential**."*
- `memmap2 = "0.9"` — required by piz (piz reads from `&[u8]`; we hand it an mmap).
- `zip = { version = "2.2", default-features = false, features = ["deflate", "aes-crypto"] }`
  → resolved **2.4.2**. Only `deflate` + `aes-crypto` are enabled — **no bzip2/zstd/xz/ppmd**.
- Dev-dependency `zip` uses `["deflate"]` only (wire-level metadata assertions in tests).

Consequence of the feature set: **both** backends handle only *stored* and *deflate* ZIP entries.
Anything else (bzip2/zstd/xz-in-ZIP) is out of scope for both and would route to libarchive. So
the compression-method axis is **not** a differentiator between piz and zip here.

## Feature comparison

Sources: context7 `/zip-rs/zip2` (574 snippets) and `/mrkline/piz-rs` (15 snippets — see the
context7 caveat at the end), plus the in-repo wrappers `src/ffi/piz_wrapper.rs` and
`src/ffi/zip_wrapper.rs`.

| Capability (relevant to this library) | `piz` 0.5.1 | `zip` 2.4.2 (this tree) |
|---|---|---|
| **ZipCrypto (legacy PKWARE) decryption** | **No** | **Yes** — `by_index_decrypt(i, pw)` |
| **AES-128/192/256 decryption** | **No** | **Yes** (`aes-crypto` feature); tracks `AesVendorVersion` AE-1/AE-2, `is_ae2_encrypted()` |
| Encrypted **creation** | No (read-only crate) | Crate *can* (`with_aes_encryption`) but the library **rejects** it (MADR-0027) |
| Detects an entry is encrypted | Yes (`entry.encrypted`, cannot decrypt) | Yes (`ZipFile::encrypted()`) |
| **Symlink** read + classify | Yes, via `unix_mode` (piz `is_file()` is false for symlinks too — R0069-0018) | Yes, via `unix_mode` |
| **Special types** (FIFO/char+block device/socket) | mapped by the **shared** `classify_zip_entry_type` → `EntryType::Other` | same shared classifier → `EntryType::Other` |
| Classifier location | **imports** `zip_wrapper::classify_zip_entry_type` | **owns** `classify_zip_entry_type` (single source of truth, R0081-0076/0077) |
| **ZIP64** (>4 GB, >65535 entries) | Yes (README: contiguous byte range on 64-bit even if archive > RAM) | Yes (read; `large_file(true)` on write) |
| **0x5455 extended-timestamp READ** | **Only** via a second `zip`-crate pass over the same mmap bytes (`read_zip_extended_timestamps`) | **Native** — `ExtendedTimestamp::try_from_reader`, `extra_data_fields()` |
| 0x5455 **WRITE** | N/A (read-only) | Yes (used by `zip_writer.rs` for atime/ctime round-trip, OI-0065-002) |
| **Duplicate-name** handling | **Keeps every central-directory record**; facade gate rejects the collision | **Dedupes upstream (last-record-wins)** — rejection *cannot* trigger (R0079-0026) |
| CRC32 from metadata | Yes (`entry.crc32`) | Yes (`ZipFile::crc32()`); AE-2 CRC=0 exempted (`crc32_check_exempt`) |
| **Streaming vs whole-file** | **Whole-file mmap** (or full in-memory `&[u8]`), per-entry decompress | **Read+Seek streaming** from `File` (or any `Cursor`) |
| **Memory model / peak RSS** | mmap is lazy-paged by the OS; RSS ≈ touched pages + largest decompressed entry (map itself is not committed RAM) | streaming buffers; RSS ≈ inflate window + entry buffer; **no whole-file map** |
| **Whole-archive vs per-entry** | one shared mmap, per-entry readers are `Send` (parallel-capable) | `by_index*` take `&mut self` → one handle is **serial**; parallel needs N handles/reopens |
| Parallelism **actually used here** | **No** — `rayon` is not a dependency; wrapper is sequential (Cargo.toml comment + AD 0005 de-facto reverted per decision-review R3) | No (same single-handle model) |
| **Non-UTF-8 / CP437 names** | paths surfaced as UTF-8 (`camino`-style `Utf8Path`); CP437-only / non-UTF-8 names are lossy or rejected at parse | decodes CP437 via the GP-flag bit 11; exposes `name()` (lossy) **and** `name_raw()` (bytes); raw-byte write API |
| Compression methods (this tree) | Stored, Deflate (`Unsupported(code)` otherwise) | Stored, Deflate only (`default-features=false`) |
| **External-mutation hazard** | **SIGBUS** on truncation past EOF; silent byte-swap on same-length overwrite (live mapping — OI-0080-002) | **None of that class** — File reads surface truncation as an ordinary I/O error, not a signal |
| Reads from an in-memory / mmap byte range | Native (`ZipArchive::new(&bytes)`) | Yes — `ZipArchive<Cursor<&[u8]>>` / `Cursor<Mmap>` (already used in-tree for the 0x5455 pass) |
| **Maintenance signal** | 0.5.1; small single-maintainer crate; sparse docs (15 context7 snippets); low release cadence | 2.4.2; actively maintained zip2 fork; broad API (574 snippets); frequent releases |

### Key reads of the table

1. **Decryption is one-directional.** piz has *no* decryption path at all — the context7
   metadata example literally prints *"WARNING: File is encrypted (decryption not supported)"*.
   The `zip` crate owns both ZipCrypto and AES (AE-1/AE-2). This single fact fixes the shape of
   the answer: **any single-backend design must be the `zip` crate**, because piz can never serve
   the encrypted path.

2. **The shared classifier already closed the special-type divergence.**
   `classify_zip_entry_type(unix_mode, name_is_dir)` (in `zip_wrapper.rs`, imported by
   `piz_wrapper.rs`) is now the one place that maps `S_IFLNK → Symlink`, `S_IFDIR → Directory`,
   `S_IFREG → File`, and every other Unix type nibble → `Other`. So R0081-0076/0077 is no longer a
   *behavioral* divergence — but it remains a *structural* one: the fix is a shared function that
   both parsers must keep calling. Collapsing to one backend deletes the obligation, not just the
   bug.

3. **The duplicate-name divergence is real and load-bearing.** Because the `zip` crate's
   central-directory map dedupes on name (last-record-wins), iterating `0..zip.len()` via
   `by_index_raw` yields **one record per name** — the backend physically cannot see, and
   therefore cannot reject, a duplicate. piz keeps all records, so the facade gate
   (`check_single_entry_safe`) sees the collision and rejects. Today that means: **unencrypted
   ZIPs reject duplicate names; encrypted ZIPs silently take last-record-wins.** Any collapse to
   the `zip` crate makes *everything* behave like today's encrypted path unless duplicate
   detection is re-implemented over the raw local-header/central-directory stream.

4. **piz's headline feature is unused.** AD 0007 justified piz with *"fast parallel ZIP
   reading."* This library never parallelizes ZIP reads: `rayon` is not a dependency, no internal
   parallel-extract path exists (decision-review R3), and the Cargo.toml comment states extraction
   is sequential. The *only* realized piz advantage over the `zip` crate is **mmap lazy paging on
   the single-threaded path** — and that advantage carries the SIGBUS liability (OI-0080-002) that
   File-based reads do not have.

5. **piz already leans on the `zip` crate anyway.** The 0x5455 extended-timestamp data that piz
   cannot parse is recovered by a second pass: `read_zip_extended_timestamps` builds a
   `zip::ZipArchive<Cursor<&[u8]>>` over the same mmap bytes. So the "unencrypted path is
   piz-only" story is already false — every default listing runs *both* parsers over the archive.

## Options

### Option A — collapse to the `zip` crate only (drop piz + memmap2)

**Does it cover the fast unencrypted path acceptably?** Functionally yes: the `zip` crate reads
stored/deflate entries, ZIP64, symlinks, special types (via the shared classifier), CP437 names,
and native 0x5455 — a *superset* of what piz surfaces, and it already handles the encrypted path.

**What mmap speed is lost?** The `zip` crate over `std::fs::File` does `seek`+`read` per entry
instead of touching mapped pages. For deflate-dominated archives the cost is almost entirely
inflate CPU, so the read-substrate difference is expected to be small; for *stored* (uncompressed)
large entries the mmap zero-copy edge is larger. Magnitude is unknown — this is exactly what the
benchmark below must measure.

**Can a self-managed mmap `Cursor` recover it?** Yes, and it is *already proven in-tree*:
`zip::ZipArchive<Cursor<Mmap>>` (or `Cursor<&[u8]>`) gives the `zip` crate the same mapped bytes
piz would use, recovering the zero-copy read for the stored-entry case. **Caveat:** doing so
*re-imports* the OI-0080-002 SIGBUS/silent-mutation hazard that plain `File` reads avoid — so the
speed recovery and the safety win are in tension. A `File`-only Option A is the safest and
simplest; a `Cursor<Mmap>` variant trades that safety back for throughput.

- **Eliminates:** the special-type structural obligation (one classifier, one parser); the mmap
  SIGBUS hazard **if** it uses `File` reads (OI-0080-002 closes); the doubled single-entry defense
  surface (OI-0076-002 halves); the whole R0079-0026 *divergence* (only one behavior remains).
- **Costs:** (1) loses the mmap fast path unless `Cursor<Mmap>` is reintroduced (which re-opens
  OI-0080-002); (2) **the default unencrypted path silently loses duplicate-name rejection** —
  today's piz-reject behavior would regress to last-record-wins unless duplicate detection is
  re-implemented over raw entries; (3) drops two dependencies (a maintenance *win*).

### Option B — collapse to `piz` only

**Can it ever decrypt?** **No.** Confirmed by context7 (`entry.encrypted` is surfaced only as a
"decryption not supported" warning) and by the crate's read-only, no-crypto API. There is no
ZipCrypto or AES path, and none is on the crate's roadmap given its maintenance cadence.

**Verdict: non-starter.** `open_encrypted()` for ZIP is a shipped, tested capability (AES AE-1/AE-2
+ ZipCrypto). A piz-only library cannot read a single encrypted ZIP, which is a hard regression.
Option B is rejected on capability grounds before cost even enters.

- Eliminates: nothing worth having (it would delete encrypted-ZIP support).
- Costs: removes a core feature. Not viable.

### Option C — keep both, feature-gate `piz` behind a `mmap-zip` feature

Make the **default** build a single backend (the `zip` crate, over `File`) and put piz + memmap2
behind an opt-in `mmap-zip` cargo feature for callers who have benchmarked their workload and want
the mmap edge.

- **Eliminates (in the default surface):** all three divergence families — duplicate-name,
  special-type, and mmap SIGBUS are *absent by default*. A default-features consumer gets one
  parser, one behavior, no SIGBUS.
- **Costs:** the divergence class still *exists* when `mmap-zip` is enabled, so the two-backend
  test matrix (single-entry parity suite, duplicate/special-type parity) must stay green for both
  feature states — the maintenance cost is not deleted, only moved off the default path. Adds a
  feature-flag axis to CI.
- This is the **hedge**: it captures Option A's default-surface cleanliness while preserving piz
  for the case where the benchmark shows a large, workload-relevant mmap win.

## Recommendation

**Run the benchmark, then default to Option A (`zip`-crate-only over `File`); fall back to Option
C only if the benchmark shows a large, real mmap win.** Rationale:

- The **capability** argument is settled: the single backend must be the `zip` crate (Option B is
  dead). The only open variable is the **throughput** cost of dropping piz's mmap, and whether
  that cost justifies keeping a second backend.
- piz's *advertised* advantage (parallel reads) is **not used** by this library, and its *actual*
  advantage (mmap lazy paging) is precisely the source of the still-open SIGBUS hazard
  (OI-0080-002). Removing piz turns an OPEN hardening issue into a non-issue.
- The `zip` crate is a strict functional superset here (it already serves the harder encrypted
  path and the 0x5455 pass piz cannot do), is more actively maintained, and would let the library
  drop two dependencies.
- The one genuine behavioral loss in Option A — duplicate-name rejection on the unencrypted path —
  is a **known, bounded** work item (re-implement duplicate detection over raw entries), not an
  open-ended risk, and it also *removes* the current reject-vs-accept asymmetry between the
  encrypted and unencrypted paths.

If, and only if, the benchmark shows piz's mmap path materially beating the `zip` crate on a
realistic workload (see below), choose **Option C** so the default surface is still single-backend
and the mmap fast path survives as an explicit opt-in. Do **not** recover mmap speed by putting the
`zip` crate over `Cursor<Mmap>` in the default build — that reintroduces OI-0080-002 without the
isolation a feature gate provides.

Whichever way the benchmark points, record the outcome as an **amendment to AD 0007** (append a
dated section; do not rewrite the Decision Outcome — per the immutability policy AD 0019/AD 0065
already follow), and cross-link OI-0080-002 and R0079-0026.

## The one benchmark that settles it

**piz mmap read throughput vs `zip`-crate `File` read, extracting every entry of a representative
large unencrypted ZIP.** A `benches/` harness already exists (`benches/sfx_detection.rs`), so add a
sibling criterion bench.

- **Fixture:** one realistic large ZIP (~1–2 GB total, hundreds–thousands of deflate entries) plus
  a second *stored* (uncompressed) large-entry ZIP to isolate the zero-copy edge where it is
  biggest. Build fixtures under `/Volumes/Temp/claude/7zip/`.
- **Arms (single-threaded, matching the shipped model):**
  1. piz over `Mmap` — the current default (`PizArchive::extract_all`).
  2. `zip` crate over `std::fs::File` — the proposed Option A default.
  3. `zip` crate over `Cursor<Mmap>` — the "recover mmap speed" variant (informational; measures
     what Option A-with-mmap or Option C would buy).
- **Metrics:** wall-clock for full extraction *and* peak RSS (the mmap memory story only matters if
  archives exceed RAM).
- **Decision rule:** if arm 1 beats arm 2 by less than ~15–20% wall-clock on the deflate fixture
  (the common case), the mmap edge does not justify a second backend → **Option A**. If the gap is
  large (driven mostly by the stored fixture / archives-larger-than-RAM), keep the fast path as an
  opt-in → **Option C**. Arm 3 tells you how much of any gap is recoverable *without* piz — and
  therefore whether the second crate is buying anything a `Cursor<Mmap>` could not.

Do **not** run this benchmark as part of this investigation — it is the decision input the AD 0007
owner should commission next.

## Context7 availability note

- `zip` (zip-rs/zip2) is well covered on context7 (`/zip-rs/zip2`, 574 snippets): decryption
  (`by_index_decrypt`, `AesVendorVersion`, `is_ae2_encrypted`), ZIP64 (`large_file`), extended
  timestamps (`ExtendedTimestamp::try_from_reader`), and raw-byte names (`start_file_with_options`)
  were all confirmed directly.
- **`piz` is a small crate** (`/mrkline/piz-rs`, only **15 snippets** — as anticipated). Context7
  has its README, the parallel-extraction and metadata examples, and the byte-slice/mmap
  constructors, which was enough to confirm the load-bearing facts (mmap + in-memory read, ZIP64,
  `entry.encrypted` with **no decryption**, `is_file()`/`unix_mode`/`crc32` metadata, Send readers
  for rayon). Anything finer-grained was cross-checked against the in-repo usage in
  `src/ffi/piz_wrapper.rs` and the pinned `piz 0.5.1` behavior rather than against context7.

## Cross-references

- AD 0007 — `docs/records/AD-0007-dual-zip-backend-strategy.md` (the decision under
  review; amend with the benchmark outcome).
- MADR-0027 — `docs/records/MADR-0027-reject-encrypted-archive-creation.md` (why encrypted *creation*
  is out of scope; relevant if a future revert re-opens ZIP-AES creation via the `zip` crate).
- OI-0080-002 — Piz mmap lifetime vs external truncation (the SIGBUS hazard Option A/C removes from
  the default path).
- R0079-0026 — duplicate-name dedup asymmetry (the one behavior Option A must consciously
  re-preserve).
- R0081-0076 / R0081-0077 — special-type mapping, fixed by the shared `classify_zip_entry_type`.
- OI-0076-002 — the doubled single-entry defense surface a single backend collapses.
- decision-review-2026-07-19.md §A/R4 — the revert candidate this analysis expands.
