// Performance benchmarks for archive integrity validation
// Critical for ensuring the library performs well in production

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;
use unified_archive::Archive;

fn bench_validation_zip(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation_zip");
    group.measurement_time(Duration::from_secs(10));

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    group.bench_function("batch_test_zip_5_files", |b| {
        b.iter(|| {
            let report = archive
                .validate_integrity()
                .expect("Validation should succeed");
            black_box(report);
        });
    });

    group.finish();
}

fn bench_validation_rar(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation_rar");
    group.measurement_time(Duration::from_secs(10));

    // Test RAR5 format (more modern)
    if let Ok(archive) = Archive::open("tests/fixtures/test_rar5.rar") {
        group.bench_function("test_rar5_1_file", |b| {
            b.iter(|| {
                if let Ok(report) = archive.validate_integrity() {
                    black_box(report);
                }
            });
        });
    }

    group.finish();
}

fn bench_validation_7z(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation_7z");
    group.measurement_time(Duration::from_secs(10));

    if let Ok(archive) = Archive::open("tests/fixtures/test.7z") {
        group.bench_function("test_7z_1_file", |b| {
            b.iter(|| {
                if let Ok(report) = archive.validate_integrity() {
                    black_box(report);
                }
            });
        });
    }

    group.finish();
}

fn bench_validation_repeated(c: &mut Criterion) {
    // Benchmark repeated validation calls on same archive
    let mut group = c.benchmark_group("validation_repeated");
    group.measurement_time(Duration::from_secs(10));

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    group.bench_function("10_consecutive_validations", |b| {
        b.iter(|| {
            for _ in 0..10 {
                let report = archive
                    .validate_integrity()
                    .expect("Validation should succeed");
                black_box(report);
            }
        });
    });

    group.finish();
}

fn bench_validation_vs_list_files(c: &mut Criterion) {
    // Compare validation performance vs simple listing
    let mut group = c.benchmark_group("validation_comparison");
    group.measurement_time(Duration::from_secs(10));

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    group.bench_function("list_files_only", |b| {
        b.iter(|| {
            let entries = archive.list_files().expect("List should succeed");
            black_box(entries);
        });
    });

    group.bench_function("full_validation", |b| {
        b.iter(|| {
            let report = archive
                .validate_integrity()
                .expect("Validation should succeed");
            black_box(report);
        });
    });

    group.finish();
}

fn bench_validation_concurrent(c: &mut Criterion) {
    // Benchmark concurrent validation performance
    let mut group = c.benchmark_group("validation_concurrent");
    group.measurement_time(Duration::from_secs(15));

    group.bench_function("4_threads_same_archive", |b| {
        b.iter(|| {
            use std::thread;

            let handles: Vec<_> = (0..4)
                .map(|_| {
                    thread::spawn(|| {
                        if let Ok(archive) = Archive::open("tests/fixtures/batch_test.zip") {
                            archive.validate_integrity().ok()
                        } else {
                            None
                        }
                    })
                })
                .collect();

            let results: Vec<_> = handles
                .into_iter()
                .filter_map(|h| h.join().ok().flatten())
                .collect();

            black_box(results);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_validation_zip,
    bench_validation_rar,
    bench_validation_7z,
    bench_validation_repeated,
    bench_validation_vs_list_files,
    bench_validation_concurrent,
);

criterion_main!(benches);
