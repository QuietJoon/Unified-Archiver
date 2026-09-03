# unified-archive

**Unified, format-agnostic Rust library for archive operations across ZIP, 7z, RAR, TAR, and more**

A cross-platform Rust library providing a unified interface for archive inspection, extraction, and creation. Write code once that automatically works for all supported archive formats.

## Features

✨ **Unified Interface** - Same API works identically for ZIP, 7z, RAR, RAR5, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST, TAR.LZ4, TAR.LZMA, GZIP, BZIP2, XZ, ZST, LZ4, LZMA, and ISO
🔍 **Automatic Format Detection** - Magic byte detection, no need to specify format explicitly
🚀 **High Performance** - SIMD-accelerated CRC32, streaming architecture, <100MB memory for 10GB+ archives (for libarchive-backed formats; ZIP, 7z, and RAR backends currently buffer entries during streaming extraction)
🔐 **Encrypted Read Support** - Open encrypted RAR, RAR5, ZIP, and 7z archives through one API
✅ **Integrity Validation** - Built-in CRC32 checksum validation and recovery record detection
🛡️ **SFX Detection and Opening** - Detect self-extracting archives and open embedded payloads via `open_sfx()` / `open_at_offset()`
📊 **Archive Metadata** - Detect solid compression, recovery records, and extract recovery percentages
🧵 **Thread-Safe** - Concurrent operations on different archives from multiple threads
🌍 **Cross-Platform** - macOS is the only platform with a recorded verification run (see `docs/verification/`); Linux and Windows code paths exist but have no recorded run yet
🦀 **Idiomatic Rust API** - One Rust-facing API over multiple native and Rust backends

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.4.0"
```

> **Not on crates.io.** No version of this crate has been published to a public registry, so
> the line above will not resolve as written. Until it is published, depend on the crate by
> `path` or `git`.

### Build Requirements

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install libarchive-dev pkg-config g++

# Fedora/RHEL
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++
```

**macOS:**
```bash
brew install libarchive pkg-config
```

> `pkg-config` and a C++ compiler are required: `build.rs` compiles the bundled
> UnRAR SDK sources through the `cc` crate, and libarchive is located with
> `pkg-config`.

**Windows:**
Not yet auto-configured. The blocker is libarchive discovery: `build.rs`'s
Windows branch emits only a build warning and no link directives, and libarchive
is not feature-gated, so it must be supplied by hand. Until vcpkg-based
discovery lands (tracked as OI-0065-001), either:

- install libarchive under `vcpkg` and set its lib directory via
  `RUSTFLAGS="-L native=<path>"`; or
- vendor libarchive yourself and provide a precompiled `.lib`.

Keep `rar-support` enabled: the bundled UnRAR sources compile on Windows through
the `cc` crate, so `--no-default-features` does not help with libarchive — it drops working RAR read support *and*
the `v2-api` typed handles (`unified_archive::v2::{ReadArchive, WriteArchive, ModifyArchive}`), default-on since
2026-09-03. The default feature set is `["rar-support", "v2-api"]`; `external-rar-create` is opt-in. RAR *creation* on Windows continues to work
through the optional `external::RarCreator` (WinRAR CLI).

## Documentation

- [User Manual](./docs/USER_MANUAL.md) - End-user guide for installation, workflows, and caveats
- [Getting Started](./docs/GETTING_STARTED.md) - First project and common tasks
- [API Reference](./docs/API_REFERENCE.md) - Public API surface and behavior notes
- [Limitations](./Limitations.md) - Current v0.4.0 caveats and unsupported cases
- [Changelog](./CHANGELOG.md) - Release history and release notes

## Usage

### List Archive Contents (Any Format!)

```rust
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    // Works for .zip, .7z, .rar, .tar.gz, etc.
    let archive = Archive::open("document.zip")?;

    for entry in archive.list_files()? {
        println!("{}: {} bytes", entry.path, entry.size.unwrap_or(0));
    }

    Ok(())
}
```

### Extract Archive (Unified API!)

```rust
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.7z")?;

let options = ExtractionOptions::new("output/").preserve_times(true);

// extract_all returns ResultWithWarnings<()>: skipped symlinks / hard-links
// surface here as ArchiveWarning entries — the extraction itself still succeeded.
let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

### Password-Protected Archives

```rust
use unified_archive::Archive;

// Check if password required
let archive = Archive::open("secret.rar")?;
if archive.is_encrypted()? {
    println!("Password required");
}

// Open with password
let archive = Archive::open_encrypted("secret.rar", "mypassword")?;
let entries = archive.list_files()?;
```

### Detect Self-Extracting Archives (SFX)

```rust
use unified_archive::Archive;

// Detect SFX archive
let result = Archive::detect_sfx("installer.exe")?;
if let Some((format, offset, stub)) = result.payload_coordinates() {
    println!("Found {:?} archive at offset {}", format, offset);
    println!("Stub type: {:?}", stub);
    println!("{}", result.summary());
}

// Open the embedded archive directly
let sfx_archive = Archive::open_sfx("installer.exe")?;
println!("Embedded entries: {}", sfx_archive.entry_count()?);
```

### Multi-part Archives

```rust
use unified_archive::{Archive, ExtractionOptions};

// RAR/RAR5 split archives are supported end-to-end in v0.4.0
let archive = Archive::open("backup.part1.rar")?;
let (is_multipart, parts) = archive.detect_multipart()?;
if is_multipart {
    println!("Multi-part archive with {} parts", parts.len());
}

let options = ExtractionOptions::new("./output");
let result = archive.extract_all(options)?; // Uses all RAR parts automatically
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

### Archive Metadata Inspection

```rust
use unified_archive::Archive;

let archive = Archive::open("data.rar")?;

// Check if archive uses solid compression
if archive.is_solid()? {
    println!("Archive uses solid compression (better ratio, slower random access)");
}

// Check for recovery records (RAR-specific feature)
if archive.has_recovery_record()? {
    println!("Archive has recovery records for repair");

    // Get actual recovery percentage
    if let Some(percentage) = archive.recovery_percentage()? {
        println!("Recovery percentage: {}%", percentage);
    }
}

// Check if password-protected
if archive.is_encrypted()? {
    println!("Archive requires password");
}
```

See the [User Manual](./docs/USER_MANUAL.md) and [Getting Started guide](./docs/GETTING_STARTED.md) for more examples.

## Supported Formats

| Format | Open / Inspect | Extract | Create via `Archive::create` | Modify | Encrypted Read | Notes |
|--------|----------------|---------|------------------------------|--------|----------------|-------|
| **RAR** | ✅ | ✅ | ❌ | ❌ | ✅ | RAR creation is not part of `Archive::create`; an optional Windows-only `external::RarCreator` exists behind `external-rar-create` |
| **RAR5** | ✅ | ✅ | ❌ | ❌ | ✅ | Same caveats as RAR |
| **ZIP** | ✅ | ✅ | ✅ | ✅ | ✅ | Encrypted ZIP creation is deliberately rejected in v0.4.0 |
| **7z** | ✅ | ✅ | ✅ | ✅ | ✅ | Encrypted 7z creation is not supported |
| **TAR** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.GZ** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.BZ2** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.XZ** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.ZST** | ✅ | ✅ | ✅ | ❌ | ❌ | Create needs a libarchive with the zstd write filter — see the codec note below |
| **TAR.LZ4** | ✅ | ✅ | ✅ | ❌ | ❌ | Create needs a libarchive with the lz4 write filter — see the codec note below |
| **TAR.LZMA** | ✅ | ✅ | ✅ | ❌ | ❌ | Create needs a libarchive with the lzma write filter — see the codec note below |
| **GZIP** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.gz` is read/extract only |
| **BZIP2** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.bz2` is read/extract only |
| **XZ** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.xz` is read/extract only |
| **ZST** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.zst` is read/extract only |
| **LZ4** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.lz4` is read/extract only |
| **LZMA** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.lzma` is read/extract only |
| **ISO** | ✅ | ✅ | ❌ | ❌ | ❌ | Read/extract only |

Additional notes:

- **Compressed-tar codecs:** creating `TAR.ZST` / `TAR.LZ4` / `TAR.LZMA` uses libarchive's zstd / lz4 / lzma **write** filters, which the linked libarchive must have been built with. When a filter is missing, creation fails immediately at writer construction with `ArchiveError::CodecUnavailable { codec, format, install_instructions }`, which names the missing codec and carries per-platform install instructions — the library never silently falls back to an external compressor binary. Reading those formats has no such requirement beyond the matching read filter.
- **Multi-part extraction:** RAR/RAR5 split sets are *detected and listed* end-to-end, but extraction across volumes is not implemented: a split file is listed once per volume with every copy sharing one path, so `extract_all` trips the duplicate-output-path guard, `extract_file` and `extract_to_memory` refuse to disambiguate, and `extract_by_ids` fails with a listing-drift error. Use `detect_multipart()` / `volume_set_report()` to inspect a set. ZIP split volumes (`.z01`, `.z02`, …) are name-level detection only. 7z numeric split volumes (`.001`, `.002`, …) are not supported.
- **SFX workflows:** `detect_sfx()`, `open_sfx()`, `open_at_offset()`, and `extract_stub()` are available for embedded archive inspection.
- **Streaming memory bounds:** Bounded-memory streaming currently applies to libarchive-backed formats (TAR family and ISO). ZIP, 7z, and RAR backends expose the same `Read` API but buffer entries first.

## Release Status

**Version 0.4.0 — current tagged version (git tag `v0.4.0`; never published to a public registry)**

Highlights at this tag:

- ✅ Unified inspection and extraction across all supported formats
- ✅ Archive creation through `Archive::create` for ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ,
  TAR.ZST, TAR.LZ4, and TAR.LZMA (the last three depend on the libarchive build carrying the
  matching write filter; see the codec-availability note above)
- ✅ Rewrite-based modification for ZIP and 7z through `Archive::modify` / `Archive::modify_with_options`
- ✅ SFX detection, stub extraction, and embedded archive opening
- ✅ Encrypted archive reading for RAR, RAR5, ZIP, and 7z
- ✅ Integrity validation, CRC32 exposure where available, and safety checks for extraction
- ⚠️ See [Limitations](./Limitations.md) for split-volume, encrypted-creation, streaming, and platform caveats

## Limitations

See [Limitations.md](./Limitations.md) for the full catalog. Key caveats in v0.4.0:

- **Modification is rewrite-based and limited to ZIP/7z.** `commit_changes()` recreates the archive instead of editing in place. `modify_with_options()` preserves modified, accessed, and created timestamps plus Unix permissions on retained regular-file entries (where the backend supports the timestamp); directory metadata still uses backend defaults. ZIP rewrites preserve the archive comment plus stored/deflated method. Encrypted archives are refused up front by `Archive::modify`, the rewrite drops symlink, hardlink, and other special entries, and ZIP64 edge coverage remains a caveat.
- **Streaming bounded-memory is libarchive-only.** `extract_to_stream()` reads TAR/ISO in chunks; ZIP, 7z, and RAR backends present the same `Read` API but buffer the entry in memory first.
- **Split archives: RAR/RAR5 only.** ZIP and 7z split volumes are not supported end-to-end in v0.4.0.
- **Encrypted creation is rejected by the main facade.** `Archive::create()` returns `OperationBlocked` when `CompressionOptions.password` is set. Optional Windows-only RAR creation lives in `external::RarCreator`, not the `Archive` facade.
- **Standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma` are read-only.** Use the creatable `.tar.*` compound variants for compressed-archive creation.
- **Encrypted-header archives** (RAR `-hp`, 7z `-mhe`) cannot be listed without the password — use `Archive::open_encrypted` up front. Encrypted ZIP entries still surface names without a password.
- **Platform coverage:** only macOS has a recorded verification run (`docs/verification/`, one record per `scripts/release-gate.sh` run, per AD-0070). Linux is unverified — its cross-check lane is parked pre-v2 — and Windows is unverified and additionally not auto-configured for libarchive (see Build Requirements).

## Architecture

**Backend Engines:**
- **zip crate** - All ZIP read and extract (encrypted and unencrypted) plus unencrypted ZIP creation; encrypted ZIP creation is rejected up front (MADR-0027)
- **SevenZ** - 7z read/extract
- **libarchive** - TAR-family formats (including TAR.ZST/TAR.LZ4/TAR.LZMA), standalone `.gz`/`.bz2`/`.xz`/`.zst`/`.lz4`/`.lzma`, ISO, and creatable non-ZIP formats (7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST, TAR.LZ4, TAR.LZMA)
- **UnRAR** - RAR/RAR5
- **In-crate SFX scanner** - Header-field stub classification (PE/ELF/Mach-O) plus payload signature scanning, in `src/sfx/`; no binary-parsing crate is linked
- **Optional `external::RarCreator`** - Windows-only WinRAR CLI bridge for out-of-process RAR creation

**Key Design Decisions:**
- Zero JVM dependency; core read/extract/create flows are in-process
- Streaming architecture for memory efficiency
- Automatic format detection via magic bytes
- Unified error handling across all formats

## License

Licensed under the [MIT License](./LICENSE).

**RAR Support:** the `rar-support` feature compiles the vendored UnRAR sources in
`src/ffi/native/unrar/`, which carry their own freeware licence
(`src/ffi/native/unrar/license.txt`). It permits use in any software that handles
RAR archives, free of charge and without a commercial-use restriction; what it
forbids is using those sources to develop a RAR (WinRAR) compatible archiver or
to re-create the RAR compression algorithm. It also requires the governing
paragraph to be reproduced verbatim — see the "Third-party: UnRAR" section of
[LICENSE](./LICENSE).

## Contributing

Contributions welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

## Acknowledgments

Inspired by [7zip-JBinding](https://github.com/borisbrodski/sevenzipjbinding) - bringing the unified interface approach to Rust.

## Further Reading

- [User Manual](./docs/USER_MANUAL.md)
- [Getting Started](./docs/GETTING_STARTED.md)
- [API Reference](./docs/API_REFERENCE.md)
- [SFX Coverage Report](./docs/sfx_coverage_report.md)
- [Quickstart Guide](./specs/001-unified-archive/quickstart.md)
- API documentation: `cargo doc --open`

## Performance

- **CRC32**: SIMD-accelerated with crc32fast
- **Memory**: <100MB for 10GB+ archives for libarchive-backed formats (TAR family). ZIP, 7z, and RAR backends currently buffer entries during streaming extraction.
- **SFX Detection**: <100ms for first 1MB scan
- **Thread Safety**: Concurrent operations on different files with zero contention
