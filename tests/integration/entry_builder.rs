//! Regression tests for R0075-0078 — `ArchiveEntryBuilder` API.
//!
//! Pins the new typed-construction surface that supersedes the
//! public-fields constructors `ArchiveEntry::new` /
//! `ArchiveEntry::directory`. Both legacy constructors stay callable
//! during the v0.3 deprecation cycle but emit
//! `#[deprecated]` warnings for external callers.

use unified_archive::{ArchiveEntry, EntryType};

#[test]
fn archive_entry_builder_constructs_valid_state() {
    let entry = ArchiveEntry::file("test.txt", 0)
        .size(100)
        .crc32(0x12345678)
        .build();
    assert_eq!(entry.path, "test.txt");
    assert_eq!(entry.size, Some(100));
    assert_eq!(entry.crc32, Some(0x12345678));
    assert!(matches!(entry.entry_type, EntryType::File));
}

#[test]
fn archive_entry_try_file_rejects_empty_path() {
    let result = ArchiveEntry::try_file("", 0);
    assert!(result.is_err(), "Empty path must be rejected by try_file");
}

#[test]
fn archive_entry_try_file_accepts_non_empty_path() {
    let result = ArchiveEntry::try_file("ok.txt", 5);
    assert!(result.is_ok());
    let entry = result.unwrap().build();
    assert_eq!(entry.path, "ok.txt");
    assert_eq!(entry.id, 5);
}

#[test]
fn archive_entry_directory_is_size_none() {
    let entry = ArchiveEntry::dir_at("dir/", 0).build();
    assert_eq!(entry.size, None);
    assert_eq!(entry.compressed_size, None);
    assert!(matches!(entry.entry_type, EntryType::Directory));
}

#[test]
fn archive_entry_builder_permissions_masks_to_unix_bits() {
    // The builder's permissions setter masks off non-Unix bits so the
    // contract from R0075-0079 (permissions & !0o7777 == 0) cannot be
    // violated through the typed-construction path.
    let entry = ArchiveEntry::file("f.bin", 0)
        .permissions(0o100755) // S_IFREG bits set
        .build();
    assert_eq!(entry.permissions, Some(0o755));
}

#[test]
fn archive_entry_builder_chains_all_setters() {
    let entry = ArchiveEntry::file("complete.bin", 7)
        .size(1024)
        .compressed_size(512)
        .crc32(0xCAFEBABE)
        .permissions(0o644)
        .raw_path(vec![1, 2, 3])
        .comment("test comment".to_string())
        .build();
    assert_eq!(entry.size, Some(1024));
    assert_eq!(entry.compressed_size, Some(512));
    assert_eq!(entry.crc32, Some(0xCAFEBABE));
    assert_eq!(entry.permissions, Some(0o644));
    assert_eq!(entry.raw_path, Some(vec![1, 2, 3]));
    assert_eq!(entry.comment, Some("test comment".to_string()));
}
