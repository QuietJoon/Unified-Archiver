//! Regression tests for R0075-0003 — SFX staging progress + cancel hook.
//!
//! `Archive::open_with_sfx_progress` lets callers observe (or abort) the
//! payload-copy step that `Archive::open` performs on self-extracting
//! archives. These tests pin both behaviours: the progress callback is
//! invoked with cumulative bytes copied, and returning `false` from a
//! `with_cancel` callback aborts staging with `ArchiveError::Cancelled`.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::NamedTempFile;
use unified_archive::{Archive, ArchiveError, ArchiveFormat, SfxStagingProgress};

/// Build an SFX-shaped fixture by prepending a shebang + zero padding
/// (a recognised `ScriptInterpreter` stub) before a real ZIP payload.
/// `padding_len` controls how many bytes of padding sit between the
/// shebang and the payload — the larger it is, the more bytes the
/// staging copy has to move.
fn build_sfx_zip_fixture(zip_bytes: &[u8], padding_len: usize) -> (NamedTempFile, u64) {
    let mut temp = NamedTempFile::new().unwrap();
    let stub = b"#!/bin/sh\necho 'sfx stub'\n# payload follows\n";
    temp.write_all(stub).unwrap();
    let padding = vec![0u8; padding_len];
    temp.write_all(&padding).unwrap();
    temp.write_all(zip_bytes).unwrap();
    temp.flush().unwrap();
    (temp, zip_bytes.len() as u64)
}

fn read_zip_fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");
    std::fs::read(&path).expect("read tests/fixtures/test.zip")
}

#[test]
fn sfx_open_invokes_staging_progress_callback() {
    let zip_bytes = read_zip_fixture();
    // Use 256 KiB of padding so the staging copy is large enough to
    // exercise the chunked-write loop (64 KiB at a time → 4+ emissions).
    let (sfx_file, payload_size) = build_sfx_zip_fixture(&zip_bytes, 256 * 1024);

    let bytes_seen = Arc::new(AtomicU64::new(0));
    let bytes_seen_cb = Arc::clone(&bytes_seen);
    let cb = SfxStagingProgress::new(move |bytes| {
        let prev = bytes_seen_cb.load(Ordering::Acquire);
        if bytes > prev {
            bytes_seen_cb.store(bytes, Ordering::Release);
        }
    });

    let archive = Archive::open_with_sfx_progress(sfx_file.path(), Some(cb))
        .expect("staging+open should succeed for an SFX-zip");
    let total = bytes_seen.load(Ordering::Acquire);
    // stage_sfx_payload reports `file_len - offset` where offset is
    // the detected archive offset. Detection lands on the ZIP signature,
    // so the running total at end-of-copy equals the ZIP byte count.
    assert_eq!(
        total, payload_size,
        "progress callback should report exactly payload_size at end of copy"
    );
    drop(archive);
}

#[test]
fn sfx_open_no_progress_still_works() {
    // Sanity: passing `None` keeps the existing open behaviour.
    let zip_bytes = read_zip_fixture();
    let (sfx_file, _) = build_sfx_zip_fixture(&zip_bytes, 1024);
    let archive = Archive::open_with_sfx_progress(sfx_file.path(), None)
        .expect("open with no progress hook should still succeed");
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[test]
fn sfx_open_can_be_cancelled_via_progress() {
    // Cancellation fires at the first progress emission (after the
    // first chunk write). Returning `false` from the callback turns
    // the next iteration into `Err(Cancelled)`.
    let zip_bytes = read_zip_fixture();
    let (sfx_file, _) = build_sfx_zip_fixture(&zip_bytes, 0);

    let cb = SfxStagingProgress::with_cancel(|_| false);
    let result = Archive::open_with_sfx_progress(sfx_file.path(), Some(cb));
    match result {
        Err(ArchiveError::Cancelled { operation }) => {
            assert_eq!(
                operation, "sfx_staging",
                "cancellation must surface the staging operation label"
            );
        }
        Err(other) => panic!("expected Cancelled, got error: {other}"),
        Ok(_) => panic!("expected Cancelled, got Ok(Archive)"),
    }
}
