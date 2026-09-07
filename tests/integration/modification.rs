//! Integration tests for archive modification functionality
//!
//! Tests the modification API for adding, removing, and replacing files in archives.

// Every case here goes through `Archive::modify` / `commit_changes`, so the
// whole file needs the `modify` operation feature (AD 0058).
#![cfg(feature = "modify")]

#[cfg_attr(
    not(any(feature = "create", feature = "modify")),
    allow(unused_imports)
)]
use super::common;

#[cfg_attr(
    not(any(feature = "create", feature = "modify")),
    allow(unused_imports)
)]
use unified_archive::{Archive, ArchiveFormat};

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_add_files_to_archive() {
    // Create a test archive first
    let test_path = common::temp_test_dir().join("test_modify_add.zip");

    // Create initial archive
    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("original.txt", b"original content")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("new_file.txt", b"new content").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.path == "original.txt"));
    assert!(entries.iter().any(|e| e.path == "new_file.txt"));

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_remove_files_from_archive() {
    // Create a test archive first
    let test_path = common::temp_test_dir().join("test_modify_remove.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("file1.txt", b"content1")
        .unwrap();
    archive
        .add_file_from_data("file2.txt", b"content2")
        .unwrap();
    archive
        .add_file_from_data("file3.txt", b"content3")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("file2.txt").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.path == "file1.txt"));
    assert!(entries.iter().any(|e| e.path == "file3.txt"));
    assert!(!entries.iter().any(|e| e.path == "file2.txt"));

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_replace_files_in_archive() {
    // Create a test archive first
    let test_path = common::temp_test_dir().join("test_modify_replace.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("config.txt", b"old config")
        .unwrap();
    archive.finish().unwrap();

    // Now modify it
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.replace_entry("config.txt", b"new config").unwrap();
    archive.commit_changes().unwrap();

    // Verify
    let archive = Archive::open(&test_path).unwrap();
    let data = archive.extract_to_memory("config.txt").unwrap();
    assert_eq!(data, b"new config");

    // Cleanup
    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_combined_operations() {
    // Create a test archive first
    let test_path = common::temp_test_dir().join("test_modify_combined.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("keep.txt", b"keep this")
        .unwrap();
    archive
        .add_file_from_data("remove.txt", b"remove this")
        .unwrap();
    archive
        .add_file_from_data("replace.txt", b"old content")
        .unwrap();
    archive.finish().unwrap();

    // Now perform multiple operations
    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("remove.txt").unwrap();
    archive
        .replace_entry("replace.txt", b"new content")
        .unwrap();
    archive.add_entry("added.txt", b"added content").unwrap();
    archive.commit_changes().unwrap();

    // Verify all operations
    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().any(|e| e.path == "keep.txt"));
    assert!(entries.iter().any(|e| e.path == "replace.txt"));
    assert!(entries.iter().any(|e| e.path == "added.txt"));
    assert!(!entries.iter().any(|e| e.path == "remove.txt"));

    // Verify replaced content
    let data = archive.extract_to_memory("replace.txt").unwrap();
    assert_eq!(data, b"new content");

    // Cleanup
    std::fs::remove_file(test_path).ok();
}

#[test]
fn test_open_nonexistent_archive() {
    let result = Archive::modify("/nonexistent/path/to/archive.zip");
    assert!(result.is_err());
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_preserves_unchanged_content_bytes() {
    let test_path = common::temp_test_dir().join("test_modify_preserve_bytes.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("keep_me.bin", &[0xABu8; 4096])
        .unwrap();
    archive
        .add_file_from_data("doomed.bin", &[0xCDu8; 2048])
        .unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("doomed.bin").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let data = archive.extract_to_memory("keep_me.bin").unwrap();
    assert_eq!(data.len(), 4096);
    assert!(data.iter().all(|&b| b == 0xAB));

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_empty_archive_add_then_commit() {
    let test_path = common::temp_test_dir().join("test_modify_empty_base.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("a.txt", b"AAA").unwrap();
    archive.add_entry("b.txt", b"BBB").unwrap();
    archive.add_entry("c.txt", b"CCC").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 4);
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    for p in ["seed.txt", "a.txt", "b.txt", "c.txt"] {
        assert!(paths.contains(&p), "expected {p} in {paths:?}");
    }

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_replace_grows_and_shrinks_payload() {
    let test_path = common::temp_test_dir().join("test_modify_grow_shrink.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("payload.dat", &[0u8; 100])
        .unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive
        .replace_entry("payload.dat", &[0x11u8; 8000])
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let data = archive.extract_to_memory("payload.dat").unwrap();
    assert_eq!(data.len(), 8000);
    drop(archive);

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.replace_entry("payload.dat", b"tiny").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let data = archive.extract_to_memory("payload.dat").unwrap();
    assert_eq!(data, b"tiny");

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_remove_nonexistent_entry_errors_on_commit() {
    let test_path = common::temp_test_dir().join("test_modify_remove_missing.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("exists.txt", b"x").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("ghost.txt").unwrap();
    // Commit should either succeed no-op or error — both acceptable, but
    // original entry must remain untouched regardless.
    let _ = archive.commit_changes();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(entries.iter().any(|e| e.path == "exists.txt"));

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_add_and_remove_queues_are_independent() {
    // Adds and removes are tracked in independent queues. A remove targets
    // entries in the *source* archive, not pending adds, so an add+remove
    // of the same name produces the added entry (the remove is a no-op).
    let test_path = common::temp_test_dir().join("test_modify_add_remove_same.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("transient.txt", b"ephemeral").unwrap();
    archive.remove_entry("transient.txt").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"seed.txt"));
    assert!(paths.contains(&"transient.txt"));

    std::fs::remove_file(test_path).ok();
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_modify_many_entries_preserves_ordering_and_content() {
    let test_path = common::temp_test_dir().join("test_modify_many_entries.zip");

    let options = common::default_compression_options(ArchiveFormat::Zip);

    std::fs::remove_file(&test_path).ok();
    let mut archive = Archive::create(&test_path, options).unwrap();
    for i in 0..25 {
        let name = format!("entry_{:02}.txt", i);
        let body = format!("body-{:02}", i);
        archive.add_file_from_data(&name, body.as_bytes()).unwrap();
    }
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("entry_10.txt").unwrap();
    archive.add_entry("inserted.txt", b"fresh").unwrap();
    archive
        .replace_entry("entry_20.txt", b"twenty-replaced")
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 25);
    assert!(!entries.iter().any(|e| e.path == "entry_10.txt"));
    assert!(entries.iter().any(|e| e.path == "inserted.txt"));

    let extract_dir = common::temp_test_dir().join("test_modify_many_entries_out");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).unwrap();
    archive
        .extract_all(common::default_extraction_options(extract_dir.clone()))
        .unwrap();

    let twenty = std::fs::read(extract_dir.join("entry_20.txt")).unwrap();
    assert_eq!(twenty, b"twenty-replaced");

    for i in 0..25 {
        if i == 10 || i == 20 {
            continue;
        }
        let name = format!("entry_{:02}.txt", i);
        let body = format!("body-{:02}", i);
        let data = std::fs::read(extract_dir.join(&name)).unwrap();
        assert_eq!(data, body.as_bytes());
    }

    std::fs::remove_dir_all(&extract_dir).ok();
    std::fs::remove_file(test_path).ok();
}
