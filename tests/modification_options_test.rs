//! Phase B.2 — `Archive::modify_with_options` wiring (OI-025-003)
//!
//! Validates that the public `ModificationOptions` fields are honored by the
//! modify/commit pipeline. Per-entry metadata fidelity (preservation of
//! timestamps and Unix permissions on retained entries) is covered by
//! `tests/integration/modification.rs` under `ModificationOptions { preserve_metadata: true, .. }`
//! (OI-025-002 resolved 2026-04-14).

use std::path::PathBuf;
use unified_archive::{Archive, ArchiveFormat, CompressionOptions, ModificationOptions};

fn fresh_zip(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&path, { std::mem::take(&mut opts) }).unwrap();
    archive.add_file_from_data("a.txt", b"original A").unwrap();
    archive.add_file_from_data("b.txt", b"original B").unwrap();
    archive.finish().unwrap();
    path
}

#[test]
fn create_backup_writes_sidecar_before_atomic_rename() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "with_backup.zip");

    let opts = ModificationOptions::default().with_backup(".bak");
    let mut modifying = Archive::modify_with_options(&archive_path, opts).unwrap();
    modifying.add_entry("c.txt", b"new C").unwrap();
    modifying.commit_changes().unwrap();

    let backup_path = archive_path.with_extension("zip.bak");
    assert!(
        backup_path.exists(),
        "backup file expected at {}",
        backup_path.display()
    );

    // Backup should still contain only the pre-commit entries (a.txt, b.txt)
    let backup_archive = Archive::open(&backup_path).unwrap();
    let backup_entries: Vec<_> = backup_archive
        .list_files()
        .unwrap()
        .iter()
        .map(|e| e.path.clone())
        .collect();
    assert!(backup_entries.iter().any(|p| p == "a.txt"));
    assert!(backup_entries.iter().any(|p| p == "b.txt"));
    assert!(
        !backup_entries.iter().any(|p| p == "c.txt"),
        "backup must not contain the post-commit entry"
    );

    // The committed archive must contain the new entry.
    let new_archive = Archive::open(&archive_path).unwrap();
    let new_entries: Vec<_> = new_archive
        .list_files()
        .unwrap()
        .iter()
        .map(|e| e.path.clone())
        .collect();
    assert!(new_entries.iter().any(|p| p == "c.txt"));
}

#[test]
fn create_backup_disabled_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "no_backup.zip");

    let mut modifying = Archive::modify(&archive_path).unwrap();
    modifying.add_entry("c.txt", b"new C").unwrap();
    modifying.commit_changes().unwrap();

    let backup_path = archive_path.with_extension("zip.bak");
    assert!(
        !backup_path.exists(),
        "no backup expected with default modify()"
    );
}

#[test]
fn backup_suffix_normalizes_missing_dot() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "suffix_norm.zip");

    let opts = ModificationOptions::default().with_backup("backup");
    let mut modifying = Archive::modify_with_options(&archive_path, opts).unwrap();
    modifying.add_entry("c.txt", b"new C").unwrap();
    modifying.commit_changes().unwrap();

    let mut expected_backup = archive_path.clone().into_os_string();
    expected_backup.push(".backup");
    let expected_backup = std::path::PathBuf::from(expected_backup);
    assert!(
        expected_backup.exists(),
        "backup with normalized suffix expected at {}",
        expected_backup.display()
    );
}

#[test]
fn no_backup_when_no_modifications_pending() {
    // commit_changes with empty tracker is a no-op; we don't want to leave
    // a stale backup around in that case.
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "noop.zip");

    let opts = ModificationOptions::default().with_backup(".bak");
    let modifying = Archive::modify_with_options(&archive_path, opts).unwrap();
    modifying.commit_changes().unwrap();

    let backup_path = archive_path.with_extension("zip.bak");
    assert!(
        !backup_path.exists(),
        "no backup should be written for no-op commit"
    );
}
