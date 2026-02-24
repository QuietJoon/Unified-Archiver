# Technology Research: unified-archive

**Date**: 2025-10-30
**Feature**: 001-unified-archive
**Purpose**: Resolve technical unknowns from Technical Context and make informed technology decisions

## Research Questions

From Technical Context, we need to resolve:

1. FFI binding strategy (which native library)
2. C/C++ interop layer approach
3. Compression codec dependencies
4. Property-based testing framework
5. Performance regression testing tools
6. License compatibility verification

## Decision 1: FFI Binding Strategy & Native Library Choice

### Options Evaluated

**Option A: p7zip (POSIX port of 7-Zip)**
- **Pros**: Mature, widely available on Linux/macOS, supports many formats
- **Cons**: C++ codebase, complex API, limited Windows support, maintenance concerns
- **License**: LGPL + unRAR restriction
- **Format support**: Excellent (7z, ZIP, TAR, GZIP, BZIP2, XZ, RAR read-only, ISO)

**Option B: libarchive**
- **Pros**: C library, well-maintained, cross-platform, clean API
- **Cons**: No RAR5 write support, different architecture than 7zip
- **License**: BSD-2-Clause (permissive)
- **Format support**: Good but lacks full RAR5 support

**Option C: 7-Zip official libraries (LZMA SDK + 7z.dll/7z.so)**
- **Pros**: Official implementation, best format compatibility, actively maintained
- **Cons**: Windows-centric, requires careful cross-platform build setup
- **License**: Public domain (LZMA SDK) + LGPL (7z library)
- **Format support**: Comprehensive (7z, ZIP, GZIP, BZIP2, XZ, TAR, RAR read)

**Option D: Reference projects approach**
- Reference compress-tools (uses libarchive)
- Reference archive-reader (pure Rust, limited formats)
- Reference sevenzipjbinding native libraries

### Decision

**Selected: Hybrid approach - libarchive + unrar library**

**Rationale**:
1. **libarchive** provides excellent cross-platform support for most formats (7z, ZIP, TAR, GZIP, BZIP2, XZ, ISO)
2. **unrar library** (from rarlab) provides RAR/RAR5 read support with official implementation
3. **License compatibility**: BSD-2-Clause (libarchive) + unRAR license (free for non-commercial, commercial requires license)
4. **Proven approach**: compress-tools reference shows libarchive works well in Rust ecosystem
5. **Maintainability**: Well-documented C APIs, active communities

**Implementation strategy**:
- Use libarchive for: 7z, ZIP, TAR variants (TAR.GZ, TAR.BZ2, TAR.XZ), GZIP, BZIP2, XZ, ISO
- Use unrar library for: RAR, RAR5 (read-only per FR-005, creation not required)
- Unified Rust wrapper provides format-agnostic interface

**Alternatives considered and rejected**:
- **Pure Rust implementations** (zip-rs, tar-rs, flate2): Fragmented ecosystem, no unified interface, lacks RAR support
- **p7zip only**: Cross-platform build complexity, C++ interop challenges, uncertain maintenance status
- **7-Zip official only**: Windows-centric, complex Linux/macOS build setup

### Supporting Evidence

From reference projects at `/Volumes/Common/QJoon/unified-archive/`:
- **compress-tools**: Successfully uses libarchive with Rust FFI
- **sevenzipjbinding**: Uses custom JNI bindings to 7z library (complex)
- **archive-reader**: Pure Rust but limited format support

## Decision 2: C/C++ Interop Layer

### Options Evaluated

**Option A: bindgen (auto-generate bindings)**
- **Pros**: Automated, maintains sync with C headers
- **Cons**: Generates verbose code, requires clang, build complexity

**Option B: cxx (C++ interop)**
- **Pros**: Type-safe, good for C++
- **Cons**: Not needed (libarchive is C, unrar has C API)

**Option C: Manual bindings**
- **Pros**: Full control, minimal dependencies, clean API surface
- **Cons**: Maintenance burden, requires updates when C API changes

### Decision

**Selected: bindgen for libarchive + manual bindings for unrar**

**Rationale**:
1. libarchive has large C API surface → bindgen automation saves effort
2. unrar has smaller, stable C API → manual bindings for fine control
3. Wrap both in safe Rust abstractions (`wrapper.rs` per structure)
4. compress-tools reference shows bindgen works well for libarchive

**Implementation approach**:
```rust
// build.rs: Generate libarchive bindings
bindgen::Builder::default()
    .header("wrapper.h")
    .allowlist_function("archive_.*")
    .generate()

// src/ffi/libarchive_wrapper.rs: Safe Rust wrappers
pub struct Archive {
    handle: *mut ffi::archive,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        // Safe wrapper around unsafe FFI calls
    }
}
```

**Alternatives rejected**:
- **cxx**: Not applicable (no C++ for libarchive/unrar C APIs)
- **All manual**: Too much maintenance burden for large libarchive API

## Decision 3: Compression Codec Dependencies

### Analysis

**libarchive dependencies** (handled by system package managers):
- lzma/xz-utils (XZ format)
- zlib (GZIP format)
- bzip2 (BZIP2 format)
- Built-in support for ZIP, TAR, 7z

**unrar dependencies**:
- Self-contained (no external codec dependencies)

### Decision

**Selected: System-provided codecs via pkg-config**

**Rationale**:
1. Use system libarchive installation on Linux/macOS (via pkg-config)
2. Bundle or statically link on Windows (via vcpkg or source build)
3. unrar built from source (small, self-contained)
4. Documented in build requirements (README.md)

**Build approach**:
```toml
# Cargo.toml
[build-dependencies]
bindgen = "0.69"
pkg-config = "0.3"
cc = "1.0"

[package.metadata.docs.rs]
# Document build requirements
```

**Alternatives rejected**:
- **Bundle all codecs**: Increases binary size unnecessarily
- **Pure Rust codecs**: Fragmented, incomplete format support

## Decision 4: Property-Based Testing Framework

### Options Evaluated

**Option A: proptest**
- **Pros**: Mature, shrinking support, good ergonomics
- **Cons**: Slower test execution

**Option B: quickcheck**
- **Pros**: Lightweight, fast
- **Cons**: Less sophisticated shrinking

### Decision

**Selected: proptest**

**Rationale**:
1. Better shrinking for debugging failures (critical for FFI code)
2. More active development and ecosystem support
3. Good integration with cargo test
4. Used successfully in Rust FFI projects

**Use cases**:
- Round-trip testing (create → extract → verify)
- Invariant testing (same data across different archive formats)
- Edge case generation (Unicode filenames, large files, corrupted data)

**Example**:
```rust
proptest! {
    #[test]
    fn roundtrip_preserves_data(data: Vec<u8>, filename: String) {
        let archive = Archive::create("test.zip")?;
        archive.add_file(&filename, &data)?;
        archive.close()?;

        let extracted = Archive::open("test.zip")?
            .extract_file(&filename)?;

        prop_assert_eq!(data, extracted);
    }
}
```

**Alternatives rejected**:
- **quickcheck**: Less powerful shrinking for complex FFI scenarios

## Decision 5: Performance Regression Testing

### Options Evaluated

**Option A: criterion**
- **Pros**: Statistical analysis, warmup support, HTML reports
- **Cons**: Slower benchmarks

**Option B: iai (Cachegrind-based)**
- **Pros**: Deterministic, measures instructions (not time)
- **Cons**: Requires valgrind, Linux-only

### Decision

**Selected: criterion for primary benchmarks + custom scripts for CI**

**Rationale**:
1. criterion provides statistical rigor for performance measurement (SC-010 requirement)
2. Custom CI scripts track performance over time (criterion-compare)
3. Benchmark suites for:
   - Archive inspection speed (10k files target)
   - Extraction throughput (10GB archive target)
   - Memory usage profiling (100MB limit validation)
4. Meets constitution Principle II (Performance First) measurement requirement

**Benchmark structure**:
```rust
// benches/inspection.rs
fn bench_inspection(c: &mut Criterion) {
    c.bench_function("inspect_10k_files", |b| {
        b.iter(|| {
            let archive = Archive::open("fixtures/10k_files.zip").unwrap();
            archive.list_files().unwrap()
        });
    });
}
```

**CI integration**:
```yaml
# .github/workflows/bench.yml
- run: cargo bench --bench inspection -- --save-baseline main
- run: cargo bench --bench inspection -- --baseline main
```

**Alternatives rejected**:
- **iai only**: Linux-only limits cross-platform validation
- **Manual timing**: Lacks statistical rigor required by constitution

## Decision 6: License Compatibility

### Analysis

**Dependencies and their licenses**:

| Dependency | License | Commercial Use | Attribution Required | Copyleft |
|-----------|---------|----------------|---------------------|----------|
| libarchive | BSD-2-Clause | ✅ Yes | ✅ Yes | ❌ No |
| unrar | unRAR license | ⚠️ Conditional | ✅ Yes | ❌ No |
| Rust crate | (User decides) | - | - | - |

**unRAR license key terms**:
- Free for non-commercial use
- **Commercial use requires license from RARLAB**
- Cannot create RAR archives (only extract) - aligns with our spec (FR-005 read-only)
- Source code available but with restrictions

### Decision

**Selected: Dual licensing strategy with clear documentation**

**Approach**:
1. **Crate license**: Apache-2.0 OR MIT (permissive, standard Rust practice)
2. **Dependency notice**: Document unRAR license implications in README
3. **Conditional compilation**: Make unRAR support optional via feature flag

**Cargo.toml**:
```toml
[features]
default = ["rar-support"]
rar-support = []  # Includes unrar library (requires license compliance)
```

**README.md documentation**:
```markdown
## License Dependencies

- libarchive: BSD-2-Clause (permissive, commercial use allowed)
- unrar (optional): UnRAR license
  - ⚠️ **Commercial use requires license from RARLAB**
  - Non-commercial use is free
  - Only extraction supported (creation not allowed by license)
  - Can be disabled with `--no-default-features`
```

**Justification**:
1. Meets Principle III (Minimal Dependencies) - licenses documented and justified
2. Provides flexibility for commercial users (disable RAR if license not obtained)
3. Maintains format compatibility for non-commercial users
4. Aligns with spec requirement FR-007 (RAR/RAR5 support for complete compatibility)

**Compliance checklist**:
- ✅ libarchive attribution in NOTICE file
- ✅ unrar license terms documented in README
- ✅ Feature flag for optional RAR support
- ✅ Clear guidance for commercial users

**Alternatives rejected**:
- **RAR exclusion**: Breaks spec requirement FR-007 and 7zip-JBinding compatibility goal
- **Different RAR library**: No permissive alternatives with RAR5 support exist

## Summary of Decisions

| Question | Decision | Key Rationale |
|---------|----------|---------------|
| Native library | libarchive + unrar hybrid | Best cross-platform support, proven in compress-tools |
| C/C++ interop | bindgen + manual bindings | Automation for large API, control for small API |
| Codec dependencies | System-provided via pkg-config | Reduces binary size, leverages system packages |
| Property testing | proptest | Superior shrinking for FFI debugging |
| Performance testing | criterion + CI tracking | Statistical rigor meets constitution requirements |
| Licensing | Dual license with feature flag | Flexibility for commercial users, clear compliance path |

## Implementation Impact

**Technical Context updates** (resolved NEEDS CLARIFICATION):
- **Primary Dependencies**: libarchive (BSD-2-Clause), unrar (conditional), bindgen (build-only)
- **C/C++ interop layer**: bindgen for libarchive, manual for unrar
- **Compression codecs**: System-provided (lzma, zlib, bzip2) via pkg-config
- **Property testing**: proptest for invariant testing
- **Performance testing**: criterion with statistical analysis and CI tracking

**Constitution Check updates**:
- ✅ Principle III (Dependencies): All dependencies justified, licensed, with mitigation strategy
- ✅ Principle II (Performance): criterion provides measurement framework
- ✅ Principle IV (Testing): proptest enables comprehensive property-based testing

**Next phase** (Phase 1 - Design):
- data-model.md: Define Archive, ArchiveEntry, error types
- contracts/: Define API signatures based on 7zip-JBinding patterns
- quickstart.md: User guide with example code

## Phase 0: Enhancement Features Research

**Date**: 2025-10-31
**Purpose**: Research technical approaches for metadata enrichment and extraction enhancements

### Research 7: Entry Caching Strategy

**Problem**: UnRAR requires sequential iteration; reopening archive for multiple operations is inefficient

**Options Evaluated**:

**Option A: OnceCell/OnceLock**
- **Pros**:
  - No locking overhead after initialization (`get()` returns `&T` not `MutexGuard`)
  - Never poisoned on panic (unlike Mutex)
  - Single-assignment guarantee simplifies reasoning
  - Stabilized in std::sync (no external dependency)
- **Cons**:
  - Cannot update once set (must use mutable reference)
  - Single-threaded OnceCell not Sync
- **Thread-safety**: Use `OnceLock<Vec<ArchiveEntry>>` for thread-safe caching
- **Memory overhead**: Single Vec allocation (~48 bytes + entries)

**Option B: Mutex<Option<Vec<ArchiveEntry>>>**
- **Pros**:
  - Can update cache (mutable after lock)
  - Familiar pattern
- **Cons**:
  - Locking overhead on every access
  - Poisoning on panic requires handling
  - More complex API (MutexGuard unwrapping)
- **Thread-safety**: Built-in
- **Memory overhead**: Vec + Mutex + Option (~64 bytes + entries)

**Option C: Memory-mapped central directory (rawzip pattern)**
- **Pros**:
  - Zero allocations for entry iteration
  - 2 orders of magnitude faster for 200k+ entries
- **Cons**:
  - Only works for formats with central directory (ZIP, not RAR/TAR)
  - Requires lending iterator (complex API)
  - Platform-specific (64-bit only)

### Decision

**Selected: OnceLock<Vec<ArchiveEntry>> for unified caching**

**Rationale**:
1. **Performance**: No locking overhead after first access (critical path)
2. **Simplicity**: Single-assignment matches our read-only use case
3. **Safety**: Never poisoned, clear ownership semantics
4. **Constitution compliance**: Meets Principle II (pragmatic performance - simple change, significant gain)
5. **API ergonomics**: Returns `&[ArchiveEntry]` directly, no guard unwrapping

**Implementation approach**:
```rust
pub struct Archive {
    backend: ArchiveBackend,
    entry_cache: OnceLock<Vec<ArchiveEntry>>,
}

impl Archive {
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.entry_cache.get_or_try_init(|| {
            match &self.backend {
                ArchiveBackend::Unrar(unrar) => unrar.list_files(),
                ArchiveBackend::Libarchive(lib) => lib.list_files(),
            }
        }).map(|v| v.as_slice())
    }
}
```

**Alternatives rejected**:
- **Mutex**: Unnecessary overhead for read-only caching
- **Rawzip pattern**: Format-specific, breaks unified interface

### Research 8: Progress Callback Design

**Problem**: Need ≥10 updates/sec without overhead, support cancellation

**Options Evaluated**:

**Option A: Trait-based callbacks**
- **Pros**:
  - Zero-cost abstraction via monomorphization
  - Static dispatch (no vtable overhead)
  - Type-safe, compiler-optimized
- **Cons**:
  - Generic parameter on extraction methods
  - Cannot store in struct (different types)
- **Pattern**: `impl Trait` or generic `<P: ProgressCallback>`

**Option B: Closure callbacks (FnMut)**
- **Pros**:
  - Ergonomic API
  - Can capture environment
  - Standard Rust pattern
- **Cons**:
  - Requires generic or Box<dyn FnMut> (heap allocation)
  - Slightly more complex lifetimes

**Option C: Channel-based progress**
- **Pros**:
  - Natural backpressure
  - Decouples producer/consumer
- **Cons**:
  - Allocation overhead per message
  - Latency from channel operations

### Decision

**Selected: Trait-based with Option parameter for zero-cost abstraction**

**Rationale**:
1. **Zero-cost**: Static dispatch via monomorphization (no vtable, no Box)
2. **Cancellation**: Return `ControlFlow` from callback (Continue/Break)
3. **Ergonomics**: Option wrapping allows omitting callback
4. **Constitution**: Meets Principle II (pragmatic performance)

**Implementation**:
```rust
pub trait ProgressCallback {
    fn on_progress(&mut self, current: u64, total: u64) -> ControlFlow<()>;
}

impl Archive {
    pub fn extract_all<P>(&self, dest: &Path, progress: Option<&mut P>) -> Result<()>
    where
        P: ProgressCallback,
    {
        // Rate limiting: only call every N bytes or M milliseconds
        if let Some(callback) = progress {
            if callback.on_progress(bytes_done, total_bytes).is_break() {
                return Err(ArchiveError::Cancelled);
            }
        }
    }
}
```

**Rate limiting strategy**:
- Call callback every 100KB or 100ms (whichever comes first)
- Ensures ≥10 updates/sec for typical extraction speeds

**Alternatives rejected**:
- **Closure-only**: Requires generic or Box (breaks zero-cost)
- **Channel-based**: Allocation overhead violates performance principle

### Research 9: Streaming Extraction Architecture

**Problem**: <100MB memory for 10GB+ archives

**Key patterns from research**:

**Option A: Read/Write trait composition**
- **Pattern**: `impl Read for ArchiveEntry` + standard `io::copy()`
- **Pros**:
  - Standard library support
  - Buffering built-in (BufReader/BufWriter)
  - Backpressure automatic (blocked write stalls read)
- **Cons**: Synchronous only

**Option B: Async streams (Tokio)**
- **Pattern**: `impl Stream<Item = Result<Bytes>>`
- **Pros**:
  - Natural for async runtimes
  - Backpressure via poll readiness
- **Cons**: Adds async dependency (violates Principle III)

**Option C: Iterator of chunks**
- **Pattern**: `impl Iterator<Item = Result<Vec<u8>>>`
- **Pros**: Simple, no I/O traits needed
- **Cons**: Manual buffering, no standard composition

### Decision

**Selected: Read trait with bounded buffering**

**Rationale**:
1. **Standard**: Uses std::io traits (no external dependencies)
2. **Memory bounded**: BufReader(16KB) + extraction buffer(8KB) = ~24KB per file
3. **Backpressure**: Automatic via blocked writes
4. **Composition**: Works with io::copy, compression decoders, etc.
5. **Constitution**: Meets Principle III (minimal dependencies)

**Implementation**:
```rust
pub struct EntryReader<'a> {
    backend_reader: Box<dyn Read + 'a>,
    buffer: [u8; 8192],
}

impl<'a> Read for EntryReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.backend_reader.read(buf)
    }
}

impl Archive {
    pub fn extract_file_streaming<W>(&self, path: &str, writer: W) -> Result<()>
    where
        W: Write,
    {
        let reader = self.get_entry_reader(path)?;
        io::copy(&mut BufReader::new(reader), &mut BufWriter::new(writer))?;
        Ok(())
    }
}
```

**Memory analysis**:
- Entry metadata: ~200 bytes
- BufReader: 16KB
- BufWriter: 16KB
- Working buffer: 8KB
- **Total per file**: ~40KB
- **For 10GB archive with 10k files**: 40KB (one at a time, not 10k * 40KB)

**Alternatives rejected**:
- **Async streams**: Unnecessary dependency
- **Iterator chunks**: Reinvents wheel, no std composition

### Research 10: CRC32 Verification During Extraction

**Problem**: Currently only check metadata presence, need streaming computation

**CRC32 Crate Evaluation**:

**Option A: crc-fast**
- **Performance**: >100GiB/s (SIMD), 220X faster than crc crate
- **Features**: Streaming digest, CRC-32/CRC-64, custom parameters
- **License**: MIT/Apache-2.0
- **Usage**: `Digest::new()` + `update()` + `finalize()`

**Option B: crc32fast**
- **Performance**: SIMD-accelerated (SSE, PCLMULQDQ, AArch64 CRC32)
- **Features**: Incremental updates via `Hasher`
- **License**: MIT/Apache-2.0
- **Maturity**: Well-established, used in zip crates

**Option C: crc32c**
- **Note**: CRC32-C variant (Castagnoli), not CRC32-IEEE used in archives

### Decision

**Selected: crc32fast for verification layer**

**Rationale**:
1. **Performance**: SIMD acceleration (meets Principle II)
2. **Correctness**: CRC32-IEEE variant used by ZIP/RAR/7z
3. **Maturity**: Battle-tested in Rust ecosystem
4. **Integration**: `Hasher::update()` matches streaming pattern
5. **Small dependency**: Single crate, permissive license

**Implementation**:
```rust
use crc32fast::Hasher;

pub struct VerifyingReader<R: Read> {
    inner: R,
    hasher: Hasher,
    expected_crc32: Option<u32>,
}

impl<R: Read> Read for VerifyingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

impl<R: Read> Drop for VerifyingReader<R> {
    fn drop(&mut self) {
        if let Some(expected) = self.expected_crc32 {
            let actual = self.hasher.finalize();
            if actual != expected {
                // Log or report CRC mismatch
            }
        }
    }
}
```

**Performance impact**:
- SIMD CRC32: ~10GB/s on modern CPU
- Extraction I/O bound: ~500MB/s (disk/decompression)
- **Overhead**: <2% (within pragmatic threshold per constitution)

**Alternatives rejected**:
- **crc-fast**: Newer, less proven in ecosystem
- **Manual CRC32**: Violates "don't reinvent" principle

### Research 11: Parallel Extraction Strategy

**Problem**: Extract 1000+ files efficiently on multi-core

**Rayon Evaluation**:

**Key findings**:
- **Work stealing**: Automatic load balancing across cores
- **Par_iter**: `files.par_iter().for_each(|file| extract(file))`
- **Memory**: Thread-local data prevents allocation contention
- **I/O limitation**: Disk I/O often bottleneck, not CPU
- **Default pool**: RAYON_NUM_THREADS or num_cpus::get()

**Trade-offs**:
- **File-level parallelism**: Extract multiple files concurrently
  - Pros: Simple, works with any archive format
  - Cons: Disk seeks if files scattered in archive
- **Chunk-level parallelism**: Split single file across threads
  - Pros: Better for large files
  - Cons: Format-dependent, complex coordination

### Decision

**Selected: Rayon file-level parallelism with conditional enable**

**Rationale**:
1. **Simplicity**: Single `.par_iter()` change (pragmatic per Principle II)
2. **CPU utilization**: Saturates cores for CPU-bound decompression
3. **Memory scaling**: Rayon manages thread pool, no manual allocation
4. **Conditional**: Only enable for >10 files (avoid overhead for small archives)
5. **Constitution**: Large change (Rayon dependency) justified by 10%+ gain on multi-file extraction

**Implementation**:
```rust
impl Archive {
    pub fn extract_all(&self, dest: &Path) -> Result<()> {
        let entries = self.list_files()?;

        if entries.len() > 10 {
            // Parallel extraction
            entries.par_iter().try_for_each(|entry| {
                self.extract_file(&entry.path, dest.join(&entry.path))
            })?;
        } else {
            // Sequential extraction (avoid thread overhead)
            for entry in entries {
                self.extract_file(&entry.path, dest.join(&entry.path))?;
            }
        }

        Ok(())
    }
}
```

**Performance expectations**:
- **1 file**: No change (sequential)
- **10 files, 4 cores**: ~3-3.5X speedup (CPU-bound decompression)
- **100 files, disk-bound**: ~1.5-2X (I/O overlap)

**Alternatives rejected**:
- **Chunk-level**: Too complex, format-specific
- **Manual threadpool**: Rayon work-stealing superior

### Research 12: Password-Protected Archive Handling

**Problem**: Unified password API across UnRAR and libarchive

**Security crate evaluation**:

**Option A: secstr**
- **Features**:
  - Constant-time comparison
  - Auto-zeroing on drop (uses zeroize)
  - mlock support (prevents swapping to disk)
- **License**: MIT
- **Usage**: `SecStr::from()` + `unsecure()` to access

**Option B: zeroize directly**
- **Features**:
  - Memory zeroing on drop
  - Zeroize trait for custom types
- **License**: Apache-2.0/MIT
- **Usage**: `#[zeroize(drop)]` attribute

**Option C: Plain String with manual zeroing**
- **Risk**: Relies on manual cleanup, error-prone

### Decision

**Selected: secstr for password storage with FFI considerations**

**Rationale**:
1. **Security**: Auto-zeroing + mlock prevents memory dump exposure
2. **FFI-safe**: Convert to CString only during FFI call, immediately drop
3. **Constitution**: Small dependency, critical security benefit
4. **Unified API**: Same SecStr type for both backends

**Implementation**:
```rust
use secstr::SecStr;

pub struct ExtractionOptions {
    pub password: Option<SecStr>,
}

impl Archive {
    pub fn extract_all(&self, dest: &Path, options: ExtractionOptions) -> Result<()> {
        match &self.backend {
            ArchiveBackend::Unrar(unrar) => {
                if let Some(password) = &options.password {
                    let c_password = CString::new(password.unsecure())?;
                    unrar.set_password(c_password.as_ptr());
                    // c_password dropped here, zeroed
                }
                unrar.extract_all(dest)
            }
            ArchiveBackend::Libarchive(lib) => {
                // Similar pattern
            }
        }
    }
}
```

**Password detection**:
- UnRAR: `archive_read_has_encrypted_entries()` FFI call
- libarchive: `archive_entry_is_encrypted()` per entry
- Expose as `ArchiveEntry::is_encrypted` field

**Alternatives rejected**:
- **Plain String**: Memory dump vulnerability
- **zeroize only**: No mlock, manual zeroing error-prone

### Research 13: Multi-Part Archive Support

**Problem**: RAR volumes (.part01.rar, .r00, etc.) and ZIP split archives

**Backend capabilities**:

**UnRAR multi-part**:
- **Support**: Native support for .partXX.rar and .rXX formats
- **API**: `archive.as_first_part()` method identifies first part
- **Detection**: Automatic volume chaining
- **Limitation**: Must provide first part path

**libarchive multi-part**:
- **Support**: ❌ Explicitly not supported per documentation
- **bsdtar**: "multivolume archives are not supported (whatever the format)"
- **ZIP splits**: No native support

**ZIP split handling**:
- **Option A**: Reassemble before extraction (temp file)
- **Option B**: Reject split ZIPs with clear error
- **Precedent**: Most Rust ZIP libraries don't support splits

### Decision

**Selected: UnRAR multi-part support, reject split ZIP with documentation**

**Rationale**:
1. **RAR volumes**: Free support via UnRAR (no extra work)
2. **ZIP splits**: Rare format, reassembly complex, not supported by libarchive
3. **Constitution**: Principle II (pragmatic) - RAR support is free, ZIP split requires large effort for marginal use case
4. **Documentation**: Clearly document limitation in README and error message

**Implementation**:
```rust
impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let format = detect_format(&path)?;

        match format {
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                let mut unrar = UnrarArchive::open(&path)?;

                // Check if multi-part and ensure first part
                if unrar.is_multipart() && !unrar.is_first_part() {
                    return Err(ArchiveError::InvalidInput(
                        "Multi-part RAR: please open the first part (.part01.rar or .rar)".into()
                    ));
                }

                Ok(Archive {
                    backend: ArchiveBackend::Unrar(unrar),
                    entry_cache: OnceLock::new(),
                })
            }
            ArchiveFormat::Zip => {
                let lib = LibarchiveArchive::open(&path)?;

                // Detect split ZIP (ends with .z01, .z02, etc.)
                if path.as_ref().extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with('z') && s[1..].parse::<u32>().is_ok())
                    .unwrap_or(false)
                {
                    return Err(ArchiveError::UnsupportedFormat(
                        "Split ZIP archives are not supported. Please use a tool to merge parts first.".into()
                    ));
                }

                Ok(Archive {
                    backend: ArchiveBackend::Libarchive(lib),
                    entry_cache: OnceLock::new(),
                })
            }
            // Other formats...
        }
    }
}
```

**Documentation**:
```markdown
## Multi-Part Archive Support

✅ **RAR Volumes** (.part01.rar, .r00, .r01, etc.)
- Fully supported via UnRAR library
- Open the first part; subsequent parts loaded automatically
- Both naming conventions supported

❌ **Split ZIP** (.zip, .z01, .z02, etc.)
- Not supported (limitation of libarchive)
- Use zip utilities to merge parts before extraction
- Error message provides guidance
```

**Alternatives rejected**:
- **ZIP reassembly**: Large effort for rare format (violates pragmatic principle)
- **Reject all multi-part**: Loses free RAR volume support

## Summary of Enhancement Research Decisions

| Topic | Decision | Key Rationale | Dependency Impact |
|-------|----------|---------------|-------------------|
| Entry caching | OnceLock<Vec> | Zero-cost after init, no locking | None (std) |
| Progress callbacks | Trait with ControlFlow | Zero-cost via monomorphization | None (std) |
| Streaming extraction | Read trait + buffering | Standard, <40KB memory | None (std) |
| CRC32 verification | crc32fast | SIMD, <2% overhead, battle-tested | +crc32fast |
| Parallel extraction | Rayon file-level | 10%+ gain, simple, conditional | +rayon |
| Password handling | secstr | Auto-zeroing, mlock, FFI-safe | +secstr |
| Multi-part support | RAR yes, ZIP no | RAR free, ZIP complex for rare case | None |

**Constitution Compliance**:
- ✅ **Principle II (Pragmatic Performance)**: All changes meet effort-to-benefit criteria
  - OnceLock: Small change, significant caching gain
  - Rayon: Large dependency justified by >10% multi-file speedup
  - CRC32: <2% overhead (acceptable per pragmatic threshold)
- ✅ **Principle III (Unified Interface + Minimal Deps)**: 3 new dependencies, all justified
  - crc32fast: Critical for integrity verification
  - rayon: Standard parallelism, significant performance gain
  - secstr: Critical security benefit
- ✅ **Principle I (Robustness)**: All patterns maintain error handling and safety

**Next Steps** (Phase 1 - Design):
- Update data-model.md with enhanced ArchiveEntry fields
- Create contracts/progress.md defining callback API
- Create contracts/streaming.md defining Read-based extraction
- Update contracts/extraction.md with password and multi-part semantics

## Performance Baseline Established

**Date**: 2025-10-31
**System**: macOS (Darwin 24.6.0), Rust 1.75+, Release mode
**Archives tested**: Small test fixtures (test.rar, test.zip, test.7z with ~1 file each)

### Current Performance (Release Build)

**Test Suite Execution**:
- 40 integration tests: **PASS** (100%)
- Total execution time: <0.03s
- Test breakdown:
  - Extraction tests: 9 tests, 0.01s
  - Format compatibility: 10 tests, 0.00s
  - Unified API: 6 tests, 0.00s
  - CRC32 verification: 4 tests, 0.00s
  - ZIP/7z operations: 11 tests, 0.01s

**Operations Measured** (small files, ~18 bytes):
- Open archive + format detection: <1ms
- List files (1 entry): <1ms
- Extract single file: <1ms
- Extract to memory: <1ms
- CRC32 validation: <1ms

### Baseline Limitations

⚠️ **Note**: Current baseline uses small test fixtures (18-byte files). Comprehensive baseline requires:

1. **Large file benchmark** (for 10k files target):
   - Create fixture: 10,000 files, ~1MB total
   - Measure: `list_files()` latency
   - Target: <1s (SC-008)

2. **Large archive benchmark** (for extraction throughput):
   - Create fixture: 1GB compressed archive
   - Measure: Extraction time vs native 7zip
   - Target: Within 20% of native speed (SC-010)

3. **Memory profiling** (for memory constraint):
   - Create fixture: 10GB+ archive
   - Measure: Peak RSS during extraction
   - Target: <100MB memory (SC-009)

4. **Backend comparison**:
   - UnRAR vs libarchive speed for equivalent formats
   - Identify performance characteristics per backend

### Action Items for Comprehensive Baseline

**Phase 1 Task**: Create performance test fixtures and benchmarks
- `benches/extraction_bench.rs` with criterion
- `tests/fixtures/large_archive.*` (10k files, 1GB, 10GB variants)
- CI integration for regression tracking

**Metrics to track**:
- Inspection latency (p50, p95, p99)
- Extraction throughput (MB/s)
- Memory usage (peak RSS)
- CPU utilization
- Backend performance delta

**Current Status**: ✅ Baseline established for correctness (40 tests pass)
**Next**: Establish performance baseline in Phase 1 with realistic workloads

## Phase 0: SFX Detection Research (2025-11-11)

### Research 14: Binary Format Parsing Library

**Problem**: Need to detect executable format (PE/ELF/Mach-O) for SFX detection

**Options Evaluated**:

**Option A: goblin crate**
- **Features**:
  - Cross-platform PE, ELF, Mach-O parsing
  - Fuzzed: 100 million runs, 1 million per seed
  - Zero-copy parsing, `#[repr(C)]` structs
  - std-free mode available
  - Rust 2024 compatible (requires 1.85.0)
- **License**: MIT/Apache-2.0
- **Maturity**: Well-established, widely used
- **Performance**: Tiny compile time, minimal overhead
- **API**: `Object::parse()` auto-detects format

**Option B: HexSpell**
- **Features**:
  - PE, ELF, Mach-O parsing and manipulation
  - Newer library (2024)
  - Focused on modification capabilities
- **Cons**: Less mature, smaller community

**Option C: LIEF with Rust bindings**
- **Features**: Comprehensive C++ library
- **Cons**: FFI complexity, heavier dependency

### Decision

**Selected: goblin crate (version 0.9+)**

**Rationale**:
1. **Battle-tested**: 100 million fuzzing runs ensures robustness
2. **Performance**: Zero-copy parsing meets <100ms SFX detection target
3. **Rust 2024 compatible**: Requires rustc 1.85.0, matches our minimum version
4. **Pure Rust**: No additional FFI complexity beyond existing UnRAR/libarchive
5. **Comprehensive**: Supports all required formats (PE/ELF/Mach-O) + BSD archives
6. **Active maintenance**: 2024 updates including TE support
7. **Constitution compliance**: Small dependency (<10ms compile time), proven stability

**Implementation**:
```rust
use goblin::Object;

fn detect_executable_format(bytes: &[u8]) -> Result<StubType> {
    match Object::parse(bytes)? {
        Object::PE(_) => Ok(StubType::WindowsPE),
        Object::Elf(_) => Ok(StubType::LinuxELF),
        Object::Mach(_) => Ok(StubType::MacOSMachO),
        _ => Err(ArchiveError::InvalidFormat("Not an executable"))
    }
}
```

**Dependency added**: `goblin = "0.9"`

**Alternatives rejected**:
- HexSpell: Too new, unproven in production
- LIEF: Additional FFI layer unnecessarily complex

---

### Research 15: SFX Detection Algorithm

**Problem**: Efficient, accurate SFX detection across platforms

**Decision**: Three-stage pipeline with early exit

**Stage 1: Executable Format Validation** (~1ms)
- Read first 4KB for header parsing
- Use goblin to identify PE/ELF/Mach-O/Script
- Early exit if not executable (performance optimization)
- Shell script detection via shebang (`#!`) or POSIX tar magic

**Stage 2: Archive Signature Scanning** (~10-50ms)
- Scan first 1MB in 512-byte aligned chunks (FR-025)
- Search for archive signatures:
  - ZIP: `PK\x03\x04` or `PK\x05\x06` (central directory)
  - RAR4: `Rar!\x1a\x07\x00`
  - RAR5: `Rar!\x1a\x07\x01\x00`
  - 7z: `7z\xbc\xaf\x27\x1c`
  - TAR: `ustar` at offset 257
- Early exit on first signature match
- 512-byte alignment reduces I/O (reads 2048 chunks for 1MB)

**Stage 3: Archive Validation** (~10-20ms)
- Validate archive structure at detected offset
- Parse format-specific headers
- Confirm valid archive (reject false positives)
- Return SfxDetectionResult with offset and format

**Performance Characteristics**:
- Best case: 1ms (non-executable, Stage 1 exit)
- Average case: 15-70ms (meets <100ms requirement, SC-017)
- Worst case: 80ms (custom stub near 1MB boundary)

**False Positive Mitigation**:
1. Executable format validation (Stage 1) reduces search space
2. Archive header validation (Stage 3) confirms genuine archives
3. Multiple signature checks (magic bytes + structure)
4. Heuristic scoring for ambiguous cases

**Rationale**:
- Meets SC-017 (<100ms for 10MB files)
- Meets SC-018 (zero false positives via validation)
- Meets FR-025 (1MB scan limit with aligned chunks)
- Constitution Principle II (pragmatic - scans only what's needed)

**Alternatives rejected**:
- Full file scan: Multi-second for large executables (violates performance)
- Fixed offsets: Fails for custom stubs (violates 100% detection)
- Heuristic-only: High false positive rate (violates SC-018)

---

### Research 16: SFX Integration with Unified Interface

**Problem**: How SFX detection fits into existing archive workflow

**Decision**: Optional preprocessing step with dedicated API

**API Design**:
```rust
// Dedicated SFX detection API
pub fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>;

// Open archive from SFX (convenience method)
pub fn open_sfx(path: impl AsRef<Path>) -> Result<Archive>;

// Manual workflow for advanced use cases
let sfx_result = Archive::detect_sfx(path)?;
if sfx_result.is_sfx {
    let archive = Archive::open_at_offset(
        path,
        sfx_result.data_offset,
        Some(sfx_result.archive_format)
    )?;
}
```

**SfxDetectionResult Structure** (FR-026):
```rust
pub struct SfxDetectionResult {
    pub is_sfx: bool,
    pub archive_format: Option<ArchiveFormat>,
    pub data_offset: Option<u64>,
    pub stub_type: Option<StubType>,
    pub confidence: f32,  // 0.0-1.0 for heuristic cases
}

pub enum StubType {
    WindowsPE,
    LinuxELF,
    MacOSMachO,
    ShellScript,
}
```

**Rationale**:
1. **Separation of concerns**: SFX detection is distinct from archive operations
2. **Performance**: Users can skip detection for known archives
3. **FR-031**: Separate stub extraction possible via offset manipulation
4. **Unified interface**: Same Archive type works for both SFX and regular archives

**Alternatives rejected**:
- Automatic detection in `Archive::open()`: Performance overhead for regular archives
- Separate SFX namespace: Breaks unified interface philosophy

---

### Research 17: Cross-Platform SFX Testing Strategy

**Problem**: Ensure 100% detection rate (SC-016) and zero false positives (SC-018)

**Decision**: Comprehensive test matrix with official samples

**Test Categories**:

**1. Windows PE SFX** (10 samples):
- 7-Zip SFX: Console (7zCon.sfx), GUI (7z.sfx), Custom (7zS.sfx)
- WinRAR SFX: Default.sfx, Zip.sfx, WinCon.sfx
- Info-ZIP: zip.exe stub
- Custom PE + ZIP combinations

**2. Linux/macOS ELF SFX** (5 samples):
- 7-Zip SFX: Linux 7zCon.sfx module
- makeself: Various wrapper versions (2.4, 2.5)
- Custom ELF + TAR.GZ

**3. Shell Script SFX** (3 samples):
- shar (shell archive) with tar.gz
- makeself with different compression (gzip, bzip2, xz)
- Custom shell + archive combinations

**4. Negative Tests** (100+ samples):
- **Regular executables** (50): System binaries, applications across platforms
- **Regular archives** (30): Pure ZIP/RAR/7z files without stubs
- **Other files** (20): Text, images, PDFs, documents

**Sample Collection**:
- Store in `tests/fixtures/sfx/{windows,linux,macos,shell,negative}/`
- Version-tag samples (e.g., `7zip_24.08_console.exe`)
- Include metadata.json describing creation tool/version

**Validation Criteria**:
```rust
#[test]
fn test_sfx_detection_official_samples() {
    for sample in official_sfx_samples() {
        let result = detect_sfx(&sample.path).unwrap();
        assert!(result.is_sfx, "Failed to detect {}", sample.name);
        assert_eq!(result.archive_format, Some(sample.expected_format));
        assert!(result.data_offset.is_some());

        // SC-019: Verify extraction works
        let archive = Archive::open_at_offset(
            &sample.path,
            result.data_offset.unwrap(),
            result.archive_format
        ).unwrap();
        // Extract and verify...
    }
}

#[test]
fn test_sfx_false_positive_prevention() {
    for sample in negative_test_samples() {
        let result = detect_sfx(&sample.path).unwrap();
        assert!(!result.is_sfx, "False positive for {}", sample.name);
    }
}
```

**Performance Benchmarking** (SC-017):
```rust
#[bench]
fn bench_sfx_detection(b: &mut Bencher) {
    b.iter(|| {
        for sample in benchmark_samples() {  // 1MB - 10MB files
            let result = detect_sfx(&sample).unwrap();
            assert!(result.duration() < Duration::from_millis(100));
        }
    });
}
```

**Rationale**:
- Covers all specified acceptance scenarios (spec lines 92-99)
- Validates success criteria SC-016 through SC-020
- Comprehensive negative testing prevents regressions
- Real samples ensure compatibility with actual SFX tools

---

### Research 18: SFX Error Handling Strategy

**Problem**: Distinguish detection failures from file errors (FR-030)

**Decision**: Hierarchical error types with specific recovery

**Error Categories**:

```rust
pub enum SfxError {
    // File-level errors (propagate immediately)
    IoError(std::io::Error),

    // Detection results (not errors, just negative)
    NotExecutable,        // Not PE/ELF/Mach-O format
    NotSfx,              // Executable but no embedded archive

    // Validation errors (require user action)
    CorruptedArchive {
        detected_format: ArchiveFormat,
        offset: u64,
        reason: String,
    },

    // Parse errors
    InvalidExecutableFormat(String),
}
```

**Error Handling Pattern**:
```rust
pub fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult> {
    // Stage 1: Executable format
    let bytes = read_first_4kb(path)?;  // Propagate I/O errors
    let exe_format = match parse_executable(&bytes) {
        Ok(fmt) => fmt,
        Err(_) => return Ok(SfxDetectionResult::not_sfx())  // Not an error
    };

    // Stage 2: Signature scan
    let signature_result = scan_for_signatures(path, 1_048_576)?;
    if signature_result.is_none() {
        return Ok(SfxDetectionResult::not_sfx())  // Not an error
    }

    // Stage 3: Validation
    let (offset, format) = signature_result.unwrap();
    match validate_archive_at_offset(path, offset, format) {
        Ok(_) => Ok(SfxDetectionResult {
            is_sfx: true,
            archive_format: Some(format),
            data_offset: Some(offset),
            stub_type: Some(exe_format),
            confidence: 1.0,
        }),
        Err(e) => Err(SfxError::CorruptedArchive {
            detected_format: format,
            offset,
            reason: e.to_string(),
        })
    }
}
```

**Rationale**:
- Meets FR-030 (graceful false positive handling)
- Distinguishes I/O errors (user fixes path) from detection negatives (file is not SFX)
- Provides actionable context for corrupted archives
- Constitution Principle I (robust error handling)

**Alternatives rejected**:
- Single error type: Ambiguous user guidance
- Panic on non-SFX: Violates robustness principle

---

### Research 19: SFX Stub Extraction (FR-031)

**Problem**: Security analysis requires extracting executable stub separately

**Decision**: Byte-range API for stub extraction

**API Design**:
```rust
pub fn extract_stub(
    path: impl AsRef<Path>,
    result: &SfxDetectionResult
) -> Result<Vec<u8>> {
    if !result.is_sfx {
        return Err(ArchiveError::InvalidOperation("Not an SFX file"));
    }

    let offset = result.data_offset.unwrap();
    let mut file = File::open(path)?;
    let mut stub = vec![0u8; offset as usize];
    file.read_exact(&mut stub)?;
    Ok(stub)
}
```

**Use Cases**:
- Malware analysis: Inspect executable behavior
- Signature verification: Check stub authenticity
- Custom stub creation: Extract and modify for new SFX archives

**Rationale**:
- Meets FR-031 requirement
- Simple implementation using byte offset
- No complex PE/ELF parsing needed

---

## Summary of SFX Research Decisions

| Research Question | Decision | Key Rationale | Dependency Impact |
|-------------------|----------|---------------|-------------------|
| Binary format parsing | goblin crate | Battle-tested, fuzzed, pure Rust | +goblin 0.9 |
| Detection algorithm | 3-stage pipeline | Meets <100ms, zero false positives | None |
| Integration pattern | Optional preprocessing | Maintains unified interface, no overhead | None |
| Testing strategy | 18+ samples + 100 negative | Validates 100% detection, zero FP | None |
| Error handling | Hierarchical types | Clear user guidance, robust | None |
| Stub extraction | Byte-range API | Simple, meets FR-031 | None |

**Constitution Compliance**:
- ✅ **Principle I (Robustness)**: Explicit error handling, validation at each stage
- ✅ **Principle II (Performance)**: <100ms detection, early exit optimization
- ✅ **Principle III (Minimal Deps)**: Only goblin added, pure Rust, well-justified
- ✅ **Principle IV (Testing)**: Comprehensive test matrix, 100+ samples
- ✅ **Principle V (Documentation)**: Clear contracts, usage examples planned

**Technical Context Resolved**:
- ~~NEEDS CLARIFICATION: Binary parsing library~~ → **goblin 0.9**

**Next Phase** (Phase 1 - Design):
- Update data-model.md with SfxDetectionResult and StubType
- Create contracts/sfx_detection.md defining API
- Update quickstart.md with SFX detection examples
- Update constitution check with goblin dependency justification

## References

- compress-tools source: `/Volumes/Common/QJoon/unified-archive/compress-tools-rs`
- archive-reader source: `/Volumes/Common/QJoon/unified-archive/archive-reader`
- sevenzipjbinding reference: `/Volumes/Common/QJoon/unified-archive/sevenzipjbinding-master`
- libarchive documentation: https://www.libarchive.org/
- unrar license: https://www.rarlab.com/rar_add.htm
- Rust FFI best practices: The Rustonomicon (FFI chapter)
- OnceLock documentation: https://doc.rust-lang.org/std/sync/struct.OnceLock.html
- Rayon documentation: https://docs.rs/rayon/latest/rayon/
- crc32fast: https://docs.rs/crc32fast/
- secstr: https://docs.rs/secstr/
- Rust async streams: https://blog.yoshuawuyts.com/rust-streams/
- goblin crate: https://docs.rs/goblin/
- goblin GitHub: https://github.com/m4b/goblin
