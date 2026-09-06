//! Contract tests for inspection operations
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/inspection.md:
//! 1. List files from each supported format
//! 2. Verify metadata consistency (size, crc32, modified time)
//! 3. Verify enhanced metadata (compression_ratio, is_encrypted, created/accessed times)
//! 4. Verify caching (repeated calls return stable metadata without re-populating)
//! 5. Validate integrity detects corrupted files
//! 6. Performance: <1s for listing

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use unified_archive::Archive;

// ── Contract 1: List files from each supported format ──

#[test]
fn contract_list_files_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "ZIP should contain entries");
    for entry in entries {
        assert!(!entry.path.is_empty(), "Entry path should not be empty");
    }
}

#[cfg(feature = "sevenzip")]
#[test]
fn contract_list_files_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "7z should contain entries");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_list_files_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "RAR should contain entries");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_list_files_rar5() {
    let archive = Archive::open(fixture("test_rar5.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "RAR5 should contain entries");
}

#[cfg(feature = "libarchive")]
#[test]
fn contract_list_files_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR should contain entries");
}

#[cfg(feature = "libarchive")]
#[test]
fn contract_list_files_tar_gz() {
    let archive = Archive::open(fixture("test.tar.gz")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.GZ should contain entries");
}

#[cfg(feature = "libarchive")]
#[test]
fn contract_list_files_tar_bz2() {
    let archive = Archive::open(fixture("test.tar.bz2")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.BZ2 should contain entries");
}

#[cfg(feature = "libarchive")]
#[test]
fn contract_list_files_tar_xz() {
    let archive = Archive::open(fixture("test.tar.xz")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty(), "TAR.XZ should contain entries");
}

// ── Contract 2: Verify metadata consistency ──

#[test]
fn contract_metadata_size_present_for_files() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    for entry in entries {
        if entry.entry_type == unified_archive::EntryType::File {
            assert!(
                entry.size.is_some(),
                "File entry '{}' should have a size",
                entry.path
            );
        }
    }
}

#[test]
fn contract_metadata_crc32_present_for_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // ZIP format generally provides CRC32 for all entries
    let has_crc = entries
        .iter()
        .filter(|e| e.entry_type == unified_archive::EntryType::File)
        .any(|e| e.crc32.is_some());
    assert!(has_crc, "ZIP files should have CRC32 checksums");
}

#[test]
fn contract_metadata_modified_time_present() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // Most formats provide modification time
    let has_mtime = entries.iter().any(|e| e.modified.is_some());
    assert!(has_mtime, "Entries should have modification timestamps");
}

#[test]
fn contract_metadata_entry_ids_sequential() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // OI-0056-010 vacuity floor: an empty listing satisfies the loop below
    // without executing it once.
    assert!(
        !entries.is_empty(),
        "test.zip must list entries before their ids are asserted"
    );
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.id, i,
            "Entry IDs should be sequential 0-based, entry '{}' has id {} at index {}",
            entry.path, entry.id, i
        );
    }
}

#[test]
fn contract_metadata_consistent_across_formats() {
    // The same content archived in different formats should have consistent metadata
    let zip = Archive::open(fixture("test.zip")).unwrap();
    let zip_entries = zip.list_files().unwrap();

    // Only the TAR half needs libarchive. Gating the whole test would take the
    // ZIP size assertions below out of the minimal profile and leave the lane
    // green while doing it (AD 0058).
    #[cfg(feature = "libarchive")]
    {
        let tar = Archive::open(fixture("test.tar")).unwrap();
        let tar_entries = tar.list_files().unwrap();
        assert!(!tar_entries.is_empty());
    }

    // Both should contain files (may differ in exact entries due to format differences)
    assert!(!zip_entries.is_empty());

    // File entries should have non-None sizes
    for entry in zip_entries
        .iter()
        .filter(|e| e.entry_type == unified_archive::EntryType::File)
    {
        assert!(
            entry.size.is_some(),
            "ZIP entry '{}' missing size",
            entry.path
        );
    }
}

// ── Contract 3: Verify enhanced metadata ──

#[test]
fn contract_enhanced_metadata_entry_type() {
    use unified_archive::EntryType;

    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();

    // OI-0056-010: this lane used to be a `match` in which every arm —
    // including a `_ => {}` wildcard — did nothing, over a collection that
    // was never asserted non-empty. It could not fail for any input, so it
    // was a green light with no bulb behind it.
    //
    // The R0076-0079 intent it was defending (`EntryType` is
    // `#[non_exhaustive]`; adding a variant must not force a same-PR
    // contract edit) is preserved by scoping the claim to this fixture: a
    // plain ZIP of regular files and directories. A new `EntryType` variant
    // still compiles and still passes here; a ZIP backend that starts
    // reporting `test.zip`'s members as links, or as an unknown category,
    // does not.
    assert!(
        !entries.is_empty(),
        "test.zip must list entries before their types are asserted"
    );
    for entry in entries {
        assert!(
            matches!(entry.entry_type, EntryType::File | EntryType::Directory),
            "test.zip carries only regular files and directories; '{}' came back as {:?}",
            entry.path,
            entry.entry_type
        );
    }
    assert!(
        entries.iter().any(|e| e.entry_type == EntryType::File),
        "at least one member must classify as a regular file: {:?}",
        entries
            .iter()
            .map(|e| (&e.path, e.entry_type))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contract_enhanced_metadata_is_encrypted_field() {
    // Non-encrypted archive should report is_encrypted = false
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    // OI-0056-010 vacuity floor.
    assert!(
        !entries.is_empty(),
        "test.zip must list entries before their encryption flag is asserted"
    );
    for entry in entries {
        assert!(
            !entry.is_encrypted,
            "Non-encrypted archive entries should have is_encrypted=false"
        );
    }
}

// ── Contract 4: Verify caching (repeated calls return stable metadata) ──

#[test]
fn contract_list_files_caching_stable() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries1 = archive.list_files().unwrap();
    let first_snapshot: Vec<_> = entries1
        .iter()
        .map(|e| (e.path.clone(), e.size, e.crc32))
        .collect();

    // OI-0056-010 vacuity floor: two empty listings are trivially "stable",
    // and the `zip()` below would iterate zero times.
    assert!(
        !first_snapshot.is_empty(),
        "test.zip must list entries before cache stability is asserted"
    );

    let entries2 = archive.list_files().unwrap();
    assert_eq!(
        entries2.len(),
        first_snapshot.len(),
        "Cached list_files() must not change length between calls",
    );
    for (entry, (path, size, crc)) in entries2.iter().zip(first_snapshot.iter()) {
        assert_eq!(&entry.path, path, "Cached entry path must be stable");
        assert_eq!(entry.size, *size, "Cached entry size must be stable");
        assert_eq!(entry.crc32, *crc, "Cached entry crc32 must be stable");
    }
}

#[cfg(feature = "sevenzip")]
#[test]
fn contract_list_files_caching_same_length() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries1 = archive.list_files().unwrap();
    let entries2 = archive.list_files().unwrap();

    // OI-0056-010 vacuity floor: `0 == 0` would satisfy this too.
    assert!(
        !entries1.is_empty(),
        "test.7z must list entries for the length comparison to mean anything"
    );
    assert_eq!(entries1.len(), entries2.len());
}

#[test]
fn contract_entry_count_matches_list_files() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let count = archive.entry_count().unwrap();
    assert_eq!(
        entries.len(),
        count,
        "entry_count() should match list_files().len()"
    );
}

// ── Contract 5: Validate integrity detects corrupted files ──

#[test]
fn contract_validate_integrity_valid_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(
        report.failed.is_empty(),
        "Valid ZIP should pass integrity check. Failed entries: {:?}",
        report.failed
    );
}

#[test]
fn contract_validate_integrity_corrupted_zip() {
    let archive = Archive::open(fixture("corrupted_crc.zip"));
    match archive {
        Ok(archive) => {
            let report = archive.validate_integrity();
            match report {
                Ok(report) => {
                    // Corrupted archive should be detected
                    assert!(
                        !report.failed.is_empty() || report.validated < report.total_entries,
                        "Corrupted archive should fail integrity check"
                    );
                }
                Err(_) => {
                    // Error during validation is also acceptable for corrupted archives
                }
            }
        }
        Err(_) => {
            // If the archive can't even be opened, that's acceptable
        }
    }
}

#[cfg(feature = "sevenzip")]
#[test]
fn contract_validate_integrity_valid_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(
        report.failed.is_empty(),
        "Valid 7z should pass integrity check. Failed: {:?}",
        report.failed
    );
}

// ── Contract 6: Performance ──

// R0074-0071: contract suites should encode behavioral compatibility
// only. The performance assertion (`elapsed.as_secs() < 1`) is
// timing-dependent and belongs in a benchmark profile, not a contract
// suite, so it stays opt-in.
//
// OI-0056-010: the opt-in used to be an `UA_RUN_PERF_CONTRACT_TEST` env
// check with an early `return`, which reported the lane as *passed* on
// every default run — a green result for a check nobody performed.
// `#[ignore]` says the same thing honestly: the harness prints it as
// ignored, and the reason carries the command.
#[test]
#[ignore = "timing-dependent spot-check, not a behavioural contract (R0074-0071); run with \
            `TMPDIR=/Volumes/Temp/claude cargo test --test contract_tests --all-features -- \
            --ignored --test-threads=1 contract_list_files_performance`"]
fn contract_list_files_performance() {
    use std::time::Instant;

    let archive = Archive::open(fixture("test.zip")).unwrap();
    let start = Instant::now();
    let entries = archive.list_files().unwrap();
    let elapsed = start.elapsed();

    // Vacuity floor: timing an empty listing measures nothing.
    assert!(
        !entries.is_empty(),
        "test.zip must list entries for the timing below to mean anything"
    );
    assert!(
        elapsed.as_secs() < 1,
        "Listing files should complete in <1s, took: {:?}",
        elapsed
    );
}

#[test]
fn contract_find_entry_existing() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_path = &entries[0].path;

    let found = archive.find_entry(first_path).unwrap();
    assert!(
        found.is_some(),
        "Should find existing entry '{}'",
        first_path
    );
    assert_eq!(found.unwrap().path, *first_path);
}

#[test]
fn contract_find_entry_nonexistent() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let found = archive
        .find_entry("definitely_not_a_real_file.xyz")
        .unwrap();
    assert!(found.is_none(), "Should not find nonexistent entry");
}
