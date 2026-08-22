use super::*;
use std::io::Cursor;

/// R0079-0015: the narrow `unsafe impl Send` on the FFI-handle owner
/// must keep compiling — `Archive`'s `Send` is derived from it.
#[test]
fn test_unrar_archive_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<UnrarArchive>();
}

#[test]
fn test_dos_time_conversion() {
    // Test a known DOS timestamp: 2024-01-15 14:30:00
    let dos_time = (2024 - 1980) << 25 | 1 << 21 | 15 << 16 | 14 << 11 | 30 << 5;
    let sys_time = dos_time_to_system_time(dos_time);
    assert!(sys_time.is_some());
}

/// R0076-0069: FILETIME conversion must keep the 100-ns sub-second
/// ticks instead of truncating to whole seconds.
#[test]
fn filetime_conversion_preserves_subsecond_ticks() {
    // Unix epoch + 1.5 s expressed as 100-ns FILETIME ticks.
    let ticks = (FILETIME_UNIX_EPOCH_DIFF_SECS + 1) * 10_000_000 + 5_000_000;
    let t = filetime_to_system_time(ticks as u32, (ticks >> 32) as u32)
        .expect("post-epoch timestamp converts");
    assert_eq!(
        t.duration_since(UNIX_EPOCH).unwrap(),
        std::time::Duration::new(1, 500_000_000)
    );
}

/// R0076-0069: the existing contract is unchanged — `None` for the
/// all-zero "missing" encoding and for pre-Unix-epoch values, even
/// when sub-second ticks are present.
#[test]
fn filetime_conversion_none_for_zero_and_pre_epoch() {
    assert!(filetime_to_system_time(0, 0).is_none());
    // One tick past 1601-01-01 is far before the Unix epoch.
    assert!(filetime_to_system_time(1, 0).is_none());
    // Just below the epoch boundary, with a non-zero sub-second part.
    let ticks = FILETIME_UNIX_EPOCH_DIFF_SECS * 10_000_000 - 1;
    assert!(filetime_to_system_time(ticks as u32, (ticks >> 32) as u32).is_none());
}

/// R0001-0019: the bulk drift guard's three refusals are all typed
/// `Format` errors tagged as RAR, so a swapped, grown, or truncated
/// archive fails closed with a diagnosable message instead of
/// extracting attacker-chosen entries.
#[test]
fn listing_drift_errors_are_rar_format_errors() {
    for err in [
        listing_drift_mismatch(3, "docs/a.txt", "docs/evil.txt"),
        listing_drift_extra(7, 5),
        listing_drift_eof(2, 5),
    ] {
        assert!(
            matches!(
                err,
                ArchiveError::Format {
                    format: Some(ArchiveFormat::Rar),
                    ..
                }
            ),
            "expected a RAR format error, got {:?}",
            err
        );
    }
}

/// R0001-0019: each drift refusal names the offending index and the
/// two sides being compared, so the operator can tell a swap from a
/// grow from a truncation.
#[test]
fn listing_drift_messages_identify_the_mismatch() {
    let swapped = listing_drift_mismatch(3, "docs/a.txt", "docs/evil.txt").to_string();
    assert!(swapped.contains("index 3"), "{}", swapped);
    assert!(swapped.contains("docs/a.txt"), "{}", swapped);
    assert!(swapped.contains("docs/evil.txt"), "{}", swapped);

    let grew = listing_drift_extra(7, 5).to_string();
    assert!(grew.contains('7') && grew.contains('5'), "{}", grew);

    let truncated = listing_drift_eof(2, 5).to_string();
    assert!(
        truncated.contains('2') && truncated.contains('5'),
        "{}",
        truncated
    );
}

/// R0076-0069: whole-second timestamps still convert exactly.
#[test]
fn filetime_conversion_whole_seconds() {
    let ticks = (FILETIME_UNIX_EPOCH_DIFF_SECS + 42) * 10_000_000;
    let t = filetime_to_system_time(ticks as u32, (ticks >> 32) as u32)
        .expect("post-epoch timestamp converts");
    assert_eq!(
        t.duration_since(UNIX_EPOCH).unwrap(),
        std::time::Duration::from_secs(42)
    );
}

/// Build a zeroed `RARHeaderDataEx` carrying an ASCII name, so the
/// name decode in `parse_header` has real input. Fields are written
/// by value only — the struct is `#[repr(C, packed)]`.
fn header_with_name(name: &str) -> RARHeaderDataEx {
    let mut header = RARHeaderDataEx::default();
    let mut wide = [0 as RarWchar; 1024];
    for (slot, ch) in wide.iter_mut().zip(name.chars()) {
        *slot = ch as RarWchar;
    }
    header.file_name_w = wide;
    header
}

/// ticgit b75cafb4: RAR5 stores `st_mode` unshifted in the file
/// header's Attributes vint, so the decode must NOT apply the RAR4
/// `>> 16`. `33188` is the exact value hand-decoded from
/// `tests/fixtures/test.rar` (vint `a4 83 02`).
#[test]
fn rar5_unix_mode_is_read_unshifted() {
    assert_eq!(unix_mode_from_file_attr(33188, 50), Some(0o644));
    // VER_PACK7 sentinel — a RAR7 body inside the RAR5 container.
    assert_eq!(unix_mode_from_file_attr(0o100755, 70), Some(0o755));
    // Directory: S_IFDIR instead of S_IFREG, same unshifted layout.
    assert_eq!(unix_mode_from_file_attr(0o40755, 50), Some(0o755));
    // VER_UNKNOWN (9999) is still a RAR5 header per headers.hpp.
    assert_eq!(unix_mode_from_file_attr(0o120777, 9999), Some(0o777));
}

/// The branch no fixture in this repo can reach — every committed
/// `.rar` starts with the RAR5 signature, and producing a RAR4
/// archive needs the proprietary `rar` binary. RAR4 packs `st_mode`
/// into the upper 16 bits (`ReadHeader15`: `hd->FileAttr=Raw.Get4()`),
/// and its `unp_ver` is the raw archived byte, which real writers
/// emit as 10/13/15/20/26/29/36.
#[test]
fn rar4_unix_mode_is_read_shifted() {
    for unp_ver in [10u32, 13, 15, 20, 26, 29, 36] {
        assert_eq!(
            unix_mode_from_file_attr(0o100644 << 16, unp_ver),
            Some(0o644),
            "unp_ver {unp_ver} must take the RAR4 shifted decode"
        );
    }
    assert_eq!(unix_mode_from_file_attr(0o40755 << 16, 29), Some(0o755));
}

/// ticgit b75cafb4's core complaint: `Some(0)` is a positive claim
/// that nobody may read the file, which is strictly worse than
/// admitting the mode is unknown. A field with no `S_IFMT` bits
/// holds no Unix mode, so it reports `None`.
///
/// This also covers RAR5's `HSYS_UNKNOWN` host: `dll.cpp` folds it
/// into the Unix arm (`HSType==HSYS_WINDOWS ? HOST_WIN32 :
/// HOST_UNIX`), so without the guard those entries would receive a
/// fabricated mode.
#[test]
fn no_file_type_bits_reports_none_not_mode_zero() {
    // A RAR5 attribute misread with the RAR4 shift degrades to
    // `None`, never `Some(0)` — this is the exact regression.
    assert_eq!(unix_mode_from_file_attr(33188, 29), None);
    assert_eq!(unix_mode_from_file_attr(0, 50), None);
    assert_eq!(unix_mode_from_file_attr(0, 29), None);
    // A DOS attribute word (FILE_ATTRIBUTE_ARCHIVE) carries no type
    // bits under either unpacking.
    assert_eq!(unix_mode_from_file_attr(0x20, 50), None);
    assert_eq!(unix_mode_from_file_attr(0x20, 29), None);
}

/// End-to-end through `parse_header`: the format-aware unpack is
/// actually wired in, and the Windows-host branch is unchanged.
#[test]
fn parse_header_unpacks_permissions_by_format() {
    // (a) RAR5 Unix host — the `tests/fixtures/test.rar` shape.
    let mut rar5 = header_with_name("test_file.txt");
    rar5.host_os = 3; // HOST_UNIX
    rar5.unp_ver = 50; // VER_PACK5
    rar5.file_attr = 33188; // 0o100644, unshifted
    let entry = parse_header(&rar5).expect("RAR5 Unix header parses");
    assert_eq!(entry.permissions, Some(0o644));
    assert_eq!(
        entry.attributes.as_ref().and_then(|a| a.windows),
        None,
        "a Unix host must not populate the Windows attribute field"
    );

    // (b) RAR4 Unix host — the shifted packing.
    let mut rar4 = header_with_name("test_file.txt");
    rar4.host_os = 3;
    rar4.unp_ver = 29; // VER_PACK, RAR 2.9/3.x
    rar4.file_attr = 0o100644 << 16;
    let entry = parse_header(&rar4).expect("RAR4 Unix header parses");
    assert_eq!(entry.permissions, Some(0o644));

    // (c) Windows host — unchanged: no permissions, attributes kept.
    let mut win = header_with_name("test_file.txt");
    win.host_os = 2; // HOST_WIN32
    win.unp_ver = 50;
    win.file_attr = 0x20; // FILE_ATTRIBUTE_ARCHIVE
    let entry = parse_header(&win).expect("Windows header parses");
    assert_eq!(entry.permissions, None);
    assert_eq!(
        entry.attributes.as_ref().and_then(|a| a.windows),
        Some(0x20)
    );
}

#[test]
fn rar5_vint_one_byte() {
    // Single-byte vint: high bit clear, payload is the lower 7 bits.
    let mut cursor = Cursor::new(vec![0x05u8]);
    assert_eq!(read_rar5_vint(&mut cursor).unwrap(), 5);
    assert_eq!(cursor.position(), 1);
}

#[test]
fn rar5_vint_zero() {
    let mut cursor = Cursor::new(vec![0x00u8]);
    assert_eq!(read_rar5_vint(&mut cursor).unwrap(), 0);
}

#[test]
fn rar5_vint_two_bytes() {
    // 0x81 = 10000001 → continuation set, payload 0x01.
    // 0x01 = 00000001 → high bit clear, payload 0x01.
    // Decoded: 1 | (1 << 7) = 0x81.
    let mut cursor = Cursor::new(vec![0x81, 0x01]);
    assert_eq!(read_rar5_vint(&mut cursor).unwrap(), 0x81);
    assert_eq!(cursor.position(), 2);
}

#[test]
fn rar5_vint_max_10_bytes() {
    // Ten bytes where only the last clears the high bit: a valid
    // (boundary-case) encoding. The plan calls this out as the
    // upper limit on byte count.
    let mut bytes = vec![0xFFu8; 9];
    bytes.push(0x7F);
    let mut cursor = Cursor::new(bytes);
    let v = read_rar5_vint(&mut cursor).expect("10-byte vint should decode");
    // No specific value assertion — we care that it accepts.
    // The decoded value fills 64 bits so must be >= 1.
    assert!(v != 0);
}

#[test]
fn rar5_vint_reject_more_than_10_bytes() {
    // 10 continuation bytes followed by a terminator → exceeds cap.
    let mut bytes = vec![0xFFu8; 10];
    bytes.push(0x01);
    let mut cursor = Cursor::new(bytes);
    assert!(read_rar5_vint(&mut cursor).is_err());
}

#[test]
fn rar5_vint_reject_eof_mid_continuation() {
    // High bit set on the only byte → continuation expected but
    // stream is exhausted.
    let mut cursor = Cursor::new(vec![0x80u8]);
    assert!(read_rar5_vint(&mut cursor).is_err());
}

/// R0079-0001: the listing must be repeat-safe — a second walk on
/// the same handle used to return zero entries because the UnRAR
/// handle was EOF-positioned after the first.
#[test]
fn rar_list_files_repeat_safe() {
    let path = crate::test_utils::fixture("test.rar");
    let archive = UnrarArchive::open(&path).expect("open RAR fixture");

    let first = archive.list_files().expect("first listing");
    let second = archive.list_files().expect("second listing");

    assert!(!first.is_empty(), "first walk must see entries");
    assert!(!second.is_empty(), "second walk must see entries");
    assert_eq!(first.len(), second.len(), "listings must match");
}

/// R0079-0001: even after `self.handle` has been exhausted by a
/// direct header walk (the `is_encrypted()` / `list_files_for_limits`
/// call pattern), `list_files` must still return the full listing.
#[test]
fn rar_list_files_full_after_handle_exhausted() {
    let path = crate::test_utils::fixture("test.rar");
    let archive = UnrarArchive::open(&path).expect("open RAR fixture");

    while archive.read_header().expect("walk header").is_some() {
        archive.skip_entry().expect("skip entry");
    }

    let listing = archive.list_files().expect("listing after exhaustion");
    assert!(
        !listing.is_empty(),
        "exhausted primary handle must not produce an empty listing"
    );
}

/// Assemble a RAR4 block with a valid HEAD_CRC. `body` is everything
/// after the 7-byte basic header (for a LONG_BLOCK that includes the
/// 4-byte ADD_SIZE field); `data` is the optional data area that follows
/// the header and is *not* covered by HEAD_CRC. HEAD_CRC is the low 16
/// bits of the standard CRC32 over HEAD_TYPE..end-of-header, matching the
/// vendored UnRAR's `GetCRC15` (R0081-0070).
fn rar4_block(head_type: u8, head_flags: u16, body: &[u8], data: &[u8]) -> Vec<u8> {
    let head_size = 7u16 + body.len() as u16;
    let mut after_crc = Vec::new();
    after_crc.push(head_type);
    after_crc.extend_from_slice(&head_flags.to_le_bytes());
    after_crc.extend_from_slice(&head_size.to_le_bytes());
    after_crc.extend_from_slice(body);
    let head_crc = (crc32fast::hash(&after_crc) & 0xFFFF) as u16;

    let mut block = head_crc.to_le_bytes().to_vec();
    block.extend_from_slice(&after_crc);
    block.extend_from_slice(data);
    block
}

/// Synthetic RAR4 block sequence: signature, main header, one
/// LONG_BLOCK (0x8000) file header with packed data, then a
/// recovery record block (type 0x78) declaring 5/100 blocks.
fn synthetic_rar4_with_recovery() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature

    // Main archive header (type 0x73), head_size 13.
    buf.extend_from_slice(&rar4_block(0x73, 0, &[0u8; 6], &[]));

    // File header (type 0x74) with LONG_BLOCK set: head_size 32,
    // ADD_SIZE 5 → 21 remaining header bytes + 5 data bytes.
    let mut file_body = Vec::new();
    file_body.extend_from_slice(&5u32.to_le_bytes()); // ADD_SIZE
    file_body.extend_from_slice(&[0u8; 21]); // rest of file header
    buf.extend_from_slice(&rar4_block(0x74, 0x8000, &file_body, &[0xAB; 5]));

    // Recovery record (type 0x78): head_size 15 → 8 data bytes read
    // as total_blocks / recovery_blocks.
    let mut rec_body = Vec::new();
    rec_body.extend_from_slice(&100u32.to_le_bytes()); // total blocks
    rec_body.extend_from_slice(&5u32.to_le_bytes()); // recovery blocks
    buf.extend_from_slice(&rar4_block(0x78, 0, &rec_body, &[]));

    buf
}

/// Assemble a minimal RAR5 block: the 4-byte HEAD_CRC over the
/// HeaderSize vint plus `body`, then those bytes. `body` begins at
/// HeaderType. HEAD_CRC is the standard CRC32 of the HeaderSize-vint bytes
/// plus the body, matching the vendored UnRAR's `GetCRC50` (R0081-0071).
/// Only single-byte HeaderSize vints (body up to 127 bytes) are encoded,
/// which is all these tests require.
fn rar5_block(body: &[u8]) -> Vec<u8> {
    assert!(body.len() < 0x80, "test helper only encodes 1-byte vints");
    let size_vint = [body.len() as u8]; // HeaderSize as a 1-byte vint
    let mut crc_region = Vec::new();
    crc_region.extend_from_slice(&size_vint);
    crc_region.extend_from_slice(body);
    let crc = crc32fast::hash(&crc_region);

    let mut block = crc.to_le_bytes().to_vec();
    block.extend_from_slice(&crc_region);
    block
}

/// R0079-0025: HEAD_FLAGS 0x8000 is LONG_BLOCK, not a terminator —
/// the scan must walk past file headers to the trailing recovery
/// record instead of returning `None` at the first file block.
#[test]
fn rar4_recovery_scan_passes_long_block_file_headers() {
    let mut cursor = Cursor::new(synthetic_rar4_with_recovery());
    let pct =
        parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor).expect("synthetic RAR4 walk");
    assert_eq!(pct, Some(5));
}

/// R0079-0025: the walk still terminates on the 0x7B end-of-archive
/// block (and at EOF) — a recovery block after the end marker is
/// not reached.
#[test]
fn rar4_recovery_scan_stops_at_end_of_archive_block() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature

    // End-of-archive block (type 0x7B), head_size 7.
    buf.extend_from_slice(&rar4_block(0x7B, 0, &[], &[]));

    // Recovery block after the end marker — must not be reached.
    let mut rec_body = Vec::new();
    rec_body.extend_from_slice(&100u32.to_le_bytes());
    rec_body.extend_from_slice(&5u32.to_le_bytes());
    buf.extend_from_slice(&rar4_block(0x78, 0, &rec_body, &[]));

    let mut cursor = Cursor::new(buf);
    let pct =
        parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor).expect("synthetic RAR4 walk");
    assert_eq!(pct, None);
}

/// R0079-0025: EOF without a recovery block resolves to `Ok(None)`.
#[test]
fn rar4_recovery_scan_none_at_eof() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature

    // Main archive header only, head_size 13.
    buf.extend_from_slice(&rar4_block(0x73, 0, &[0u8; 6], &[]));

    let mut cursor = Cursor::new(buf);
    let pct =
        parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor).expect("synthetic RAR4 walk");
    assert_eq!(pct, None);
}

#[test]
fn rar5_vint_known_values() {
    // Pin a handful of known encodings so future refactors can't
    // silently change the decoded value.
    let cases: &[(&[u8], u64)] = &[
        (&[0x00], 0),
        (&[0x05], 5),
        (&[0x7F], 0x7F),
        (&[0x80, 0x01], 0x80),
        (&[0x81, 0x01], 0x81),
        (&[0xFF, 0x7F], 0x3FFF),
        (&[0xFF, 0xFF, 0xFF, 0x01], 0x3F_FFFF),
    ];
    for (bytes, expected) in cases {
        let mut cursor = Cursor::new(bytes.to_vec());
        let v = read_rar5_vint(&mut cursor).expect("stream decode");
        assert_eq!(v, *expected, "encoding {:x?}", bytes);
    }
}

/// R0080-0052: a clean end of the RAR5 block chain (zero bytes at a
/// block boundary) resolves to `Ok(None)`, not an error.
#[test]
fn rar5_recovery_clean_eof_is_none() {
    let mut cursor = Cursor::new(b"Rar!\x1A\x07\x01\x00".to_vec());
    let pct = parse_rar5_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect("clean end of block chain is not an error");
    assert_eq!(pct, None);
}

/// R0080-0052: a CRC field truncated mid-read is corruption, never a
/// silent "no recovery record".
#[test]
fn rar5_recovery_truncated_crc_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x01\x00"); // RAR5 signature
    buf.extend_from_slice(&[0u8, 0]); // only 2 of the 4 CRC bytes
    let mut cursor = Cursor::new(buf);
    let err = parse_rar5_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("a truncated CRC field must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0080-0055: a header_size that makes the next-block offset overflow
/// u64 is rejected as corruption rather than wrapping to an attacker
/// chosen offset.
#[test]
fn rar5_recovery_next_block_overflow_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x01\x00"); // RAR5 signature
    buf.extend_from_slice(&[0u8; 4]); // block CRC32
    buf.extend_from_slice(&[0xFFu8; 9]); // header_size vint continuation bytes
    buf.push(0x01); // vint terminator -> decodes to u64::MAX
    buf.push(0x01); // header_type = 1 (neither service nor end)
    buf.push(0x00); // header_flags = 0 (no extra/data areas)
    let mut cursor = Cursor::new(buf);
    let err = parse_rar5_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("an overflowing next-block offset must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0080-0059: a partially-read RAR4 block header is corruption, not a
/// clean EOF that silently ends the scan.
#[test]
fn rar4_recovery_partial_block_header_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature (7 bytes)
    buf.extend_from_slice(&[0u8, 0, 0x73]); // only 3 of the 7 header bytes
    let mut cursor = Cursor::new(buf);
    let err = parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("a partial block header must be corruption, not clean EOF");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0080-0060: a RAR4 head_size below the 7-byte minimum is rejected as
/// corruption before any skip arithmetic.
#[test]
fn rar4_recovery_undersized_header_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature (7 bytes)
    buf.extend_from_slice(&[0u8, 0]); // HEAD_CRC
    buf.push(0x73); // HEAD_TYPE (main header)
    buf.extend_from_slice(&0u16.to_le_bytes()); // HEAD_FLAGS
    buf.extend_from_slice(&3u16.to_le_bytes()); // HEAD_SIZE = 3 (< 7 minimum)
    let mut cursor = Cursor::new(buf);
    let err = parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("head_size below 7 must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0081-0063: a second `unrar_lock()` on the same thread returns a typed
/// error instead of deadlocking on the non-reentrant mutex. A `try_lock`
/// would instead break legitimate cross-thread contention, so the guard
/// uses a thread-local sentinel.
#[test]
fn unrar_lock_rejects_same_thread_reentry() {
    let outer = unrar_lock().expect("first lock acquires");
    assert!(
        matches!(unrar_lock(), Err(ArchiveError::OperationBlocked { .. })),
        "a second same-thread unrar_lock must be rejected, not deadlock"
    );
    drop(outer);
    // The sentinel is cleared on drop, so a fresh acquisition succeeds.
    assert!(
        unrar_lock().is_ok(),
        "lock must re-acquire once the outer guard is dropped"
    );
}

/// R0081-0070: a RAR4 block whose HEAD_CRC does not match the header bytes
/// is rejected as corruption before any field is trusted.
#[test]
fn rar4_recovery_bad_header_crc_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature
    let mut block = rar4_block(0x73, 0, &[0u8; 6], &[]);
    block[0] ^= 0xFF; // corrupt the stored HEAD_CRC
    buf.extend_from_slice(&block);

    let mut cursor = Cursor::new(buf);
    let err = parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("a bad HEAD_CRC must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0081-0072: a LONG_BLOCK whose ADD_SIZE data area runs past the end of
/// the archive is rejected as corruption instead of seeking blindly past
/// EOF and reporting "no recovery record".
#[test]
fn rar4_recovery_add_size_past_eof_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x00"); // RAR4 signature
    // File header (LONG_BLOCK) declaring a 4 KiB data area not present.
    let mut file_body = Vec::new();
    file_body.extend_from_slice(&4096u32.to_le_bytes()); // ADD_SIZE
    file_body.extend_from_slice(&[0u8; 21]); // rest of file header
    buf.extend_from_slice(&rar4_block(0x74, 0x8000, &file_body, &[]));

    let mut cursor = Cursor::new(buf);
    let err = parse_rar4_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("an ADD_SIZE past EOF must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

/// R0081-0071: a RAR5 block with a valid HEAD_CRC is accepted, so the walk
/// proceeds to interpret the header (here an end-of-archive marker, which
/// resolves to `Ok(None)`).
#[test]
fn rar5_recovery_valid_header_crc_end_of_archive() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x01\x00"); // RAR5 signature
    buf.extend_from_slice(&rar5_block(&[0x05, 0x00])); // header_type 5 (EOA), flags 0

    let mut cursor = Cursor::new(buf);
    let pct = parse_rar5_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect("a valid header CRC must be accepted");
    assert_eq!(pct, None);
}

/// R0081-0071: a RAR5 block whose HEAD_CRC does not match the header is
/// rejected as corruption before the header is treated as authoritative.
#[test]
fn rar5_recovery_bad_header_crc_is_corruption() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Rar!\x1A\x07\x01\x00"); // RAR5 signature
    let mut block = rar5_block(&[0x05, 0x00]);
    block[0] ^= 0xFF; // corrupt the stored CRC
    buf.extend_from_slice(&block);

    let mut cursor = Cursor::new(buf);
    let err = parse_rar5_recovery(Path::new("synthetic.rar"), &mut cursor)
        .expect_err("a bad header CRC must be corruption");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
}

// ── UCM_CHANGEVOLUME handling (ticgit d3cfce) ──

/// Build a bare context for driving the trampoline directly. `progress` is
/// `None`, which is the only field the volume-change arms read nothing from —
/// they touch `abort` and nothing else.
fn volume_test_context() -> UnrarExtractContext<'static> {
    UnrarExtractContext::new("test", None, 0, None)
}

/// Call the trampoline the way the SDK does, with a context pointer.
fn call_trampoline(ctx: &mut UnrarExtractContext<'_>, msg: c_uint, p2: isize) -> c_int {
    let user_data = ctx as *mut UnrarExtractContext<'_> as isize;
    // SAFETY: `user_data` points at `ctx`, which outlives the call, and the
    // trampoline only touches it on this thread — the same single-threaded
    // discipline AD 0019 gives it in production.
    unsafe { unrar_process_callback(msg, user_data, 0, p2) }
}

/// The defect this pins: answering `UCM_CHANGEVOLUME*`/`RAR_VOL_ASK` with a
/// non-abort code makes the SDK retry the same volume name indefinitely.
///
/// `volume.cpp`'s `DllVolChange` quits only on `DllVolAborted`, or when no
/// callback is registered at all — and its own comment says returning an
/// unchanged name is a legitimate way to say "waiting for a volume that does
/// not exist yet". We register a callback and cannot rewrite the buffer, so
/// `-1` is the only answer that terminates. The spin would hold AD 0019's
/// process-wide lock, stalling every RAR operation in the process.
#[test]
fn unrar_callback_aborts_when_the_next_volume_is_missing() {
    for msg in [UCM_CHANGEVOLUME, UCM_CHANGEVOLUMEW] {
        let mut ctx = volume_test_context();
        let rc = call_trampoline(&mut ctx, msg, RAR_VOL_ASK);
        assert_eq!(rc, -1, "msg {msg} with RAR_VOL_ASK must abort, not retry");
        assert!(
            matches!(ctx.abort, Some(UnrarAbort::MissingVolume)),
            "msg {msg} must record why it aborted, got {:?}",
            ctx.abort
        );
    }
}

/// The other half, and the reason the fix cannot simply return `-1` for every
/// volume-change message: `RAR_VOL_NOTIFY` reports that the next volume *was*
/// opened. `DllVolNotify` treats `-1` there as a refusal and gives up, so an
/// unconditional abort would break every valid multi-volume archive.
#[test]
fn unrar_callback_lets_an_opened_volume_through() {
    for msg in [UCM_CHANGEVOLUME, UCM_CHANGEVOLUMEW] {
        let mut ctx = volume_test_context();
        let rc = call_trampoline(&mut ctx, msg, RAR_VOL_NOTIFY);
        assert!(
            rc >= 0,
            "msg {msg} with RAR_VOL_NOTIFY must not abort a volume that opened, got {rc}"
        );
        assert!(
            ctx.abort.is_none(),
            "a successful volume transition must record no abort, got {:?}",
            ctx.abort
        );
    }
}

/// A missing volume aborts even when there is no context to explain it in.
/// A stalled process-wide lock is worse than an unlabelled error, so the
/// `user_data == 0` path must not fall through to the non-abort default.
#[test]
fn unrar_callback_aborts_a_missing_volume_without_a_context() {
    // SAFETY: `user_data` is 0, so the trampoline never dereferences it.
    let rc = unsafe { unrar_process_callback(UCM_CHANGEVOLUMEW, 0, 0, RAR_VOL_ASK) };
    assert_eq!(rc, -1);
}

/// Messages that genuinely do want UnRAR's default behaviour still get it —
/// the volume arms must not have widened into a blanket abort.
#[test]
fn unrar_callback_keeps_the_default_for_other_messages() {
    for msg in [UCM_NEEDPASSWORD, UCM_NEEDPASSWORDW, UCM_LARGEDICT] {
        let mut ctx = volume_test_context();
        let rc = call_trampoline(&mut ctx, msg, 0);
        assert_eq!(rc, 1, "msg {msg} must keep the non-abort default");
        assert!(ctx.abort.is_none());
    }
}

/// The abort must surface as a typed error a caller can act on, and the
/// action is named: `VolumeSetReport::defects()` answers "which volume", which the
/// callback itself cannot do portably (the `W` message that arrives first
/// carries a platform-width `wchar` buffer).
#[test]
fn missing_volume_maps_to_corruption_pointing_at_the_defect_list() {
    let mut ctx = volume_test_context();
    ctx.abort = Some(UnrarAbort::MissingVolume);
    let err = ctx
        .take_abort_error(Path::new("/tmp/set.part1.rar"))
        .expect("a recorded abort must produce an error");
    match err {
        ArchiveError::Corruption { path, details } => {
            assert!(path.contains("set.part1.rar"), "path was {path}");
            assert!(
                details.contains("not available"),
                "details must say the volume is missing: {details}"
            );
            assert!(
                details.contains("defects()"),
                "details must point the caller at the typed parser: {details}"
            );
            assert!(
                details.contains("VolumeSetReport"),
                "details must name the type that owns defects(): {details}"
            );
        }
        other => panic!("expected Corruption, got {other:?}"),
    }
    assert!(
        ctx.abort.is_none(),
        "take_abort_error must consume the abort"
    );
}

/// The advice in that error message has to be a call that compiles, not a
/// substring that looks right.
///
/// The message previously said `VolumeSet::defects()`. `defects()` is on
/// `VolumeSetReport`; `VolumeSet` owns `paths`, `to_layout` and `expected_name`.
/// A caller who followed the advice got a compile error, and the assertion above
/// did not notice because the substring `defects()` was present either way. So
/// this test *performs* the recommended sequence. If the type or the method
/// moves again, this stops compiling instead of passing.
#[test]
fn the_recommended_recovery_call_actually_compiles() {
    use crate::format::multipart::parse_volume_set;

    let paths = [
        PathBuf::from("set.part1.rar"),
        PathBuf::from("set.part3.rar"),
    ];
    let report = parse_volume_set(&paths);
    // The exact chain the error message tells the caller to use.
    let defects = report.defects();
    assert!(
        !defects.is_empty(),
        "a set missing volume 2 must report a defect, got {defects:?}"
    );
    assert!(
        !report.is_complete(),
        "a set missing volume 2 must not be complete"
    );
}
