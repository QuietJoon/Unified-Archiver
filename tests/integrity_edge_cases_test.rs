// Additional edge case tests for integrity validation
// Critical scenarios that could break in production
//
// OI-0056-010: the fixture-building tests here used to shell out to the
// `zip` CLI and `eprintln!`-skip when it was absent, so nine lanes reported
// success having validated nothing on a host without the CLI. Fixtures are
// now built in-process through `common::build_zip`, which means there is no
// external prerequisite left to be missing and no lane that can skip. The
// R0079-0032 rule that every step must assert still holds.

#[path = "common/mod.rs"]
mod common;

use common::ZipMember;
use std::fs;
use std::path::Path;
use unified_archive::Archive;

/// Build a DEFLATE-compressed ZIP holding `members`, then assert it opens.
///
/// Replaces the old `run_zip` CLI wrapper. Entry names are the flat,
/// basename-only shape the CLI's `-j` produced.
fn build_zip(path: &Path, members: &[ZipMember<'_>]) {
    common::build_zip(path, members, false);
    assert!(
        path.is_file(),
        "fixture zip {} was not written",
        path.display()
    );
}

// ============================================================================
// FILE TYPE TESTS
// ============================================================================

#[test]
fn test_validation_binary_files() {
    // Test validation of archives containing binary data
    let temp_dir = common::temp_test_dir();

    // Binary payload with every byte value represented.
    let binary_data: Vec<u8> = (0..=255u8).cycle().take(1024).collect();

    let zip_path = temp_dir.join("binary.zip");
    build_zip(&zip_path, &[ZipMember::File("binary.bin", &binary_data)]);

    let archive = Archive::open(&zip_path).expect("Open binary ZIP");
    let report = archive.validate_integrity().expect("Validate binary ZIP");

    assert_eq!(report.failed.len(), 0, "Binary files should validate");
    assert_eq!(report.validated, 1, "Should validate the one binary member");

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_text_files_various_encodings() {
    // Test text files with different line endings and content
    let temp_dir = common::temp_test_dir();

    let zip_path = temp_dir.join("text.zip");
    build_zip(
        &zip_path,
        &[
            // Unix line endings
            ZipMember::File("unix.txt", b"Line 1\nLine 2\nLine 3\n"),
            // Windows line endings
            ZipMember::File("windows.txt", b"Line 1\r\nLine 2\r\nLine 3\r\n"),
            // Mixed content, including a NUL and a non-UTF-8 byte
            ZipMember::File("mixed.txt", b"Text\n\x00Binary\xFF\nMore text\n"),
        ],
    );

    let archive = Archive::open(&zip_path).expect("Open text ZIP");
    let report = archive.validate_integrity().expect("Validate text ZIP");

    assert_eq!(report.failed.len(), 0, "Text files should validate");
    assert_eq!(report.validated, 3, "Should validate 3 text files");

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_mixed_file_types() {
    // Test archive with mixed binary, text and empty members
    let temp_dir = common::temp_test_dir();

    let binary: Vec<u8> = (0..256).map(|i| i as u8).collect();

    let zip_path = temp_dir.join("mixed.zip");
    build_zip(
        &zip_path,
        &[
            ZipMember::File("readme.txt", b"This is a text file\n"),
            ZipMember::File("data.bin", &binary),
            ZipMember::File("empty.dat", b""),
        ],
    );

    let archive = Archive::open(&zip_path).expect("Open mixed ZIP");
    let report = archive.validate_integrity().expect("Validate mixed ZIP");

    assert_eq!(report.failed.len(), 0, "Mixed files should all validate");
    assert_eq!(report.validated, 3, "Should validate all 3 files");

    common::cleanup(&temp_dir);
}

// ============================================================================
// DIRECTORY STRUCTURE TESTS
// ============================================================================

#[test]
fn test_validation_nested_directories() {
    // Test validation with deeply nested directory structures
    let temp_dir = common::temp_test_dir();

    let zip_path = temp_dir.join("nested.zip");
    build_zip(
        &zip_path,
        &[
            ZipMember::Dir("dir1/"),
            ZipMember::Dir("dir1/dir2/"),
            ZipMember::Dir("dir1/dir2/dir3/"),
            ZipMember::File("dir1/dir2/dir3/deep_file.txt", b"Deeply nested file\n"),
            ZipMember::File("root_file.txt", b"Root level file\n"),
        ],
    );

    let archive = Archive::open(&zip_path).expect("Open nested ZIP");
    let report = archive.validate_integrity().expect("Validate nested ZIP");

    assert_eq!(report.failed.len(), 0, "Nested files should validate");
    assert!(
        report.validated >= 2,
        "Should validate files in nested structure, got {}",
        report.validated
    );
    // The nested member really is present under its full path, so the
    // "nested" in this lane's name is load-bearing.
    let listed = archive.list_files().expect("list nested ZIP");
    assert!(
        listed
            .iter()
            .any(|e| e.path == "dir1/dir2/dir3/deep_file.txt"),
        "nested member missing from the listing: {:?}",
        listed.iter().map(|e| &e.path).collect::<Vec<_>>()
    );

    common::cleanup(&temp_dir);
}

// ============================================================================
// FILENAME TESTS
// ============================================================================

#[test]
fn test_validation_long_filename() {
    // Test validation with very long filenames (near system limits)
    let temp_dir = common::temp_test_dir();

    // 200 characters, below the 255-byte per-component limit.
    let long_name = "a".repeat(200) + ".txt";

    let zip_path = temp_dir.join("longname.zip");
    build_zip(
        &zip_path,
        &[ZipMember::File(&long_name, b"File with long name\n")],
    );

    let archive = Archive::open(&zip_path).expect("Open long filename ZIP");
    let report = archive
        .validate_integrity()
        .expect("Validate long filename ZIP");

    assert_eq!(report.failed.len(), 0, "Long filename should validate");
    assert_eq!(report.validated, 1, "the long-named member must validate");
    assert!(
        archive
            .list_files()
            .expect("list long filename ZIP")
            .iter()
            .any(|e| e.path == long_name),
        "the 200-character name must survive the round trip intact"
    );

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_unicode_filename() {
    // Test validation with Unicode/UTF-8 filenames
    let temp_dir = common::temp_test_dir();

    let unicode_names = [
        "文件.txt",          // Chinese
        "файл.txt",          // Cyrillic
        "αρχείο.txt",        // Greek
        "ファイル.txt",      // Japanese
        "emoji_😀_test.txt", // Emoji
    ];

    let members: Vec<ZipMember<'_>> = unicode_names
        .iter()
        .map(|n| ZipMember::File(n, b"Unicode filename test\n"))
        .collect();

    let zip_path = temp_dir.join("unicode.zip");
    build_zip(&zip_path, &members);

    let archive = Archive::open(&zip_path).expect("Open Unicode ZIP");
    let report = archive.validate_integrity().expect("Validate Unicode ZIP");

    assert_eq!(report.failed.len(), 0, "Unicode filenames should validate");
    assert_eq!(
        report.validated,
        unicode_names.len(),
        "every Unicode-named member must validate"
    );
    // The names themselves must survive, not just the payloads — the old
    // lane asserted nothing about them.
    let listed = archive.list_files().expect("list Unicode ZIP");
    for name in &unicode_names {
        assert!(
            listed.iter().any(|e| e.path == *name),
            "{name} missing from the listing: {:?}",
            listed.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    common::cleanup(&temp_dir);
}

// ============================================================================
// CORRUPTION PERSISTENCE TESTS
// ============================================================================

#[test]
fn test_validation_repeated_on_corrupted() {
    // Test that repeated validation calls on corrupted archive remain consistent
    let archive =
        Archive::open("tests/fixtures/corrupted_crc.zip").expect("Open corrupted-CRC fixture");

    let mut results = Vec::new();

    // Validate 3 times
    for i in 0..3 {
        let report = archive
            .validate_integrity()
            .unwrap_or_else(|e| panic!("Validation {} should complete: {}", i, e));
        println!(
            "Validation {}: {} validated, {} failed",
            i,
            report.validated,
            report.failed.len()
        );
        results.push((report.validated, report.failed.clone()));
    }

    // Results should be consistent across calls
    let first_failed_count = results[0].1.len();
    assert!(
        first_failed_count > 0,
        "Corrupted-CRC fixture should report failed entries"
    );
    for (i, (_, failed)) in results.iter().enumerate() {
        assert_eq!(
            failed.len(),
            first_failed_count,
            "Validation {} should have same failed count",
            i
        );
    }
    println!(
        "✓ Corrupted archive validation consistency: {} calls",
        results.len()
    );
}

// ============================================================================
// MEMORY STRESS TESTS
// ============================================================================

#[test]
fn test_validation_large_file_stress() {
    // Test validation of archive with larger file (5MB)
    let temp_dir = common::temp_test_dir();

    let large_data: Vec<u8> = (0..5_000_000).map(|i| (i % 256) as u8).collect();

    let zip_path = temp_dir.join("large_5mb.zip");
    build_zip(&zip_path, &[ZipMember::File("large_5mb.bin", &large_data)]);

    let archive = Archive::open(&zip_path).expect("Open large ZIP");
    let report = archive.validate_integrity().expect("Validate large ZIP");

    assert_eq!(report.failed.len(), 0, "Large file (5MB) should validate");
    assert_eq!(report.validated, 1, "the 5 MB member must validate");
    assert_eq!(
        archive
            .list_files()
            .expect("list large ZIP")
            .iter()
            .find(|e| e.path == "large_5mb.bin")
            .and_then(|e| e.size),
        Some(large_data.len() as u64),
        "the declared size must match the payload actually stored"
    );

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_many_small_files_stress() {
    // Test validation with many small files (stress file count)
    let temp_dir = common::temp_test_dir();

    const COUNT: usize = 50;
    let names: Vec<String> = (0..COUNT).map(|i| format!("file_{:03}.txt", i)).collect();
    let payloads: Vec<String> = (0..COUNT)
        .map(|i| format!("Content of file {}\n", i))
        .collect();
    let members: Vec<ZipMember<'_>> = names
        .iter()
        .zip(payloads.iter())
        .map(|(n, p)| ZipMember::File(n.as_str(), p.as_bytes()))
        .collect();

    let zip_path = temp_dir.join("many_files.zip");
    build_zip(&zip_path, &members);

    let archive = Archive::open(&zip_path).expect("Open many files ZIP");
    let report = archive
        .validate_integrity()
        .expect("Validate many files ZIP");

    assert_eq!(report.failed.len(), 0, "All 50 files should validate");
    assert_eq!(report.validated, COUNT, "Should validate exactly 50 files");

    common::cleanup(&temp_dir);
}

// ============================================================================
// SPECIFIC FORMAT EDGE CASES
// ============================================================================

#[test]
fn test_validation_zip_compression_methods() {
    // Test ZIP archives with different compression methods (store, deflate)
    let temp_dir = common::temp_test_dir();

    // Compressible enough that DEFLATE genuinely differs from STORE, so the
    // two arms below are not the same archive under two names.
    let payload = b"Test data for compression\n".repeat(64);

    // STORE (the old `-0`).
    let zip_store = temp_dir.join("stored.zip");
    common::build_zip(&zip_store, &[ZipMember::File("test.txt", &payload)], true);
    let stored = Archive::open(&zip_store).expect("Open stored ZIP");
    let report = stored.validate_integrity().expect("Validate stored ZIP");
    assert_eq!(report.failed.len(), 0, "Stored ZIP should validate");
    assert_eq!(report.validated, 1, "stored member must validate");

    // DEFLATE (the old `-9`).
    let zip_deflate = temp_dir.join("deflated.zip");
    common::build_zip(
        &zip_deflate,
        &[ZipMember::File("test.txt", &payload)],
        false,
    );
    let deflated = Archive::open(&zip_deflate).expect("Open deflated ZIP");
    let report = deflated
        .validate_integrity()
        .expect("Validate deflated ZIP");
    assert_eq!(report.failed.len(), 0, "Deflated ZIP should validate");
    assert_eq!(report.validated, 1, "deflated member must validate");

    // The two really are different methods, not one method twice: the
    // DEFLATE archive must be the smaller file on this payload.
    let stored_len = fs::metadata(&zip_store).expect("stat stored").len();
    let deflated_len = fs::metadata(&zip_deflate).expect("stat deflated").len();
    assert!(
        deflated_len < stored_len,
        "DEFLATE ({deflated_len} bytes) should beat STORE ({stored_len} bytes) on a repetitive \
         payload; if it does not, both arms are exercising the same method"
    );

    // Both round-trip to the same bytes.
    for archive in [&stored, &deflated] {
        assert_eq!(
            archive
                .extract_to_memory("test.txt")
                .expect("extract test.txt"),
            payload,
            "payload must survive both compression methods"
        );
    }

    common::cleanup(&temp_dir);
}

#[test]
fn test_validation_error_recovery() {
    // Test that validation errors don't leave archive in bad state
    let archive = Archive::open("tests/fixtures/batch_test.zip").expect("Open test ZIP");

    // First validation
    let report1 = archive.validate_integrity().expect("First validation");

    // Try to cause an error by validating nonexistent archive
    let _ = Archive::open("nonexistent.zip");

    // Second validation on original archive should still work
    let report2 = archive.validate_integrity().expect("Second validation");

    assert_eq!(
        report1.validated, report2.validated,
        "Validation results should be consistent"
    );
    assert_eq!(
        report1.failed, report2.failed,
        "Failed lists should be identical"
    );

    println!("✓ Validation error recovery: results consistent");
}
