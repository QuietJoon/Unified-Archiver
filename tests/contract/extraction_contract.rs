//! Contract tests for extraction operations
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/extraction.md:
//! 1. Extract from each supported format
//! 2. Verify extracted files match originals (byte-for-byte)
//! 3. Test password-protected extraction
//! 4. Test progress callbacks
//! 5. Test memory bounds
//! 6. Test performance

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use unified_archive::{Archive, ArchiveFormat, CompressionOptions, ExtractionOptions};

// ── Contract 1: Extract from each supported format ──

#[test]
fn contract_extract_all_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "ZIP extraction should succeed: {:?}",
        result.err()
    );
}

#[test]
fn contract_extract_all_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "7z extraction should succeed: {:?}",
        result.err()
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_extract_all_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "RAR extraction should succeed: {:?}",
        result.err()
    );
}

#[test]
fn contract_extract_all_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "TAR extraction should succeed: {:?}",
        result.err()
    );
}

#[test]
fn contract_extract_all_tar_gz() {
    let archive = Archive::open(fixture("test.tar.gz")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "TAR.GZ extraction should succeed: {:?}",
        result.err()
    );
}

#[test]
fn contract_extract_all_tar_bz2() {
    let archive = Archive::open(fixture("test.tar.bz2")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "TAR.BZ2 extraction should succeed: {:?}",
        result.err()
    );
}

#[test]
fn contract_extract_all_tar_xz() {
    let archive = Archive::open(fixture("test.tar.xz")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "TAR.XZ extraction should succeed: {:?}",
        result.err()
    );
}

// ── Contract 2: Verify extracted files match originals (byte-for-byte) ──

#[test]
fn contract_extract_to_memory_matches_original() {
    // Create an archive with known content, then verify extraction matches
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("roundtrip.zip");
    let original_content = b"Hello, this is a contract test for byte-for-byte verification!";

    // Create
    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive
        .add_file_from_data("contract_test.txt", original_content)
        .unwrap();
    archive.finish().unwrap();

    // Extract and verify
    let archive = Archive::open(&archive_path).unwrap();
    let extracted = archive.extract_to_memory("contract_test.txt").unwrap();
    assert_eq!(
        extracted, original_content,
        "Extracted content must match original byte-for-byte"
    );
}

#[test]
fn contract_extract_file_matches_original() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("roundtrip_file.zip");
    let original = b"File content for extract_file contract test";

    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("verify.txt", original).unwrap();
    archive.finish().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let extract_dir = temp.path().join("extracted");
    archive
        .extract_file(
            "verify.txt",
            common::default_extraction_options(extract_dir.clone()),
        )
        .unwrap();

    let extracted = std::fs::read(extract_dir.join("verify.txt")).unwrap();
    assert_eq!(extracted, original, "Extracted file must match original");
}

#[test]
fn contract_extract_multiple_files_match() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("multi.zip");

    let files: Vec<(&str, &[u8])> = vec![
        ("a.txt", b"content A"),
        ("subdir/b.txt", b"content B in subdir"),
        ("c.bin", &[0u8, 1, 2, 3, 255, 254, 253]),
    ];

    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    for (name, data) in &files {
        archive.add_file_from_data(name, data).unwrap();
    }
    archive.finish().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    for (name, expected) in &files {
        let extracted = archive.extract_to_memory(name).unwrap();
        assert_eq!(&extracted, expected, "File '{}' content mismatch", name);
    }
}

// ── Contract 3: Test password-protected extraction ──

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_extract_encrypted_rar_with_password() {
    // test_encrypted_data.rar has data encryption only (headers readable), password "test123"
    let archive = Archive::open_encrypted(fixture("test_encrypted_data.rar"), "test123").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = ExtractionOptions {
        password: Some("test123".to_string().into()),
        ..common::default_extraction_options(temp.path().to_path_buf())
    };
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "Encrypted RAR extraction with correct password should succeed: {:?}",
        result.err()
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_extract_encrypted_without_password_fails() {
    let archive = Archive::open(fixture("test_encrypted.rar")).unwrap();
    let result = archive.extract_to_memory("test_file.txt");
    assert!(
        result.is_err(),
        "Extracting encrypted file without password should fail"
    );
}

// ── Contract 4: Progress callbacks ──
// Note: extract_all_with_progress is not implemented; progress is via ExtractionOptions

#[test]
fn contract_extraction_options_accept_progress_callback() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_clone = call_count.clone();

    let options = ExtractionOptions {
        progress: Some(Box::new(move |current: u64, total: Option<u64>| {
            count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Some(t) = total {
                assert!(current <= t || t == 0, "current should not exceed total");
            }
            std::ops::ControlFlow::Continue(())
        })),
        ..common::default_extraction_options(temp.path().to_path_buf())
    };
    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "Extraction with progress callback should succeed: {:?}",
        result.err()
    );
    assert!(
        call_count.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "ZIP backend must invoke progress callback at least once"
    );
}

// ── Contract 5: Memory bounds ──

#[test]
fn contract_extract_to_memory_returns_correct_size() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();

    // OI-0056-010 vacuity floor: a listing with no sized entries would make
    // the loop below assert nothing at all.
    assert!(
        entries.iter().any(|e| e.size.is_some()),
        "test.zip must list at least one entry carrying a declared size: {:?}",
        entries
            .iter()
            .map(|e| (&e.path, e.size))
            .collect::<Vec<_>>()
    );
    for entry in entries.iter().filter(|e| e.size.is_some()) {
        let data = archive.extract_to_memory(&entry.path).unwrap();
        if let Some(expected_size) = entry.size {
            assert_eq!(
                data.len() as u64,
                expected_size,
                "Extracted size for '{}' should match reported size",
                entry.path
            );
        }
    }
}

// ── Contract 6: Performance ──

#[test]
fn contract_extraction_completes_in_reasonable_time() {
    use std::time::Instant;

    let archive = Archive::open(fixture("test.zip")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let start = Instant::now();
    let options = common::default_extraction_options(temp.path().to_path_buf());
    archive.extract_all(options).unwrap();
    let elapsed = start.elapsed();

    // Small test archive should extract in well under 5 seconds
    assert!(
        elapsed.as_secs() < 5,
        "Extraction took too long: {:?}",
        elapsed
    );
}

// ── Extra: extract_filtered contract ──

#[test]
fn contract_extract_filtered_only_matching() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("filtered.zip");

    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("include.txt", b"yes").unwrap();
    archive.add_file_from_data("exclude.dat", b"no").unwrap();
    archive
        .add_file_from_data("also_include.txt", b"yes too")
        .unwrap();
    archive.finish().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let extract_dir = temp.path().join("filtered_out");
    archive
        .extract_filtered(
            |e| e.path.ends_with(".txt"),
            common::default_extraction_options(extract_dir.clone()),
        )
        .unwrap();

    assert!(extract_dir.join("include.txt").exists());
    assert!(extract_dir.join("also_include.txt").exists());
    assert!(
        !extract_dir.join("exclude.dat").exists(),
        "Filtered-out file should not be extracted"
    );
}
