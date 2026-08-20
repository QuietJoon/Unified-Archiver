//! Performance baseline comparison tests (SC-010, T127b)
//!
//! Compares unified-archive extraction throughput vs native 7z CLI.
//! Target: Within 20% of native 7zip performance (SC-010).
//!
//! **Opt-in (R0074-0080).** This suite shells out to the system
//! `7z` / `unzip` binaries for the comparison and adds wall-clock
//! variance to the default test run, so each timing-comparison test
//! short-circuits unless `UA_PRINT_PERF_BASELINE` is set
//! (see `R0074-0078`). Suites that build the test fixture but do not
//! compare against the external CLI run as normal integration tests.
//! Long-term destination is `benches/` (R0074-0070).

use super::common;
use super::common::command_exists;

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Create a test archive with specified size for performance testing
fn create_performance_test_archive(
    archive_path: &Path,
    total_size_mb: usize,
) -> std::io::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let source_dir = temp_dir.path();

    // Create files totaling approximately total_size_mb
    let file_size = 1024 * 1024; // 1MB per file
    let num_files = total_size_mb;

    for i in 0..num_files {
        let file_path = source_dir.join(format!("perf_test_{:04}.bin", i));
        let mut file = BufWriter::new(File::create(&file_path)?);

        // Write 1MB of varied data (not too compressible, not random)
        let pattern: Vec<u8> = (0..=255u8)
            .cycle()
            .enumerate()
            .map(|(idx, b)| b.wrapping_add((idx % 17) as u8))
            .take(file_size)
            .collect();
        file.write_all(&pattern)?;
        file.flush()?;
    }

    // Create ZIP archive with moderate compression
    let output = Command::new("zip")
        .args(["-r", "-5"]) // -5 = moderate compression
        .arg(archive_path)
        .arg(".")
        .current_dir(source_dir)
        .output()?;

    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "zip failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(())
}

/// Measure extraction time using native 7z CLI
fn measure_7z_extraction(archive_path: &Path, output_dir: &Path) -> Option<Duration> {
    if !command_exists("7z") {
        return None;
    }

    fs::create_dir_all(output_dir).ok()?;

    let start = Instant::now();
    let output = Command::new("7z")
        .args(["x", "-y", "-o"])
        .arg(output_dir)
        .arg(archive_path)
        .output()
        .ok()?;

    if output.status.success() {
        Some(start.elapsed())
    } else {
        None
    }
}

/// Measure extraction time using unzip CLI (fallback)
fn measure_unzip_extraction(archive_path: &Path, output_dir: &Path) -> Option<Duration> {
    if !command_exists("unzip") {
        return None;
    }

    fs::create_dir_all(output_dir).ok()?;

    let start = Instant::now();
    let output = Command::new("unzip")
        .args(["-o", "-q"]) // overwrite, quiet
        .arg(archive_path)
        .args(["-d"])
        .arg(output_dir)
        .output()
        .ok()?;

    if output.status.success() {
        Some(start.elapsed())
    } else {
        None
    }
}

/// Measure extraction time using unified-archive
fn measure_unified_archive_extraction(archive_path: &Path, output_dir: &Path) -> Option<Duration> {
    fs::create_dir_all(output_dir).ok()?;

    let archive = unified_archive::Archive::open(archive_path).ok()?;

    let start = Instant::now();
    let options = common::default_extraction_options(output_dir.to_path_buf());
    let result = archive.extract_all(options);

    if result.is_ok() {
        Some(start.elapsed())
    } else {
        eprintln!("unified-archive extraction failed: {:?}", result);
        None
    }
}

#[test]
fn test_performance_baseline_small_archive() {
    // R0074-0080: timing comparison against the external 7z/unzip CLI
    // runs only under the opt-in profile (see module doc).
    if std::env::var_os("UA_PRINT_PERF_BASELINE").is_none() {
        return;
    }

    // Test with a small archive (10MB) for quick feedback
    if !command_exists("zip") {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("perf_test_10mb.zip");

    // Create 10MB test archive
    if let Err(e) = create_performance_test_archive(&archive_path, 10) {
        eprintln!("Skipping test: Failed to create archive: {}", e);
        return;
    }

    let archive_size = fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0);
    println!("Archive size: {} bytes", archive_size);

    // Measure unified-archive extraction
    let ua_output = temp_dir.path().join("ua_output");
    let ua_time = measure_unified_archive_extraction(&archive_path, &ua_output);

    // Measure native tool extraction (7z or unzip)
    let native_output = temp_dir.path().join("native_output");
    let native_time = measure_7z_extraction(&archive_path, &native_output)
        .or_else(|| measure_unzip_extraction(&archive_path, &native_output));

    // Report results
    println!("\n=== Performance Baseline Results ===");
    println!("Archive: 10MB test archive");

    if let Some(ua_duration) = ua_time {
        println!("unified-archive: {:?}", ua_duration);

        if let Some(native_duration) = native_time {
            println!("Native tool: {:?}", native_duration);

            let ratio = ua_duration.as_secs_f64() / native_duration.as_secs_f64();
            println!("Ratio (unified/native): {:.2}x", ratio);

            // SC-010 target: within 20% of native (ratio <= 1.2)
            if ratio <= 1.2 {
                println!("✓ PASS: Within 20% of native performance");
            } else if ratio <= 2.0 {
                println!(
                    "△ ACCEPTABLE: {:.0}% slower than native (target: ≤20%)",
                    (ratio - 1.0) * 100.0
                );
            } else {
                println!(
                    "✗ NEEDS IMPROVEMENT: {:.0}% slower than native",
                    (ratio - 1.0) * 100.0
                );
            }
        } else {
            println!("Native tool not available for comparison");
        }
    } else {
        println!("unified-archive extraction failed");
    }

    // Verify extracted files
    if ua_output.exists() {
        let extracted_files = fs::read_dir(&ua_output)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        println!("Extracted files: {}", extracted_files);
        assert!(extracted_files > 0, "Should extract some files");
    }
}

#[test]
fn test_performance_baseline_documentation() {
    // R0074-0078: gate the documentation print behind opt-in env var
    // so default test runs stay quiet.
    if std::env::var_os("UA_PRINT_PERF_BASELINE").is_none() {
        return;
    }
    // Document the performance requirements from SC-010
    println!("=== SC-010 Performance Requirement ===");
    println!("Target: Extraction within 20% of native 7zip performance");
    println!("Measurement: Extraction throughput (MB/s) on identical hardware");
    println!();
    println!("Factors affecting performance:");
    println!("- Archive format (ZIP, 7z, RAR have different backends)");
    println!("- Compression level (higher compression = more CPU)");
    println!("- I/O subsystem (SSD vs HDD)");
    println!("- File sizes (many small files vs few large files)");
    println!();
    println!("Note: Full benchmark suite available in benches/extraction.rs");
    println!("Run with: cargo bench --bench extraction");
}

#[test]
fn test_throughput_calculation() {
    // Verify throughput calculation is correct
    let size_bytes: u64 = 100 * 1024 * 1024; // 100MB
    let duration_secs = 2.0;

    let throughput_mbps = (size_bytes as f64 / (1024.0 * 1024.0)) / duration_secs;

    assert!(
        (throughput_mbps - 50.0).abs() < 0.01,
        "Throughput calculation: expected 50 MB/s, got {}",
        throughput_mbps
    );
}

#[test]
fn test_performance_ratio_calculation() {
    // Test that ratio calculation works correctly
    let native_time = Duration::from_secs(10);
    let unified_time = Duration::from_secs(12);

    let ratio = unified_time.as_secs_f64() / native_time.as_secs_f64();

    // 12s / 10s = 1.2 (exactly 20% slower)
    assert!(
        (ratio - 1.2).abs() < 0.01,
        "Ratio should be 1.2, got {}",
        ratio
    );

    // This is exactly at the SC-010 threshold (within 20%)
    assert!(ratio <= 1.2, "Ratio {} exceeds 20% threshold", ratio);
}
