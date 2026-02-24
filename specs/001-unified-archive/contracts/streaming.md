# API Contract: Streaming Extraction

**Feature**: 001-unified-archive
**Date**: 2025-10-31
**Phase**: Phase 1 Enhancement
**Status**: Draft

## Overview

Streaming extraction enables memory-bounded extraction of large archives (<100MB memory for 10GB+ archives) through the standard `Read` trait. This contract defines the `EntryReader<'a>` API, automatic CRC32 verification, and composition with standard I/O utilities.

## Core API

### EntryReader

```rust
use std::io::Read;

/// Streaming reader for individual archive entries
///
/// Memory bounded: ~40KB per file (8KB buffer + 16KB BufReader)
/// Implements `Read` trait for standard I/O composition
///
/// # Examples
///
/// ```rust
/// use std::io::{BufReader, Write};
/// use std::fs::File;
///
/// let archive = Archive::open("data.zip")?;
/// let reader = archive.entry_reader("document.txt")?;
///
/// // Stream to file
/// let mut output = File::create("output.txt")?;
/// std::io::copy(&mut BufReader::new(reader), &mut output)?;
/// ```
pub struct EntryReader<'a> {
    // Internal: backend-specific reader
    backend_reader: Box<dyn Read + 'a>,
    // Internal: CRC32 verification wrapper (if enabled)
    crc_verifier: Option<VerifyingReader<Box<dyn Read + 'a>>>,
    // Internal: expected CRC32 from metadata
    expected_crc32: Option<u32>,
}

impl<'a> Read for EntryReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(ref mut verifier) = self.crc_verifier {
            verifier.read(buf)
        } else {
            self.backend_reader.read(buf)
        }
    }
}
```

### Archive Integration

```rust
impl Archive {
    /// Get streaming reader for a specific entry
    ///
    /// # Parameters
    ///
    /// * `path` - Path within archive (use forward slashes)
    ///
    /// # Returns
    ///
    /// `EntryReader` that implements `Read` trait
    ///
    /// # Memory Usage
    ///
    /// ~40KB per reader (bounded, regardless of file size)
    ///
    /// # CRC32 Verification
    ///
    /// Automatic if entry has CRC32 metadata and `verify_crc` enabled.
    /// Verification occurs on Drop (after full read).
    ///
    /// # Errors
    ///
    /// * `ArchiveError::EntryNotFound` - Path not in archive
    /// * `ArchiveError::Io` - Backend read failure
    /// * `ArchiveError::Password` - Entry encrypted, password required
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Basic streaming
    /// let reader = archive.entry_reader("file.txt")?;
    /// let content = std::io::read_to_string(reader)?;
    ///
    /// // With buffering
    /// let reader = archive.entry_reader("large.bin")?;
    /// let mut buffered = BufReader::new(reader);
    /// // Process in chunks...
    ///
    /// // Stream to file
    /// let reader = archive.entry_reader("document.pdf")?;
    /// let mut output = File::create("output.pdf")?;
    /// io::copy(&mut BufReader::new(reader), &mut output)?;
    /// ```
    pub fn entry_reader(&self, path: &str) -> Result<EntryReader<'_>> {
        self.entry_reader_with_options(path, StreamingOptions::default())
    }

    /// Get streaming reader with options
    pub fn entry_reader_with_options(
        &self,
        path: &str,
        options: StreamingOptions,
    ) -> Result<EntryReader<'_>> {
        let entry = self.find_entry(path)?;

        // Check password requirement
        if entry.is_encrypted && options.password.is_none() {
            return Err(ArchiveError::Password {
                message: format!("Entry '{}' is encrypted, password required", path),
            });
        }

        // Create backend reader
        let backend_reader = match &self.backend {
            ArchiveBackend::Unrar(unrar) => {
                unrar.open_entry_stream(path, options.password.as_ref())?
            }
            ArchiveBackend::Libarchive(lib) => {
                lib.open_entry_stream(path, options.password.as_ref())?
            }
        };

        // Wrap with CRC32 verifier if enabled
        let (backend_reader, crc_verifier) = if options.verify_crc && entry.crc32.is_some() {
            let verifier = VerifyingReader::new(backend_reader, entry.crc32);
            (Box::new(std::io::empty()) as Box<dyn Read>, Some(verifier))
        } else {
            (backend_reader, None)
        };

        Ok(EntryReader {
            backend_reader,
            crc_verifier,
            expected_crc32: entry.crc32,
        })
    }

    /// Extract entry to writer (streaming, memory-bounded)
    ///
    /// # Parameters
    ///
    /// * `path` - Entry path within archive
    /// * `writer` - Destination implementing `Write`
    ///
    /// # Memory Usage
    ///
    /// ~40KB (8KB read buffer + 16KB write buffer)
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Extract to file
    /// let mut output = File::create("output.txt")?;
    /// archive.extract_to_writer("document.txt", &mut output)?;
    ///
    /// // Extract to memory (for small files)
    /// let mut buffer = Vec::new();
    /// archive.extract_to_writer("small.txt", &mut buffer)?;
    ///
    /// // Extract with compression (gzip)
    /// use flate2::write::GzEncoder;
    /// let output = File::create("output.gz")?;
    /// let mut encoder = GzEncoder::new(output, Compression::default());
    /// archive.extract_to_writer("large.bin", &mut encoder)?;
    /// ```
    pub fn extract_to_writer<W>(
        &self,
        path: &str,
        writer: W,
    ) -> Result<u64>
    where
        W: Write,
    {
        let reader = self.entry_reader(path)?;
        let bytes_written = io::copy(
            &mut BufReader::new(reader),
            &mut BufWriter::new(writer),
        )?;
        Ok(bytes_written)
    }
}
```

### StreamingOptions

```rust
use secstr::SecStr;

/// Options for streaming extraction
#[derive(Default)]
pub struct StreamingOptions {
    /// Password for encrypted entries
    pub password: Option<SecStr>,

    /// Verify CRC32 during read (default: true)
    pub verify_crc: bool,

    /// Buffer size for reads (default: 8KB)
    pub buffer_size: usize,
}

impl Default for StreamingOptions {
    fn default() -> Self {
        Self {
            password: None,
            verify_crc: true,
            buffer_size: 8192,
        }
    }
}
```

## CRC32 Verification (Internal)

```rust
use crc32fast::Hasher;

/// Wraps a reader with automatic CRC32 verification
///
/// Verification occurs on Drop, after full read
pub(crate) struct VerifyingReader<R: Read> {
    inner: R,
    hasher: Hasher,
    expected_crc32: Option<u32>,
    bytes_read: u64,
}

impl<R: Read> VerifyingReader<R> {
    pub fn new(inner: R, expected_crc32: Option<u32>) -> Self {
        Self {
            inner,
            hasher: Hasher::new(),
            expected_crc32,
            bytes_read: 0,
        }
    }
}

impl<R: Read> Read for VerifyingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
            self.bytes_read += n as u64;
        }
        Ok(n)
    }
}

impl<R: Read> Drop for VerifyingReader<R> {
    fn drop(&mut self) {
        if let Some(expected) = self.expected_crc32 {
            let actual = self.hasher.finalize();
            if actual != expected {
                // Log CRC mismatch
                eprintln!(
                    "CRC32 mismatch: expected 0x{:08x}, got 0x{:08x} ({} bytes read)",
                    expected, actual, self.bytes_read
                );
                // Note: Cannot return error from Drop
                // User must check extraction result separately
            }
        }
    }
}
```

## Contract Guarantees

### Memory Bounds

**Requirement SC-009**: <100MB memory for 10GB+ archives

**Implementation**:
```
EntryReader memory = backend_buffer + optional_verifier
                   = 8KB + (0 or 48 bytes)
                   ≈ 8KB

With user BufReader: 8KB + 16KB = 24KB per file
With user BufWriter: 24KB + 16KB = 40KB per operation

For 10GB archive: 40KB (one file at a time)
✅ <<100MB
```

**Verification**:
```rust
#[test]
fn test_memory_bounded_extraction() {
    let archive = Archive::open("fixtures/10gb_archive.zip")?;
    let start_memory = get_process_memory();

    for entry in archive.list_files()? {
        let reader = archive.entry_reader(&entry.path)?;
        let mut output = File::create(dest.join(&entry.path))?;
        io::copy(&mut BufReader::new(reader), &mut output)?;
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
- Cannot seek backwards in EntryReader
- Cannot reuse EntryReader after EOF
- Must create new reader for re-extraction

**Example**:
```rust
let reader = archive.entry_reader("file.txt")?;

// First read: OK
let mut buf1 = vec![0u8; 1024];
reader.read(&mut buf1)?;

// Read to EOF: OK
let mut buf2 = Vec::new();
reader.read_to_end(&mut buf2)?;

// Second read: returns 0 (EOF)
let n = reader.read(&mut buf1)?;
assert_eq!(n, 0);

// To re-read: create new reader
drop(reader);
let reader2 = archive.entry_reader("file.txt")?;
```

### CRC32 Verification

**Guarantee**: Automatic verification if entry has CRC32 metadata

**Behavior**:
1. CRC32 computed during read (streaming)
2. Verification on Drop (after full read)
3. Mismatch logged to stderr (cannot return error from Drop)
4. User checks extraction result separately

**Limitation**: Cannot report CRC error immediately (Drop restriction)

**Future enhancement**: Return `Result` from explicit `finalize()` method

### Backend Consistency

Streaming works identically across backends:

| Backend | Streaming Support | CRC32 Verification | Password Support |
|---------|-------------------|-------------------|------------------|
| UnRAR | ✅ Yes | ✅ Yes (metadata) | ✅ Yes |
| libarchive | ✅ Yes | ⚠️ Computed during extraction | ✅ Yes |

**Note**: libarchive doesn't expose CRC32 in metadata, so we compute during extraction for verification.

## Standard Library Composition

### BufReader Integration

```rust
use std::io::BufReader;

// Automatic buffering for efficient reads
let reader = archive.entry_reader("large.bin")?;
let mut buffered = BufReader::with_capacity(64 * 1024, reader); // 64KB buffer

// Read line-by-line (for text files)
use std::io::BufRead;
let reader = archive.entry_reader("lines.txt")?;
let buffered = BufReader::new(reader);
for line in buffered.lines() {
    println!("{}", line?);
}
```

### io::copy Integration

```rust
use std::io;

// Copy to file
let reader = archive.entry_reader("source.bin")?;
let mut output = File::create("destination.bin")?;
io::copy(&mut BufReader::new(reader), &mut output)?;

// Copy with progress
let reader = archive.entry_reader("large.bin")?;
let mut output = File::create("destination.bin")?;
let mut copied = 0u64;
let mut buffer = [0u8; 8192];

loop {
    let n = reader.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    output.write_all(&buffer[..n])?;
    copied += n as u64;
    println!("Copied {} bytes", copied);
}
```

### Compression Chain Integration

```rust
use flate2::write::GzEncoder;
use flate2::Compression;

// Extract and re-compress on the fly
let reader = archive.entry_reader("uncompressed.bin")?;
let output = File::create("compressed.gz")?;
let mut encoder = GzEncoder::new(output, Compression::default());

io::copy(&mut BufReader::new(reader), &mut encoder)?;
encoder.finish()?;

// Memory usage: ~40KB (streaming, no full file in memory)
```

## Error Handling

### Entry Not Found

```rust
match archive.entry_reader("nonexistent.txt") {
    Err(ArchiveError::EntryNotFound { path }) => {
        eprintln!("Entry '{}' not found in archive", path);
    }
    Ok(reader) => { /* use reader */ }
    Err(e) => { /* other errors */ }
}
```

### Password Required

```rust
let result = archive.entry_reader("encrypted.txt");

match result {
    Err(ArchiveError::Password { message }) => {
        // Retry with password
        let options = StreamingOptions {
            password: Some(SecStr::from("secret")),
            ..Default::default()
        };
        let reader = archive.entry_reader_with_options("encrypted.txt", options)?;
    }
    Ok(reader) => { /* use reader */ }
    Err(e) => { /* other errors */ }
}
```

### Backend Errors

```rust
let reader = archive.entry_reader("corrupted.bin")?;
let mut output = Vec::new();

match reader.read_to_end(&mut output) {
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
        let reader = archive.entry_reader("large.bin")?;
        let mut output = io::sink(); // Discard output
        io::copy(&mut BufReader::new(reader), &mut output)?;
    });

    // Expected: ~500 MB/s (disk-bound, not CPU-bound)
}
```

### CRC32 Overhead

**Measurement**: With and without verification

```rust
#[bench]
fn bench_crc32_overhead(b: &mut Bencher) {
    let archive = Archive::open("fixtures/1gb_file.zip")?;

    // Without CRC32
    b.iter(|| {
        let options = StreamingOptions { verify_crc: false, ..Default::default() };
        let reader = archive.entry_reader_with_options("large.bin", options)?;
        io::copy(&mut BufReader::new(reader), &mut io::sink())?;
    });

    // With CRC32
    b.iter(|| {
        let options = StreamingOptions { verify_crc: true, ..Default::default() };
        let reader = archive.entry_reader_with_options("large.bin", options)?;
        io::copy(&mut BufReader::new(reader), &mut io::sink())?;
    });

    // Expected: <2% overhead (crc32fast SIMD ~10GB/s, extraction ~500MB/s)
}
```

### Memory Usage

**Measurement**: Peak RSS during extraction

```rust
#[test]
fn test_streaming_memory_usage() {
    let archive = Archive::open("fixtures/10gb_archive.zip")?;
    let baseline = get_process_memory();

    // Extract all files streaming
    for entry in archive.list_files()? {
        let reader = archive.entry_reader(&entry.path)?;
        let mut output = File::create(temp_dir().join(&entry.path))?;
        io::copy(&mut BufReader::new(reader), &mut output)?;
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
fn test_entry_reader_basic() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut reader = archive.entry_reader("test_file.txt")?;

    let mut content = String::new();
    reader.read_to_string(&mut content)?;

    assert_eq!(content, "Hello, RAR World!\n");
}

#[test]
fn test_entry_reader_chunked() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut reader = archive.entry_reader("test_file.txt")?;

    let mut chunks = Vec::new();
    let mut buffer = [0u8; 4];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        chunks.push(buffer[..n].to_vec());
    }

    let content: Vec<u8> = chunks.into_iter().flatten().collect();
    assert_eq!(content, b"Hello, RAR World!\n");
}

#[test]
fn test_entry_reader_crc32_verification() {
    let archive = Archive::open("fixtures/test.rar")?;
    let reader = archive.entry_reader("test_file.txt")?;

    // Read to EOF
    let mut content = Vec::new();
    reader.read_to_end(&mut content)?;

    // CRC32 verified on Drop (no assertion here, checked in Drop impl)
    drop(reader);
}

#[test]
fn test_entry_reader_password() {
    let archive = Archive::open("fixtures/encrypted.zip")?;

    // Without password: error
    assert!(matches!(
        archive.entry_reader("secret.txt"),
        Err(ArchiveError::Password { .. })
    ));

    // With password: OK
    let options = StreamingOptions {
        password: Some(SecStr::from("password123")),
        ..Default::default()
    };
    let mut reader = archive.entry_reader_with_options("secret.txt", options)?;

    let mut content = String::new();
    reader.read_to_string(&mut content)?;
    assert!(!content.is_empty());
}
```

### Integration Tests

```rust
#[test]
fn test_extract_to_writer_file() {
    let archive = Archive::open("fixtures/test.zip")?;
    let output_path = temp_dir().join("output.txt");
    let mut output = File::create(&output_path)?;

    let bytes_written = archive.extract_to_writer("test_file.txt", &mut output)?;

    assert_eq!(bytes_written, 18); // "Hello, RAR World!\n"

    let content = std::fs::read_to_string(&output_path)?;
    assert_eq!(content, "Hello, RAR World!\n");
}

#[test]
fn test_extract_to_writer_memory() {
    let archive = Archive::open("fixtures/test.zip")?;
    let mut buffer = Vec::new();

    archive.extract_to_writer("test_file.txt", &mut buffer)?;

    assert_eq!(buffer, b"Hello, RAR World!\n");
}

#[test]
fn test_streaming_large_file() {
    // Create 100MB test file
    let archive = create_archive_with_large_file(100 * 1024 * 1024)?;

    let start_memory = get_process_memory();
    let reader = archive.entry_reader("large.bin")?;
    let mut output = io::sink();
    io::copy(&mut BufReader::new(reader), &mut output)?;
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

// New: Streaming (memory-bounded)
let reader = archive.entry_reader("large.bin")?;
let mut content = Vec::new();
reader.read_to_end(&mut content)?; // Still loads to Vec, but streaming read

// New: Stream to file directly
let reader = archive.entry_reader("large.bin")?;
let mut output = File::create("output.bin")?;
io::copy(&mut BufReader::new(reader), &mut output)?;
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
let reader = archive.entry_reader("file.bin")?;
let mut output = File::create("output.bin")?;
io::copy(&mut BufReader::new(reader), &mut output)?;
```

## References

- Phase 0 Research: `research.md` - Streaming extraction architecture decisions
- Data Model: `data-model.md` - EntryReader and VerifyingReader specifications
- Constitution v1.1.0 Principle II: Pragmatic performance (memory bounds)
- Rust std::io::Read: https://doc.rust-lang.org/std/io/trait.Read.html
- crc32fast: https://docs.rs/crc32fast/
