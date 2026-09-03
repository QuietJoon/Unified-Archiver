//! The id-addressed route is the supported way to reach an entry whose
//! archived name is not valid UTF-8 (AD 0064 amendment 2026-09-03,
//! OI-0076-001 Required Action 5 / R0076-0083).
//!
//! R0076-0083 asked for a new "extract-by-raw-bytes" surface on the
//! grounds that "callers cannot extract a non-UTF-8 entry by exact
//! name". They can — by id rather than by name — and these tests pin the
//! three facts that make that the answer rather than a workaround:
//!
//! 1. `raw_path` carries the exact stored bytes, so a caller can *find*
//!    the entry they mean.
//! 2. Distinct raw names can collapse to one lossy `path`, so the
//!    `&str`-keyed routes are genuinely ambiguous — and refuse rather
//!    than guess, naming `extract_by_ids` in the refusal.
//! 3. `extract_by_ids` selects positionally and never consults the name,
//!    so it resolves that ambiguity.
//!
//! And the limit, asserted rather than assumed: the id route picks the
//! right *entry*, not a byte-faithful *destination name*. The file lands
//! under the lossy name, because the destination hand-off is still one of
//! the lossy sites OI-0076-001 Required Actions 2 and 4 track.
//!
//! Unix-only: the assertions read destination names as raw bytes.

#![cfg(unix)]

use super::common;

use std::os::unix::ffi::OsStrExt;

use unified_archive::{Archive, ExtractionOptions};

/// `0xFF` and `0xFE` are not valid UTF-8 in any position, so each of these
/// lossy-decodes to the *same* `caf\u{FFFD}.txt` while staying distinct on
/// the wire — the collision the by-name routes cannot resolve.
const RAW_A: &[u8] = b"caf\xFF.txt";
const RAW_B: &[u8] = b"caf\xFE.txt";
const LOSSY: &str = "caf\u{FFFD}.txt";

/// tar stores the member name as raw bytes with no encoding attached, so
/// the crate's own ustar writer is the only portable way to produce this
/// shape — a host `tar` would not make one on request.
fn build(path: &std::path::Path) {
    common::write_tar(
        path,
        &[
            common::TarMember::file("plain.txt", b"plain"),
            common::TarMember::file_raw(RAW_A, b"AAAA"),
            common::TarMember::file_raw(RAW_B, b"BBBB"),
        ],
    );
}

#[test]
fn raw_path_carries_the_exact_stored_bytes() {
    let tmp = common::temp_test_dir();
    let archive_path = tmp.join("raw-names.tar");
    build(&archive_path);

    let archive = Archive::open(&archive_path).expect("open");
    let entries = archive.list_files().expect("list");

    assert_eq!(entries[1].raw_path(), Some(RAW_A));
    assert_eq!(entries[2].raw_path(), Some(RAW_B));
    assert_eq!(
        entries[0].raw_path(),
        None,
        "an ASCII name needs no raw form: `path` is already byte-exact"
    );

    // The premise of the whole issue: two distinct entries, one string.
    assert_eq!(entries[1].path, LOSSY);
    assert_eq!(entries[2].path, entries[1].path);

    common::cleanup(&tmp);
}

/// The `&str`-keyed single-entry routes refuse the collision instead of
/// picking one, and the refusal names the route that can resolve it. This
/// is why no new by-raw-bytes surface is owed: the crate already tells
/// callers where to go.
#[test]
fn the_name_keyed_route_refuses_the_collision_and_names_the_id_route() {
    let tmp = common::temp_test_dir();
    let archive_path = tmp.join("raw-names.tar");
    build(&archive_path);

    let archive = Archive::open(&archive_path).expect("open");
    let err = archive
        .extract_to_memory(LOSSY)
        .expect_err("two entries share this lossy name");
    let text = err.to_string();
    assert!(
        text.contains("extract_by_ids"),
        "the refusal must point at the route that can resolve it, got: {text}"
    );

    common::cleanup(&tmp);
}

/// A *uniquely* named non-UTF-8 entry still round-trips through its lossy
/// `path`, so the id route is the answer for the ambiguous case, not a
/// blanket replacement for by-name access.
#[test]
fn a_unique_non_utf8_name_still_round_trips_by_name() {
    let tmp = common::temp_test_dir();
    let archive_path = tmp.join("unique.tar");
    common::write_tar(
        &archive_path,
        &[common::TarMember::file_raw(b"only\xFF.txt", b"CCCC")],
    );

    let archive = Archive::open(&archive_path).expect("open");
    let entry = archive.list_files().expect("list")[0].clone();
    assert_eq!(entry.raw_path(), Some(&b"only\xFF.txt"[..]));

    let bytes = archive
        .extract_to_memory(&entry.path)
        .expect("one entry bears this lossy name, so it is unambiguous");
    assert_eq!(bytes, b"CCCC");

    common::cleanup(&tmp);
}

/// The ruling's substance: selection is positional, so the id route
/// reaches the entry the caller meant even though its name cannot be
/// spelled.
#[test]
fn extract_by_ids_reaches_the_entry_the_name_cannot_address() {
    let tmp = common::temp_test_dir();
    let archive_path = tmp.join("raw-names.tar");
    build(&archive_path);

    let archive = Archive::open(&archive_path).expect("open");
    let entries = archive.list_files().expect("list");
    // Pick by the bytes, act by the id — the documented pairing.
    let wanted = entries
        .iter()
        .find(|e| e.raw_path() == Some(RAW_B))
        .expect("the 0xFE entry is listed");

    let dest = tmp.join("out");
    archive
        .extract_by_ids(&[wanted.id], ExtractionOptions::new(&dest))
        .expect("extract the id-selected entry");

    let written: Vec<_> = std::fs::read_dir(&dest)
        .expect("destination")
        .flatten()
        .collect();
    assert_eq!(written.len(), 1, "exactly the one selected entry");
    assert_eq!(
        std::fs::read(written[0].path()).expect("read it"),
        b"BBBB",
        "the payload must be the 0xFE entry's, not the 0xFF entry it collides with"
    );

    common::cleanup(&tmp);
}

/// The honest limit, pinned so it cannot be quietly assumed away: the id
/// route selects the right entry and writes the right bytes, but the
/// destination *name* is still lossy. Closing that is Required Actions 2
/// and 4 of OI-0076-001, not this ruling.
#[test]
fn the_destination_name_is_still_lossy_which_this_ruling_does_not_close() {
    let tmp = common::temp_test_dir();
    let archive_path = tmp.join("raw-names.tar");
    build(&archive_path);

    let archive = Archive::open(&archive_path).expect("open");
    let entries = archive.list_files().expect("list");
    let wanted = entries
        .iter()
        .find(|e| e.raw_path() == Some(RAW_A))
        .expect("the 0xFF entry is listed");

    let dest = tmp.join("out");
    archive
        .extract_by_ids(&[wanted.id], ExtractionOptions::new(&dest))
        .expect("extract");

    let name = std::fs::read_dir(&dest)
        .expect("destination")
        .flatten()
        .next()
        .expect("one file")
        .file_name();
    assert_eq!(
        name.as_bytes(),
        LOSSY.as_bytes(),
        "the on-disk name is the U+FFFD substitution, not the stored 0xFF byte"
    );
    assert_ne!(
        name.as_bytes(),
        RAW_A,
        "if this ever passes, the destination hand-off stopped being lossy \
         and OI-0076-001 Required Actions 2 and 4 can be re-examined"
    );

    common::cleanup(&tmp);
}

/// The control for the case that must NOT change: an ordinary ASCII-named
/// `.gz` still reports `raw_path == None`, because `path` is byte-exact and
/// `None` is what that means.
#[test]
fn an_ascii_named_raw_gz_still_reports_no_raw_path() {
    let temp = tempfile::tempdir().expect("temp dir");
    let payload = b"hello";
    let mut gz: Vec<u8> = vec![0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 0, 0xff];
    gz.extend_from_slice(&[0x01, 0x05, 0x00, 0xfa, 0xff]);
    gz.extend_from_slice(payload);
    gz.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
    gz.extend_from_slice(&(payload.len() as u32).to_le_bytes());

    let path = temp.path().join("plain.gz");
    std::fs::write(&path, &gz).expect("write");

    let archive = Archive::open(&path).expect("open");
    let entries = archive.list_files().expect("list");
    assert_eq!(entries[0].path, "plain");
    assert_eq!(
        entries[0].raw_path(),
        None,
        "an ASCII stem needs no raw form, and inventing one would make \
         `raw_path == None` stop meaning anything"
    );
}
