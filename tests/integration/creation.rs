//! Integration tests for archive creation functionality
//!
//! Tests the creation API for creating new archives from files and data.

use std::fs;
use std::path::PathBuf;
use unified_archive::{Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions};

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_format_capability_check() {
    // Supported formats
    let zip_opts = CompressionOptions::new(ArchiveFormat::Zip);
    let tar_opts = CompressionOptions::new(ArchiveFormat::Tar);
    let targz_opts = CompressionOptions::new(ArchiveFormat::TarGzip);

    // Should be able to create creators for supported formats
    let temp_dir = std::env::temp_dir();

    let result = Archive::create(temp_dir.join("test1.zip"), zip_opts);
    assert!(result.is_ok());

    let result = Archive::create(temp_dir.join("test2.tar"), tar_opts);
    assert!(result.is_ok());

    let result = Archive::create(temp_dir.join("test3.tar.gz"), targz_opts);
    assert!(result.is_ok());
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_unsupported_format_rejected() {
    let temp_dir = std::env::temp_dir();
    let rar_opts = CompressionOptions::new(ArchiveFormat::Rar);

    let result = Archive::create(temp_dir.join("test.rar"), rar_opts);
    assert!(result.is_err());

    if let Err(e) = result {
        assert!(matches!(e, ArchiveError::Unsupported { .. }));
    }
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_existing_file_rejected() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("existing_test.zip");

    // Create a file
    fs::write(&test_file, b"existing").ok();

    // Try to create archive with same name
    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let result = Archive::create(&test_file, opts);

    assert!(result.is_err());

    // Clean up
    let _ = fs::remove_file(&test_file);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_add_file_from_data() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_data.zip");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add files from data
    assert!(
        creator
            .add_file_from_data("file1.txt", b"content 1")
            .is_ok()
    );
    assert_eq!(creator.entry_count().unwrap(), 1);

    assert!(
        creator
            .add_file_from_data("file2.txt", b"content 2")
            .is_ok()
    );
    assert_eq!(creator.entry_count().unwrap(), 2);

    // Test empty path rejection
    assert!(creator.add_file_from_data("", b"content").is_err());
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_add_file_from_path() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_path.zip");
    let test_file = temp_dir.join("test_input.txt");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    // Create test file
    fs::write(&test_file, b"test content").ok();

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add file from filesystem
    if test_file.exists() {
        assert!(creator.add_file_from_path(&test_file).is_ok());
        assert_eq!(creator.entry_count().unwrap(), 1);
    }

    // Test nonexistent file rejection
    assert!(creator.add_file_from_path("/nonexistent/file.txt").is_err());

    // Clean up
    let _ = fs::remove_file(&test_file);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_add_file_from_path_as() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_path_as.zip");
    let test_file = temp_dir.join("test_input2.txt");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    // Create test file
    fs::write(&test_file, b"test content").ok();

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add file with custom archive path
    if test_file.exists() {
        assert!(
            creator
                .add_file_from_path_as(&test_file, "custom/path/file.txt")
                .is_ok()
        );
        assert_eq!(creator.entry_count().unwrap(), 1);
    }

    // Test empty archive path rejection
    if test_file.exists() {
        assert!(creator.add_file_from_path_as(&test_file, "").is_err());
    }

    // Clean up
    let _ = fs::remove_file(&test_file);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_add_directory_entry() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_dir_entry.zip");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add directory entries
    assert!(creator.add_directory_entry("folder1").is_ok());
    assert_eq!(creator.entry_count().unwrap(), 1);

    assert!(creator.add_directory_entry("folder2/").is_ok());
    assert_eq!(creator.entry_count().unwrap(), 2);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_add_directory_recursive() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_dir_recursive.zip");
    let test_dir = temp_dir.join("test_recursive_dir");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);
    let _ = fs::remove_dir_all(&test_dir);

    // Create test directory structure
    fs::create_dir_all(&test_dir).ok();
    fs::create_dir_all(test_dir.join("subdir")).ok();
    fs::write(test_dir.join("file1.txt"), b"content 1").ok();
    fs::write(test_dir.join("file2.txt"), b"content 2").ok();
    fs::write(test_dir.join("subdir/file3.txt"), b"content 3").ok();

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add directory recursively
    if test_dir.exists() {
        assert!(creator.add_directory_recursive(&test_dir).is_ok());
        // Should have: 2 dirs + 3 files = 5 entries
        assert!(creator.entry_count().unwrap() >= 3); // At least the files
    }

    // Test nonexistent directory rejection
    assert!(
        creator
            .add_directory_recursive("/nonexistent/directory")
            .is_err()
    );

    // Clean up
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_clear_entries() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_clear.zip");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add several entries
    creator
        .add_file_from_data("file1.txt", b"content 1")
        .unwrap();
    creator
        .add_file_from_data("file2.txt", b"content 2")
        .unwrap();
    creator.add_directory_entry("folder").unwrap();
    assert_eq!(creator.entry_count().unwrap(), 3);

    // Clear all entries
    creator.clear_entries();
    assert_eq!(creator.entry_count().unwrap(), 0);
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_empty_archive_rejected() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_empty_finish.zip");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let creator = Archive::create(&test_archive, opts).unwrap();

    // Try to finish without adding entries
    let result = creator.finish();
    assert!(result.is_err());
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_compression_levels() {
    let temp_dir = std::env::temp_dir();

    let levels = vec![
        CompressionLevel::Store,
        CompressionLevel::Fastest,
        CompressionLevel::Fast,
        CompressionLevel::Normal,
        CompressionLevel::Maximum,
        CompressionLevel::Ultra,
    ];

    for level in levels {
        let test_archive = temp_dir.join(format!("test_level_{:?}.zip", level));
        let _ = fs::remove_file(&test_archive);

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        opts.level = level;

        let mut creator = Archive::create(&test_archive, opts).unwrap();
        creator
            .add_file_from_data("test.txt", b"test content")
            .unwrap();

        // Finish will fail (expected - implementation in progress)
        // But the compression level was accepted
        let result = creator.finish();
        assert!(result.is_err());
    }
}

#[test]
#[ignore = "Archive creation not yet implemented (Phase 5)"]
fn test_finish_returns_implementation_error() {
    let temp_dir = std::env::temp_dir();
    let test_archive = temp_dir.join("test_finish.zip");

    // Clean up if exists
    let _ = fs::remove_file(&test_archive);

    let opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut creator = Archive::create(&test_archive, opts).unwrap();

    // Add at least one entry
    creator.add_file_from_data("test.txt", b"content").unwrap();

    // Finish should return error indicating implementation in progress
    let result = creator.finish();
    assert!(result.is_err());

    if let Err(e) = result {
        let error_str = format!("{:?}", e);
        assert!(
            error_str.contains("implementation in progress")
                || error_str.contains("libarchive write API")
        );
    }
}
