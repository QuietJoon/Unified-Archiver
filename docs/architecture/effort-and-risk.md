---
type: Effort and Risk
title: "Effort and Risk"
description: "Dependency-ordered implementation slices with risk notes."
tags: [architecture, ADR-0004, ADR-0005, ADR-0021, ADR-0027, ADR-0020, ADR-0040, OI-0065-002, OI-0057-003]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# Effort and Risk

Dependency-ordered implementation slices with risk notes. Retrospective — all slices landed with tracked gaps; risk notes reflect actual outcomes.

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
- UnRAR sequential iterator exhaustion required entry caching (OnceCell) — AD 0004
- CRC32 for libarchive-backed formats required on-the-fly computation (no header access)
- Multi-part RAR handled automatically by UnRAR; ZIP read via ZipReader, 7z read via SevenZ — multi-part detection present but extraction untested for ZIP/7z

### Slice 3: Extraction (Phase 4)
**Dependencies:** Slice 2
**Scope:** `extract_all`, `extract_file`, `extract_to_memory`, `extract_filtered`, `extract_to_stream`, parallel extraction, progress callbacks, password handling, CRC verification
**Status:** Complete
**Risk notes:**
- Per-entry reopen strategy for parallel extraction adds I/O overhead — AD 0005
- `extract_to_memory` uses temp files for UnRAR backend; libarchive reads directly into buffer — IG-011-004. ZipReader provides buffered decryption for encrypted ZIP entries.
- Non-libarchive streaming wraps buffer in Cursor (ZipReader, SevenZ, UnRAR); libarchive backends truly stream — IG-004-01
- Rate-limited progress callbacks prevent UI flooding at ~60 Hz

### Slice 4: Creation (Phase 5)
**Dependencies:** Slice 1
**Scope:** `Archive::create`, `add_file_from_path`, `add_file_from_data`, `add_directory_recursive`, `finish`, compression levels, ZIP password encryption
**Status:** Complete with tracked gaps (RAR creation via external CLI deferred)
**Risk notes:**
- libarchive write API integration required careful resource management (archive_write_close + free)
- Creation backends: libarchive for TAR variants and 7z; ZipWriter (Rust `zip` crate) for ZIP
- RAR creation via external WinRAR CLI deferred — Windows-only, requires licensed WinRAR, behind `external-rar-create` feature flag
- `split_size` option exists but is not honored by any writer — IG-005-08
- Creation progress callback is invoked per-entry by both ZIP and libarchive backends with `total=None` (OI-025-003 resolved, AD 0021)
- Creation encryption is blocked for every format per MADR-0027: `Archive::create` with `password: Some(_)` returns `ArchiveError::OperationBlocked`. Encrypted *reads* remain supported via `Archive::open_encrypted()`.

### Slice 5: Modification (Phase 6)
**Dependencies:** Slice 3 + Slice 4
**Scope:** `Archive::modify`, `add_entry`, `remove_entry`, `replace_entry`, `commit_changes`, copy-on-write rewrite, atomic rename
**Status:** Functional with lossy-rewrite caveats (`commit_changes()` is implemented)
**Risk notes:**
- `commit_changes()` performs a full copy-on-write rewrite: libarchive reads the source side while the write side is the format-specific creation backend (`ZipWriter` for ZIP; libarchive for the rest). The result is atomic-renamed over the original. This is a public `Archive` workflow, not a libarchive-internal detail.
- Copy-on-write rewrite requires full archive read + write — significant temp disk usage for large archives
- ZIP modification using libarchive on the read side is unreliable — some test scenarios ignore-gated
- Platform-specific atomic rename: Unix `rename` vs Windows retry logic
- Backup creation during modification: `create_backup` and `backup_suffix` are honored via `modify_with_options()` (AD 0020); `preserve_metadata` preserves modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries (OI-0065-002 resolved); `ModificationOptions::compression` overrides archive recreation settings (OI-025-001 resolved 2026-04-14)

### Slice 6: SFX Detection (Phase 7)
**Dependencies:** Slice 1
**Scope:** `detect_sfx`, `open_sfx`, `extract_stub`, stub type detection (PE/ELF/Mach-O/Script), signature scanning, 3-stage pipeline
**Status:** Shipped: detection complete; `Archive::open_at_offset()` is implemented via a tempfile slice of the payload with a 16 GiB ceiling (AD 0040). Remaining caveats: limited real-world corpus coverage.
**Risk notes:**
- 1MB scan limit is a design trade-off: covers known synthetic SFX stubs but misses custom stubs >1MB. Coverage validated against synthetic fixtures only (no real-world corpus); SFX tests are synthetic-suite-only evidence.
- `open_at_offset` currently copies the payload tail to a tempfile before handing off to the backend; true in-place offset-aware opening remains the architectural target — IG-005-01
- False positive rate controlled by validation stage after signature match; zero false positives observed in synthetic test suite only. Real-world false-positive rate unknown pending real-sample corpus.

## Known Technical Debt (from architecture investigation)

| Item | Impact | Suggested Resolution |
|---|---|---|
| Streaming APIs memory-backed for most backends | Memory diverges from true streaming expectations | Implement handle-backed streaming readers |
| UnRAR handle lifecycle causes reopen-heavy flows | Additional I/O overhead | Introduce internal handle manager/pool |
| `open_at_offset` copies the payload tail to a tempfile | Cost scales with payload size (capped at 16 GiB); no true in-place offset-aware opening | Add backend offset support to avoid the tempfile copy |
| Modify-mode comment/xattr propagation | Comments and extended attributes still not carried through modify-mode commits on non-ZIP backends | Extend `commit_changes()` metadata pass to comments/xattrs |
| ZIP modification partially reliable | Some scenarios ignore-gated | Use ZIP-native read/write pipeline |
| `ZipReader::list_files_for_limits` lacks optimization | Full CRC32 computed unnecessarily | Add metadata-only listing method |
| ~~Secure password handling not integrated~~ | ~~Passwords stored as `Option<String>` on heap; secstr crate imported but not wired into password fields~~ | **Resolved** (OI-0057-003, 2026-04-17): `ExtractionOptions.password` and `CompressionOptions.password` are `Option<SecStr>`; UnRAR/WinRAR wrappers intern passwords as `SecStr` and decode only at the FFI boundary. |
| `split_size` public but unused | API advertises non-functional feature | Implement or narrow API surface |
| Creation progress callbacks (OI-025-003 resolved) | Per-entry progress now invoked during creation with `total=None` (AD 0021) | Resolved |
| `ModificationOptions` wired (AD 0020) | `create_backup`/`backup_suffix` honored; `preserve_metadata` preserves modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries (OI-0065-002 resolved). Directory metadata still uses backend defaults; comments and extended attributes not yet preserved. | Extend to cover directory metadata, comments, xattrs, and non-file entry types |
