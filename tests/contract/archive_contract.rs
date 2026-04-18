//! Contract tests for Archive handle operations
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/archive.md:
//! 1. Open valid archive: each supported format
//! 2. Open invalid file: expect ArchiveError::Format
//! 3. Open non-existent file: expect ArchiveError::Io
//! 4. Create archive: each writable format
//! 5. Modify unsupported format: expect ArchiveError::Unsupported
//! 6. Concurrent opens: multiple Archive::open() to same file succeeds
//! 7. Close error handling: handle Err from close()
//! 8. Drop behavior: archive auto-closes on scope exit

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use unified_archive::{Archive, ArchiveError, ArchiveFormat, CompressionOptions};

// ── Contract 1: Open valid archive for each supported format ──

#[test]
fn contract_open_valid_zip() {
    let archive = Archive::open(fixture("test.zip")).expect("Should open valid ZIP");
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[test]
fn contract_open_valid_7z() {
    let archive = Archive::open(fixture("test.7z")).expect("Should open valid 7z");
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_open_valid_rar() {
    let archive = Archive::open(fixture("test.rar")).expect("Should open valid RAR");
    // test.rar is actually RAR5 format (created by RAR 7.12+)
    assert!(
        archive.format() == ArchiveFormat::Rar || archive.format() == ArchiveFormat::Rar5,
        "Expected RAR or RAR5 format, got {:?}",
        archive.format()
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_open_valid_rar5() {
    let archive = Archive::open(fixture("test_rar5.rar")).expect("Should open valid RAR5");
    assert_eq!(archive.format(), ArchiveFormat::Rar5);
}

#[test]
fn contract_open_valid_tar() {
    let archive = Archive::open(fixture("test.tar")).expect("Should open valid TAR");
    assert_eq!(archive.format(), ArchiveFormat::Tar);
}

#[test]
fn contract_open_valid_tar_gz() {
    let archive = Archive::open(fixture("test.tar.gz")).expect("Should open valid TAR.GZ");
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

#[test]
fn contract_open_valid_tar_bz2() {
    let archive = Archive::open(fixture("test.tar.bz2")).expect("Should open valid TAR.BZ2");
    assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
}

#[test]
fn contract_open_valid_tar_xz() {
    let archive = Archive::open(fixture("test.tar.xz")).expect("Should open valid TAR.XZ");
    assert_eq!(archive.format(), ArchiveFormat::TarXz);
}

// ── Contract 2: Open invalid file ──

#[test]
fn contract_open_invalid_file_returns_format_error() {
    // Use a text file as input — not a valid archive
    let result = Archive::open(fixture("test_file.txt"));
    assert!(
        result.is_err(),
        "Opening a text file as archive should fail"
    );
    let err = result.err().unwrap();
    // Should be a format-related error
    let msg = format!("{}", err);
    assert!(
        matches!(err, ArchiveError::Format { .. })
            || msg.contains("format")
            || msg.contains("detect")
            || msg.contains("unsupported"),
        "Expected format error, got: {msg}"
    );
}

// ── Contract 3: Open non-existent file ──

#[test]
fn contract_open_nonexistent_file_returns_io_error() {
    let result = Archive::open("/nonexistent/path/to/archive.zip");
    assert!(result.is_err(), "Opening nonexistent file should fail");
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    assert!(
        matches!(err, ArchiveError::Io { .. })
            || msg.contains("No such file")
            || msg.contains("not found")
            || msg.contains("does not exist"),
        "Expected I/O error for nonexistent file, got: {msg}"
    );
}

// ── Contract 4: Create archive for each writable format ──

#[test]
fn contract_create_zip_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("created.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&path, options).expect("Should create ZIP");
    archive
        .add_file_from_data("hello.txt", b"hello")
        .expect("Should add file");
    archive.finish().expect("Should finish");
    assert!(path.exists(), "ZIP file should exist after creation");

    // Verify by reading back
    let archive = Archive::open(&path).expect("Should open created ZIP");
    let entries = archive.list_files().expect("Should list files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "hello.txt");
}

#[test]
fn contract_create_tar_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("created.tar");
    let options = CompressionOptions::new(ArchiveFormat::Tar);
    let mut archive = Archive::create(&path, options).expect("Should create TAR");
    archive
        .add_file_from_data("hello.txt", b"hello")
        .expect("Should add file");
    archive.finish().expect("Should finish");
    assert!(path.exists(), "TAR file should exist after creation");
}

#[test]
fn contract_create_tar_gz_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("created.tar.gz");
    let options = CompressionOptions::new(ArchiveFormat::TarGzip);
    let mut archive = Archive::create(&path, options).expect("Should create TAR.GZ");
    archive
        .add_file_from_data("hello.txt", b"hello")
        .expect("Should add file");
    archive.finish().expect("Should finish");
    assert!(path.exists());
}

#[test]
fn contract_create_tar_bz2_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("created.tar.bz2");
    let options = CompressionOptions::new(ArchiveFormat::TarBzip2);
    let mut archive = Archive::create(&path, options).expect("Should create TAR.BZ2");
    archive
        .add_file_from_data("hello.txt", b"hello")
        .expect("Should add file");
    archive.finish().expect("Should finish");
    assert!(path.exists());
}

#[test]
fn contract_create_tar_xz_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("created.tar.xz");
    let options = CompressionOptions::new(ArchiveFormat::TarXz);
    let mut archive = Archive::create(&path, options).expect("Should create TAR.XZ");
    archive
        .add_file_from_data("hello.txt", b"hello")
        .expect("Should add file");
    archive.finish().expect("Should finish");
    assert!(path.exists());
}

// ── Contract 5: Modify unsupported format ──

#[test]
#[serial_test::file_serial(rar)]
fn contract_modify_rar_returns_unsupported() {
    let result = Archive::modify(fixture("test.rar"));
    assert!(result.is_err(), "Modifying RAR should fail");
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    assert!(
        msg.contains("read-only") || msg.contains("modify") || msg.contains("RAR"),
        "Error should explain RAR doesn't support modification: {msg}"
    );
}

#[test]
fn contract_modify_tar_returns_unsupported() {
    // TAR doesn't support modification (only ZIP and 7z do via can_modify())
    let result = Archive::modify(fixture("test.tar"));
    assert!(result.is_err(), "Modifying TAR should fail");
}

// ── Contract 6: Concurrent opens to same file ──

#[test]
fn contract_concurrent_opens_same_file() {
    // Multiple Archive::open() on the same file should all succeed
    let path = fixture("test.zip");
    let archive1 = Archive::open(&path).expect("First open should succeed");
    let archive2 = Archive::open(&path).expect("Second open should succeed");
    let archive3 = Archive::open(&path).expect("Third open should succeed");

    // All should be able to list files independently
    let entries1 = archive1.list_files().expect("Should list files");
    let entries2 = archive2.list_files().expect("Should list files");
    let entries3 = archive3.list_files().expect("Should list files");

    assert_eq!(entries1.len(), entries2.len());
    assert_eq!(entries2.len(), entries3.len());
}

#[test]
fn contract_concurrent_opens_different_threads() {
    use std::thread;

    let path = fixture("test.zip");
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            thread::spawn(move || {
                let archive = Archive::open(&p).expect("Should open in thread");
                let entries = archive.list_files().expect("Should list files");
                entries.len()
            })
        })
        .collect();

    let counts: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    // All threads should see the same number of entries
    assert!(counts.windows(2).all(|w| w[0] == w[1]));
}

// ── Contract 7: Close error handling ──

#[test]
fn contract_close_valid_archive() {
    let archive = Archive::open(fixture("test.zip")).expect("Should open");
    let result = archive.close();
    assert!(result.is_ok(), "Closing valid archive should succeed");
}

#[test]
fn contract_finish_created_archive() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("to_finish.zip");
    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&path, options).unwrap();
    archive.add_file_from_data("f.txt", b"data").unwrap();
    let result = archive.finish();
    assert!(result.is_ok(), "Finishing created archive should succeed");
}

// ── Contract 8: Drop behavior ──

#[test]
fn contract_drop_auto_closes_archive() {
    let path = fixture("test.zip");
    {
        let _archive = Archive::open(&path).expect("Should open");
        // archive goes out of scope here — Drop should auto-close
    }
    // If we get here, Drop didn't panic
    // Re-open to verify file is still valid
    let archive = Archive::open(&path).expect("Should open after previous drop");
    let entries = archive.list_files().expect("Should list files");
    assert!(!entries.is_empty());
}

#[test]
fn contract_drop_created_archive_without_finish() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("dropped.zip");
    {
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("f.txt", b"data").unwrap();
        // Dropped without calling finish()
    }
    // Should not panic
}
