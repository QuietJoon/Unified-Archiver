//! `VolumeChain` behaves as the concatenation of its parts.
//!
//! These are byte-level and use no archive format at all, which is the point:
//! the whole claim behind 7z multi-volume support is that a split set is
//! nothing but a byte split, so the adapter that reassembles it must be
//! correct as a pure `Read + Seek` question, independent of 7z.

use std::io::{Read, Seek, SeekFrom, Write};

use super::*;

/// Write `parts` as files in a fresh directory and chain them.
fn chain_of(parts: &[&[u8]]) -> (tempfile::TempDir, VolumeChain) {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut paths = Vec::new();
    for (i, bytes) in parts.iter().enumerate() {
        let path = dir.path().join(format!("part.{:03}", i + 1));
        let mut f = std::fs::File::create(&path).expect("create part");
        f.write_all(bytes).expect("write part");
        paths.push(path);
    }
    let chain = VolumeChain::open(&paths).expect("open chain");
    (dir, chain)
}

#[test]
fn reads_the_concatenation_across_part_boundaries() {
    let (_dir, mut chain) = chain_of(&[b"hello ", b"volume ", b"chain"]);
    assert_eq!(chain.total_len(), 18);
    assert_eq!(chain.volume_count(), 3);

    let mut out = Vec::new();
    chain.read_to_end(&mut out).expect("read all");
    assert_eq!(
        out, b"hello volume chain",
        "the chain must present exactly what the parts concatenate to"
    );
}

/// A read that lands mid-part must not silently splice the next part in;
/// `Read` is allowed to return short, and the caller's next call continues.
#[test]
fn a_read_stops_at_a_part_boundary_and_resumes() {
    let (_dir, mut chain) = chain_of(&[b"AAAA", b"BBBB"]);
    let mut buf = [0u8; 8];

    let first = chain.read(&mut buf).expect("first read");
    assert_eq!(first, 4, "the first read stops at the end of part 1");
    assert_eq!(&buf[..4], b"AAAA");

    let second = chain.read(&mut buf).expect("second read");
    assert_eq!(second, 4);
    assert_eq!(&buf[..4], b"BBBB");

    assert_eq!(chain.read(&mut buf).expect("at end"), 0);
}

#[test]
fn seeks_address_the_concatenation_not_the_parts() {
    let (_dir, mut chain) = chain_of(&[b"0123", b"4567", b"89"]);

    // A position inside part 2, reached from the start.
    assert_eq!(chain.seek(SeekFrom::Start(5)).expect("seek"), 5);
    let mut two = [0u8; 2];
    chain.read_exact(&mut two).expect("read across");
    assert_eq!(&two, b"56");

    // End-relative, which is what an archive parser does to find a footer.
    assert_eq!(chain.seek(SeekFrom::End(-2)).expect("seek end"), 8);
    let mut rest = Vec::new();
    chain.read_to_end(&mut rest).expect("tail");
    assert_eq!(rest, b"89");

    // Current-relative, backwards, across a boundary.
    assert_eq!(chain.seek(SeekFrom::Start(6)).expect("seek"), 6);
    assert_eq!(chain.seek(SeekFrom::Current(-3)).expect("back"), 3);
    let mut one = [0u8; 1];
    chain.read_exact(&mut one).expect("read");
    assert_eq!(&one, b"3");
}

/// Past the end reads empty rather than erroring — the behaviour `File` has,
/// and the one `PayloadWindow` is written against.
#[test]
fn seeking_past_the_end_reads_empty() {
    let (_dir, mut chain) = chain_of(&[b"abc"]);
    assert_eq!(chain.seek(SeekFrom::Start(99)).expect("seek past"), 99);
    let mut buf = [0u8; 4];
    assert_eq!(chain.read(&mut buf).expect("read past end"), 0);
}

#[test]
fn a_negative_seek_is_refused() {
    let (_dir, mut chain) = chain_of(&[b"abc"]);
    let err = chain.seek(SeekFrom::End(-99)).expect_err("negative seek");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

/// A chain of one is the ordinary unsplit archive, and must be
/// indistinguishable from reading the file directly. The 7z backend builds
/// every reader through this type, so if the single case were special the
/// split support would have changed unsplit behaviour.
#[test]
fn a_chain_of_one_behaves_like_the_file() {
    let (_dir, mut chain) = chain_of(&[b"single volume"]);
    assert_eq!(chain.volume_count(), 1);
    assert_eq!(chain.total_len(), 13);
    let mut out = String::new();
    chain.read_to_string(&mut out).expect("read");
    assert_eq!(out, "single volume");
}

/// An empty volume occupies no byte range, so it must be transparent rather
/// than a stopping point. `7zz -v` will not produce one, but a truncated or
/// hand-assembled set can.
#[test]
fn a_zero_length_volume_is_skipped() {
    let (_dir, mut chain) = chain_of(&[b"aa", b"", b"bb"]);
    assert_eq!(chain.total_len(), 4);
    let mut out = Vec::new();
    chain.read_to_end(&mut out).expect("read");
    assert_eq!(out, b"aabb");
}

#[test]
fn an_empty_path_list_is_refused() {
    let err = VolumeChain::open(&[]).expect_err("no volumes");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

/// The chain reports where it is without moving, which parsers rely on.
#[test]
fn stream_position_tracks_reads() {
    let (_dir, mut chain) = chain_of(&[b"1234", b"5678"]);
    let mut buf = [0u8; 6];
    chain
        .read_exact(&mut buf)
        .expect("read 6 across the boundary");
    assert_eq!(chain.stream_position().expect("pos"), 6);
}
