---
type: Intake
title: "MVP Intake: unified-archive"
description: "Retroactive intake record."
tags: [project-control, ADR-0019, ADR-0018, ADR-0040, ADR-0027, ADR-0021, ADR-0016, OI-0057-003]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# MVP Intake: unified-archive

> Retroactive intake record. Implementation is complete with tracked gaps. See [`open-issues.md`](./open-issues.md) for deferred items.

## Product Goal
- Goal: Unified, format-agnostic Rust library for archive operations (inspection, extraction, creation, modification, SFX detection) across ZIP, 7z, RAR, RAR5, TAR family (`.tar`, `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`, `.tar.lz4`, `.tar.lzma`), standalone GZIP/BZIP2/XZ/ZST/LZ4/LZMA (read-only, MADR-0019), and ISO — inspired by 7zip-JBinding's design philosophy. Standalone compressed-file *creation* is out of scope per AD 0018; the TAR.ZST/TAR.LZ4/TAR.LZMA variants are read *and* write as of 0.3.1, which wired the libarchive `zstd` / `lz4` / `lzma` write filters and their compression levels.
- Local run target: `cargo test` (full test suite across unit, contract, and integration suites)
- Definition of "working": All five user stories implemented with tracked gaps (`open_at_offset` ships tempfile-backed per AD 0040 with a 16 GiB payload ceiling; native streaming buffers entries). Creation progress is now wired per-entry (OI-025-003 resolved). Library compiles on macOS (primary), with Windows/Linux as secondary targets. Bounded memory streaming (target: <100MB for 10GB+ archives) applies only to libarchive-backed extraction paths; native backends (ZIP, SevenZ, UnRAR) buffer entries in memory.

## Actors and Roles
- Actor: Rust developer
- What they do: Integrates archive operations into their application using a single, format-agnostic API. Inspects, extracts, creates, and modifies archives without writing format-specific code.

## Mandatory MVP Scenarios

| ID | Scenario | Trigger/Input | Expected Output | Critical? |
|---|---|---|---|---|
| SCN-INS-01 | Inspect archive contents (unified API) | Archive file (ZIP, 7z, RAR, RAR5, TAR/.tar.gz/.tar.bz2/.tar.xz, ISO — see support matrix in specs/) | List of entries with path, size, compressed size, dates, CRC32 (format-dependent — available for ZIP, 7z, RAR; not available for TAR/ISO) | Yes |
| SCN-INS-02 | Inspect password-protected archive metadata | Encrypted archive without password | Filenames and metadata accessible; content protected | Yes |
| SCN-INS-03 | Validate archive integrity via CRC32 | Archive file | Corruption detected and reported per-file | Yes |
| SCN-INS-04 | Inspect multi-part archive (RAR/RAR5 multipart) | First part of split RAR/RAR5 archive | Complete file list from all parts (other multipart formats not yet supported) | No |
| SCN-INS-05 | Filter large archive listings | Archive with 1000+ files + filter predicate | Filtered entries with metadata preserved | No |
| SCN-EXT-01 | Extract all files (unified API) | Archive + destination path | All files extracted with correct structure/content | Yes |
| SCN-EXT-02 | Extract password-protected archive | Encrypted archive + correct password | Decrypted files extracted | Yes |
| SCN-EXT-03 | Extract multi-layer compressed archive | TAR.GZ / TAR.BZ2 / TAR.XZ | Transparent multi-layer decompression | Yes |
| SCN-EXT-04 | Handle corrupted archive extraction | Invalid/corrupted archive | Clear error with format-consistent messaging | Yes |
| SCN-EXT-05 | Monitor extraction progress | Large archive + callback | Rate-limited progress updates (frequency varies by backend) | No |
| SCN-CRE-01 | Create archive in multiple formats | Files + format selection | Valid archive readable by standard tools | Yes |
| SCN-CRE-02 | Create with max compression | Files + Maximum compression level | Optimal compression ratio per format | No |
| SCN-CRE-03 | Create password-protected archive (rejected) | Files + password | `OperationBlocked` for every format per MADR-0027; encrypted reads remain supported via `open_encrypted()` | Yes |
| SCN-CRE-04 | Create from large dataset | 10GB+ data + callback | Bounded memory usage, per-entry progress with `total=None` | Resolved (OI-025-003, AD 0021) |
| SCN-CRE-05 | Create with compression level control | Pre-compressed files + level adjustment | Adjustable compression overhead | No |
| SCN-MOD-01 | Add files to existing archive | Archive + new files | Archive contains original + new entries | Yes |
| SCN-MOD-02 | Remove entries from archive | Archive + entry names | Entries removed, archive size reduced | Yes |
| SCN-MOD-03 | Replace file in archive | Archive + updated file | Logical replacement via copy-on-write rewrite | No |
| SCN-MOD-04 | Modify large archive efficiently | Large archive + modification | Temp disk scales with archive size (copy-on-write rewrite) | No |
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | Windows .exe with embedded ZIP | is_sfx=true, format=ZIP, offset returned | Yes |
| SCN-SFX-02 | Detect WinRAR SFX | WinRAR .exe | is_sfx=true, format=RAR/RAR5, offset returned | Yes |
| SCN-SFX-03 | Detect 7-Zip SFX | 7-Zip .exe | is_sfx=true, format=7z, offset returned | Yes |
| SCN-SFX-04 | Detect Linux ELF SFX | ELF binary with embedded archive | is_sfx=true, correct format, offset returned | Yes |
| SCN-SFX-05 | Detect script-interpreter SFX (`ScriptInterpreter` variant per AD 0016) | Shell script + embedded ZIP | is_sfx=true, format detected, offset returned | Yes |
| SCN-SFX-06 | Reject standard archive (not SFX) | Regular archive file | is_sfx=false | Yes |
| SCN-SFX-07 | Reject non-archive executable | Regular binary | `SfxDetectionResult::not_sfx()` returns `is_sfx=false` (does not separately certify absence of embedded archives) | Yes |
| SCN-SFX-08 | Detect SFX with unknown stub | SFX with custom stub | `StubType::Unknown` proceeds to signature scanning (OI-027-001 resolved) | No |

## Explicit Non-Goals
- ISO format creation (read-only via libarchive)
- True streaming extraction (native backends — ZIP, SevenZ, UnRAR — buffer full entries before constructing StreamingExtractor; true streaming limited to libarchive-backed formats)
- Split archive creation (`split_size` field exists but is unused — IG-005-08)
- In-place `open_at_offset` (without tempfile staging) — the shipped `Archive::open_at_offset()` copies the payload tail to a tempfile (capped at 16 GiB per AD 0040) before backend dispatch; a backend-native offset-aware opening path is still a future item (IG-005-01).
- ~~SecStr migration for all password fields~~ — **Resolved 2026-04-17** (OI-0057-003): `ExtractionOptions.password` and `CompressionOptions.password` are `Option<SecStr>` with zeroize-on-drop.
- Backend trait abstraction (manual dispatch is explicit and performant — IG-014-001)
- Interactive password prompts (password must be provided before extraction)
- Windows/Linux production-grade support (macOS is primary target; wchar_t layout issue on Linux — IG-020-003)

## Persistent State
| Entity / State | Why it must persist | Source of truth |
|---|---|---|
| (none) | Library crate — no durable internal data store | N/A |

> The library has no durable internal data store. All API state is in-memory within `Archive` handles and dropped when handles go out of scope. Temporary files may be created during extraction and modification (see File / Blob Lifecycle below).

## File / Blob Lifecycle
| File type | Ingress | Canonical storage | Generated outputs | Cleanup / retention |
|---|---|---|---|---|
| Input archive | Caller provides path | Caller's filesystem | (read-only access) | Caller manages lifecycle |
| Extracted files | Archive contents | Destination dir (caller-specified) | Extracted files + directory structure | Caller manages lifecycle |
| Temp files (extraction) | extract_to_memory (backend-specific) | OS temp dir or /Volumes/Temp/claude/7zip/ | libarchive: streams directly to memory (no temp dir). SevenZ/ZIP: buffer full entries in memory. UnRAR: extracts via FFI callbacks, may use temp dir for multipart volumes. | RAII cleanup via `tempfile` types (`TempDir` / `TempPath` / `NamedTempFile`) on Drop |
| Temp files (modification) | commit_changes creates copy | Same directory as source archive | Modified archive via copy-on-write | Atomic rename replaces original; temp removed |
| Created archives | Creator API output | Caller-specified path | New archive file | Caller manages lifecycle |
| SFX stubs | extract_stub extracts prefix bytes | In-memory (returns Vec<u8>; caller writes to disk if needed) | Stub binary bytes | Caller manages lifecycle |

## Integrations
| Integration | Required for MVP? | Real or deferred? | Notes |
|---|---|---|---|
| UnRAR SDK (FFI) | Yes | Real | RAR/RAR5 read/extract. Statically linked. License: free of charge for handling RAR archives (commercial use included); the sources may not be used to build a RAR-compatible archiver or re-create the RAR compression algorithm, and the governing paragraph must be reproduced — see `LICENSE`. |
| libarchive (FFI) | Yes | Real | TAR family (.tar.gz, .tar.bz2, .tar.xz), ISO read, archive creation (7z/TAR — not ZIP), and direct read/extract of standalone `.gz` / `.bz2` / `.xz` streams per MADR-0019. Standalone stream *creation* remains out of scope per AD 0018. Linked via pkg-config. |
| goblin crate | Yes | Real | PE/ELF/Mach-O binary parsing for SFX detection. Pure Rust. |
| sevenz-rust2 crate | Yes | Real | Native Rust 7z inspection/extraction backend with CRC32 metadata access. |
| zip crate (ZipArchive/ZipWriter) | Yes | Real | Sole ZIP backend: `ZipArchive` reads, lists, and extracts all ZIP archives (encrypted and unencrypted) with CRC32 metadata; `ZipWriter` handles ZIP creation. |
| External WinRAR CLI | No | Deferred | RAR creation on Windows only. Behind `external-rar-create` feature flag. |

## System Shape
- Monolith / multi-binary / multi-process: **Single library crate** (no binaries, no processes)
- Communication style: Direct function calls (Rust API)
- Languages / frameworks: Rust 2024 edition (minimum 1.85), FFI to C/C++ (UnRAR SDK, libarchive)
- Contract format: Rust trait/struct signatures + prose contracts at `specs/001-unified-archive/contracts/`
- Verification preference: `cargo test` (integration tests, property-based tests via proptest). Criterion benchmarks exist in `benches/` for profiling but are not part of correctness verification.

## Critical Libraries / Systems / Scenarios
- Item: UnRAR SDK (FFI)
- Why critical: RAR is proprietary; no pure-Rust alternative exists. FFI boundary requires careful unsafe handling.
- Required negative path behavior: `ERAR_BAD_DATA` mapped to `ArchiveError::Corruption`; password failures mapped to `ArchiveError::Password`; missing volumes handled gracefully.

- Item: libarchive (FFI)
- Why critical: TAR/ISO backbone and creation backend for 7z/TAR (not ZIP/7z main read paths — those use the native ZIP and SevenZ backends). FFI boundary with C library.
- Required negative path behavior: `ARCHIVE_FAILED` mapped to appropriate `ArchiveError` variants; checksum errors detected and mapped to `ArchiveError::Corruption`.

- Item: Path traversal attacks during extraction
- Why critical: Security vulnerability if archive contains `../` paths.
- Required negative path behavior: `sanitize_entry_path` uses sanitize-then-validate: strips leading `/`, `../`, and `./` components first, then validates the resulting path is within the destination directory. Malicious paths are sanitized to safe equivalents rather than rejected outright.

- Item: SFX detection with malicious executables
- Why critical: Security scanning is a primary use case for SFX detection.
- Required negative path behavior: Detection scans only first 1MB. No execution of the SFX stub. `open_at_offset()` ships as a tempfile-backed implementation (AD 0040) with a 16 GiB payload ceiling — an invalid archive at the detected offset surfaces as a format/corruption error from the chosen backend.

## Estimation Preference
- Default: dependency-ordered slices with risk notes
- Slices (historical implementation order, not a normative dependency): Inspection -> Extraction -> Creation -> Modification -> SFX Detection
