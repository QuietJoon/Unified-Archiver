//! Phase B.2 — `Archive::modify_with_options` wiring (OI-025-003)
//!
//! Validates that the public `ModificationOptions` fields are honored by the
//! modify/commit pipeline. Per-entry metadata fidelity is asserted directly in
//! this file: `modify_zip_preserves_atime_and_btime_through_commit` checks that
//! the ZIP extended-timestamp `ac_time`/`cr_time` survive `commit_changes` at
//! wire level, so `created`/`accessed` *are* round-tripped (ZIP 0x5455 writer;
//! libarchive `archive_entry_set_atime`/`set_birthtime`). The suite in
//! `tests/integration/modification.rs` covers archive structure only — entry
//! counts, paths and payload bytes — not metadata. OI-0065-002, which the
//! earlier wording pointed readers at, was resolved 2026-04-30.

// Every case here drives `Archive::modify` / `commit_changes`, so the whole
// file needs the `modify` operation feature — which itself implies `read`,
// `create` and `libarchive` (AD 0058 decision 1 / AD 0071).
#![cfg(feature = "modify")]

use std::path::PathBuf;
#[cfg_attr(not(feature = "libarchive"), allow(unused_imports))]
use unified_archive::{Archive, CompressionOptions, ModificationOptions, WritableFormat};

#[path = "common/mod.rs"]
mod common;

#[cfg(feature = "create")]
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn fresh_zip(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let mut opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut archive = Archive::create(&path, std::mem::take(&mut opts)).unwrap();
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

/// R0066-0013: commit with backup enabled must refuse to clobber an
/// existing backup file. Previously `std::fs::copy` silently overwrote.
#[test]
fn backup_noclobber_refuses_to_overwrite_existing_backup() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "noclobber.zip");

    // First commit produces a backup.
    let opts = ModificationOptions::default().with_backup(".bak");
    let mut modifying = Archive::modify_with_options(&archive_path, opts).unwrap();
    modifying.add_entry("c.txt", b"first").unwrap();
    modifying.commit_changes().unwrap();

    let backup_path = archive_path.with_extension("zip.bak");
    assert!(backup_path.exists(), "first backup must exist");
    let original_backup_bytes = std::fs::read(&backup_path).unwrap();

    // Second commit must refuse with AlreadyExists-shaped error.
    let opts2 = ModificationOptions::default().with_backup(".bak");
    let mut modifying2 = Archive::modify_with_options(&archive_path, opts2).unwrap();
    modifying2.add_entry("d.txt", b"second").unwrap();
    let err = modifying2.commit_changes().unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("already exists") || msg.contains("AlreadyExists"),
        "expected noclobber error, got: {msg}"
    );

    // Backup must be untouched.
    assert_eq!(std::fs::read(&backup_path).unwrap(), original_backup_bytes);
}

/// R0066-0011/0012: `commit_changes` rejects retained+added and added+added
/// duplicate output paths up front. The prior commit loop silently appended
/// the colliding entry, producing ambiguous archives.
#[test]
fn commit_rejects_retained_vs_added_duplicate_path() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = fresh_zip(temp.path(), "dup.zip");

    let mut modifying = Archive::modify(&archive_path).unwrap();
    // a.txt is already in the source archive. Adding it again without
    // removing the source record is a duplicate.
    modifying.add_entry("a.txt", b"conflict").unwrap();
    let err = modifying.commit_changes().unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("duplicate") || msg.contains("more than once"),
        "expected duplicate-path error, got: {msg}"
    );
}

/// R0066-0050: `compression_fraction` and `expansion_ratio` semantics.
#[test]
fn compression_fraction_and_expansion_ratio_are_inverses() {
    use unified_archive::ArchiveEntry;

    let mut entry = ArchiveEntry::file("data.bin", 0).build();
    entry.size = Some(1000);
    entry.compressed_size = Some(250);

    let fraction = entry.compression_fraction().unwrap();
    let expansion = entry.expansion_ratio().unwrap();

    assert!((fraction - 0.25).abs() < f64::EPSILON);
    assert!((expansion - 4.0).abs() < f64::EPSILON);
    assert!(
        (fraction * expansion - 1.0).abs() < f64::EPSILON,
        "fraction and expansion must be inverses"
    );
}

/// R0066-0050: edge cases for `expansion_ratio` — `compressed_size == 0`
/// must yield `None` to avoid a /0 in callers gating on the zip-bomb limit.
#[test]
fn expansion_ratio_handles_zero_compressed_size() {
    use unified_archive::ArchiveEntry;

    let mut entry = ArchiveEntry::file("empty.bin", 0).build();
    entry.size = Some(100);
    entry.compressed_size = Some(0);
    assert!(entry.expansion_ratio().is_none());
}

/// OI-0065-002: ZIP commit-changes round-trips `accessed` and `created`
/// through the 0x5455 Universal Time extra field. Verification reads
/// the post-commit ZIP at wire level via the `zip` crate's extra-field
/// API, independent of the read backend — so it asserts what was
/// *written* rather than what any reader surfaces. (The sole ZIP read
/// backend does now surface 0x5455; the wire-level read keeps this test
/// backend-agnostic.)
#[test]
fn modify_zip_preserves_atime_and_btime_through_commit() {
    use std::time::{Duration, UNIX_EPOCH};

    let temp = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("ts_roundtrip.zip");

    // Step 1: build a ZIP whose first entry carries non-default mtime/atime/ctime.
    let mtime = UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    let atime = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let btime = UNIX_EPOCH + Duration::from_secs(1_400_000_000);

    common::build_zip_with_extended_timestamp(&zip_path, mtime, atime, btime);

    // Step 2: open + add a sibling entry + commit with preserve_metadata=true.
    let opts = ModificationOptions::default(); // preserve_metadata is on by default
    let mut modifying = Archive::modify_with_options(&zip_path, opts).unwrap();
    modifying
        .add_entry("filler.txt", b"new entry forces a rewrite")
        .unwrap();
    modifying.commit_changes().unwrap();

    // Step 3: read back at wire level via the `zip` crate to confirm the
    // 0x5455 extended-timestamp field survived the rewrite. Reading the
    // wire directly keeps the assertion about what was *written*,
    // independent of any read backend.
    use zip::extra_fields::ExtraField;
    let f = std::fs::File::open(&zip_path).unwrap();
    let mut zip = zip::ZipArchive::new(f).unwrap();
    let zf = zip.by_name("ts.txt").unwrap();
    let ext = zf
        .extra_data_fields()
        .find_map(|e| match e {
            ExtraField::ExtendedTimestamp(t) => Some(t),
            _ => None,
        })
        .expect("OI-0065-002: 0x5455 extended-timestamp must survive ZIP commit");

    let secs = |t: std::time::SystemTime| -> u32 {
        t.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32
    };
    assert_eq!(
        ext.ac_time(),
        Some(secs(atime)),
        "accessed timestamp must round-trip via 0x5455"
    );
    assert_eq!(
        ext.cr_time(),
        Some(secs(btime)),
        "created timestamp must round-trip via 0x5455"
    );
}
