# unified-archive

**Unified, format-agnostic Rust library for archive operations across ZIP, 7z, RAR, TAR, and more**

A cross-platform Rust library providing a unified interface for archive inspection, extraction, and creation. Write code once that automatically works for all supported archive formats.

## Features

✨ **Unified Interface** - Same API works identically for ZIP, 7z, RAR, RAR5, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, and ISO
🔍 **Automatic Format Detection** - Magic byte detection, no need to specify format explicitly
🚀 **High Performance** - SIMD-accelerated CRC32, streaming architecture, <100MB memory for 10GB+ archives (for libarchive-backed formats; ZIP, 7z, and RAR backends currently buffer entries during streaming extraction)
🔐 **Password Support** - Unified encryption handling for RAR, RAR5, ZIP, 7z
✅ **Integrity Validation** - Built-in CRC32 checksum validation and recovery record detection
🛡️ **SFX Detection** - Detect and analyze self-extracting archives (Windows PE, Linux ELF, macOS Mach-O, Script interpreters); extraction via offset opening is deferred
📊 **Archive Metadata** - Detect solid compression, recovery records, and extract recovery percentages
🧵 **Thread-Safe** - Concurrent operations on different archives from multiple threads
🌍 **Cross-Platform** - macOS and Linux support; Windows not yet tested
🦀 **Pure Rust API** - Zero JVM dependency, idiomatic Rust interface

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

    // Note: open_at_offset() is not yet implemented (returns Unsupported).
    // Use detect_sfx() to identify the archive format and offset,
    // then extract the embedded archive data manually.
}
```

### Multi-part Archives

```rust
use unified_archive::Archive;

// Automatically detects and handles multi-part archives
let archive = Archive::open("backup.part1.rar")?; // or .z01, .001, etc.
let (is_multipart, parts) = archive.detect_multipart()?;
if is_multipart {
    println!("Multi-part archive with {} parts", parts.len());
}
archive.extract_all(options)?; // Seamlessly extracts from all parts
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

See [quickstart guide](./specs/001-unified-archive/quickstart.md) for more examples.

## Supported Formats

| Format    | Read | Extract | Create* | CRC32 | Encryption | Multi-part | SFX Detection | Solid | Recovery |
|-----------|------|---------|---------|-------|------------|------------|---------------|-------|----------|
| **RAR**   | ✅   | ✅      | ❌      | ✅    | ✅         | ✅         | ✅            | ✅    | ✅       |
| **RAR5**  | ✅   | ✅      | ❌      | ✅    | ✅         | ✅         | ✅            | ✅    | ✅       |
| **ZIP**   | ✅   | ✅      | ✅       | ✅    | ✅         | ✅         | ✅            | ❌    | ❌       |
| **7z**    | ✅   | ✅      | ✅       | ✅    | ✅†        | ✅         | ✅            | ✅    | ❌       |
| **TAR**   | ✅   | ✅      | ✅       | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **TAR.GZ**| ✅   | ✅      | ✅       | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **TAR.BZ2**| ✅  | ✅      | ✅       | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **TAR.XZ**| ✅   | ✅      | ✅       | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **GZIP**‡ | ✅   | ✅      | ❌      | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **BZIP2**‡| ✅   | ✅      | ❌      | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **XZ**‡   | ✅   | ✅      | ❌      | ✅    | ❌         | ❌         | ❌            | ❌    | ❌       |
| **ISO**   | ✅   | ✅      | ❌      | ⏳    | ❌         | ❌         | ❌            | ❌    | ❌       |

*Legend:*
- ✅ = Fully supported
- 🚧 = Partially implemented (basic functionality complete, advanced features pending)
- ⏳ = Planned for future release
- ❌ = Not supported by format specification

†7z encryption: read-only via `open_encrypted()`

‡GZIP, BZIP2, and XZ are currently supported only as TAR compound formats (`.tar.gz`, `.tar.bz2`, `.tar.xz`). Standalone `.gz`/`.bz2`/`.xz` files are not yet supported.

**Metadata Features:**
- **Solid**: Solid compression detection (all files compressed as single stream)
- **Recovery**: Recovery record detection and percentage extraction (parity data for archive repair)

## Project Status

**Version 0.1.0 - Beta**

Implementation progress:

- ✅ Phase 1: Setup - Complete
- ✅ Phase 2: Foundation - Complete (format detection, error handling, FFI bindings)
- ✅ Phase 3: Archive Inspection - Complete (list files, metadata, validation, multi-part detection)
- ✅ Phase 4: Archive Extraction - Complete (all formats, progress tracking, streaming, multi-part support)
- 🚧 Phase 5: Archive Creation - Implemented with tracked gaps (metadata preservation, progress callbacks)
- 🚧 Phase 6: Archive Modification - Implemented with tracked gaps (settings/metadata loss during commit)
- 🚧 Phase 7: SFX Detection - Detection and stub extraction working; offset-based opening deferred (Windows PE, Linux ELF, macOS Mach-O, Script interpreters)
- ✅ Phase 8: Polish - Complete; documentation reconciliation in progress (architecture docs and contracts being aligned with implementation state)

**Current capabilities:**
- ✅ Full read/inspect support for all formats
- ✅ Full extraction support for all formats
- ✅ Thread-safe concurrent operations
- ✅ Multi-part archive handling (.z01, .001, .part1.rar)
- ✅ SFX detection and stub extraction (direct embedded-archive opening via `open_at_offset()` is deferred)
- ✅ Password-protected archives (RAR, RAR5, ZIP, 7z)
- ✅ CRC32 integrity validation
- ✅ Archive metadata inspection (solid compression, recovery records with percentage)
- ✅ Symlink detection and warnings
- ✅ Streaming extraction (<100MB memory for 10GB+ archives for libarchive-backed formats (TAR family); ZIP, 7z, and RAR backends currently buffer entries during streaming extraction)

## Architecture

**Backend Engines:**
- **Piz** - ZIP (read/extract)
- **zip crate** - Encrypted ZIP + creation
- **SevenZ** - 7z
- **libarchive** - TAR-family compounds (tar.gz, tar.bz2, tar.xz) and ISO
- **UnRAR** - RAR/RAR5
- **goblin** - Binary parsing for SFX detection (PE/ELF/Mach-O)

**Key Design Decisions:**
- Zero JVM/external tool dependencies
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

## Documentation

- [Quickstart Guide](./specs/001-unified-archive/quickstart.md)
- [Specification](./specs/001-unified-archive/spec.md)
- [Technical Plan](./specs/001-unified-archive/plan.md)
- [API Contracts](./specs/001-unified-archive/contracts/)
- [Task Breakdown](./specs/001-unified-archive/tasks.md)
- [SFX Coverage Report](./docs/sfx_coverage_report.md)
- Examples: See `examples/` directory
- API Documentation: Run `cargo doc --open`

## Performance

- **CRC32**: SIMD-accelerated with crc32fast
- **Memory**: <100MB for 10GB+ archives for libarchive-backed formats (TAR family). ZIP, 7z, and RAR backends currently buffer entries during streaming extraction.
- **SFX Detection**: <100ms for first 1MB scan
- **Thread Safety**: Concurrent operations on different files with zero contention
