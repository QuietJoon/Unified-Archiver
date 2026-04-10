# Effort and Risk

Dependency-ordered implementation slices with risk notes. Retrospective — all slices implemented; risk notes reflect actual outcomes.

## Implementation Slices (Dependency Order)

### Slice 1: Foundation (Phase 1-2)
**Dependencies:** None
**Scope:** Format detection, error types, entry types, FFI bindings for UnRAR + libarchive, build.rs
**Status:** Complete
**Risk notes:**
- UnRAR static linking required platform-conditional `wchar_t` handling (macOS UTF-32 vs Windows UTF-16)
- libarchive linking via `pkg-config` required system-level library installation
- Linux `wchar_t` layout issue remains (IG-020-003) — macOS is primary target

### Slice 2: Inspection (Phase 3)
**Dependencies:** Slice 1
**Scope:** `list_files`, `find_entry`, `validate_integrity`, entry caching, multi-part detection, symlink warnings, solid/recovery detection
**Status:** Complete
**Risk notes:**
- UnRAR sequential iterator exhaustion required entry caching (OnceCell) — DD-004
- CRC32 for libarchive-backed formats required on-the-fly computation (no header access)
- Multi-part RAR handled automatically by UnRAR; ZIP/7z multi-part via libarchive untested

### Slice 3: Extraction (Phase 4)
**Dependencies:** Slice 2
**Scope:** `extract_all`, `extract_file`, `extract_to_memory`, `extract_filtered`, `extract_to_stream`, parallel extraction, progress callbacks, password handling, CRC verification
**Status:** Complete
**Risk notes:**
- Per-entry reopen strategy for parallel extraction adds I/O overhead — DD-005
- `extract_to_memory` uses temp files for UnRAR/libarchive backends (not true in-memory) — IG-011-004
- Streaming extraction wraps extract-to-memory in Cursor (not true streaming) — IG-004-01
- Rate-limited progress callbacks prevent UI flooding at ~60 Hz

### Slice 4: Creation (Phase 5)
**Dependencies:** Slice 1
**Scope:** `Archive::create`, `add_file_from_path`, `add_file_from_data`, `add_directory_recursive`, `finish`, compression levels, ZIP password encryption, RAR creation via external CLI
**Status:** Complete
**Risk notes:**
- libarchive write API integration required careful resource management (archive_write_close + free)
- ZIP writer uses native Rust `zip` crate — separate from libarchive for better control
- RAR creation requires external `rar.exe` — Windows-only, feature-gated, not pure Rust
- `split_size` option exists but is not honored by any writer — IG-005-08

### Slice 5: Modification (Phase 6)
**Dependencies:** Slice 3 + Slice 4
**Scope:** `Archive::modify`, `add_entry`, `remove_entry`, `replace_entry`, `commit_changes`, copy-on-write rewrite, atomic rename
**Status:** Partial (API complete, libarchive write integration pending)
**Risk notes:**
- Copy-on-write rewrite requires full archive read + write — significant temp disk usage for large archives
- ZIP modification using libarchive on the read side is unreliable — some test scenarios ignore-gated
- Platform-specific atomic rename: Unix `rename` vs Windows retry logic
- Modification options (backup, metadata preservation) not fully applied by commit flow

### Slice 6: SFX Detection (Phase 7)
**Dependencies:** Slice 1
**Scope:** `detect_sfx`, `open_sfx`, `extract_stub`, stub type detection (PE/ELF/Mach-O/Script), signature scanning, 3-stage pipeline
**Status:** Complete (detection); `open_at_offset` unimplemented
**Risk notes:**
- 1MB scan limit is a design trade-off: covers >99% of SFX stubs but misses custom stubs >1MB
- `open_at_offset` requires all backends to support offset-aware opening — architectural gap — IG-005-01
- False positive rate controlled by validation stage after signature match

## Known Technical Debt (from architecture investigation)

| Item | Impact | Suggested Resolution |
|---|---|---|
| Streaming APIs memory-backed for most backends | Memory diverges from true streaming expectations | Implement handle-backed streaming readers |
| UnRAR handle lifecycle causes reopen-heavy flows | Additional I/O overhead | Introduce internal handle manager/pool |
| `open_at_offset` unimplemented despite SFX offsets | SFX cannot fully open embedded archives in-place | Add backend offset support or robust stub-stripping |
| Modify-mode options partially applied | Backup/metadata preservation incomplete | Implement option-aware commit pipeline |
| ZIP modification partially reliable | Some scenarios ignore-gated | Use ZIP-native read/write pipeline |
| `ZipReader::list_files_for_limits` lacks optimization | Full CRC32 computed unnecessarily | Add metadata-only listing method |
| Secure password handling not implemented | Passwords in heap Strings | Replace with SecStr across all fields |
| `split_size` public but unused | API advertises non-functional feature | Implement or narrow API surface |
