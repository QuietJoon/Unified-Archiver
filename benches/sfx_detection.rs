//! Benchmark for SFX detection performance (SC-017)
//!
//! Verifies that detection completes in <100ms for typical SFX files.
//!
//! Performance targets:
//! - Non-executable files: <1ms (early exit)
//! - Shell script SFX: 15-50ms (full scan)
//! - Binary executable SFX: 20-70ms (goblin parsing + scan)

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::io::Write;
use tempfile::NamedTempFile;
use unified_archive::Archive;

/// Create a test SFX file with given stub and archive sizes
fn create_test_sfx(stub_size: usize, archive_size: usize) -> NamedTempFile {
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub
    temp.write_all(b"#!/bin/sh\necho 'SFX'\n").unwrap();

    // Padding to reach stub_size
    let stub_padding = vec![0u8; stub_size.saturating_sub(23)];
    temp.write_all(&stub_padding).unwrap();

    // ZIP signature
    temp.write_all(b"PK\x03\x04").unwrap();

    // Archive data
    let archive_data = vec![0u8; archive_size];
    temp.write_all(&archive_data).unwrap();

    temp.flush().unwrap();
    temp
}

/// Create a non-executable file for early-exit testing
fn create_text_file(size: usize) -> NamedTempFile {
    let mut temp = NamedTempFile::new().unwrap();
    let text = vec![b'A'; size];
    temp.write_all(&text).unwrap();
    temp.flush().unwrap();
    temp
}

/// Benchmark: Detection of non-executable files (should be very fast)
fn bench_non_executable_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("sfx_non_executable");

    for size in [1024, 10_240, 102_400].iter() {
        let file = create_text_file(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", size / 1024)),
            size,
            |b, _| {
                b.iter(|| {
                    let result = Archive::detect_sfx(black_box(file.path())).unwrap();
                    assert!(!result.is_sfx);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Detection of shell script SFX files
fn bench_shell_script_sfx_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("sfx_shell_script");

    // Test with different archive offsets
    for offset in [512, 4096, 65536].iter() {
        let file = create_test_sfx(*offset, 1024);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("offset_{}B", offset)),
            offset,
            |b, _| {
                b.iter(|| {
                    let result = Archive::detect_sfx(black_box(file.path())).unwrap();
                    assert!(result.is_sfx);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Detection with large stub (scan more data)
fn bench_large_stub_sfx_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("sfx_large_stub");

    // Stub sizes approaching the 1MB scan limit
    for stub_size in [102_400, 524_288, 1_048_576].iter() {
        let file = create_test_sfx(*stub_size, 1024);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", stub_size / 1024)),
            stub_size,
            |b, _| {
                b.iter(|| {
                    let result = Archive::detect_sfx(black_box(file.path())).unwrap();
                    assert!(result.is_sfx);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Stub extraction performance
fn bench_stub_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("sfx_stub_extraction");

    for stub_size in [4096, 65536, 524_288].iter() {
        let file = create_test_sfx(*stub_size, 1024);
        let detection = Archive::detect_sfx(file.path()).unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", stub_size / 1024)),
            stub_size,
            |b, _| {
                b.iter(|| {
                    let stub = Archive::extract_stub(black_box(file.path()), black_box(&detection))
                        .unwrap();
                    assert!(stub.len() > 0);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Detection with no archive signature (full scan, no match)
fn bench_executable_without_archive(c: &mut Criterion) {
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script without archive
    let script = b"#!/bin/sh\necho 'Not an SFX'\n";
    let padding = vec![0u8; 100_000]; // 100KB of data, no archive
    temp.write_all(script).unwrap();
    temp.write_all(&padding).unwrap();
    temp.flush().unwrap();

    c.bench_function("sfx_no_archive_full_scan", |b| {
        b.iter(|| {
            let result = Archive::detect_sfx(black_box(temp.path())).unwrap();
            assert!(!result.is_sfx);
        });
    });
}

/// Benchmark: Multiple signatures (worst case: scan finds multiple matches)
fn bench_multiple_signatures(c: &mut Criterion) {
    let mut temp = NamedTempFile::new().unwrap();

    // Shell script stub
    temp.write_all(b"#!/bin/sh\n").unwrap();

    // Multiple signatures at different offsets
    for i in 0..10 {
        let padding = vec![0u8; 1000];
        temp.write_all(&padding).unwrap();
        temp.write_all(b"PK\x03\x04").unwrap(); // ZIP signature
        let data = vec![0u8; 100];
        temp.write_all(&data).unwrap();
    }

    temp.flush().unwrap();

    c.bench_function("sfx_multiple_signatures", |b| {
        b.iter(|| {
            let result = Archive::detect_sfx(black_box(temp.path())).unwrap();
            assert!(result.is_sfx);
        });
    });
}

/// Benchmark: Signature at different alignments (512-byte chunks)
fn bench_signature_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("sfx_signature_alignment");

    // Test signatures at aligned and unaligned positions
    for offset in [512, 513, 1024, 1025, 2048].iter() {
        let file = create_test_sfx(*offset, 1024);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("offset_{}", offset)),
            offset,
            |b, _| {
                b.iter(|| {
                    let result = Archive::detect_sfx(black_box(file.path())).unwrap();
                    assert!(result.is_sfx);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_non_executable_detection,
    bench_shell_script_sfx_detection,
    bench_large_stub_sfx_detection,
    bench_stub_extraction,
    bench_executable_without_archive,
    bench_multiple_signatures,
    bench_signature_alignment,
);

criterion_main!(benches);
