//! R6 (ti-c0d6fad6 / DCR-011): the digest surface holds a CRC-less entry's
//! payload stream to its declared size **exactly**.
//!
//! `entry_crc32_for_digest` bounded the hasher with a ceiling-only cap, so a
//! truncated entry in a format without a per-entry checksum (plain TAR,
//! CPIO, ISO) digested its short payload and
//! `calculate_content_multiset_digest_and_size` reported success — the same
//! defect class as OI-0001-001 on `extract_to_stream`, surfacing through a
//! different public API.
//!
//! Four properties are pinned here:
//!
//! 1. a truncated plain-TAR member makes the digest return
//!    `ArchiveError::Corruption` naming that entry;
//! 2. a healthy TAR digests to the **same value** it did before the change
//!    (exactness must not move the digest);
//! 3. an entry with no declared size (raw gzip) still digests, its early EOF
//!    tolerated — no declaration is invented;
//! 4. a sparse TAR member digests and streams to a clean EOF, guarding the
//!    assumption that libarchive delivers the full logical extent.

mod common;

use std::io::Read;
use std::path::Path;

use unified_archive::{Archive, ArchiveError, CompressionOptions, StreamBound, WritableFormat};

/// Digest of `tests/fixtures/test.tar` measured on the tree immediately
/// before the R6 change landed. Exactness must not alter the value a
/// well-formed archive produces — only the verdict on a damaged one.
///
/// Provenance caveat: the probe that measured this pre-change was removed
/// with the change, so the value is **trusted, not re-derivable** from the
/// current tree. It still does useful work — it locks the digest against
/// future drift from here — but if it ever fails, do not assume the new
/// behaviour is wrong: re-measure on a commit before the R6 change
/// (DCR-011) and compare, because the constant itself is the weaker half
/// of the assertion.
const HEALTHY_TAR_DIGEST: &str = "f9413c0f";
const HEALTHY_TAR_TOTAL: u64 = 18;

fn write_tar(path: &Path, entry: &str, len: usize) {
    let mut archive = Archive::create(path, CompressionOptions::for_writable(WritableFormat::TAR))
        .expect("create tar");
    archive
        .add_file_from_data(entry, &vec![b'A'; len])
        .expect("add entry");
    archive.finish().expect("finish tar");
}

/// R6 property 1. The shortfall is produced exactly as
/// `stream_bound_test`'s truncation case does it: take the AD 0065 listing
/// snapshot, then rewrite the file on disk so it carries the same entry
/// name with a shorter payload. The name-only drift guards (OI-0001-002)
/// pass it through, so the digest's own stream bound is the last line of
/// defence. Before this change the call returned `Ok` with a digest over
/// the 512-byte payload.
///
/// The rewrite is identity-preserving — same inode, same length — because
/// the read handle is bound to the archive file's identity (OI-0001-002).
/// A construction that moved either half (`std::fs::copy`, which this used
/// to use, moves the length) is refused by the identity guard before the
/// digest walk starts, and the corruption verdict this test exists to pin
/// would never be reached. `tests/listing_identity_test.rs` covers that
/// refusal for the digest surface's siblings.
#[test]
fn truncated_tar_entry_makes_the_digest_report_corruption() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    let short = temp.join("short.tar");

    write_tar(&target, "payload.bin", 4096);
    write_tar(&short, "payload.bin", 512);

    let archive = Archive::open(&target).expect("open tar");
    // Take the snapshot the digest walk will reuse.
    let entries = archive.list_files().expect("list");
    let declared = entries
        .iter()
        .find(|e| e.is_file())
        .and_then(|e| e.size)
        .expect("TAR declares its entry size");
    assert_eq!(declared, 4096);

    common::rewrite_in_place_preserving_identity(&target, &short);

    match archive.calculate_content_multiset_digest_and_size() {
        Ok((digest, total)) => panic!(
            "a 512-byte payload cannot satisfy a 4096-byte declaration \
             (got digest {digest}, total {total})"
        ),
        Err(ArchiveError::Corruption { path, details }) => {
            assert_eq!(path, "payload.bin", "the corrupt entry must be named");
            assert!(
                details.contains("declared size"),
                "the diagnostic should say what was violated, got: {details}"
            );
        }
        Err(other) => panic!("expected Corruption, got: {other}"),
    }

    // The legacy shims inherit the verdict.
    assert!(
        archive.calculate_manifest_digest().is_err(),
        "calculate_manifest_digest must inherit the exactness verdict"
    );
    assert!(
        archive.calculate_manifest_summary().is_err(),
        "calculate_manifest_summary must inherit the exactness verdict"
    );

    common::cleanup(&temp);
}

/// R6 property 2: exactness changes the verdict on damaged archives, never
/// the value produced by a healthy one.
#[test]
fn healthy_tar_digest_value_is_unchanged() {
    let archive = Archive::open(common::fixture("test.tar")).expect("open tar");
    let (digest, total) = archive
        .calculate_content_multiset_digest_and_size()
        .expect("a healthy TAR must still digest");
    assert_eq!(
        digest, HEALTHY_TAR_DIGEST,
        "the exactness bound must not move the digest value"
    );
    assert_eq!(total, HEALTHY_TAR_TOTAL);
}

/// R6 property 3 / hard constraint 3: the raw gzip reader leaves the size
/// field unset, so there is no declaration to hold the stream to. The
/// entry keeps a ceiling-only bound and its natural EOF stays an ordinary
/// EOF.
#[test]
fn unknown_size_entry_still_digests() {
    let archive = Archive::open(common::fixture("test.gz")).expect("open gz");
    let entries = archive.list_files().expect("list");
    let declared = entries.iter().find(|e| e.is_file()).and_then(|e| e.size);
    assert_eq!(
        declared, None,
        "the raw gzip reader declares no uncompressed size"
    );

    let (digest, _) = archive
        .calculate_content_multiset_digest_and_size()
        .expect("an unknown-size entry must still digest");
    assert!(!digest.is_empty());
}

/// R6 known-risk tripwire: exactness assumes libarchive delivers an entry's
/// full *logical* extent, holes included. A sparse TAR member whose data
/// blocks are shorter than its declared size would otherwise read as a
/// truncation and produce a false `Corruption`.
///
/// ## Why this lane is `#[ignore]`d (OI-0056-010)
///
/// It needs a `tar` that actually sparse-**encodes** a member. bsdtar — the
/// `tar` on macOS, this project's primary dev platform — accepts `-cSf` and
/// exits 0 but documents `-S` as extract-mode-only, so it writes a dense
/// ~1 MiB archive and the hole-materialisation tripwire cannot fire.
///
/// It used to `eprintln!` and return in that case, i.e. report success
/// having proved nothing, on every default run on the primary dev platform.
/// It is now `#[ignore]`d and every former skip is a hard failure, so the
/// lane is either genuinely exercised or visibly absent — never a false
/// green. DCR-011's Known Risk therefore stays *declared* unexercised
/// rather than silently unexercised.
///
/// The fixture is committed rather than built here. `tests/fixtures/sparse.tar`
/// is generated by `scripts/generate-tar-fixtures.sh` on a GNU-tar host and
/// checked in, which is what lets this lane run everywhere instead of sitting
/// `#[ignore]`d.
///
/// It used to shell out to `tar -cSf`, and could not run on this project's
/// macOS host: bsdtar accepts `-S` and ignores it, writing a DENSE archive, so
/// the member was not sparse-encoded and the tripwire could not fire. Waiting
/// for CI on a GNU-tar host was the recorded plan; committing the bytes is
/// cheaper, needs no CI, and removes the host dependency permanently — the
/// same trade `scripts/generate-rar-fixtures.sh` already makes for RAR.
#[test]
fn sparse_tar_entry_digests_and_streams_to_clean_eof() {
    let archive_path = common::fixture("sparse.tar");

    // The lane only means anything if the member is sparse-ENCODED; a dense
    // archive would degrade it to "a mostly-zero member digests exactly",
    // proving nothing about hole materialisation. Both checks are cheap and
    // neither alone is sufficient — a dense archive can still be small if the
    // input compresses, and a non-GNU typeflag can still be under a megabyte.
    let archive_bytes = std::fs::metadata(&archive_path)
        .expect("stat sparse.tar")
        .len();
    assert!(
        archive_bytes < 1024 * 1024,
        "the committed fixture is a DENSE {archive_bytes}-byte archive, so the member is not \
         sparse-encoded and the hole-materialisation tripwire cannot fire. Regenerate it with \
         scripts/generate-tar-fixtures.sh --force on a GNU-tar host."
    );
    let typeflag = {
        use std::io::Read;
        let mut header = [0u8; 512];
        std::fs::File::open(&archive_path)
            .expect("open sparse.tar")
            .read_exact(&mut header)
            .expect("read the member header");
        header[156]
    };
    assert_eq!(
        typeflag, b'S',
        "the member's typeflag must be 'S' (GNU sparse); got {:?}. The fixture was written by a \
         tar that does not sparse-encode.",
        typeflag as char
    );

    let archive = Archive::open(&archive_path).expect("open sparse tar");
    let entries = archive.list_files().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.is_file())
        .expect("sparse member present");
    let declared = entry.size.expect("TAR declares the logical size");
    assert_eq!(declared, 1024 * 1024, "the declaration is the logical size");
    let entry_path = entry.path.clone();

    // The stream bound is the same machinery the digest now uses, so pin
    // both: a clean EOF at the logical extent, and a successful digest.
    let mut stream = archive
        .extract_to_stream(&entry_path, StreamBound::DeclaredSize)
        .expect("stream opens");
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .expect("a sparse entry must deliver its full logical extent");
    assert_eq!(buf.len() as u64, declared);

    archive
        .calculate_content_multiset_digest_and_size()
        .expect("a sparse entry must digest, not read as a truncation");
}
