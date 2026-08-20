---
type: Persistence and Files
title: "Persistence and Files"
description: "State persistence and file lifecycle design for unified-archive."
tags: [architecture, persistence, ADR-0020]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# Persistence and Files

State persistence and file lifecycle design for unified-archive.

## Persistent State

This library has **no persistent state**. It is a stateless library crate with no database, no config files, and no durable storage of its own. All state is in-memory within `Archive` handles and dropped when handles go out of scope (RAII).

| Entity / State | Why it must persist | Owner | Accessed by |
|---|---|---|---|
| (none) | N/A | N/A | N/A |

## File Lifecycle

### File Types

| File Type | Ingress | Canonical Storage | Generated Outputs | Cleanup / Retention | Owner Module |
|---|---|---|---|---|---|
| Input archive | Caller provides path | Caller's filesystem | (read-only access) | Caller manages | `archive.rs` |
| Extracted files | Archive contents | Destination dir (caller-specified) | Files + directory structure | Caller manages | `extraction.rs` |
| Temp dir (extract-to-memory, UnRAR) | `extract_to_memory` creates temp dir | OS temp dir | Single file extracted, then read into memory | RAII via `tempfile::TempDir`'s `Drop` (whole tree removed) | `src/ffi/wrapper.rs` |
| Memory buffer (extract-to-memory, libarchive) | `extract_to_memory` reads directly into memory buffer (no temp dir) | In-memory | Archive data read into `Vec<u8>` | Dropped with owning scope | `src/ffi/libarchive_wrapper.rs` |
| Temp archive (modification) | `commit_changes` creates new archive | Same directory as source | Modified archive via copy-on-write rewrite | Atomic rename replaces original; temp removed on success | `modification.rs` |
| Backup archive (modification) | Optional backup before commit | Same directory as source | Copy of original before modification | Caller manages (if backup requested) | `modification.rs` | `commit_changes()` honors `create_backup` + `backup_suffix` via `modify_with_options()` (OI-025-003 resolved, AD 0020). `preserve_metadata` preserves modification time and Unix permissions via metadata-aware add helpers (OI-025-002 resolved 2026-04-14). |
| Created archive | `Archive::create` + `finish()` | Caller-specified path | New archive file | Caller manages | `creation.rs` |
| SFX stub bytes | `extract_stub` extracts prefix | In-memory | Returns `Vec<u8>` (does not write to file) | Dropped with owning scope | `archive.rs` |

### Detailed File Flows

#### Extract-to-Memory (UnRAR backend)

```
1. Create temp directory with unique name
2. Extract single file to temp directory using backend
3. Read extracted file into Vec<u8>
4. Return Vec<u8> to caller
5. Drop: `tempfile::TempDir` removes the temp directory recursively
```

**Note:** Libarchive reads directly into a memory buffer (no temp dir). The ZIP and SevenZ backends also extract directly to memory (no temp files needed).

#### Archive Modification (Copy-on-Write)

Modification is a public `Archive` workflow (not libarchive-specific). `Archive::modify()` opens the archive in Modify mode, and `commit_changes()` performs a lossy copy-on-write rewrite: libarchive handles the source read side, while the write side is the format-specific creation backend — `ZipWriter` for ZIP (with a side-car read of the source ZIP central directory for archive comment + per-entry compression method) and libarchive for the rest.

```
1. Archive::modify() takes an fs4 advisory exclusive lock on the archive path
   *before* format detection, and records the locked inode's identity
2. Format detection, capability check, and the encrypted-source refusal all run
   inside that lock window (identity revalidated before the encryption probe
   reopens the pathname)
3. Track changes via add_entry / remove_entry / replace_entry (ModificationTracker)
4. commit_changes() creates temp archive file in same directory (e.g., "archive.zip.tmp.<pid>.<nanos>.<counter>")
5. Copy non-removed entries from source to temp (libarchive read + format-specific creation backend write)
6. Add new/replaced entries from ModificationTracker
7. Close both archives
8. Revalidate that the path still names the locked inode, immediately before
   any backup copy or replace (`revalidate_locked_identity`)
9. Atomic rename: temp -> original
   - Unix: single rename() syscall
   - Windows: single `MoveFileExW` call with `MOVEFILE_REPLACE_EXISTING` (no retry loop — transient sharing violations propagate as `ArchiveError::Io`)
10. If rename fails: temp file cleaned up, error returned
```

#### Archive Creation

```
1. Create output file at caller-specified path
2. Add entries via add_file_from_path / add_file_from_data / add_directory_recursive
3. finish() writes final archive structures (central directory for ZIP, etc.)
4. Drop impl ensures writer cleanup even if finish() not called
```

### Temporary File Naming

| Backend | Naming Pattern | Collision Risk |
|---|---|---|
| UnRAR extract-to-memory | `unrar_mem_{pid}_{timestamp}` | Low (PID + timestamp) |
| Modification temp | `{original_name}.tmp.{pid}.{nanos}.{counter}` | Very low (PID + wall-clock nanos + process-wide atomic counter, same directory) |

### Cleanup Guarantees

- All temp artifacts use RAII, owned by concrete `tempfile` types rather than by a shared in-crate
  guard: `tempfile::TempDir` for UnRAR extract-to-memory staging, `tempfile::TempPath` for staged
  SFX payloads (held in `Archive::_backing_tempfile`, so it lives exactly as long as the handle) and
  for UnRAR single-entry writes, and `tempfile::NamedTempFile` behind `AtomicOutputFile` /
  `write_entry_atomically` for per-file extraction writes (renamed into place on commit, unlinked on
  drop)
- Cleanup occurs even on error paths and panics
- Modification is the one exception: it builds its own `{original_name}.tmp.{pid}.{nanos}.{counter}`
  sibling and removes it explicitly on failure rather than relying on a `Drop` guard
- The `TempDirGuard` this section named previously was removed — see
  `docs/records/AD-0059-r0069-wide-modular-design-closure.md` (R0001-0094). `src/ffi/common.rs` now
  owns `AtomicOutputFile`, `write_entry_atomically`, path normalization and CRC helpers
