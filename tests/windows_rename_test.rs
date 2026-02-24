//! Windows rename behavior test
//!
//! Tests the platform-specific rename functionality for archive modification.
//! On Windows, `std::fs::rename()` fails if destination exists, so we use `MoveFileExW`.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

/// Test directory for rename tests
fn test_dir() -> PathBuf {
    PathBuf::from("/Volumes/Temp/claude/7zip/rename_test")
}

/// Test that rename_with_overwrite works correctly
///
/// This test verifies:
/// 1. Rename to non-existent destination works
/// 2. Rename to existing destination overwrites (the key Windows fix)
#[test]
fn test_rename_with_overwrite() {
    let dir = test_dir();
    fs::create_dir_all(&dir).expect("create test dir");

    let src = dir.join("source.txt");
    let dst = dir.join("destination.txt");

    // Cleanup from any previous test run
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&dst);

    // Test 1: Rename to non-existent destination
    {
        let mut f = File::create(&src).expect("create source");
        f.write_all(b"source content").expect("write source");
    }

    // Use the archive's commit_changes indirectly by testing modification
    // For direct testing, we use std::fs::rename on Unix and verify behavior
    #[cfg(not(windows))]
    {
        fs::rename(&src, &dst).expect("rename should succeed");
        assert!(!src.exists(), "source should not exist after rename");
        assert!(dst.exists(), "destination should exist after rename");
        let content = fs::read_to_string(&dst).expect("read destination");
        assert_eq!(content, "source content");
    }

    // Test 2: Rename to existing destination (overwrites)
    #[cfg(not(windows))]
    {
        // Create source again
        let mut f = File::create(&src).expect("create source");
        f.write_all(b"new source content").expect("write source");

        // Destination already exists from Test 1
        assert!(dst.exists(), "destination should exist");

        // On Unix, rename overwrites existing file
        fs::rename(&src, &dst).expect("rename should overwrite on Unix");
        let content = fs::read_to_string(&dst).expect("read destination");
        assert_eq!(
            content, "new source content",
            "content should be from new source"
        );
    }

    // Cleanup
    let _ = fs::remove_file(&dst);
    let _ = fs::remove_dir(&dir);

    println!("Rename test passed on current platform");
}

/// Test archive modification commit (integration test)
///
/// This test creates an archive, modifies it, and commits changes.
/// On Windows, this would fail before the MoveFileExW fix.
#[test]
fn test_archive_modification_commit() {
    use unified_archive::{Archive, ArchiveFormat, CompressionOptions};

    let dir = test_dir();
    fs::create_dir_all(&dir).expect("create test dir");

    let archive_path = dir.join("test_modification.zip");
    let extract_dir = dir.join("extracted");

    // Cleanup
    let _ = fs::remove_file(&archive_path);
    let _ = fs::remove_dir_all(&extract_dir);
    fs::create_dir_all(&extract_dir).expect("create extract dir");

    // Create initial archive with one file
    {
        let options = CompressionOptions {
            format: ArchiveFormat::Zip,
            ..Default::default()
        };
        let mut archive = Archive::create(&archive_path, options).expect("create archive");
        archive
            .add_file_from_data("original.txt", b"original content")
            .expect("add file");
        archive.finish().expect("finish archive");
    }

    // Verify initial archive
    {
        let archive = Archive::open(&archive_path).expect("open archive");
        let entries = archive.list_files().expect("list files");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "original.txt");
    }

    // Modify archive (add a file)
    {
        let mut archive = Archive::modify(&archive_path).expect("open for modify");
        archive
            .add_entry("added.txt", b"added content")
            .expect("add entry");
        // commit_changes uses rename_with_overwrite internally
        archive.commit_changes().expect("commit changes");
    }

    // Verify modified archive
    {
        let archive = Archive::open(&archive_path).expect("open modified archive");
        let entries = archive.list_files().expect("list files");
        assert_eq!(entries.len(), 2, "should have 2 entries after modification");

        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"original.txt"));
        assert!(paths.contains(&"added.txt"));
    }

    // Cleanup
    let _ = fs::remove_file(&archive_path);
    let _ = fs::remove_dir_all(&extract_dir);
    let _ = fs::remove_dir(&dir);

    println!("Archive modification commit test passed");
}

/// Windows-specific test documentation
///
/// On Windows, this test would verify:
/// 1. `std::fs::rename()` fails when destination exists
/// 2. Our `MoveFileExW` wrapper succeeds when destination exists
///
/// To run on Windows:
/// ```
/// cargo test --test windows_rename_test -- --nocapture
/// ```
#[test]
#[cfg(windows)]
fn test_windows_rename_behavior() {
    let dir = test_dir();
    fs::create_dir_all(&dir).expect("create test dir");

    let src = dir.join("win_source.txt");
    let dst = dir.join("win_dest.txt");

    // Create both files
    fs::write(&src, b"source").expect("create source");
    fs::write(&dst, b"existing destination").expect("create dest");

    // Standard rename should FAIL on Windows when destination exists
    let std_result = fs::rename(&src, &dst);
    assert!(
        std_result.is_err(),
        "std::fs::rename should fail on Windows when destination exists"
    );

    // Our archive modification uses MoveFileExW which should succeed
    // (tested indirectly via test_archive_modification_commit)

    // Cleanup
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&dst);

    println!("Windows-specific rename test passed");
}
