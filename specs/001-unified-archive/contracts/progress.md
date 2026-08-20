# API Contract: Progress Callbacks

**Feature**: 001-unified-archive
**Date**: 2025-10-31
**Phase**: Phase 1 Enhancement
**Status**: Implemented (retrospective documentation)
**Scope**: Extraction and creation progress. Creation progress (`CompressionOptions.progress`) is invoked per-entry by both ZIP and libarchive backends with `total=None` (AD 0021 / OI-025-003 resolved).

> **Post-v0.1.0 reality check (2026-04-18):** The extraction-entry-point signatures below still show `Result<()>` — the shipped code returns `Result<ResultWithWarnings<()>>` per MADR-0010. Treat the snippets as illustrative for progress semantics only; use `docs/API_REFERENCE.md` for current signatures.

## Overview

Progress callbacks are implemented for both extraction and creation via dispatch through boxed trait objects (`Box<dyn ProgressCallback>`). Creation progress is invoked per-entry by both ZIP and libarchive backends; `total` is `None` because creation cannot pre-size streamed sources. There is no separate validation-progress API. The implementation includes built-in cancellation support and automatic rate limiting (must not exceed ~60 callbacks/sec; actual frequency varies by backend and is often much lower).

## Core Trait

### ProgressCallback

```rust
use std::ops::ControlFlow;

pub trait ProgressCallback: Send + Sync {
    /// Called periodically during long-running operations
    ///
    /// # Parameters
    ///
    /// * `processed` - Bytes processed so far (monotonically increasing)
    /// * `total` - Total bytes to process (None if unknown, e.g. streaming)
    ///
    /// # Returns
    ///
    /// * `ControlFlow::Continue(())` - Continue operation
    /// * `ControlFlow::Break(())` - Cancel operation gracefully
    ///
    /// # Frequency
    ///
    /// Rate-limited to ~60 updates/sec maximum (time-based interval, default 16ms).
    /// Actual frequency varies by backend.
    ///
    /// # Thread Safety
    ///
    /// Must be `Send + Sync`. Callbacks should be written to be thread-safe as a
    /// defensive measure; in practice, current backends call from the caller's thread,
    /// but this is not a guaranteed invariant across future backends.
    ///
    /// # Panics
    ///
    /// Panics in callbacks propagate normally (no `catch_unwind` wrapper).
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>;
}
```

## Automatic Implementations

### Closure Support

```rust
// Any compatible closure automatically implements ProgressCallback
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> ControlFlow<()> + Send + Sync,
{
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()> {
        self(processed, total)
    }
}
```

### Usage Example

```rust
use std::ops::ControlFlow;
use std::path::Path;
use unified_archive::{Archive, ExtractionOptions, ProgressCallback};

let archive = Archive::open("data.zip")?;
let dest = Path::new("./output");

// Closure-based via ExtractionOptions (most common)
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(|processed: u64, total: Option<u64>| {
        if let Some(t) = total {
            let percent = (processed as f64 / t as f64) * 100.0;
            println!("Progress: {:.1}%", percent);
        }
        ControlFlow::Continue(())
    })),
    ..Default::default()
};
archive.extract_all(options)?;

// Struct-based (for complex state)
use std::time::{Instant, Duration};

struct ProgressBar {
    last_update: Instant,
}

impl ProgressCallback for ProgressBar {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()> {
        if self.last_update.elapsed() > Duration::from_millis(100) {
            if let Some(t) = total {
                println!("[{}{}] {:.1}%",
                    "=".repeat((processed * 50 / t) as usize),
                    " ".repeat((50 - processed * 50 / t) as usize),
                    (processed as f64 / t as f64) * 100.0
                );
            }
            self.last_update = Instant::now();
        }
        ControlFlow::Continue(())
    }
}
```

## API Integration

### Extraction with Progress

Progress is configured via the `progress` field on `ExtractionOptions`
(type `Option<Box<dyn ProgressCallback>>`), then passed to `extract_all(options)`.

```rust
impl Archive {
    /// Extract with options (includes optional progress)
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<()> {
        // Progress callback, if present, is forwarded to the format-specific
        // backend which calls it at per-entry granularity via a RateLimiter.
        // ...
    }
}
```

### ExtractionOptions Integration

The `progress` field on `ExtractionOptions` accepts an `Option<Box<dyn ProgressCallback>>`.
Set it directly when constructing the struct (see Usage Example above).
For the full `ExtractionOptions` definition, see `src/options.rs`.

## Rate Limiting (Internal)

Each backend wraps its progress dispatch in a time-based rate limiter (default minimum interval: 16ms). The limiter tracks the wall-clock time since the last dispatched callback and suppresses calls that arrive before the interval elapses. There is no byte-based threshold component. See `src/options.rs` for the `RateLimiter` implementation.

## Contract Guarantees

### Frequency

**Requirement SC-013**: Progress callbacks must not exceed ~60 callbacks/sec (16ms minimum interval between dispatches). This is an upper-bound guarantee only; actual callback frequency is often much lower and depends on backend entry granularity.

**Implementation**:
- Minimum interval: 16ms (time-based only, no byte-based threshold)
- Callback fires when the time interval has elapsed since the last call
- No minimum cadence is guaranteed; backends with few entries may fire very infrequently

### Monotonicity

**Guarantee (processed)**: The `processed` parameter is monotonically non-decreasing across successive callbacks within a single operation.

**Guarantee (processed vs total)**: When `total` is `Some(t)`, `processed <= t` holds. When `total` is `None`, only the monotonicity of `processed` is guaranteed; no upper-bound assertion is possible.

> **Implementation note**: In the current implementation, `total` is computed once at operation start from the sum of known entry sizes. If a backend later reports different sizes, `total` is not revised (it retains the original estimate). This is an implementation detail, not a contract guarantee, and may change.

### Cancellation

**Guarantee**: Returning `ControlFlow::Break(())` from a progress callback requests graceful cancellation.

**Semantics**:
1. The callback returns `ControlFlow::Break(())`
2. Processing stops as soon as the backend checks the return value (best-effort; may not be immediate)
3. The enclosing `extract_all` call returns `Err(ArchiveError)`. There is no dedicated `ArchiveError::Cancelled` variant; the error is typically a format-level error with "Extraction cancelled by user" or similar message. Callers who need to distinguish cancellation from other errors should track the cancellation decision in their callback (e.g., via an `AtomicBool`).

**Important**: Cancellation is best-effort. Partially extracted files may remain on disk depending on the backend and timing. Callers should verify output directory state after cancellation.

**Example** (pseudocode -- `user_cancelled` is application-specific logic):
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// Application-specific cancellation signal (e.g., set by a UI button handler)
let user_cancelled: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

let was_cancelled = Arc::new(AtomicBool::new(false));
let cancel_signal = user_cancelled.clone();
let flag = was_cancelled.clone();
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(move |processed: u64, _total: Option<u64>| {
        if cancel_signal.load(Ordering::Relaxed) {
            flag.store(true, Ordering::Relaxed);
            ControlFlow::Break(())  // Graceful cancellation
        } else {
            ControlFlow::Continue(())
        }
    })),
    ..Default::default()
};
let result = archive.extract_all(options);

if was_cancelled.load(Ordering::Relaxed) || result.is_err() {
    // Cancellation surfaces as an error. Partially extracted files may remain
    // on disk depending on the backend and timing. Verify output directory state.
}
```

### Performance Overhead

**Target**: Negligible overhead relative to I/O-bound extraction work.

**Design expectation**: Callback dispatch (dynamic dispatch via `Box<dyn ProgressCallback>`) and rate-limiter checks (`Instant::elapsed`) are lightweight relative to I/O-bound extraction. At the capped rate of ~60 calls/sec the aggregate overhead is expected to be imperceptible. No formal benchmarks exist in this repository to substantiate specific nanosecond-level figures.

### Error Handling

> **Note**: There is no `catch_unwind` wrapper around callback invocation.
> If a callback panics, the panic propagates normally. Cancellation semantics
> are documented in the Cancellation section above; in short, cancellation
> surfaces as `Err(ArchiveError)` (not `Ok(())`).

## Cross-Format Support

> **Implementation note**: The per-entry update patterns described in this table reflect
> current backend behaviour and are not contractual guarantees. Backend internals may
> change without notice; the contract guarantees are limited to the properties in
> [Contract Guarantees](#contract-guarantees) above (rate limiting, monotonicity,
> cancellation semantics).

| Format | Total Calculation | Current Update Pattern | Cancellation | Notes |
|--------|-------------------|------------------------|--------------|-------|
| RAR | Sum of entry sizes | Per-entry cumulative update after each entry completes | Supported | Reports after full entry decompression |
| ZIP | Sum of entry sizes | Pre-entry and post-entry callbacks per entry | Supported | No intra-entry streaming progress |
| 7z | Sum of entry sizes | Pre-entry and post-entry callbacks per entry | Supported | No intra-entry streaming progress |
| TAR.* | Sum of entry sizes | Per-entry via libarchive | Supported | Granularity depends on libarchive backend |

**Note**: No format currently provides chunk-level (intra-entry) streaming progress. All updates are at entry boundaries. All formats report `total` as `Some(n)` computed from entry metadata when opened via the path-based `Archive::open()` API.

## Testing Strategy

### Unit Tests

> **Note**: The following unit tests are illustrative pseudocode showing the intended
> contract verification approach. They are not compiled as part of the test suite.
> See `tests/` for actual shipped tests.

```rust
#[test]
fn test_progress_callbacks_invoked() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();

    let archive = Archive::open("tests/fixtures/test.zip")?;
    let dest = Path::new("./output");
    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(move |_: u64, _: Option<u64>| {
            counter.fetch_add(1, Ordering::Relaxed);
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };
    archive.extract_all(options)?;

    // Verify that at least one callback was invoked for a non-empty archive.
    // Note: No *minimum frequency* is guaranteed — only that the maximum rate
    // does not exceed ~60/sec. For empty archives, zero callbacks is correct.
    assert!(call_count.load(Ordering::Relaxed) > 0,
        "Expected at least one progress callback for a non-empty archive");
}

#[test]
fn test_progress_monotonic() {
    let last_processed = Arc::new(AtomicU64::new(0));
    let tracker = last_processed.clone();

    let archive = Archive::open("tests/fixtures/test.zip")?;
    let dest = Path::new("./output");
    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(move |processed: u64, total: Option<u64>| {
            let prev = tracker.swap(processed, Ordering::Relaxed);
            assert!(processed >= prev, "Progress decreased");
            if let Some(t) = total {
                assert!(processed <= t, "Current > total");
            }
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };
    archive.extract_all(options)?;
}

#[test]
fn test_progress_cancellation() {
    let was_cancelled = Arc::new(AtomicBool::new(false));
    let flag = was_cancelled.clone();

    let archive = Archive::open("tests/fixtures/test.zip")?;
    let dest = Path::new("./output");
    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(move |processed: u64, _: Option<u64>| {
            if processed > 1000 {
                flag.store(true, Ordering::Relaxed);
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })),
        ..Default::default()
    };
    let result = archive.extract_all(options);

    // Cancellation surfaces as an error (no ArchiveError::Cancelled variant).
    // The exact error depends on the backend — typically a format error.
    assert!(result.is_err() || was_cancelled.load(Ordering::Relaxed));
}

#[test]
#[should_panic(expected = "Simulated panic")]
fn test_progress_panic_propagates() {
    let archive = Archive::open("tests/fixtures/test.zip")?;
    let dest = Path::new("./output");
    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(|_: u64, _: Option<u64>| -> ControlFlow<()> {
            panic!("Simulated panic");
        })),
        ..Default::default()
    };
    // Panic propagates -- no catch_unwind wrapper around callbacks
    let _ = archive.extract_all(options);
}
```

### Integration Tests

> **Note**: The following test is schematic (illustrative pseudocode). It is not
> compiled as part of the test suite. See `tests/` for actual integration tests.

```rust
#[test]
fn test_progress_large_archive() {
    // Schematic: assumes a 1GB archive with 1000 files
    let updates = Arc::new(Mutex::new(Vec::new()));
    let tracker = updates.clone();

    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(move |processed: u64, total: Option<u64>| {
            tracker.lock().unwrap().push((Instant::now(), processed, total));
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };
    archive.extract_all(options)?;

    let updates = updates.lock().unwrap();

    // Verify bounded rate: contractual max is ~60/sec (16ms interval).
    // The 75 Hz threshold here provides a safety margin for timer jitter
    // and measurement granularity.
    let duration = updates.last().unwrap().0 - updates.first().unwrap().0;
    let calls_per_sec = updates.len() as f64 / duration.as_secs_f64();
    assert!(calls_per_sec <= 75.0,
        "Expected ≤75 Hz (contractual ~60 Hz + jitter margin), got {:.1} Hz",
        calls_per_sec);

    // Verify monotonicity
    for window in updates.windows(2) {
        assert!(window[1].1 >= window[0].1, "Progress not monotonic");
    }
}
```

## Performance Benchmarks

> **Note**: Performance benchmarks use [Criterion](https://docs.rs/criterion/) for stable, statistically rigorous measurement. The unstable `#[bench]` API is not used. See `benches/` directory for actual benchmark implementations.

## Edge Cases

> **Note**: The snippets below assume `archive` and `dest` are defined as in the
> [Usage Example](#usage-example) section (e.g., `let archive = Archive::open("data.zip")?;`
> and `let dest = Path::new("./output");`).

### Unknown Total Size

The `total` parameter is `Option<u64>`. When the total is unknown, `None`
is passed. Callbacks must handle `None` gracefully.

**Per-backend behaviour**:
- **RAR, ZIP, 7z**: `total` is `Some(n)` computed from entry metadata at the start of extraction. Virtually always available.
- **TAR.\***: `total` is `Some(n)` when the archive is a seekable file with readable entry headers. In principle, `total` could be `None` if the underlying reader cannot determine entry sizes upfront, but the public `Archive::open()` API accepts file paths (seekable), so `None` is uncommon in practice. (Backend implementation detail: a future non-path-based API for stdin/pipe input would surface `None` more often.)

```rust
// The callback must handle None gracefully
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(|processed: u64, total: Option<u64>| {
        match total {
            Some(t) => println!("{}/{} bytes", processed, t),
            None => println!("{} bytes (total unknown)", processed),
        }
        ControlFlow::Continue(())
    })),
    ..Default::default()
};
```

### Empty Archives

```rust
// Zero-file archive: no progress callbacks
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(|_: u64, _: Option<u64>| -> ControlFlow<()> {
        panic!("Should not be called for empty archive");
    })),
    ..Default::default()
};
let result = archive.extract_all(options)?;
// No panic, no calls
```

### Single Small File

```rust
// <100KB file: may only get 1 callback (at completion), or none at all
// if the backend completes too quickly for the rate limiter to fire.
// The contract guarantees a maximum rate, NOT a minimum number of callbacks.
let call_count = Arc::new(AtomicUsize::new(0));
let counter = call_count.clone();

let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(move |_: u64, _: Option<u64>| {
        counter.fetch_add(1, Ordering::Relaxed);
        ControlFlow::Continue(())
    })),
    ..Default::default()
};
archive.extract_all(options)?;

// Note: Do NOT assert call_count >= 1 here. The contract does not guarantee
// a minimum number of callbacks. A very small, fast extraction may complete
// without any rate-limiter window opening.
println!("Received {} callback(s)", call_count.load(Ordering::Relaxed));
```

## Migration from Other APIs

### From 7zip-JBinding

```java
// 7zip-JBinding
IOutCreateCallback callback = new IOutCreateCallback() {
    public void setTotal(long total) { ... }
    public void setCompleted(long complete) { ... }
};
```

```rust
// unified-archive
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(|processed: u64, total: Option<u64>| {
        // setTotal maps to total (Some(n) when known)
        // setCompleted maps to processed (called repeatedly)
        ControlFlow::Continue(())
    })),
    ..Default::default()
};
archive.extract_all(options)?;
```

### From indicatif (Rust progress bars)

```rust
use indicatif::ProgressBar;

let bar = Arc::new(ProgressBar::new(0));
let bar_ref = bar.clone();

let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(move |processed: u64, total: Option<u64>| {
        if let Some(t) = total {
            bar_ref.set_length(t);
        }
        bar_ref.set_position(processed);
        ControlFlow::Continue(())
    })),
    ..Default::default()
};
archive.extract_all(options)?;

bar.finish();
```

## References

- Phase 0 Research: `research.md` - Progress callback design decisions
- Data Model: `data-model.md` - ProgressCallback entity specification
- Constitution v1.1.0 Principle II: Pragmatic performance (zero-cost abstraction)
- Rust std::ops::ControlFlow: https://doc.rust-lang.org/std/ops/enum.ControlFlow.html
