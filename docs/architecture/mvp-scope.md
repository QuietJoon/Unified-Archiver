---
type: MVP Scope
title: "MVP Scope: unified-archive v0.1.0"
description: "List archive contents with metadata via unified API (ZIP, RAR/RAR5, 7z, TAR variants, ISO)"
tags: [architecture, ADR-0021, ADR-0040, ADR-0053, R0071-0020, R0070-0001, R0070-0002, OI-0057-003]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# MVP Scope: unified-archive v0.1.0

## In-Scope Scenarios

### User Story 1 — Archive Inspection (P1, MVP)
- List archive contents with metadata via unified API (ZIP, RAR/RAR5, 7z, TAR variants, ISO)
- Inspect password-protected archive metadata without password. Caveat: header-encrypted archives (e.g., RAR5 with header encryption) may require a password even for metadata listing; this is a format-level limitation, not a library gap.
- Validate archive integrity via CRC32
- Inspect multi-part archives (RAR multi-volume; ZIP/7z multi-part detection only)
- Filter large archive listings by path patterns or attributes

### User Story 2 — Archive Extraction (P2, MVP)
- Extract all files via unified API across all formats
- Extract password-protected archives (RAR, RAR5, ZIP, 7z)
- Extract multi-layer compressed archives (TAR.GZ, TAR.BZ2, TAR.XZ)
- Handle corrupted archives with clear errors
- Monitor extraction progress via callbacks

### User Story 3 — Archive Creation (P3)
- Create archives in multiple formats (ZIP, 7z, TAR variants)
- Create with configurable compression levels (Store through Ultra)
- Create password-protected archives (ZIP)
- Create from large datasets. Implementation note: bounded-memory streaming is backend-dependent — ZIP (zip crate) and TAR (libarchive) support streaming writes; 7z creation via libarchive may buffer internally. Bounded-memory behavior for ZIP/TAR creation has not been independently verified via benchmarks.
  - **Resolved:** creation progress callbacks invoked per-entry with `total=None` (OI-025-003, AD 0021)
- Create with compression level control for pre-compressed content

### User Story 4 — Archive Modification (P4)
- Add files to existing archives (ZIP, 7z)
- Remove entries from archives
- Replace files in archives
- Modify archives via copy-on-write full rewrite (minimal-temp optimization deferred to future release)

### User Story 5 — SFX Detection (P5)
- Detect Windows PE SFX (ZIP, RAR, 7z)
- Detect WinRAR SFX executables
- Detect 7-Zip SFX executables
- Detect Linux ELF SFX
- Detect ScriptInterpreter SFX (shebang-based self-extractors)
- Reject standard archives (not SFX) correctly
- Reject non-archive executables correctly
- Detect SFX with unknown/custom stubs via heuristic scan — resolved (OI-027-001)

## Out-of-Scope (Explicit Non-Goals)

| Item | Reason |
|---|---|
| ISO format creation | Read-only via libarchive; no write support in scope |
| In-place `open_at_offset` for every backend | **Mostly shipped.** ZIP landed 2026-08-21 (ticket `1ddc37ec`); RAR and 7z landed 2026-09-01 (ticket `5858e17b`). For all three, a payload behind an SFX stub is opened where it lies — no tempfile copy, and the AD 0040 ceiling does not apply. Each arrives differently: ZIP and RAR relocate themselves (the `zip` crate resolves prepended data from the end-of-central-directory record; UnRAR's own `IsArchive` scans for the first marker), while 7z is reached through `crate::payload_window::PayloadWindow`, a `Read + Seek` view that makes the payload offset look like byte 0. **libarchive (TAR family, ISO, raw streams) still stages**, and that is a decision rather than an omission: its formats carry no strong magic at the payload offset — plain tar's `ustar` sits at +257, gzip's is two bytes — so a gate cheap enough to run would admit garbage, and committing to an open that then failed format bidding would need a fall-back-after-failed-open path `try_open_in_place` deliberately does not have. Tracked as ticket `05e2dea4`, which carries the design; the constituency it costs is makeself `.run` installers, which are stub + tar.gz and routinely multi-GB. `Archive::payload_access()` reports which path a handle took. |
| ~~SecStr migration for all password fields~~ | **Resolved 2026-04-17** (OI-0057-003): `ExtractionOptions.password` and `CompressionOptions.password` are `Option<SecStr>` with zeroize-on-drop; decoded only at the FFI boundary. |
| Split archive creation | `split_size` field exists but ignored; multi-part creation remains deferred (originally targeted v0.2.0; the v0.2.0 release shipped without it — R0071-0020). |
| True streaming extraction | Native backends (ZipReader, SevenZ, UnRAR) buffer full entries; true streaming limited to libarchive. Current non-libarchive path: extract-to-memory + Cursor wrapper. ZipReader is the sole ZIP backend (DCR-009) and provides buffered decryption for encrypted ZIP. |
| ~~Backend trait abstraction~~ | **Landed in v0.2.0**: `src/backend.rs` provides `ReadBackend` plus mode-aware `dispatch_read_archive` / `dispatch_read_backend` helpers (R0070-0001 / R0070-0002, R0071-0020). Further `WriteBackend` / `ModifyBackend` traits remain deferred — see AD 0053. |
| Interactive password prompts | Password must be provided programmatically before extraction |
| Full cross-platform production support | See platform status below |
| GZIP/BZIP2/XZ standalone creation | Library creates these only as TAR compression layers |

### Platform Status

| Platform | Status | Notes |
|---|---|---|
| macOS (aarch64/x86_64) | Primary target | All backends tested |
| Linux (x86_64) | Partial | UnRAR `wchar_t` layout issue pending (IG-020-003) |
| Windows | Not yet tested on Windows | Build expected to work; no CI coverage |

## Deferred Integrations

| Integration | Reason for Deferral |
|---|---|
| External WinRAR CLI (`rar.exe`) | RAR creation. Windows-only, requires licensed WinRAR. Behind `external-rar-create` feature flag. |
| ZIP-native modify pipeline | Current modify-mode uses libarchive for ZIP reads; some scenarios remain unreliable. Planned: use zip crate for ZIP modification. |

## Allowed DEFERRED Placeholders

| Placeholder | Location | Justification |
|---|---|---|
| In-place `open_at_offset` for the staging backends | `src/archive.rs` (`Archive::try_open_in_place`) | DEF-001 closed 2026-04-18 with a tempfile-backed implementation (16 GiB payload ceiling per AD 0040). Ticket `1ddc37ec` (2026-08-21) removed the copy for ZIP; ticket `5858e17b` (2026-09-01) removed it for RAR and 7z. **Two things remain deferred, for unrelated reasons, and conflating them has caused confusion before.** (1) *libarchive* — ticket `05e2dea4`. Blocked on gate policy, not on FFI: the formats have no strong magic at the offset, so the gate would have to run the AD 0052 eager first-header probe and decline on failure, which is new policy and wants its own review. (2) *The SFX-shaped-path gate itself*, which every arm still carries. Its cause is elsewhere entirely: the password-aware extraction reopen in `src/extraction.rs` resolves an embedded payload only through the SFX route, so an in-place handle with a password would fail where staging succeeded. Lifting it needs an offset-aware encrypted reopen and is its own ticket, not part of the libarchive work. |
| `split_size` | `src/options.rs` (CompressionOptions) | Field reserved for future multi-part creation; not currently honored by writers |
