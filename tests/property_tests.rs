//! Property-based tests for archive operations
//!
//! Uses proptest to verify invariants hold across many random inputs.
//! Tests compression ratios, entry count consistency, and CRC32 verification.

use proptest::prelude::*;
use std::path::PathBuf;
use unified_archive::{Archive, ArchiveFormat, EntryType, StreamBound};

/// Helper to get test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The fixtures whose backend this build can actually read.
///
/// This whole file used to be `rar-support`-gated — strategies included — so
/// it "compiled to nothing" in any build without RAR, taking every ZIP
/// property with it. The strategies draw from the compiled-in backends
/// instead, so each profile exercises exactly what it can read and ZIP is
/// never lost (AD 0058 format features). ZIP is unconditional, so the list is
/// never empty and `select` always has an arm.
fn available_fixtures() -> Vec<(&'static str, ArchiveFormat)> {
    let mut v = vec![("test.zip", ArchiveFormat::Zip)];
    if cfg!(feature = "rar-support") {
        v.push(("test.rar", ArchiveFormat::Rar5));
    }
    if cfg!(feature = "sevenzip") {
        v.push(("test.7z", ArchiveFormat::SevenZip));
    }
    v
}

/// Strategy: pick one of the available test fixture files
fn fixture_file_strategy() -> impl Strategy<Value = (&'static str, ArchiveFormat)> {
    proptest::sample::select(available_fixtures())
}

/// Strategy: pick a fixture filename
fn fixture_name_strategy() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(
        available_fixtures()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
    )
}

// ── Property-based tests using proptest macros ──

proptest! {
    /// Property: Format detection is deterministic across random iteration counts
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_format_detection_deterministic(
        (filename, expected_format) in fixture_file_strategy(),
        iterations in 1u32..20,
    ) {
        let path = fixtures_dir().join(filename);

        for _ in 0..iterations {
            let archive = Archive::open(&path)
                .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

            prop_assert_eq!(
                archive.format(),
                expected_format,
                "Format detection should be deterministic for {}",
                filename
            );
        }
    }

    /// Property: Entry count is consistent regardless of how many times list_files is called
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_entry_count_consistent(
        filename in fixture_name_strategy(),
        iterations in 2u32..30,
    ) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let first_count = archive.entry_count()
            .map_err(|e| TestCaseError::fail(format!("Failed to get entry count: {}", e)))?;

        for _ in 0..iterations {
            let count = archive.entry_count()
                .map_err(|e| TestCaseError::fail(format!("Failed to get entry count: {}", e)))?;
            prop_assert_eq!(count, first_count, "Entry count inconsistent for {}", filename);
        }

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("Failed to list files: {}", e)))?;
        prop_assert_eq!(
            entries.len(), first_count,
            "list_files().len() should match entry_count() for {}", filename
        );
    }

    /// Property: Entry paths are always unique within any archive
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_entry_paths_unique(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("Failed to list files: {}", e)))?;

        let mut paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        let original_len = paths.len();
        paths.sort();
        paths.dedup();

        prop_assert_eq!(paths.len(), original_len, "Duplicate paths found in {}", filename);
    }

    /// Property: All file entries in RAR archives have CRC32 checksums
        #[cfg(feature = "rar-support")]
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_rar_file_entries_have_crc32(_dummy in 0u8..1) {
        let path = fixtures_dir().join("test.rar");
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open test.rar: {}", e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("Failed to list files: {}", e)))?;

        for entry in entries {
            if entry.entry_type == EntryType::File {
                prop_assert!(
                    entry.crc32.is_some(),
                    "File entry '{}' in test.rar should have CRC32",
                    entry.path
                );
            }
        }
    }

    /// Property: Extracted data size matches the entry's reported size
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_extracted_size_matches_entry(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let file_to_extract = "test_file.txt";

        let archive1 = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let entry = archive1.find_entry(file_to_extract)
            .map_err(|e| TestCaseError::fail(format!("find_entry failed: {}", e)))?
            .ok_or_else(|| TestCaseError::fail(format!("{} not found in {}", file_to_extract, filename)))?;

        let archive2 = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let data = archive2.extract_to_memory(file_to_extract)
            .map_err(|e| TestCaseError::fail(format!("extract_to_memory failed: {}", e)))?;

        if let Some(expected_size) = entry.size {
            prop_assert_eq!(
                data.len() as u64, expected_size,
                "Extracted size mismatch for {} in {}", file_to_extract, filename
            );
        }
    }

    /// Property: Streaming extraction produces identical data to memory extraction
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_streaming_equals_memory_extraction(filename in fixture_name_strategy()) {
        use std::io::Read;

        let path = fixtures_dir().join(filename);
        let file_to_extract = "test_file.txt";

        let archive1 = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;
        let memory_data = archive1.extract_to_memory(file_to_extract)
            .map_err(|e| TestCaseError::fail(format!("extract_to_memory failed: {}", e)))?;

        let archive2 = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;
        let mut stream = archive2.extract_to_stream(file_to_extract, StreamBound::Unbounded)
            .map_err(|e| TestCaseError::fail(format!("extract_to_stream failed: {}", e)))?;

        let mut stream_data = Vec::new();
        stream.read_to_end(&mut stream_data)
            .map_err(|e| TestCaseError::fail(format!("read_to_end failed: {}", e)))?;

        prop_assert_eq!(
            stream_data, memory_data,
            "Streaming != memory extraction for {} in {}", file_to_extract, filename
        );
    }

    /// Property: Archive opening is idempotent (N random opens all succeed)
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_archive_open_idempotent(
        filename in fixture_name_strategy(),
        iterations in 1u32..50,
    ) {
        let path = fixtures_dir().join(filename);

        for iteration in 0..iterations {
            let result = Archive::open(&path);
            prop_assert!(
                result.is_ok(),
                "Opening {} failed on iteration {}: {:?}",
                filename, iteration, result.err()
            );
        }
    }

    /// Property: Validation reports are consistent across multiple runs
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_validation_consistent(
        filename in fixture_name_strategy(),
        iterations in 2u32..15,
    ) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let first_report = archive.validate_integrity()
            .map_err(|e| TestCaseError::fail(format!("validate_integrity failed: {}", e)))?;

        for _ in 0..iterations {
            let report = archive.validate_integrity()
                .map_err(|e| TestCaseError::fail(format!("validate_integrity failed: {}", e)))?;

            prop_assert_eq!(
                report.total_entries, first_report.total_entries,
                "Total entries inconsistent for {}", filename
            );
            prop_assert_eq!(
                report.validated, first_report.validated,
                "Validated count inconsistent for {}", filename
            );
            prop_assert_eq!(
                report.failed.len(), first_report.failed.len(),
                "Failed count inconsistent for {}", filename
            );
        }
    }

    /// Property: find_entry is equivalent to filtering list_files
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_find_entry_equivalent_to_filter(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let search_path = "test_file.txt";

        let found_via_find = archive.find_entry(search_path)
            .map_err(|e| TestCaseError::fail(format!("find_entry failed: {}", e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("list_files failed: {}", e)))?;
        let found_via_filter = entries.iter().find(|e| e.path == search_path);

        match (found_via_find, found_via_filter) {
            (Some(entry1), Some(entry2)) => {
                prop_assert_eq!(
                    &entry1.path, &entry2.path,
                    "find_entry and filter found different entries in {}", filename
                );
            }
            (None, None) => { /* Both agree - ok */ }
            _ => {
                return Err(TestCaseError::fail(format!(
                    "find_entry and filter disagree on existence of {} in {}",
                    search_path, filename
                )));
            }
        }
    }

    /// Property: Entry IDs are sequential starting from 0
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_entry_ids_sequential(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("list_files failed: {}", e)))?;

        for (i, entry) in entries.iter().enumerate() {
            prop_assert_eq!(
                entry.id, i,
                "Entry ID should be sequential (expected {}, got {}) in {}",
                i, entry.id, filename
            );
        }
    }

    /// Property: File entries have non-negative sizes
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_file_entries_have_sizes(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("list_files failed: {}", e)))?;

        for entry in entries {
            if entry.entry_type == EntryType::File {
                prop_assert!(
                    entry.size.is_some(),
                    "File entry '{}' should have a size in {}",
                    entry.path, filename
                );
            }
        }
    }

    /// Property: Compression ratio, when present, is non-negative
        #[test]
    #[serial_test::file_serial(rar)]
    fn prop_compression_ratio_non_negative(filename in fixture_name_strategy()) {
        let path = fixtures_dir().join(filename);
        let archive = Archive::open(&path)
            .map_err(|e| TestCaseError::fail(format!("Failed to open {}: {}", filename, e)))?;

        let entries = archive.list_files()
            .map_err(|e| TestCaseError::fail(format!("list_files failed: {}", e)))?;

        for entry in entries {
            if let Some(ratio) = entry.compression_fraction() {
                prop_assert!(
                    ratio >= 0.0,
                    "Compression ratio for '{}' should be non-negative, got {} in {}",
                    entry.path, ratio, filename
                );
            }
        }
    }
}
