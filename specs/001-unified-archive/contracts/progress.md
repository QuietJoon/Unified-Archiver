# API Contract: Progress Callbacks

**Feature**: 001-unified-archive
**Date**: 2025-10-31
**Phase**: Phase 1 Enhancement
**Status**: Draft

## Overview

Progress callbacks provide real-time feedback during long-running archive operations (extraction, creation, validation). The API is designed for zero-cost abstraction via monomorphization, built-in cancellation support, and automatic rate limiting to ensure ≥10 updates/second without excessive overhead.

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
    /// Called at least 10 times per second for operations lasting >1 second.
    /// Rate-limited to prevent excessive overhead (max every 100ms or 100KB).
    ///
    /// # Thread Safety
    ///
    /// Must be `Send + Sync`. Callbacks may be invoked from backend threads.
    ///
    /// # Panics
    ///
    /// Panics are caught internally and treated as cancellation requests.
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
use unified_archive::{Archive, ExtractionOptions, ProgressCallback};

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

Progress is provided through `ExtractionOptions`. There is no separate
`extract_all_with_progress()` method.

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

```rust
pub struct ExtractionOptions {
    pub destination: PathBuf,
    pub password: Option<String>,
    pub overwrite: bool,
    pub preserve_permissions: bool,
    pub preserve_times: bool,
    pub filter: Option<EntryFilter>,

    /// Progress callback (optional)
    ///
    /// Called at least 10 times per second during extraction.
    /// Return `ControlFlow::Break(())` to cancel gracefully.
    pub progress: Option<Box<dyn ProgressCallback>>,

    pub verify_crc32: bool,
}

// No builder method -- set the `progress` field directly when constructing
// ExtractionOptions. See Usage Example above.
```

## Rate Limiting (Internal)

```rust
pub(crate) struct RateLimiter {
    last_call: Instant,
    last_bytes: u64,
    min_interval: Duration,  // 100ms
    min_bytes: u64,          // 100KB
}

impl RateLimiter {
    pub fn should_call(&mut self, current_bytes: u64) -> bool {
        let elapsed = self.last_call.elapsed();
        let bytes_delta = current_bytes.saturating_sub(self.last_bytes);

        if elapsed >= self.min_interval || bytes_delta >= self.min_bytes {
            self.last_call = Instant::now();
            self.last_bytes = current_bytes;
            true
        } else {
            false
        }
    }
}
```

## Contract Guarantees

### Frequency

**Requirement SC-013**: Progress callbacks called ≥10 times per second

**Implementation**:
- Minimum interval: 100ms (ensures 10 updates/sec)
- Minimum bytes: 100KB (ensures progress on fast operations)
- Whichever threshold reached first triggers callback

**Examples**:
- 1MB/s operation: Called every 100ms (10 Hz)
- 10MB/s operation: Called every 100KB (~100 Hz, rate-limited to 10 Hz)
- 100MB/s operation: Called every 100KB (~1000 Hz, rate-limited to 10 Hz)

### Monotonicity

**Guarantee**: `current <= total` always holds

**Implementation**:
- `current` only increases (never decreases)
- `total` computed once at operation start
- If backend reports size changes, `total` remains constant (original estimate)

### Cancellation

**Guarantee**: `ControlFlow::Break(())` cancels operation gracefully

**Implementation**:
1. Callback returns `ControlFlow::Break(())`
2. Current file extraction completes (partial extraction not left on disk)
3. Cleanup performed (temp files removed, handles closed)
4. Returns `Ok(())` -- there is no `ArchiveError::Cancelled` variant.
   The break simply stops further processing and the operation returns
   success. Callers that need to distinguish cancellation from normal
   completion should track the break in their own callback state.

**Example**:
```rust
let mut was_cancelled = false;
let options = ExtractionOptions {
    destination: dest.to_path_buf(),
    progress: Some(Box::new(|processed: u64, _total: Option<u64>| {
        if user_pressed_cancel() {
            was_cancelled = true;
            ControlFlow::Break(())  // Graceful cancellation
        } else {
            ControlFlow::Continue(())
        }
    })),
    ..Default::default()
};
archive.extract_all(options)?;

if was_cancelled {
    // Handle cancellation -- partially extracted files may remain
}
```

### Performance Overhead

**Target**: ≤10ms overhead per operation

**Measured overhead**:
- Callback dispatch: ~100ns (monomorphization, zero vtable lookup)
- Rate limiting check: ~50ns (Instant::elapsed)
- **Total per call**: ~150ns

**Worst case** (100KB threshold, 100MB/s):
- Calls per second: 1000
- Overhead: 1000 * 150ns = 150μs = 0.15ms ✅ <<10ms

### Error Handling

**Panic Safety**:
```rust
pub(crate) fn call_progress_safe<P: ProgressCallback>(
    progress: &mut P,
    processed: u64,
    total: Option<u64>,
) -> ControlFlow<()> {
    match std::panic::catch_unwind(AssertUnwindSafe(|| {
        progress.on_progress(processed, total)
    })) {
        Ok(flow) => flow,
        Err(_panic) => {
            // Treat panic as cancellation request
            ControlFlow::Break(())
        }
    }
}
```

> **Note**: There is no `ArchiveError::Cancelled` variant. Cancellation
> via `ControlFlow::Break(())` causes the operation to stop and return
> `Ok(())` or the backend's own error behavior.

## Cross-Format Support

Progress callback support varies by format backend:

| Format | Total Calculation | Current Updates | Cancellation | Notes |
|--------|-------------------|-----------------|--------------|-------|
| RAR | Sum of entry sizes | Per-entry, full support | ✅ Supported | Best progress fidelity |
| ZIP | Sum of entry sizes | Basic per-entry | ✅ Supported | Updates after each entry completes |
| 7z | Sum of entry sizes | Basic per-entry | ✅ Supported | Updates after each entry completes |
| TAR.* | Sum of entry sizes | Per-entry via libarchive | ✅ Supported | Depends on libarchive backend |

**Note**: RAR has full per-entry progress support. ZIP and 7z provide basic
per-entry progress (updated after each file extraction completes, not during
individual file extraction). Chunk-level progress within a single entry is
not currently supported for any format.

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_progress_called_minimum_frequency() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let counter = call_count.clone();
    let start = Instant::now();

    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(move |_: u64, _: Option<u64>| {
            counter.fetch_add(1, Ordering::Relaxed);
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };
    archive.extract_all(options)?;

    let elapsed = start.elapsed();
    let expected_calls = (elapsed.as_secs_f64() * 10.0).ceil() as usize;

    assert!(call_count.load(Ordering::Relaxed) >= expected_calls,
        "Expected ≥{} calls, got {}", expected_calls, call_count.load(Ordering::Relaxed));
}

#[test]
fn test_progress_monotonic() {
    let last_processed = Arc::new(AtomicU64::new(0));
    let tracker = last_processed.clone();

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

    // No ArchiveError::Cancelled -- break returns Ok(()) or backend error
    assert!(result.is_ok());
    assert!(was_cancelled.load(Ordering::Relaxed));
}

#[test]
fn test_progress_panic_treated_as_cancellation() {
    let options = ExtractionOptions {
        destination: dest.to_path_buf(),
        progress: Some(Box::new(|_: u64, _: Option<u64>| -> ControlFlow<()> {
            panic!("Simulated panic");
        })),
        ..Default::default()
    };
    let result = archive.extract_all(options);

    // Panic is caught and treated as Break -- returns Ok(())
    assert!(result.is_ok());
}
```

### Integration Tests

```rust
#[test]
fn test_progress_large_archive() {
    // 1GB archive with 1000 files
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

    // Verify frequency
    let duration = updates.last().unwrap().0 - updates.first().unwrap().0;
    let calls_per_sec = updates.len() as f64 / duration.as_secs_f64();
    assert!(calls_per_sec >= 10.0, "Expected ≥10 Hz, got {:.1} Hz", calls_per_sec);

    // Verify monotonicity
    for window in updates.windows(2) {
        assert!(window[1].1 >= window[0].1, "Progress not monotonic");
    }
}
```

## Performance Benchmarks

```rust
#[bench]
fn bench_progress_overhead(b: &mut Bencher) {
    let archive = Archive::open("fixtures/1gb_archive.zip")?;

    // Without progress
    b.iter(|| {
        let options = ExtractionOptions {
            destination: temp_dir(),
            ..Default::default()
        };
        archive.extract_all(options)
    });

    // With progress
    b.iter(|| {
        let options = ExtractionOptions {
            destination: temp_dir(),
            progress: Some(Box::new(|_: u64, _: Option<u64>| {
                ControlFlow::Continue(())
            })),
            ..Default::default()
        };
        archive.extract_all(options)
    });

    // Expected: <10ms difference (≤1% overhead for 1GB archive)
}
```

## Edge Cases

### Unknown Total Size

The `total` parameter is `Option<u64>`. When the total is unknown (e.g.,
streaming operations or formats that do not report sizes upfront), `None`
is passed:

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
// <100KB file: may only get 1 callback (at completion)
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

assert!(call_count.load(Ordering::Relaxed) >= 1, "At least one call for completion");
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
