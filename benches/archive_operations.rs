//! Performance benchmarks for archive operations
//!
//! Measures throughput and overhead for core operations:
//! - Archive opening
//! - Entry listing
//! - File extraction
//! - Streaming extraction

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::io::Read;
use std::path::PathBuf;
use unified_archive::{Archive, StreamBound};

/// Helper to get test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Benchmark: Archive opening overhead
///
/// Measures time to open archive and detect format (no listing or extraction).
fn bench_archive_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("archive_open");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);

        group.bench_with_input(
            BenchmarkId::from_parameter(format_name),
            &path,
            |b, path| {
                b.iter(|| {
                    let archive = Archive::open(black_box(path)).unwrap();
                    black_box(archive);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Entry listing throughput
///
/// Measures time to list all entries in archive (tests Phase 1 caching).
fn bench_entry_listing(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry_listing");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).unwrap();

        group.bench_with_input(
            BenchmarkId::new("first_call", format_name),
            &archive,
            |b, _archive| {
                b.iter(|| {
                    // Drop and reopen to test uncached performance
                    let fresh_archive = Archive::open(black_box(&path)).unwrap();
                    let entries = fresh_archive.list_files().unwrap();
                    black_box(entries);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("cached", format_name),
            &archive,
            |b, archive| {
                // First call to populate cache
                let _ = archive.list_files().unwrap();

                // Now benchmark cached access
                b.iter(|| {
                    let entries = archive.list_files().unwrap();
                    black_box(entries);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Small file extraction to memory
///
/// Measures extraction throughput for small files.
fn bench_extract_small_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_small_file");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    let file_to_extract = "test_file.txt";

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);

        group.bench_with_input(
            BenchmarkId::from_parameter(format_name),
            &path,
            |b, path| {
                b.iter(|| {
                    let archive = Archive::open(black_box(path)).unwrap();
                    let data = archive
                        .extract_to_memory(black_box(file_to_extract))
                        .unwrap();
                    black_box(data);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Streaming extraction throughput
///
/// Measures streaming extraction performance with different chunk sizes.
fn bench_streaming_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_extraction");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    let file_to_extract = "test_file.txt";
    let chunk_sizes = vec![64, 512, 4096, 8192];

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);

        for chunk_size in &chunk_sizes {
            group.bench_with_input(
                BenchmarkId::new(format_name, chunk_size),
                &(path.clone(), *chunk_size),
                |b, (path, chunk_size)| {
                    b.iter(|| {
                        let archive = Archive::open(black_box(path)).unwrap();
                        let mut stream = archive
                            .extract_to_stream(
                                black_box(file_to_extract),
                                StreamBound::DeclaredSize,
                            )
                            .unwrap();

                        let mut buffer = vec![0u8; *chunk_size];
                        let mut total = 0;

                        loop {
                            match stream.read(&mut buffer) {
                                Ok(0) => break,
                                Ok(n) => total += n,
                                Err(_) => break,
                            }
                        }

                        black_box(total);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark: Entry lookup performance
///
/// Measures time to find specific entries (tests find_entry vs manual filtering).
fn bench_entry_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry_lookup");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    let search_path = "test_file.txt";

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).unwrap();

        // Pre-populate cache
        let _ = archive.list_files().unwrap();

        group.bench_with_input(
            BenchmarkId::new("find_entry", format_name),
            &archive,
            |b, archive| {
                b.iter(|| {
                    let entry = archive.find_entry(black_box(search_path)).unwrap();
                    black_box(entry);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("manual_filter", format_name),
            &archive,
            |b, archive| {
                b.iter(|| {
                    let entries = archive.list_files().unwrap();
                    let entry = entries.iter().find(|e| e.path == black_box(search_path));
                    black_box(entry);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Format detection overhead
///
/// Measures time to detect archive format from magic bytes.
fn bench_format_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_detection");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);

        group.bench_with_input(
            BenchmarkId::from_parameter(format_name),
            &path,
            |b, path| {
                b.iter(|| {
                    let format = unified_archive::ArchiveFormat::detect(black_box(path)).unwrap();
                    black_box(format);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Validation overhead
///
/// Measures time to validate archive integrity.
fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");

    let test_files = vec![("RAR", "test.rar"), ("ZIP", "test.zip"), ("7z", "test.7z")];

    for (format_name, filename) in test_files {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path).unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format_name),
            &archive,
            |b, archive| {
                b.iter(|| {
                    let report = archive.validate_integrity().unwrap();
                    black_box(report);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_archive_open,
    bench_entry_listing,
    bench_extract_small_file,
    bench_streaming_extraction,
    bench_entry_lookup,
    bench_format_detection,
    bench_validation
);

criterion_main!(benches);
