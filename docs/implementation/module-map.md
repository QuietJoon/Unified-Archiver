---
type: Module Map
title: "Module Map"
description: "Ownership and placement of key structural pieces."
tags: [implementation]
timestamp: 2026-05-03T00:00:00Z
status: active
---

# Module Map

Ownership and placement of key structural pieces.

## Library Crate: unified-archive

| Binary / Process | Module | Responsibility | Owns Entities / DTOs / Repos / Caches / Config |
|---|---|---|---|
| library | `src/lib.rs` | Public crate surface and re-exports | Re-exports only |
| library | `src/archive.rs` | Core Archive type, format detection, backend routing, mode lifecycle | `Archive`, `ArchiveBackend`, `ArchiveMode`, `entry_cache: OnceCell<Vec<ArchiveEntry>>`, `finish()` |
| library | `src/entry.rs` | Entry metadata types | `ArchiveEntry`, `EntryType`, `FileAttributes` |
| library | `src/error.rs` | Unified error types | `ArchiveError` (`#[non_exhaustive]`; current variants include Io, Format, Corruption, Password, Unsupported, CodecUnavailable, WriteModeOnly, ReadOnlyBackend, NotImplemented, OperationBlocked, InvalidPath, Cancelled) |
| library | `src/format.rs` | Format enumeration and detection | `ArchiveFormat` (`#[non_exhaustive]`; 18 variants today: Rar, Rar5, Zip, SevenZip, Tar, TarGzip, TarBzip2, TarXz, TarZst, TarLz4, TarLzma, Gzip, Bzip2, Xz, Zst, Lz4, Lzma, Iso), magic byte tables, capability matrix |
| library | `src/options.rs` | Operational policy objects | `ExtractionOptions`, `CompressionOptions`, `CompressionLevel`, `ProgressCallback` trait, `RateLimiter` |
| library | `src/security.rs` | Path sanitization, resource limits | `ExtractionLimits`, `ExtractionLimitsBuilder`, `Cap`, `CompressionRatio` (public); raw policy helpers (`sanitize_entry_path`, `sanitize_entry_path_with_base`, `validate_archive_internal_path`, `verify_crc32`, `check_extraction_safe`) are crate-internal |
| library | `src/inspection.rs` | Listing, lookup, integrity, multi-part, symlink warnings | `MultipartLayout`, `ValidationReport` |
| library | `src/extraction.rs` | Extraction orchestration, shared sequential selective extraction, progress | (no owned types — orchestrates backends) |
| library | `src/creation.rs` | Archive creation flow, write-mode operations | Archive write-mode API (`impl Archive` methods: create, add_file_from_data, add_file_from_path, add_file_from_path_as, add_directory, add_directory_recursive) |
| library | `src/modification.rs` | Modify-mode API, copy-on-write commit | `ModificationTracker`, `ModificationOptions` |
| library | `src/streaming.rs` | Streaming extraction abstraction | `StreamingExtractor` |
| library | `src/stream_crc.rs` | Compression-stream checksum parsing | `StreamChecksum` |
| library | `src/ffi/common.rs` | Shared backend utilities | `AtomicOutputFile` / `write_entry_atomically`, path normalization helpers, CRC helpers |
| library | `src/ffi/wrapper.rs` | Safe UnRAR adapter | `UnrarArchive` |
| library | `src/ffi/unrar.rs` | Raw UnRAR C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/libarchive_wrapper.rs` | Safe libarchive adapter | `LibarchiveArchive` |
| library | `src/ffi/libarchive.rs` | Raw libarchive C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/sevenz_wrapper.rs` | Native Rust 7z read/extract | `SevenZArchive` |
| library | `src/ffi/zip_wrapper.rs` | Native Rust ZIP read/extract (encrypted) | `ZipArchive` (ZipReader backend) |
| library | `src/ffi/zip_writer.rs` | Native Rust ZIP creation | `ZipWriter` |
| library | `src/sfx.rs` | SFX module root | Public submodules `detection`, `result`, `stub_types`; `limits` and `signatures` are `pub(crate)`. Re-exports four items, not submodules: `detect_sfx`, `SfxConfidence`, `SfxDetectionResult`, `StubType` |
| library | `src/sfx/detection.rs` | SFX detection pipeline | (no owned types — orchestrates detection) |
| library | `src/sfx/result.rs` | SFX detection result types | `SfxDetectionResult`, `SfxConfidence` |
| library | `src/sfx/signatures.rs` | Archive signature tables (`pub(crate)`) | `Signature`, `SIGNATURES`, `scan_for_signatures`, `find_first_signature` — none reachable from outside the crate |
| library | `src/sfx/limits.rs` | SFX detection and payload ceilings (`pub(crate)`) | `HEADER_SIZE`, `MAX_SCAN_SIZE`, `MAX_SFX_PAYLOAD_SIZE` |
| library | `src/sfx/stub_types.rs` | Executable stub type detection | `StubType` |
| library | `src/external.rs` | External tools module root | Re-exports submodules (`rar`) |
| library | `src/external/rar.rs` | Optional WinRAR CLI integration (feature-gated) | `RarCreator` |
| library | `src/test_utils.rs` | Shared test utilities (compile-time `#[cfg(test)]` only) | `fixture()` helper |
| library | `build.rs` | Native build/link orchestration | (build script — not runtime) |
