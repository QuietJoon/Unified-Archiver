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
use std::process::Command;

use unified_archive::{Archive, ArchiveError, ArchiveFormat, CompressionOptions, StreamBound};

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
    let mut archive =
        Archive::create(path, CompressionOptions::new(ArchiveFormat::Tar)).expect("create tar");
    archive
        .add_file_from_data(entry, &vec![b'A'; len])
        .expect("add entry");
    archive.finish().expect("finish tar");
}

/// R6 property 1. The shortfall is produced exactly as
/// `stream_bound_test`'s truncation case does it: take the AD 0065 listing
/// snapshot, then swap the file on disk for one carrying the same entry
/// name with a shorter payload. The name-only drift guards (OI-0001-002)
/// pass it through, so the digest's own stream bound is the last line of
/// defence. Before this change the call returned `Ok` with a digest over
/// the 512-byte payload.
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

    std::fs::copy(&short, &target).expect("swap in the shorter archive");

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
/// Skipped when the local `tar` cannot build a sparse archive.
#[test]
fn sparse_tar_entry_digests_and_streams_to_clean_eof() {
    let temp = common::temp_test_dir();
    let staging = temp.join("staging");
    std::fs::create_dir_all(&staging).expect("staging dir");

    if !common::command_exists("tar") {
        eprintln!("skipping: tar CLI not available");
        common::cleanup(&temp);
        return;
    }

    // A 1 MiB file that is one byte of data at the end and a hole before it.
    let sparse_file = staging.join("sparse.bin");
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::File::create(&sparse_file).expect("create sparse file");
        f.seek(SeekFrom::Start(1024 * 1024 - 1)).expect("seek");
        f.write_all(b"Z").expect("write tail byte");
    }

    let archive_path = temp.join("sparse.tar");
    let built = Command::new("tar")
        .args([
            "-cSf",
            archive_path.to_str().unwrap(),
            "-C",
            staging.to_str().unwrap(),
            "sparse.bin",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !built {
        eprintln!("skipping: tar could not build a sparse archive");
        common::cleanup(&temp);
        return;
    }

    // The tripwire only means anything if `tar` actually sparse-ENCODED the
    // member. bsdtar — the `tar` on macOS, the primary dev platform — accepts
    // `-cSf` and exits 0, but documents `-S` as extract-mode-only, so it
    // writes a dense ~1 MiB archive and this test would silently degrade to
    // "a mostly-zero member digests exactly", proving nothing about hole
    // materialisation. Detect that from the archive size rather than from the
    // exit status, and skip loudly instead of passing vacuously.
    let archive_bytes = std::fs::metadata(&archive_path)
        .expect("stat sparse.tar")
        .len();
    if archive_bytes >= 1024 * 1024 {
        eprintln!(
            "skipping: this tar wrote a DENSE {archive_bytes}-byte archive, so the member is not \
             sparse-encoded and the hole-materialisation tripwire cannot fire (expected on bsdtar; \
             needs a GNU-tar host). DCR-011's Known Risk stays unexercised here."
        );
        common::cleanup(&temp);
        return;
    }

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

    common::cleanup(&temp);
}
