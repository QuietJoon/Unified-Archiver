//! Performance baseline comparison tests (SC-010, T127b)
//!
//! SC-010 asks that extraction land within 20% of a native archiver, i.e.
//! `unified / native <= 1.2`.
//!
//! ## Why this file is split in two (OI-0056-010)
//!
//! It used to hold one lane that short-circuited unless
//! `UA_PRINT_PERF_BASELINE` was set and, when it did run, `println!`ed
//! `✓ PASS` / `△ ACCEPTABLE` / `✗ NEEDS IMPROVEMENT` without asserting
//! anything — so a real regression could never turn the suite red, and on a
//! default run the lane passed having measured nothing at all. It also built
//! its fixture with the external `zip` CLI and skipped when that was
//! missing.
//!
//! The split settles both halves of OI-0056-010:
//!
//! * [`perf_baseline_fixture_extracts_completely`] runs in the default lane.
//!   The fixture is built in-process, so there is no CLI to be missing, and
//!   it asserts the extraction is complete and byte-exact. No wall clock is
//!   involved, so it cannot flake.
//! * [`perf_baseline_ratio_meets_sc010`] is the controlled comparison lane.
//!   It is `#[ignore]`d because it shells out to a native archiver and is
//!   wall-clock sensitive, but when it runs it **asserts** the SC-010 ratio
//!   and fails loudly if no native archiver is installed. It never prints a
//!   verdict for a human to ignore.
//!
//! Run the comparison lane with:
//!
//! ```text
//! TMPDIR=/Volumes/Temp/claude cargo test --test integration_tests --all-features \
//!     -- --ignored --test-threads=1 integration::performance_baseline
//! ```
//!
//! Long-term destination is `benches/` (R0074-0070).

use super::common;
use super::common::command_exists;

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// SC-010's stated ceiling: unified-archive within 20% of native.
const SC010_MAX_RATIO: f64 = 1.2;

/// Members in the perf fixture, one per MiB.
const FIXTURE_MEMBERS: usize = 10;

/// One 1 MiB member payload: varied enough not to be trivially
/// compressible, deterministic so the fixture is reproducible.
fn member_payload() -> Vec<u8> {
    (0..=255u8)
        .cycle()
        .enumerate()
        .map(|(idx, b)| b.wrapping_add((idx % 17) as u8))
        .take(1024 * 1024)
        .collect()
}

fn member_name(i: usize) -> String {
    format!("perf_test_{:04}.bin", i)
}

/// Build the DEFLATE-compressed perf fixture in-process.
///
/// OI-0056-010: replaces `Command::new("zip").args(["-r", "-5"])`. The old
/// builder returned an `io::Error` the caller turned into a skip; this one
/// cannot fail without panicking, so the lane can never quietly vanish.
fn create_performance_test_archive(archive_path: &Path) {
    let payload = member_payload();
    let names: Vec<String> = (0..FIXTURE_MEMBERS).map(member_name).collect();
    let members: Vec<common::ZipMember<'_>> = names
        .iter()
        .map(|n| common::ZipMember::File(n.as_str(), &payload))
        .collect();
    common::build_zip(archive_path, &members, false);
}

/// Measure extraction time using a native archiver CLI.
///
/// Returns `None` only when the tool is absent or refuses the archive; the
/// caller decides whether that is a skip or a failure, and in this file it
/// is always a failure.
fn measure_7z_extraction(archive_path: &Path, output_dir: &Path) -> Option<Duration> {
    let cli = if command_exists("7zz") {
        "7zz"
    } else if command_exists("7z") {
        "7z"
    } else {
        return None;
    };

    fs::create_dir_all(output_dir).ok()?;

    let start = Instant::now();
    let output = Command::new(cli)
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

/// Measure extraction time using unified-archive. Panics rather than
/// returning `None` on failure: a failed extraction is a defect, not a
/// reason to stop measuring.
fn measure_unified_archive_extraction(archive_path: &Path, output_dir: &Path) -> Duration {
    fs::create_dir_all(output_dir).expect("create unified output dir");

    let archive = unified_archive::Archive::open(archive_path).expect("open perf fixture");

    let start = Instant::now();
    let options = common::default_extraction_options(output_dir.to_path_buf());
    archive
        .extract_all(options)
        .expect("unified-archive must extract the perf fixture");
    start.elapsed()
}

/// Default-lane half: the perf fixture extracts completely and byte-exactly.
///
/// No wall clock, no external process — this is the part of the old lane
/// that was worth keeping every run, made unconditional.
#[test]
fn perf_baseline_fixture_extracts_completely() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("perf_test_10mb.zip");
    create_performance_test_archive(&archive_path);

    let out = temp_dir.path().join("ua_output");
    measure_unified_archive_extraction(&archive_path, &out);

    let expected = member_payload();
    for i in 0..FIXTURE_MEMBERS {
        let path = out.join(member_name(i));
        let got = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(
            got.len(),
            expected.len(),
            "{} extracted at the wrong length",
            path.display()
        );
        assert!(
            got == expected,
            "{} extracted with the wrong bytes",
            path.display()
        );
    }

    // Vacuity floor: the loop above proves nothing if the fixture is empty.
    assert_eq!(
        fs::read_dir(&out)
            .expect("read unified output dir")
            .filter_map(|e| e.ok())
            .count(),
        FIXTURE_MEMBERS,
        "the extraction must produce exactly the members the fixture carries"
    );
}

/// Controlled-lane half: the SC-010 ratio is **asserted**, not printed.
///
/// `#[ignore]`d because it shells out to a native archiver and is
/// wall-clock sensitive. Run it with:
///
/// ```text
/// TMPDIR=/Volumes/Temp/claude cargo test --test integration_tests --all-features \
///     -- --ignored --test-threads=1 integration::performance_baseline
/// ```
///
/// A missing native archiver fails this lane rather than skipping it: the
/// whole point of the lane is the comparison, so having nothing to compare
/// against is a setup error, not a pass.
#[test]
#[ignore = "wall-clock comparison against a native archiver CLI; run with \
            `TMPDIR=/Volumes/Temp/claude cargo test --test integration_tests --all-features \
            -- --ignored --test-threads=1 integration::performance_baseline`"]
fn perf_baseline_ratio_meets_sc010() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("perf_test_10mb.zip");
    create_performance_test_archive(&archive_path);

    let ua_time =
        measure_unified_archive_extraction(&archive_path, &temp_dir.path().join("ua_output"));

    let native_output = temp_dir.path().join("native_output");
    let native_time = measure_7z_extraction(&archive_path, &native_output)
        .or_else(|| measure_unzip_extraction(&archive_path, &native_output))
        .expect(
            "no native archiver available to compare against — install `7zz`/`7z` \
             (`brew install sevenzip`, `apt install p7zip-full`) or `unzip`. This lane exists \
             only for the comparison, so an absent baseline is a setup error, not a pass.",
        );

    let ratio = ua_time.as_secs_f64() / native_time.as_secs_f64();
    assert!(
        ratio <= SC010_MAX_RATIO,
        "SC-010 regression: unified-archive took {ua_time:?} against the native archiver's \
         {native_time:?}, a ratio of {ratio:.2}x (ceiling {SC010_MAX_RATIO}x)"
    );
}

// R0074-0078 gated `test_performance_baseline_documentation` behind
// `UA_PRINT_PERF_BASELINE`; OI-0056-010 removed it. It only `println!`ed
// prose about SC-010 and asserted nothing, so it could not fail in either
// configuration — the same class R0079-0046 removed from
// `streaming_memory.rs`. The prose it printed now lives in this module's
// own documentation, where it cannot masquerade as coverage.

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

    // This is exactly at the SC-010 threshold, and it is the same constant
    // `perf_baseline_ratio_meets_sc010` compares against — so a silent edit
    // to the ceiling shows up here too.
    assert!(
        ratio <= SC010_MAX_RATIO,
        "Ratio {ratio} exceeds the SC-010 threshold {SC010_MAX_RATIO}"
    );
}
