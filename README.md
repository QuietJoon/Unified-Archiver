# unified-archive

**Unified, format-agnostic Rust library for archive operations across ZIP, 7z, RAR, TAR, and more**

A cross-platform Rust library providing a unified interface for archive inspection, extraction, and creation. Write code once that automatically works for all supported archive formats.

## Features

✨ **Unified Interface** - Same API works identically for ZIP, 7z, RAR, RAR5, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZIP, BZIP2, XZ, and ISO
🔍 **Automatic Format Detection** - Magic byte detection, no need to specify format explicitly
🚀 **High Performance** - SIMD-accelerated CRC32, streaming architecture, <100MB memory for 10GB+ archives (for libarchive-backed formats; ZIP, 7z, and RAR backends currently buffer entries during streaming extraction)
🔐 **Encrypted Read Support** - Open encrypted RAR, RAR5, ZIP, and 7z archives through one API
✅ **Integrity Validation** - Built-in CRC32 checksum validation and recovery record detection
🛡️ **SFX Detection and Opening** - Detect self-extracting archives and open embedded payloads via `open_sfx()` / `open_at_offset()`
📊 **Archive Metadata** - Detect solid compression, recovery records, and extract recovery percentages
🧵 **Thread-Safe** - Concurrent operations on different archives from multiple threads
🌍 **Cross-Platform** - macOS and Linux tested; Windows support is present but not release-verified
🦀 **Idiomatic Rust API** - One Rust-facing API over multiple native and Rust backends

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.1.0"
```

### Build Requirements

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install libarchive-dev pkg-config make g++

# Fedora/RHEL
sudo dnf install libarchive-devel pkgconf-pkg-config make gcc-c++
```

**macOS:**
```bash
brew install libarchive pkg-config
```

> `pkg-config`, `make`, and a C++ compiler are required for building the bundled UnRAR SDK.

**Windows:**
Dependencies bundled automatically

## Documentation

- [User Manual](./docs/USER_MANUAL.md) - End-user guide for installation, workflows, and caveats
- [Getting Started](./docs/GETTING_STARTED.md) - First project and common tasks
- [API Reference](./docs/API_REFERENCE.md) - Public API surface and behavior notes
- [Limitations](./Limitations.md) - Current v0.1.0 caveats and unsupported cases
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
use std::path::PathBuf;

let archive = Archive::open("backup.7z")?;

let options = ExtractionOptions {
    destination: PathBuf::from("output/"),
    preserve_times: true,
    ..Default::default()
};

archive.extract_all(options)?;
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
if result.is_sfx {
    println!("Found {} archive at offset {}",
        result.archive_format.unwrap(),
        result.data_offset.unwrap());
    println!("Stub type: {:?}", result.stub_type);
    println!("{}", result.summary());
}

// Open the embedded archive directly
let sfx_archive = Archive::open_sfx("installer.exe")?;
println!("Embedded entries: {}", sfx_archive.entry_count()?);
```

### Multi-part Archives

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

// RAR/RAR5 split archives are supported end-to-end in v0.1.0
let archive = Archive::open("backup.part1.rar")?;
let (is_multipart, parts) = archive.detect_multipart()?;
if is_multipart {
    println!("Multi-part archive with {} parts", parts.len());
}

let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    ..Default::default()
};
archive.extract_all(options)?; // Uses all RAR parts automatically
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
| **ZIP** | ✅ | ✅ | ✅ | ✅ | ✅ | Encrypted ZIP creation is deliberately rejected in v0.1.0 |
| **7z** | ✅ | ✅ | ✅ | ✅ | ✅ | Encrypted 7z creation is not supported |
| **TAR** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.GZ** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.BZ2** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **TAR.XZ** | ✅ | ✅ | ✅ | ❌ | ❌ | |
| **GZIP** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.gz` is read/extract only |
| **BZIP2** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.bz2` is read/extract only |
| **XZ** | ✅ | ✅ | ❌ | ❌ | ❌ | Standalone `.xz` is read/extract only |
| **ISO** | ✅ | ✅ | ❌ | ❌ | ❌ | Read/extract only |

Additional notes:

- **Multi-part extraction:** RAR/RAR5 split archives are supported end-to-end. ZIP and 7z split volumes are not supported in v0.1.0.
- **SFX workflows:** `detect_sfx()`, `open_sfx()`, `open_at_offset()`, and `extract_stub()` are available for embedded archive inspection.
- **Streaming memory bounds:** Bounded-memory streaming currently applies to libarchive-backed formats (TAR family and ISO). ZIP, 7z, and RAR backends expose the same `Read` API but buffer entries first.

## Release Status

**Version 0.1.0 - Initial public release**

Current release highlights:

- ✅ Unified inspection and extraction across all supported formats
- ✅ Archive creation through `Archive::create` for ZIP, 7z, and TAR variants
- ✅ Rewrite-based modification for ZIP and 7z through `Archive::modify` / `Archive::modify_with_options`
- ✅ SFX detection, stub extraction, and embedded archive opening
- ✅ Encrypted archive reading for RAR, RAR5, ZIP, and 7z
- ✅ Integrity validation, CRC32 exposure where available, and safety checks for extraction
- ⚠️ See [Limitations](./Limitations.md) for split-volume, encrypted-creation, streaming, and platform caveats

## Limitations

See [Limitations.md](./Limitations.md) for the full catalog. Key caveats in v0.1.0:

- **Modification is rewrite-based and limited to ZIP/7z.** `commit_changes()` recreates the archive instead of editing in place. `modify_with_options()` can preserve timestamps and Unix permissions on retained entries, and ZIP rewrites preserve the archive comment plus stored/deflated method, but encrypted ZIP re-encryption and ZIP64 edge coverage remain caveats.
- **Streaming bounded-memory is libarchive-only.** `extract_to_stream()` reads TAR/ISO in chunks; ZIP, 7z, and RAR backends present the same `Read` API but buffer the entry in memory first.
- **Split archives: RAR/RAR5 only.** ZIP and 7z split volumes are not supported end-to-end in v0.1.0.
- **Encrypted creation is rejected by the main facade.** `Archive::create()` returns `OperationBlocked` when `CompressionOptions.password` is set. Optional Windows-only RAR creation lives in `external::RarCreator`, not the `Archive` facade.
- **Standalone `.gz` / `.bz2` / `.xz` are read-only.** Use the `.tar.*` compound variants for compressed-archive creation.
- **Encrypted-header RAR** (`-hp`) cannot be listed without the password — other formats surface entry names without one.
- **Platform coverage:** macOS and Linux are tested; Windows support exists but is not release-verified yet.

## Architecture

**Backend Engines:**
- **Piz** - ZIP read/extract
- **zip crate** - Encrypted ZIP read + ZIP creation
- **SevenZ** - 7z read/extract
- **libarchive** - TAR-family formats, standalone `.gz`/`.bz2`/`.xz`, ISO, and non-ZIP creation
- **UnRAR** - RAR/RAR5
- **goblin** - Binary parsing for SFX detection (PE/ELF/Mach-O)
- **Optional `external::RarCreator`** - Windows-only WinRAR CLI bridge for out-of-process RAR creation

**Key Design Decisions:**
- Zero JVM dependency; core read/extract/create flows are in-process
- Streaming architecture for memory efficiency
- Automatic format detection via magic bytes
- Unified error handling across all formats

## License

Licensed under the [MIT License](./LICENSE-MIT).

**RAR Support:** UnRAR license applies (free for non-commercial use). Commercial use requires license from RARLAB. Disable with `--no-default-features` if needed.

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
