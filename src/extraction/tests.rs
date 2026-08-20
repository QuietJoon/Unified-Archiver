use super::*;
use crate::test_utils::fixture;

// ── ensure_destination tests ──

#[test]
fn test_ensure_destination_creates_dir() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("new_subdir");
    assert!(!dest.exists());
    ensure_destination(&dest).unwrap();
    assert!(dest.exists());
}

#[test]
fn test_ensure_destination_nested() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("a").join("b").join("c");
    ensure_destination(&dest).unwrap();
    assert!(dest.exists());
}

#[test]
fn test_ensure_destination_existing_dir() {
    let temp = tempfile::tempdir().unwrap();
    // Already exists
    ensure_destination(temp.path()).unwrap();
}

// ── check_overwrite_conflicts tests ──

// Helper to create a test ArchiveEntry
fn make_entry(path: &str, entry_type: crate::entry::EntryType) -> ArchiveEntry {
    ArchiveEntry {
        id: 0,
        path: path.to_string(),
        size: Some(100),
        compressed_size: Some(50),
        modified: None,
        created: None,
        accessed: None,
        crc32: None,
        is_encrypted: false,
        entry_type,
        permissions: None,
        comment: None,
        attributes: None,
        link_target: None,
        raw_path: None,
    }
}

#[test]
fn test_check_overwrite_conflicts_no_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![make_entry("new_file.txt", crate::entry::EntryType::File)];
    assert!(check_overwrite_conflicts(&entries, temp.path(), false, "test").is_ok());
}

#[test]
fn test_check_overwrite_conflicts_existing_file_no_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing.txt");
    std::fs::write(&existing, b"content").unwrap();

    let entries = vec![make_entry("existing.txt", crate::entry::EntryType::File)];
    let result = check_overwrite_conflicts(&entries, temp.path(), false, "test");
    assert!(result.is_err());
}

#[test]
fn test_check_overwrite_conflicts_existing_file_with_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing.txt");
    std::fs::write(&existing, b"content").unwrap();

    let entries = vec![make_entry("existing.txt", crate::entry::EntryType::File)];
    // With overwrite=true, should succeed
    assert!(check_overwrite_conflicts(&entries, temp.path(), true, "test").is_ok());
}

#[test]
fn test_check_overwrite_conflicts_directory_ignored() {
    let temp = tempfile::tempdir().unwrap();
    // Directories should be ignored (only files checked)
    let entries = vec![make_entry("some_dir/", crate::entry::EntryType::Directory)];
    assert!(check_overwrite_conflicts(&entries, temp.path(), false, "test").is_ok());
}

#[test]
fn test_check_overwrite_conflicts_file_entry_vs_existing_dir_rejected() {
    // R0079-0017: a pre-existing directory at a file entry's output path
    // is rejected regardless of `overwrite` — no backend can replace a
    // directory with a file, so letting it pass produces a mid-extract
    // EISDIR with partial output.
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("collide")).unwrap();
    let entries = vec![make_entry("collide", crate::entry::EntryType::File)];

    for overwrite in [true, false] {
        let err = check_overwrite_conflicts(&entries, temp.path(), overwrite, "test").unwrap_err();
        match err {
            ArchiveError::OperationBlocked { reason, .. } => {
                assert!(
                    reason.contains("exists as a directory"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected OperationBlocked, got {other:?}"),
        }
    }
}

#[test]
fn test_check_overwrite_conflicts_file_then_nested_file_rejected() {
    // R0079-0018: file `a` + file `a/b` — `a/b` requires directory `a`,
    // which is already claimed by a file entry.
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("a", crate::entry::EntryType::File),
        make_entry("a/b", crate::entry::EntryType::File),
    ];
    let err = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
}

#[test]
fn test_check_overwrite_conflicts_nested_file_then_file_rejected() {
    // R0079-0018, reverse order: file `a/b` registers implicit
    // directory `a`; a later file entry `a` collides with it.
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("a/b", crate::entry::EntryType::File),
        make_entry("a", crate::entry::EntryType::File),
    ];
    let err = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
}

#[test]
fn test_check_overwrite_conflicts_file_vs_dir_entry_ancestor_rejected() {
    // R0079-0018: an explicit directory entry `a/b/` requires directory
    // `a` just like a nested file does — both orders against file `a`.
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("a", crate::entry::EntryType::File),
        make_entry("a/b/", crate::entry::EntryType::Directory),
    ];
    let err = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));

    let entries = vec![
        make_entry("a/b/", crate::entry::EntryType::Directory),
        make_entry("a", crate::entry::EntryType::File),
    ];
    let err = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap_err();
    assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
}

#[test]
fn test_check_overwrite_conflicts_nested_tree_accepted() {
    // Sanity: a well-formed tree (dir entry + files beneath it, plus
    // sibling files sharing an implicit parent) passes the ancestor
    // tracking added for R0079-0018.
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("a/", crate::entry::EntryType::Directory),
        make_entry("a/b", crate::entry::EntryType::File),
        make_entry("a/c", crate::entry::EntryType::File),
        make_entry("d/e/f", crate::entry::EntryType::File),
        make_entry("d/e/g", crate::entry::EntryType::File),
    ];
    assert!(check_overwrite_conflicts(&entries, temp.path(), false, "test").is_ok());
}

#[test]
fn test_check_overwrite_conflicts_case_fold_collision_warns() {
    // R0079-0036: `README` and `readme` merge on case-insensitive
    // destination filesystems (APFS default, Windows). The preflight
    // cannot know the destination's sensitivity, so it warns instead
    // of blocking.
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("README", crate::entry::EntryType::File),
        make_entry("readme", crate::entry::EntryType::File),
    ];
    let warnings = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap();
    assert_eq!(warnings.len(), 1, "expected exactly one case-fold warning");
    match &warnings[0] {
        ArchiveWarning::OutputPathCaseCollision { first, second } => {
            assert_eq!(first, "README");
            assert_eq!(second, "readme");
        }
        other => panic!("expected OutputPathCaseCollision, got {other:?}"),
    }
}

#[test]
fn test_check_overwrite_conflicts_distinct_paths_no_case_warning() {
    let temp = tempfile::tempdir().unwrap();
    let entries = vec![
        make_entry("alpha", crate::entry::EntryType::File),
        make_entry("beta/gamma", crate::entry::EntryType::File),
    ];
    let warnings = check_overwrite_conflicts(&entries, temp.path(), true, "test").unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

// ── open_archive_for_extraction tests ──

#[test]
fn test_open_archive_for_extraction_no_password() {
    let archive = open_archive_for_extraction(&fixture("test.zip"), None).unwrap();
    assert_eq!(archive.format(), crate::format::ArchiveFormat::Zip);
}

#[test]
fn test_open_archive_for_extraction_with_password_uses_encrypted_open_for_zip() {
    // R0073-0008: ZIP with password should reach the password-aware
    // ZipReader path through `Archive::open_encrypted`. (Both ZIP paths now
    // use the same `zip`-crate backend after the AD 0007 collapse; this
    // still asserts the password-carrying constructor is chosen.)
    let archive = open_archive_for_extraction(&fixture("test.zip"), Some("password")).unwrap();
    assert_eq!(archive.format(), crate::format::ArchiveFormat::Zip);
}

// ── extract_all tests ──

#[test]
fn test_extract_all_zip() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_all(options).unwrap();
    // Verify at least one file was extracted
    let mut has_files = false;
    for entry in std::fs::read_dir(temp.path()).unwrap() {
        if entry.unwrap().path().is_file() {
            has_files = true;
            break;
        }
    }
    // Check either direct files or subdirectories
    assert!(
        std::fs::read_dir(temp.path()).unwrap().count() > 0,
        "Expected extracted files in output directory"
    );
    let _ = has_files;
}

#[test]
fn test_extract_all_tar() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_all(options).unwrap();
}

#[test]
fn test_extract_all_creates_destination() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("auto_created");
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: dest.clone(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_all(options).unwrap();
    assert!(dest.exists());
}

// ── extract_file tests ──

#[test]
fn test_extract_file_zip() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries.iter().find(|e| e.is_file()).unwrap();

    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_file(&first_file.path, options).unwrap();
}

#[test]
fn test_extract_file_nonexistent() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    let result = archive.extract_file("nonexistent_file_xyz.txt", options);
    assert!(result.is_err());
}

// ── extract_to_memory tests ──

#[test]
fn test_extract_to_memory_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries.iter().find(|e| e.is_file()).unwrap();
    let data = archive.extract_to_memory(&first_file.path).unwrap();
    assert!(!data.is_empty());
}

#[test]
fn test_extract_to_memory_nonexistent() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let result = archive.extract_to_memory("nonexistent_xyz.txt");
    assert!(result.is_err());
}

// ── extract_files tests ──

#[test]
fn test_extract_files_empty_list() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    // Empty list should succeed (no-op)
    archive.extract_files(&[], options).unwrap();
}

#[test]
fn test_extract_files_nonexistent_path() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    let result = archive.extract_files(&["nonexistent.txt"], options);
    assert!(result.is_err());
}

// ── extract_by_ids tests ──

#[test]
fn test_extract_by_ids_empty_list() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    // Empty list should succeed (no-op)
    archive.extract_by_ids(&[], options).unwrap();
}

#[test]
fn test_extract_by_ids_invalid_id() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    let result = archive.extract_by_ids(&[99999], options);
    assert!(result.is_err());
}

#[test]
fn test_extract_by_ids_valid_id() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_by_ids(&[0], options).unwrap();
}

// ── extract_filtered tests ──

#[test]
fn test_extract_filtered_no_matches() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        ..Default::default()
    };
    // Filter that matches nothing
    archive.extract_filtered(|_| false, options).unwrap();
}

#[test]
fn test_extract_filtered_all_files() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive
        .extract_filtered(|entry| entry.is_file(), options)
        .unwrap();
}

// ── Overwrite behavior tests ──

#[test]
fn test_extract_all_no_overwrite_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open(fixture("test.zip")).unwrap();

    // First extraction
    let options = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: true,
        ..Default::default()
    };
    archive.extract_all(options).unwrap();

    // Second extraction without overwrite should fail
    let options2 = ExtractionOptions {
        destination: temp.path().to_path_buf(),
        overwrite: false,
        ..Default::default()
    };
    let result = archive.extract_all(options2);
    assert!(
        result.is_err(),
        "Should fail when files already exist and overwrite=false"
    );
}

// ── stream_backend_cap tests (R0001-0011 / DEF-004) ──

/// The backend materialization budget is `min(caller ceiling, bound
/// tightening)` in all four `Option` quadrants, and the caller's limits
/// are a ceiling a `StreamBound` may tighten but never loosen.
#[test]
fn stream_backend_cap_is_the_minimum_of_ceiling_and_bound() {
    use crate::streaming::StreamBound;

    let both = ExtractionLimits::builder()
        .max_file_size(crate::security::Cap::Limited(1000))
        .max_total_size(crate::security::Cap::Limited(600))
        .build();
    let unlimited = ExtractionLimits::builder()
        .max_file_size(crate::security::Cap::Unlimited)
        .max_total_size(crate::security::Cap::Unlimited)
        .build();

    // Ceiling is min(max_file_size, max_total_size), never max_file_size
    // alone — the gap `effective_entry_cap` closed for the memory paths.
    assert_eq!(
        stream_backend_cap(StreamBound::Unbounded, None, &both),
        Some(600)
    );
    assert_eq!(
        stream_backend_cap(StreamBound::Unbounded, None, &unlimited),
        None
    );

    // A bound above the ceiling clamps to the ceiling; below it tightens.
    assert_eq!(
        stream_backend_cap(StreamBound::Cap(10_000), None, &both),
        Some(600)
    );
    assert_eq!(
        stream_backend_cap(StreamBound::Cap(16), None, &both),
        Some(16)
    );
    assert_eq!(
        stream_backend_cap(StreamBound::Cap(16), None, &unlimited),
        Some(16)
    );

    // DeclaredSize tightens to the listing size when it has one, and
    // contributes no tightening at all when it does not (hard constraint:
    // no declaration is invented for unknown-size entries).
    assert_eq!(
        stream_backend_cap(StreamBound::DeclaredSize, Some(64), &both),
        Some(64)
    );
    assert_eq!(
        stream_backend_cap(StreamBound::DeclaredSize, None, &both),
        Some(600)
    );
    assert_eq!(
        stream_backend_cap(StreamBound::DeclaredSize, None, &unlimited),
        None
    );
}

/// Constraint restated as a property over a grid: whatever the bound, the
/// derived budget never exceeds `min(max_file_size, max_total_size)`.
#[test]
fn stream_backend_cap_never_exceeds_the_caller_limits() {
    use crate::streaming::StreamBound;

    let limits = ExtractionLimits::builder()
        .max_file_size(crate::security::Cap::Limited(4096))
        .max_total_size(crate::security::Cap::Limited(2048))
        .build();
    let ceiling = 2048u64;

    for bound in [
        StreamBound::DeclaredSize,
        StreamBound::Cap(1),
        StreamBound::Cap(2048),
        StreamBound::Cap(u64::MAX),
        StreamBound::Unbounded,
    ] {
        for declared in [None, Some(0), Some(1024), Some(u64::MAX)] {
            let cap = stream_backend_cap(bound, declared, &limits)
                .expect("a limited ceiling always yields a cap");
            assert!(
                cap <= ceiling,
                "bound {bound:?} with declared {declared:?} produced {cap}, above the {ceiling} ceiling"
            );
        }
    }
}
