//! ti-2a6e3153 regression: content-multiset digest on a duplicate-path
//! CRC-less tar must hash each occurrence's real payload.
//!
//! OI-0076-002 routed backend single-entry streaming through
//! `validate_single_entry`, which rejects duplicate paths, so
//! `calculate_content_multiset_digest_and_size` errored ("Multiple
//! entries match") on a legitimate `tar -rf`-appended duplicate member
//! instead of producing a digest. Seeking each entry by its stable
//! listing id restores digest support and hashes the shadowed payload
//! distinctly (R0079-0028).

use super::common;
use super::common::command_exists;

use std::fs;
use std::path::Path;
use std::process::Command;

use unified_archive::Archive;

/// Build a tar at `archive_path` with `member.txt` written once via
/// `tar -cf` and appended again via `tar -rf` (the legal way a duplicate
/// path arises in a real tar). The two occurrences carry `first` then
/// `second` as their payloads. Returns false when the `tar` CLI is
/// unavailable so callers can skip gracefully.
fn build_dup_member_tar(archive_path: &Path, staging: &Path, first: &[u8], second: &[u8]) -> bool {
    if !command_exists("tar") {
        return false;
    }
    fs::create_dir_all(staging).expect("staging dir");
    fs::write(staging.join("member.txt"), first).expect("write first payload");
    let created = Command::new("tar")
        .args([
            "cf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "member.txt",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !created {
        return false;
    }
    // Overwrite the staged file and append the same member name — a
    // duplicate path carrying a distinct payload.
    fs::write(staging.join("member.txt"), second).expect("write second payload");
    Command::new("tar")
        .args([
            "rf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "member.txt",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn duplicate_path_tar_digests_both_payloads_distinctly() {
    let tmp = common::temp_test_dir();

    let first = b"first-payload";
    let second = b"second-payload";

    // Archive under test: member.txt appears twice with distinct payloads.
    let dup = tmp.join("dup.tar");
    if !build_dup_member_tar(&dup, &tmp.join("dup_staging"), first, second) {
        eprintln!("skipping: tar CLI not available");
        return;
    }

    // Control: the same duplicate path, but BOTH occurrences carry the
    // first payload. A digest built by re-hashing the first occurrence
    // for every duplicate (the pre-fix by-path behavior) would make `dup`
    // and this control collide.
    let control = tmp.join("control.tar");
    assert!(
        build_dup_member_tar(&control, &tmp.join("control_staging"), first, first),
        "control tar fixture must build once the dup fixture did"
    );

    let dup_archive = Archive::open(&dup).expect("open dup tar");

    // Sanity: the fixture really does carry two same-path entries, so the
    // by-path single-entry gate would have rejected it pre-fix.
    let member_count = dup_archive
        .list_files()
        .expect("list dup")
        .iter()
        .filter(|e| e.path == "member.txt")
        .count();
    assert_eq!(
        member_count, 2,
        "fixture must carry member.txt twice, got {member_count}"
    );

    // (a) digest succeeds where the by-path gate previously errored.
    let (dup_digest, dup_total) = dup_archive
        .calculate_content_multiset_digest_and_size()
        .expect("digest must succeed on a duplicate-path CRC-less tar");

    let control_archive = Archive::open(&control).expect("open control tar");
    let (control_digest, _) = control_archive
        .calculate_content_multiset_digest_and_size()
        .expect("digest must succeed on control tar");

    // (b) the distinct second-occurrence payload changes the digest —
    // proof the shadowed bytes were hashed, not the first occurrence twice.
    assert_ne!(
        dup_digest, control_digest,
        "duplicate-path digest must reflect the second occurrence's distinct payload"
    );

    // (c) total size sums BOTH occurrences' payloads.
    assert_eq!(
        dup_total,
        (first.len() + second.len()) as u64,
        "total size must sum both duplicate-path payloads"
    );
}
