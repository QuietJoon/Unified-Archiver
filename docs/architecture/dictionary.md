# Dictionary

Project terminology for unified-archive. Started in Phase 0, updated throughout.

## Core Domain Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| Archive | Handle to an opened archive file providing inspection, extraction, creation, and modification operations via a format-agnostic API | A compressed file containing other files | `src/archive.rs` |
| ArchiveEntry | Metadata for a single file or directory within an archive, normalized across all formats (path, size, compressed size, timestamps, CRC32, encryption status) | A file inside a compressed archive | `src/entry.rs` |
| ArchiveFormat | Enumeration of 12 supported archive formats with capability queries (compression, encryption, multipart, modification) | The type of compression used | `src/format.rs` |
| ArchiveBackend | Internal enum dispatching operations to the correct native library (UnRAR, Piz, SevenZ, Libarchive, ZipReader) | N/A | `src/archive.rs` |
| ArchiveMode | Access mode of an archive handle: Read, Write, or Modify | N/A | `src/archive.rs` |
| EntryType | Classification of archive entries: File, Directory, Symlink, Other | N/A | `src/entry.rs` |
| SfxDetectionResult | Result of SFX scan: is_sfx flag, archive_format, data_offset, stub_type | N/A | `src/sfx/detection.rs` |
| StubType | Executable format of an SFX stub: WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown | N/A | `src/sfx/stub_types.rs` |
| ExtractionOptions | Policy object configuring extraction: destination, password, overwrite, limits, progress, CRC verification | N/A | `src/options.rs` |
| CompressionOptions | Policy object configuring archive creation: format, level, password, split_size, progress | N/A | `src/options.rs` |
| ExtractionLimits | Safety bounds for extraction: max file size, max total size, max entry count (zip bomb guard) | N/A | `src/security.rs` |
| ModificationOptions | Policy object for archive modification: backup creation, metadata preservation | N/A | `src/modification.rs` |
| ArchiveError | Unified error enum with variants: Io, Format, Corruption, Password, Unsupported, CodecUnavailable, UnsupportedOperation, InvalidPath | N/A | `src/error.rs` |
| CompressionLevel | Six-level abstraction mapped to format-specific settings: Store, Fastest, Fast, Normal, Maximum, Ultra | N/A | `src/options.rs` |
| ProgressCallback | Trait for progress reporting with ControlFlow-based cancellation, rate-limited to ~60 Hz | N/A | `src/options.rs` |
| RateLimiter | Time-based throttling gate that limits progress callback frequency | N/A | `src/options.rs` |
| StreamingExtractor | Read-trait wrapper for memory-efficient file extraction with progress tracking | N/A | `src/streaming.rs` |
| StreamChecksum | Parsed checksum from compression stream headers/trailers (GZIP CRC32, XZ CRC64) | N/A | `src/stream_crc.rs` |
| FileAttributes | Platform-specific file attributes (Windows flags, Unix xattr) | N/A | `src/entry.rs` |

## Infrastructure & Protocol Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| FFI | Foreign Function Interface — Rust-to-C/C++ boundary for UnRAR SDK and libarchive | Language interop mechanism | `src/ffi/` |
| UnRAR SDK | Proprietary C++ library for RAR/RAR5 format reading/extraction, statically linked | RARLAB's decompression library | `src/ffi/unrar.rs`, `src/ffi/wrapper.rs` |
| libarchive | BSD-licensed C library for multi-format archive read/write support | Multi-format archive library | `src/ffi/libarchive.rs`, `src/ffi/libarchive_wrapper.rs` |
| Piz | Native Rust crate for mmap-based parallel ZIP reading with CRC32 metadata access | ZIP reading library | `src/ffi/piz_wrapper.rs` |
| sevenz-rust2 | Native Rust crate for 7z format reading with CRC32 metadata (maintained fork) | 7z reading library | `src/ffi/sevenz_wrapper.rs` |
| zip crate | Native Rust crate for ZIP read/write with AES-crypto support for encrypted ZIPs | ZIP library | `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs` |
| goblin | Pure Rust binary parser for PE/ELF/Mach-O executables, used in SFX stub detection | Binary format parser | `src/sfx/stub_types.rs` |
| Magic bytes | Format-identifying byte sequences at the start of archive files (e.g., `PK\x03\x04` for ZIP, `7z\xbc\xaf\x27\x1c` for 7z) | File signature / magic number | `src/format.rs` |
| TempDirGuard | RAII wrapper ensuring temporary directories are cleaned up on drop | N/A | `src/ffi/common.rs` |

## Standard Technology Terms

| Term | Project Meaning | Common Meaning | Defined In |
|---|---|---|---|
| RAII | Resource Acquisition Is Initialization — used for `Drop`-based cleanup of archive handles, temp dirs, and writers | C++/Rust resource management pattern | Throughout |
| OnceCell / OnceLock | Lazy one-time initialization cells used for entry metadata caching and format detection | Thread-safe lazy initialization | `src/archive.rs` |
| mmap | Memory-mapped file I/O used by Piz backend for fast ZIP access | Virtual memory file mapping | `src/ffi/piz_wrapper.rs` |
| CRC32 | 32-bit cyclic redundancy check used for archive integrity validation, SIMD-accelerated via `crc32fast` | Error detection code | `src/security.rs`, `src/stream_crc.rs` |
| Copy-on-write | Modification strategy: write new archive with changes, then atomically replace original | Delayed copy optimization | `src/modification.rs` |
| Atomic rename | Platform-aware file replacement (`rename` on Unix, `rename` + retry on Windows) for safe archive modification commits | File system operation | `src/modification.rs` |
