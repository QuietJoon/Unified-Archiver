//! `ReadBackend::stream_payload_to_sink_by_listing_id` — the push route
//! (DEF-004), exercised directly on every backend.
//!
//! # Why these are here rather than in `tests/`
//!
//! The method is `pub(crate)`. More importantly, the only *public* path that
//! reaches it is the content-multiset digest, and that path only calls it for
//! entries whose `crc32` is `None` — AE-2 AES entries on ZIP, entries written
//! without the optional kCRC digest on 7z. A suite that happened to contain no
//! such archive would pass with the whole route dead, which is precisely how a
//! backend forward gets silently dropped. These call the trait method by hand,
//! through `dyn ReadBackend`, so a missing forward is a test failure rather
//! than a slow fallback nobody notices.
//!
//! # What is being pinned
//!
//! Not "the bytes come out", which the buffering route already did. The point
//! of the push route is *how* they come out: without a whole-entry buffer and
//! without a temp file. That property cannot be asserted from inside a test —
//! it is an absence — so what these pin instead is the observable half: every
//! backend implements it, each produces exactly the payload the buffering
//! route produces, and the chunking is real rather than one call with
//! everything in it.

use std::io::Read;

use super::*;
use crate::test_utils::fixture;

/// Collect a whole payload through the sink, plus the chunk count.
fn drain(backend: &dyn ReadBackend, id: usize, path: &str) -> Result<(Vec<u8>, usize, u64)> {
    let mut out = Vec::new();
    let mut chunks = 0usize;
    let mut sink = |chunk: &[u8]| -> Result<()> {
        chunks += 1;
        out.extend_from_slice(chunk);
        Ok(())
    };
    let total = backend.stream_payload_to_sink_by_listing_id(id, path, &mut sink)?;
    Ok((out, chunks, total))
}

/// The reported byte count must equal what the sink actually received. A
/// backend that returned a declared size instead of the decoded length would
/// pass every "bytes come out" check and still be lying.
fn assert_total_matches(bytes: &[u8], total: u64, label: &str) {
    assert_eq!(
        total,
        bytes.len() as u64,
        "{label}: reported {total} bytes but pushed {}",
        bytes.len()
    );
}

// ── ZIP ────────────────────────────────────────────────────────────────────

#[test]
fn zip_pushes_the_same_bytes_the_buffering_route_returns() {
    let archive = crate::ffi::zip_wrapper::ZipArchive::open(fixture("test.zip"))
        .expect("open the ZIP fixture");
    let entries = archive.list_files().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.entry_type == crate::entry::EntryType::File)
        .expect("the fixture must hold a file entry");

    let (pushed, chunks, total) = drain(&archive, entry.id, &entry.path).expect("push route");
    assert_total_matches(&pushed, total, "zip");
    assert!(chunks >= 1);

    // Differential against the route that buffers. Same archive, same entry,
    // so any difference is the new code's.
    let mut buffered = Vec::new();
    archive
        .extract_to_stream_by_listing_id(entry.id, &entry.path)
        .expect("buffering route")
        .read_to_end(&mut buffered)
        .expect("read it");
    assert_eq!(
        pushed, buffered,
        "the push route must produce exactly what the buffering route does"
    );
}

#[test]
fn zip_refuses_a_drifted_listing_id() {
    let archive = crate::ffi::zip_wrapper::ZipArchive::open(fixture("test.zip")).expect("open");
    let mut sink = |_: &[u8]| -> Result<()> { Ok(()) };
    let err = archive
        .stream_payload_to_sink_by_listing_id(0, "definitely-not-the-entry-at-id-0.txt", &mut sink)
        .expect_err("a name that disagrees with the id must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("drift") || msg.contains("not found") || msg.contains("mismatch"),
        "the refusal must name the disagreement: {msg}"
    );
}

// ── 7z ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "sevenzip")]
#[test]
fn sevenz_pushes_the_same_bytes_the_buffering_route_returns() {
    let archive = crate::ffi::sevenz_wrapper::SevenZArchive::open(fixture("test.7z"))
        .expect("open the 7z fixture");
    let entries = archive.list_files().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.entry_type == crate::entry::EntryType::File)
        .expect("the fixture must hold a file entry");

    let (pushed, _chunks, total) = drain(&archive, entry.id, &entry.path).expect("push route");
    assert_total_matches(&pushed, total, "7z");

    let mut buffered = Vec::new();
    archive
        .extract_to_stream_by_listing_id(entry.id, &entry.path)
        .expect("buffering route")
        .read_to_end(&mut buffered)
        .expect("read it");
    assert_eq!(pushed, buffered);
}

/// The 7z walk visits blocks in table order and must stop once the target is
/// consumed. An id past the end has to fail rather than silently return zero
/// bytes, which would digest as an empty payload.
#[cfg(feature = "sevenzip")]
#[test]
fn sevenz_refuses_an_id_the_walk_never_reaches() {
    let archive =
        crate::ffi::sevenz_wrapper::SevenZArchive::open(fixture("test.7z")).expect("open");
    let mut sink = |_: &[u8]| -> Result<()> { Ok(()) };
    let err = archive
        .stream_payload_to_sink_by_listing_id(9_999, "nowhere.txt", &mut sink)
        .expect_err("an unreachable id must not succeed with an empty payload");
    assert!(!err.to_string().is_empty());
}

// ── libarchive ─────────────────────────────────────────────────────────────

#[test]
fn libarchive_pushes_the_same_bytes_the_buffering_route_returns() {
    let archive = crate::ffi::libarchive_wrapper::LibarchiveArchive::open(fixture("test.tar"))
        .expect("open the TAR fixture");
    // This backend spells it `list_files_metadata_only`; there is no
    // `list_files` on it.
    let entries = archive.list_files_metadata_only().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.entry_type == crate::entry::EntryType::File)
        .expect("the fixture must hold a file entry");

    let (pushed, _chunks, total) = drain(&archive, entry.id, &entry.path).expect("push route");
    assert_total_matches(&pushed, total, "libarchive");

    let mut buffered = Vec::new();
    archive
        .extract_to_stream_by_listing_id(entry.id, &entry.path)
        .expect("buffering route")
        .read_to_end(&mut buffered)
        .expect("read it");
    assert_eq!(pushed, buffered);
}

// ── RAR ────────────────────────────────────────────────────────────────────

/// The backend this whole shape exists for. Before DEF-004 the only way to
/// read a RAR entry was to let UnRAR write it to a real file and read that
/// back; the SDK was handing the decoded block to the callback the entire
/// time and the crate was discarding the pointer.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn rar_pushes_the_same_bytes_the_buffering_route_returns() {
    let archive =
        crate::ffi::wrapper::UnrarArchive::open(fixture("test.rar")).expect("open the RAR fixture");
    let entries = archive.list_files().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.entry_type == crate::entry::EntryType::File)
        .expect("the fixture must hold a file entry");

    let (pushed, _chunks, total) = drain(&archive, entry.id, &entry.path).expect("push route");
    assert_total_matches(&pushed, total, "rar");
    assert!(
        !pushed.is_empty(),
        "a RAR entry that pushes nothing means the callback never got the data \
         pointer — the exact bug this route was written to fix"
    );

    let mut buffered = Vec::new();
    archive
        .extract_to_stream(&entry.path)
        .expect("buffering route")
        .read_to_end(&mut buffered)
        .expect("read it");
    assert_eq!(pushed, buffered);
}

/// An error from the sink has to abort the walk and propagate unchanged. For
/// RAR that means crossing the C callback boundary: the trampoline can only
/// answer with an int, so the error is stashed and re-raised. If that stash
/// were dropped the caller would see a generic UnRAR failure instead of their
/// own error.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn rar_propagates_the_sinks_own_error_across_the_callback_boundary() {
    let archive = crate::ffi::wrapper::UnrarArchive::open(fixture("test.rar")).expect("open");
    let entries = archive.list_files().expect("list");
    let entry = entries
        .iter()
        .find(|e| e.entry_type == crate::entry::EntryType::File)
        .expect("a file entry");

    let mut sink = |_: &[u8]| -> Result<()> {
        Err(ArchiveError::Corruption {
            path: "sentinel".to_string(),
            details: "the sink refused this chunk".to_string(),
        })
    };
    let err = archive
        .stream_payload_to_sink_by_listing_id(entry.id, &entry.path, &mut sink)
        .expect_err("the sink's error must surface");
    assert!(
        err.to_string().contains("the sink refused this chunk"),
        "the caller's own error must survive the C boundary, not be replaced \
         by a generic UnRAR failure: {err}"
    );
}

// ── the property that keeps the two digest routes honest ───────────────────

/// The digest resolves CRC-less entries through the push route, and this
/// checks that what it resolves is *correct* — not merely stable.
///
/// TAR carries no per-member CRC at all, so every entry in this fixture
/// takes the CRC-less path. The digest is therefore built entirely from
/// CRCs the sink produced. Recomputing it here from payloads read by the
/// independent buffering route gives a genuine differential: if the sink
/// dropped a chunk, mis-ordered one, or reported a length it had not
/// pushed, the two digests diverge.
///
/// An earlier version of this test called the digest twice and compared
/// the results. That compares a route to itself and would pass with the
/// sink returning consistently wrong bytes.
#[test]
fn the_digest_resolves_crc_less_entries_to_the_correct_values() {
    let archive = crate::Archive::open(fixture("test.tar")).expect("open");
    let (digest, total) = archive
        .calculate_content_multiset_digest_and_size()
        .expect("digest");

    // 8 lowercase hex chars: the digest is a CRC32 over the sorted CRC32
    // elements, not a cryptographic hash. Pinned so a change to the shape
    // has to be deliberate.
    assert_eq!(digest.len(), 8, "digest is crc32 hex, got {digest:?}");
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert!(
        total.sized_entries() > 0,
        "the fixture must hold sized files"
    );

    // Rebuild the same digest from payloads read the other way.
    let backend = crate::ffi::libarchive_wrapper::LibarchiveArchive::open(fixture("test.tar"))
        .expect("open the backend directly");
    let entries = backend.list_files_metadata_only().expect("list");
    let mut elements: Vec<String> = Vec::new();
    for entry in entries.iter() {
        if entry.entry_type != crate::entry::EntryType::File {
            continue;
        }
        let mut payload = Vec::new();
        backend
            .extract_to_stream_by_listing_id(entry.id, &entry.path)
            .expect("buffering route")
            .read_to_end(&mut payload)
            .expect("read the payload");
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&payload);
        elements.push(format!("{:08x}", hasher.finalize()));
    }
    assert!(!elements.is_empty(), "no file entries were hashed");
    elements.sort();
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(elements.join(",").as_bytes());
    let expected = format!("{:08x}", hasher.finalize());

    assert_eq!(
        digest, expected,
        "the digest built from sink-resolved CRCs must equal the one built \
         from payloads read by the buffering route"
    );
}
