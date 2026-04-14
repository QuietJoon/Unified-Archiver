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
| `open_at_offset` for SFX | Deferred. Requires significant changes to all backends. Returns `ArchiveError::Unsupported`. Workaround: `extract_stub()` returns the executable prefix bytes (up to the detected archive offset) for security analysis; no temp-file payload extraction helper exists yet. Callers can use `data_offset` from `SfxDetectionResult` to locate the embedded archive. |
| SecStr migration for all password fields | secstr imported but not yet fully integrated; passwords remain in heap `String`s |
| Split archive creation | `split_size` field exists but ignored; multi-part creation deferred to v0.2.0 |
| True streaming extraction | Native backends (Piz, ZipReader, SevenZ, UnRAR) buffer full entries; true streaming limited to libarchive. Current non-libarchive path: extract-to-memory + Cursor wrapper. ZipReader provides buffered decryption for encrypted ZIP. |
| Backend trait abstraction | Manual dispatch is explicit and performant; refactoring deferred |
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
| `open_at_offset` | `src/archive.rs` | SFX direct opening requires multi-backend offset support; returns `ArchiveError::Unsupported`. Workaround: `extract_stub()` returns the executable prefix bytes for security analysis; callers can use `data_offset` from `SfxDetectionResult` to locate the embedded archive, but no helper to open at offset exists yet. |
| `split_size` | `src/options.rs` (CompressionOptions) | Field reserved for future multi-part creation; not currently honored by writers |
