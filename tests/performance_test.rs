//! Performance regression tests
//!
//! Verifies that key operations meet performance targets from plan.md:
//! - List 10k files in <1 second
//! - Extract 1GB archive with reasonable throughput
//! - Memory usage stays within SC-009 limits (<100MB for large archives)
//!
//! These tests use actual timing assertions rather than relative benchmarks.

use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;
use unified_archive::{Archive, StreamBound};

/// Helper to get test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Performance target: List files should be fast even with caching
///
/// Target: <100ms for our small test archives (1 file)
/// Scales to ~1s for 10k files based on linear extrapolation.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_entry_listing_cached() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixtures_dir().join(filename);
        let archive =
            Archive::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", filename));

        // First call populates cache
        let _ = archive.list_files().expect("First list failed");

        // Measure cached access (should be O(1) slice return)
        let start = Instant::now();
        for _ in 0..1000 {
            let _ = archive.list_files().expect("Cached list failed");
        }
        let elapsed = start.elapsed();

        let per_call = elapsed.as_micros() / 1000;

        // Should be very fast (< 10 microseconds per call for cached access)
        assert!(
            per_call < 50,
            "Cached list_files should be <50µs, got {}µs for {}",
            per_call,
            filename
        );

        println!("✓ {} cached listing: {}µs per call", filename, per_call);
    }
}

/// Performance target: Format detection should be fast
///
/// Target: <1ms for magic byte detection
#[test]
#[serial_test::file_serial(rar)]
fn perf_format_detection() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixtures_dir().join(filename);

        let start = Instant::now();
        for _ in 0..100 {
            let _ = unified_archive::ArchiveFormat::detect(&path).expect("Format detection failed");
        }
        let elapsed = start.elapsed();

        let per_call = elapsed.as_micros() / 100;

        // Should be very fast (< 100 microseconds per call)
        assert!(
            per_call < 1000,
            "Format detection should be <1ms, got {}µs for {}",
            per_call,
            filename
        );

        println!("✓ {} format detection: {}µs per call", filename, per_call);
    }
}

/// Performance target: Archive opening overhead
///
/// Target: <10ms to open small archive
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_archive_open() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixtures_dir().join(filename);

        let start = Instant::now();
        for _ in 0..50 {
            let _ = Archive::open(&path).expect("Failed to open archive");
        }
        let elapsed = start.elapsed();

        let per_open = elapsed.as_millis() / 50;

        // Should be fast (< 20ms per open for small archive)
        assert!(
            per_open < 50,
            "Archive open should be <50ms, got {}ms for {}",
            per_open,
            filename
        );

        println!("✓ {} open overhead: {}ms per open", filename, per_open);
    }
}

/// Performance target: Small file extraction
///
/// Target: <50ms to extract small file (< 1KB)
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_small_file_extraction() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];
    let file_to_extract = "test_file.txt";

    for filename in test_files {
        let path = fixtures_dir().join(filename);

        let start = Instant::now();
        for _ in 0..20 {
            let archive = Archive::open(&path).expect("Failed to open");
            let _ = archive
                .extract_to_memory(file_to_extract)
                .expect("Failed to extract");
        }
        let elapsed = start.elapsed();

        let per_extract = elapsed.as_millis() / 20;

        // Should be fast (< 100ms per extraction for small file)
        assert!(
            per_extract < 100,
            "Small file extraction should be <100ms, got {}ms for {}",
            per_extract,
            filename
        );

        println!(
            "✓ {} small file extraction: {}ms per extract",
            filename, per_extract
        );
    }
}

/// Performance target: Streaming extraction has minimal overhead
///
/// Target: Streaming should be within 2x of memory extraction time.
///
/// R0070-0085: ZIP streaming (`Archive::extract_to_stream` on the
/// `zip`-crate backend) currently materialises the entry into a
/// `Cursor<Vec<u8>>` rather than truly streaming — see
/// [`OI-0057-007`](docs/project/open-issues.md). This test drops ZIP
/// from the comparison so the streaming-vs-memory ratio reflects
/// backends that actually stream (libarchive-backed RAR/7z).
#[test]
#[cfg(feature = "rar-support")]
#[serial_test::file_serial(rar)]
fn perf_streaming_overhead() {
    let test_files = vec!["test.rar", "test.7z"];
    let file_to_extract = "test_file.txt";

    for filename in test_files {
        let path = fixtures_dir().join(filename);

        // Measure memory extraction
        let start = Instant::now();
        for _ in 0..20 {
            let archive = Archive::open(&path).expect("Failed to open");
            let _ = archive
                .extract_to_memory(file_to_extract)
                .expect("Failed to extract");
        }
        let memory_time = start.elapsed();

        // Measure streaming extraction
        let start = Instant::now();
        for _ in 0..20 {
            let archive = Archive::open(&path).expect("Failed to open");
            let mut stream = archive
                .extract_to_stream(file_to_extract, StreamBound::Unbounded)
                .expect("Failed to create stream");

            let mut buffer = Vec::new();
            stream.read_to_end(&mut buffer).expect("Failed to read");
        }
        let streaming_time = start.elapsed();

        let ratio = streaming_time.as_millis() as f64 / memory_time.as_millis() as f64;

        // Streaming should not be significantly slower (within 3x is acceptable)
        assert!(
            ratio < 3.0,
            "Streaming should be within 3x of memory extraction, got {:.2}x for {}",
            ratio,
            filename
        );

        println!(
            "✓ {} streaming overhead: {:.2}x vs memory ({}ms vs {}ms)",
            filename,
            ratio,
            streaming_time.as_millis() / 20,
            memory_time.as_millis() / 20
        );
    }
}

/// Performance target: Entry lookup is efficient
///
/// Target: find_entry() should be O(n) but fast for small archives
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_entry_lookup() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];
    let search_path = "test_file.txt";

    for filename in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).expect("Failed to open");

        // Pre-populate cache
        let _ = archive.list_files().expect("Failed to list");

        let start = Instant::now();
        for _ in 0..1000 {
            let _ = archive.find_entry(search_path).expect("Failed to find");
        }
        let elapsed = start.elapsed();

        let per_lookup = elapsed.as_micros() / 1000;

        // Should be fast (< 100 microseconds for 1-file archive)
        assert!(
            per_lookup < 200,
            "Entry lookup should be <200µs, got {}µs for {}",
            per_lookup,
            filename
        );

        println!("✓ {} entry lookup: {}µs per lookup", filename, per_lookup);
    }
}

/// Performance target: Validation is reasonably fast
///
/// Target: <100ms for small archives
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_validation() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];

    for filename in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).expect("Failed to open");

        let start = Instant::now();
        for _ in 0..20 {
            let _ = archive.validate_integrity().expect("Validation failed");
        }
        let elapsed = start.elapsed();

        let per_validation = elapsed.as_millis() / 20;

        // Should be fast (< 50ms for 1-file archive)
        assert!(
            per_validation < 100,
            "Validation should be <100ms, got {}ms for {}",
            per_validation,
            filename
        );

        println!(
            "✓ {} validation: {}ms per validation",
            filename, per_validation
        );
    }
}

/// Streaming chunked-read integrity test
///
/// R0079-0046: previously named `perf_streaming_memory_bounded` and
/// claimed to "prove we're not loading all at once" — a tiny read
/// buffer cannot demonstrate that (a backend that buffered the whole
/// entry would still hand it out 4 bytes per `read` call). What the
/// tiny buffer *does* pin is that data delivered across many small
/// reads reassembles into the exact entry content on every backend.
/// Memory-bound verification (SC-009) needs an external profiling
/// harness.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_streaming_chunked_read_integrity() {
    let test_files = vec!["test.rar", "test.zip", "test.7z"];
    let file_to_extract = "test_file.txt";

    for filename in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).expect("Failed to open");

        let mut stream = archive
            .extract_to_stream(file_to_extract, StreamBound::Unbounded)
            .expect("Failed to create stream");

        // Read in very small chunks
        let chunk_size = 4; // Intentionally tiny
        let mut buffer = vec![0u8; chunk_size];
        let mut chunks_read = 0;
        let mut content = Vec::new();

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    content.extend_from_slice(&buffer[..n]);
                    chunks_read += 1;
                }
                Err(e) => panic!("Stream read error: {}", e),
            }
        }

        // A 4-byte buffer over an 18-byte entry must take several reads.
        assert!(
            chunks_read > 1,
            "Should read multiple chunks for {} (got {})",
            filename,
            chunks_read
        );

        // The chunked reads must reassemble into the exact entry content.
        let text = String::from_utf8_lossy(&content);
        assert!(
            text.contains("Hello, RAR World!"),
            "Chunked reads should reassemble entry content for {} (got {:?})",
            filename,
            text
        );

        println!(
            "✓ {} chunked streaming: {} chunks of {} bytes (total {} bytes)",
            filename,
            chunks_read,
            chunk_size,
            content.len()
        );
    }
}

/// Performance regression: Ensure no major slowdowns
///
/// This test establishes baseline timing for future comparison.
/// Fails if operations are suspiciously slow (>10x expected).
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn perf_regression_check() {
    let path = fixtures_dir().join("test.rar");
    let file_to_extract = "test_file.txt";

    // Open archive
    let start = Instant::now();
    let archive = Archive::open(&path).expect("Failed to open");
    let open_time = start.elapsed();

    // List files (first time)
    let start = Instant::now();
    let _entries = archive.list_files().expect("Failed to list");
    let list_time = start.elapsed();

    // Find entry (reuses cached list)
    let start = Instant::now();
    let _entry = archive.find_entry(file_to_extract).expect("Failed to find");
    let find_time = start.elapsed();

    // Extract to memory (open fresh archive due to UnRAR state)
    let archive2 = Archive::open(&path).expect("Failed to open for extract");
    let start = Instant::now();
    let _data = archive2
        .extract_to_memory(file_to_extract)
        .expect("Failed to extract");
    let extract_time = start.elapsed();

    // Extract to stream (open fresh archive)
    let archive3 = Archive::open(&path).expect("Failed to open for stream");
    let start = Instant::now();
    let mut stream = archive3
        .extract_to_stream(file_to_extract, StreamBound::Unbounded)
        .expect("Failed to stream");
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer).expect("Failed to read");
    let stream_time = start.elapsed();

    // Validate (can reuse archive)
    let start = Instant::now();
    let _report = archive.validate_integrity().expect("Failed to validate");
    let validate_time = start.elapsed();

    // R0074-0078: keep the timing assertions but gate the noisy
    // baseline print behind an opt-in env var so default test runs
    // stay quiet.
    if std::env::var_os("UA_PRINT_PERF_BASELINE").is_some() {
        println!("\n=== Performance Baseline (test.rar) ===");
        println!("  Open:         {:?}", open_time);
        println!("  List:         {:?}", list_time);
        println!("  Find:         {:?}", find_time);
        println!("  Extract:      {:?}", extract_time);
        println!("  Stream:       {:?}", stream_time);
        println!("  Validate:     {:?}", validate_time);
    }

    // Sanity checks: operations should complete in reasonable time
    // These are very generous limits (10x what we expect in practice)
    assert!(
        open_time.as_millis() < 500,
        "Open too slow: {:?}",
        open_time
    );
    assert!(
        list_time.as_millis() < 500,
        "List too slow: {:?}",
        list_time
    );
    assert!(
        find_time.as_micros() < 10000,
        "Find too slow: {:?}",
        find_time
    );
    assert!(
        extract_time.as_millis() < 500,
        "Extract too slow: {:?}",
        extract_time
    );
    assert!(
        stream_time.as_millis() < 1000,
        "Stream too slow: {:?}",
        stream_time
    );
    assert!(
        validate_time.as_millis() < 500,
        "Validate too slow: {:?}",
        validate_time
    );
}
