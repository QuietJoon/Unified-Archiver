//! `PayloadWindow` unit tests.
//!
//! The window's whole job is arithmetic, so these are arithmetic tests: a
//! `Cursor` over a known byte pattern stands in for the file, and every
//! case asserts on the bytes actually delivered rather than on positions
//! alone. The two that earn their keep are the frozen-`len` case and the
//! `End`-relative seek — one is the bound that keeps a growing SFX from
//! leaking bytes, the other is what 7z calls before it reads anything.

use super::PayloadWindow;
use std::io::{Cursor, Read, Seek, SeekFrom};

/// `0..n` as bytes, so any slice's content identifies its offset.
fn ramp(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

fn window(bytes: Vec<u8>, start: u64, len: u64) -> PayloadWindow<Cursor<Vec<u8>>> {
    PayloadWindow::new(Cursor::new(bytes), start, len)
}

#[test]
fn reads_start_at_the_window_base_not_the_file_base() {
    let mut w = window(ramp(100), 10, 20);
    let mut buf = [0u8; 5];
    w.read_exact(&mut buf).expect("read the first five bytes");
    assert_eq!(
        buf,
        [10, 11, 12, 13, 14],
        "window byte 0 must be inner byte `start`"
    );
}

#[test]
fn reads_stop_at_the_window_end_even_though_the_file_continues() {
    let mut w = window(ramp(100), 10, 20);
    let mut got = Vec::new();
    w.read_to_end(&mut got).expect("drain the window");
    assert_eq!(got.len(), 20, "a window must not serve past its own length");
    assert_eq!(got.first(), Some(&10u8));
    assert_eq!(got.last(), Some(&29u8));
}

/// The bound that makes an in-place open as safe as the staged copy it
/// replaces: bytes appended after construction are invisible.
#[test]
fn len_is_frozen_so_a_file_that_grows_is_not_followed() {
    // A window sized for a 40-byte file, over a source that is longer —
    // the same shape as an SFX that gained bytes after the gate measured it.
    let mut w = window(ramp(100), 10, 30);
    let mut got = Vec::new();
    w.read_to_end(&mut got).expect("drain");
    assert_eq!(
        got.len(),
        30,
        "the frozen length, not what the source grew to, bounds the read"
    );
}

/// `sevenz_rust2::Archive::read` seeks `End(0)` to learn the archive's
/// length before it reads a byte. If that resolved against the file it
/// would include the stub and everything past the payload.
#[test]
fn end_relative_seeks_resolve_against_the_window_not_the_file() {
    let mut w = window(ramp(100), 10, 20);
    assert_eq!(
        w.seek(SeekFrom::End(0)).expect("seek to window end"),
        20,
        "End(0) is the window's length, not the file's"
    );
    assert_eq!(w.seek(SeekFrom::End(-4)).expect("seek back four"), 16);
    let mut buf = [0u8; 4];
    w.read_exact(&mut buf).expect("read the last four bytes");
    assert_eq!(
        buf,
        [26, 27, 28, 29],
        "End(-4) must land on the window's last four bytes"
    );
}

#[test]
fn current_relative_seeks_move_within_the_window() {
    let mut w = window(ramp(100), 10, 20);
    w.seek(SeekFrom::Start(5)).expect("absolute seek");
    assert_eq!(w.seek(SeekFrom::Current(3)).expect("forward"), 8);
    assert_eq!(w.seek(SeekFrom::Current(-6)).expect("backward"), 2);
    let mut buf = [0u8; 2];
    w.read_exact(&mut buf).expect("read");
    assert_eq!(buf, [12, 13]);
}

#[test]
fn seeking_past_the_end_is_allowed_and_reads_there_return_zero() {
    let mut w = window(ramp(100), 10, 20);
    assert_eq!(
        w.seek(SeekFrom::Start(500))
            .expect("File allows this, so do we"),
        500
    );
    let mut buf = [0u8; 8];
    assert_eq!(w.read(&mut buf).expect("read past end"), 0);
}

#[test]
fn seeking_to_a_negative_position_is_invalid_input() {
    let mut w = window(ramp(100), 10, 20);
    for (label, target) in [
        ("Current below zero", SeekFrom::Current(-1)),
        ("End before the window start", SeekFrom::End(-21)),
    ] {
        let err = w.seek(target).expect_err(label);
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidInput,
            "{label} must match how `File` reports it"
        );
    }
}

#[test]
fn a_zero_offset_window_is_equivalent_to_the_source_itself() {
    // The property that lets `payload_offset: 0` be the universal default:
    // wrapping must be behaviourally invisible at offset zero.
    let bytes = ramp(64);
    let mut w = window(bytes.clone(), 0, bytes.len() as u64);
    let mut got = Vec::new();
    w.read_to_end(&mut got).expect("drain");
    assert_eq!(got, bytes);
    assert_eq!(w.seek(SeekFrom::End(0)).expect("end"), bytes.len() as u64);
}

#[test]
fn interleaved_seeks_and_reads_do_not_let_the_cursors_drift() {
    // The inner cursor is re-synced lazily, so the risk is a read that
    // trusts a stale position. Alternate the two often enough to catch it.
    let mut w = window(ramp(200), 50, 100);
    for offset in [0u64, 40, 7, 99, 12, 63] {
        w.seek(SeekFrom::Start(offset)).expect("seek");
        let mut one = [0u8; 1];
        assert_eq!(w.read(&mut one).expect("read one"), 1);
        assert_eq!(
            one[0],
            ((50 + offset) % 251) as u8,
            "byte at window offset {offset} must come from inner offset {}",
            50 + offset
        );
    }
}

#[test]
fn an_empty_window_reads_nothing_without_touching_the_source() {
    let mut w = window(ramp(100), 10, 0);
    let mut buf = [0u8; 4];
    assert_eq!(w.read(&mut buf).expect("read an empty window"), 0);
    assert_eq!(w.seek(SeekFrom::End(0)).expect("end of nothing"), 0);
}

#[test]
fn from_file_sizes_the_window_from_the_descriptor() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("payload.bin");
    std::fs::write(&path, ramp(500)).expect("write fixture");

    let file = std::fs::File::open(&path).expect("open");
    let w = PayloadWindow::from_file(file, 120).expect("window from offset 120");
    assert_eq!(
        w.len(),
        380,
        "the window runs from the offset to the end of the file"
    );
}

#[test]
fn from_file_refuses_an_offset_past_the_end() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("short.bin");
    std::fs::write(&path, ramp(10)).expect("write fixture");

    let file = std::fs::File::open(&path).expect("open");
    let err = PayloadWindow::from_file(file, 64)
        .expect_err("an offset past the end names a payload that cannot exist");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}
