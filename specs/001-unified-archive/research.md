# Technology Research: unified-archive

**Date**: 2025-10-30
**Feature**: 001-unified-archive
**Purpose**: Resolve technical unknowns from Technical Context and make informed technology decisions

> **Post-v0.1.0 reality check (2026-04-18):** This research log is historical — it captures decisions made during planning, not shipped behavior. Canonical current behavior is in the source tree and `docs/architecture/decisions/*.md`. Notable deltas the research log predates: passwords are now `Option<SecStr>` (not plain strings); `Archive::open_at_offset()` and `open_sfx()` are shipped (not deferred); standalone `.gz` / `.bz2` / `.xz` read/extract are supported per AD 0019 (only stream *creation* is out of scope per AD 0018).

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
- **Format support**: Excellent (7z, ZIP, TAR compounds, RAR read-only, ISO)
  > **Historical note (R040-083, AD 0018)**: Standalone GZIP/BZIP2/XZ are not directly openable via `Archive::open`. This was decided in AD 0018; these formats are only supported as TAR compound wrappers (e.g., `.tar.gz`).

**Option B: libarchive**
- **Pros**: C library, well-maintained, cross-platform, clean API
- **Cons**: No RAR5 write support, different architecture than 7zip
- **License**: BSD-2-Clause (permissive)
- **Format support**: Good but lacks full RAR5 support

**Option C: 7-Zip official libraries (LZMA SDK + 7z.dll/7z.so)**
- **Pros**: Official implementation, best format compatibility, actively maintained
- **Cons**: Windows-centric, requires careful cross-platform build setup
- **License**: Public domain (LZMA SDK) + LGPL (7z library)
- **Format support**: Comprehensive (7z, ZIP, TAR compounds, RAR read)
  > **Historical note (R040-084, AD 0018)**: Same standalone-format limitation applies -- standalone GZIP/BZIP2/XZ are not directly openable. Only TAR compound wrappers are supported.

**Option D: Reference projects approach**
- Reference compress-tools (uses libarchive)
- Reference archive-reader (pure Rust, limited formats)
- Reference sevenzipjbinding native libraries

### Decision

**Selected: Hybrid approach - libarchive + unrar library**

> **Retrospective correction (R040-024)**: The original decision framed this as a two-backend "libarchive + unrar" hybrid. The shipped architecture is a five-backend split: **Piz** (ZIP read), **ZipReader** (encrypted ZIP read), **SevenZ** (7z read), **UnRAR** (RAR read), and **libarchive** (TAR/ISO read + all archive creation). The rationale below reflects the original design-time thinking; the "Implementation strategy" section that follows documents the final state.

**Rationale**:
1. **libarchive** provides excellent cross-platform support for TAR variants, ISO, and archive creation
2. **unrar library** (from rarlab) provides RAR/RAR5 read support with official implementation
3. **License compatibility**: BSD-2-Clause (libarchive) + unRAR license (free for non-commercial, commercial requires license)
4. **Proven approach**: compress-tools reference shows libarchive works well in Rust ecosystem
5. **Maintainability**: Well-documented C APIs, active communities

> **Post-implementation note (R040-025)**: libarchive's current role is narrower than originally envisioned -- it handles TAR/ISO read and all archive creation. ZIP read is handled by Piz/ZipReader and 7z read by SevenZ, each chosen for better Rust-native ergonomics in those formats.

**Implementation strategy** *(updated post-implementation)*:
- Use libarchive for: TAR variants (TAR.GZ, TAR.BZ2, TAR.XZ), ISO, archive creation
- Use Piz/ZipReader for: ZIP (read), ZipWriter for: ZIP (write)
- Use SevenZ for: 7z (read)
- Use unrar library for: RAR, RAR5 (read-only per FR-005, creation not required)
- Standalone GZIP, BZIP2, XZ not currently supported (AD 0018)
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

**Selected: Manual checked-in bindings for both libarchive and unrar**

The build uses `pkg-config` to locate libarchive and checked-in manual bindings for both libarchive and unrar. `bindgen` is not used at build time.

**Rationale**:
1. Manual bindings give full control over the exposed API surface
2. No clang/LLVM dependency at build time
3. Both C APIs are stable enough that manual maintenance is low-effort
4. Raw bindings and safe wrappers are split explicitly:
   - `src/ffi/libarchive.rs` -- raw `extern "C"` bindings for libarchive
   - `src/ffi/wrapper.rs` -- safe Rust wrapper for UnRAR FFI calls
   - Each backend (Piz, ZipReader, SevenZ) uses its own Rust-native crate API, not FFI

> *Considered but rejected*: bindgen was originally planned for libarchive's large API surface, and compress-tools showed it works. However, manual bindings proved simpler and removed the build-time clang dependency.

```rust
// build.rs: Link libarchive via pkg-config (no bindgen)
// Manual bindings in src/ffi/libarchive.rs (raw bindings) and src/ffi/wrapper.rs (safe UnRAR wrapper)
```

**Alternatives rejected**:
- **cxx**: Not applicable (no C++ for libarchive/unrar C APIs)
- **bindgen**: Originally planned but manual bindings proved simpler and removed the clang dependency

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
1. Use system libarchive installation via pkg-config
2. unrar built from source (small, self-contained)
3. Documented in build requirements (README.md)

**Platform support matrix**:
| Platform | libarchive linking | Status |
|----------|-------------------|--------|
| macOS    | pkg-config        | Primary (supported) |
| Linux    | pkg-config        | Secondary (supported) |
| Windows  | vcpkg (planned)   | Not yet tested; macOS primary, Linux secondary |

**Build approach** *(updated to match shipped Cargo.toml)*:
```toml
# Cargo.toml
[build-dependencies]
pkg-config = "0.3"

[package.metadata.docs.rs]
# Document build requirements
```

**Alternatives rejected**:
- **Bundle all codecs**: Increases binary size unnecessarily
- **Pure Rust codecs**: Fragmented, incomplete format support
- **vcpkg/static bundling on Windows**: Not yet tested on Windows; macOS primary, Linux secondary. Tracked as future work

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
3. Meets constitution Principle II (Performance First) measurement requirement

#### Pending verification: benchmark fixtures (R040-087)

The following benchmark suites are target-state goals. **Large-archive fixtures have not yet landed**, so these benchmarks cannot run today:

- Archive inspection speed (10k files target)
- Extraction throughput (10GB archive target)
- Memory usage profiling (100MB limit target -- libarchive-backed streaming only)

Once fixtures are generated and committed, the benchmark structure will look like:
```rust
// benches/inspection.rs (hypothetical -- requires fixtures)
fn bench_inspection(c: &mut Criterion) {
    c.bench_function("inspect_10k_files", |b| {
        b.iter(|| {
            let archive = Archive::open("fixtures/10k_files.zip").unwrap();
            archive.list_files().unwrap()
        });
    });
}
```

**CI integration** *(hypothetical -- no CI benchmark workflow exists yet; R040-088)*:
```yaml
# .github/workflows/bench.yml (example, not yet implemented)
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

**Selected: MIT licensing with clear documentation**

**Approach**:
1. **Crate license**: MIT (permissive)
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
| Native library | Split-backend (Piz, ZipReader, SevenZ, UnRAR, libarchive) | Best cross-platform support; see retrospective R040-024 |
| C/C++ interop | Manual checked-in bindings + pkg-config | Full control, no clang dependency |
| Codec dependencies | System-provided via pkg-config | Reduces binary size, leverages system packages |
| Property testing | proptest | Superior shrinking for FFI debugging |
| Performance testing | criterion + CI tracking | Statistical rigor meets constitution requirements |
| Licensing | MIT with feature flag for RAR | Flexibility for commercial users, clear compliance path |

## Implementation Impact

**Technical Context updates** (resolved NEEDS CLARIFICATION):
- **Primary Dependencies**: libarchive (BSD-2-Clause), unrar (conditional), pkg-config (build-only)
- **C/C++ interop layer**: Manual checked-in bindings for both libarchive and unrar
- **Compression codecs**: System-provided (lzma, zlib, bzip2) via pkg-config
- **Property testing**: proptest for invariant testing
- **Performance testing**: criterion with statistical analysis and CI tracking

**Constitution Check updates**:
- ✅ Principle III (Dependencies): All dependencies justified, licensed, with mitigation strategy
- ✅ Principle II (Performance): criterion provides measurement framework
- ✅ Principle IV (Testing): proptest enables comprehensive property-based testing

**Retrospective note**: Phase 1 design documents (data-model.md, contracts/, quickstart.md) have been created and iterated through implementation. See the current spec set for shipped designs.

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
- **Thread-safety**: Use `OnceCell<Vec<ArchiveEntry>>` (`once_cell::sync::OnceCell`) for thread-safe caching
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

**Selected: OnceCell<Vec<ArchiveEntry>> for unified caching**

**Shipped implementation** uses `once_cell::sync::OnceCell` (not `std::sync::OnceLock`).

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
    entry_cache: OnceCell<Vec<ArchiveEntry>>,  // once_cell::sync::OnceCell
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

**Selected: ~~Trait-based with Option parameter for zero-cost abstraction~~ Boxed trait objects in options structs**

**Original rationale** (pre-implementation):
1. Zero-cost static dispatch via monomorphization
2. Cancellation via `ControlFlow` return (Continue/Break)
3. Option wrapping allows omitting callback

**Shipped design**: The implementation uses `Box<dyn ProgressCallback>` stored in `ExtractionOptions` and `CompressionOptions`, with `Send + Sync` bounds. Callbacks are not monomorphized.

**Shipped implementation**:
```rust
pub trait ProgressCallback: Send + Sync {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>;
}

// Callbacks are passed via ExtractionOptions:
let options = ExtractionOptions {
    progress: Some(Box::new(my_callback)),
    ..Default::default()
};
archive.extract_all(options)?;
// Cancellation via ControlFlow::Break surfaces as an ArchiveError (no Cancelled variant).
```

**Shipped rate limiting**:
- Time-based 16ms interval (~60 updates/sec max), no byte-based threshold

**Alternatives rejected**:
- **Closure-only**: Requires generic or Box (breaks zero-cost)
- **Channel-based**: Allocation overhead violates performance principle

### Research 9: Streaming Extraction Architecture

**Problem**: <100MB memory for 10GB+ archives (applies to libarchive-backed streaming paths; native backends buffer entries)

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

**Selected: ~~Read trait with bounded buffering~~ `StreamingExtractor` with backend-dependent memory**

**Shipped design**: The streaming type is `StreamingExtractor` in `src/streaming.rs`. Memory behavior depends on backend:
- **Libarchive-backed formats** (TAR family, ISO): Truly stream with bounded memory (~40KB per entry)
- **Piz, ZipReader, SevenZ, UnRAR**: Buffer the full entry in memory, then wrap in a `Cursor` for the `Read` interface

**Rationale**:
1. **Standard**: Uses std::io traits (no external dependencies)
2. **Memory bounded** (libarchive only): ~40KB per file for TAR-family formats
3. **Backpressure**: Automatic via blocked writes
4. **Composition**: Works with io::copy, compression decoders, etc.

**Shipped implementation**:
```rust
// src/streaming.rs
pub struct StreamingExtractor {
    // Internal reader: direct libarchive stream, or Cursor<Vec<u8>> for buffered backends
}

impl Read for StreamingExtractor { /* ... */ }

// Public API:
let extractor = archive.extract_to_stream("path/in/archive.txt")?;
// extractor implements Read
```

**Memory analysis** (libarchive-backed formats only):
- Entry metadata: ~200 bytes
- BufReader: 16KB
- BufWriter: 16KB
- Working buffer: 8KB
- **Total per file**: ~40KB (libarchive streaming path only)
- **For 10GB archive with 10k files (libarchive)**: 40KB (one at a time, not 10k * 40KB)
- **Non-libarchive backends** (Piz, ZipReader, SevenZ, UnRAR): Full entry buffered in memory — the 40KB estimate does not apply

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

**Shipped implementation**: `VerifyingReader` was not implemented as a standalone type. CRC32 verification is handled inline by each backend via the `verify_crc32` option in `ExtractionOptions`. The crc32fast `Hasher` is used directly in backend extraction paths.

```rust
// Research design (not shipped as a standalone type):
use crc32fast::Hasher;

// Actual verification happens inline in backend extraction code
// when ExtractionOptions { verify_crc32: true, .. } is set.
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

**Shipped**: The API is `extract_all(&self, ExtractionOptions)` with Rayon parallel dispatch for 4+ files (not 10 as originally planned).

**Rationale**:
1. **Simplicity**: Single `.par_iter()` change (pragmatic per Principle II)
2. **CPU utilization**: Saturates cores for CPU-bound decompression
3. **Memory scaling**: Rayon manages thread pool, no manual allocation
4. **Conditional**: Enable for 4+ files (threshold lowered from original 10)
5. **Constitution**: Large change (Rayon dependency) justified by 10%+ gain on multi-file extraction

**Shipped implementation**:
```rust
impl Archive {
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<ExtractionResult> {
        let entries = self.list_files()?;

        if entries.len() >= 4 {
            // Parallel extraction via Rayon
            entries.par_iter().try_for_each(|entry| {
                self.extract_file(&entry.path, options.clone())
            })?;
        } else {
            // Sequential extraction
            for entry in entries {
                self.extract_file(&entry.path, options.clone())?;
            }
        }
        // ...
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

**Selected: ~~secstr for password storage~~ Plain `Option<String>` (secstr deferred)**

The shipped API uses `Option<String>` for passwords. The secstr approach was researched but not integrated.

**Original rationale** (pre-implementation):
1. Security: Auto-zeroing + mlock prevents memory dump exposure
2. FFI-safe: Convert to CString only during FFI call
3. Small dependency, critical security benefit

**Shipped implementation**:
```rust
pub struct ExtractionOptions {
    pub password: Option<String>,
    // ...
}
```

**Password detection**:
- UnRAR: `archive_read_has_encrypted_entries()` FFI call
- libarchive: `archive_entry_is_encrypted()` per entry
- Exposed via `Archive::is_encrypted() -> Result<bool>`

**Alternatives considered**:
- **secstr**: Researched but deferred; adds dependency for marginal security gain in CLI context
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
                    return Err(ArchiveError::Format {
                        format: "RAR".into(),
                        message: "Multi-part RAR: please open the first part (.part01.rar or .rar)".into(),
                    });
                }

                Ok(Archive {
                    backend: ArchiveBackend::Unrar(unrar),
                    entry_cache: OnceCell::new(),
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
                    return Err(ArchiveError::Unsupported {
                        format: "ZIP".into(),
                        details: "Split ZIP archives are not supported. Please use a tool to merge parts first.".into(),
                    });
                }

                Ok(Archive {
                    backend: ArchiveBackend::Libarchive(lib),
                    entry_cache: OnceCell::new(),
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
| Entry caching | OnceCell<Vec> (once_cell) | Zero-cost after init, no locking | +once_cell |
| Progress callbacks | Boxed trait objects (Send+Sync) | Stored in ExtractionOptions | None (std) |
| Streaming extraction | StreamingExtractor | Libarchive: ~40KB; others: buffer full entry | None (std) |
| CRC32 verification | crc32fast | SIMD, <2% overhead, battle-tested | +crc32fast |
| Parallel extraction | Rayon file-level (4+ files) | 10%+ gain, simple, conditional | +rayon |
| Password handling | Option<String> (secstr deferred) | Plain strings, secstr not integrated | None |
| Multi-part support | RAR yes, ZIP no | RAR free, ZIP complex for rare case | None |

**Constitution Compliance**:
- ✅ **Principle II (Pragmatic Performance)**: All changes meet effort-to-benefit criteria
  - OnceCell: Small change, significant caching gain
  - Rayon: Large dependency justified by >10% multi-file speedup
  - CRC32: <2% overhead (acceptable per pragmatic threshold)
- ✅ **Principle III (Unified Interface + Minimal Deps)**: Dependencies justified
  - crc32fast: Critical for integrity verification
  - rayon: Standard parallelism, significant performance gain
  - once_cell: Thread-safe caching
- ✅ **Principle I (Robustness)**: All patterns maintain error handling and safety

**Retrospective note**: Phase 1 design documents (data-model.md, contracts/progress.md, contracts/streaming.md, contracts/extraction.md) have been created and iterated through implementation.

## Performance Baseline Established

**Date**: 2025-10-31
**System**: macOS (Darwin 24.6.0), Rust 2024 edition (1.85+), Release mode
**Archives tested**: Small test fixtures (test.rar, test.zip, test.7z with ~1 file each)

### Current Performance (Release Build)

**Test Suite Execution** *(snapshot at time of initial baseline; current suite has 867+ tests)*:
- 40 integration tests at baseline: **PASS** (100%)
- Total execution time: <0.03s
- Test breakdown (initial baseline):
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
   - Target: <100MB memory (SC-009) — applies to libarchive-backed formats only; native backends buffer entries

4. **Backend comparison**:
   - UnRAR vs libarchive speed for equivalent formats
   - Identify performance characteristics per backend

### Action Items for Comprehensive Baseline

**Phase 1 Task**: Create performance test fixtures and benchmarks
- `benches/extraction.rs` with Criterion (shipped)
- Large-archive fixture creation for comprehensive benchmarking (not yet landed)
- CI integration for regression tracking

**Metrics to track**:
- Inspection latency (p50, p95, p99)
- Extraction throughput (MB/s)
- Memory usage (peak RSS)
- CPU utilization
- Backend performance delta

**Current Status**: ✅ Baseline established for correctness (867+ tests pass as of current implementation)
**Next**: Establish comprehensive performance baseline with realistic workloads (large-archive fixtures not yet landed)

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
        _ => Err(ArchiveError::Format { format: "SFX".into(), message: "Not an executable".into() })
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
- Shell/script detection via shebang (`#!`) only (per shipped stub-type detection)

**Stage 2: Archive Signature Scanning** (~10-50ms)
- Scan first 1MB for all candidate signatures (per AD 0015: iterate all candidates, not first-match)
- Search for archive signatures:
  - ZIP: `PK\x03\x04` (local file header only; central-directory `PK\x01\x02` demoted per AD 0015)
  - RAR4: `Rar!\x1a\x07\x00`
  - RAR5: `Rar!\x1a\x07\x01\x00`
  - 7z: `7z\xbc\xaf\x27\x1c`
  - ~~GZIP/BZIP2/XZ~~: removed from SFX detection signatures (AD 0015 — standalone compressed streams and TAR compounds are not real-world SFX payloads)
  - ~~TAR `ustar`~~: removed from SFX signatures (AD 0015 — TAR SFX not a real-world use case)
- All candidate offsets collected and validated in Stage 3

**Stage 3: Archive Validation** (~10-20ms)
- Validate archive structure at detected offset
- Parse format-specific headers
- Confirm valid archive (reject false positives)
- Return SfxDetectionResult with offset and format

**Performance Characteristics** (illustrative estimates from synthetic fixtures — no dedicated benchmark artifact exists; real-sample corpus not yet landed):
- Best case: ~1ms (non-executable, Stage 1 exit)
- Average case: ~15-70ms (target: <100ms, SC-017)
- Worst case: ~80ms (custom stub near 1MB boundary)

**False Positive Mitigation**:
1. Executable format validation (Stage 1) reduces search space
2. Archive header validation (Stage 3) confirms genuine archives
3. Multiple signature checks (magic bytes + structure)
4. Confidence scoring for ambiguous cases (unknown stubs proceed to signature scanning, OI-027-001 resolved)

**Rationale**:
- Targets SC-017 (<100ms for 10MB files) — estimated from synthetic fixtures only, no benchmark artifact
- Targets SC-018 (zero false positives via validation) — zero false positives observed on synthetic fixtures only; real-sample corpus needed to validate against production binaries
- Meets FR-025 (1MB scan limit)
- Constitution Principle II (pragmatic - scans only what's needed)

**Alternatives rejected**:
- Full file scan: Multi-second for large executables (violates performance)
- Fixed offsets: Fails for custom stubs (reduces detection coverage for unknown stub layouts)
- Heuristic-only: High false positive rate (violates SC-018)

---

### Research 16: SFX Integration with Unified Interface

**Problem**: How SFX detection fits into existing archive workflow

**Decision**: Optional preprocessing step with dedicated API

**API Design**:
```rust
// Dedicated SFX detection API (fully implemented)
pub fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>;

// Detection-only workflow (open_at_offset is deferred — returns ArchiveError::Unsupported)
let sfx_result = Archive::detect_sfx(path)?;
if sfx_result.is_sfx {
    // Currently detection-only: offset opening is not yet implemented
    let offset = sfx_result.data_offset.unwrap();
    println!("SFX detected: format={:?}, offset={}", sfx_result.archive_format, offset);

    // This will return Err(ArchiveError::Unsupported { .. }):
    // let archive = Archive::open_at_offset(path, offset)?;
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
    ScriptInterpreter,  // Renamed from ShellScript (AD 0016)
    Unknown,            // Added for future use (AD 0016)
}
```

**Rationale**:
1. **Separation of concerns**: SFX detection is distinct from archive operations
2. **Performance**: Users can skip detection for known archives
3. **FR-031**: Separate stub extraction via `extract_stub()` returns `Vec<u8>` (does not solve direct offset opening)
4. **Unified interface**: Same Archive type works for both SFX and regular archives

**Alternatives rejected**:
- Automatic detection in `Archive::open()`: Performance overhead for regular archives
- Separate SFX namespace: Breaks unified interface philosophy

---

### Research 17: Cross-Platform SFX Testing Strategy

**Problem**: Approach high detection coverage (SC-016 targets 100%) and minimize false positives (SC-018)

**Decision**: Comprehensive test matrix with official samples (future-state target — unknown/custom-stub scanning resolved per OI-027-001; real-sample corpus not yet landed)

**Note:** The described sample matrix (10 Windows, 5 ELF/macOS, 3 shell, 100+ negatives) is planned but not yet implemented. Current tests use synthetic fixtures.

**Test Categories**:

**1. Windows PE SFX** (10 samples):
- 7-Zip SFX: Console (7zCon.sfx), GUI (7z.sfx), Custom (7zS.sfx)
- WinRAR SFX: Default.sfx, Zip.sfx, WinCon.sfx
- Info-ZIP: zip.exe stub
- Custom PE + ZIP combinations

**2. Linux/macOS ELF SFX** (5 samples):
- 7-Zip SFX: Linux 7zCon.sfx module
- makeself: Various wrapper versions (2.4, 2.5)
- Custom ELF + ZIP/RAR/7z payloads (TAR removed from SFX signatures per AD 0015)

**3. Shell Script SFX** (3 samples):
- Script with embedded ZIP/RAR/7z payloads
- makeself with supported archive formats
- Custom script + archive combinations (TAR/shar-based cases removed per AD 0015)

**4. Negative Tests** (100+ samples):
- **Regular executables** (50): System binaries, applications across platforms
- **Regular archives** (30): Pure ZIP/RAR/7z files without stubs
- **Other files** (20): Text, images, PDFs, documents

**Sample Collection** (future corpus layout, not yet landed):
- Planned location: `tests/fixtures/sfx/{windows,linux,macos,shell,negative}/`
- Version-tag samples (e.g., `7zip_24.08_console.exe`)
- Current tests use synthetic fixtures in `tests/integration/` (not tests/fixtures/sfx/)

**Validation Criteria**:
```rust
#[test]
fn test_sfx_detection_official_samples() {
    for sample in official_sfx_samples() {
        let result = detect_sfx(&sample.path).unwrap();
        assert!(result.is_sfx, "Failed to detect {}", sample.name);
        assert_eq!(result.archive_format, Some(sample.expected_format));
        assert!(result.data_offset.is_some());

        // Detection-only validation (open_at_offset is deferred, returns ArchiveError::Unsupported)
        let err = Archive::open_at_offset(
            &sample.path,
            result.data_offset.unwrap(),
        ).unwrap_err();
        assert!(matches!(err, ArchiveError::Unsupported { .. }));
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
// Criterion-based benchmark (shipped approach)
fn bench_sfx_detection(c: &mut Criterion) {
    c.bench_function("sfx_detection", |b| {
        b.iter(|| {
            for sample in benchmark_samples() {  // 1MB - 10MB files
                let _result = detect_sfx(&sample).unwrap();
            }
        });
    });
}
```

**Rationale**:
- Covers all specified acceptance scenarios
- Targets success criteria SC-016 through SC-020
- Negative testing prevents regressions
- Real samples (when landed) will ensure compatibility with actual SFX tools

---

### Research 18: SFX Error Handling Strategy

**Problem**: Distinguish detection failures from file errors (FR-030)

**Decision**: ~~Hierarchical error types~~ Unified `ArchiveError` model

The shipped crate uses the unified `ArchiveError` type for all SFX errors; the planned `SfxError` hierarchy was not implemented. Non-SFX files return `SfxDetectionResult::not_sfx()` (a result, not an error). I/O failures surface as `ArchiveError::Io`.

```rust
// Original research design (not shipped):
// pub enum SfxError { IoError, NotExecutable, NotSfx, CorruptedArchive, ... }

// Shipped approach: uses ArchiveError variants
// - I/O failures → ArchiveError::Io { operation, path, source }
// - Non-SFX files → SfxDetectionResult::not_sfx() (success, not error)
// - Format issues → ArchiveError::Format { format, message }
```

**Shipped Error Handling Pattern**:
```rust
pub fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult> {
    // Stage 1: Executable format — identifies StubType
    let bytes = read_first_bytes(path)?;  // Propagate I/O errors
    let stub_type = match parse_executable(&bytes) {
        Ok(st) => st,
        Err(_) => return Ok(SfxDetectionResult::not_sfx())  // Not an error
    };

    // Stage 2: Signature scan — collects all candidates
    let candidates = scan_for_signatures(&bytes)?;
    if candidates.is_empty() {
        return Ok(SfxDetectionResult::not_sfx())  // Not an error
    }

    // Stage 3: Validate candidates (iterate all per AD 0015)
    for (offset, format) in &candidates {
        if validate_format_probe(&bytes[*offset..], *format) {
            return Ok(SfxDetectionResult::probable(
                stub_type, *format, *offset as u64, 0.9,  // 0.9 is the detector's current policy value, not a type contract
            ));
        }
    }
    Ok(SfxDetectionResult::not_sfx())
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
        return Err(ArchiveError::Unsupported {
            format: "SFX".into(),
            details: "extract_stub requires an SFX file".into(),
        });
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
| Detection algorithm | 3-stage pipeline | Targets <100ms, iterate-all-candidates (AD 0015) | None |
| Integration pattern | Optional preprocessing | Maintains unified interface, no overhead | None |
| Testing strategy | Synthetic fixtures (real-sample corpus planned) | Detection verified on synthetic fixtures only; coverage of real-world SFX binaries unknown | None |
| Error handling | Unified ArchiveError | Clear user guidance, robust | None |
| Stub extraction | Byte-range API | Simple, meets FR-031 | None |

**Constitution Compliance**:
- ✅ **Principle I (Robustness)**: Explicit error handling, validation at each stage
- ✅ **Principle II (Performance)**: Targets <100ms detection, early exit optimization
- ✅ **Principle III (Minimal Deps)**: Only goblin added, pure Rust, well-justified
- ✅ **Principle IV (Testing)**: Synthetic test matrix; comprehensive 100+ real-sample corpus planned
- ✅ **Principle V (Documentation)**: Clear contracts, usage examples shipped (e.g., `examples/detect_sfx.rs`)

**Technical Context Resolved**:
- ~~NEEDS CLARIFICATION: Binary parsing library~~ → **goblin 0.9**

**Retrospective note**: SFX types (SfxDetectionResult, StubType) are defined in `src/sfx/result.rs` and `src/sfx/stub_types.rs`. Detection is implemented in `src/sfx/detection.rs`. The quickstart and data-model documents have been updated with SFX examples.

## References

- compress-tools source: `/Volumes/Common/QJoon/unified-archive/compress-tools-rs`
- archive-reader source: `/Volumes/Common/QJoon/unified-archive/archive-reader`
- sevenzipjbinding reference: `/Volumes/Common/QJoon/unified-archive/sevenzipjbinding-master`
- libarchive documentation: https://www.libarchive.org/
- unrar license: https://www.rarlab.com/rar_add.htm
- Rust FFI best practices: The Rustonomicon (FFI chapter)
- once_cell documentation: https://docs.rs/once_cell/latest/once_cell/
- Rayon documentation: https://docs.rs/rayon/latest/rayon/
- crc32fast: https://docs.rs/crc32fast/
- Rust async streams: https://blog.yoshuawuyts.com/rust-streams/

**Future hardening references** (not currently integrated):
- secstr: https://docs.rs/secstr/ (researched for password handling, deferred per Decision 12)
- goblin crate: https://docs.rs/goblin/
- goblin GitHub: https://github.com/m4b/goblin
