# Quickstart Guide: unified-archive

**Feature**: 001-unified-archive
**Date**: 2025-10-31 (Phase 1 Enhanced)
**Purpose**: Get started with unified-archive in 5 minutes

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.4.0"
```

### Build Requirements

With the default feature set the crate links against native C/C++ libraries at build time. You need (none of this is required for a pure-Rust build — `--no-default-features --features read,zip-read`, or any feature set that omits both `libarchive` and `rar-support`; `build.rs` gates the libarchive pkg-config probe on `CARGO_FEATURE_LIBARCHIVE` and the UnRAR C++ build on `rar-support`):
- **pkg-config** (to locate libarchive headers/libraries)
- **make** and a **C++ compiler** (g++ or clang++) for building the UnRAR SDK
- **libarchive** development headers

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install libarchive-dev pkg-config make g++

# Fedora/RHEL
sudo dnf install libarchive-devel pkgconf-pkg-config make gcc-c++
```

**macOS:**
```bash
# Xcode Command Line Tools provide make, clang++, and pkg-config
xcode-select --install
brew install libarchive
```

**Windows:**
Windows support is not yet tested; macOS is the primary platform, Linux secondary. See `build.rs` for current linking status.

## 5-Minute Tutorial

### 1. List Archive Contents

```rust
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    // Works for all supported formats: .zip, .7z, .rar, .tar.gz, .tar.bz2, .tar.xz, .tar, .iso
    // Standalone .gz/.bz2/.xz not directly openable (AD 0018).
    // See "Implementation Status" at the end of this guide for format-specific limitations.
    let archive = Archive::open("data.zip")?;

    // Phase 1: Returns &[ArchiveEntry] (cached for repeated access)
    for entry in archive.list_files()? {
        println!("{}: {} bytes", entry.path, entry.size.unwrap_or(0));

        // Phase 1: Enhanced metadata
        if let Some(ratio) = entry.compression_ratio() {
            println!("  Compression: {:.1}%", ratio * 100.0);
        }
        if entry.is_encrypted {
            println!("  [ENCRYPTED]");
        }
    }

    Ok(())
}
```

### 2. Extract Archive

```rust
use unified_archive::{Archive, ArchiveError, ExtractionOptions};
use std::path::PathBuf;

fn main() -> Result<(), ArchiveError> {
    let archive = Archive::open("backup.7z")?;

    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        preserve_times: true,
        verify_crc32: true,  // Phase 1: Automatic CRC32 verification
        ..Default::default()
    };

    archive.extract_all(options)?;
    println!("Extraction complete!");
    Ok(())
}
```

### 3. Extract with Progress

```rust
use unified_archive::{Archive, ArchiveError, ExtractionOptions};
use std::ops::ControlFlow;
use std::path::PathBuf;

fn main() -> Result<(), ArchiveError> {
    let archive = Archive::open("large.zip")?;

    let options = ExtractionOptions {
        destination: PathBuf::from("output/"),
        progress: Some(Box::new(|processed, total| {
            if let Some(t) = total {
                print!("\rProgress: {:.1}%", processed as f64 / t as f64 * 100.0);
            } else {
                print!("\rProcessed: {} bytes", processed);
            }

            // Cancellation support via ControlFlow
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };

    archive.extract_all(options)?;
    println!("\nExtraction complete!");
    Ok(())
}
```

### 4. Password-Protected Archives

> **Note:** This snippet is illustrative, not self-contained. Password
> acquisition is application-specific; replace the hard-coded string with
> your own input method (CLI prompt, environment variable, GUI dialog).

```rust
// Illustrative — password acquisition is application-specific.
use unified_archive::{Archive, ArchiveError, ExtractionOptions};
use std::path::PathBuf;

fn main() -> Result<(), ArchiveError> {
    let archive = Archive::open("encrypted.zip")?;

    if archive.is_encrypted()? {
        let password = String::from("user-supplied"); // replace with real input

        let options = ExtractionOptions {
            destination: PathBuf::from("output/"),
            password: Some(password),  // Password stored as plain String
            ..Default::default()
        };

        archive.extract_all(options)?;
    }

    Ok(())
}
```

### 5. Streaming Extraction

> **Note:** This snippet is illustrative. The `extract_to_stream` API
> shown here reflects the design contract; check the current API surface
> for exact signatures.

```rust
use unified_archive::{Archive, ArchiveError};
use std::io::{BufReader, Write};
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open("data.tar.gz")?;

    // Streaming extraction (memory-bounded for libarchive-backed formats like TAR.GZ;
    // Piz/ZipReader/SevenZ/UnRAR buffer full entries in memory)
    let reader = archive.extract_to_stream("large.bin")?;
    let mut output = File::create("output.bin")?;

    std::io::copy(&mut BufReader::new(reader), &mut output)?;
    println!("Streamed extraction complete!");
    Ok(())
}
```

## Implementation Status

Phase 1 is implemented with tracked gaps. The items below are grouped by
maturity so callers know what works today and what is still in progress.

### Deferred / Partial

- **Creation progress** — per-entry progress now invoked during creation with `total=None` (OI-025-003 resolved, AD 0021)
- **Unknown-stub SFX scanning** — unknown stubs now proceed to signature scanning (OI-027-001 resolved)
- **Modification metadata loss** — some backends discard mtime/atime during round-trip extraction
- **`open_at_offset`** — opening an archive at an arbitrary byte offset is deferred
- **Standalone gz/bz2/xz** — not directly openable as archives (AD 0018)

### Shipped

- **Entry caching** via `OnceCell` (zero-cost repeated access)
- **Progress callbacks** with `ControlFlow` cancellation for extraction; signature: `fn on_progress(&mut self, processed: u64, total: Option<u64>)`
- **Streaming extraction**: memory-bounded for libarchive-backed formats; Piz, ZipReader, SevenZ, and UnRAR buffer entries
- **Password handling**: passwords stored as `Option<SecStr>` (zeroize-on-drop)
- **CRC32 verification** during extraction
- **SFX detection** with `SfxDetectionResult` (includes `confidence` field); `open_sfx()` and `open_at_offset()` are shipped (temp-file-backed payload opening)
- **Raw compressed streams** (`.gz`, `.bz2`, `.xz`) are directly openable via `Archive::open()` per MADR-0019 (only stream *creation* is out of scope per AD 0018)
- **EntryType** variants: `File`, `Directory`, `Symlink`, `HardLink`, `Other`
- **ArchiveError** variants: `Io`, `Format`, `Corruption`, `Password`, `Unsupported`, `CodecUnavailable`, `WriteModeOnly`, `ReadOnlyBackend`, `NotImplemented`, `OperationBlocked`, `InvalidPath` (see `src/error.rs` for the canonical list)

See [`docs/project/open-issues.md`](../../docs/project/open-issues.md) for the full deferred-items list and known gaps.

## License

Licensed under the MIT License.

**RAR Support Note**: UnRAR license applies (free for non-commercial use). Commercial use requires license from RARLAB. Disable it by selecting features explicitly without `rar-support` — e.g. `--no-default-features --features read,integrity,create,modify,zip-read,zip-write,zip-crypto,sevenzip,libarchive,sfx,v2-api`. A bare `--no-default-features` no longer compiles: `src/lib.rs` refuses a build with no backend feature.
