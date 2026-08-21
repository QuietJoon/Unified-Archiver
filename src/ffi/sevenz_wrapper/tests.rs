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
    let reader = archive.open_reader().unwrap();

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
    assert_eq!(total_size, b"seven content".len() as u64);
}
