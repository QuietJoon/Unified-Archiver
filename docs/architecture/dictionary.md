---
type: Dictionary
title: "Dictionary"
description: "Project terminology for unified-archive."
tags: [architecture, ADR-0019, ADR-0018, ADR-0020]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# Dictionary

Project terminology for unified-archive. Started in Phase 0, updated throughout.

## Core Domain Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| Archive | Handle to an opened archive file providing inspection, extraction, creation, and modification operations via a format-agnostic API | A compressed file containing other files | `src/archive.rs` |
| ArchiveEntry | Metadata for a single file or directory within an archive, normalized across all formats (path, size, compressed size, timestamps, CRC32, encryption status) | A file inside a compressed archive | `src/entry.rs` |
| ArchiveFormat | Enumeration of the supported archive and stream format variants, with capability queries (compression, encryption, multipart, modification). Standalone `Gzip`/`Bzip2`/`Xz`/`Zst`/`Lz4`/`Lzma` are independently openable via `Archive::open()` per MADR-0019; only *creation* of standalone raw streams is out of scope per AD 0018. | The type of compression used | `src/format.rs` |
| ArchiveBackend | Internal enum dispatching operations to the correct native library (UnRAR, SevenZ, Libarchive, ZipReader) | N/A | `src/archive.rs` |
| ArchiveMode | Access mode of an archive handle: Read, Write, or Modify | N/A | `src/archive.rs` |
| EntryType | Classification of archive entries: File, Directory, Symlink, HardLink, Other | N/A | `src/entry.rs` |
| SfxDetectionResult | Result of SFX scan: is_sfx flag, archive_format, data_offset, stub_type, confidence (`SfxConfidence` tri-state: `NotSfx`/`Probable`/`Confirmed`; production `detect_sfx()` emits only `NotSfx`/`Probable`), evidence (`Vec<String>`) | N/A | `src/sfx/result.rs` |
| StubType | Executable format of an SFX stub: WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown | N/A | `src/sfx/stub_types.rs` |
| ExtractionOptions | Policy object configuring extraction: destination, password, overwrite, limits, progress, CRC verification | N/A | `src/options.rs` |
| CompressionOptions | Policy object configuring archive creation: format, level, password, split_size, progress | N/A | `src/options.rs` |
| ExtractionLimits | Safety bounds for extraction: max_file_size, max_total_size, max_entry_count, max_compression_ratio (zip bomb guard), max_sfx_payload_size. Built through `ExtractionLimits::builder()`; each axis takes a typed `Cap` and the ratio a `CompressionRatio`. | N/A | `src/security.rs` |
| ModificationOptions | Policy object for archive modification: backup creation, metadata preservation, compression override. `create_backup`/`backup_suffix` honored via `modify_with_options()` (AD 0020); `preserve_metadata` preserves modification time and Unix permissions on retained entries; `compression: Option<CompressionOptions>` overrides format defaults for the recreated archive. | N/A | `src/modification.rs` |
| ArchiveError | Unified error enum with variants: Io, Format, Corruption, Password, Unsupported, CodecUnavailable, WriteModeOnly, ReadOnlyBackend, NotImplemented, OperationBlocked, InvalidPath. Note: `Unsupported` is returned for format-level inability; `WriteModeOnly`/`ReadOnlyBackend` cover mode-level restrictions; `NotImplemented` flags deferred features surfaced in the public API (no DEF entries currently return it after `open_at_offset` shipped); `OperationBlocked` covers runtime constraints (resource limits, path collisions, advisory-lock contention). | N/A | `src/error.rs` |
| CompressionLevel | Six-level abstraction mapped to format-specific settings: Store, Fastest, Fast, Normal, Maximum, Ultra | N/A | `src/options.rs` |
| ProgressCallback | Trait defining a single `on_progress` method with ControlFlow-based cancellation. The trait itself does not enforce throttling; ~60 Hz rate-limiting is an implementation-level behavior provided by `RateLimiter`. | N/A | `src/options.rs` |
| RateLimiter | Time-based throttling gate that limits progress callback frequency | N/A | `src/options.rs` |
| StreamingExtractor | Read-trait wrapper for file extraction with progress tracking. True streaming for libarchive-backed formats; SevenZ, UnRAR, and ZipReader buffer full entries. | N/A | `src/streaming.rs` |
| StreamChecksum | Parsed checksum from compression stream headers/trailers. Fields: `crc32` (GZIP), `crc64` (XZ), `uncompressed_size`, `check_type`. | N/A | `src/stream_crc.rs` |
| FileAttributes | Platform-specific file attributes (Windows flags, Unix xattr) | N/A | `src/entry.rs` |

## Infrastructure & Protocol Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| FFI | Foreign Function Interface — Rust-to-C/C++ boundary for UnRAR SDK and libarchive | Language interop mechanism | `src/ffi/` |
| UnRAR SDK | Proprietary C++ library for RAR/RAR5 format reading/extraction, statically linked | RARLAB's decompression library | `src/ffi/unrar.rs`, `src/ffi/wrapper.rs` |
| libarchive | BSD-licensed C library for multi-format archive read/write support | Multi-format archive library | `src/ffi/libarchive.rs`, `src/ffi/libarchive_wrapper.rs` |
| sevenz-rust2 | Native Rust crate for 7z format reading with CRC32 metadata (maintained fork) | 7z reading library | `src/ffi/sevenz_wrapper.rs` |
| zip crate | Native Rust crate for ZIP read/write with AES-crypto support for encrypted ZIPs | ZIP library | `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs` |
| Stub header-field classification | In-crate PE/ELF/Mach-O identification-header and shebang checks in `src/sfx/stub_types.rs`; no binary-parsing crate is linked | Binary format parser | `src/sfx/stub_types.rs` |
| Magic bytes | Format-identifying byte sequences at the start of archive files (e.g., `PK\x03\x04` for ZIP, `7z\xbc\xaf\x27\x1c` for 7z) | File signature / magic number | `src/format.rs` |
| AtomicOutputFile | RAII staging wrapper for a single extracted file: writes to a sibling `tempfile::NamedTempFile`, renames into place on `commit()`, unlinks on drop | N/A | `src/ffi/common.rs` |

## Standard Technology Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| RAII | Resource Acquisition Is Initialization — used for `Drop`-based cleanup of archive handles, temp dirs, and writers | C++/Rust resource management pattern | Throughout |
| OnceCell | Lazy one-time initialization cell (from `once_cell` or `std`) used for `Archive.entry_cache` | Lazy initialization | `src/archive.rs` |
| OnceLock | `std::sync::OnceLock` used for one-time initialization of the SFX signature table (process-global, thread-safe) | Thread-safe lazy initialization | `src/sfx/signatures.rs` |
| CRC32 | 32-bit cyclic redundancy check used for archive integrity validation, SIMD-accelerated via `crc32fast` | Error detection code | `src/security.rs`, `src/stream_crc.rs` |
| Copy-on-write | Modification strategy: write new archive with changes, then atomically replace original | Delayed copy optimization | `src/modification.rs` |
| Atomic rename | Platform-aware file replacement (`std::fs::rename` on Unix; a single `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` on Windows — no retry loop, so a transient sharing violation surfaces rather than being retried) for safe archive modification commits | File system operation | `src/modification.rs` |
