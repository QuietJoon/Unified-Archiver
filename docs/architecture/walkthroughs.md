# Walkthroughs

End-to-end architectural walkthroughs for mandatory scenarios.

## Happy-Path Walkthroughs

### 1. Archive Inspection (SCN-INS-01)

**Trigger:** `Archive::open("document.zip")` then `archive.list_files()`

1. `Archive::open` reads first 512 bytes for magic byte detection
2. `ArchiveFormat::detect` matches `PK\x03\x04` -> `ArchiveFormat::Zip`
3. Backend selection: ZIP + unencrypted -> `ArchiveBackend::Piz`
4. `PizArchive` stores path only (stateless reopen per operation)
5. `list_files()` checks `entry_cache` (OnceCell) — empty on first call
6. `PizArchive::list_files` mmaps the ZIP file, iterates entries, reads CRC32 from ZIP central directory
7. Returns `Vec<ArchiveEntry>` with normalized paths (forward slashes), sizes, timestamps, CRC32
8. Result cached in `entry_cache` for subsequent calls
9. Caller iterates entries — same struct fields regardless of format

**Ownership transitions:** Caller -> Archive (path) -> Piz (mmap) -> ArchiveEntry vec -> entry_cache (owned by Archive) -> caller (&[ArchiveEntry])

### 2. Archive Extraction (SCN-EXT-01)

**Trigger:** `archive.extract_all(options)` with `ExtractionOptions { destination, .. }`

1. `extraction::extract_all` invoked on `Archive`
2. **Destination prep:** `ensure_destination` creates destination dir if needed
3. **Password-aware reopen:** If archive is encrypted and password provided, reopen with correct backend
4. **Metadata pre-scan:** `list_files_for_limits` fetches entry list (cheaper path — skips CRC computation for libarchive)
5. **Safety checks:**
   - `ExtractionLimits` checked: max file size, max total size, max entry count
   - Overwrite policy: if `overwrite=false`, scan for existing destination files and fail early
6. **Backend dispatch:** `ArchiveBackend::extract_all_with_options` routes to correct backend
7. Backend iterates entries, sanitizes each path via `security::sanitize_entry_path`, writes to destination
8. Progress callbacks fire (rate-limited to ~60 Hz via `RateLimiter`)
9. CRC32 verification happens during extraction (automatic in UnRAR/libarchive)
10. Returns `Ok(())` on success

**Ownership transitions:** Caller -> extraction orchestrator (options owned) -> security layer (path validation) -> backend (file writes) -> filesystem

### 3. Archive Creation (SCN-CRE-01)

**Trigger:** `Archive::create("output.zip", options)` then `creator.add_file_from_path(...)` then `creator.finish()`

1. `Archive::create` sets mode to `ArchiveMode::Write`
2. Backend selection based on `CompressionOptions.format`:
   - ZIP -> `ZipWriter` (native Rust via `zip` crate)
   - 7z, TAR variants -> `LibarchiveArchive` write mode
3. `add_file_from_path`: reads file, adds to archive with format-appropriate compression
4. `add_file_from_data`: adds in-memory bytes as archive entry
5. `add_directory_recursive`: walks directory tree via `walkdir`, adds each file
6. `finish()`:
   - ZipWriter: writes central directory, flushes, closes file
   - Libarchive: calls `archive_write_close` + `archive_write_free`
7. Drop impl ensures cleanup even if `finish()` not called explicitly

### 4. Archive Modification (SCN-MOD-01)

**Trigger:** `Archive::modify("archive.zip")` then `modifier.add_entry(...)` then `modifier.commit_changes()`

1. `Archive::modify` opens archive in Read mode, creates `ModificationTracker`
2. `add_entry` / `remove_entry` / `replace_entry` queue operations in `ModificationTracker`
3. `commit_changes`:
   a. Creates temporary archive file in same directory as source
   b. Opens source archive for reading (libarchive)
   c. Creates new archive for writing (creation API)
   d. Copies non-removed entries from source to new archive
   e. Adds new/replaced entries from tracker
   f. Closes both archives
   g. Atomic rename: replaces original with new archive
4. If backup requested: copies original before rename

### 5. SFX Detection (SCN-SFX-01)

**Trigger:** `Archive::detect_sfx("installer.exe")`

1. **Stage 1 — Stub type detection:** Read file header, parse with `goblin` to identify PE/ELF/Mach-O/Script
2. **Stage 2 — Signature scan:** Scan first 1MB byte-by-byte for archive magic bytes:
   - `PK\x03\x04` (ZIP), `Rar!\x1a\x07\x00` (RAR), `Rar!\x1a\x07\x01\x00` (RAR5), `7z\xbc\xaf\x27\x1c` (7z)
3. **Stage 3 — Validation:** If signature found, attempt lightweight archive header parse at detected offset
4. Returns `SfxDetectionResult { is_sfx: true, archive_format: Some(Zip), data_offset: Some(offset), stub_type: PE }`
5. If no signature found within 1MB -> `SfxDetectionResult { is_sfx: false, .. }`

**Latency bound:** <100ms for files up to 10MB (1MB scan limit)

## Failure-Path Walkthroughs

### F1. Encrypted Archive Without Password (SCN-EXT-02, negative)

1. `Archive::open("secret.rar")` succeeds (metadata accessible without password for most RAR archives)
2. `archive.is_encrypted()` returns `Ok(true)` — checks entry encryption flags
3. `archive.extract_all(options)` without password:
   - UnRAR backend returns `ERAR_MISSING_PASSWORD`
   - Mapped to `ArchiveError::Password { message: "Password required for encrypted archive" }`
4. Caller receives clear, actionable error

### F2. Corrupted Archive (SCN-EXT-04)

1. `Archive::open("corrupted.zip")` may succeed if header is intact
2. `archive.extract_all(options)`:
   - Backend encounters CRC mismatch during extraction
   - UnRAR: `ERAR_BAD_DATA` -> `ArchiveError::Corruption { path: "file.txt", details: "CRC32 mismatch" }`
   - Libarchive: `ARCHIVE_FAILED` with checksum message -> `ArchiveError::Corruption`
3. Extraction stops; partial files may remain at destination

### F3. Path Traversal Attack (Security)

1. Archive contains entry with path `../../etc/passwd`
2. During extraction, `security::sanitize_entry_path` called:
   a. Joins destination + entry path
   b. Creates parent directories (required for `canonicalize`)
   c. `canonicalize` resolves to absolute path
   d. Checks if resolved path starts with destination prefix
   e. Path `../../etc/passwd` resolves outside destination -> fail
3. Returns `ArchiveError::InvalidPath { path: "../../etc/passwd", reason: "path traversal detected" }`
4. Extraction of this entry skipped; other entries continue
