# API Contract: Streaming Extraction

**Feature**: 001-unified-archive
**Date**: 2025-10-31
**Phase**: Phase 1 Enhancement
**Status**: Draft

## Overview

Streaming extraction enables memory-bounded extraction of large archives (<100MB memory for 10GB+ archives) through the standard `Read` trait. This contract defines the `extract_to_stream()` API returning a `StreamingExtractor`, automatic CRC32 verification, and composition with standard I/O utilities.

> **Implementation note**: True streaming (bounded memory regardless of entry size) is currently only achieved via the libarchive backend. Piz, SevenZ, and UnRAR backends buffer the full entry in memory and wrap it in a `Cursor`; the streaming API surface is identical but the memory bound applies only to libarchive-backed archives.

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
///
/// let archive = Archive::open("data.zip")?;
/// let mut extractor = archive.extract_to_stream("document.txt")?;
///
/// // Stream to file
/// let mut output = File::create("output.txt")?;
/// std::io::copy(&mut extractor, &mut output)?;
/// ```
pub struct StreamingExtractor {
    /// Internal reader - either from temporary file or direct stream
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
    /// - Piz/SevenZ/UnRAR: buffers full entry in memory, then wraps in Cursor
    ///
    /// # Errors
    ///
    /// * `ArchiveError::Format` - Path not found in archive
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
        match &self.backend {
            ArchiveBackend::Unrar(unrar) => unrar.extract_to_stream(file_path),
            ArchiveBackend::Piz(piz) => piz.extract_to_stream(file_path),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_to_stream(file_path),
            ArchiveBackend::ZipWriter(_) => {
                Err(ArchiveError::write_mode_only("extract_to_stream"))
            }
            ArchiveBackend::ZipReader(zip) => zip.extract_to_stream(file_path),
            ArchiveBackend::Libarchive(lib) => lib.extract_to_stream(file_path),
        }
    }
}
```

## CRC32 Verification

CRC32 verification is **not** built into the `StreamingExtractor`. Instead, it is
handled by:

1. **`ExtractionOptions::verify_crc32`** -- used by `extract_all` / `extract_file` operations,
   which verify after writing each entry via `security::verify_crc32()`.
2. **`security::verify_crc32(data, expected_crc, file_path)`** -- public utility that callers
   can invoke manually after reading the full entry from a `StreamingExtractor`.

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

**Requirement SC-009**: <100MB memory for 10GB+ archives

**Implementation** (libarchive backend -- true streaming):
```
StreamingExtractor memory = backend_stream_buffer
                          ≈ 8KB

With user BufReader: 8KB + 16KB = 24KB per file
With user BufWriter: 24KB + 16KB = 40KB per operation

For 10GB archive: 40KB (one file at a time)
<<100MB
```

**Caveat**: Piz, SevenZ, and UnRAR backends call `extract_to_memory` internally and wrap
the result in a `Cursor`. For those backends, memory usage equals the uncompressed entry
size. True bounded-memory streaming is only available via the libarchive backend.

**Verification**:
```rust
#[test]
fn test_memory_bounded_extraction() {
    let archive = Archive::open("fixtures/10gb_archive.zip")?;
    let start_memory = get_process_memory();

    for entry in archive.list_files()? {
        let mut extractor = archive.extract_to_stream(&entry.path)?;
        let mut output = File::create(dest.join(&entry.path))?;
        io::copy(&mut extractor, &mut output)?;
    }

    let end_memory = get_process_memory();
    let memory_delta = end_memory - start_memory;

    assert!(memory_delta < 100_000_000, // 100MB
        "Memory usage {} exceeded 100MB limit", memory_delta);
}
```

### Streaming Semantics

**Guarantee**: Single-pass, forward-only iteration

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
2. Callers who need verification should use `security::verify_crc32()` after reading the full entry
3. The `extract_all` / `extract_file` paths verify CRC32 automatically when `ExtractionOptions::verify_crc32` is `true`

### Backend Consistency

The `extract_to_stream()` API surface is identical across all backends, but the internal
strategy differs:

| Backend | True Streaming | Internal Strategy | Password Support |
|---------|---------------|-------------------|------------------|
| libarchive | Yes | Direct stream from archive | Yes |
| UnRAR | No | `extract_to_memory` + `Cursor` | Yes |
| Piz | No | `extract_to_memory` + `Cursor` | No |
| SevenZ | No | `extract_to_memory` + `Cursor` | Yes |
| ZipReader | No | `extract_to_memory` + `Cursor` | Yes |

**Note**: Piz, SevenZ, UnRAR, and ZipReader buffer the full entry in memory before exposing it through the `Read` trait. True streaming with bounded memory is only available via the libarchive backend.

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
use std::io;

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
    println!("Copied {} bytes ({:.1}%)", copied,
        extractor.progress().unwrap_or(0.0) * 100.0);
}
```

### Compression Chain Integration

```rust
use flate2::write::GzEncoder;
use flate2::Compression;

// Extract and re-compress on the fly
let mut extractor = archive.extract_to_stream("uncompressed.bin")?;
let output = File::create("compressed.gz")?;
let mut encoder = GzEncoder::new(output, Compression::default());

io::copy(&mut extractor, &mut encoder)?;
encoder.finish()?;

// Memory usage: ~8KB with libarchive backend (streaming, no full file in memory)
// Note: Piz/SevenZ/UnRAR backends will buffer full entry in memory
```

## Error Handling

### Entry Not Found

There is no dedicated `EntryNotFound` variant. When an entry path does not exist in the
archive, backends return `ArchiveError::Format` or `ArchiveError::Io` depending on the
backend.

```rust
match archive.extract_to_stream("nonexistent.txt") {
    Err(ArchiveError::Format { message, .. }) => {
        eprintln!("Entry not found or format error: {}", message);
    }
    Err(ArchiveError::Io { context, .. }) => {
        eprintln!("I/O error looking up entry: {}", context);
    }
    Ok(mut extractor) => { /* use extractor */ }
    Err(e) => { /* other errors */ }
}
```

### Password Required

```rust
// Password must be set on the Archive before calling extract_to_stream.
// extract_to_stream itself does not accept password options.
let result = archive.extract_to_stream("encrypted.txt");

match result {
    Err(ArchiveError::Password { message }) => {
        eprintln!("Password required: {}", message);
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

**Measurement**: Extraction of 1GB file

```rust
#[bench]
fn bench_streaming_extraction(b: &mut Bencher) {
    let archive = Archive::open("fixtures/1gb_file.zip")?;

    b.iter(|| {
        let mut extractor = archive.extract_to_stream("large.bin")?;
        let mut output = io::sink(); // Discard output
        io::copy(&mut extractor, &mut output)?;
    });

    // Expected: ~500 MB/s (disk-bound, not CPU-bound)
}
```

### CRC32 Overhead

CRC32 verification is handled by `ExtractionOptions::verify_crc32` during
`extract_all` / `extract_file` operations, not during `extract_to_stream`.
The streaming API does not perform inline CRC32 verification; callers who
need it should use `security::verify_crc32()` after reading the full entry.

**Expected overhead**: <2% (crc32fast SIMD ~10GB/s, extraction ~500MB/s)

### Memory Usage

**Measurement**: Peak RSS during extraction

```rust
#[test]
fn test_streaming_memory_usage() {
    // Note: this test is only meaningful with libarchive backend.
    // Piz/SevenZ/UnRAR buffer full entry in memory.
    let archive = Archive::open("fixtures/10gb_archive.zip")?;
    let baseline = get_process_memory();

    // Extract all files streaming
    for entry in archive.list_files()? {
        let mut extractor = archive.extract_to_stream(&entry.path)?;
        let mut output = File::create(temp_dir().join(&entry.path))?;
        io::copy(&mut extractor, &mut output)?;
    }

    let peak = get_peak_memory();
    let delta = peak - baseline;

    assert!(delta < 100_000_000, "Peak memory {} exceeded 100MB", delta);
}
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_extract_to_stream_basic() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;

    let mut content = String::new();
    extractor.read_to_string(&mut content)?;

    assert_eq!(content, "Hello, RAR World!\n");
}

#[test]
fn test_extract_to_stream_chunked() {
    let archive = Archive::open("fixtures/test.zip")?;
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
    assert_eq!(content, b"Hello, RAR World!\n");
}

#[test]
fn test_extract_to_stream_progress() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;

    // total_size is known if backend provides it
    if let Some(total) = extractor.total_size() {
        assert!(total > 0);
    }

    let mut content = Vec::new();
    extractor.read_to_end(&mut content)?;

    assert_eq!(extractor.bytes_read(), content.len() as u64);
}
```

### Integration Tests

```rust
#[test]
fn test_extract_to_stream_to_file() {
    let archive = Archive::open("fixtures/test.zip")?;
    let output_path = temp_dir().join("output.txt");
    let mut output = File::create(&output_path)?;

    let mut extractor = archive.extract_to_stream("test_file.txt")?;
    let bytes_written = io::copy(&mut extractor, &mut output)?;

    assert_eq!(bytes_written, 18); // "Hello, RAR World!\n"

    let content = std::fs::read_to_string(&output_path)?;
    assert_eq!(content, "Hello, RAR World!\n");
}

#[test]
fn test_extract_to_stream_to_memory() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut extractor = archive.extract_to_stream("test_file.txt")?;
    let mut buffer = Vec::new();

    extractor.read_to_end(&mut buffer)?;

    assert_eq!(buffer, b"Hello, RAR World!\n");
}

#[test]
fn test_streaming_large_file() {
    // Create 100MB test file
    let archive = create_archive_with_large_file(100 * 1024 * 1024)?;

    let start_memory = get_process_memory();
    let mut extractor = archive.extract_to_stream("large.bin")?;
    let mut output = io::sink();
    io::copy(&mut extractor, &mut output)?;
    let end_memory = get_process_memory();

    let memory_delta = end_memory - start_memory;
    assert!(memory_delta < 10_000_000, // 10MB
        "Memory delta {} too high for streaming", memory_delta);
}
```

## Migration Guide

### From extract_to_memory()

```rust
// Old: All in memory
let content = archive.extract_to_memory("large.bin")?; // Loads full file

// New: Streaming
let mut extractor = archive.extract_to_stream("large.bin")?;
let mut content = Vec::new();
extractor.read_to_end(&mut content)?; // Still loads to Vec, but streaming read

// New: Stream to file directly (memory-bounded with libarchive backend)
let mut extractor = archive.extract_to_stream("large.bin")?;
let mut output = File::create("output.bin")?;
io::copy(&mut extractor, &mut output)?;
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
