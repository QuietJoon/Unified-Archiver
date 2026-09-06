use super::*;
use crate::options::WritableFormat;
use crate::test_utils::fixture;

/// Copy a fixture into a freshly-named tempfile so parallel tests each
/// hold their own archive path — required now that `Archive::modify()`
/// acquires an advisory exclusive lock (MADR-0009, MADR-0016).
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn fixture_copy(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join(name);
    std::fs::copy(fixture(name), &dst).expect("copy fixture");
    (dir, dst)
}

// ── ModificationOptions tests ──

#[test]
fn test_modification_options_new_defaults() {
    let opts = ModificationOptions::new();
    assert!(opts.preserve_metadata);
    assert!(!opts.create_backup);
    assert_eq!(opts.backup_suffix, ".bak");
}

#[test]
fn test_modification_options_default_matches_new() {
    let from_new = ModificationOptions::new();
    let from_default = ModificationOptions::default();
    assert_eq!(from_new.preserve_metadata, from_default.preserve_metadata);
    assert_eq!(from_new.create_backup, from_default.create_backup);
    assert_eq!(from_new.backup_suffix, from_default.backup_suffix);
}

#[test]
fn test_modification_options_with_backup() {
    let opts = ModificationOptions::new().with_backup(".backup");
    assert!(opts.create_backup);
    assert_eq!(opts.backup_suffix, ".backup");
    // preserve_metadata should remain unchanged
    assert!(opts.preserve_metadata);
}

#[test]
fn test_modification_options_without_metadata_preservation() {
    let opts = ModificationOptions::new().without_metadata_preservation();
    assert!(!opts.preserve_metadata);
    // Other fields should remain unchanged
    assert!(!opts.create_backup);
    assert_eq!(opts.backup_suffix, ".bak");
}

#[test]
fn test_modification_options_chained_builders() {
    let opts = ModificationOptions::new()
        .with_backup(".orig")
        .without_metadata_preservation();
    assert!(opts.create_backup);
    assert_eq!(opts.backup_suffix, ".orig");
    assert!(!opts.preserve_metadata);
}

#[test]
fn test_modification_options_debug() {
    let opts = ModificationOptions::new();
    let debug = format!("{:?}", opts);
    assert!(debug.contains("ModificationOptions"));
    assert!(debug.contains("preserve_metadata"));
    assert!(debug.contains("create_backup"));
}

// ModificationOptions no longer implements Clone — `compression` can
// hold a non-cloneable progress callback. Callers that want to reuse
// options across commits build a fresh value from scratch.

// ── ModificationTracker tests ──

#[test]
fn test_modification_tracker_default_empty() {
    let tracker = ModificationTracker::default();
    assert!(tracker.removed.is_empty());
    assert!(tracker.added.is_empty());
}

#[test]
fn test_modification_tracker_add_entries() {
    let mut tracker = ModificationTracker::default();
    tracker.added.push((
        "file.txt".to_string(),
        crate::modification::EntrySource::Buffered(b"data".to_vec()),
    ));
    assert_eq!(tracker.added.len(), 1);
    assert_eq!(tracker.added[0].0, "file.txt");
    match &tracker.added[0].1 {
        crate::modification::EntrySource::Buffered(v) => assert_eq!(v, b"data"),
        _ => panic!("expected Buffered variant"),
    }
}

#[test]
fn test_modification_tracker_remove_entries() {
    let mut tracker = ModificationTracker::default();
    tracker.removed.insert(3);
    assert_eq!(tracker.removed.len(), 1);
    assert!(tracker.removed.contains(&3));
}

// ── Archive::modify() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_modify_zip_archive() {
    let (_td, _p) = fixture_copy("test.zip");
    let archive = Archive::modify(&_p);
    assert!(archive.is_ok());
    let archive = archive.unwrap();
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[cfg(all(feature = "sevenzip", feature = "libarchive"))]
#[test]
fn test_modify_7z_archive() {
    let (_td, _p) = fixture_copy("test.7z");
    let archive = Archive::modify(&_p);
    assert!(archive.is_ok());
    let archive = archive.unwrap();
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_second_modify_blocked_by_advisory_lock() {
    // Two handles on the SAME path: the second modify() must fail with
    // OperationBlocked while the first holds the advisory exclusive lock,
    // and succeed again once the first handle is dropped. Regression guard
    // for the lock dependency itself (fs2 → fs4 migration): contention must
    // surface as an error from the try-lock call, never as a silent
    // acquisition or an indefinite block.
    let (_td, path) = fixture_copy("test.zip");

    let first = Archive::modify(&path).expect("first modify() should acquire the lock");

    match Archive::modify(&path) {
        Err(ArchiveError::OperationBlocked { reason, .. }) => {
            assert!(
                reason.contains("Another Modify session holds the advisory lock"),
                "unexpected block reason: {reason}"
            );
        }
        Err(other) => panic!("second modify() must be OperationBlocked, got {other:?}"),
        Ok(_) => panic!("second modify() must be OperationBlocked, but it acquired the lock"),
    }

    drop(first);
    Archive::modify(&path).expect("modify() should reacquire after the first handle dropped");
}

#[test]
fn test_modify_rar_archive_fails() {
    let result = Archive::modify(fixture("test.rar"));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let msg = format!("{}", err);
    assert!(
        msg.contains("read-only") || msg.contains("modify") || msg.contains("RAR"),
        "Error should mention RAR limitation: {msg}"
    );
}

#[test]
fn test_modify_nonexistent_archive() {
    let result = Archive::modify("/nonexistent/path/archive.zip");
    assert!(result.is_err());
}

#[test]
fn test_modify_tar_archive() {
    // TAR doesn't support can_modify() (only ZIP and 7z do)
    let result = Archive::modify(fixture("test.tar"));
    assert!(result.is_err());
}

// ── add_entry() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_in_modify_mode() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.add_entry("new.txt", b"hello world");
    assert!(result.is_ok());
}

#[test]
fn test_add_entry_not_in_modify_mode() {
    // Open in read mode
    let mut archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.add_entry("new.txt", b"hello");
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("Modify mode") || msg.contains("add_entry"));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_multiple() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.add_entry("a.txt", b"aaa").unwrap();
    archive.add_entry("b.txt", b"bbb").unwrap();
    archive.add_entry("c.txt", b"ccc").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 3);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_empty_data() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.add_entry("empty.txt", b"");
    assert!(result.is_ok());
}

// ── add_directory_entry() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_add_directory_entry_in_modify_mode() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.add_directory_entry("subdir");
    assert!(result.is_ok());
    assert_eq!(archive.pending_operations().unwrap(), 1);
}

#[test]
fn test_add_directory_entry_not_in_modify_mode() {
    let mut archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.add_directory_entry("subdir");
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("Modify mode") || msg.contains("add_directory_entry"));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_add_directory_entry_tracks_in_pending() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.add_directory_entry("dir1").unwrap();
    archive.add_directory_entry("dir2").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 2);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_clear_operations_clears_directories() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.add_directory_entry("dir1").unwrap();
    archive.add_entry("file.txt", b"data").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 2);
    archive.clear_operations().unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 0);
}

// ── remove_entry() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_remove_entry_in_modify_mode() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.remove_entry("test_file.txt");
    assert!(result.is_ok());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_remove_entry_unknown_path_is_accepted() {
    // removes target the source listing; a path that matches nothing is
    // a silent no-op so callers can freely queue removes before adds.
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.remove_entry("definitely-not-there.txt");
    assert_eq!(result.unwrap(), 0);
    assert_eq!(archive.pending_operations().unwrap(), 0);
}

#[test]
fn test_remove_entry_not_in_modify_mode() {
    let mut archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.remove_entry("file.txt");
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("Modify mode") || msg.contains("remove_entry"));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_remove_entry_tracks_path() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.remove_entry("test_file.txt").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 1);
}

// ── replace_entry() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_replace_entry_in_modify_mode() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    let result = archive.replace_entry("test_file.txt", b"replacement");
    assert!(result.is_ok());
    // replace = remove + add, so 2 pending operations
    assert_eq!(archive.pending_operations().unwrap(), 2);
}

#[test]
fn test_replace_entry_not_in_modify_mode() {
    let mut archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.replace_entry("file.txt", b"new data");
    assert!(result.is_err());
}

// ── pending_operations() tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_pending_operations_initially_zero() {
    let (_td, _p) = fixture_copy("test.zip");
    let archive = Archive::modify(&_p).unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 0);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_pending_operations_counts_adds_and_removes() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.add_entry("new.txt", b"data").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 1);
    archive.remove_entry("test_file.txt").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 2);
}

#[test]
fn test_pending_operations_read_mode_errors() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    // Read-mode handles have no tracker; the accessor surfaces misuse
    // instead of silently returning zero.
    let err = archive.pending_operations().unwrap_err();
    assert!(
        format!("{err}").contains("Modify mode"),
        "expected Modify-mode error, got {err}"
    );
}

// ── clear_operations() ──

#[cfg(feature = "libarchive")]
#[test]
fn test_clear_operations_clears_pending() {
    let (_td, _p) = fixture_copy("test.zip");
    let mut archive = Archive::modify(&_p).unwrap();
    archive.add_entry("file.txt", b"data").unwrap();
    archive.remove_entry("test_file.txt").unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 2);
    archive.clear_operations().unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 0);
}

// ── commit_changes() tests ──

#[test]
fn test_commit_changes_not_in_modify_mode() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.commit_changes();
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("Modify mode") || msg.contains("commit_changes"));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_no_modifications_succeeds() {
    // When there are no pending modifications, commit should succeed immediately
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("test_commit_noop.zip");

    // Create a valid archive first
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("file.txt", b"content").unwrap();
    archive.finish().unwrap();

    // Open in modify mode and commit with no changes
    let archive = Archive::modify(&test_path).unwrap();
    assert_eq!(archive.pending_operations().unwrap(), 0);
    let result = archive.commit_changes();
    assert!(result.is_ok());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_add_entry_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("test_commit_add.zip");

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive
        .add_file_from_data("original.txt", b"original")
        .unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("added.txt", b"added content").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
}

/// R0080-0005/0041-0046: once `modify()` has captured the locked archive's
/// identity, a non-cooperating process that renames the archive aside and
/// drops a different file at the pathname must not cause `commit_changes` to
/// touch that impostor. The commit fails with the identity error and the
/// impostor is left byte-for-byte intact. Unix-only: identity capture is
/// `(dev, ino)` via `MetadataExt`; on other platforms revalidation is skipped.
#[cfg(unix)]
#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_aborts_on_path_identity_change() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("identity.zip");

    // The archive this modify session will lock.
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive
        .add_file_from_data("original.txt", b"original")
        .unwrap();
    archive.finish().unwrap();

    // A valid but unrelated ZIP to impersonate the pathname after the swap.
    let impostor_src = temp.path().join("impostor.zip");
    let imp_options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut impostor = Archive::create(&impostor_src, imp_options).unwrap();
    impostor
        .add_file_from_data("impostor.txt", b"do not touch")
        .unwrap();
    impostor.finish().unwrap();
    let impostor_bytes = std::fs::read(&impostor_src).unwrap();

    // Open for modification: captures the locked inode identity (dev, ino).
    let mut modify = Archive::modify(&archive_path).unwrap();
    modify.add_entry("added.txt", b"added content").unwrap();

    // Non-cooperating swap: rename the locked archive aside (the advisory
    // lock follows the inode) and drop a different file at the pathname.
    let moved_aside = temp.path().join("identity.moved.zip");
    std::fs::rename(&archive_path, &moved_aside).unwrap();
    std::fs::write(&archive_path, &impostor_bytes).unwrap();

    // Commit must refuse rather than clobber the impostor.
    let err = modify.commit_changes().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("identity changed"),
        "expected identity-change error, got: {msg}"
    );

    // The impostor at the pathname is untouched, byte for byte.
    let after = std::fs::read(&archive_path).unwrap();
    assert_eq!(after, impostor_bytes);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_remove_entry_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("test_commit_remove.zip");

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("keep.txt", b"keep").unwrap();
    archive.add_file_from_data("remove.txt", b"remove").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.remove_entry("remove.txt").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "keep.txt");
}

// ── R0069-0062: directory/file conflict detection ──

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_rejects_file_under_existing_file() {
    // Source has `a.txt` (file). The user then queues a new file at
    // `a.txt/sub.txt`, which would force `a.txt` to be both a leaf
    // file and a directory ancestor. The dup-check gate must reject
    // before any temp archive is written.
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("conflict_file_under_file.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("a.txt", b"leaf file").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("a.txt/sub.txt", b"under leaf").unwrap();
    let err = archive.commit_changes().unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
    let msg = err.to_string();
    assert!(
        msg.contains("file") && msg.contains("directory"),
        "expected dir/file conflict diagnostic, got: {msg}"
    );
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_rejects_directory_then_file_at_same_path() {
    // Adding `dir/` and `dir` (same normalized key, one is a leaf
    // file the other is a directory) must fail.
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("conflict_dir_vs_file.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_directory_entry("collide").unwrap();
    archive.add_entry("collide", b"shouldn't merge").unwrap();
    let err = archive.commit_changes().unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_accepts_file_under_added_directory() {
    // `dir/` and `dir/file.txt` together is the normal case and
    // must succeed — the directory ancestor of the file is the same
    // path the directory entry claims.
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("ok_dir_with_file.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_directory_entry("nested").unwrap();
    archive
        .add_entry("nested/leaf.txt", b"under nested")
        .unwrap();
    archive.commit_changes().unwrap();
}

// ── AD 0062 A.4: typed EntrySource adds ──

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_from_path_round_trip_zip() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("from_path.zip");
    let payload_path = temp.path().join("payload.bin");
    let payload_bytes = b"streamed-from-disk";
    std::fs::write(&payload_path, payload_bytes).unwrap();

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive
        .add_entry_from_path("payload.bin", &payload_path)
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let read_back = archive.extract_to_memory("payload.bin").unwrap();
    assert_eq!(read_back, payload_bytes);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_from_reader_with_size_zip() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("from_reader.zip");

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let payload = b"streamed-from-reader-with-size".to_vec();
    let size = payload.len() as u64;

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive
        .add_entry_from_reader(
            "stream.txt",
            std::io::Cursor::new(payload.clone()),
            Some(size),
        )
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let read_back = archive.extract_to_memory("stream.txt").unwrap();
    assert_eq!(read_back, payload);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_add_entry_from_reader_unknown_size_zip() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("from_reader_unknown.zip");

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let payload = b"unknown-size-staged-via-tempfile".to_vec();

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive
        .add_entry_from_reader("stream.txt", std::io::Cursor::new(payload.clone()), None)
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let read_back = archive.extract_to_memory("stream.txt").unwrap();
    assert_eq!(read_back, payload);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_replace_entry_from_path_swaps_payload() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("replace_from_path.zip");
    let payload_path = temp.path().join("new_payload.bin");
    std::fs::write(&payload_path, b"new-content-from-disk").unwrap();

    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive
        .add_file_from_data("target.bin", b"old-content")
        .unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive
        .replace_entry_from_path("target.bin", &payload_path)
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let read_back = archive.extract_to_memory("target.bin").unwrap();
    assert_eq!(read_back, b"new-content-from-disk");
}

// ── R0079-0014: validate_pending_commit preflights mod_options gates ──

#[cfg(feature = "libarchive")]
#[test]
fn test_validate_pending_commit_rejects_format_override_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("preflight_format_mismatch.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut mod_opts = ModificationOptions::new();
    mod_opts.compression = Some(crate::options::CompressionOptions::for_writable(
        WritableFormat::TAR,
    ));
    let mut archive = Archive::modify_with_options(&test_path, mod_opts).unwrap();
    archive.add_entry("added.txt", b"added").unwrap();

    let err = archive
        .validate_pending_commit(archive.modifications.as_ref().unwrap())
        .unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
    assert!(
        err.to_string()
            .contains("does not match the archive's format"),
        "expected the R0071-0002 container-format diagnostic from the dry-run, got: {err}"
    );
}

#[cfg(feature = "libarchive")]
#[test]
fn test_validate_pending_commit_rejects_password_in_options() {
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("preflight_password.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut compression = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    compression.password = Some("secret".to_string().into());
    let mut mod_opts = ModificationOptions::new();
    mod_opts.compression = Some(compression);
    let mut archive = Archive::modify_with_options(&test_path, mod_opts).unwrap();
    archive.add_entry("added.txt", b"added").unwrap();

    let err = archive
        .validate_pending_commit(archive.modifications.as_ref().unwrap())
        .unwrap_err();
    assert!(
        err.to_string().contains("password"),
        "expected the MADR-0027 password rejection from the dry-run, got: {err}"
    );
}

// ── R0079-0020: strict-upfront retained-path validation ──

#[cfg(feature = "libarchive")]
#[test]
fn test_validate_pending_commit_rejects_wild_retained_directory_path() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("wild_retained.zip");
    {
        let file = std::fs::File::create(&test_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        writer.start_file("keep.txt", opts).unwrap();
        writer.write_all(b"keep").unwrap();
        // Wild-but-readable shape: a './'-prefixed directory entry as
        // produced by legacy zip tooling. The write-side validator
        // rejects '.' segments.
        writer.add_directory("./wild", opts).unwrap();
        writer.finish().unwrap();
    }

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("added.txt", b"added").unwrap();

    // The non-consuming gate (the `try_commit_changes` path) fires
    // before any temp file exists, labeled with the commit gate…
    let gate_err = archive
        .validate_pending_commit(archive.modifications.as_ref().unwrap())
        .unwrap_err();
    assert!(matches!(gate_err, ArchiveError::OperationBlocked { .. }));
    let gate_msg = gate_err.to_string();
    assert!(
        gate_msg.contains("commit_changes") && gate_msg.contains("./wild"),
        "expected a commit-labeled retained-path rejection, got: {gate_msg}"
    );

    // …and the consuming commit rejects with the same gate.
    let err = archive.commit_changes().unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
    assert!(
        err.to_string().contains("./wild"),
        "expected the retained-path diagnostic to name the wild path, got: {err}"
    );
}

// ── R0079-0037: gate skips link entries the rewrite drops (FR-022) ──

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_allows_add_at_dropped_symlink_path() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("symlink_slot.zip");
    {
        let file = std::fs::File::create(&test_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        writer.start_file("keep.txt", opts).unwrap();
        writer.write_all(b"keep").unwrap();
        writer.add_symlink("link_path", "keep.txt", opts).unwrap();
        writer.finish().unwrap();
    }

    let mut archive = Archive::modify(&test_path).unwrap();
    {
        let listing = archive.list_files().unwrap();
        let link = listing
            .iter()
            .find(|e| e.path == "link_path")
            .expect("symlink entry must appear in the source listing");
        assert_eq!(
            link.entry_type,
            crate::entry::EntryType::Symlink,
            "premise: the crafted zip symlink must list as Symlink"
        );
    }

    // The rewrite drops the symlink per FR-022, so adding a regular
    // file at its path must pass the gate instead of being rejected
    // as a duplicate of an entry the commit never writes.
    archive
        .add_entry("link_path", b"now a regular file")
        .unwrap();
    archive
        .validate_pending_commit(archive.modifications.as_ref().unwrap())
        .unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&test_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|e| e.path == "link_path" && e.entry_type == crate::entry::EntryType::File),
        "the added entry must land as a regular file at the dropped link's path"
    );
}

// ── R0079-0021: commit preserves the archive file's own mode ──

#[cfg(unix)]
#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_preserves_archive_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let test_path = temp.path().join("private.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&test_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    std::fs::set_permissions(&test_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let mut archive = Archive::modify(&test_path).unwrap();
    archive.add_entry("added.txt", b"added").unwrap();
    archive.commit_changes().unwrap();

    let mode = std::fs::metadata(&test_path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(
        mode, 0o600,
        "commit_changes must carry the original archive's mode across the atomic replace"
    );
}

// ── R0079-0034: locked-handle detection mirrors the DCR-003 fallback set ──

#[test]
fn test_detect_format_from_locked_lzma_family_extension_fallback() {
    let temp = tempfile::tempdir().unwrap();
    for (name, expected) in [
        ("magicless.lzma", ArchiveFormat::Lzma),
        ("magicless.tlz", ArchiveFormat::TarLzma),
        ("magicless.tar.lzma", ArchiveFormat::TarLzma),
    ] {
        let path = temp.path().join(name);
        // Payload with no recognizable magic so detection must fall
        // back to the extension.
        std::fs::write(&path, [0xAAu8; 16]).unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let format = detect_format_from_locked(&file, &path).unwrap();
        assert_eq!(format, expected, "extension fallback for {name}");
    }
}

#[test]
fn test_modify_lzma_rejected_with_capability_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("payload.lzma");
    std::fs::write(&path, [0xAAu8; 16]).unwrap();
    let result = Archive::modify(&path);
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "expected the capability-gated OperationBlocked instead of Unknown archive format, got: {err:?}"
    );
    assert!(
        err.to_string().contains("modification"),
        "expected the can_modify diagnostic, got: {err}"
    );
}

#[test]
fn test_ancestor_paths_basic() {
    use crate::modification::ancestor_paths;
    assert_eq!(ancestor_paths(""), Vec::<String>::new());
    assert_eq!(ancestor_paths("a"), Vec::<String>::new());
    assert_eq!(ancestor_paths("a/b"), vec!["a".to_string()]);
    assert_eq!(
        ancestor_paths("a/b/c"),
        vec!["a".to_string(), "a/b".to_string()]
    );
}

// ── R0081-0037 / R0081-0038: modify-commit staging cap + directory dedup ──

#[test]
fn test_drain_into_tempfile_rejects_over_cap_reader() {
    // A reader that would deliver more than the cap must be rejected
    // rather than staged in full (R0081-0037), so a hostile/infinite
    // unknown-size source cannot fill the disk during commit.
    let cap = 4096u64;
    let oversized = vec![0u8; (cap as usize) + 1];
    let err = drain_into_tempfile(std::io::Cursor::new(oversized), "big.bin", cap)
        .expect_err("drain must reject a reader that exceeds the staging cap");
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "expected OperationBlocked for an over-cap staged reader, got: {err:?}"
    );
}

#[test]
fn test_drain_into_tempfile_accepts_reader_at_cap() {
    // Exactly `cap` bytes is allowed; the ceiling is inclusive.
    use std::io::Read;
    let cap = 4096u64;
    let payload = vec![7u8; cap as usize];
    let (mut staged, learned) =
        drain_into_tempfile(std::io::Cursor::new(payload), "exact.bin", cap)
            .expect("draining exactly cap bytes must succeed");
    assert_eq!(learned, cap);
    let mut buf = Vec::new();
    staged.read_to_end(&mut buf).unwrap();
    assert_eq!(buf.len() as u64, cap);
    assert!(buf.iter().all(|&b| b == 7));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_commit_changes_emits_duplicate_directory_once() {
    // Adding the same directory path several times (including a
    // trailing-slash spelling that normalizes to the same key) must
    // emit a single directory entry in the rewritten archive
    // (R0081-0038).
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("dup_dir.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive.add_file_from_data("seed.txt", b"seed").unwrap();
    archive.finish().unwrap();

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive.add_directory_entry("dup").unwrap();
    archive.add_directory_entry("dup").unwrap();
    archive.add_directory_entry("dup/").unwrap();
    archive.commit_changes().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    let dup_count = entries
        .iter()
        .filter(|e| normalize_dup_check_path(&e.path) == "dup")
        .count();
    assert_eq!(
        dup_count, 1,
        "duplicate directory adds must emit a single entry, got {dup_count}"
    );
}

// ── rename_with_overwrite() tests (Unix) ──

#[cfg(not(windows))]
mod rename_tests {
    use super::*;

    #[test]
    fn test_rename_with_overwrite_basic() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("dest.txt");

        std::fs::write(&src, b"hello").unwrap();
        rename_with_overwrite(&src, &dst).unwrap();

        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
    }

    #[test]
    fn test_rename_with_overwrite_replaces_existing() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("dest.txt");

        std::fs::write(&src, b"new content").unwrap();
        std::fs::write(&dst, b"old content").unwrap();

        rename_with_overwrite(&src, &dst).unwrap();

        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"new content");
    }

    #[test]
    fn test_rename_with_overwrite_nonexistent_source() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("nonexistent.txt");
        let dst = temp.path().join("dest.txt");

        let result = rename_with_overwrite(&src, &dst);
        assert!(result.is_err());
    }
}

// ── commit-phase tests (OI-0076-003 item 6) ────────────────────────────────
//
// These call `commit_plan` directly. Before the plan / session / swap split
// there was no way to do that: every property below could only be reached
// through a whole commit, so a failure named the entire operation instead of
// the phase that broke — and several of them could not be observed at all,
// because the plan's decisions are invisible once the write has happened.
//
// The suffix test is the one that mattered most: it had NO coverage in any
// form, and the property it pins is a filesystem-write target.

/// Build a Modify-mode handle over a one-entry ZIP with one pending change.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn modify_handle_with_pending_change(dir: &std::path::Path, name: &str) -> Archive {
    let path = dir.join(name);
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&path, options).unwrap();
    archive
        .add_file_from_data("original.txt", b"original")
        .unwrap();
    archive.finish().unwrap();

    let mut modify = Archive::modify(&path).unwrap();
    modify.add_entry("added.txt", b"added").unwrap();
    modify
}

/// R0070-0009. `backup_suffix` is `pub`, so a caller can bypass
/// `with_backup`'s sanitiser by assigning the field directly. The suffix is
/// appended to the archive path by `backup_path_for` with no further
/// separator check, and the result is then opened `create_new`, chmod'ed and
/// — on a copy failure — unlinked. A separator in it therefore aims those
/// three operations outside the archive's own directory.
///
/// This had no test of any kind before the split. `with_backup` was covered;
/// the direct-assignment bypass it exists to catch was not.
#[cfg(feature = "libarchive")]
#[test]
fn commit_plan_rejects_a_backup_suffix_that_would_escape_the_directory() {
    let temp = tempfile::tempdir().unwrap();

    for bad in ["/../evil", "..\\evil", "", "sub/dir"] {
        let mut modify = modify_handle_with_pending_change(temp.path(), "esc.zip");
        modify.mod_options = Some(ModificationOptions {
            create_backup: true,
            backup_suffix: bad.to_string(),
            ..Default::default()
        });

        let plan = modify
            .commit_plan()
            .expect("planning must succeed")
            .expect("a pending change means a plan");
        assert_eq!(
            plan.mod_options.backup_suffix, ".bak",
            "a suffix of {bad:?} must fall back to `.bak`; it reaches \
             backup_path_for, which appends it to the archive path with no \
             separator check"
        );
        std::fs::remove_file(temp.path().join("esc.zip")).ok();
    }
}

/// A suffix with no separator is the caller's business and must survive.
/// Pinned so the sanitiser above cannot be "hardened" into ignoring the
/// caller entirely.
#[cfg(feature = "libarchive")]
#[test]
fn commit_plan_keeps_a_backup_suffix_that_is_merely_unusual() {
    let temp = tempfile::tempdir().unwrap();
    let mut modify = modify_handle_with_pending_change(temp.path(), "keep.zip");
    modify.mod_options = Some(ModificationOptions {
        create_backup: true,
        backup_suffix: ".backup-2026".to_string(),
        ..Default::default()
    });

    let plan = modify.commit_plan().unwrap().unwrap();
    assert_eq!(plan.mod_options.backup_suffix, ".backup-2026");
}

/// The staging file must be a sibling of the original, and must *append* to
/// the archive's name rather than replace its extension.
///
/// Sibling because the swap is a rename, which is only atomic within one
/// filesystem — a staging file in a temp directory could land on a different
/// mount and turn the atomic replace into a copy. Appending because it keeps
/// the original extension visible in leftover staging names.
#[cfg(feature = "libarchive")]
#[test]
fn commit_plan_stages_beside_the_original_and_keeps_its_extension() {
    let temp = tempfile::tempdir().unwrap();
    let mut modify = modify_handle_with_pending_change(temp.path(), "beside.zip");
    let archive_path = temp.path().join("beside.zip");

    let plan = modify.commit_plan().unwrap().unwrap();

    assert_eq!(
        plan.temp_path.parent(),
        archive_path.parent(),
        "the staging file must be a sibling: rename is only atomic within a filesystem"
    );
    let name = plan
        .temp_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        name.starts_with("beside.zip"),
        "the suffix must be appended, not replace the extension: {name}"
    );
    assert!(
        !plan.temp_path.exists(),
        "planning must not create the staging file — nothing before the \
         empty-tracker check may leave a file behind"
    );
}

/// Two plans in the same process must not collide on a staging name.
///
/// Be precise about what this does and does not pin. It pins the observable
/// property — two plans, two distinct staging paths. It does **not** pin the
/// `static COUNTER` that the temp-name comment credits for uniqueness:
/// replacing that counter with a constant leaves this test green, which was
/// checked rather than assumed.
///
/// That is not a gap in the test so much as a property of the name. The
/// staging name is derived from `self.path`, so two *different* archives
/// differ with or without the counter, and two commits on the *same* path
/// cannot overlap because `Archive::modify` holds an advisory lock on it.
/// The counter is defence-in-depth against a same-tick collision that a test
/// cannot force. Written down here so nobody later reads a green suite as
/// evidence the counter is load-bearing and deletes it.
#[cfg(feature = "libarchive")]
#[test]
fn commit_plan_staging_names_are_unique_within_one_process() {
    let temp = tempfile::tempdir().unwrap();
    let mut h1 = modify_handle_with_pending_change(temp.path(), "u1.zip");
    let a = h1.commit_plan().unwrap().unwrap();
    let mut h2 = modify_handle_with_pending_change(temp.path(), "u2.zip");
    let b = h2.commit_plan().unwrap().unwrap();
    assert_ne!(
        a.temp_path, b.temp_path,
        "two commits in one process must not share a staging path"
    );
}

/// The empty-tracker case is an early *success*, and the caller's cleanup
/// only runs on `Err` — so a file created before this point would be
/// orphaned permanently rather than removed. `None` is the signal that
/// nothing was decided and nothing exists.
#[cfg(feature = "libarchive")]
#[test]
fn commit_plan_returns_none_for_an_empty_tracker_and_creates_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("empty.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&path, options).unwrap();
    archive.add_file_from_data("only.txt", b"x").unwrap();
    archive.finish().unwrap();

    let mut modify = Archive::modify(&path).unwrap();
    modify.mod_options = Some(ModificationOptions {
        create_backup: true,
        ..Default::default()
    });

    assert!(
        modify.commit_plan().unwrap().is_none(),
        "an empty tracker must plan nothing"
    );

    let siblings: Vec<String> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        siblings,
        vec!["empty.zip".to_string()],
        "planning an empty commit must create neither a staging file nor a \
         backup — the success path removes nothing, so either would leak"
    );
}

/// A non-Modify handle must be refused before anything else happens. Such a
/// handle has no advisory-lock descriptor and no captured identity, so every
/// downstream gate degenerates to a pass and the swap would replace a path
/// this session never locked.
#[test]
fn commit_plan_refuses_a_handle_that_is_not_in_modify_mode() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("read.zip");
    let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&path, options).unwrap();
    archive.add_file_from_data("a.txt", b"a").unwrap();
    archive.finish().unwrap();

    let mut reader = Archive::open(&path).unwrap();
    // Matched rather than `expect_err`: `CommitPlan` cannot be `Debug`,
    // because `EntrySource::Reader` holds a `Box<dyn Read>`.
    let err = match reader.commit_plan() {
        Err(e) => e,
        Ok(_) => panic!("a Read-mode handle must be refused"),
    };
    assert!(
        err.to_string().contains("Modify mode"),
        "the refusal must name the mode requirement: {err}"
    );
}
