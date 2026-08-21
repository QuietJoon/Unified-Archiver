//! In-place payload access tests for [`Archive`](super::Archive).
//!
//! Moved out of `src/archive.rs` so that file stops growing: it is a named
//! D10 large-file target in AD-0057, and the same change that introduced
//! these tests moved ~2,700 test lines out of three FFI files on exactly
//! this principle. A child module reaches every private item of its
//! ancestors, so nothing had to be widened to make the move compile.

/// Ticket `1ddc37ec`: the accounting an in-place open must not get
/// wrong. Reading the payload where it lies means the handle's source
/// file is the *whole* SFX — stub included — so any size the crate
/// derives from that file has to subtract the payload offset again.
/// `payload_size_for_ratio` is the one that matters: it is the
/// denominator of the compression-ratio (zip-bomb) gate, and counting
/// stub bytes in it would dilute the ratio by the size of the stub and
/// quietly loosen the gate (R0069-0006).
use super::{Archive, PayloadAccess};
use crate::test_utils::fixture;
use std::io::Write;

fn build_sfx(dir: &std::path::Path, name: &str, payload: &[u8]) -> (std::path::PathBuf, u64) {
    let stub = b"#!/bin/sh\necho 'sfx stub'\n";
    let padding = vec![0u8; 400];
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(stub).unwrap();
    file.write_all(&padding).unwrap();
    file.write_all(payload).unwrap();
    file.flush().unwrap();
    (path, (stub.len() + padding.len()) as u64)
}

#[test]
fn in_place_ratio_denominator_excludes_the_stub() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).unwrap();
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &zip_bytes);

    let archive = Archive::open_at_offset(&path, offset).expect("in-place open");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
    assert_eq!(
        archive.payload_size_for_ratio().unwrap(),
        zip_bytes.len() as u64,
        "the ratio denominator must be the payload, not stub + payload",
    );
    // The stub is real, so the whole-file length is strictly larger —
    // i.e. the subtraction is load-bearing, not a no-op.
    assert!(
        std::fs::metadata(&path).unwrap().len() > zip_bytes.len() as u64,
        "fixture must actually carry a stub",
    );
}

#[test]
fn staged_ratio_denominator_is_the_staged_payload() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).unwrap();
    // No executable extension: this one stages, and the staged
    // tempfile is exactly the payload — same denominator, other path.
    let (path, offset) = build_sfx(dir.path(), "payload.bin", &zip_bytes);

    let archive = Archive::open_at_offset(&path, offset).expect("staged open");
    assert_eq!(archive.payload_access(), PayloadAccess::Staged);
    assert_eq!(
        archive.payload_size_for_ratio().unwrap(),
        zip_bytes.len() as u64,
    );
}

/// The in-place path does not lose the detection→open identity
/// binding (OI-0081-001 / R0001-0002). Staging revalidates at its
/// copy-source open; in place there is no copy, so the check runs
/// once the backend exists — the same shape as `Archive::open`'s.
/// Unix-only: the drift is produced by `rename`, which hands the
/// pathname a fresh inode at an identical length, and the non-Unix
/// identity is length-only.
#[cfg(unix)]
#[test]
fn in_place_open_still_refuses_a_post_detection_inode_swap() {
    use super::{DEFAULT_SFX_PAYLOAD_CAP, capture_read_identity};
    use crate::error::ArchiveError;

    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).unwrap();
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &zip_bytes);
    let identity = capture_read_identity(&path).expect("capture");

    // A different inode of the SAME length, and still a valid
    // in-place ZIP SFX — only a stub padding byte differs — so the
    // in-place probe succeeds and the identity check is what
    // refuses it.
    let bytes = std::fs::read(&path).unwrap();
    let mut swapped = bytes.clone();
    swapped[20] = 0xFF; // inside the zero padding, before the payload
    let replacement = dir.path().join("replacement.sh");
    std::fs::write(&replacement, &swapped).unwrap();
    std::fs::rename(&replacement, &path).unwrap();

    // `Archive` is not `Debug`, so match instead of `expect_err`.
    let err = match Archive::open_at_offset_with_format_hint_and_progress(
        &path,
        offset,
        DEFAULT_SFX_PAYLOAD_CAP,
        Some(crate::format::ArchiveFormat::Zip),
        Some(identity),
        None,
    ) {
        Ok(_) => panic!("an in-place open must refuse a post-detection inode swap"),
        Err(e) => e,
    };
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "expected OperationBlocked, got: {err:?}",
    );
    assert!(
        err.to_string().contains("identity changed while opening"),
        "message must name the identity drift, got: {err}",
    );
}

/// The source used for internal reopens differs by path: the staged
/// copy for a staged handle, the caller's own file for an in-place
/// one (the backend re-derives the offset from it).
#[test]
fn reopen_source_follows_the_payload_path() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).unwrap();

    let (in_place_path, offset) = build_sfx(dir.path(), "installer.sh", &zip_bytes);
    let in_place = Archive::open_at_offset(&in_place_path, offset).expect("in-place open");
    assert_eq!(in_place.source_path_for_reopen(), in_place_path.as_path());

    let (staged_path, offset) = build_sfx(dir.path(), "payload.bin", &zip_bytes);
    let staged = Archive::open_at_offset(&staged_path, offset).expect("staged open");
    assert_ne!(
        staged.source_path_for_reopen(),
        staged_path.as_path(),
        "a staged handle must reopen the tempfile, not the outer file",
    );
}
