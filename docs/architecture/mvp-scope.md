# MVP Scope: unified-archive v0.1.0

## In-Scope Scenarios

### User Story 1 — Archive Inspection (P1, MVP)
- List archive contents with metadata across all formats via unified API
- Inspect password-protected archive metadata without password
- Validate archive integrity via CRC32
- Inspect multi-part archives
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
- Create from large datasets with bounded memory and progress monitoring
- Create with compression level control for pre-compressed content

### User Story 4 — Archive Modification (P4)
- Add files to existing archives (ZIP, 7z)
- Remove entries from archives
- Replace files in archives
- Minimize temporary disk usage during modification

### User Story 5 — SFX Detection (P5)
- Detect Windows PE SFX (ZIP, RAR, 7z)
- Detect WinRAR SFX executables
- Detect 7-Zip SFX executables
- Detect Linux ELF SFX
- Detect shell script SFX
- Reject standard archives (not SFX) correctly
- Reject non-archive executables correctly
- Detect SFX with unknown/custom stubs via heuristic scan

## Out-of-Scope (Explicit Non-Goals)

| Item | Reason |
|---|---|
| ISO format creation | Read-only via libarchive; no write support in scope |
| `open_at_offset` for SFX | Requires significant changes to all backends; workaround exists (extract stub to temp archive) |
| SecStr migration for all password fields | secstr imported but not yet fully integrated; passwords remain in heap `String`s |
| Split archive creation | `split_size` field exists but ignored; multi-part creation deferred to v0.2.0 |
| True streaming extraction via RAR callback API | Current: extract-to-memory + Cursor wrapper; functional but not optimal |
| Backend trait abstraction | Manual dispatch is explicit and performant; refactoring deferred |
| Interactive password prompts | Password must be provided programmatically before extraction |
| Windows/Linux production support | macOS is primary target; wchar_t layout issue on Linux pending |
| GZIP/BZIP2/XZ standalone creation | Library creates these only as TAR compression layers |
| Empty directory preservation in created archives | Feature request; most use cases focus on file preservation |

## Deferred Integrations

| Integration | Reason for Deferral |
|---|---|
| External WinRAR CLI (`rar.exe`) | RAR creation. Windows-only, requires licensed WinRAR. Behind `external-rar-create` feature flag. |
| ZIP-native modify pipeline | Current modify-mode uses libarchive for ZIP reads; some scenarios remain unreliable. Planned: use zip crate for ZIP modification. |

## Allowed DEFERRED Placeholders

| Placeholder | Location | Justification |
|---|---|---|
| `open_at_offset` | `src/archive.rs` | SFX direct opening requires multi-backend offset support; workaround exists |
| `split_size` | `src/options.rs` (CompressionOptions) | Field reserved for future multi-part creation; not currently honored by writers |
