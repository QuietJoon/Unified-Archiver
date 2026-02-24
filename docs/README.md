# unified-archive Documentation

Complete documentation for the unified-archive Rust library.

## Quick Links

- **[Getting Started](./GETTING_STARTED.md)** - Installation and first program
- **[API Reference](./API_REFERENCE.md)** - Complete API documentation
- **[Main README](../README.md)** - Project overview and features
- **[Limitations](../Limitations.md)** - Known limitations and workarounds
- **[Changelog](../CHANGELOG.md)** - Version history

---

## Documentation Structure

### For New Users

1. **[Getting Started Guide](./GETTING_STARTED.md)**
   - Installation instructions
   - Your first program
   - Common use cases with examples
   - Next steps

2. **[Main README](../README.md)**
   - Feature overview
   - Quick examples
   - Supported formats
   - Installation guide

### For Developers

3. **[API Reference](./API_REFERENCE.md)**
   - Complete API documentation
   - All public types and methods
   - Code examples for each function
   - Error handling guide
   - Performance tips

4. **[Examples Directory](../examples/)**
   - `inspect_archive.rs` - List archive contents
   - `extract_archive.rs` - Extract with progress tracking
   - `streaming_extract.rs` - Memory-efficient extraction
   - `test_extract.rs` - Simple extraction example

### Reference Material

5. **[Limitations](../Limitations.md)**
   - Known limitations
   - Workarounds
   - Future enhancements
   - Platform-specific notes

6. **[Changelog](../CHANGELOG.md)**
   - Version history
   - Release notes
   - Migration guides

---

## Topics

### Installation

- [System Requirements](./GETTING_STARTED.md#prerequisites)
- [Adding to Your Project](./GETTING_STARTED.md#add-to-your-project)
- [Verification](./GETTING_STARTED.md#verify-installation)

### Basic Usage

- [Opening Archives](./API_REFERENCE.md#archiveopen)
- [Listing Files](./API_REFERENCE.md#archivelist_files)
- [Extracting Files](./API_REFERENCE.md#archiveextract_all)
- [Reading Metadata](./API_REFERENCE.md#archiveentry)

### Advanced Features

- [Password-Protected Archives](./GETTING_STARTED.md#use-case-7-password-protected-archives)
- [Streaming Extraction](./GETTING_STARTED.md#use-case-5-process-large-files-efficiently)
- [Progress Tracking](./GETTING_STARTED.md#use-case-8-progress-tracking)
- [Parallel Extraction](./API_REFERENCE.md#archiveextract_filtered)

### Error Handling

- [Error Types](./API_REFERENCE.md#archiveerror)
- [Error Handling Examples](./GETTING_STARTED.md#error-handling)
- [Common Errors](./API_REFERENCE.md#example-error-handling)

### Performance

- [Memory Efficiency](./API_REFERENCE.md#streamingextractor)
- [Parallel Processing](./API_REFERENCE.md#performance-tips)
- [CRC32 Computation](../README.md#performance-characteristics)

---

## Code Examples

### Quick Start

```rust
use unified_archive::Archive;

// Open any archive format
let archive = Archive::open("file.zip")?;

// List contents
for entry in archive.list_files()? {
    println!("{}: {} bytes", entry.path, entry.size.unwrap_or(0));
}
```

### Extract All Files

```rust
use unified_archive::{Archive, ExtractionOptions};
use std::path::PathBuf;

let options = ExtractionOptions {
    destination: PathBuf::from("./output"),
    preserve_permissions: true,
    ..Default::default()
};

Archive::open("backup.7z")?.extract_all(options)?;
```

### Extract to Memory

```rust
let archive = Archive::open("data.rar")?;
let data = archive.extract_to_memory("config.json")?;
let text = String::from_utf8(data)?;
```

### Streaming Large Files

```rust
use std::io::Read;

let mut stream = archive.extract_to_stream("large.bin")?;
let mut buffer = [0u8; 8192];

while let Ok(n) = stream.read(&mut buffer) {
    if n == 0 { break; }
    // Process chunk
}
```

---

## Supported Formats

| Format | Read | Extract | CRC32 | Password |
|--------|------|---------|-------|----------|
| RAR    | ✅   | ✅      | ✅    | ✅       |
| RAR5   | ✅   | ✅      | ✅    | ✅       |
| ZIP    | ✅   | ✅      | ✅    | ⏳       |
| 7z     | ✅   | ✅      | ✅    | ⏳       |
| TAR    | ✅   | ✅      | ✅    | ❌       |
| TAR.GZ | ✅   | ✅      | ✅    | ❌       |
| TAR.BZ2| ✅   | ✅      | ✅    | ❌       |
| TAR.XZ | ✅   | ✅      | ✅    | ❌       |

See [Supported Formats](../README.md#supported-formats) for details.

---

## API Overview

### Core Types

- **[Archive](./API_REFERENCE.md#archive)** - Main archive handle
- **[ArchiveEntry](./API_REFERENCE.md#archiveentry)** - File metadata
- **[ArchiveFormat](./API_REFERENCE.md#archiveformat)** - Format enumeration
- **[ExtractionOptions](./API_REFERENCE.md#extractionoptions)** - Extraction config
- **[ProgressCallback](./API_REFERENCE.md#progresscallback)** - Progress monitoring
- **[StreamingExtractor](./API_REFERENCE.md#streamingextractor)** - Stream interface

### Main Operations

```rust
// Opening
Archive::open(path)
Archive::open_encrypted(path, password)

// Inspection
archive.format()
archive.list_files()
archive.find_entry(path)
archive.is_encrypted()
archive.validate_integrity()

// Extraction
archive.extract_all(options)
archive.extract_file(path, dest)
archive.extract_filtered(predicate, options)
archive.extract_to_memory(path)
archive.extract_to_stream(path)
```

---

## Testing

Run the test suite:

```bash
# All tests
cargo test

# Specific test
cargo test test_zip_extraction

# Performance tests
cargo test --release perf_

# With output
cargo test -- --nocapture
```

**Test Coverage**: 82 tests across all formats and features

---

## Performance Characteristics

- **Memory Usage**: <100MB for multi-GB files (streaming)
- **CRC32 Speed**: ~300MB/s (SIMD-accelerated)
- **Parallel Extraction**: Linear scaling to CPU cores
- **Format Detection**: O(1) header read

See [Performance](../README.md#performance-characteristics) for details.

---

## Common Issues

### Archive Opening Fails

```rust
match Archive::open("file.rar") {
    Err(ArchiveError::NotFound { path }) => {
        eprintln!("File doesn't exist: {}", path);
    },
    Err(ArchiveError::UnsupportedFormat { .. }) => {
        eprintln!("Format not supported");
    },
    Ok(archive) => { /* use it */ },
    Err(e) => eprintln!("Error: {}", e),
}
```

### Password Required

```rust
// Check first
if archive.is_encrypted()? {
    // Use open_encrypted instead
    let archive = Archive::open_encrypted(path, password)?;
}
```

### Iterator Exhaustion (RAR)

```rust
// Don't do this - iterator exhausted after first call
let entries1 = archive.list_files()?;
let entries2 = archive.list_files()?; // Empty!

// Do this instead
let archive1 = Archive::open(path)?;
let entries1 = archive1.list_files()?;

let archive2 = Archive::open(path)?;
let entries2 = archive2.list_files()?;
```

See [Limitations](../Limitations.md) for more details.

---

## Platform Notes

### macOS
- Requires: `brew install libarchive`
- Tested on: Darwin 24.6.0
- Status: ✅ Fully supported

### Linux
- Requires: `apt-get install libarchive-dev` or `dnf install libarchive-devel`
- Tested on: Ubuntu/Debian, Fedora
- Status: ✅ Fully supported

### Windows
- libarchive bundled automatically
- Status: ⏳ Not yet tested

---

## Contributing

See the main README for contribution guidelines.

For documentation improvements:
1. Update relevant .md files
2. Test examples compile: `cargo test --doc`
3. Build docs: `cargo doc --no-deps`
4. Submit PR with clear description

---

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/yourusername/unified-archive/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/unified-archive/discussions)
- **Documentation**: You're reading it!

---

## License

Dual licensed under Apache-2.0 OR MIT (your choice).

**RAR Support**: Uses UnRAR library (free for non-commercial use).
See [LICENSE](../LICENSE-APACHE) for details.

---

**Last Updated**: 2025-11-01
**Version**: 0.1.0
**Rust**: 1.85+ (edition 2024)
