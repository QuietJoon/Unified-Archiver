---
type: Reference
title: Capability Matrix
description: What each archive format can and cannot do in this library, and why — the canonical per-format, per-platform capability reference.
tags: [reference, capabilities, formats, platforms]
---

# Capability Matrix

This library's product is **one interface across every archive format** (AD 0072).
That promise is about shape and honesty, not coverage: the same call spelling, the
same way of asking what a format supports, and the same way of refusing. Not every
format supports every operation — and **what cannot be done is documented here as
something that cannot be done, with the reason.**

This file is the canonical answer. Where it disagrees with README, the crate
rustdoc, or `Limitations.md`, this file is right and the other should be fixed.

## How to read this

| Mark | Meaning |
|---|---|
| ✅ | Supported |
| △ | Partial — works, but with a stated limit. **Always read the reason.** |
| ❌ | Not supported |

Every ❌ and △ carries a **kind**, because "not implemented" without one tells a
reader nothing about whether to wait, work around it, or ask:

| Kind | Meaning | What a user should do |
|---|---|---|
| `format` | The archive format itself has no such concept | Nothing — it will never exist |
| `upstream` | The dependency cannot do it | Wait on the dependency, or ask us to wrap a second one |
| `scope` | We decided not to offer it | Ask, if you need it — decisions can change |
| `hold` | Decided, then paused by the owner | Ask; the design exists |
| `platform` | Needs a host or licence we do not assume | Use a supported host |
| `unbuilt` | In scope, nothing blocks it, simply not written | Ask, or expect it eventually |
| `unknown` | **The reason is not recorded anywhere.** | Tell us — this is a documentation defect |

`unknown` entries are listed at the end. They are gaps in this document as much as
in the code, and they are the first thing to fix.

## Platform verdict — read this before the table

The table below describes a host where everything links. That is **macOS today, and
nothing else**.

- **macOS** — the only platform with a recorded verification run under AD 0070.
  Everything in the table applies. Note the arm64 caveat under *Architecture*.
- **Linux** — code-identical to macOS for every capability checked; no
  `cfg(target_os = "linux")` changes any of them. But **no verification run has ever
  been recorded on Linux**, so every cell is `untested` rather than confirmed.
- **Windows** — **the crate does not currently build in its default feature set.** Not
  "some formats are unavailable" — nothing compiles in that configuration. No *other*
  Windows configuration has a recorded compile result either, so a reduced build is
  unknown rather than known-broken. The gate's `check-windows-cfg` lane now probes a
  libarchive-free set (`read,zip-read,create,zip-write,external-rar-create`), but it is
  owner-invoked only and has not been run. Two independent
  causes, both verified by reading the tree: `src/ffi/libarchive_wrapper/writer.rs`
  calls `libc::_open_osfhandle` / `libc::_close`, which are spelled
  `open_osfhandle` / `close` in the pinned libc (the leading underscore is the C
  symbol, carried by `#[link_name]`, not the Rust path); and `build.rs` emits no
  link directives on Windows while `#[link(name = "archive")]` is unconditional, so
  MSVC demands `archive.lib` regardless. Both causes now live inside the `libarchive` feature — `src/ffi.rs` gates
  `libarchive` and `libarchive_wrapper` on it, and `#[link(name = "archive")]` sits
  inside `src/ffi/libarchive.rs` — so a build that omits `libarchive` reaches neither.
  A ZIP-only Windows build (`--features read,zip-read`) is therefore no longer excluded
  by these two causes; it has simply never been compiled. (Bare `--no-default-features`
  is not a valid build in any case.) Windows work is deferred by owner ruling; this entry documents the
  state, it does not propose a fix.

## The matrix

Formats are grouped by backend, because the backend is what most limits are about.

| Format | List | Extract | Create | Modify | Read encrypted | Create encrypted | Read split | Create split | SFX in place | Streams without buffering |
|---|---|---|---|---|---|---|---|---|---|---|
| **ZIP** | ✅ | ✅ | ✅ | △ | △ | ❌ | △ | ❌ | △ | △ |
| **7z** | ✅ | ✅ | ✅ | △ | ✅ | ❌ | ✅ | ❌ | △ | △ |
| **RAR / RAR5** | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ✅ | ❌ | △ | ❌ |
| **TAR** | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **TAR.GZ / .BZ2 / .XZ** | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **TAR.ZST / .LZ4 / .LZMA** | ✅ | ✅ | △ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **GZIP / BZIP2 / XZ** | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **ZST / LZ4 / LZMA** | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **ISO** | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

Reading and extracting are the two things every format does. Everything else varies,
and the rest of this document is why.

## Create

**Supported:** ZIP (via the `zip` crate) and 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ,
TAR.ZST, TAR.LZ4, TAR.LZMA (via libarchive).

- **RAR / RAR5 — ❌ `upstream`.** The only RAR engine here is the vendored UnRAR
  SDK, which decompresses only. Its licence forbids using the source to build a
  RAR-compatible archiver, so no in-process RAR writer can exist. A Windows-only,
  opt-in `external::RarCreator` shells out to a licensed WinRAR `rar.exe` — but it
  is `platform`-gated, is not part of the uniform facade, and **is not reachable
  today because the Windows build is broken**. RAR creation currently works on no
  platform.
- **GZIP / BZIP2 / XZ / ZST / LZ4 / LZMA — ❌ `scope`.** Standalone single-file
  compressor creation is out of scope and stays out (AD 0018, re-affirmed
  2026-09-05 under AD 0072). libarchive *could* emit these via
  `archive_write_set_format_raw`; capability does not imply scope. Produce these
  codecs through the `TAR.*` compound formats instead.
- **TAR.ZST / TAR.LZ4 / TAR.LZMA — △ `upstream`.** In scope and implemented, but
  creation succeeds only if the libarchive this build linked was compiled with the
  matching **write** filter. That is a property of the C library on the host, so
  `can_create()` still answers yes and the truth is learned at writer construction:
  a missing codec raises `CodecUnavailable { codec, format, install_instructions }`
  rather than silently shelling out to an external compressor. There is no
  pre-flight API to ask whether the codec is present.
- **ISO — ❌ `unknown`.** See *Reasons not established*.

## Modify

**Supported:** ZIP and 7z only, and both are △ — never `Full`.

- **ZIP, 7z — △ `scope`.** A commit cannot re-emit anything that is not a regular
  file or a directory. Retained symlinks, hard links and special entries (FIFO,
  socket, device) are **dropped from the output**; the operation is not refused and
  nothing is substituted. This was ruled warn-not-refuse, so `commit_changes()`
  succeeds silently — only `commit_changes_with_warnings()` returns the
  `SkippedUnsupportedEntry` warnings that say what was lost.
- **7z — △ `upstream`, additionally.** The rewrite is written by libarchive's 7z
  writer, which cannot reproduce most of what a 7-Zip-produced archive contains: no
  BCJ/BCJ2/delta/branch filters, no AES-256, and one archive-wide compression
  setting rather than per-folder. A modified 7z is a valid 7z, not a faithful copy
  of the original's structure.
- **ZIP — △ consequence of AD 0071.** Modify reads ZIP through libarchive while the
  write side stays on the `zip` crate. Visible effects: commit reopens the archive
  and walks the central directory a second time to recover the comment and per-entry
  compression method, and a modify-mode `list_files()` reports *different* metadata
  than `Archive::open` on the same file (no `crc32`, no `compressed_size`, no
  `attributes`).
- **RAR / RAR5 — ❌ `upstream`.** The UnRAR FFI surface is open / read-header /
  process-file / close / set-password / set-callback / version. There is no write
  entry point to rewrite into.
- **TAR family — ❌ `scope`.** Nothing technical blocks it: all seven variants are
  creatable and the commit path is format-agnostic. It is the capability gate alone.
- **GZIP / BZIP2 / XZ / ZST / LZ4 / LZMA — ❌ `format`.** A single compressed byte
  stream is not an entry container. libarchive presents exactly one synthetic entry,
  so "add", "remove" and "replace" have no meaning.
- **ISO — ❌ `unknown`.** Same missing reason as ISO creation.

## Encryption

**Read:** ZIP △, 7z ✅, RAR / RAR5 ✅. Everything else ❌ `format` — tar, the
standalone streams and ISO 9660 define no encryption, so there is nothing to
decrypt. `Archive::open_encrypted` says exactly that rather than "not yet".

- **ZIP read — △ `upstream`.** ZIP reports `Support::Full`, but the `zip` crate
  covers two of ZIP's encryption schemes: WinZip AES (AE-1/AE-2) and legacy
  ZipCrypto. It decides "encrypted" from general-purpose flag bit 0 alone and never
  inspects bit 6 (strong encryption) or bit 13 (central-directory encryption). A
  PKWARE Strong-Encryption ZIP is not readable. The declared `Full` over-claims.
- **Create encrypted — ❌ for every format.** For ZIP and 7z the kind is **`hold`,
  not `scope`**: MADR-0027 originally banned it permanently, its 2026-07-20
  amendment reversed that to a default-off opt-in, and its 2026-09-02 amendment
  records that the owner **paused** that opt-in on 2026-09-01. The `zip` crate can
  write AES entries (`FileOptions::with_aes_encryption`, already enabled here for
  reading), so this is not an upstream limit. For RAR it is `upstream`; for
  everything else `format`.
- **Encrypted modify — ❌ `hold`.** `Archive::modify` refuses an encrypted source
  before any rewrite starts, because the writer cannot emit encrypted output and
  proceeding would silently hand back a **plaintext** archive.

## Split / multi-volume

**Read:** RAR / RAR5 ✅, 7z ✅, ZIP △. **Create: ❌ for every format.**

- **RAR / RAR5 — ✅.** Open the first volume; a file split across volumes lists as
  one logical entry that every read route reaches. Two consequences: the coalesced
  entry reports `crc32: None` (each volume carries a CRC over its own fragment, not
  over the file, so surfacing one would verify nothing), and a set with a volume
  missing fails at listing with an error naming what is absent.
- **7z — ✅** (since 2026-09-05). A 7z volume set is a plain byte split of one
  archive, so the members are read as their concatenation. An **incomplete** set is
  refused by name before any bytes are read — a byte split has nothing marking a
  boundary, so a missing middle volume would otherwise decode as corruption rather
  than as an absence.
- **ZIP — △ `upstream`.** Detection only: sibling names (`base.z01`, `base.z02`, …)
  are enumerated and never opened, and no read route spans segments. The `zip` crate
  refuses multi-disk archives, and unlike 7z a split ZIP is **not** a plain byte
  split (spanning marker, disk-relative central-directory offsets), so the
  concatenation trick that unlocked 7z does not apply.
- **Create split — ❌ `scope` (DEF-002).** `CompressionOptions::split_size` exists as
  a public field but no writer consumes it, so rather than accept it silently
  `validate_for_format` rejects any `Some(_)` for every format.
- **TAR family, standalone streams, ISO — ❌ `unknown`.** See *Reasons not
  established*.

## Streaming — processing an entry without writing it to disk

This is the library's own definition of streaming, and it is narrower than what each
dependency calls streaming: **read and process an entry in memory, without a temp
file** — the shape checksum and integrity work needs.

- **All 14 libarchive-backed formats — ✅.** `archive_read_data` reads straight into
  the caller's buffer. Nothing is materialised, no temp file, memory is O(buffer).
- **ZIP, 7z — △ `upstream`.** They stream in name only: the whole entry is decoded
  into a `Vec<u8>` and handed back as a cursor, so peak memory is the entry's
  uncompressed size. Neither dependency hands out an owned per-entry reader — the
  `zip` crate's `ZipFile<'_>` borrows from the archive, and `sevenz-rust2`'s entry
  data is callback-scoped. No disk is touched, so the owner's definition is met on
  the disk axis but not on the memory axis.
- **RAR / RAR5 — ❌ `unbuilt`.** The worst failure mode of the three and the one that
  fails the definition outright: the entry is written to a **real file on disk**
  under the OS temp dir, then read back into memory. The in-code comment blaming the
  UnRAR API is wrong — the vendored SDK fires `UCM_PROCESSDATA` with each decoded
  block, which this crate already relies on elsewhere. The real obstacle is adapting
  a push callback to a pull `Read`.
- **7z as a modification source — △ `upstream`.** A commit pulls each retained entry
  fully into memory, so peak RSS during a modify scales with the largest retained
  entry.
- **No way to ask.** `FormatCapabilities` has no streaming or memory-bound field, so
  a caller cannot branch on "will this be buffered". Under AD 0072's "same way of
  asking", this axis fails that test today.

## Self-extracting archives (SFX)

**Detectable payloads (7):** ZIP, RAR, RAR5, 7z, GZIP, BZIP2, XZ. A `.tar.gz`
payload is found through its gzip signature.

**Opened in place (no copy):** ZIP, 7z, RAR / RAR5 — all △, because two further
gates apply.

- **Outer path must carry an executable extension** — exactly `exe`, `com`, `scr`,
  `app`, `run`, `sh`, `bash`. A ZIP payload behind `installer.bin` or an
  extensionless file stages instead. Not cosmetic: a password-bearing extraction
  reopens the outer file and only the SFX fallback resolves an embedded payload.
- **An encrypted payload always stages**, even for the three formats that otherwise
  open in place, and under the built-in 16 GiB ceiling rather than any caller limit.
- **RAR additionally declines** for a payload at offset 1–6, past UnRAR's ~4 MiB
  scan reach, or when an earlier `Rar!` byte run appears in the stub.
- **All libarchive-backed payloads stage a full copy — ❌ `scope`, decided
  2026-09-05.** In practice this means **makeself installers** (`.run` / `.sh`: a
  shell stub plus a gzip-compressed tar — NVIDIA drivers, CUDA, VirtualBox's Linux
  packages), which are routinely multi-GB. Every in-place arm proves the payload
  from magic bytes *before* a backend is built; libarchive has no such cheap anchor,
  so confirming it would mean constructing a reader and discarding it on failure.
  This is a performance difference, not an interface one — those files open, list
  and extract normally; they pay a copy. Staging is capped at 16 GiB and pre-checks
  free temp space.
- **TAR, ISO, LZMA, TAR.LZMA — ❌ `format`.** Undetectable at an arbitrary offset:
  tar's `ustar` sits at +257 and is optional, ISO's anchor is at +32769, raw LZMA has
  no stable magic. For all four the file extension is the authoritative signal, and
  an embedded payload has no extension.
- **Only the first 1 MiB is scanned** for a signature, for any format.
- **ZST / LZ4 — ❌ `unknown`.** See below.

## Per-entry metadata fidelity

Same interface, different completeness. This is the axis with the widest spread, and
there is **no way to ask** — `FormatCapabilities` has no metadata field.

Always populated everywhere: `path`, `id`, `entry_type`, `is_encrypted`.

- **`crc32`** — ZIP, 7z, RAR (non-split) yes. `None` for the whole TAR family and
  ISO (`format` — tar carries a header checksum, never a payload CRC). `None` for
  GZIP / BZIP2 / XZ even though those formats *do* store a stream checksum
  (`scope` — exposed through `stream_crc` instead of the uniform field). `None` for
  split RAR entries (`format`, as above).
- **`compressed_size`** — `None` for all 14 libarchive formats (`upstream` —
  libarchive's entry API publishes no stored-size accessor), so
  `compression_fraction()` returns `None` for them. For 7z it is a **per-block**
  number misattributed per entry: the first file in a block reports the whole
  block's packed size and its siblings report `Some(0)`.
- **`link_target`** — populated **only** by the libarchive reader. ZIP / 7z / RAR
  classify symlinks and hard links but always report `None`, which violates the
  crate's own documented entry-kind invariant.
- **`comment`** — never populated by any backend. `upstream` for RAR (UnRAR zeroes
  it) and libarchive (no accessor); `unbuilt` for ZIP and 7z.
- **`created` / `accessed`** — `None` for 7z. The in-code comment blames
  `sevenz-rust2`, but the pinned version publishes both — so this is `unbuilt`, not
  `upstream`.
- **`raw_path`** — populated only by libarchive, so a non-UTF-8 name in a ZIP / 7z /
  RAR archive leaves it `None`, which by the field's own contract asserts `path` is
  byte-exact when it may be lossy.
- **`permissions`** — for a DOS/Windows-created ZIP this is **fabricated**: the `zip`
  crate synthesises `0o664`/`0o775` from the MS-DOS directory bit. It is not archive
  data.
- **Modify mode changes the answers.** The same ZIP or 7z yields different metadata
  through `Archive::modify` than through `Archive::open`, because modify reads via
  libarchive.

## Platform differences

macOS and Linux are **code-identical** for every capability above; no
`cfg(target_os)` changes any of them. The real split is Unix versus Windows, and it
is about guarantees rather than formats.

| Property | macOS / Linux | Windows |
|---|---|---|
| Builds at all | ✅ | ❌ — see the platform verdict above |
| Everything libarchive-backed | ✅ | ❌ `platform` — no link configuration is emitted |
| Unix permission bits, stored and restored | ✅ | △ — dropped; a placeholder is stored |
| Inode-based identity checks (TOCTOU) | ✅ | △ — weaker identity |
| Parent-directory fsync on durable writes | ✅ | △ — not available |
| Non-UTF-8 entry names on disk | Linux ✅ / macOS ❌ (APFS rejects them with `EILSEQ`) | Different again — WTF-8 of UTF-16 |
| `external::RarCreator` | ❌ — module does not exist | `platform` — needs the opt-in feature *and* a licensed WinRAR |

Archive-name policy is **not** platform-dependent: `ArchivePathPolicy::Portable`
rejects Windows reserved device names, `:` and trailing dot/space on every host, so
archives written on Unix are Windows-safe by default.

## Architecture (Intel / ARM)

**There is no architecture-dependent capability difference.** The crate's own code
contains zero `target_arch`, `target_feature`, `target_pointer_width` and
`target_endian` conditionals. Endianness is explicit throughout — 45 `from_le_bytes`
sites, deliberate `from_be_bytes` where a format is big-endian, and zero
`from_ne_bytes` / `transmute` / `read_unaligned`. The Mach-O parser derives byte
order from the file's own magic rather than the host's.

The README's "SIMD-accelerated CRC32" comes from the `crc32fast` dependency and is a
**speed** difference only — same bytes in, same bytes out. The vendored UnRAR
likewise selects SSE/AES-NI or NEON from compiler-predefined macros, with identical
results.

Two honest △s, both **evidentiary rather than functional**:

- **x86_64 is untested everywhere, including macOS.** Every AD 0070 verification
  record was produced on an arm64 host — the gate records `uname -srm` and all four
  read `arm64`. Nothing suggests x86_64 is broken; there is simply no record.
- **7z / XZ / LZMA decoding runs different machine code per architecture.**
  `lzma-rust2` ships hand-written per-arch assembly for the LZMA range decoder and
  SIMD match-finder variants. Only the aarch64 path has ever run here.

32-bit hosts are not a declared target. A 4 GiB+ archive would behave differently
there, but **by design it fails closed** with a typed error rather than truncating.

## Reasons not established

These gaps are real and their reason is recorded **nowhere** in the tree. They are
documentation defects, and they need an owner decision to close.

1. **ISO creation and modification.** `can_create()`'s rustdoc says "libarchive's
   ISO support is read-only". **That is false** — the linked libarchive declares and
   exports `archive_write_set_format_iso9660`. The crate simply never binds the
   symbol, and no record says whether ISO writing is out of scope or merely unbuilt.
   README and `docs/API_REFERENCE.md` repeat the same unsourced claim.
2. **Split reading for the TAR family, the standalone streams, and ISO.** All carry
   `multipart_read: Support::None` with no comment, no record and no backlog entry.
   It is demonstrably **not** an upstream block: libarchive exposes
   `archive_read_open_filenames` for exactly this.
3. **ZST / LZ4 as SFX payloads.** Both have strong fixed magic that the crate already
   recognises elsewhere, but neither has ever been in the SFX signature table, and no
   record says why.
4. **7z split-set modification.** Reading a split set landed 2026-09-05; modification
   did not follow, and nothing gates it.
5. **SFX payload modification.** `Archive::modify` cannot open an SFX at all — the
   format resolves from offset 0 and a stub matches nothing — so it fails with a bare
   "Unknown archive format" before the capability gate is reached.

## Known documentation defects

Found while assembling this file; listed so they are fixed rather than rediscovered.

- **README says RAR creation "continues to work" on Windows.** The Windows build does
  not compile in its default feature set, so RAR creation is reachable on no platform.

**Six of the seven entries this list carried were re-checked on 2026-09-08 and are
resolved; they are struck rather than left to be rediscovered as still-open.** README's
matrix now reads `△` for ZIP/7z Modify, not `✅`. All three of README, `Limitations.md`
and `docs/USER_MANUAL.md` now document 7z numeric splits as supported for reading. The
"encrypted ZIP creation is deliberately rejected" wording is gone — README now says
"paused, not refused on principle (MADR-0027)". `UnrarArchive::extract_to_stream`'s
rustdoc now names the real obstacle (adapting the SDK's `UCM_PROCESSDATA` push callback
to a pull `Read`, DEF-004). `sevenz_wrapper::parse_entry`'s comment now says the blame
on `sevenz-rust2` "was wrong: the pinned" version publishes both timestamps. And
`Limitations.md` §2 no longer carries the "ISO are read-only" sentence.
