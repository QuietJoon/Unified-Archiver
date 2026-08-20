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
| In-place `open_at_offset` (no tempfile copy) | The shipped implementation (AD 0040) copies the payload tail to a tempfile (capped at 16 GiB) before backend dispatch. True in-place offset-aware backend opening — which would remove that copy — remains deferred. |
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
| In-place `open_at_offset` (no tempfile copy) | `src/archive.rs` | DEF-001 closed 2026-04-18: `Archive::open_at_offset()` ships a tempfile-backed implementation (16 GiB payload ceiling per AD 0040). A true in-place offset-aware backend opening that avoids the tempfile copy is still a future item. |
| `split_size` | `src/options.rs` (CompressionOptions) | Field reserved for future multi-part creation; not currently honored by writers |
