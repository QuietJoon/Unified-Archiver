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

// Every test here builds a tar fixture, so the whole file needs the
// libarchive backend (AD 0058 format features).
#![cfg(feature = "libarchive")]

use super::common;

use std::path::Path;

use unified_archive::Archive;

/// Build a tar at `archive_path` carrying `member.txt` twice — the shape a
/// `tar -cf` followed by `tar -rf` append produces, and the legal way a
/// duplicate path arises in a real tar. The two occurrences carry `first`
/// then `second` as their payloads.
///
/// OI-0056-010: this used to drive the `tar` CLI and return `false` when it
/// was missing, so the whole lane reported success on a host without `tar`.
/// The shared ustar writer produces the same bytes on every host and cannot
/// fail to produce the duplicate.
fn build_dup_member_tar(archive_path: &Path, first: &[u8], second: &[u8]) {
    common::write_tar(
        archive_path,
        &[
            common::TarMember::file("member.txt", first),
            common::TarMember::file("member.txt", second),
        ],
    );
}

#[test]
fn duplicate_path_tar_digests_both_payloads_distinctly() {
    let tmp = common::temp_test_dir();

    let first = b"first-payload";
    let second = b"second-payload";

    // Archive under test: member.txt appears twice with distinct payloads.
    let dup = tmp.join("dup.tar");
    build_dup_member_tar(&dup, first, second);

    // Control: the same duplicate path, but BOTH occurrences carry the
    // first payload. A digest built by re-hashing the first occurrence
    // for every duplicate (the pre-fix by-path behavior) would make `dup`
    // and this control collide.
    let control = tmp.join("control.tar");
    build_dup_member_tar(&control, first, first);

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

    // (c) total size sums BOTH occurrences' payloads. TAR declares every
    // member's size, so the OI-0001-007 total is complete here and
    // `exact()` is the assertion that would notice a dropped occurrence as
    // a coverage change rather than only as a smaller sum.
    assert_eq!(
        dup_total.exact(),
        Some((first.len() + second.len()) as u64),
        "total size must sum both duplicate-path payloads"
    );
}
