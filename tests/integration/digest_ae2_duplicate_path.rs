//! DCR-012 + OI-0076-002 regression at the **facade** level: an AE-2 AES
//! ZIP whose entry names collide after normalization must still produce a
//! content-multiset digest, and each occurrence's own payload must
//! contribute to it.
//!
//! Why this test exists at this altitude. DCR-012 made AE-2 AES entries
//! list `crc32: None` (their stored CRC is the specification's
//! placeholder, not a checksum), which routes them through the digest
//! walk's streaming arm. That arm addresses entries by stable listing id
//! — but the id-based stream only reaches the ZIP backend if
//! `ReadBackend::extract_to_stream_by_listing_id` forwards to the
//! inherent `ZipArchive` method. When it did not, the trait default
//! returned `NotImplemented`, the walk fell back to the by-*path* stream,
//! and `security::validate_single_entry` / `reject_if_duplicate` refused
//! the archive with `OperationBlocked { reason: "Multiple entries match
//! ..." }` — the OI-0076-002 defect, reintroduced through ZIP.
//!
//! The missing forward produced no compile error, and every test written
//! against the inherent method passed, because calling
//! `ZipArchive::extract_to_stream_by_listing_id` directly bypasses the
//! dispatch that was broken. Only a call through `Archive` exercises it.
//! Keep these assertions on the facade.

use super::common;

use std::io::Write as _;
use std::path::Path;

use unified_archive::Archive;

const PASSWORD: &str = "dup-pw";

/// A payload short enough that the `zip` crate picks AE-2 (it switches to
/// AE-1 at 20 bytes, where a real CRC32 is stored and the entry would
/// never reach the streaming arm at all).
fn assert_is_ae2_length(payload: &[u8]) {
    assert!(
        payload.len() < 20,
        "payload must stay under 20 bytes or the writer emits AE-1, \
         which lists a real CRC32 and does not exercise this path"
    );
}

/// Build an AE-2 AES ZIP holding two entries whose *raw* names differ but
/// which normalize to the same listing path: `sub/a.txt` and `sub\a.txt`
/// (R0076-0048). Both are separate central-directory records, so the
/// listing carries the path twice.
fn build_ae2_dup_after_normalization_zip(path: &Path, first: &[u8], second: &[u8]) {
    assert_is_ae2_length(first);
    assert_is_ae2_length(second);

    let file = std::fs::File::create(path).expect("create AES zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .with_aes_encryption(zip::AesMode::Aes256, PASSWORD);

    writer
        .start_file("sub/a.txt", options)
        .expect("start forward-slash entry");
    writer.write_all(first).expect("write first payload");

    // Backslash separator: a distinct raw name that the crate stores
    // verbatim, and that the listing normalizes onto the same path.
    writer
        .start_file("sub\\a.txt", options)
        .expect("start backslash entry");
    writer.write_all(second).expect("write second payload");

    writer.finish().expect("finish AES zip");
}

/// Expected digest for a two-entry archive: elements are bare 8-hex CRC32
/// values, sorted, comma-joined, and CRC32-folded (DCR-012).
fn expected_digest(payloads: [&[u8]; 2]) -> String {
    let mut elements: Vec<String> = payloads
        .iter()
        .map(|p| format!("{:08x}", crc32fast::hash(p)))
        .collect();
    elements.sort();
    format!("{:08x}", crc32fast::hash(elements.join(",").as_bytes()))
}

/// The headline case: the digest succeeds where the by-path fallback
/// returned `OperationBlocked`, and its value proves both payloads were
/// hashed distinctly.
#[test]
fn ae2_duplicate_after_normalization_zip_digests_each_payload() {
    let tmp = common::temp_test_dir();

    let first: &[u8] = b"alpha";
    let second: &[u8] = b"bravo";

    let dup = tmp.join("ae2-dup.zip");
    build_ae2_dup_after_normalization_zip(&dup, first, second);

    let archive = Archive::open_encrypted(&dup, PASSWORD).expect("open AE-2 zip with password");

    // The fixture really is the duplicate-after-normalization shape, and
    // the entries really are CRC-less — otherwise the assertions below
    // would pass without touching the dispatch under test.
    let entries = archive.list_files().expect("list AE-2 zip");
    let collisions: Vec<_> = entries.iter().filter(|e| e.path == "sub/a.txt").collect();
    assert_eq!(
        collisions.len(),
        2,
        "fixture must list sub/a.txt twice (raw names differ, normalized names collide), got {:?}",
        entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );
    for entry in &collisions {
        assert!(entry.is_encrypted, "AE-2 entries list as encrypted");
        assert_eq!(
            entry.crc32, None,
            "an AE-2 entry's placeholder CRC must not be listed as a checksum (DCR-012)"
        );
    }

    // (a) The digest is produced at all. Before the ReadBackend forward
    // was wired this returned Err(OperationBlocked { operation:
    // "extract_to_stream", reason: "Multiple entries match ..." }).
    let (digest, total) = archive
        .calculate_content_multiset_digest_and_size()
        .expect("digest must succeed on a duplicate-after-normalization AE-2 ZIP");

    // (b) Each occurrence contributed its OWN payload. A resolver that
    // re-read the first occurrence for both would produce
    // expected_digest([first, first]).
    assert_eq!(
        digest,
        expected_digest([first, second]),
        "each occurrence must contribute its own decrypted payload's CRC32"
    );
    assert_ne!(
        digest,
        expected_digest([first, first]),
        "the shadowed occurrence must not be re-hashed from the first one"
    );

    assert_eq!(
        total,
        (first.len() + second.len()) as u64,
        "total size sums both occurrences"
    );

    common::cleanup(&tmp);
}

/// Content sensitivity through the same path: two archives that differ
/// only in the shadowed occurrence's payload must digest differently.
/// This is what the pre-DCR-012 `Some(0)` listing destroyed — every AE-2
/// entry folded the same constant, so distinct AES archives collided.
#[test]
fn ae2_duplicate_path_digest_tracks_the_shadowed_payload() {
    let tmp = common::temp_test_dir();

    let control = tmp.join("ae2-control.zip");
    let variant = tmp.join("ae2-variant.zip");
    build_ae2_dup_after_normalization_zip(&control, b"alpha", b"alpha");
    build_ae2_dup_after_normalization_zip(&variant, b"alpha", b"bravo");

    let control_digest = Archive::open_encrypted(&control, PASSWORD)
        .expect("open control")
        .calculate_content_multiset_digest_and_size()
        .expect("digest control")
        .0;
    let variant_digest = Archive::open_encrypted(&variant, PASSWORD)
        .expect("open variant")
        .calculate_content_multiset_digest_and_size()
        .expect("digest variant")
        .0;

    assert_ne!(
        control_digest, variant_digest,
        "two AE-2 archives differing only in the shadowed payload must not collide"
    );

    common::cleanup(&tmp);
}

/// The new public-API precondition, pinned so it cannot regress silently:
/// resolving an AE-2 entry's CRC32 means decrypting it, so a handle opened
/// without a password can no longer digest the archive. It used to return
/// `Ok` — by folding the placeholder 0 for every AE-2 entry, which is the
/// collision DCR-012 removed, so the old success was wrong rather than
/// merely cheaper.
///
/// The error *variant* is deliberately not asserted: the ZIP no-password
/// read path still reports `ArchiveError::Format` carrying a
/// "Password required to decrypt file" message rather than
/// `ArchiveError::Password`. That mislabel predates this change and is
/// owned by ticgit `9bdf2c`; that ticket must be free to sharpen the
/// classification without editing this test.
#[test]
fn ae2_digest_without_password_is_an_error_not_a_placeholder_digest() {
    let tmp = common::temp_test_dir();

    let path = tmp.join("ae2-nopw.zip");
    build_ae2_dup_after_normalization_zip(&path, b"alpha", b"bravo");

    let archive = Archive::open(&path).expect("listing an encrypted ZIP needs no password");
    // Listing still works without the password (AD 0014).
    assert_eq!(
        archive.list_files().expect("list without password").len(),
        2
    );

    assert!(
        archive
            .calculate_content_multiset_digest_and_size()
            .is_err(),
        "an AE-2 payload cannot be hashed without the password; \
         returning Ok would mean a placeholder was folded in instead"
    );

    common::cleanup(&tmp);
}
