//! Single-entry defense parity across backends (OI-0076-002 / R0076-0090).
//!
//! Direct backend single-entry calls must enforce the same policy the
//! facade gate does — exactly one normalized-path match, regular files
//! only — because the `ffi` module is public and callers can bypass the
//! facade. Every backend now routes its single-entry methods through
//! `security::validate_single_entry`, so these tests drive the BACKEND
//! methods directly; the facade paths are covered by the existing
//! extraction suites.
//!
//! Coverage notes: RAR is read-only here (no writer or CLI available to
//! build directory/symlink/duplicate fixtures), so it is covered by the
//! not-found shape only; 7z duplicate rejection is covered at the unit
//! level in `src/ffi/sevenz_wrapper.rs` (the 7z writer is a lib
//! dependency, not a dev-dependency).

use super::common;
use super::common::command_exists;

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;

use unified_archive::ArchiveError;
use unified_archive::ffi::libarchive_wrapper::LibarchiveArchive;
use unified_archive::ffi::sevenz_wrapper::SevenZArchive;
use unified_archive::ffi::zip_wrapper::ZipArchive;

// ── helpers ──

/// Unwrap the expected rejection and hand back its Debug text.
fn rejection_text<T>(res: Result<T, ArchiveError>, ctx: &str) -> String {
    match res {
        Ok(_) => panic!("{ctx}: expected rejection, got Ok"),
        Err(e) => format!("{e:?}"),
    }
}

fn assert_contains(haystack: &str, needle: &str, ctx: &str) {
    assert!(
        haystack.contains(needle),
        "{ctx}: expected error containing {needle:?}, got: {haystack}"
    );
}

/// Build a ZIP exercising every gate branch: regular `a.txt`, directory
/// `subdir/`, symlink `link.txt -> a.txt`, and a duplicated name
/// `dup0.txt` (written as `dup0.txt` + `dup1.txt`, then byte-patched —
/// entry names are not covered by any checksum, mirroring the
/// R0079-0026 fixture technique from the since-removed piz unit tests).
fn build_parity_zip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let plain =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    writer.start_file("a.txt", plain).unwrap();
    writer.write_all(b"payload-a").unwrap();
    writer.add_directory("subdir/", plain).unwrap();
    writer.add_symlink("link.txt", "a.txt", plain).unwrap();
    writer.start_file("dup0.txt", plain).unwrap();
    writer.write_all(b"first").unwrap();
    writer.start_file("dup1.txt", plain).unwrap();
    writer.write_all(b"second").unwrap();
    writer.finish().unwrap();

    let mut bytes = fs::read(path).unwrap();
    let needle = b"dup1.txt";
    for i in 0..bytes.len().saturating_sub(needle.len() - 1) {
        if &bytes[i..i + needle.len()] == needle {
            bytes[i..i + needle.len()].copy_from_slice(b"dup0.txt");
        }
    }
    fs::write(path, &bytes).unwrap();
}

/// Directory-carrying 7z via the CLI (`7zz`/`7z`); false = skip.
fn build_7z_with_dir(archive_path: &Path, staging: &Path) -> bool {
    let cli = if command_exists("7zz") {
        "7zz"
    } else if command_exists("7z") {
        "7z"
    } else {
        return false;
    };
    fs::create_dir_all(staging.join("subdir")).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello\n").expect("write regular");
    Command::new(cli)
        .args([
            "a",
            "-y",
            archive_path.to_str().unwrap(),
            "regular.txt",
            "subdir",
        ])
        .current_dir(staging)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// TAR with a directory entry plus a duplicated member name
/// (`tar -rf` append — the legal way duplicates arise in real tars);
/// false = skip.
fn build_tar_with_dir_and_dup(archive_path: &Path, staging: &Path) -> bool {
    if !command_exists("tar") {
        return false;
    }
    fs::create_dir_all(staging.join("subdir")).expect("staging dir");
    fs::write(staging.join("regular.txt"), b"hello\n").expect("write regular");
    let created = Command::new("tar")
        .args([
            "cf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "regular.txt",
            "subdir",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !created {
        return false;
    }
    // Append the same member again — duplicate path, distinct payload.
    fs::write(staging.join("regular.txt"), b"world\n").expect("rewrite regular");
    Command::new("tar")
        .args([
            "rf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "regular.txt",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── ZIP (sole `zip`-crate backend after the AD 0007 collapse) ──
//
// These once compared the piz and zip-crate readers for parity; with piz
// removed (DCR-009) they assert the security contract on the single ZIP
// backend. The duplicate-name case additionally pins the re-homed
// R0079-0026 rejection: the `zip` crate collapses byte-identical
// central-directory names, so the backend re-scans the raw central
// directory and refuses the ambiguous name rather than silently handing
// back the surviving record.

#[test]
fn zip_backend_rejects_directory_entries() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("parity.zip");
    build_parity_zip(&archive);

    let zipb = ZipArchive::open(&archive).unwrap();
    let dir_path = zipb
        .list_files()
        .unwrap()
        .iter()
        .find(|e| e.is_directory())
        .expect("fixture must carry a directory entry")
        .path
        .clone();

    let msg = rejection_text(zipb.extract_to_memory(&dir_path), "zip memory dir");
    assert_contains(&msg, "not a regular file", "zip memory dir");

    let out = tmp.join("zip_out");
    fs::create_dir_all(&out).unwrap();
    let msg = rejection_text(
        zipb.extract_file_with_options(&dir_path, &out, true, false),
        "zip file dir",
    );
    assert_contains(&msg, "not a regular file", "zip file dir");
    assert!(
        !out.join("subdir").exists(),
        "rejected directory extraction must not materialize the directory"
    );

    common::cleanup(&tmp);
}

#[test]
fn zip_backend_rejects_symlink_entries() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("parity.zip");
    build_parity_zip(&archive);

    let zipb = ZipArchive::open(&archive).unwrap();
    let entries = zipb.list_files().unwrap();
    let Some(link) = entries.iter().find(|e| e.is_symlink()) else {
        eprintln!("skipping: zip writer did not produce a symlink-flagged entry");
        common::cleanup(&tmp);
        return;
    };
    let link_path = link.path.clone();

    let msg = rejection_text(zipb.extract_to_memory(&link_path), "zip memory symlink");
    assert_contains(&msg, "symbolic link", "zip memory symlink");

    common::cleanup(&tmp);
}

#[test]
fn zip_backend_duplicate_name_rejected() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("parity.zip");
    build_parity_zip(&archive);

    // The `zip` crate's central-directory map dedupes byte-identical names
    // (later records replace earlier ones), so its listing snapshot holds
    // exactly one `dup0.txt`.
    let zipb = ZipArchive::open(&archive).unwrap();
    let zip_matches = zipb
        .list_files()
        .unwrap()
        .iter()
        .filter(|e| e.path == "dup0.txt")
        .count();
    assert_eq!(
        zip_matches, 1,
        "zip crate dedupes duplicate names in its central-directory map"
    );

    // R0079-0026 (re-homed, DCR-009): even though the deduped listing shows
    // one record, the raw central directory carried two `dup0.txt` records.
    // The backend re-scans it and refuses the ambiguous single-entry
    // extraction instead of silently returning the surviving payload.
    let msg = rejection_text(zipb.extract_to_memory("dup0.txt"), "zip memory dup");
    assert_contains(&msg, "Multiple entries match", "zip memory dup");
    let out = tmp.join("out");
    fs::create_dir_all(&out).unwrap();
    let msg = rejection_text(
        zipb.extract_file_with_options("dup0.txt", &out, true, false),
        "zip file dup",
    );
    assert_contains(&msg, "Multiple entries match", "zip file dup");

    common::cleanup(&tmp);
}

#[test]
fn zip_backend_not_found_uses_gate_shape() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("parity.zip");
    build_parity_zip(&archive);

    let zipb = ZipArchive::open(&archive).unwrap();
    let msg = rejection_text(zipb.extract_to_memory("nope.txt"), "zip not-found");
    assert_contains(&msg, "not found in archive metadata", "zip not-found");

    common::cleanup(&tmp);
}

/// The happy path must keep working: a unique regular file whose name is
/// NOT duplicated extracts byte-exact even though the same archive carries
/// a duplicated `dup0.txt` elsewhere (the duplicate guard is per-name).
#[test]
fn zip_backend_unique_file_still_extracts() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("parity.zip");
    build_parity_zip(&archive);

    let zipb = ZipArchive::open(&archive).unwrap();
    assert_eq!(zipb.extract_to_memory("a.txt").unwrap(), b"payload-a");

    common::cleanup(&tmp);
}

// ── 7z ──

#[test]
fn sevenz_backend_rejects_directory_entries() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("parity.7z");
    if !build_7z_with_dir(&archive, &staging) {
        eprintln!("skipping: 7z/7zz CLI not available");
        common::cleanup(&tmp);
        return;
    }

    let sz = SevenZArchive::open(&archive).unwrap();
    let dir_path = sz
        .list_files()
        .unwrap()
        .iter()
        .find(|e| e.is_directory())
        .expect("7z fixture must carry a directory entry")
        .path
        .clone();

    let msg = rejection_text(sz.extract_to_memory(&dir_path), "7z memory dir");
    assert_contains(&msg, "not a regular file", "7z memory dir");

    let out = tmp.join("out");
    fs::create_dir_all(&out).unwrap();
    let msg = rejection_text(
        sz.extract_file_with_options(&dir_path, &out, true, false),
        "7z file dir",
    );
    assert_contains(&msg, "not a regular file", "7z file dir");
    assert!(!out.join("subdir").exists());

    common::cleanup(&tmp);
}

#[test]
fn sevenz_backend_not_found_uses_gate_shape() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("parity.7z");
    if !build_7z_with_dir(&archive, &staging) {
        eprintln!("skipping: 7z/7zz CLI not available");
        common::cleanup(&tmp);
        return;
    }

    let sz = SevenZArchive::open(&archive).unwrap();
    let msg = rejection_text(sz.extract_to_memory("nope.txt"), "7z not-found");
    assert_contains(&msg, "not found in archive metadata", "7z not-found");

    common::cleanup(&tmp);
}

// ── TAR (libarchive) ──

#[test]
#[cfg(unix)]
fn libarchive_backend_rejects_directory_and_duplicate_entries() {
    let tmp = common::temp_test_dir();
    let staging = tmp.join("staging");
    let archive = tmp.join("parity.tar");
    if !build_tar_with_dir_and_dup(&archive, &staging) {
        eprintln!("skipping: tar CLI not available");
        common::cleanup(&tmp);
        return;
    }

    let la = LibarchiveArchive::open(&archive).unwrap();
    let entries = la.list_files_metadata_only().unwrap();

    let dir_path = entries
        .iter()
        .find(|e| e.is_directory())
        .expect("tar fixture must carry a directory entry")
        .path
        .clone();
    let msg = rejection_text(la.extract_to_memory(&dir_path), "tar memory dir");
    assert_contains(&msg, "not a regular file", "tar memory dir");

    let out = tmp.join("out");
    fs::create_dir_all(&out).unwrap();
    let msg = rejection_text(la.extract_file(&dir_path, &out), "tar file dir");
    assert_contains(&msg, "not a regular file", "tar file dir");
    assert!(!out.join("subdir").exists());

    // Appended duplicate member: ambiguous by name across all
    // single-entry surfaces (memory, disk, stream).
    let msg = rejection_text(la.extract_to_memory("regular.txt"), "tar memory dup");
    assert_contains(&msg, "Multiple entries match", "tar memory dup");
    let msg = rejection_text(la.extract_file("regular.txt", &out), "tar file dup");
    assert_contains(&msg, "Multiple entries match", "tar file dup");
    let msg = match la.extract_to_stream("regular.txt") {
        Ok(_) => panic!("tar stream dup: expected rejection, got Ok"),
        Err(e) => format!("{e:?}"),
    };
    assert_contains(&msg, "Multiple entries match", "tar stream dup");

    common::cleanup(&tmp);
}

#[test]
#[cfg(unix)]
fn libarchive_backend_rejects_link_entries() {
    let tmp = common::temp_test_dir();

    // Symlink tar via the shared builder (regular.txt + link.txt).
    let sym_tar = tmp.join("symlink.tar");
    if !common::build_tar_with_symlink(&sym_tar, &tmp.join("sym_staging")) {
        eprintln!("skipping: cannot build symlink tar fixture");
        common::cleanup(&tmp);
        return;
    }
    let la = LibarchiveArchive::open(&sym_tar).unwrap();
    let msg = rejection_text(la.extract_to_memory("link.txt"), "tar memory symlink");
    assert_contains(&msg, "symbolic link", "tar memory symlink");
    let msg = match la.extract_to_stream("link.txt") {
        Ok(_) => panic!("tar stream symlink: expected rejection, got Ok"),
        Err(e) => format!("{e:?}"),
    };
    assert_contains(&msg, "symbolic link", "tar stream symlink");

    // Hardlink tar via the shared builder (regular.txt + hardlink.txt).
    let hard_tar = tmp.join("hardlink.tar");
    if !common::build_tar_with_hardlink(&hard_tar, &tmp.join("hard_staging")) {
        eprintln!("skipping: cannot build hardlink tar fixture");
        common::cleanup(&tmp);
        return;
    }
    let la = LibarchiveArchive::open(&hard_tar).unwrap();
    let msg = rejection_text(la.extract_to_memory("hardlink.txt"), "tar memory hardlink");
    assert_contains(&msg, "hard link", "tar memory hardlink");

    common::cleanup(&tmp);
}

// ── RAR (read-only coverage: fixture cannot carry dirs/links/dups) ──

#[cfg(feature = "rar-support")]
#[test]
fn rar_backend_not_found_uses_gate_shape() {
    use unified_archive::ffi::wrapper::UnrarArchive;

    let rar = UnrarArchive::open(common::fixture("test.rar")).expect("open RAR fixture");
    let msg = rejection_text(rar.extract_to_memory("no_such_entry.txt"), "rar not-found");
    assert_contains(&msg, "not found in archive metadata", "rar not-found");
}
