use super::*;
use crate::test_utils::fixture;

#[test]
fn test_sevenz_open_valid() {
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path);
    assert!(archive.is_ok());
    assert_eq!(archive.unwrap().path(), path);
}

#[test]
fn test_sevenz_open_nonexistent() {
    // open() succeeds lazily; error surfaces on first use (list_files)
    let archive = SevenZArchive::open("/nonexistent/archive.7z").unwrap();
    assert!(archive.list_files().is_err());
}

#[test]
fn test_sevenz_list_files() {
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();
    let entries = archive.list_files();
    assert!(entries.is_ok());
    let entries = entries.unwrap();
    assert!(!entries.is_empty(), "7z should have at least one entry");

    for entry in entries.iter() {
        assert!(!entry.path.is_empty(), "Entry path should not be empty");
    }
}

#[test]
fn test_sevenz_list_files_have_crc32() {
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();

    for entry in entries.iter() {
        if entry.entry_type == EntryType::File {
            // 7z should provide CRC32 in metadata
            assert!(
                entry.crc32.is_some(),
                "File entry '{}' should have CRC32",
                entry.path
            );
        }
    }
}

#[test]
fn test_sevenz_extract_to_memory() {
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();
    let data = archive.extract_to_memory("test_file.txt");
    assert!(data.is_ok());
    let data = data.unwrap();
    assert!(!data.is_empty(), "Extracted data should not be empty");
}

#[test]
fn test_sevenz_extract_to_memory_nonexistent_entry() {
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();
    let result = archive.extract_to_memory("no_such_file.txt");
    assert!(result.is_err());
}

#[test]
fn test_sevenz_extract_to_stream() {
    use std::io::Read;
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();
    let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    assert!(!buf.is_empty());
}

#[test]
fn test_sevenz_memory_and_stream_produce_same_data() {
    use std::io::Read;
    let path = fixture("test.7z");
    let archive = SevenZArchive::open(&path).unwrap();

    let mem_data = archive.extract_to_memory("test_file.txt").unwrap();

    let archive2 = SevenZArchive::open(&path).unwrap();
    let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
    let mut stream_data = Vec::new();
    stream.read_to_end(&mut stream_data).unwrap();

    assert_eq!(mem_data, stream_data);
}

/// Build a 7z whose TOC interleaves no-stream entries (directory,
/// empty file) ahead of stream entries — the layout 7-Zip produces.
/// List order is [dir, empty.txt, dir/a.txt, dir/b.txt, top.txt]
/// while the for_each_entries walk visits stream files (block
/// order) first.
fn build_mixed_layout_archive(dir: &Path) -> PathBuf {
    let path = dir.join("mixed.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_directory("dir"),
            None::<&[u8]>,
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("empty.txt"),
            None::<&[u8]>,
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dir/a.txt"),
            Some(&b"alpha content"[..]),
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dir/b.txt"),
            Some(&b"bravo content"[..]),
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("top.txt"),
            Some(&b"top content"[..]),
        )
        .unwrap();
    writer.finish().unwrap();
    path
}

/// R0079-0004: pins the upstream block-major iteration model the
/// visit-order translation relies on. If a sevenz-rust2 bump
/// changes the for_each_entries walk, this fails loudly.
#[test]
fn test_sevenz_visit_order_is_block_major() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_mixed_layout_archive(temp.path());
    let archive = SevenZArchive::open(&archive_path).unwrap();
    let reader = archive.open_reader(crate::error::ops::LIST_FILES).unwrap();

    // Stream files in block order (a, b, top), then no-stream
    // files (dir, empty.txt) in files order.
    assert_eq!(
        archive.visit_order(reader.archive()).unwrap(),
        vec![2, 3, 4, 0, 1]
    );
}

/// R0079-0004: selection indices mean `list_files()` order, not
/// callback order — extracting `dir/a.txt` by its list index must
/// not materialize a different entry.
#[test]
fn test_sevenz_selective_extraction_matches_list_order() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_mixed_layout_archive(temp.path());
    let archive = SevenZArchive::open(&archive_path).unwrap();

    let entries = archive.list_files().unwrap();
    let a_idx = entries.iter().position(|e| e.path == "dir/a.txt").unwrap();
    // The no-stream entries precede it in the TOC, so list order
    // and callback order genuinely differ for this index.
    assert_eq!(a_idx, 2);

    let dest = temp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    let selection: std::collections::HashSet<usize> = [a_idx].into_iter().collect();
    archive
        .extract_all_with_options(&dest, None, true, true, true, true, Some(&selection))
        .unwrap();

    assert_eq!(
        std::fs::read(dest.join("dir/a.txt")).unwrap(),
        b"alpha content"
    );
    assert!(!dest.join("dir/b.txt").exists());
    assert!(!dest.join("top.txt").exists());
    assert!(!dest.join("empty.txt").exists());
}

/// R0079-0005: entries without the optional kCRC digest
/// (directories, no-stream empty files) list `crc32 = None`, file
/// entries report the real digest, and `verify_crc32` extraction
/// does not spuriously fail on the CRC-less entries.
#[test]
fn test_sevenz_crcless_entries_list_none_and_extract_verified() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_mixed_layout_archive(temp.path());
    let archive = SevenZArchive::open(&archive_path).unwrap();

    let entries = archive.list_files().unwrap();
    let by_path = |p: &str| entries.iter().find(|e| e.path == p).unwrap();
    assert_eq!(by_path("dir").crc32, None);
    assert_eq!(by_path("empty.txt").crc32, None);
    assert_eq!(
        by_path("dir/a.txt").crc32,
        Some(crc32fast::hash(b"alpha content"))
    );

    let dest = temp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    archive
        .extract_all_with_options(&dest, None, true, true, true, true, None)
        .unwrap();
    assert_eq!(std::fs::read(dest.join("empty.txt")).unwrap(), b"");
    assert_eq!(std::fs::read(dest.join("top.txt")).unwrap(), b"top content");
}

/// R0079-0006: the default `7z a -p` shape (content encrypted,
/// header plaintext) opens fine with any password; both the
/// missing-password and wrong-password failures must surface as
/// `ArchiveError::Password`, not Format/Io/Corruption.
#[test]
fn test_sevenz_content_encrypted_password_classification() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("enc.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer.set_encrypt_header(false);
    writer.set_content_methods(vec![
        sevenz_rust2::encoder_options::AesEncoderOptions::new(Password::from("correct")).into(),
        sevenz_rust2::EncoderMethod::LZMA2.into(),
    ]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("secret.txt"),
            Some(&b"secret payload"[..]),
        )
        .unwrap();
    writer.finish().unwrap();

    // Missing password: the typed PasswordRequired error is raised
    // by the walk itself.
    let no_pw = SevenZArchive::open(&archive_path).unwrap();
    let err = no_pw.extract_to_memory("secret.txt").unwrap_err();
    assert!(matches!(err, ArchiveError::Password { .. }), "got {err:?}");

    // Wrong password: garbage decode surfaces from inside the
    // callback and must reclassify as password-suspect.
    let wrong = SevenZArchive::open_with_password(&archive_path, "wrong").unwrap();
    let err = wrong.extract_to_memory("secret.txt").unwrap_err();
    assert!(matches!(err, ArchiveError::Password { .. }), "got {err:?}");

    let right = SevenZArchive::open_with_password(&archive_path, "correct").unwrap();
    assert_eq!(
        right.extract_to_memory("secret.txt").unwrap(),
        b"secret payload"
    );
}

/// OI-0080-007 item 1: pin the ambiguity `ArchiveError::Password`
/// documents, as behaviour rather than as prose.
///
/// A payload damaged on disk and decoded with the **correct** password
/// classifies as `Password`, exactly like the wrong-password case above
/// — 7z AES-256 has no authentication tag, so the backend cannot tell
/// the two apart and does not pretend to. The identical damage in an
/// unencrypted entry keeps its media classification, which is what makes
/// `classify_decode_error` scoped rather than a blanket rewrite.
#[test]
fn test_sevenz_damaged_encrypted_entry_classifies_as_password() {
    // Incompressible payload, so LZMA2 stores it near-verbatim and the
    // packed stream is comfortably longer than the byte we flip.
    let mut state: u32 = 0x1234_5678;
    let payload: Vec<u8> = (0..4096)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    // Past the 32-byte signature header and far short of the trailing
    // TOC: the flip lands in the entry's packed stream.
    const FLIP_AT: usize = 32 + 1024;

    let temp = tempfile::tempdir().unwrap();
    let build = |name: &str, encrypted: bool| -> PathBuf {
        let path = temp.path().join(name);
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        if encrypted {
            writer.set_encrypt_header(false);
            writer.set_content_methods(vec![
                sevenz_rust2::encoder_options::AesEncoderOptions::new(Password::from("correct"))
                    .into(),
                sevenz_rust2::EncoderMethod::LZMA2.into(),
            ]);
        }
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("payload.bin"),
                Some(&payload[..]),
            )
            .unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.len() > FLIP_AT + 64,
            "{name}: archive too short ({}) to damage its packed stream at {FLIP_AT}",
            bytes.len()
        );
        bytes[FLIP_AT] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();
        path
    };

    let encrypted = build("damaged_encrypted.7z", true);
    let err = SevenZArchive::open_with_password(&encrypted, "correct")
        .unwrap()
        .extract_to_memory("payload.bin")
        .unwrap_err();
    assert!(
        matches!(err, ArchiveError::Password { .. }),
        "damaged ciphertext read with the correct password must still \
         classify as Password (the documented ambiguity), got {err:?}"
    );

    let plain = build("damaged_plain.7z", false);
    let err = SevenZArchive::open(&plain)
        .unwrap()
        .extract_to_memory("payload.bin")
        .unwrap_err();
    assert!(
        matches!(
            err,
            ArchiveError::Corruption { .. } | ArchiveError::Io { .. }
        ),
        "the same damage in an unencrypted entry must keep its media \
         classification, got {err:?}"
    );
}

/// R0079-0019: `preserve_permissions` / `preserve_times` must be
/// honoured by the 7z extract path — an entry carrying Unix mode
/// 0o755 (p7zip attribute convention) and an NT-time mtime keeps
/// both when the flags are set, and gets neither when cleared.
#[test]
#[cfg(unix)]
fn test_sevenz_extract_preserves_mode_and_mtime_per_flags() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("mode.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    let mut entry = sevenz_rust2::ArchiveEntry::new_file("tool.sh");
    entry.has_windows_attributes = true;
    // Upper half carries the Unix mode (p7zip convention); 0x8000
    // is the "Unix extension" marker bit p7zip sets in the lower half.
    entry.windows_attributes = (0o755 << 16) | 0x8000;
    entry.has_last_modified_date = true;
    // 2010-01-02T03:04:06Z as NT time (100-ns ticks since 1601).
    let unix_secs: u64 = 1_262_401_446;
    entry.last_modified_date =
        sevenz_rust2::NtTime::new(unix_secs * 10_000_000 + 116_444_736_000_000_000);
    writer
        .push_archive_entry(entry, Some(&b"#!/bin/sh\n"[..]))
        .unwrap();
    writer.finish().unwrap();
    let expected_mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs);

    let archive = SevenZArchive::open(&archive_path).unwrap();

    let preserved = temp.path().join("preserved");
    std::fs::create_dir_all(&preserved).unwrap();
    archive
        .extract_all_with_options(&preserved, None, true, true, true, false, None)
        .unwrap();
    let meta = std::fs::metadata(preserved.join("tool.sh")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    assert_eq!(meta.modified().unwrap(), expected_mtime);

    let plain = temp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    archive
        .extract_all_with_options(&plain, None, true, false, false, false, None)
        .unwrap();
    let meta = std::fs::metadata(plain.join("tool.sh")).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o111,
        0,
        "exec bits must not be applied when preserve_permissions = false"
    );
    assert_ne!(meta.modified().unwrap(), expected_mtime);
}

/// R0079-0024: after a fatal per-entry error the walk must stop
/// materializing entries — upstream keeps invoking the callback
/// for later blocks, so without the guard `second.txt` would still
/// be written.
#[test]
fn test_sevenz_extract_all_stops_after_fatal_error() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("two_files.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("first.txt"),
            Some(&b"first"[..]),
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("second.txt"),
            Some(&b"second"[..]),
        )
        .unwrap();
    writer.finish().unwrap();

    let dest = temp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    // Pre-existing file + overwrite=false makes the first entry
    // fail deterministically.
    std::fs::write(dest.join("first.txt"), b"existing").unwrap();

    let archive = SevenZArchive::open(&archive_path).unwrap();
    let err = archive
        .extract_all_with_options(&dest, None, false, true, true, false, None)
        .unwrap_err();
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "got {err:?}"
    );
    assert!(!dest.join("second.txt").exists());
}

/// OI-0076-002: duplicate entry names are ambiguous for single-entry
/// APIs — the shared gate refuses to pick one (the previous
/// first-match behavior is deliberately superseded). R0079-0024's
/// block-stop guard still matters for the walk itself: once the
/// validated id has been handled, the callback must stop decoding
/// later blocks.
#[test]
fn test_sevenz_extract_to_memory_duplicate_names_blocked() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("dup.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dup.txt"),
            Some(&b"first copy"[..]),
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dup.txt"),
            Some(&b"second copy"[..]),
        )
        .unwrap();
    writer.finish().unwrap();

    let archive = SevenZArchive::open(&archive_path).unwrap();
    let err = archive.extract_to_memory("dup.txt").unwrap_err();
    assert!(
        matches!(
            &err,
            ArchiveError::OperationBlocked { reason, .. }
                if reason.contains("Multiple entries match")
        ),
        "got {err:?}"
    );
}

/// R0080-0030: a corrupt regular-file payload must be recorded in the
/// failed list, not abort `test_integrity` with an error. Only genuine
/// archive-file I/O errors propagate. COPY (stored) compression is used
/// so flipping one packed byte leaves the framing intact but breaks the
/// entry CRC, exercising the checksum-verification failure path.
#[test]
fn test_sevenz_integrity_records_corrupt_payload() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("corrupt.7z");
    let payload = b"unified-archive integrity payload marker bytes";

    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer.set_content_methods(vec![sevenz_rust2::EncoderConfiguration::new(
        sevenz_rust2::EncoderMethod::COPY,
    )]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("data.txt"),
            Some(&payload[..]),
        )
        .unwrap();
    writer.finish().unwrap();

    // COPY stores the payload verbatim; flip one packed byte so the
    // stored entry CRC no longer matches while the TOC stays intact.
    let mut bytes = std::fs::read(&archive_path).unwrap();
    let at = bytes
        .windows(payload.len())
        .position(|w| w == &payload[..])
        .expect("stored payload present in archive");
    bytes[at] ^= 0xFF;
    std::fs::write(&archive_path, &bytes).unwrap();

    let archive = SevenZArchive::open(&archive_path).unwrap();
    let failed = archive
        .test_integrity()
        .expect("corrupt payload must be recorded, not abort the walk");
    assert_eq!(failed, vec!["data.txt".to_string()]);
}

/// R0001-0024: an entry that decodes *short* must be recorded as an
/// integrity failure. Upstream's `Crc32VerifyingReader` compares the
/// digest only once its declared byte budget is consumed, so a clean
/// early EOF never reaches the comparison — the old drain loop saw
/// `Ok(0)`, broke, and reported the archive as valid.
///
/// The fixture removes the tail of the COPY-stored pack stream and
/// slides the trailing header back over it, fixing the signature
/// header's `NextHeaderOffset` and `StartHeaderCRC` so the TOC still
/// parses and still declares the full unpacked size.
#[test]
fn test_sevenz_integrity_records_short_entry() {
    const SIGNATURE_HEADER_SIZE: usize = 32;
    const REMOVED: u64 = 10_000;

    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("short.7z");
    let payload: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();

    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer.set_content_methods(vec![sevenz_rust2::EncoderConfiguration::new(
        sevenz_rust2::EncoderMethod::COPY,
    )]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("data.txt"),
            Some(&payload[..]),
        )
        .unwrap();
    writer.finish().unwrap();

    // Signature header layout: [8..12] StartHeaderCRC over [12..32],
    // [12..20] NextHeaderOffset (relative to the 32-byte signature
    // header), [20..28] NextHeaderSize, [28..32] NextHeaderCRC.
    let mut bytes = std::fs::read(&archive_path).unwrap();
    let next_header_offset = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    let header_start = SIGNATURE_HEADER_SIZE + next_header_offset as usize;
    // 0x01 = kHeader (raw). A kEncodedHeader (0x17) would put a second
    // packed stream inside the region this fixture truncates.
    assert_eq!(
        bytes[header_start], 0x01,
        "fixture assumes sevenz-rust2 wrote a raw (unencoded) 7z header"
    );
    assert!(next_header_offset > REMOVED);

    bytes.drain(header_start - REMOVED as usize..header_start);
    bytes[12..20].copy_from_slice(&(next_header_offset - REMOVED).to_le_bytes());
    let start_header_crc = crc32fast::hash(&bytes[12..SIGNATURE_HEADER_SIZE]);
    bytes[8..12].copy_from_slice(&start_header_crc.to_le_bytes());
    std::fs::write(&archive_path, &bytes).unwrap();

    let archive = SevenZArchive::open(&archive_path).unwrap();
    // The TOC survived: the entry still declares the full payload size.
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.first().unwrap().size, Some(payload.len() as u64));

    let failed = archive
        .test_integrity()
        .expect("a short payload is an integrity failure, not an operational error");
    assert_eq!(failed, vec!["data.txt".to_string()]);
}

/// R0001-0059: NT time counts from 1601-01-01, so 1601..1970 values
/// are valid instants. They used to be dropped by the
/// `checked_sub(unix_epoch_nt)` baseline; `SystemTime` represents them
/// fine. 1960-01-01T00:00:00Z is 3653 days (three leap years) before
/// the Unix epoch.
#[test]
fn test_sevenz_pre_epoch_modified_time_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("pre_epoch.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    let mut entry = sevenz_rust2::ArchiveEntry::new_file("old.txt");
    entry.has_last_modified_date = true;
    let before_epoch_secs: u64 = 3653 * 86_400;
    entry.last_modified_date =
        sevenz_rust2::NtTime::new(116_444_736_000_000_000 - before_epoch_secs * 10_000_000);
    writer
        .push_archive_entry(entry, Some(&b"vintage"[..]))
        .unwrap();
    writer.finish().unwrap();

    let expected = std::time::UNIX_EPOCH - std::time::Duration::from_secs(before_epoch_secs);
    let archive = SevenZArchive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.first().unwrap().modified, Some(expected));
}

/// R0001-0022: a p7zip entry whose Unix mode carries a non-regular,
/// non-directory `S_IFMT` (here `S_IFIFO`) must list as
/// `EntryType::Other`, must not be materialized by bulk extraction,
/// and must be refused by the single-entry gate. Previously it was
/// classified as a regular file and decoded under file semantics.
#[test]
fn test_sevenz_special_unix_mode_lists_other_and_is_not_materialized() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("special.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("plain.txt"),
            Some(&b"plain content"[..]),
        )
        .unwrap();
    let mut fifo = sevenz_rust2::ArchiveEntry::new_file("pipe");
    fifo.has_windows_attributes = true;
    // S_IFIFO | 0644 in the attribute word's upper half; 0x8000 is the
    // "Unix extension" marker bit p7zip sets in the lower half.
    fifo.windows_attributes = (0o010644 << 16) | 0x8000;
    writer
        .push_archive_entry(fifo, Some(&b"not a real fifo payload"[..]))
        .unwrap();
    writer.finish().unwrap();

    let archive = SevenZArchive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    let by_path = |p: &str| entries.iter().find(|e| e.path == p).unwrap();
    assert_eq!(by_path("pipe").entry_type, EntryType::Other);
    assert_eq!(by_path("plain.txt").entry_type, EntryType::File);

    let dest = temp.path().join("out");
    std::fs::create_dir_all(&dest).unwrap();
    archive
        .extract_all_with_options(&dest, None, true, true, true, false, None)
        .unwrap();
    assert_eq!(
        std::fs::read(dest.join("plain.txt")).unwrap(),
        b"plain content"
    );
    assert!(
        !dest.join("pipe").exists(),
        "a special-mode entry must not be materialized as a regular file"
    );

    // The shared single-entry gate refuses non-regular entries.
    let err = archive.extract_to_memory("pipe").unwrap_err();
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "got {err:?}"
    );
}

/// R0001-0060: `preserve_permissions` / `preserve_times` reached only
/// the file writer, leaving extracted directories with the umask mode
/// and the creation-time mtime. The deferred deepest-first pass must
/// restore both — after the directory's children are installed, so the
/// child writes cannot re-stamp the parent.
#[test]
#[cfg(unix)]
fn test_sevenz_extract_restores_directory_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("dirmeta.7z");
    let unix_secs: u64 = 1_400_000_000;
    let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
    let mut dir = sevenz_rust2::ArchiveEntry::new_directory("subdir");
    dir.has_windows_attributes = true;
    // S_IFDIR | 0750 in the attribute word's upper half.
    dir.windows_attributes = (0o040750 << 16) | 0x8000;
    dir.has_last_modified_date = true;
    dir.last_modified_date =
        sevenz_rust2::NtTime::new(unix_secs * 10_000_000 + 116_444_736_000_000_000);
    writer.push_archive_entry(dir, None::<&[u8]>).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("subdir/child.txt"),
            Some(&b"child content"[..]),
        )
        .unwrap();
    writer.finish().unwrap();
    let expected_mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs);

    let archive = SevenZArchive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    let listed_dir = entries.iter().find(|e| e.path == "subdir").unwrap();
    assert_eq!(listed_dir.entry_type, EntryType::Directory);

    let preserved = temp.path().join("preserved");
    std::fs::create_dir_all(&preserved).unwrap();
    archive
        .extract_all_with_options(&preserved, None, true, true, true, false, None)
        .unwrap();
    assert_eq!(
        std::fs::read(preserved.join("subdir/child.txt")).unwrap(),
        b"child content"
    );
    let meta = std::fs::metadata(preserved.join("subdir")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o750);
    assert_eq!(meta.modified().unwrap(), expected_mtime);

    let plain = temp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    archive
        .extract_all_with_options(&plain, None, true, false, false, false, None)
        .unwrap();
    let meta = std::fs::metadata(plain.join("subdir")).unwrap();
    assert_ne!(
        meta.modified().unwrap(),
        expected_mtime,
        "directory mtime must not be applied when preserve_times = false"
    );
}
/// Build a 7z that repeats one listing path, where the first
/// occurrence carries **no** kCRC digest.
///
/// `ArchiveEntry::new_file` with a `None` payload produces a
/// no-stream entry: `has_crc` stays false, so
/// [`SevenZArchive::entry_crc32`] lists `crc32: None` for it. The
/// second occurrence is a streamed member and does carry a digest.
/// The pair is therefore the minimal 7z that reaches the
/// content-multiset digest walk's streaming arm *and* trips the
/// by-path single-entry gate.
fn build_crcless_duplicate_path_archive(dir: &Path) -> PathBuf {
    let path = dir.join("dup-crcless.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dup.txt"),
            None::<&[u8]>,
        )
        .unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("dup.txt"),
            Some(&b"seven content"[..]),
        )
        .unwrap();
    writer.finish().unwrap();
    path
}

/// The listing precondition the two tests below rest on: a 7z file
/// entry really can carry `crc32: None`, so the claim that "7z carries
/// a real per-entry CRC32 for every file entry" is false and the
/// digest walk really does reach 7z's streaming arm.
#[test]
fn test_sevenz_crcless_file_entry_can_repeat_a_path() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_crcless_duplicate_path_archive(temp.path());
    let entries = SevenZArchive::open(&archive_path)
        .unwrap()
        .list_files()
        .unwrap();

    let dups: Vec<_> = entries.iter().filter(|e| e.path == "dup.txt").collect();
    assert_eq!(dups.len(), 2, "both occurrences must be listed");
    assert!(
        dups.iter().all(|e| e.entry_type == crate::EntryType::File),
        "both occurrences must list as file entries"
    );
    assert_eq!(
        dups[0].crc32, None,
        "no-stream entry carries no kCRC digest"
    );
    assert_eq!(dups[1].crc32, Some(crc32fast::hash(b"seven content")));
}

/// Id-addressed streaming reaches each occurrence's own payload
/// instead of aliasing to the first (OI-0076-002).
#[test]
fn test_sevenz_extract_to_stream_by_listing_id_hits_each_occurrence() {
    use std::io::Read as _;

    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_crcless_duplicate_path_archive(temp.path());
    let archive = SevenZArchive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    let ids: Vec<usize> = entries
        .iter()
        .filter(|e| e.path == "dup.txt")
        .map(|e| e.id)
        .collect();

    let read_id = |id: usize| {
        let mut buf = Vec::new();
        archive
            .extract_to_stream_by_listing_id(id, "dup.txt")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        buf
    };
    assert_eq!(read_id(ids[0]), b"");
    assert_eq!(read_id(ids[1]), b"seven content");

    // The drift guard still refuses a name that disagrees with the id.
    assert!(
        archive
            .extract_to_stream_by_listing_id(ids[1], "other.txt")
            .is_err(),
        "listing drift must be refused"
    );
}

/// The regression this whole seam exists for, asserted at the
/// **facade** — the only altitude that exercises the
/// `ReadBackend::extract_to_stream_by_listing_id` forward. Calling the
/// inherent method (the test above) passes with or without the
/// forward; without it, this one fails with `OperationBlocked
/// { reason: "Multiple entries match ..." }` from
/// `security::validate_single_entry`, exactly as ZIP did between the
/// DCR-012 listing change and its own forward being wired.
#[test]
fn test_sevenz_crcless_duplicate_path_digests_through_the_facade() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = build_crcless_duplicate_path_archive(temp.path());

    let (digest, total_size) = crate::Archive::open(&archive_path)
        .unwrap()
        .calculate_content_multiset_digest_and_size()
        .expect("a CRC-less duplicate-path 7z must still digest");

    // One element per file entry: the empty no-stream occurrence's
    // streamed payload, and the listed digest of the streamed one.
    let mut elements = [
        format!("{:08x}", crc32fast::hash(b"")),
        format!("{:08x}", crc32fast::hash(b"seven content")),
    ];
    elements.sort();
    assert_eq!(
        digest,
        format!("{:08x}", crc32fast::hash(elements.join(",").as_bytes()))
    );
    // OI-0001-007: 7z declares a size for every non-directory entry, so the
    // typed total is complete here and `exact()` is the honest assertion —
    // the empty no-stream occurrence contributes a declared `0`, not an
    // unknown.
    assert_eq!(total_size.exact(), Some(b"seven content".len() as u64));
}

// ── OI-0001-002: the handle is bound to the archive FILE ──
//
// The listing is memoised once (AD 0065) and every later 7z operation
// re-opens the archive by pathname. The per-entry guards on those
// re-opens compare the normalised entry *name*, so a replacement that
// keeps the names passed all of them while the safety-gate decisions
// stayed those of the stale snapshot. `SevenZArchive::open_reader` now
// binds the handle to the file itself and re-checks at every re-open.

/// Build a single-entry 7z at `path` whose sole entry is `only.txt`
/// carrying `payload`.
///
/// Two calls with different payloads produce archives that agree on
/// every entry name and cardinality and disagree on file length — the
/// exact shape the name guards cannot tell apart.
fn build_single_entry_archive(path: &Path, payload: &[u8]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("only.txt"),
            Some(payload),
        )
        .unwrap();
    writer.finish().unwrap();
}

/// A decoy payload long enough that the two archives cannot come out
/// the same length — the length half of the identity is the only half
/// that exists off Unix.
fn decoy_payload() -> Vec<u8> {
    (0..4096u32).map(|i| (i % 251) as u8).collect()
}

/// The error from a call that must have been refused.
///
/// Spelled as a helper rather than `.err().expect(..)` (which clippy's
/// `err_expect` rejects) or `.expect_err(..)` (which needs `T: Debug`,
/// and several of these `Ok` types are not).
#[track_caller]
fn refusal<T>(result: crate::error::Result<T>, what: &str) -> ArchiveError {
    match result {
        Ok(_) => panic!("{what}"),
        Err(err) => err,
    }
}

/// Assert `err` is the identity refusal, raised for `op`.
///
/// Pins both halves of the two-vocabulary split: identity drift is
/// `OperationBlocked` + "identity changed", name/cardinality drift stays
/// `Format` + "listing drift", and neither borrows the other's words.
#[track_caller]
fn assert_identity_blocked(err: ArchiveError, op: &str) {
    match err {
        ArchiveError::OperationBlocked { operation, reason } => {
            assert_eq!(
                operation, op,
                "the refusal must name the caller's own operation, not a fixed open-family label"
            );
            assert!(
                reason.contains("identity changed"),
                "expected the identity-drift vocabulary, got: {reason}"
            );
            assert!(
                !reason.contains("listing drift"),
                "identity drift must not borrow the name-guard vocabulary: {reason}"
            );
        }
        other => panic!("expected the identity refusal for {op}, got {other:?}"),
    }
}

/// Every operation that re-opens the archive refuses a same-name
/// replacement, at the facade — the altitude a caller actually uses.
///
/// **Non-vacuity.** The decoy lists the same single entry name as the
/// original (asserted before the swap), so no name or cardinality guard
/// can see anything wrong here; the file-identity comparison is the
/// only thing that can refuse. Delete the comparison in
/// `SevenZArchive::bind_or_check_identity` and every
/// `assert_identity_blocked` below fails — starting with
/// `extract_to_memory`, which would hand back the decoy's payload as
/// though it were the listed entry's.
#[test]
fn test_sevenz_same_name_file_swap_is_refused_by_every_reopening_operation() {
    let temp = tempfile::tempdir().unwrap();
    let live = temp.path().join("live.7z");
    let decoy = temp.path().join("decoy.7z");
    build_single_entry_archive(&live, b"original 7z payload");
    build_single_entry_archive(&decoy, &decoy_payload());

    let archive = crate::Archive::open(&live).unwrap();
    let listed: Vec<String> = archive
        .list_files()
        .unwrap()
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    assert_eq!(listed, vec!["only.txt".to_string()]);

    {
        let decoy_handle = crate::Archive::open(&decoy).unwrap();
        let decoy_listed: Vec<String> = decoy_handle
            .list_files()
            .unwrap()
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert_eq!(
            decoy_listed, listed,
            "the replacement must be invisible to the name and cardinality guards"
        );
    }
    assert_ne!(
        std::fs::metadata(&live).unwrap().len(),
        std::fs::metadata(&decoy).unwrap().len(),
        "the two archives must differ in length, or the off-Unix half of the binding proves nothing"
    );

    std::fs::rename(&decoy, &live).unwrap();

    // `EXTRACT_ALL`, not this backend's own `EXTRACT`: the facade's
    // compression-ratio gate revalidates the binding before dispatching, so
    // for `extract_all` its refusal lands first and names the public call.
    // The 7z backend's own guard still says `extract` and would be what a
    // caller saw if the gate were removed — that divergence is real and is
    // tracked separately (ticgit c9e726, op labels differ by backend).
    assert_identity_blocked(
        refusal(
            archive.extract_all(crate::ExtractionOptions::new(temp.path().join("all"))),
            "extract_all must refuse a swapped archive",
        ),
        crate::error::ops::EXTRACT_ALL,
    );
    assert_identity_blocked(
        refusal(
            archive.extract_file(
                "only.txt",
                crate::ExtractionOptions::new(temp.path().join("one")),
            ),
            "extract_file must refuse a swapped archive",
        ),
        crate::error::ops::EXTRACT_FILE,
    );
    assert_identity_blocked(
        refusal(
            archive.extract_to_memory("only.txt"),
            "extract_to_memory must refuse a swapped archive",
        ),
        crate::error::ops::EXTRACT_TO_MEMORY,
    );
    assert_identity_blocked(
        refusal(
            archive.extract_to_stream("only.txt", crate::StreamBound::DeclaredSize),
            "extract_to_stream must refuse a swapped archive",
        ),
        crate::error::ops::EXTRACT_TO_STREAM,
    );
    assert_identity_blocked(
        refusal(
            archive.validate_integrity(),
            "validate_integrity must refuse a swapped archive",
        ),
        crate::error::ops::VALIDATE_INTEGRITY,
    );
    assert_identity_blocked(
        refusal(archive.is_solid(), "is_solid must refuse a swapped archive"),
        crate::error::ops::LIST_FILES,
    );
}

/// The listing walk is a bound re-open too, not just its consumers.
///
/// Binding through `is_solid` leaves the AD 0065 listing cache empty, so
/// the swap lands *before* the snapshot is taken and the walk itself is
/// what refuses. Without the comparison the walk would happily adopt the
/// decoy's table of contents as this handle's listing.
#[test]
fn test_sevenz_listing_walk_refuses_a_swapped_file() {
    let temp = tempfile::tempdir().unwrap();
    let live = temp.path().join("live.7z");
    let decoy = temp.path().join("decoy.7z");
    build_single_entry_archive(&live, b"original 7z payload");
    build_single_entry_archive(&decoy, &decoy_payload());

    let archive = SevenZArchive::open(&live).unwrap();
    archive
        .is_solid()
        .expect("the first probe binds the handle");

    std::fs::rename(&decoy, &live).unwrap();

    assert_identity_blocked(
        refusal(
            archive.list_files(),
            "the listing walk must refuse a swapped archive",
        ),
        crate::error::ops::LIST_FILES,
    );
}

/// `len` is part of the identity on purpose: an append keeps the inode,
/// so `(dev, ino)` alone would be blind to entries the safety gate never
/// saw. Unix-only because the inode precondition is what makes the test
/// mean anything — off Unix the identity is the length already.
#[cfg(unix)]
#[test]
fn test_sevenz_append_under_a_live_handle_is_refused() {
    use std::io::Write as _;

    let temp = tempfile::tempdir().unwrap();
    let live = temp.path().join("live.7z");
    build_single_entry_archive(&live, b"original 7z payload");

    let archive = crate::Archive::open(&live).unwrap();
    archive.list_files().unwrap();
    let before = std::fs::metadata(&live).unwrap();

    let mut appended = std::fs::OpenOptions::new()
        .append(true)
        .open(&live)
        .unwrap();
    appended.write_all(b"trailing bytes").unwrap();
    appended.flush().unwrap();
    drop(appended);

    let after = std::fs::metadata(&live).unwrap();
    assert_eq!(
        crate::fs_identity::InodeId::from_metadata(&before),
        crate::fs_identity::InodeId::from_metadata(&after),
        "an append keeps the inode — that precondition is the whole point of this test"
    );
    assert_ne!(before.len(), after.len());

    assert_identity_blocked(
        refusal(
            archive.extract_to_memory("only.txt"),
            "an appended-to archive must be refused",
        ),
        crate::error::ops::EXTRACT_TO_MEMORY,
    );
}

/// AD 0052: `open()` touches no file and the *first operation* validates,
/// so a first operation against an archive a producer is still writing is
/// a supported, recoverable shape — the 7z directory lives at the end, the
/// parse fails, and the caller (every entry point takes `&self`) retries.
///
/// Non-vacuity: move the bind in `SevenZArchive::open_reader` back ahead of
/// `ArchiveReader::open` — i.e. make the pre-open half
/// `bind_or_check_identity` again — and the retry below fails with
/// `OperationBlocked` / "identity changed", because the failed attempt
/// recorded the half-written length as the binding. Nobody swapped
/// anything; the archive is complete and healthy.
#[test]
fn test_sevenz_a_failed_first_operation_leaves_the_handle_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let complete = dir.path().join("complete.7z");
    build_single_entry_archive(&complete, &decoy_payload());
    let bytes = std::fs::read(&complete).unwrap();

    // The producer is mid-write: the end-of-archive header is not there
    // yet, so nothing can parse this.
    let path = dir.path().join("still-being-written.7z");
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();

    let archive = SevenZArchive::open(&path).unwrap();
    let err = refusal(
        archive.list_files(),
        "a half-written 7z must not parse — the fixture proves nothing otherwise",
    );
    assert!(
        matches!(err, ArchiveError::Format { .. }),
        "a truncated 7z is a format failure, not an identity one: {err:?}"
    );

    // The producer finishes, in place: same inode, full length.
    std::fs::write(&path, &bytes).unwrap();

    let entries = archive
        .list_files()
        .expect("the retry runs against a complete, healthy archive nobody swapped");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "only.txt");
}

// ── DEF-001: in-place reads of a 7z payload at a non-zero offset ──
//
// A self-extracting 7z is a stub executable with a real archive some way
// in. The old answer copied `[offset, EOF)` to a tempfile and opened
// that; `SevenZArchive::open_at_offset` opens it where it lies, over a
// `PayloadWindow`. These tests pin the three things that makes true:
// the payload is found and read correctly at a non-zero offset, offset 0
// is untouched, and a *wrong* offset is refused rather than
// misinterpreted.

/// Write `stub` followed by the bytes of a freshly built single-entry
/// 7z, and return `(path, offset_of_the_payload)`.
///
/// The 7z is built by this crate's own dependency, so no external
/// archiver is invoked and nothing is read from `tests/fixtures/`.
fn build_embedded_archive(dir: &Path, stub: &[u8], payload: &[u8]) -> (PathBuf, u64) {
    let bare = dir.join("payload-source.7z");
    build_single_entry_archive(&bare, payload);
    let archive_bytes = std::fs::read(&bare).unwrap();

    let embedded = dir.join("sfx.exe");
    let mut blob = stub.to_vec();
    blob.extend_from_slice(&archive_bytes);
    std::fs::write(&embedded, &blob).unwrap();

    (embedded, stub.len() as u64)
}

/// A stub whose bytes are deliberately *not* 7z-shaped, so nothing in
/// the read path can succeed by accidentally starting at byte 0.
fn sfx_stub() -> Vec<u8> {
    let mut stub = b"MZ\x90\x00 not a 7z, this is the extractor stub ".to_vec();
    stub.extend((0..8192u32).map(|i| (i % 253) as u8));
    stub
}

/// The headline case: a real 7z embedded at a non-zero offset lists and
/// extracts through `open_at_offset`, with no copy of the payload.
///
/// Non-vacuity: the same handle opened at offset 0 (the next test)
/// fails, so the offset is doing the work rather than some fallback
/// finding the archive on its own.
#[test]
fn test_sevenz_open_at_offset_lists_and_extracts_the_embedded_payload() {
    let temp = tempfile::tempdir().unwrap();
    let payload = decoy_payload();
    let (embedded, offset) = build_embedded_archive(temp.path(), &sfx_stub(), &payload);
    assert_ne!(offset, 0, "the fixture must actually be offset");

    let archive = SevenZArchive::open_at_offset(&embedded, offset).unwrap();

    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "only.txt");

    assert_eq!(
        archive.extract_to_memory("only.txt").unwrap(),
        payload,
        "the entry must decode to the bytes that went in, not to stub bytes"
    );

    assert!(
        archive.test_integrity().unwrap().is_empty(),
        "CRC32 verification must pass through the window"
    );
}

/// Non-vacuity for the test above: opening the same SFX at offset 0
/// fails, because the stub is not a 7z. If this ever passes, the
/// offset test is proving nothing.
#[test]
fn test_sevenz_embedded_payload_is_invisible_at_offset_zero() {
    let temp = tempfile::tempdir().unwrap();
    let (embedded, _) = build_embedded_archive(temp.path(), &sfx_stub(), b"payload");

    let err = refusal(
        SevenZArchive::open(&embedded).unwrap().list_files(),
        "an SFX stub is not a 7z and must not parse as one",
    );
    assert!(
        matches!(err, ArchiveError::Format { .. }),
        "expected a format failure at offset 0, got: {err:?}"
    );
}

/// Offset 0 is the ordinary path and must be indistinguishable from
/// what `open` does — same listing, same bytes.
#[test]
fn test_sevenz_offset_zero_matches_a_plain_open() {
    let temp = tempfile::tempdir().unwrap();
    let bare = temp.path().join("plain.7z");
    let payload = decoy_payload();
    build_single_entry_archive(&bare, &payload);

    let plain = SevenZArchive::open(&bare).unwrap();
    let at_zero = SevenZArchive::open_at_offset(&bare, 0).unwrap();

    assert_eq!(
        plain.list_files().unwrap().len(),
        at_zero.list_files().unwrap().len()
    );
    assert_eq!(
        plain.extract_to_memory("only.txt").unwrap(),
        at_zero.extract_to_memory("only.txt").unwrap()
    );
    assert_eq!(at_zero.extract_to_memory("only.txt").unwrap(), payload);
}

/// A wrong offset must fail the six-byte signature check, not open
/// something bogus. `sevenz_rust2::Archive::read` demands the signature
/// at stream position 0 and searches for nothing, which is exactly why
/// no offset-agreement gate is needed for 7z.
#[test]
fn test_sevenz_wrong_offset_fails_the_signature_check() {
    let temp = tempfile::tempdir().unwrap();
    let (embedded, offset) = build_embedded_archive(temp.path(), &sfx_stub(), &decoy_payload());

    // One byte early and one byte late: both land inside the file, so
    // the window is a perfectly good stream — it simply does not begin
    // with `7z\xBC\xAF\x27\x1C`.
    for wrong in [offset - 1, offset + 1] {
        let archive = SevenZArchive::open_at_offset(&embedded, wrong).unwrap();
        let err = refusal(
            archive.list_files(),
            "a payload window that does not start at the signature must be refused",
        );
        assert!(
            matches!(err, ArchiveError::Format { .. }),
            "offset {wrong} should be a format failure, got: {err:?}"
        );
    }
}

/// An offset past the end of the file is a caller bug worth naming:
/// `PayloadWindow::from_file` refuses it and the backend surfaces a
/// typed `Io` error rather than an empty window that reads as a corrupt
/// archive.
#[test]
fn test_sevenz_offset_past_eof_is_an_io_error() {
    let temp = tempfile::tempdir().unwrap();
    let bare = temp.path().join("plain.7z");
    build_single_entry_archive(&bare, b"payload");
    let len = std::fs::metadata(&bare).unwrap().len();

    let archive = SevenZArchive::open_at_offset(&bare, len + 1).unwrap();
    let err = refusal(
        archive.list_files(),
        "an offset past EOF cannot name a payload",
    );
    assert!(
        matches!(err, ArchiveError::Io { .. }),
        "expected a typed Io error, got: {err:?}"
    );
}

/// R0070-0049 / precision improvement: a missing archive now surfaces as
/// `ArchiveError::Io` instead of being funnelled through the sevenz
/// error-*text* classifier as "Invalid 7z". `open_reader` owns the
/// `File::open` since DEF-001, so the typed error is available and a
/// file that cannot be opened is no longer reported as a format failure.
#[test]
fn test_sevenz_missing_archive_is_an_io_error_not_a_format_error() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("not-there.7z");

    let archive = SevenZArchive::open(&missing).unwrap();
    let err = refusal(
        archive.list_files(),
        "AD 0052 defers validation, so the *operation* must be the one to fail",
    );
    match err {
        ArchiveError::Io {
            operation, path, ..
        } => {
            assert_eq!(operation, "open");
            assert_eq!(path, missing);
        }
        other => panic!("expected a typed Io error for a missing archive, got {other:?}"),
    }
}

/// AD 0052 survives the offset constructor: `open_at_offset` parses
/// nothing, so a payload that carries 7z magic but is corrupt fails at
/// the *first operation*, exactly like an ordinary corrupt `.7z`.
#[test]
fn test_sevenz_open_at_offset_defers_validation_to_the_first_operation() {
    let temp = tempfile::tempdir().unwrap();
    let bare = temp.path().join("complete.7z");
    build_single_entry_archive(&bare, &decoy_payload());
    let archive_bytes = std::fs::read(&bare).unwrap();

    // Magic intact, end-of-archive header gone.
    let stub = sfx_stub();
    let mut blob = stub.clone();
    blob.extend_from_slice(&archive_bytes[..archive_bytes.len() / 2]);
    let truncated = temp.path().join("truncated-sfx.exe");
    std::fs::write(&truncated, &blob).unwrap();

    let offset = stub.len() as u64;
    assert_eq!(
        &blob[stub.len()..stub.len() + 6],
        b"7z\xBC\xAF\x27\x1C",
        "the fixture must keep the signature — otherwise this proves nothing"
    );

    // The constructor touches no file.
    let archive = SevenZArchive::open_at_offset(&truncated, offset)
        .expect("open_at_offset must not parse, and so must not fail here");

    let err = refusal(
        archive.list_files(),
        "a truncated payload must fail at the first operation",
    );
    assert!(
        matches!(err, ArchiveError::Format { .. }),
        "a truncated 7z is a format failure: {err:?}"
    );
}

/// The identity binding is captured for an offset handle exactly as it
/// is for an ordinary one: the first successful operation binds, and a
/// same-name replacement of the *outer* file is refused afterwards.
///
/// Non-vacuity: both SFX files carry the same single entry name and the
/// same stub, so no name or cardinality guard can tell them apart — the
/// file-identity comparison is the only thing that can.
#[test]
fn test_sevenz_offset_handle_still_binds_its_identity() {
    let temp = tempfile::tempdir().unwrap();
    let stub = sfx_stub();

    let live = temp.path().join("live.exe");
    let (built, offset) = build_embedded_archive(temp.path(), &stub, b"original payload");
    std::fs::rename(&built, &live).unwrap();

    let archive = SevenZArchive::open_at_offset(&live, offset).unwrap();
    assert_eq!(archive.list_files().unwrap()[0].path, "only.txt");

    // A different SFX at the same pathname, same entry name, different
    // length.
    let decoy_dir = temp.path().join("decoy");
    std::fs::create_dir(&decoy_dir).unwrap();
    let (decoy, decoy_offset) = build_embedded_archive(&decoy_dir, &stub, &decoy_payload());
    assert_eq!(decoy_offset, offset, "same stub, so same payload offset");
    std::fs::rename(&decoy, &live).unwrap();

    assert_identity_blocked(
        refusal(
            archive.extract_to_memory("only.txt"),
            "an offset handle must refuse a swapped outer file",
        ),
        crate::error::ops::EXTRACT_TO_MEMORY,
    );
}

// ── OI-0080-007 item 2: the re-alignment drain ──

/// A `Read` scripted to produce a fixed number of bytes and then a
/// chosen ending, so both `drain_to_realign` abort branches are
/// reachable. No 7z archive buildable on this host reaches them: a
/// COPY-stored entry only fails its CRC once its byte budget is spent,
/// so the drain that follows sees a clean EOF, and a corrupt LZMA2
/// stream hangs inside the upstream decoder before the drain is reached.
struct ScriptedReader {
    /// Bytes still to hand out before `ending` applies.
    yield_bytes: usize,
    ending: Ending,
}

enum Ending {
    /// Clean end of stream — the cursor is where it should be.
    Eof,
    /// The decoder failed while draining.
    Error,
    /// The decoder keeps producing past the declared size.
    Endless,
}

impl std::io::Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.yield_bytes > 0 {
            let n = self.yield_bytes.min(buf.len());
            self.yield_bytes -= n;
            buf[..n].fill(0xAB);
            return Ok(n);
        }
        match self.ending {
            Ending::Eof => Ok(0),
            Ending::Error => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "decoder gave up mid-drain",
            )),
            Ending::Endless => {
                let n = buf.len().max(1).min(buf.len());
                buf[..n].fill(0xCD);
                Ok(n)
            }
        }
    }
}

/// The ordinary case: the rest of the entry is consumed, the cursor is
/// re-aligned, and the walk continues. This is what keeps
/// `test_sevenz_integrity_records_corrupt_payload` passing — a corrupt
/// payload stays a recorded failure and does not abort the scan.
#[test]
fn test_drain_realigns_when_the_entry_ends_cleanly() {
    let mut buf = [0u8; 8192];
    let mut reader = ScriptedReader {
        yield_bytes: 20_000,
        ending: Ending::Eof,
    };
    assert!(super::drain_to_realign(&mut reader, &mut buf, 20_000).is_ok());
}

/// A drain that errors means re-alignment did not happen. The old code
/// swallowed this (`read(..).unwrap_or(0)`), so the walk continued from
/// an unknown offset and could record a healthy later entry as corrupt.
#[test]
fn test_drain_reports_a_lost_cursor_when_the_drain_errors() {
    let mut buf = [0u8; 8192];
    let mut reader = ScriptedReader {
        yield_bytes: 100,
        ending: Ending::Error,
    };
    let reason = super::drain_to_realign(&mut reader, &mut buf, 20_000)
        .expect_err("a drain error must not be reported as a successful re-alignment");
    assert!(
        reason.contains("could not be re-aligned"),
        "the reason must say the cursor is lost, got: {reason}"
    );
    assert!(
        reason.contains("decoder gave up mid-drain"),
        "the underlying error must survive into the reason, got: {reason}"
    );
}

/// An entry that decodes past its declared size is the same
/// misalignment by a different route, and is the branch that makes the
/// bound necessary: without it the loop never returns.
#[test]
fn test_drain_reports_a_lost_cursor_when_the_entry_overruns() {
    let mut buf = [0u8; 8192];
    let mut reader = ScriptedReader {
        yield_bytes: 0,
        ending: Ending::Endless,
    };
    let reason = super::drain_to_realign(&mut reader, &mut buf, 4_096)
        .expect_err("an over-producing entry must not be reported as re-aligned");
    assert!(
        reason.contains("past its declared size"),
        "the reason must name the overrun, got: {reason}"
    );
}

/// The bound is what makes the drain terminate at all. An endless
/// reader must be answered, not looped on — this test hangs forever if
/// the bound is removed, which is exactly the pre-existing defect in
/// the loop this replaced.
#[test]
fn test_drain_terminates_against_an_endless_reader() {
    let mut buf = [0u8; 8192];
    let mut reader = ScriptedReader {
        yield_bytes: 0,
        ending: Ending::Endless,
    };
    // 64 MiB of declared size against a reader that never stops: the
    // bound has to end this in a bounded number of iterations.
    assert!(super::drain_to_realign(&mut reader, &mut buf, 64 * 1024 * 1024).is_err());
}

/// `IntegrityScanAborted` must not read like an ordinary failure
/// report. A caller skimming the message has to learn that the entries
/// it never reached are *untested*, not passed.
#[test]
fn test_integrity_scan_aborted_message_states_the_results_are_incomplete() {
    let err = crate::error::ArchiveError::integrity_scan_aborted(
        "/tmp/broken.7z",
        "a.txt",
        "the solid-block cursor could not be re-aligned after a corrupt entry",
        vec!["a.txt".to_string()],
    );
    let msg = err.to_string();
    assert!(msg.contains("incomplete"), "{msg}");
    assert!(msg.contains("untested, not passed"), "{msg}");
    assert!(msg.contains("a.txt"), "{msg}");
}
