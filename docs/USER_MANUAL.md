---
type: User Manual
title: "unified-archive User Manual"
description: "This manual is the practical guide for using unified-archive v0.4.0."
tags: [reference]
timestamp: 2026-05-03T00:00:00Z
status: active
---

# unified-archive User Manual

This manual is the practical guide for using `unified-archive` `v0.4.0`.

It focuses on what public users need to know:

- how to install the crate
- which formats and workflows are supported
- how to inspect, extract, create, and modify archives
- where the important caveats are

For exact type and method signatures, use the [API Reference](./API_REFERENCE.md).

## 1. What The Library Is

`unified-archive` is a Rust library that gives you one API across multiple archive formats.

The main entry point is [`Archive`](./API_REFERENCE.md#archive). You open an archive once, then use the same methods to:

- inspect metadata and entry lists
- extract files or full archives
- validate integrity
- read encrypted archives
- create new archives
- modify ZIP and 7z archives
- detect and open self-extracting archives

## 2. Installation

Add the crate to `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.4.0"
```

> **Not on crates.io.** No version of this crate has been published to a public registry, so
> the line above will not resolve as written. Until it is published, depend on the crate by
> `path` or `git`.

### Build prerequisites

**macOS**

```bash
brew install libarchive pkg-config
```

**Ubuntu / Debian**

```bash
sudo apt-get update
sudo apt-get install libarchive-dev pkg-config g++
```

**Fedora / RHEL**

```bash
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++
```

Additional notes:

- `libarchive` is required on macOS and Linux.
- `pkg-config` is needed so `build.rs` can locate and link `libarchive`. A C++ compiler is needed
  because the bundled UnRAR SDK (default `rar-support` feature) is built as part of the crate: `build.rs` compiles its sources through the `cc` crate, so no `make` is involved.
- Windows support exists in the codebase, but `v0.4.0` is not release-verified on Windows yet.

## 3. Supported Workflows

### Read / inspect / extract

Supported through the main `Archive` facade:

- RAR / RAR5
- ZIP
- 7z
- TAR
- TAR.GZ
- TAR.BZ2
- TAR.XZ
- TAR.ZST / TAR.LZ4 / TAR.LZMA
- standalone GZIP / BZIP2 / XZ / ZST / LZ4 / LZMA
- ISO

### Create archives with `Archive::create`

Supported:

- ZIP
- 7z
- TAR
- TAR.GZ
- TAR.BZ2
- TAR.XZ
- TAR.ZST / TAR.LZ4 / TAR.LZMA — requires a libarchive built with the matching zstd / lz4 / lzma
  **write** filter; stock Homebrew and vcpkg builds normally carry all three. A missing filter fails
  at writer construction rather than silently degrading to an uncompressed TAR.

Not supported through `Archive::create` (read/extract only):

- standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma`
- ISO (libarchive ISO support is read-only)
- RAR / RAR5 (creation is out of scope; see `external::RarCreator`)

### Modify existing archives

Supported:

- ZIP
- 7z

Not supported:

- RAR / RAR5
- TAR family
- standalone GZIP / BZIP2 / XZ / ZST / LZ4 / LZMA
- ISO

### Read encrypted archives

Supported through `Archive::open_encrypted`:

- RAR / RAR5
- ZIP
- 7z

### Create encrypted archives

Not supported by `Archive::create` in `v0.4.0`.

If you pass `CompressionOptions.password` to `Archive::create`, the call is rejected with `ArchiveError::OperationBlocked`.

### Optional RAR creation

There is an **optional** Windows-only integration behind the `external-rar-create` feature flag:

- module: `unified_archive::external`
- type: `external::RarCreator`
- requirement: licensed WinRAR / `rar.exe`

This is **not** part of the main `Archive::create` flow and should be treated as a separate, platform-specific escape hatch.

#### Locating `rar.exe`

Discovery is intentionally narrow and runs entirely in-process — no `where.exe` is spawned, because
`where` itself resolves against the current directory first and a planted `where.exe` would have
been executed. `PATH` is walked directly (relative entries skipped, batch shims refused), then the
stock WinRAR directories under `%ProgramFiles%` and `%ProgramFiles(x86)%` are probed. Custom or portable installs are not
auto-detected — add the directory containing `rar.exe` to your `PATH` before
constructing `RarCreator`.

#### Password handling caveat

`RarCreator::set_password` stores the password in [`SecStr`](https://docs.rs/secstr) inside
the calling process, but at archive-creation time it is forwarded to
`rar.exe` as the `-hp{password}` command-line switch. The Windows process
listing therefore shows the password in plaintext for the lifetime of the
worker. `SecStr` cannot mask process argv.

If leaking the password to the local OS process listing is unacceptable for
your environment, prefer the in-process `rar-support` Cargo feature for
read-side workloads, or use a non-CLI archiver. The external-RAR path is
provided for convenience; it is not a confidential channel.

## 4. First Useful Program

```rust
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    let archive = Archive::open("example.zip")?;

    println!("Format: {:?}", archive.format());
    println!("Entries:");

    for entry in archive.list_files()? {
        println!(
            "  {} (size: {}, crc32: {:08X?})",
            entry.path,
            entry.size.unwrap_or(0),
            entry.crc32
        );
    }

    Ok(())
}
```

## 5. Common Tasks

### 5.1 Inspect an archive

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.7z")?;
let entries = archive.list_files()?;

for entry in entries {
    println!("{} {:?}", entry.path, entry.entry_type);
}
```

Useful inspection methods:

- `format()`
- `list_files()`
- `entry_count()`
- `find_entry()`
- `is_encrypted()`
- `is_solid()`
- `has_recovery_record()`
- `recovery_percentage()`
- `validate_integrity()`

### 5.2 Extract everything

```rust
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.tar.gz")?;

let options = ExtractionOptions::new("./output")
    .preserve_permissions(true)
    .preserve_times(true);

// extract_all returns ResultWithWarnings<()> — the call succeeded even if
// warnings (e.g., skipped symlinks or hard-links) are present.
let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

### 5.3 Extract one file

```rust
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("assets.zip")?;

archive.extract_file(
    "images/logo.png",
    ExtractionOptions::new("./output"),
)?;
```

### 5.4 Extract to memory

```rust
use unified_archive::Archive;

let archive = Archive::open("config.rar")?;
let bytes = archive.extract_to_memory("config.json")?;
let text = String::from_utf8(bytes)?;
```

This is convenient for configs, manifests, and other small files.

### 5.5 Stream a large file

```rust
use std::io::Read;
use unified_archive::{Archive, StreamBound};

let archive = Archive::open("media.tar")?;
let mut stream = archive.extract_to_stream("large-video.bin", StreamBound::DeclaredSize)?;

let mut buffer = [0u8; 8192];
loop {
    let n = stream.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    // Process chunk
}
```

Important caveat:

- bounded-memory streaming currently applies to **libarchive-backed** formats only, which means the TAR family, ISO, and the standalone compressed formats (`.gz`, `.bz2`, `.xz`, `.zst`, `.lz4`, `.lzma`)
- ZIP, 7z, and RAR still expose the same `Read` interface, but they buffer the full entry before handing it to `StreamingExtractor`
- propagate read errors instead of treating them as end-of-stream: under a hard cap (`StreamBound::DeclaredSize` or `StreamBound::Cap(n)`) an archive that emits more bytes than its header declared surfaces an `io::ErrorKind::InvalidData` read error rather than a silent EOF, and under `StreamBound::DeclaredSize` one that ends *early* surfaces an `io::ErrorKind::UnexpectedEof` error rather than a short read; a `while let Ok(n) = stream.read(..)` loop would swallow either and hand you truncated data

### 5.6 Open an encrypted archive

```rust
use unified_archive::Archive;

let archive = Archive::open_encrypted("secret.zip", "password123")?;
let entries = archive.list_files()?;
println!("{} entries", entries.len());
```

Notes:

- `is_encrypted()` lets you detect password-protected archives
- RAR archives created with encrypted headers (`-hp`) cannot be listed without the password
- this library reads encrypted archives but does not create them through `Archive::create`

### 5.7 Create a new archive

```rust
use unified_archive::{Archive, CompressionOptions, WritableFormat};

let options = CompressionOptions::for_writable(WritableFormat::ZIP);
let mut archive = Archive::create("release.zip", options)?;

archive.add_file_from_data("README.txt", b"hello")?;
archive.add_file_from_path("Cargo.toml")?;
archive.finish()?;
```

Good to know:

- the output path must not already exist
- creation-time passwords are rejected in `v0.4.0`
- standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma` creation is out of scope

### 5.8 Modify an existing ZIP or 7z archive

```rust
use unified_archive::Archive;

let mut archive = Archive::modify("release.zip")?;
archive.add_entry("notes.txt", b"new file")?;
archive.remove_entry("old.txt")?;
archive.commit_changes()?;
```

If you need backup or metadata-preservation behavior:

```rust
use unified_archive::{Archive, ModificationOptions};

let opts = ModificationOptions::default().with_backup("bak");
let mut archive = Archive::modify_with_options("release.zip", opts)?;
archive.replace_entry("notes.txt", b"updated")?;
archive.commit_changes()?;
```

Important caveat:

- modification is **rewrite-based**, not in-place
- `modify_with_options()` preserves modified, accessed, and created
  timestamps plus Unix permissions on retained regular-file entries,
  where the backend supports the timestamp; directory metadata still
  uses backend defaults
- ZIP rewrites preserve archive comments and per-entry stored/deflated method
- modification of an encrypted archive is refused: `Archive::modify` probes the
  source and returns `OperationBlocked` labelled `modify` with the reason
  "Encrypted archives cannot be modified (password-aware modification not yet
  supported)"
- the rewrite **drops symlink, hardlink, and other special entries entirely** —
  they are not re-emitted into the replacement archive, and the dry-run
  namespace gate models the same drop, so their paths are free for new entries
  (`rewrite_drops_entry_type` in `src/modification.rs`)

### 5.9 Detect and open a self-extracting archive

```rust
use unified_archive::Archive;

let detection = Archive::detect_sfx("installer.exe")?;
if let Some((format, offset, stub)) = detection.payload_coordinates() {
    println!("Stub type: {:?}", stub);
    println!("Payload format: {:?}", format);
    println!("Offset: {}", offset);
}

let embedded = Archive::open_sfx("installer.exe")?;
println!("Embedded entries: {}", embedded.entry_count()?);
```

Related methods:

- `detect_sfx()`
- `open_sfx()`
- `open_at_offset()`
- `extract_stub()`

## 6. Important Caveats In v0.4.0

### Split archives

**No split/multi-volume archive can be extracted end-to-end in `v0.4.0`.**

RAR / RAR5 volume sets are *detected and listed*: `Archive::open` on the first volume works, and
`detect_multipart()` / `volume_set_report()` enumerate the parts and name any that are missing. But
every extraction path is closed, because a split file is listed once per volume with all copies
sharing one path: `extract_all` trips the duplicate-output-path guard, `extract_file` and
`extract_to_memory` refuse to disambiguate, and `extract_by_ids` — the call those two errors
recommend — fails with a listing-drift error. `ArchiveFormat::Rar`/`Rar5` report
`multipart_read: Support::Partial` accordingly.

ZIP split volumes (`.z01`, `.z02`, …) are name-level detection only — `detect_multipart` enumerates
sibling file names and never opens a volume. 7z numeric split volumes (`.001`, `.002`, …) are not
supported at all.

### Standalone compressed files

Standalone `.gz`, `.bz2`, `.xz`, `.zst`, `.lz4`, and `.lzma` files can be opened and extracted, but not created or modified.

Use `.tar.gz`, `.tar.bz2`, and `.tar.xz` if you need compressed archive creation.

### Encrypted creation

`Archive::create` does not produce encrypted archives in `v0.4.0`.

That applies even to ZIP.

### Platform coverage

- macOS: tested
- Linux: tested
- Windows: present in codebase, not release-verified

### Temporary files

Some operations stage data in temporary files:

- `open_at_offset()` / `open_sfx()` read a ZIP, RAR or 7z payload **in place** when the path has an
  executable extension and the bytes at the offset carry that format's signature — nothing is
  copied and no staging ceiling applies (DCR-015; the RAR arm needs `rar-support`). Otherwise —
  the TAR family, ISO, or any open that declines those gates — the payload is staged into a
  temporary file bounded by `ExtractionLimits::max_sfx_payload_size`. `Archive::payload_access()`
  reports which happened
- UnRAR-backed `extract_to_memory()` uses a temporary extraction path internally

That behavior is expected in `v0.4.0`.

## 7. How To Choose The Right Extraction API

Use:

- `list_files()` when you only need metadata
- `extract_all()` when you need files on disk
- `extract_file()` for one disk target
- `extract_to_memory()` for small in-memory content
- `extract_to_stream()` for incremental reads, especially TAR / ISO workloads

## 8. Safety Features

Extraction paths are guarded by:

- path sanitization to prevent path traversal
- overwrite checking
- symlink / hard-link warnings
- extraction limits (`ExtractionLimits`) for zip-bomb resistance

If you need to tune resource limits, pass a built `ExtractionLimits` to the
`.limits(...)` setter on `ExtractionOptions`.

## 9. Troubleshooting

### "File does not exist" or other I/O errors

Check:

- the input path exists
- the output directory is writable
- required system packages (`libarchive`, `pkg-config`) are installed

### Password errors

Use `Archive::open_encrypted(...)` for encrypted archives instead of `Archive::open(...)`.

### Streaming uses more memory than expected

That usually means you are on a buffered backend such as ZIP, 7z, or RAR. For true bounded-memory streaming, prefer TAR-family archives, ISO, or a standalone compressed stream.

### Creation with `password` fails

That is expected in `v0.4.0`. The main facade does not create encrypted archives.

## 10. Where To Go Next

- [Getting Started](./GETTING_STARTED.md) for quick recipes
- [API Reference](./API_REFERENCE.md) for exact signatures and error behavior
- [Limitations](../Limitations.md) for the full caveat list
- [README](../README.md) for the project overview
