use super::*;
use crate::test_utils::fixture;

#[test]
fn test_zip_wrapper_open_valid() {
    let path = fixture("test.zip");
    let archive = ZipArchive::open(&path);
    assert!(archive.is_ok());
    assert_eq!(archive.unwrap().path(), path);
}

/// AD 0052 (deferral retired 2026-08-21): `validate()` is only worth
/// having if it is *cheap*, and "cheap" rests entirely on the AD 0065
/// frozen listing cache absorbing the parse it forces. This proves the
/// second call does not parse again — without it, `validate()` would be
/// a hidden second cost and a trap at every call site.
///
/// The proof is allocation identity rather than a timing measurement: a
/// re-parse necessarily builds a *new* `Vec<ArchiveEntry>`, so if the
/// listing the facade hands out after `validate()` is the very
/// allocation `validate()` populated in the backend's cache, no second
/// parse happened. `test.zip` holds one entry, so the `Vec` is a real
/// allocation and not the shared dangling pointer every empty `Vec`
/// reports.
#[test]
fn validate_then_list_files_serves_the_cached_listing() {
    use crate::archive::{Archive, ArchiveBackend};
    use std::sync::Arc;

    let archive = Archive::open(fixture("test.zip")).unwrap();
    let ArchiveBackend::ZipReader(zip) = &archive.backend else {
        panic!("a .zip must open on the ZipReader backend");
    };

    // First-operation validation: `open` parsed nothing, so both cache
    // layers are still empty. If this ever fails, `open` grew an eager
    // parse and the AD 0052 contract changed.
    assert!(
        zip.listing.get().is_none(),
        "Archive::open must not parse the central directory (AD 0052 first-operation validation)"
    );
    assert!(
        archive.entry_cache.get().is_none(),
        "Archive::open must not populate the facade listing cache"
    );

    archive.validate().expect("test.zip is a valid archive");

    let parsed = zip
        .listing
        .get()
        .expect("validate() must force the first parse and memoise it (AD 0065)");
    let parsed_arc = Arc::as_ptr(parsed);
    let parsed_entries = parsed.as_ptr();
    assert!(!parsed.is_empty(), "fixture must have at least one entry");

    let listed = archive.list_files().unwrap();

    // Same `Vec` allocation ⇒ the listing was not rebuilt.
    assert!(
        std::ptr::eq(listed.as_ptr(), parsed_entries),
        "list_files() after validate() re-parsed the archive: it returned a different \
         Vec<ArchiveEntry> allocation than the one validate() cached"
    );
    // Same `Arc` allocation ⇒ not even a fresh Arc was wrapped around a
    // fresh Vec; the facade cache is a clone of the backend's.
    assert_eq!(
        Arc::as_ptr(archive.entry_cache.get().expect("facade cache populated")),
        parsed_arc,
        "the facade listing cache must share the backend's Arc, not a second snapshot"
    );
    // And the backend's own cell was never re-initialised.
    assert_eq!(
        Arc::as_ptr(zip.listing.get().unwrap()),
        parsed_arc,
        "the backend listing cache changed identity across list_files()"
    );
}

/// The other half of the `validate()` contract: it must actually
/// *report* an unusable input. A corrupt ZIP opens fine (AD 0052) — the
/// probe is what turns that into an error.
#[test]
fn validate_reports_a_corrupt_archive_that_open_accepted() {
    use crate::archive::Archive;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.zip");
    // Valid local-file-header magic so format detection says ZIP, then
    // garbage where the rest of the archive should be.
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(&[0xAB; 256]);
    std::fs::write(&path, &bytes).unwrap();

    let archive = Archive::open(&path).expect("open must succeed: validation is first-operation");
    let err = archive
        .validate()
        .expect_err("validate() must surface the corruption open() skipped");
    // Same error the first real operation would have produced.
    let from_list = archive.list_files().expect_err("listing must fail too");
    assert_eq!(
        std::mem::discriminant(&err),
        std::mem::discriminant(&from_list),
        "validate() must report the same failure class as the first operation"
    );
}

#[test]
fn test_zip_wrapper_open_nonexistent_list_fails() {
    // open() succeeds (lazy open), but list_files() fails when file doesn't exist
    let archive = ZipArchive::open("/nonexistent/archive.zip").unwrap();
    let result = archive.list_files();
    assert!(result.is_err());
}

/// R0001-0084: exercise decryption, not just password storage. This
/// used to open the UNENCRYPTED `test.zip` with an arbitrary password
/// and assert only the stored path, so an encrypted-open regression
/// could not fail it. `test_encrypted.zip` is a ZipCrypto archive with
/// readable headers, password `test123` (see `tests/fixtures/README.md`
/// and `tests/review_0068_test.rs`), holding a copy of `test_file.txt`.
#[test]
fn test_zip_wrapper_open_with_password() {
    let path = fixture("test_encrypted.zip");
    let expected = std::fs::read(fixture("test_file.txt")).unwrap();

    let archive = ZipArchive::open_with_password(&path, "test123").unwrap();
    assert_eq!(archive.path(), path);
    assert_eq!(
        archive.extract_to_memory("test_file.txt").unwrap(),
        expected,
        "the correct password must yield the entry plaintext"
    );

    // Headers are not encrypted: listing works without a password and
    // reports the entry as encrypted (AD 0014 defers the password check
    // to extraction).
    let entries = archive.list_files().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == "test_file.txt" && e.is_encrypted),
        "the entry must be listed and flagged encrypted"
    );

    // No password at all: a missing credential, not a malformed
    // archive (ticgit `9bdf2c`). This must be `Password` so a caller can
    // tell "ask the user for a password" from "this archive is broken"
    // without string-matching, and so ZIP answers the way RAR
    // (`ERAR_MISSING_PASSWORD`) and 7z (`Error::PasswordRequired`)
    // already do.
    let no_password = ZipArchive::open(&path).unwrap();
    match no_password.extract_to_memory("test_file.txt") {
        Err(ArchiveError::Password { message }) => assert!(
            message.contains("Password required"),
            "the missing-credential message must say so: {message}"
        ),
        other => {
            panic!("a missing password must surface as ArchiveError::Password, got {other:?}")
        }
    }

    // Wrong password: ZipCrypto's check byte rejects nearly every wrong
    // key (`Password`); on a check-byte collision the decrypted bytes
    // still fail the central-directory CRC32 (`Corruption`). Neither
    // path may return data.
    let wrong = ZipArchive::open_with_password(&path, "not-the-password").unwrap();
    match wrong.extract_to_memory("test_file.txt") {
        Err(ArchiveError::Password { .. }) | Err(ArchiveError::Corruption { .. }) => {}
        other => panic!("wrong password must not yield plaintext, got {other:?}"),
    }
}

#[test]
fn test_zip_wrapper_list_files() {
    let path = fixture("test.zip");
    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files();
    assert!(entries.is_ok());
    let entries = entries.unwrap();
    assert!(!entries.is_empty());

    for entry in entries.iter() {
        assert!(!entry.path.is_empty());
        // zip crate provides CRC32 from metadata
        if entry.entry_type == EntryType::File {
            assert!(
                entry.crc32.is_some(),
                "File entry '{}' should have CRC32 from zip metadata",
                entry.path
            );
        }
    }
}

#[test]
fn test_zip_wrapper_extract_to_memory() {
    let path = fixture("test.zip");
    let archive = ZipArchive::open(&path).unwrap();
    let data = archive.extract_to_memory("test_file.txt");
    assert!(data.is_ok());
    assert!(!data.unwrap().is_empty());
}

#[test]
fn test_zip_wrapper_extract_to_memory_nonexistent() {
    let path = fixture("test.zip");
    let archive = ZipArchive::open(&path).unwrap();
    let result = archive.extract_to_memory("does_not_exist.txt");
    assert!(result.is_err());
}

/// R0076-0048: an entry stored with backslash separators is listed
/// under the normalized (`/`) name; by-name opening must find it by
/// that listed name instead of requiring the raw stored spelling.
#[test]
fn test_zip_wrapper_backslash_entry_opens_by_listed_name() {
    use std::io::Write as _;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("backslash.zip");
    let payload = b"backslash payload";

    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("a\\b.txt", options).unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "a/b.txt", "listing must normalize");

    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("listed (normalized) name must resolve the stored entry");
    assert_eq!(data, payload);
}

/// OI-0080-003: build a ZIP with many tiny stored entries for the
/// parse-time entry-count budget tests.
fn build_many_entry_zip(path: &Path, count: usize) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..count {
        writer
            .start_file(format!("entry_{i}.txt"), options)
            .unwrap();
        writer.write_all(b"x").unwrap();
    }
    writer.finish().unwrap();
}

/// OI-0080-003: an over-budget listing aborts with the parse-time
/// `OperationBlocked` shape, and the aborted parse does not poison the
/// backend cache — a later unbudgeted call still materializes.
#[test]
fn test_zip_list_files_budget_aborts_over_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("many.zip");
    build_many_entry_zip(&path, 50);

    let archive = ZipArchive::open(&path).unwrap();
    let err = archive.list_files_budgeted(Some(10)).unwrap_err();
    match err {
        ArchiveError::OperationBlocked { reason, .. } => assert!(
            reason.contains("parsed more than 10 entries"),
            "unexpected reason: {reason}"
        ),
        other => panic!("expected OperationBlocked, got {other:?}"),
    }

    // No cache poisoning: the aborted budgeted parse left `listing` empty,
    // so a later unbudgeted parse on the same handle still succeeds.
    let entries = archive.list_files_budgeted(None).unwrap();
    assert_eq!(entries.len(), 50);
}

/// OI-0080-003: an unbudgeted listing (`budget = None`) succeeds.
#[test]
fn test_zip_list_files_budget_none_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("many.zip");
    build_many_entry_zip(&path, 50);

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files_budgeted(None).unwrap();
    assert_eq!(entries.len(), 50);
}

/// OI-0080-003 / AD 0065 (documented behavior): once the listing is
/// materialized, a budgeted call hits the cache and returns the cached
/// `Arc` unchanged — the budget applies only to the first parse.
#[test]
fn test_zip_list_files_budget_ignored_on_cache_hit() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("many.zip");
    build_many_entry_zip(&path, 50);

    let archive = ZipArchive::open(&path).unwrap();
    // Populate the cache unbudgeted.
    assert_eq!(archive.list_files_budgeted(None).unwrap().len(), 50);
    // Cache hit: the smaller budget is ignored, cached 50 entries returned.
    assert_eq!(archive.list_files_budgeted(Some(10)).unwrap().len(), 50);
}

#[test]
fn test_zip_wrapper_extract_to_stream() {
    use std::io::Read;
    let path = fixture("test.zip");
    let archive = ZipArchive::open(&path).unwrap();
    let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    assert!(!buf.is_empty());
}

/// Build an AES-encrypted single-entry ZIP. The zip crate writes the
/// AE-2 variant (stored CRC32 = 0) for payloads under 20 bytes and
/// AE-1 (real stored CRC32) otherwise.
fn build_aes_zip(path: &Path, payload: &[u8], password: &str) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .with_aes_encryption(zip::AesMode::Aes256, password);
    writer.start_file("secret.txt", options).unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();
}

/// R0079-0007: AE-2 entries store CRC32 = 0 in the central directory,
/// so the wrapper-level CRC comparison must be skipped — integrity is
/// covered by the AES authentication tag instead.
#[test]
fn test_zip_wrapper_ae2_encrypted_not_flagged_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae2.zip");
    let payload = b"tiny"; // < 20 bytes => AE-2 (placeholder CRC 0)
    build_aes_zip(&path, payload, "pw");

    let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
    let data = archive.extract_to_memory("secret.txt").unwrap();
    assert_eq!(data, payload);

    let failed = archive.test_integrity().unwrap();
    assert!(
        failed.is_empty(),
        "AE-2 entry falsely reported corrupt: {:?}",
        failed
    );

    let dest = tmp.path().join("out");
    archive
        .extract_all_with_options(&dest, None, true, true, true, true, None)
        .unwrap();
    assert_eq!(std::fs::read(dest.join("secret.txt")).unwrap(), payload);
}

/// AE-1 entries (>= 20 bytes) keep a real stored CRC32, so the
/// wrapper-level verification must still run for them.
#[test]
fn test_zip_wrapper_ae1_encrypted_crc_still_verified() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae1.zip");
    let payload = b"payload long enough for AE-1";
    build_aes_zip(&path, payload, "pw");

    let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
    let data = archive.extract_to_memory("secret.txt").unwrap();
    assert_eq!(data, payload);
    assert!(archive.test_integrity().unwrap().is_empty());
}

/// Build a single-entry, unencrypted, stored ZIP.
fn build_plain_zip(path: &Path, name: &str, payload: &[u8]) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file(name, options).unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();
}

/// DCR-012: an AE-2 entry's central-directory CRC32 is the
/// specification's placeholder 0, not a checksum, so the listing must
/// report `None`. Reporting `Some(0)` made every AE-2 entry fold the
/// same constant into the content-multiset digest, so two AES ZIPs with
/// different contents collided.
#[test]
fn test_zip_wrapper_ae2_entry_lists_crc32_none() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae2-listing.zip");
    let payload = b"tiny"; // < 20 bytes => AE-2 (placeholder CRC 0)
    build_aes_zip(&path, payload, "pw");

    let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "secret.txt")
        .expect("entry listed");
    assert_eq!(entry.entry_type, EntryType::File);
    assert!(entry.is_encrypted, "AE-2 entry must list as encrypted");
    assert_eq!(
        entry.crc32, None,
        "AE-2 placeholder CRC must not be listed as a checksum"
    );
}

/// The DCR-012 gate keys on the AE-2 *placeholder*, not on encryption:
/// an AE-1 entry (>= 20 bytes) carries a real stored CRC32, so it must
/// still be listed.
#[test]
fn test_zip_wrapper_ae1_entry_still_lists_stored_crc32() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae1-listing.zip");
    let payload = b"payload long enough for AE-1";
    build_aes_zip(&path, payload, "pw");

    let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "secret.txt")
        .expect("entry listed");
    assert!(entry.is_encrypted);
    assert_eq!(entry.crc32, Some(crc32fast::hash(payload)));
}

/// AD 0012 anti-regression: a CRC32 *value* of 0 is a valid checksum —
/// `CRC32(b"") == 0` — so a plaintext empty file must keep listing
/// `Some(0)`. This test fails if anyone re-widens the DCR-012 gate from
/// `encrypted() && crc == 0` to `crc == 0`.
#[test]
fn test_zip_wrapper_plaintext_empty_file_still_lists_crc32_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("empty.zip");
    build_plain_zip(&path, "empty.txt", b"");

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "empty.txt")
        .expect("entry listed");
    assert!(!entry.is_encrypted);
    assert_eq!(
        entry.crc32,
        Some(0),
        "AD 0012: CRC32 value 0 is a valid checksum, not an absent one"
    );
}

/// Build a ZIP holding one Stored, zero-byte entry that is flagged
/// **encrypted** but carries **no** AES extra field — the shape a
/// legacy ZipCrypto empty file has, and the exact case the
/// pre-narrowing gate `encrypted() && crc == 0` swept in:
/// `CRC32(b"") == 0` is a *real* stored checksum here, not the AE-2
/// placeholder.
///
/// The `zip` crate cannot write one directly
/// (`with_deprecated_encryption` is crate-private), so the fixture is
/// a crate-written plaintext empty file with general-purpose flag bit
/// 0 set in both the local and the central header. Building it by
/// mutating a well-formed archive rather than hand-assembling one
/// keeps every other field exactly as the writer emitted it, so a
/// parse failure here would be about the flag, not about the fixture.
///
/// The payload bytes a real ZipCrypto entry carries (a 12-byte
/// encryption header) are absent; nothing in this test reads entry
/// data, and `list_files` reads the central directory only.
fn build_encrypted_empty_zip_without_aes_field(path: &Path, name: &str) {
    const FLAG_OFFSET_IN_LOCAL_HEADER: usize = 6;
    const FLAG_OFFSET_IN_CENTRAL_HEADER: usize = 8;
    const LOCAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
    const CENTRAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];

    build_plain_zip(path, name, b"");
    let mut bytes = std::fs::read(path).unwrap();

    // Single-entry archive with an empty payload, so the first
    // occurrence of each signature is the header we want.
    let set_encrypted_flag = |bytes: &mut Vec<u8>, signature: [u8; 4], flag_offset: usize| {
        let at = bytes
            .windows(4)
            .position(|w| w == signature)
            .expect("signature present in a crate-written archive");
        let flag_at = at + flag_offset;
        bytes[flag_at] |= 0x01;
    };
    set_encrypted_flag(&mut bytes, LOCAL_SIGNATURE, FLAG_OFFSET_IN_LOCAL_HEADER);
    set_encrypted_flag(&mut bytes, CENTRAL_SIGNATURE, FLAG_OFFSET_IN_CENTRAL_HEADER);

    std::fs::write(path, &bytes).unwrap();
}

/// AD-0012 anti-regression for the *narrowed* gate. The pre-narrowing
/// predicate `encrypted() && crc == 0` was a superset of AE-2: an
/// encrypted entry whose payload is genuinely empty carries a real
/// stored `CRC32(b"") == 0`, and exempting it discarded a checksum the
/// archive did carry (and, post-DCR-012, forced a pointless
/// decrypt-and-stream during the digest walk). Requiring the `0x9901`
/// vendor version to be AE-2 keeps it listed as `Some(0)`.
#[test]
fn test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("zipcrypto-empty.zip");
    build_encrypted_empty_zip_without_aes_field(&path, "empty.txt");

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "empty.txt")
        .expect("entry listed");
    assert!(
        entry.is_encrypted,
        "the fixture must actually carry the encrypted flag, \
         otherwise it does not exercise the gate"
    );
    assert_eq!(
        entry.crc32,
        Some(0),
        "an encrypted empty file's zero CRC is a real checksum (AD 0012), \
         not the AE-2 placeholder — the gate must not sweep it in"
    );
}

/// The `0x9901` parser reads the vendor version out of an extra-field
/// chain, tolerates a preceding field, and refuses malformed input
/// rather than guessing. Exercised through a real AE-1 / AE-2 archive
/// below; this pins the byte-level behaviour the gate depends on.
#[test]
fn test_zip_wrapper_aes_vendor_version_reads_ae1_and_ae2() {
    let tmp = tempfile::tempdir().unwrap();

    // AE-2: the zip writer picks it for payloads under 20 bytes.
    let ae2 = tmp.path().join("ae2-vendor.zip");
    build_aes_zip(&ae2, b"tiny", "pw");
    let ae2_archive = ZipArchive::open(&ae2).unwrap();
    let ae2_entry = ae2_archive
        .list_files()
        .unwrap()
        .iter()
        .find(|e| e.path == "secret.txt")
        .cloned()
        .expect("entry listed");
    assert_eq!(
        ae2_entry.crc32, None,
        "AE-2 placeholder CRC must not be listed as a checksum"
    );

    // AE-1: 20 bytes or more, so the writer keeps the real CRC32.
    let ae1 = tmp.path().join("ae1-vendor.zip");
    let payload = b"payload long enough for AE-1";
    build_aes_zip(&ae1, payload, "pw");
    let ae1_archive = ZipArchive::open(&ae1).unwrap();
    let ae1_entry = ae1_archive
        .list_files()
        .unwrap()
        .iter()
        .find(|e| e.path == "secret.txt")
        .cloned()
        .expect("entry listed");
    assert!(ae1_entry.is_encrypted);
    assert_eq!(
        ae1_entry.crc32,
        Some(crc32fast::hash(payload)),
        "AE-1 stores a real CRC32 and must keep it"
    );
}

/// DCR-012: with the listing reporting `None`, the digest walk streams
/// AE-2 entries by listing id. The id-addressed path must return the
/// decrypted payload (and must not trip the AE-2 CRC compare), because
/// it is what keeps a duplicate-name AES ZIP digestible at all
/// (OI-0076-002).
#[test]
fn test_zip_wrapper_extract_to_stream_by_listing_id_ae2() {
    use std::io::Read as _;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae2-by-id.zip");
    let payload = b"tiny"; // AE-2
    build_aes_zip(&path, payload, "pw");

    let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "secret.txt")
        .expect("entry listed");

    let mut stream = archive
        .extract_to_stream_by_listing_id(entry.id, &entry.path)
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, payload);

    // The drift guard still binds: a mismatched validated name is refused.
    assert!(
        archive
            .extract_to_stream_by_listing_id(entry.id, "other.txt")
            .is_err()
    );
}

/// Without a password the AE-2 payload cannot be read, so the
/// id-addressed stream the digest walk uses must fail rather than
/// substitute a value. Listing itself keeps working (AD 0014).
///
/// The variant is asserted: TicGit `9bdf2c` moved the ZIP no-password
/// read path from `ArchiveError::Format` to `ArchiveError::Password`, and
/// this is the path DCR-012 made user-visible — the content digest
/// streams AE-2 entries, so a digest call on a password-protected ZIP
/// lands here.
#[test]
fn test_zip_wrapper_ae2_stream_by_id_without_password_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ae2-nopw.zip");
    build_aes_zip(&path, b"tiny", "pw");

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "secret.txt")
        .expect("listing an encrypted ZIP needs no password (AD 0014)");
    assert_eq!(entry.crc32, None);

    match archive.extract_to_stream_by_listing_id(entry.id, &entry.path) {
        Err(ArchiveError::Password { message }) => assert!(
            message.contains("Password required"),
            "the missing-credential message must say so: {message}"
        ),
        Ok(_) => panic!("an AE-2 entry must not be digested without the password"),
        Err(e) => {
            panic!("the refusal must be a missing credential, not a format fault; got {e:?}")
        }
    }
}

/// R0079-0019: `preserve_permissions` / `preserve_times` must be
/// honoured by the zip-crate extract paths — a 0o755 entry keeps
/// its exec bit and archive mtime when the flags are set, and gets
/// neither when they are cleared.
#[test]
#[cfg(unix)]
fn test_zip_wrapper_extract_preserves_mode_and_mtime_per_flags() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mode.zip");
    crate::test_utils::build_zip_with_mode_and_mtime(&path);
    let expected_mtime = crate::ffi::common::ymd_hms_to_system_time(2010, 1, 2, 3, 4, 6).unwrap();

    let archive = ZipArchive::open(&path).unwrap();

    let preserved = tmp.path().join("preserved");
    archive
        .extract_all_with_options(&preserved, None, true, true, true, false, None)
        .unwrap();
    let meta = std::fs::metadata(preserved.join("tool.sh")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    assert_eq!(meta.modified().unwrap(), expected_mtime);

    let plain = tmp.path().join("plain");
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

    // Single-file path honours the same flags.
    let single = tmp.path().join("single");
    archive
        .extract_file_with_options_preserve("tool.sh", &single, true, true, true, false)
        .unwrap();
    let meta = std::fs::metadata(single.join("tool.sh")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
    assert_eq!(meta.modified().unwrap(), expected_mtime);
}

#[test]
fn test_zip_wrapper_memory_and_stream_same_data() {
    use std::io::Read;
    let path = fixture("test.zip");

    let archive1 = ZipArchive::open(&path).unwrap();
    let mem_data = archive1.extract_to_memory("test_file.txt").unwrap();

    let archive2 = ZipArchive::open(&path).unwrap();
    let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
    let mut stream_data = Vec::new();
    stream.read_to_end(&mut stream_data).unwrap();

    assert_eq!(mem_data, stream_data);
}

/// R0080-0030: a corrupt regular-file payload must be recorded in the
/// failed list, not abort `test_integrity` with an error. Only genuine
/// archive-file I/O errors propagate.
#[test]
fn test_zip_wrapper_integrity_records_corrupt_payload() {
    use std::io::Write as _;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("corrupt.zip");
    let payload = b"unified-archive integrity payload marker bytes";

    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("data.txt", options).unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();

    // Flip one stored-payload byte on disk so its CRC no longer matches
    // the central-directory value, leaving all framing intact.
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes
        .windows(payload.len())
        .position(|w| w == &payload[..])
        .expect("stored payload present in archive");
    bytes[at] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();

    let archive = ZipArchive::open(&path).unwrap();
    let failed = archive
        .test_integrity()
        .expect("corrupt payload must be recorded, not abort the walk");
    assert_eq!(failed, vec!["data.txt".to_string()]);
}

/// R0081-0076 / R0081-0077: the shared classifier maps every Unix
/// `S_IFMT` type to a single, stable `EntryType`, so both ZIP backends
/// agree. Verified directly over raw mode bits (no archive needed).
#[test]
fn test_classify_zip_entry_type_over_mode_bits() {
    // With a Unix mode present, the type nibble is authoritative.
    assert_eq!(
        classify_zip_entry_type(Some(0o100644), false),
        EntryType::File
    );
    assert_eq!(
        classify_zip_entry_type(Some(0o040755), true),
        EntryType::Directory
    );
    assert_eq!(
        classify_zip_entry_type(Some(0o120777), false),
        EntryType::Symlink
    );
    // Symlink is decided before the directory-style name (ordering kept).
    assert_eq!(
        classify_zip_entry_type(Some(0o120777), true),
        EntryType::Symlink
    );
    // Every remaining Unix type -> Other (FIFO, char/block device,
    // socket, and any other non-standard nibble).
    for special in [0o010644u32, 0o020644, 0o060644, 0o140644, 0o160644] {
        assert_eq!(
            classify_zip_entry_type(Some(special), false),
            EntryType::Other,
            "mode {:o} must classify as Other",
            special
        );
    }
    // A mode with no type nibble falls back to the directory-name hint.
    assert_eq!(
        classify_zip_entry_type(Some(0o000644), false),
        EntryType::File
    );
    assert_eq!(
        classify_zip_entry_type(Some(0o000644), true),
        EntryType::Directory
    );
    // No Unix mode at all: name hint only.
    assert_eq!(classify_zip_entry_type(None, false), EntryType::File);
    assert_eq!(classify_zip_entry_type(None, true), EntryType::Directory);
}

/// Build a single-entry ZIP ("special") whose central-directory Unix
/// mode carries the `S_IFIFO` type nibble — a special (non-regular,
/// non-symlink, non-directory) entry both ZIP backends must classify as
/// `EntryType::Other`. `unix_permissions` masks to the low 12 bits, so
/// the type nibble is injected by patching the external-attributes field
/// of the central-directory header directly (no header is checksummed).
fn build_zip_with_special_mode_entry(path: &Path) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);
    writer.start_file("special", options).unwrap();
    writer.write_all(b"x").unwrap();
    writer.finish().unwrap();

    // Locate the central-directory header (signature "PK\x01\x02"). Its
    // 4-byte external-file-attributes field is at CDH offset 38, and the
    // Unix mode lives in the high 16 bits, so the byte carrying the
    // `S_IFMT` type nibble sits at CDH offset 41. Turning its high nibble
    // from 0 (mode 0o644 has no type bits) into 1 yields S_IFIFO.
    let mut bytes = std::fs::read(path).unwrap();
    let cdh = bytes
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .expect("central-directory header present");
    bytes[cdh + 41] |= 0x10;
    std::fs::write(path, &bytes).unwrap();
}

/// R0081-0076 / R0081-0077: a special-mode entry must be classified
/// `EntryType::Other` and materialized by neither listing nor
/// extraction. (Pre-AD-0007 this asserted parity across the two ZIP
/// backends; the `zip` crate is now the sole ZIP backend.)
#[test]
fn test_zip_special_mode_entry_classified_other() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("special.zip");
    build_zip_with_special_mode_entry(&path);

    let zip = ZipArchive::open(&path).unwrap();
    let zip_entries = zip.list_files().unwrap();
    assert_eq!(zip_entries.len(), 1);
    assert_eq!(
        zip_entries[0].entry_type,
        EntryType::Other,
        "zip backend must classify a special-mode entry as Other"
    );

    // The special entry must not be materialized on extract_all.
    let zip_dest = tmp.path().join("zip_out");
    zip.extract_all(&zip_dest, None).unwrap();
    assert!(
        !zip_dest.join("special").exists(),
        "zip backend must not materialize a special-mode entry"
    );
}

// ── raw-central-directory fixtures (R0001-0027 / R0001-0028 /
//    R0001-0029 / R0001-0031 / R0001-0032) ──
//
// Entry names and the EOCD counters carry no checksum, so a fixture
// can be patched in place after `ZipWriter::finish` — the R0079-0026
// technique the integration parity suite already uses.

/// Build a stored-only ZIP with one entry per name. Payloads are
/// name-independent so byte-patching a name never disturbs a CRC.
fn build_stored_zip(path: &Path, names: &[&str]) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (i, name) in names.iter().enumerate() {
        writer.start_file(*name, options).unwrap();
        writer.write_all(format!("payload-{i}").as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

fn le_u16(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Offset of the EOCD record (these fixtures carry no archive comment).
fn eocd_offset(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
        .expect("EOCD present")
}

/// Byte offsets of every central-directory record, in stored order.
fn central_record_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut at = le_u32(bytes, eocd_offset(bytes) + 16) as usize;
    let mut offsets = Vec::new();
    while at + 46 <= bytes.len() && bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
        offsets.push(at);
        at += 46 + le_u16(bytes, at + 28) + le_u16(bytes, at + 30) + le_u16(bytes, at + 32);
    }
    offsets
}

/// Rewrite both EOCD entry counters so the `zip` crate parses only the
/// first `count` central-directory records while the file still
/// physically carries more.
fn set_eocd_entry_count(bytes: &mut [u8], count: u16) {
    let eocd = eocd_offset(bytes);
    bytes[eocd + 8..eocd + 10].copy_from_slice(&count.to_le_bytes());
    bytes[eocd + 10..eocd + 12].copy_from_slice(&count.to_le_bytes());
}

/// Rename every occurrence of `from` to `to` — both the local and the
/// central header. Lengths must match so no offset moves.
fn rename_entry_bytes(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len(), "in-place rename needs equal lengths");
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
}

/// R0001-0029: a duplicated non-UTF-8 name must be refused under the
/// path the *listing* exposes. The scan used to tally raw names through
/// `String::from_utf8_lossy`, recording a replacement-character key that
/// the crate's CP437-decoded listing name can never equal — so the
/// duplicate was detected and then never matched by `reject_if_duplicate`.
#[test]
fn test_zip_duplicate_non_utf8_name_refused_under_listed_path() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("dup_cp437.zip");
    build_stored_zip(&path, &["dup0.txt", "dup1.txt"]);

    // The entries are written with ASCII names, so the general-purpose
    // UTF-8 flag stays clear and the crate decodes the patched bytes as
    // CP437. 0xE9 is not valid UTF-8.
    let mut bytes = std::fs::read(&path).unwrap();
    rename_entry_bytes(&mut bytes, b"dup0.txt", b"d\xE9p0.txt");
    rename_entry_bytes(&mut bytes, b"dup1.txt", b"d\xE9p0.txt");
    std::fs::write(&path, &bytes).unwrap();

    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 1, "the zip crate collapses the duplicate");

    let listed = entries[0].path.clone();
    assert!(
        !listed.contains('\u{FFFD}'),
        "listed path is the crate's CP437 decode, not a lossy one: {listed}"
    );
    match archive.extract_to_memory(&listed) {
        Err(ArchiveError::OperationBlocked { .. }) => {}
        other => panic!("duplicated non-UTF-8 name must be refused, got {other:?}"),
    }
}

/// R0001-0028: an attributable duplicate must not disarm the
/// unexplained-surplus refusal. The guard used to require
/// `names.is_empty()`, so one attributed duplicate re-enabled every
/// other by-name extraction even when the raw directory carried records
/// the `zip` crate never accounted for.
#[test]
fn test_zip_duplicate_surplus_not_masked_by_attributed_duplicate() {
    let tmp = tempfile::tempdir().unwrap();

    // Control: the surplus is fully attributed to `dup0.txt`, so only
    // that name is refused and unrelated entries stay extractable.
    let plain = tmp.path().join("dup_only.zip");
    build_stored_zip(&plain, &["a.txt", "dup0.txt", "dup1.txt", "z.txt"]);
    let mut bytes = std::fs::read(&plain).unwrap();
    rename_entry_bytes(&mut bytes, b"dup1.txt", b"dup0.txt");
    std::fs::write(&plain, &bytes).unwrap();

    let archive = ZipArchive::open(&plain).unwrap();
    archive
        .extract_to_memory("a.txt")
        .expect("a fully attributed surplus must not refuse unrelated entries");
    match archive.extract_to_memory("dup0.txt") {
        Err(ArchiveError::OperationBlocked { .. }) => {}
        other => panic!("duplicated name must be refused, got {other:?}"),
    }

    // Same bytes, but the EOCD undercounts the directory by one: a
    // fourth raw record exists that the crate never parsed. `dup0.txt`
    // is still attributable, yet one collapse is not — the whole
    // by-name single-entry surface must now be refused.
    let masked = tmp.path().join("dup_plus_surplus.zip");
    let mut bytes = std::fs::read(&plain).unwrap();
    set_eocd_entry_count(&mut bytes, 3);
    std::fs::write(&masked, &bytes).unwrap();

    let archive = ZipArchive::open(&masked).unwrap();
    match archive.extract_to_memory("a.txt") {
        Err(ArchiveError::OperationBlocked { .. }) => {}
        other => {
            panic!("an unattributed collapse must refuse every by-name op, got {other:?}")
        }
    }
}

/// R0001-0027: a malformed or truncated central directory must fail
/// closed. The scan used to `break` on any read error or unknown
/// signature, which silently truncated the tally — and a tally that
/// stops early *disables* duplicate detection for everything after it.
#[test]
fn test_zip_duplicate_scan_rejects_malformed_central_directory() {
    let tmp = tempfile::tempdir().unwrap();

    // (a) An unknown signature where the walk expects another header or
    //     a directory terminator.
    let bogus = tmp.path().join("bogus_signature.zip");
    build_stored_zip(&bogus, &["a.txt", "b.txt"]);
    let mut bytes = std::fs::read(&bogus).unwrap();
    let second = central_record_offsets(&bytes)[1];
    set_eocd_entry_count(&mut bytes, 1); // keep the crate's own parse valid
    bytes[second..second + 4].copy_from_slice(&[0x50, 0x4b, 0x77, 0x77]);
    std::fs::write(&bogus, &bytes).unwrap();

    let archive = ZipArchive::open(&bogus).unwrap();
    match archive.extract_to_memory("a.txt") {
        Err(ArchiveError::Corruption { .. }) => {}
        other => panic!("unknown record signature must fail closed, got {other:?}"),
    }

    // (b) A record whose declared name length runs past the end of file.
    let short = tmp.path().join("short_record.zip");
    build_stored_zip(&short, &["a.txt", "b.txt"]);
    let mut bytes = std::fs::read(&short).unwrap();
    let second = central_record_offsets(&bytes)[1];
    set_eocd_entry_count(&mut bytes, 1);
    bytes[second + 28..second + 30].copy_from_slice(&u16::MAX.to_le_bytes());
    std::fs::write(&short, &bytes).unwrap();

    let archive = ZipArchive::open(&short).unwrap();
    match archive.extract_to_memory("a.txt") {
        Err(ArchiveError::Corruption { .. }) => {}
        other => panic!("a record running past EOF must fail closed, got {other:?}"),
    }
}

/// OI-0001-003 half one (R0001-0026): the raw index must be read
/// through the descriptor the cached handle already owns, never by
/// re-opening `self.path`.
///
/// The fixture makes the two sources disagree on purpose: a CLEAN
/// archive is opened and its handle materialised, then a genuinely
/// AMBIGUOUS archive is moved onto the same path (a new inode, so the
/// open descriptor keeps pointing at the clean bytes). A path-based
/// re-scan would read the ambiguous replacement and refuse operations on
/// the archive the extractor is actually reading — the "guard blesses one
/// file while extraction reads another" defect, observed from its safe
/// side. Reading one source means the verdict follows the descriptor.
#[test]
fn test_zip_raw_index_reads_the_cached_handle_not_the_path() {
    let tmp = tempfile::tempdir().unwrap();

    // The replacement, and proof that it really is ambiguous when a scan
    // reads *it* — otherwise this test could pass vacuously.
    let ambiguous = tmp.path().join("ambiguous.zip");
    build_stored_zip(&ambiguous, &["a.txt", "b.txt"]);
    let mut bytes = std::fs::read(&ambiguous).unwrap();
    rename_entry_bytes(&mut bytes, b"b.txt", b"a.txt");
    std::fs::write(&ambiguous, &bytes).unwrap();
    match ZipArchive::open(&ambiguous).unwrap().test_integrity() {
        Err(ArchiveError::OperationBlocked { .. }) => {}
        other => panic!("the replacement fixture must be ambiguous, got {other:?}"),
    }

    let path = tmp.path().join("live.zip");
    build_stored_zip(&path, &["a.txt", "b.txt"]);
    let archive = ZipArchive::open(&path).unwrap();
    // Materialise the cached handle WITHOUT building the index — every
    // public entry point now consults the index, so warming it here
    // would memoise the verdict before the swap and prove nothing.
    archive.with_zip(|_| Ok(())).unwrap();

    // Swap in a new inode at the same path.
    let staged = tmp.path().join("staged.zip");
    std::fs::copy(&ambiguous, &staged).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&staged, &path).unwrap();

    let raw = archive
        .raw_central_directory()
        .expect("the index must come from the open descriptor");
    assert_eq!(
        raw.raw_len(),
        2,
        "the scan must see the clean archive's two records, not the replacement's"
    );
    assert!(
        !raw.is_collapsed(),
        "the descriptor's archive is unambiguous: {}",
        raw.collapsed_reason()
    );
    assert!(
        archive.test_integrity().unwrap().is_empty(),
        "the archive the extractor reads is healthy, so integrity must pass"
    );
    assert_eq!(
        archive.extract_to_memory("a.txt").unwrap(),
        b"payload-0",
        "the guard and the payload must come from the same source"
    );
}

/// OI-0001-003 half two (R0001-0030): a collapsed duplicate must be
/// refused through the paths that consume the whole archive, not only
/// through the by-name single-entry route.
///
/// Bulk extraction, the integrity walk and the id-addressed stream all
/// iterate the `zip` crate's deduped view, so before this they reported
/// success over a subset of the records the file carries: a
/// complete-looking extraction that dropped the shadowed payload, an
/// "all entries pass" integrity answer that never read it, and a content
/// digest that never folded it in. None of them takes a name, so none of
/// them could reach `reject_if_duplicate`.
#[test]
fn test_zip_collapsed_duplicate_refused_outside_the_by_name_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("dup_bulk.zip");
    build_stored_zip(&path, &["a.txt", "dup0.txt", "dup1.txt"]);
    let mut bytes = std::fs::read(&path).unwrap();
    rename_entry_bytes(&mut bytes, b"dup1.txt", b"dup0.txt");
    std::fs::write(&path, &bytes).unwrap();

    let archive = ZipArchive::open(&path).unwrap();
    // The listing is the crate's deduped view — three raw records, two
    // addressable entries. It still lists: the ambiguity is attributable
    // to `dup0.txt`, so it is localizable by name.
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), 2, "the zip crate collapses the duplicate");

    let out = tmp.path().join("out");
    match archive.extract_all_with_options(&out, None, true, false, false, false, None) {
        Err(ArchiveError::OperationBlocked { operation, reason }) => {
            assert_eq!(operation, crate::error::ops::EXTRACT_ALL);
            assert!(
                reason.contains("dup0.txt") && reason.contains("3 raw record(s)"),
                "the refusal must name the ambiguity: {reason}"
            );
        }
        other => panic!("bulk extraction must refuse a collapsed archive, got {other:?}"),
    }
    assert!(
        !out.exists(),
        "a refused bulk extraction must not touch the destination"
    );

    match archive.test_integrity() {
        Err(ArchiveError::OperationBlocked { operation, .. }) => {
            assert_eq!(operation, crate::error::ops::VALIDATE_INTEGRITY);
        }
        other => panic!("the integrity walk must refuse a collapsed archive, got {other:?}"),
    }

    let unique = entries
        .iter()
        .find(|e| e.path == "a.txt")
        .expect("the unambiguous entry must be listed");
    match archive.extract_to_stream_by_listing_id(unique.id, &unique.path) {
        Err(ArchiveError::OperationBlocked { operation, .. }) => {
            assert_eq!(operation, crate::error::ops::EXTRACT_TO_STREAM);
        }
        Ok(_) => panic!("the id-addressed stream must refuse a collapsed archive"),
        Err(e) => panic!("the id-addressed stream must refuse a collapse, got {e:?}"),
    }

    // The by-name surface keeps its per-name behaviour: the unambiguous
    // entry still extracts, the ambiguous name is still refused.
    assert_eq!(archive.extract_to_memory("a.txt").unwrap(), b"payload-0");
    match archive.extract_to_memory("dup0.txt") {
        Err(ArchiveError::OperationBlocked { reason, .. }) => assert!(
            reason.contains("Multiple entries match"),
            "the by-name refusal keeps its own wording: {reason}"
        ),
        other => panic!("the duplicated name must stay refused, got {other:?}"),
    }
}

/// R0001-0031 / R0001-0032: the central directory's uncompressed size is
/// authoritative. An entry that decodes short must not pass integrity
/// validation just because its stored CRC32 still matches the shortened
/// data, and it must not escape the memory path as a valid buffer.
#[test]
fn test_zip_short_decode_is_corruption_not_success() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("short_decode.zip");
    build_stored_zip(&path, &["a.txt"]);

    // Inflate only the central directory's declared uncompressed size.
    // A stored entry copies `compressed_size` bytes through untouched,
    // so the payload and its CRC32 stay self-consistent — exactly the
    // case a checksum-only comparison cannot see.
    let mut bytes = std::fs::read(&path).unwrap();
    let record = central_record_offsets(&bytes)[0];
    let declared = le_u32(&bytes, record + 24);
    bytes[record + 24..record + 28].copy_from_slice(&(declared + 5).to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();

    let archive = ZipArchive::open(&path).unwrap();
    assert_eq!(
        archive.test_integrity().unwrap(),
        vec!["a.txt".to_string()],
        "an entry that decodes fewer bytes than declared is not intact"
    );
    match archive.extract_to_memory("a.txt") {
        Err(ArchiveError::Corruption { .. }) => {}
        other => panic!("a short decode must not be returned as data, got {other:?}"),
    }
}

/// R0001-0062: extraction must restore the timestamp the listing
/// reports. Both now resolve the 0x5455 extended timestamp; extraction
/// used to fall back to the coarse 2-second DOS value, so the listed and
/// the restored mtime could disagree.
#[test]
fn test_zip_extraction_restores_extended_timestamp() {
    use std::io::Write as _;
    use std::time::{Duration, UNIX_EPOCH};

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ts.zip");

    // 0x5455 payload: flags byte (bit 0 = mtime present) followed by a
    // signed 32-bit Unix second count. An odd value is deliberate — the
    // DOS timestamp cannot represent it.
    let secs: i32 = 1_700_000_001;
    let mut extra = Vec::with_capacity(5);
    extra.push(0b0000_0001u8);
    extra.extend_from_slice(&secs.to_le_bytes());

    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let mut options = zip::write::FullFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    options
        .add_extra_data(0x5455, extra.into_boxed_slice(), false)
        .unwrap();
    writer.start_file("ts.txt", options).unwrap();
    writer.write_all(b"payload").unwrap();
    writer.finish().unwrap();

    let expected = UNIX_EPOCH + Duration::from_secs(secs as u64);
    let archive = ZipArchive::open(&path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(
        entries[0].modified,
        Some(expected),
        "listing must surface the 0x5455 timestamp"
    );

    let all_dest = tmp.path().join("all");
    archive
        .extract_all_with_options(&all_dest, None, true, true, true, false, None)
        .unwrap();
    assert_eq!(
        std::fs::metadata(all_dest.join("ts.txt"))
            .unwrap()
            .modified()
            .unwrap(),
        expected,
        "extract_all must restore the listed (0x5455) mtime"
    );

    let single_dest = tmp.path().join("single");
    archive
        .extract_file_with_options_preserve("ts.txt", &single_dest, true, true, true, false)
        .unwrap();
    assert_eq!(
        std::fs::metadata(single_dest.join("ts.txt"))
            .unwrap()
            .modified()
            .unwrap(),
        expected,
        "extract_file must restore the listed (0x5455) mtime"
    );
}
