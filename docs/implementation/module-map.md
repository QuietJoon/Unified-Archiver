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
| library | `src/modification.rs` | Modify-mode API, copy-on-write commit (feature `modify`) | `ModificationTracker`, `ModificationOptions` |
| library | `src/write_namespace.rs` | Archive-internal file/dir namespace conflict gate shared by both write paths (feature `create` or `modify`) | `NamespaceTracker`, `record_file`, `record_dir`, `normalize_dup_check_path`, `ancestor_paths` |
| library | `src/backend.rs` | `ReadBackend` dispatch trait and the `ArchiveBackend` read/write routing | `ReadBackend`, `dispatch_read_backend` |
| library | `src/ffi.rs` | Backend module declarations, each behind its format feature | `zip_wrapper`, `zip_writer`, `sevenz_wrapper`, `libarchive_wrapper`, `wrapper` |
| library | `src/password.rs` | Password handling for encrypted archives | `Password` |
| library | `src/volume_chain.rs` | Multi-volume 7z part chaining (feature `sevenzip`) | `VolumeChain` |
| library | `src/streaming.rs` | Streaming extraction abstraction | `StreamingExtractor` |
| library | `src/stream_crc.rs` | Stream-checksum module root | Holds the module doc and the re-exports only, no logic. **Every child is private**; each public item keeps its historic `stream_crc::…` path, so the R7 split (AD 0010 amendment 2026-08-21) added no public module path. Re-exports `StreamChecksum`, `CheckType` |
| library | `src/stream_crc/digest.rs` | Value layer: checksum types and bit-level search (private) | `StreamChecksum`, `CheckType`, `find_pattern_last`, `find_bzip2_eos_crc_bitwise` — the I/O-free seam, which is why the bzip2 bit scan lives here rather than beside its codec |
| library | `src/stream_crc/codec.rs` | Framing layer root and the shared truncation classifier (private) | `framing_read_error` — the single definition mapping `UnexpectedEof` on a fixed-size framing read to `Format`, used by all three codecs and by the probe |
| library | `src/stream_crc/codec/gzip.rs` | gzip header/trailer parsing (private) | `extract_gzip_stream_crc` |
| library | `src/stream_crc/codec/bzip2.rs` | bzip2 stream-CRC parsing (private) | `extract_bzip2_stream_crc` |
| library | `src/stream_crc/codec/xz.rs` | xz stream-header check parsing (private) | `extract_xz_stream_check` |
| library | `src/stream_crc/detect.rs` | Dispatch layer: the six-byte probe and the routing table (private) | `extract_stream_checksum` |
| library | `src/ffi/common.rs` | Shared backend utilities | `AtomicOutputFile` / `write_entry_atomically`, path normalization helpers, CRC helpers |
| library | `src/ffi/wrapper.rs` | Safe UnRAR adapter | `UnrarArchive` |
| library | `src/ffi/unrar.rs` | Raw UnRAR C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/libarchive_wrapper.rs` | Safe libarchive adapter | `LibarchiveArchive` |
| library | `src/ffi/libarchive.rs` | Raw libarchive C FFI bindings | Raw C struct declarations, extern functions |
| library | `src/ffi/sevenz_wrapper.rs` | Native Rust 7z read/extract | `SevenZArchive` |
| library | `src/ffi/zip_wrapper.rs` | Native Rust ZIP read/extract — the sole ZIP reader, encrypted and unencrypted (DCR-009 retired the second backend) | `ZipArchive` (ZipReader backend) |
| library | `src/archive/mode_split.rs` | D2 typed-handle split, re-exported as `crate::v2`; behind the `v2-api` feature, which is one of the twelve members of `default` (`default` currently equals `full`: rar-support, v2-api, sevenzip, zip-crypto, libarchive, sfx, read, integrity, create, modify, zip-read, zip-write — see the `[features]` table in Cargo.toml) | `ReadArchive`, `WriteArchive`, `ModifyArchive` |
| library | `src/payload_window.rs` | `Read + Seek` window over a byte range, for in-place SFX opens (DCR-015) | `PayloadWindow` |
| library | `src/fs_identity.rs` | Read-handle file-identity capture and revalidation (DCR-014) | `FileIdentity` |
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
