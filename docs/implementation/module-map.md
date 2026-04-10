# Module Map

Ownership and placement of key structural pieces.

## Library Crate: unified-archive

| Binary / Process | Module | Responsibility | Owns Entities / DTOs / Repos / Caches / Config |
|---|---|---|---|
| library | `src/lib.rs` | Public crate surface and re-exports | Re-exports only |
| library | `src/archive.rs` | Core Archive type, format detection, backend routing, mode lifecycle | `Archive`, `ArchiveBackend`, `ArchiveMode`, `entry_cache: OnceCell<Vec<ArchiveEntry>>` |
| library | `src/entry.rs` | Entry metadata types | `ArchiveEntry`, `EntryType`, `FileAttributes` |
| library | `src/error.rs` | Unified error types | `ArchiveError` (6 variants) |
| library | `src/format.rs` | Format enumeration and detection | `ArchiveFormat` (12 variants), magic byte tables, capability matrix |
| library | `src/options.rs` | Operational policy objects | `ExtractionOptions`, `CompressionOptions`, `CompressionLevel`, `ProgressCallback` trait, `RateLimiter` |
| library | `src/security.rs` | Path sanitization, resource limits | `ExtractionLimits`, `sanitize_entry_path()`, zip bomb guards |
| library | `src/inspection.rs` | Listing, lookup, integrity, multi-part, symlink warnings | (no owned types — operates on Archive) |
| library | `src/extraction.rs` | Extraction orchestration, parallel execution, progress | (no owned types — orchestrates backends) |
| library | `src/creation.rs` | Archive creation flow, write-mode operations | `ArchiveCreator` (internal) |
| library | `src/modification.rs` | Modify-mode API, copy-on-write commit | `ModificationTracker`, `ModificationOptions` |
| library | `src/streaming.rs` | Streaming extraction abstraction | `StreamingExtractor` |
| library | `src/stream_crc.rs` | Compression-stream checksum parsing | `StreamChecksum` |
| library | `src/ffi/common.rs` | Shared backend utilities | `TempDirGuard`, path normalization helpers, CRC helpers |
| library | `src/ffi/wrapper.rs` | Safe UnRAR adapter | `UnrarArchive` |
| library | `src/ffi/unrar.rs` | Raw UnRAR C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/libarchive_wrapper.rs` | Safe libarchive adapter | `LibarchiveArchive` |
| library | `src/ffi/libarchive.rs` | Raw libarchive C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/piz_wrapper.rs` | Native Rust ZIP read/extract (mmap) | `PizArchive` |
| library | `src/ffi/sevenz_wrapper.rs` | Native Rust 7z read/extract | `SevenZArchive` |
| library | `src/ffi/zip_wrapper.rs` | Native Rust ZIP read/extract (encrypted) | `ZipArchive` (ZipReader backend) |
| library | `src/ffi/zip_writer.rs` | Native Rust ZIP creation | `ZipWriter` |
| library | `src/sfx/detection.rs` | SFX detection pipeline | `SfxDetectionResult` |
| library | `src/sfx/signatures.rs` | Archive signature tables | Signature byte patterns |
| library | `src/sfx/stub_types.rs` | Executable stub type detection | `StubType` |
| library | `src/external/rar.rs` | Optional WinRAR CLI integration (feature-gated) | (no owned types — shells out to rar.exe) |
| library | `build.rs` | Native build/link orchestration | (build script — not runtime) |
