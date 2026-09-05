use super::*;
use crate::options::WritableFormat;
use crate::test_utils::fixture;

// ── ArchiveMode tests ──

#[test]
fn test_archive_mode_debug() {
    assert_eq!(format!("{:?}", ArchiveMode::Read), "Read");
    assert_eq!(format!("{:?}", ArchiveMode::Write), "Write");
    assert_eq!(format!("{:?}", ArchiveMode::Modify), "Modify");
}

#[test]
fn test_archive_mode_clone() {
    let mode = ArchiveMode::Read;
    let cloned = mode;
    assert_eq!(mode, cloned);
}

#[test]
fn test_archive_mode_eq() {
    assert_eq!(ArchiveMode::Read, ArchiveMode::Read);
    assert_ne!(ArchiveMode::Read, ArchiveMode::Write);
    assert_ne!(ArchiveMode::Write, ArchiveMode::Modify);
}

// ── Archive::open tests ──

#[test]
fn test_open_valid_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::Zip);
    assert_eq!(archive.path(), fixture("test.zip"));
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_valid_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    // test.rar is actually RAR5 format (created by RAR 7.12+)
    assert!(matches!(
        archive.format(),
        ArchiveFormat::Rar | ArchiveFormat::Rar5
    ));
}

#[test]
fn test_open_valid_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
}

#[test]
fn test_open_valid_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::Tar);
}

#[test]
fn test_open_valid_tar_gz() {
    let archive = Archive::open(fixture("test.tar.gz")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

#[test]
fn test_open_valid_tar_bz2() {
    let archive = Archive::open(fixture("test.tar.bz2")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
}

#[test]
fn test_open_valid_tar_xz() {
    let archive = Archive::open(fixture("test.tar.xz")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::TarXz);
}

#[test]
fn test_open_nonexistent_file() {
    let result = Archive::open("/nonexistent/path/archive.zip");
    assert!(result.is_err());
}

#[test]
fn test_open_sets_read_mode() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert_eq!(archive.mode, ArchiveMode::Read);
}

#[test]
fn test_open_initializes_empty_cache() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(archive.entry_cache.get().is_none());
}

#[test]
fn test_open_has_no_modifications() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(archive.modifications.is_none());
}

// ── Archive::open_encrypted tests ──

/// R0001-0083: the old body asserted `result.is_ok() || result.is_err()`,
/// which holds for every `Result` and could not fail. `test_encrypted.rar`
/// is header-encrypted (its first RAR5 block is a `HEAD_CRYPT` record) and
/// its password is `test123` — the same one `tests/fixtures/README.md`
/// documents for the sibling `test_encrypted_data.rar`, not the `"test"`
/// the old body passed. UnRAR sets the password on the already-open handle
/// and defers validation to the first header read, so the contract is: the
/// open yields a RAR handle or fails specifically with
/// [`ArchiveError::Password`], and the listing either exposes the fixture
/// payload or refuses with that same password error — never another error
/// variant and never a payload-free listing. Gated on `rar-support` like
/// the other RAR unit tests: without the feature `open_encrypted` answers
/// `Unsupported`, which is a different contract.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_encrypted_rar() {
    let result = Archive::open_encrypted(fixture("test_encrypted.rar"), "test123");
    match result {
        Ok(archive) => {
            assert!(
                matches!(archive.format(), ArchiveFormat::Rar | ArchiveFormat::Rar5),
                "encrypted RAR must open as a RAR handle, got {:?}",
                archive.format()
            );
            match archive.list_files() {
                Ok(entries) => assert!(
                    entries.iter().any(|e| e.path.ends_with("test_file.txt")),
                    "decrypted listing must expose the fixture payload, got: {entries:?}"
                ),
                Err(err) => assert!(
                    matches!(err, crate::error::ArchiveError::Password { .. }),
                    "listing an encrypted RAR may only fail with a password error, got: {err:?}"
                ),
            }
        }
        Err(err) => assert!(
            matches!(err, crate::error::ArchiveError::Password { .. }),
            "expected a successful open or ArchiveError::Password, got: {err:?}"
        ),
    }
}

#[test]
fn test_open_encrypted_zip() {
    // Opening an unencrypted ZIP with a password should succeed (ZipReader backend)
    let result = Archive::open_encrypted(fixture("test.zip"), "password");
    assert!(
        result.is_ok(),
        "ZIP encrypted open should succeed: {:?}",
        result.err()
    );
    let archive = result.unwrap();
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[test]
fn test_open_encrypted_unsupported_tar() {
    let result = Archive::open_encrypted(fixture("test.tar"), "password");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(matches!(
        err,
        crate::error::ArchiveError::Unsupported { .. }
    ));
}

#[test]
fn test_open_encrypted_nonexistent() {
    let result = Archive::open_encrypted("/nonexistent/archive.rar", "password");
    assert!(result.is_err());
}

// ── Archive::format tests ──

#[test]
fn test_format_returns_detected_format() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

// ── Archive::path tests ──

#[test]
fn test_path_returns_archive_path() {
    let expected = fixture("test.zip");
    let archive = Archive::open(&expected).unwrap();
    assert_eq!(archive.path(), expected);
}

// ── Archive::is_encrypted tests ──

#[test]
fn test_is_encrypted_unencrypted_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(!archive.is_encrypted().unwrap());
}

/// A header-encrypted archive must **open** without a password.
///
/// `is_encrypted()` may then succeed or fail — the header cannot be read
/// without the password, so both `Ok(true)` and `Err(Password)` are correct.
/// But the `unwrap()` on `open` below is load-bearing, not incidental:
/// reporting that a file is encrypted is a legitimate thing to do without
/// knowing its password, and it is the only way a caller learns which
/// password to ask their user for.
///
/// ticgit 3f8790 broke exactly this and was caught here. Supplying the
/// password to `RAROpenArchiveEx` needs a callback registered in the open
/// data, and registering one *unconditionally* makes the SDK proceed as
/// though an empty password had been given when there is none to offer —
/// turning this open into `ERAR_MISSING_PASSWORD`. The fix registers at
/// open only when a password exists and installs the callback immediately
/// after otherwise; see `UnrarArchive::open_with_mode_and_password`.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_is_encrypted_encrypted_rar() {
    // test_encrypted.rar has header encryption - is_encrypted() may fail
    // with Password error since UnRAR can't read headers without a password.
    // Both Ok(true) and Err(Password) are valid outcomes.
    let archive = Archive::open(fixture("test_encrypted.rar")).unwrap();
    let result = archive.is_encrypted();
    match result {
        Ok(encrypted) => assert!(encrypted, "Should report as encrypted"),
        Err(ref e) => {
            let msg = format!("{}", e);
            assert!(
                msg.contains("assword") || msg.contains("encrypt"),
                "Error should be password-related, got: {}",
                msg
            );
        }
    }
}

// ── Archive::has_recovery_record tests ──

#[test]
fn test_has_recovery_record_zip_returns_false() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(!archive.has_recovery_record().unwrap());
}

#[test]
fn test_has_recovery_record_7z_returns_false() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    assert!(!archive.has_recovery_record().unwrap());
}

// ── Archive::recovery_percentage tests ──

#[test]
fn test_recovery_percentage_zip_returns_none() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert_eq!(archive.recovery_percentage().unwrap(), None);
}

#[test]
fn test_recovery_percentage_7z_returns_none() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    assert_eq!(archive.recovery_percentage().unwrap(), None);
}

// ── Archive::is_solid tests ──

#[test]
fn test_is_solid_zip_returns_false() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(!archive.is_solid().unwrap());
}

#[test]
fn test_is_solid_tar_returns_false() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    assert!(!archive.is_solid().unwrap());
}

/// R0079-0035: `Archive::modify` wraps every format in the Libarchive
/// backend, so `is_solid` must dispatch on the source format — a solid
/// 7z opened for modification previously fell into the always-`false`
/// backend arm.
#[test]
fn test_is_solid_7z_modify_mode_answers_truthfully() {
    use sevenz_rust2::{ArchiveEntry as SzEntry, ArchiveWriter, SourceReader};
    use std::io::Cursor;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solid.7z");

    // Two entries packed into one block (`push_archive_entries`) →
    // `num_unpack_sub_streams > 1` → solid per the sevenz-rust2 reader.
    let mut writer = ArchiveWriter::create(&path).unwrap();
    writer
        .push_archive_entries(
            vec![SzEntry::new_file("a.txt"), SzEntry::new_file("b.txt")],
            vec![
                SourceReader::new(Cursor::new(b"alpha".to_vec())),
                SourceReader::new(Cursor::new(b"bravo".to_vec())),
            ],
        )
        .unwrap();
    writer.finish().unwrap();

    assert!(
        Archive::open(&path).unwrap().is_solid().unwrap(),
        "read-mode baseline: the generated archive must be solid"
    );

    let archive = Archive::modify(&path).unwrap();
    assert_eq!(archive.mode, ArchiveMode::Modify);
    assert!(
        archive.is_solid().unwrap(),
        "Modify-mode 7z handle must report the source archive's solid flag"
    );
}

/// R0079-0035 companion: a genuinely non-solid 7z still answers `false`
/// through the Modify-mode read-side probe.
#[test]
fn test_is_solid_7z_modify_mode_non_solid_returns_false() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.7z");
    std::fs::copy(fixture("test.7z"), &path).unwrap();

    let archive = Archive::modify(&path).unwrap();
    assert!(!archive.is_solid().unwrap());
}

// ── Archive::open_at_offset tests ──

#[test]
fn test_open_at_offset_zero_is_plain_open() {
    let archive = Archive::open_at_offset(fixture("test.zip"), 0)
        .expect("offset=0 should be equivalent to Archive::open");
    assert_eq!(archive.format(), ArchiveFormat::Zip);
}

#[test]
fn test_open_at_offset_past_eof_rejected() {
    let len = std::fs::metadata(fixture("test.zip")).unwrap().len();
    let result = Archive::open_at_offset(fixture("test.zip"), len);
    assert!(result.is_err(), "offset == file_len must error");
}

// ── Archive::open_sfx tests ──

#[test]
fn test_open_sfx_non_sfx_file() {
    // A normal ZIP is not an SFX
    let result = Archive::open_sfx(fixture("test.zip"));
    assert!(result.is_err());
}

// ── Archive::extract_stub tests ──

#[test]
fn test_extract_stub_non_sfx_detection() {
    let detection = crate::sfx::SfxDetectionResult {
        is_sfx: false,
        archive_format: None,
        data_offset: None,
        stub_type: None,
        confidence: crate::sfx::SfxConfidence::NotSfx,
        evidence: Vec::new(),
        // R0001-0002: no detection open backs a hand-built result.
        source_identity: None,
    };
    let result = Archive::extract_stub(fixture("test.zip"), &detection);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("non-SFX"));
}

#[test]
fn test_extract_stub_missing_offset() {
    let detection = crate::sfx::SfxDetectionResult {
        is_sfx: true,
        archive_format: None,
        data_offset: None, // Missing offset
        stub_type: None,
        confidence: crate::sfx::SfxConfidence::Probable,
        evidence: Vec::new(),
        // R0001-0002: no detection open backs a hand-built result.
        source_identity: None,
    };
    let result = Archive::extract_stub(fixture("test.zip"), &detection);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("missing offset"));
}

#[test]
fn test_extract_stub_rejects_forged_detection() {
    // R0069-0007: with internal re-detection, a forged
    // `SfxDetectionResult` against a non-SFX file is rejected before
    // any read happens. The previous unit tests (`oversized_stub`,
    // `valid_small_offset`) trusted the caller's detection and
    // exercised only the size cap / happy path; both are now
    // structurally unreachable because re-detection refuses to
    // confirm SFX on `test.zip`. The cap remains as defense-in-depth
    // and is exercised by integration tests against real SFX
    // fixtures.
    let forged = crate::sfx::SfxDetectionResult {
        is_sfx: true,
        archive_format: None,
        data_offset: Some(100 * 1024 * 1024),
        stub_type: None,
        confidence: crate::sfx::SfxConfidence::Probable,
        evidence: Vec::new(),
        // R0001-0002: a forged result carries no detection identity either.
        source_identity: None,
    };
    let err = Archive::extract_stub(fixture("test.zip"), &forged).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("no longer detected as SFX") || msg.contains("re-detection"),
        "expected SFX re-verification diagnostic, got: {msg}"
    );
}

// ── Archive::finish / close tests ──

#[test]
fn test_close_read_mode_succeeds() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(archive.close().is_ok());
}

#[test]
fn test_finish_read_mode_is_noop() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    assert!(archive.finish().is_ok());
}

// ── Send safety test ──

#[test]
fn test_archive_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Archive>();
}

#[test]
fn test_archive_is_not_sync() {
    // Archive should NOT be Sync - verify this at compile time would require
    // negative trait bounds which Rust doesn't support.
    // Instead, this test documents the design intent.
    // Send is auto-derived from the fields; the narrow unsafe impls live
    // on the raw-handle backends (UnrarArchive, LibarchiveArchive), none
    // of which is Sync (R0079-0015).
}

// ── Drop behavior tests ──

#[test]
fn test_drop_read_mode_no_panic() {
    {
        let _archive = Archive::open(fixture("test.zip")).unwrap();
        // archive dropped here - should not panic
    }
}

#[test]
fn test_drop_write_mode_no_panic() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test_drop.zip");
    {
        let options = crate::options::CompressionOptions::for_writable(WritableFormat::ZIP);
        let _archive = Archive::create(&path, options).unwrap();
        // archive dropped in Write mode - should not panic
    }
}
