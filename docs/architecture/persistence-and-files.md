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
| Temp dir (extract-to-memory, UnRAR) | `extract_to_memory` creates temp dir | OS temp dir | Single file extracted, then read into memory | RAII via `TempDirGuard` / `Drop` | `src/ffi/wrapper.rs` |
| Memory buffer (extract-to-memory, libarchive) | `extract_to_memory` reads directly into memory buffer (no temp dir) | In-memory | Archive data read into `Vec<u8>` | Dropped with owning scope | `src/ffi/libarchive_wrapper.rs` |
| Temp archive (modification) | `commit_changes` creates new archive | Same directory as source | Modified archive via copy-on-write rewrite | Atomic rename replaces original; temp removed on success | `modification.rs` |
| Backup archive (modification) | Optional backup before commit | Same directory as source | Copy of original before modification | Caller manages (if backup requested) | `modification.rs` | `commit_changes()` honors `create_backup` + `backup_suffix` via `modify_with_options()` (OI-025-003 resolved, AD 0020). `preserve_metadata` is accepted but no-op pending OI-025-002. |
| Created archive | `Archive::create` + `finish()` | Caller-specified path | New archive file | Caller manages | `creation.rs` |
| SFX stub bytes | `extract_stub` extracts prefix | In-memory | Returns `Vec<u8>` (does not write to file) | Dropped with owning scope | `archive.rs` |
| Mmap region (Piz) | Created lazily per ZIP operation (not at `Archive::open` time) | Memory-mapped from input file | (no file output — memory-only) | Unmapped on `Archive` drop | `src/ffi/piz_wrapper.rs` |

### Detailed File Flows

#### Extract-to-Memory (UnRAR backend)

```
1. Create temp directory with unique name
2. Extract single file to temp directory using backend
3. Read extracted file into Vec<u8>
4. Return Vec<u8> to caller
5. Drop: TempDirGuard removes temp directory recursively
```

**Note:** Libarchive reads directly into a memory buffer (no temp dir). Piz and SevenZ backends also extract directly to memory (no temp files needed).

#### Archive Modification (Copy-on-Write)

Modification is a public `Archive` workflow (not libarchive-specific). `Archive::modify()` opens the archive in Modify mode, and `commit_changes()` performs a lossy copy-on-write rewrite that currently uses libarchive for both read and write sides.

```
1. Open source archive via Archive::modify() (public API)
2. Track changes via add_entry / remove_entry / replace_entry (ModificationTracker)
3. commit_changes() creates temp archive file in same directory (e.g., "archive.zip.tmp.<pid>")
4. Copy non-removed entries from source to temp (libarchive read + write)
5. Add new/replaced entries from ModificationTracker
6. Close both archives
7. Atomic rename: temp -> original
   - Unix: single rename() syscall
   - Windows: retry loop for locked files
8. If rename fails: temp file cleaned up, error returned
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
| Modification temp | `{original_name}.tmp.{pid}` | Low (PID-based, same directory) |

### Cleanup Guarantees

- All temp directories use RAII patterns (`Drop` impl or `TempDirGuard`)
- Cleanup occurs even on error paths and panics
- `TempDirGuard` in `src/ffi/common.rs` provides shared cleanup logic
- Mmap regions unmapped automatically when `PizArchive` drops
