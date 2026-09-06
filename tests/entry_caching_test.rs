//! Tests for Phase 1: Entry caching functionality
//!
//! Verifies that Archive::list_files() returns cached entries on repeated calls
//! (zero-cost repeated access via OnceCell)

use unified_archive::Archive;

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_entry_caching_same_pointer() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // First call: initializes cache
    let entries1 = archive
        .list_files()
        .expect("Failed to list files (first call)");

    // Second call: should return cached slice (same pointer)
    let entries2 = archive
        .list_files()
        .expect("Failed to list files (second call)");

    // Verify same slice pointer (zero-cost repeated access)
    assert!(
        std::ptr::eq(entries1.as_ptr(), entries2.as_ptr()),
        "Entry cache should return same slice pointer on repeated calls"
    );

    // Verify same length
    assert_eq!(entries1.len(), entries2.len());
}

#[test]
fn test_entry_caching_zip() {
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    // Multiple calls should all return cached slice
    let entries1 = archive.list_files().expect("Failed to list files (call 1)");
    let entries2 = archive.list_files().expect("Failed to list files (call 2)");
    let entries3 = archive.list_files().expect("Failed to list files (call 3)");

    // All should point to same cached data
    assert!(std::ptr::eq(entries1.as_ptr(), entries2.as_ptr()));
    assert!(std::ptr::eq(entries2.as_ptr(), entries3.as_ptr()));
}

#[cfg(feature = "sevenzip")]
#[test]
fn test_entry_caching_7z() {
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    // Multiple calls should all return cached slice
    let entries1 = archive.list_files().expect("Failed to list files (call 1)");
    let entries2 = archive.list_files().expect("Failed to list files (call 2)");

    assert!(
        std::ptr::eq(entries1.as_ptr(), entries2.as_ptr()),
        "7z archive caching should work identically to other formats"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_dependent_methods_use_cache() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // Call list_files first to initialize cache
    let entries = archive.list_files().expect("Failed to list files");

    // These methods should use the cached entries (not re-read from backend)
    let count = archive.entry_count().expect("Failed to get entry count");
    assert_eq!(count, entries.len());

    let found = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry");
    assert!(found.is_some());

    // Verify cache still returns same pointer
    let entries_again = archive.list_files().expect("Failed to list files again");
    assert!(std::ptr::eq(entries.as_ptr(), entries_again.as_ptr()));
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_caching_resolves_unrar_limitation() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // Before Phase 1: This pattern required reopening archive due to UnRAR sequential iteration
    // After Phase 1: All operations use cached entries, no reopening needed

    let entries = archive.list_files().expect("Failed to list files");
    assert_eq!(entries.len(), 1);

    let count = archive.entry_count().expect("Failed to get count");
    assert_eq!(count, 1);

    let found = archive.find_entry("test_file.txt").expect("Failed to find");
    assert!(found.is_some());

    let report = archive.validate_integrity().expect("Failed to validate");
    assert_eq!(report.total_entries, 1);

    // All operations above used the same cached entries (verified by no errors)
    // Before Phase 1, this would have required reopening archive 4 times
}
