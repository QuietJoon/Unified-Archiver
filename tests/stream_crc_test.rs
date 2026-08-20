//! Tests for stream-level CRC32 extraction
//!
//! This tests extraction of CRC32 checksums stored in the compression format's
//! metadata (GZIP trailer, BZIP2 EOS marker, XZ stream flags).
//!
//! The compressed inputs are embedded as const byte arrays and written
//! into a fresh tempdir per test (R0079-0032). Earlier revisions read
//! manually staged files from a hardcoded path and silently skipped
//! when they were absent, so the public `stream_crc` API had no
//! coverage on a fresh checkout.

use std::path::PathBuf;
use unified_archive::stream_crc::{
    CheckType, extract_bzip2_stream_crc, extract_gzip_stream_crc, extract_stream_checksum,
    extract_xz_stream_check,
};

/// Uncompressed payload shared by every embedded stream below.
const PAYLOAD: &[u8] = b"unified-archive stream CRC32 test payload\n";

/// `gzip -n -9` over `PAYLOAD`. Trailer carries the IEEE CRC32 and
/// ISIZE (42) of the payload.
const PAYLOAD_GZ: &[u8] = &[
    0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x2B, 0xCD, 0xCB, 0x4C, 0xCB, 0x4C,
    0x4D, 0xD1, 0x4D, 0x2C, 0x4A, 0xCE, 0xC8, 0x2C, 0x4B, 0x55, 0x28, 0x2E, 0x29, 0x4A, 0x4D, 0xCC,
    0x55, 0x70, 0x0E, 0x72, 0x36, 0x36, 0x52, 0x28, 0x49, 0x2D, 0x2E, 0x51, 0x28, 0x48, 0xAC, 0xCC,
    0xC9, 0x4F, 0x4C, 0xE1, 0x02, 0x00, 0x54, 0x12, 0x26, 0x6F, 0x2A, 0x00, 0x00, 0x00,
];

/// `bzip2` over `PAYLOAD` (single block, so stream CRC == block CRC).
const PAYLOAD_BZ2: &[u8] = &[
    0x42, 0x5A, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x47, 0x52, 0x9F, 0x2E, 0x00, 0x00,
    0x13, 0xDF, 0x80, 0x00, 0x10, 0x40, 0x02, 0x18, 0x00, 0x08, 0x00, 0x10, 0x00, 0x2F, 0x67, 0xDF,
    0x20, 0x20, 0x00, 0x22, 0x9A, 0x68, 0x19, 0x00, 0x03, 0x42, 0x80, 0x01, 0xA0, 0x64, 0xC8, 0x2B,
    0x72, 0x70, 0x6C, 0x48, 0xC9, 0x99, 0x84, 0xB5, 0xBA, 0xB7, 0xC3, 0xAA, 0x86, 0x33, 0xCD, 0xA1,
    0xA7, 0xB0, 0x27, 0x41, 0x40, 0x04, 0xA3, 0xFE, 0x2E, 0xE4, 0x8A, 0x70, 0xA1, 0x20, 0x8E, 0xA5,
    0x3E, 0x5C,
];

/// Stream CRC of `PAYLOAD_BZ2`, cross-checked against
/// `bzip2 -tvvvv` ("combined CRCs: stored = 0x47529f2e"). bzip2's
/// stream CRC is CRC-32/BZIP2, not the IEEE CRC32 of the payload.
const PAYLOAD_BZ2_STREAM_CRC: u32 = 0x4752_9F2E;

/// `xz` over `PAYLOAD` with the default check type (CRC64).
const PAYLOAD_XZ_CRC64: &[u8] = &[
    0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x04, 0xE6, 0xD6, 0xB4, 0x46, 0x04, 0xC0, 0x2E, 0x2A,
    0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x67, 0x35, 0x82, 0x5B,
    0x01, 0x00, 0x29, 0x75, 0x6E, 0x69, 0x66, 0x69, 0x65, 0x64, 0x2D, 0x61, 0x72, 0x63, 0x68, 0x69,
    0x76, 0x65, 0x20, 0x73, 0x74, 0x72, 0x65, 0x61, 0x6D, 0x20, 0x43, 0x52, 0x43, 0x33, 0x32, 0x20,
    0x74, 0x65, 0x73, 0x74, 0x20, 0x70, 0x61, 0x79, 0x6C, 0x6F, 0x61, 0x64, 0x0A, 0x00, 0x00, 0x00,
    0x0D, 0x59, 0xA9, 0xD0, 0x0D, 0xEA, 0x3F, 0x3D, 0x00, 0x01, 0x4A, 0x2A, 0x72, 0xDB, 0xAB, 0xF1,
    0x1F, 0xB6, 0xF3, 0x7D, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59, 0x5A,
];

/// `xz --check=crc32` over `PAYLOAD`.
const PAYLOAD_XZ_CRC32: &[u8] = &[
    0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x01, 0x69, 0x22, 0xDE, 0x36, 0x04, 0xC0, 0x2E, 0x2A,
    0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x67, 0x35, 0x82, 0x5B,
    0x01, 0x00, 0x29, 0x75, 0x6E, 0x69, 0x66, 0x69, 0x65, 0x64, 0x2D, 0x61, 0x72, 0x63, 0x68, 0x69,
    0x76, 0x65, 0x20, 0x73, 0x74, 0x72, 0x65, 0x61, 0x6D, 0x20, 0x43, 0x52, 0x43, 0x33, 0x32, 0x20,
    0x74, 0x65, 0x73, 0x74, 0x20, 0x70, 0x61, 0x79, 0x6C, 0x6F, 0x61, 0x64, 0x0A, 0x00, 0x00, 0x00,
    0x54, 0x12, 0x26, 0x6F, 0x00, 0x01, 0x46, 0x2A, 0x7E, 0x94, 0x1E, 0x5D, 0x90, 0x42, 0x99, 0x0D,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x59, 0x5A,
];

/// IEEE CRC32 of `PAYLOAD`, as stored in the GZIP trailer.
fn payload_crc32() -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(PAYLOAD);
    hasher.finalize()
}

/// Write an embedded stream into a fresh tempdir. The returned
/// `TempDir` guard removes the directory on drop.
fn write_stream(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("write embedded stream");
    (dir, path)
}

#[test]
fn test_gzip_stream_crc() {
    let (_dir, path) = write_stream("payload.txt.gz", PAYLOAD_GZ);

    let checksum = extract_gzip_stream_crc(&path).expect("Should extract GZIP stream CRC");

    assert_eq!(checksum.check_type, CheckType::Crc32);
    assert_eq!(
        checksum.crc32,
        Some(payload_crc32()),
        "GZIP trailer CRC32 should match the payload's IEEE CRC32"
    );
    assert_eq!(checksum.uncompressed_size, Some(PAYLOAD.len() as u64));
}

#[test]
fn test_bzip2_stream_crc() {
    let (_dir, path) = write_stream("payload.txt.bz2", PAYLOAD_BZ2);

    let checksum = extract_bzip2_stream_crc(&path).expect("Should extract BZIP2 stream CRC");

    assert_eq!(checksum.check_type, CheckType::Crc32);
    assert_eq!(checksum.crc32, Some(PAYLOAD_BZ2_STREAM_CRC));
    assert_eq!(
        checksum.uncompressed_size, None,
        "BZIP2 does not store uncompressed size"
    );
}

#[test]
fn test_xz_stream_check_default_crc64() {
    let (_dir, path) = write_stream("payload.txt.xz", PAYLOAD_XZ_CRC64);

    let checksum = extract_xz_stream_check(&path).expect("Should extract XZ stream check");

    assert_eq!(
        checksum.check_type,
        CheckType::Crc64,
        "xz default check type is CRC64"
    );
}

#[test]
fn test_xz_stream_check_crc32() {
    let (_dir, path) = write_stream("payload_crc32.txt.xz", PAYLOAD_XZ_CRC32);

    let checksum = extract_xz_stream_check(&path).expect("Should extract XZ stream check");

    assert_eq!(checksum.check_type, CheckType::Crc32);
}

#[test]
fn test_auto_detect_gzip() {
    // Deliberately misleading extension: detection is magic-byte driven.
    let (_dir, path) = write_stream("renamed_gzip.dat", PAYLOAD_GZ);

    let checksum = extract_stream_checksum(&path).expect("Should detect GZIP by magic");

    assert_eq!(checksum.check_type, CheckType::Crc32);
    assert_eq!(checksum.crc32, Some(payload_crc32()));
}

#[test]
fn test_auto_detect_bzip2() {
    let (_dir, path) = write_stream("renamed_bzip2.dat", PAYLOAD_BZ2);

    let checksum = extract_stream_checksum(&path).expect("Should detect BZIP2 by magic");

    assert_eq!(checksum.check_type, CheckType::Crc32);
    assert_eq!(checksum.crc32, Some(PAYLOAD_BZ2_STREAM_CRC));
}

#[test]
fn test_verify_crc32_values() {
    // Independent oracle: a freshly gzip'd file (not the embedded
    // bytes) must report the crc32fast checksum of its input.
    //
    // OI-0056-010: this used to probe with `which gzip` and `eprintln!`-skip
    // when the probe failed, which self-disabled twice over — once on a host
    // without `gzip`, and once on any host without `which` (the same
    // R0001-0086 defect `common::command_exists` was written to fix). The
    // external oracle is the whole point of the lane, so its absence is now
    // a hard failure: the `gzip` invocation below is unguarded and reports
    // the dependency in its own panic message.
    use std::process::Command;

    let dir = tempfile::tempdir().expect("create tempdir");
    let test_path = dir.path().join("test_known_crc.txt");
    let gz_path = dir.path().join("test_known_crc.txt.gz");

    let content = b"Test data for CRC32 verification";
    std::fs::write(&test_path, content).expect("write test file");

    let status = Command::new("gzip")
        .arg("-k")
        .arg(&test_path)
        .status()
        .expect(
            "this lane needs the `gzip` CLI as an independent CRC32 oracle (the embedded-bytes \
         lanes above cover the parser against hard-coded values; this one deliberately does \
         not compute the reference itself). Install gzip and re-run.",
        );
    assert!(status.success(), "gzip should compress the test file");

    let checksum = extract_gzip_stream_crc(&gz_path).expect("Should extract GZIP stream CRC");

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(content);
    let expected_crc = hasher.finalize();

    assert_eq!(
        checksum.crc32,
        Some(expected_crc),
        "Stream CRC32 should match computed CRC32"
    );
    assert_eq!(checksum.uncompressed_size, Some(content.len() as u64));
}
