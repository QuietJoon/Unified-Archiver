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
| Temp dir (extract-to-memory, UnRAR) | `extract_to_memory` creates temp dir | OS temp dir | Single file extracted, then read into memory | RAII via `TempDirGuard` / `Drop` | `ffi/wrapper.rs` |
| Temp dir (extract-to-memory, libarchive) | `extract_to_memory` creates temp dir | OS temp dir | Single file extracted, then read into memory | RAII via cleanup in `Drop` | `ffi/libarchive_wrapper.rs` |
| Temp archive (modification) | `commit_changes` creates new archive | Same directory as source | Modified archive via copy-on-write rewrite | Atomic rename replaces original; temp removed on success | `modification.rs` |
| Backup archive (modification) | Optional backup before commit | Same directory as source | Copy of original before modification | Caller manages (if backup requested) | `modification.rs` |
| Created archive | `Archive::create` + `finish()` | Caller-specified path | New archive file | Caller manages | `creation.rs` |
| SFX stub bytes | `extract_stub` extracts prefix | Caller-specified path | Binary stub bytes written to file | Caller manages | `sfx/detection.rs` |
| Mmap region (Piz) | `Archive::open` for ZIP | Memory-mapped from input file | (no file output — memory-only) | Unmapped on `Archive` drop | `ffi/piz_wrapper.rs` |

### Detailed File Flows

#### Extract-to-Memory (UnRAR/libarchive backends)

```
1. Create temp directory with unique name
2. Extract single file to temp directory using backend
3. Read extracted file into Vec<u8>
4. Return Vec<u8> to caller
5. Drop: TempDirGuard removes temp directory recursively
```

**Note:** Piz and SevenZ backends extract directly to memory (no temp files needed).

#### Archive Modification (Copy-on-Write)

```
1. Open source archive for reading (libarchive)
2. Create temp archive file in same directory (e.g., "archive.zip.tmp")
3. Copy non-removed entries from source to temp
4. Add new/replaced entries from ModificationTracker
5. Close both archives
6. Atomic rename: temp -> original
   - Unix: single rename() syscall
   - Windows: retry loop for locked files
7. If rename fails: temp file cleaned up, error returned
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
| UnRAR extract-to-memory | `unrar_mem_{pid}` | Low (PID-based) |
| Libarchive extract-to-memory | `la_mem_{timestamp_nanos}` | Very low (nanosecond-based; acknowledged race condition in IG-012-003) |
| Modification temp | `{original_name}.tmp` | Low (same directory, unique extension) |

### Cleanup Guarantees

- All temp directories use RAII patterns (`Drop` impl or `TempDirGuard`)
- Cleanup occurs even on error paths and panics
- `TempDirGuard` in `ffi/common.rs` provides shared cleanup logic
- Mmap regions unmapped automatically when `PizArchive` drops
