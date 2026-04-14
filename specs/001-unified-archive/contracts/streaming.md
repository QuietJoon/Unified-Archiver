# API Contract: Streaming Extraction

**Feature**: 001-unified-archive
**Date**: 2025-10-31
**Phase**: Phase 1 Enhancement
**Status**: Implemented (retrospective documentation)

> **Scope note**: This document mixes stable API contract (Core API, Contract
> Guarantees, Error Handling) with illustrative examples and approximate performance
> estimates that have not been validated by automated benchmarks. Sections marked
> "Illustrative" or "Schematic" are informational only and may change without notice.

## Overview

Streaming-style extraction API. All backends expose the same `Read`-based surface via `StreamingExtractor`, but only the libarchive backend provides true bounded-memory streaming. Piz, ZipReader, SevenZ, and UnRAR backends buffer the full entry in memory before constructing `StreamingExtractor` (wrapping it in a `Cursor`). This contract defines the `extract_to_stream()` API returning a `StreamingExtractor` and its composition with standard I/O utilities.

> **Implementation note**: The streaming API surface is identical across backends, but the memory-bounded guarantee applies only to libarchive-backed archives. Non-libarchive backends call `extract_to_memory` internally, so memory usage equals the uncompressed entry size for those backends.

## Core API

### StreamingExtractor

```rust
use std::io::Read;

/// Streaming extractor that implements Read trait
///
/// Allows extracting archive entries directly to a stream without loading
/// entire files into memory (when backed by libarchive). Implements `Read`
/// trait for standard I/O composition.
///
/// # Examples
///
/// ```rust
/// use std::io::{BufReader, Read, Write};
/// use std::fs::File;
/// use unified_archive::Archive;
///
/// let archive = Archive::open("data.tar.gz")?;  // Use libarchive-backed format for true streaming
/// let mut extractor = archive.extract_to_stream("document.txt")?;
///
/// // Stream to file
/// let mut output = File::create("output.txt")?;
/// std::io::copy(&mut extractor, &mut output)?;
/// ```
pub struct StreamingExtractor {
    /// Internal reader - direct libarchive stream or Cursor<Vec<u8>> for buffered backends
    reader: Box<dyn Read + Send>,
    /// Total bytes available (if known)
    total_size: Option<u64>,
    /// Bytes read so far
    bytes_read: u64,
}

impl StreamingExtractor {
    /// Get total size if known
    pub fn total_size(&self) -> Option<u64> { ... }

    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64 { ... }

    /// Get progress as percentage (0.0 to 1.0) if total size is known
    pub fn progress(&self) -> Option<f64> { ... }
}

impl Read for StreamingExtractor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { ... }
}
```

### Archive Integration

```rust
impl Archive {
    /// Get streaming extractor for a specific entry
    ///
    /// # Parameters
    ///
    /// * `file_path` - Path within archive (use forward slashes)
    ///
    /// # Returns
    ///
    /// `StreamingExtractor` that implements `Read` trait
    ///
    /// # Memory Usage
    ///
    /// Depends on backend:
    /// - libarchive: true streaming, bounded memory regardless of entry size
    /// - Piz/ZipReader/SevenZ/UnRAR: buffers full entry in memory, then wraps in Cursor
    ///
    /// # Errors
    ///
    /// * `ArchiveError::Format` or `ArchiveError::Io` - Path not found in archive (error variant is backend-dependent)
    /// * `ArchiveError::Io` - Backend read failure
    /// * `ArchiveError::Password` - Entry encrypted, password required
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Basic streaming
    /// let mut extractor = archive.extract_to_stream("file.txt")?;
    /// let mut content = String::new();
    /// extractor.read_to_string(&mut content)?;
    ///
    /// // With buffering
    /// let extractor = archive.extract_to_stream("large.bin")?;
    /// let mut buffered = BufReader::new(extractor);
    /// // Process in chunks...
    ///
    /// // Stream to file
    /// let mut extractor = archive.extract_to_stream("document.pdf")?;
    /// let mut output = File::create("output.pdf")?;
    /// io::copy(&mut extractor, &mut output)?;
    /// ```
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<StreamingExtractor> {
        // Delegates to the format-specific backend.
        // Returns Err if the archive was opened in write-only mode.
        // ...
    }
}
```

## CRC32 Verification

CRC32 verification is **not** built into the `StreamingExtractor`. Instead, it is
handled by:

1. **`ExtractionOptions::verify_crc32`** -- used by `extract_all` / `extract_file` operations,
   which verify after writing each entry via `security::verify_crc32()`.
2. **`security::verify_crc32(data, expected_crc, file_path)`** -- a public utility function
   (defined in `src/security.rs`) that callers can invoke manually after reading the full
   entry from a `StreamingExtractor`. See also the API reference for the full signature.

```rust
use unified_archive::security::verify_crc32;

let mut extractor = archive.extract_to_stream("file.txt")?;
let mut data = Vec::new();
extractor.read_to_end(&mut data)?;

// Manual CRC32 verification
verify_crc32(&data, Some(expected_crc), "file.txt")?;
```

## Contract Guarantees

### Memory Bounds

**Target SC-009** (libarchive-backed formats only): <100MB memory for 10GB+ archives. This is a design goal, not a hard requirement. Piz/ZipReader/SevenZ/UnRAR backends buffer full entries and are not subject to this target.

**Implementation** (libarchive backend -- true streaming):

The libarchive backend streams data through a fixed-size internal buffer without loading the
full entry into memory. User-side `BufReader`/`BufWriter` wrappers add their own fixed
buffers (typically 8 KB each by default). The combined memory footprint is expected to be
small relative to entry size, but exact figures have not been verified by automated benchmarks
and may vary by platform and libarchive version.

**Caveat**: Piz, ZipReader, SevenZ, and UnRAR backends call `extract_to_memory` internally and wrap
the result in a `Cursor`. For those backends, memory usage equals the uncompressed entry
size. True bounded-memory streaming is only available via the libarchive backend.

> **Verification note**: Memory verification relies on manual profiling or platform-specific
> RSS measurement tools. The test fixtures (`large_archive.tar.gz`) and helpers
> (`get_process_memory`) are not shipped. No automated CI memory check exists today.

### Streaming Semantics

**API guarantee**: Single-pass, forward-only iteration. This is a design choice of the `StreamingExtractor` API, not an inherent limitation of every archive format. Some underlying formats support seeking, but `StreamingExtractor` intentionally exposes only a forward `Read` interface for simplicity and consistency across backends.

**Implications**:
- Cannot seek backwards in StreamingExtractor
- Cannot reuse StreamingExtractor after EOF
- Must create new extractor for re-extraction

**Example**:
```rust
let mut extractor = archive.extract_to_stream("file.txt")?;

// First read: OK
let mut buf1 = vec![0u8; 1024];
extractor.read(&mut buf1)?;

// Read to EOF: OK
let mut buf2 = Vec::new();
extractor.read_to_end(&mut buf2)?;

// Second read: returns 0 (EOF)
let n = extractor.read(&mut buf1)?;
assert_eq!(n, 0);

// To re-read: create new extractor
drop(extractor);
let mut extractor2 = archive.extract_to_stream("file.txt")?;
```

### CRC32 Verification

**Guarantee**: CRC32 verification is available but **not automatic** in the streaming path.

**Behavior**:
1. `extract_to_stream()` does not perform CRC32 verification
2. Callers who need verification must buffer the full entry (e.g., `read_to_end` into a `Vec`) and then call `security::verify_crc32()`. This defeats the bounded-memory benefit of libarchive streaming for entries that require verification.
3. The `extract_all` / `extract_file` paths verify CRC32 automatically when `ExtractionOptions::verify_crc32` is `true`

### Backend Consistency

The `extract_to_stream()` API surface is identical across all backends, but the internal
strategy differs:

| Backend | True Streaming | Internal Strategy | Password Support |
|---------|---------------|-------------------|------------------|
| libarchive | Yes | Direct stream from archive | Yes (passphrase set on `Archive` before extraction) |
| UnRAR | No | `extract_to_memory` + `Cursor` | Yes (passphrase set on `Archive` before extraction) |
| Piz | No | `extract_to_memory` + `Cursor` | No (plain ZIP only) |
| SevenZ | No | `extract_to_memory` + `Cursor` | Yes (passphrase set on `Archive` before extraction) |
| ZipReader | No | `extract_to_memory` + `Cursor` | Yes (encrypted ZIP; passphrase set on `Archive` before extraction) |

**Note**: Piz, SevenZ, UnRAR, and ZipReader buffer the full entry in memory before exposing it through the `Read` trait. True streaming with bounded memory is only available via the libarchive backend. Password must be set on the `Archive` instance before calling `extract_to_stream`; the method itself does not accept password options. ZipReader is specifically the backend used for encrypted ZIP archives.

## Standard Library Composition

### BufReader Integration

```rust
use std::io::BufReader;

// Automatic buffering for efficient reads
let extractor = archive.extract_to_stream("large.bin")?;
let mut buffered = BufReader::with_capacity(64 * 1024, extractor); // 64KB buffer

// Read line-by-line (for text files)
use std::io::BufRead;
let extractor = archive.extract_to_stream("lines.txt")?;
let buffered = BufReader::new(extractor);
for line in buffered.lines() {
    println!("{}", line?);
}
```

### io::copy Integration

```rust
use std::io::{self, Read, Write};
use std::fs::File;

// Copy to file
let mut extractor = archive.extract_to_stream("source.bin")?;
let mut output = File::create("destination.bin")?;
io::copy(&mut extractor, &mut output)?;

// Copy with progress
let mut extractor = archive.extract_to_stream("large.bin")?;
let mut output = File::create("destination.bin")?;
let mut copied = 0u64;
let mut buffer = [0u8; 8192];

loop {
    let n = extractor.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    output.write_all(&buffer[..n])?;
    copied += n as u64;
    // progress() returns Some only when total_size is known
    if let Some(pct) = extractor.progress() {
        println!("Copied {} bytes ({:.1}%)", copied, pct * 100.0);
    } else {
        println!("Copied {} bytes (total size unknown)", copied);
    }
}
```

### Compression Chain Integration

```rust
use std::io;
use std::fs::File;
use flate2::write::GzEncoder;
use flate2::Compression;

// Extract and re-compress on the fly
let mut extractor = archive.extract_to_stream("uncompressed.bin")?;
let output = File::create("compressed.gz")?;
let mut encoder = GzEncoder::new(output, Compression::default());

io::copy(&mut extractor, &mut encoder)?;
encoder.finish()?;

// Memory usage with libarchive backend: bounded (streaming, no full file in memory)
// Non-libarchive backends (Piz/ZipReader/SevenZ/UnRAR) buffer full entry in memory
```

## Error Handling

### Entry Not Found

There is no dedicated `EntryNotFound` variant. The error variant returned for a missing
entry path is **backend-dependent**: some backends return `ArchiveError::Format`, others
return `ArchiveError::Io`. Callers should handle both variants when catching missing-entry
errors.

```rust
match archive.extract_to_stream("nonexistent.txt") {
    Err(ArchiveError::Format { message, .. }) => {
        eprintln!("Entry not found or format error: {}", message);
    }
    Err(ArchiveError::Io { operation, path, source }) => {
        eprintln!("I/O error during '{}' on {}: {}", operation, path.display(), source);
    }
    Ok(mut extractor) => { /* use extractor */ }
    Err(e) => { /* other errors: {} */ eprintln!("{}", e); }
}
```

### Password Required

Password must be set on the `Archive` before calling `extract_to_stream`. Use
`Archive::open_encrypted(path, password)` (defined in `src/archive.rs`) to open a
password-protected archive. The `extract_to_stream` method itself does not accept
password options.

```rust
// Open with password via the public open_encrypted() constructor
let archive = Archive::open_encrypted("encrypted.7z", "my_password")?;

// Now streaming extraction works for encrypted entries
let result = archive.extract_to_stream("encrypted.txt");

match result {
    Err(ArchiveError::Password { message }) => {
        eprintln!("Password required or incorrect: {}", message);
    }
    Ok(mut extractor) => { /* use extractor */ }
    Err(e) => { /* other errors */ }
}
```

### Backend Errors

```rust
let mut extractor = archive.extract_to_stream("corrupted.bin")?;
let mut output = Vec::new();

match extractor.read_to_end(&mut output) {
    Err(e) if e.kind() == io::ErrorKind::InvalidData => {
        eprintln!("Archive corrupted: {}", e);
    }
    Err(e) => {
        eprintln!("I/O error: {}", e);
    }
    Ok(bytes_read) => {
        println!("Read {} bytes", bytes_read);
    }
}
```

## Performance Characteristics

### Throughput

**Illustrative estimate**: ~500 MB/s for uncompressed entries (disk-bound, not CPU-bound). This figure is a rough order-of-magnitude estimate; no published benchmark artifact exists in this repository.

> **Note**: If performance benchmarks are added, they should use [Criterion](https://docs.rs/criterion/) for stable, statistically rigorous measurement. The unstable `#[bench]` API is not used. See `benches/` directory if benchmark implementations are added in the future.

### CRC32 Overhead

CRC32 verification is handled by `ExtractionOptions::verify_crc32` during
`extract_all` / `extract_file` operations, not during `extract_to_stream`.
The streaming API does not perform inline CRC32 verification; callers who
need it should use `security::verify_crc32()` after reading the full entry.

**Rough estimate**: <2% overhead (crc32fast SIMD is rated at ~10GB/s, far exceeding expected extraction throughput). This estimate is not backed by a benchmark artifact in this repository.

### Memory Usage

**Measurement**: Peak RSS during extraction

> **Future verification (not yet implemented)**: Memory verification for streaming
> extraction would measure peak RSS using fixtures like `10gb_archive.zip` and helpers
> such as `get_process_memory` / `get_peak_memory`. These are not currently shipped.
> Today, memory verification relies on manual profiling or platform-specific RSS tools.
> The <100MB target applies to libarchive-backed formats only; Piz/ZipReader/SevenZ/UnRAR
> backends buffer full entries in memory and are not subject to this target.

## Testing Strategy

### Unit Tests

> **Note**: The following tests are illustrative pseudocode demonstrating the intended
> contract verification approach. They are not compiled as part of the test suite.
> See `tests/` for actual shipped tests.

```rust
// Note: Fixture paths shown as "tests/fixtures/..." -- adjust to match
// actual project layout. Content strings are illustrative.

#[test]
fn test_extract_to_stream_basic() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;

    let mut content = String::new();
    extractor.read_to_string(&mut content)?;

    assert_eq!(content, "Hello, World!\n");
}

#[test]
fn test_extract_to_stream_chunked() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;

    let mut chunks = Vec::new();
    let mut buffer = [0u8; 4];

    loop {
        let n = extractor.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        chunks.push(buffer[..n].to_vec());
    }

    let content: Vec<u8> = chunks.into_iter().flatten().collect();
    assert_eq!(content, b"Hello, World!\n");
}

#[test]
fn test_extract_to_stream_progress() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;

    // total_size may be None if the backend does not report it
    if let Some(total) = extractor.total_size() {
        assert!(total > 0);
    }

    let mut content = Vec::new();
    extractor.read_to_end(&mut content)?;

    assert_eq!(extractor.bytes_read(), content.len() as u64);
}
```

### Integration Tests

> **Note**: Illustrative pseudocode. See `tests/` for actual shipped integration tests.

```rust
#[test]
fn test_extract_to_stream_to_file() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let output_path = temp_dir().join("output.txt");
    let mut output = File::create(&output_path)?;

    let mut extractor = archive.extract_to_stream("test_file.txt")?;
    let bytes_written = io::copy(&mut extractor, &mut output)?;

    assert_eq!(bytes_written, 14); // "Hello, World!\n"

    let content = std::fs::read_to_string(&output_path)?;
    assert_eq!(content, "Hello, World!\n");
}

#[test]
fn test_extract_to_stream_to_memory() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;
    let mut buffer = Vec::new();

    extractor.read_to_end(&mut buffer)?;

    assert_eq!(buffer, b"Hello, World!\n");
}
```

### Backlog: Memory Verification (Not Shipped)

The following test sketch illustrates the intended memory verification approach for the
libarchive backend. It depends on unshipped helpers (`create_archive_with_large_file`,
`get_process_memory`) and large test fixtures. Implementation is deferred to a future
milestone when memory-profiling infrastructure is in place.

```rust
// BACKLOG -- not compiled, depends on unshipped helpers
#[test]
#[ignore]
fn test_streaming_large_file() {
    let archive = create_archive_with_large_file(100 * 1024 * 1024)?;

    let start_memory = get_process_memory();
    let mut extractor = archive.extract_to_stream("large.bin")?;
    let mut output = io::sink();
    io::copy(&mut extractor, &mut output)?;
    let end_memory = get_process_memory();

    let memory_delta = end_memory - start_memory;
    // <100MB target for libarchive backend; other backends buffer full entry
    assert!(memory_delta < 100_000_000,
        "Memory delta {} too high for streaming", memory_delta);
}
```

## Choosing Between extract_to_memory() and extract_to_stream()

Both methods are supported and neither is deprecated. Choose based on your use case:

| Use case | Recommended method | Why |
|----------|-------------------|-----|
| Small entries, need `Vec<u8>` | `extract_to_memory()` | Simpler API, returns bytes directly |
| Large entries, write to disk | `extract_to_stream()` | Bounded memory with libarchive backend |
| Need CRC verification | `extract_to_memory()` or `extract_all` | Streaming path requires manual CRC after buffering |
| Pipe to another I/O sink | `extract_to_stream()` | Composes with `io::copy`, `BufReader`, etc. |

```rust
// extract_to_memory: returns full entry as Vec<u8> (simple, but loads all into memory)
let content = archive.extract_to_memory("small.txt")?;

// extract_to_stream: returns a Read impl (bounded memory with libarchive backend)
let mut extractor = archive.extract_to_stream("large.bin")?;
let mut output = File::create("output.bin")?;
io::copy(&mut extractor, &mut output)?;

// Caution: read_to_end on a StreamingExtractor loads the full entry into memory,
// which is equivalent to extract_to_memory in terms of peak memory usage.
let mut extractor = archive.extract_to_stream("large.bin")?;
let mut content = Vec::new();
extractor.read_to_end(&mut content)?;
```

### From 7zip-JBinding

```java
// 7zip-JBinding
IInArchive archive = ...;
ISequentialOutStream stream = ...;
archive.extractSlow(index, stream);
```

```rust
// unified-archive
let mut extractor = archive.extract_to_stream("file.bin")?;
let mut output = File::create("output.bin")?;
io::copy(&mut extractor, &mut output)?;
```

## References

- Phase 0 Research: `research.md` - Streaming extraction architecture decisions
- Data Model: `data-model.md` - StreamingExtractor specification
- Constitution v1.1.0 Principle II: Pragmatic performance (memory bounds)
- Rust std::io::Read: https://doc.rust-lang.org/std/io/trait.Read.html
- crc32fast: https://docs.rs/crc32fast/
