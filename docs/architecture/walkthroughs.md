---
type: Walkthroughs
title: "Walkthroughs"
description: "End-to-end architectural walkthroughs for mandatory scenarios."
tags: [architecture, scenarios, ADR-0015, ADR-0033, ADR-0060]
timestamp: 2026-08-09T00:00:00Z
status: active
---

# Walkthroughs

End-to-end architectural walkthroughs for mandatory scenarios.

## Happy-Path Walkthroughs

### 1. Archive Inspection (SCN-INS-01)

**Trigger:** `Archive::open("document.zip")` then `archive.list_files()`

1. `Archive::open` reads first 512 bytes for magic byte detection (ordinary format detection; SFX detection uses a separate two-stage read pattern — see SCN-SFX-01)
2. `ArchiveFormat::detect` matches `PK\x03\x04` -> `ArchiveFormat::Zip`
3. Backend selection: ZIP -> `ArchiveBackend::ZipReader` (the sole ZIP backend since DCR-009)
4. `ZipArchive` stores path only (stateless reopen per operation)
5. `list_files()` checks `entry_cache` (OnceCell) — empty on first call
6. `ZipArchive::list_files` opens the file with `std::fs::File`, iterates the central directory, reads each entry's stored CRC32, and memoises the listing
7. Returns `&[ArchiveEntry]` with normalized paths (forward slashes), sizes, timestamps, CRC32
8. Result cached in `entry_cache` for subsequent calls
9. Caller iterates entries — same struct fields regardless of format

**Ownership transitions:** Caller -> Archive (path) -> ZipReader (`File`) -> ArchiveEntry vec -> backend listing cell + entry_cache (both hold the same `Arc`, owned by Archive) -> caller (&[ArchiveEntry])

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
8. Progress callbacks fire (rate-limited to ~60 Hz via `RateLimiter` on libarchive and UnRAR; ZIP and SevenZ invoke the callback per entry or chunk without throttling)
9. CRC32 verification varies by backend: UnRAR uses native test mode; ZIP and 7z extract to memory and verify CRC32; libarchive streams entries and checks read errors
10. **Warning collection:** non-fatal outcomes accumulate in a `Vec<ArchiveWarning>` instead of aborting the run — the overwrite/conflict preflight contributes `OutputPathCaseCollision`, and every backend contributes `SkippedSymlink` / `SkippedHardLink` at each link entry it refuses to materialise (FR-022 — link skips are a deliberate safety gate, not a failure)
11. Returns `Result<ResultWithWarnings<()>>`, **not** `Ok(())` (R0001-0095). Success carries the accumulated warnings, so callers must inspect `result.warnings`; an empty vector is the only signal that nothing was skipped. The warning channel is the settled contract per `docs/records/MADR-0010-r052-extraction-warnings-result-with-warnings.md` and `docs/records/AD-0033-restore-result-with-warnings-dispatch.md`, which restored it after the signature had silently collapsed back to `Result<()>`. The same shape applies to `extract_some` / `extract_files` / `extract_by_ids`; single-entry `extract_file` has no warning channel because it rejects link and directory entries outright

**Ownership transitions:** Caller -> extraction orchestrator (options owned) -> security layer (path validation) -> backend (file writes) -> filesystem. Warnings travel the reverse path: backend skip sites -> extraction orchestrator (`Vec<ArchiveWarning>`, merged with the preflight's own) -> caller, wrapped in `ResultWithWarnings<()>`.

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

**Preconditions:** `Archive::modify` rejects encrypted archives (password-aware modification is not yet supported) and formats that do not support modification (e.g., RAR).

1. `Archive::modify` opens archive in Modify mode, creates `ModificationTracker`
2. `add_entry` / `remove_entry` / `replace_entry` queue operations in `ModificationTracker`
3. `commit_changes()` (takes `self`, no options parameter):
   a. Creates temporary archive file in same directory as source
   b. Opens source archive for reading (libarchive)
   c. Creates new archive for writing (creation API)
   d. Copies non-removed entries from source to new archive
   e. Adds new/replaced entries from tracker
   f. Closes both archives
   g. Atomic rename: replaces original with new archive; no backup is created

**Accepted caveats (OI-025-001, OI-025-002):** `commit_changes()` rebuilds the archive from scratch through the creation API, so format-specific metadata (e.g., archive comments) and non-default settings from the original archive may be lost in the resulting file. The rewrite is lossy by design; callers who need to preserve original metadata should keep their own copy.

### 5. SFX Detection (SCN-SFX-01)

**Trigger:** `Archive::detect_sfx("installer.exe")`

1. **Stage 1 — Stub type detection:** A single read pulls the first 1 MiB (plus a small probe tail) into one buffer; `StubType::detect` classifies the first 4 KiB of it. Classification is done **in-crate from header fields only** — a newline-terminated shebang (`#!`) line yields `ScriptInterpreter`, and prefix/header-field checks over the ELF, PE, and Mach-O identification structures yield `LinuxELF` / `WindowsPE` / `MacOSMachO`. There is **no binary-parsing dependency**: `goblin` was named here in error and is not in `Cargo.toml` at all (R0001-0080). Its whole-file parsers chase section/import tables that sit far past a 4 KiB window, so every real stub classified as `Unknown`; the header-field checks replaced them (R0079-0010). See `src/sfx/stub_types.rs` for the current field set.
   - Anything unrecognised is `StubType::Unknown`, and detection **stops here** with `is_sfx = false` — a real SFX must carry a recognised executable stub ahead of its payload (R0070-0076, `docs/records/AD-0060-r0070-broad-modular-triage-closure.md`). Stage 2 is never reached for unknown stubs; see SCN-SFX-08.
2. **Stage 2 — Signature scan (heuristic):** Scan the first 1MB, collecting every candidate offset for the active signature table (`src/sfx/signatures.rs`):
   - `PK\x03\x04` (ZIP), `Rar!\x1a\x07\x00` (RAR), `Rar!\x1a\x07\x01\x00` (RAR5), `7z\xbc\xaf\x27\x1c` (7z), `\x1f\x8b` (gzip), `BZh` (bzip2), `\xfd7zXZ\x00` (xz)
   - Correction (R0001-0080): AD 0015 removed the ZIP **central-directory** signature (`PK\x01\x02`) and TAR's fixed-offset `ustar` from this table. gzip, bzip2, and xz were never removed and are still scanned for.
   - This is a heuristic search with known limitations (e.g., signatures embedded in data sections may produce false positives; archives beyond the 1MB window are missed). It is not exhaustive validation.
3. **Stage 3 — Heuristic offset screening:** Each candidate offset gets a cheap per-format structural probe over the bytes at that offset — a recognised header shape plus a minimum-length gate scaled to the bytes actually remaining in the file. Each format enforces its own minimum instead of a blanket trailing-byte gate (R0070-0075). Candidates are screened in offset order, but a passing strong-magic candidate (a multi-byte signature with a structural probe) outranks the weak 2-byte gzip signature regardless of position, so an incidental gzip resource in the stub region cannot shadow the real payload behind it (R0079-0031). A passing candidate yields `probable()`: the detector returns `SfxConfidence::Probable` with an evidence list naming the matched stub and signature (I3 — there is no longer a confidence float; the "~0.9" figure some older docs quote no longer exists in the code). The `SfxConfidence` tri-state distinguishes `NotSfx`/`Probable`/`Confirmed`, but no production code path reaches `Confirmed` — that would require full backend validation, which runs later via `Archive::open_sfx()` and is not part of `detect_sfx()`'s contract.
4. Returns `SfxDetectionResult { is_sfx: true, archive_format: Some(Zip), data_offset: Some(offset), stub_type: WindowsPE, confidence: SfxConfidence::Probable, evidence: [..] }`
5. If no signature found within 1MB -> `SfxDetectionResult { is_sfx: false, .. }`

**Design-goal note:** The target latency is <100ms for files up to 10MB (1MB scan limit). No benchmark artifact currently validates this target; treat it as a design goal, not a contractual guarantee.

## Failure-Path Walkthroughs

> **Note:** The following scenarios illustrate representative backend error paths. Not every backend maps errors identically; the examples below show the most common mapping for each scenario.

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
   a. **Normalize entry path:** strip non-normal components (`.`, `..`, root prefixes) — `../../etc/passwd` becomes `etc/passwd`
   b. **Join** normalized path to destination — e.g., `/tmp/out/etc/passwd`
   c. **Create parent directories** if needed (required for canonicalization)
   d. **Canonicalize parent directory** to resolve symlinks and relative segments
   e. **Verify** canonicalized parent starts with canonicalized destination prefix
   f. **Check for symlink escape** on the final path itself
3. Because traversal components are stripped before joining, `../../etc/passwd` is safely extracted as `<destination>/etc/passwd`, not rejected. The path stays within the destination by construction.
4. An entry whose path consists *only* of traversal components (e.g., `../../..`) normalizes to an empty path and is rejected with `ArchiveError::InvalidPath { path: "../../..", reason: "Path contains only traversal components" }`. A symlink escape (destination itself contains a symlink that redirects outside) is similarly rejected.
5. Partial outputs from previously extracted entries may exist on disk
