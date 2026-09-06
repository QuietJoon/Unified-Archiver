//! Comprehensive edge case tests for recovery percentage extraction
//!
//! Tests covering:
//! - Consistency between has_recovery_record() and recovery_percentage()
//! - Error handling (invalid files, I/O errors, corrupted data)
//! - Boundary conditions (0%, 100%, edge percentages)
//! - Format-specific scenarios (RAR4 vs RAR5, encrypted, solid, multi-volume)
//! - Performance and caching behavior

#[path = "common/mod.rs"]
mod common;

use std::fs;
use unified_archive::Archive;

// ============================================================================
// FIXTURE FACTS
// ============================================================================

/// Recovery metadata of the committed RAR fixtures, read off the bytes on
/// disk (R0001-0088). Each entry is that fixture's fact, not the format's:
/// the RAR5 main header carries an archive-flags vint, and its
/// `MHFL_RECOVERY` bit (0x0008) is what `has_recovery_record()` reports.
///
/// This is the oracle the consistency tests were missing: they only printed
/// the value when a recovery record was present, so `None` or an impossible
/// percentage from a recovery-bearing archive passed silently.
///
/// The recovery-bearing fixtures come from `scripts/generate-rar-fixtures.sh`,
/// which is deliberately *outside* the build: it shells out to the
/// proprietary RARLAB `rar` CLI, which is not a build dependency and which
/// this crate cannot substitute for (RAR creation exists only behind
/// `cfg(all(target_os = "windows", feature = "external-rar-create"))`).
/// Regenerate a fixture and this table in the same commit — the assertions
/// below are what force that.
#[cfg(feature = "rar-support")]
const FIXTURE_RECOVERY: &[(&str, bool, Option<u16>)] = &[
    // Produced without `rar -rr`: archive-flags vint is 0, so
    // `MHFL_RECOVERY` is clear and the percentage must short-circuit.
    ("tests/fixtures/test.rar", false, None),
    ("tests/fixtures/test_rar5.rar", false, None),
    ("tests/fixtures/test_encrypted_data.rar", false, None),
    // Produced with `rar a -rr5p`: archive-flags vint is 0x0008, and the
    // FHEXTRA_SUBDATA record of the "RR" service header carries the 5.
    ("tests/fixtures/test_recovery.rar", true, Some(5)),
    // `rar a -ptest123 -rr5p`: data encrypted, headers readable. This is the
    // combination that exercises `parse_recovery_percentage`'s second,
    // password-less open against an encrypted archive *and gets an answer*
    // (R0001-0089 item 2).
    (
        "tests/fixtures/test_encrypted_data_recovery.rar",
        true,
        Some(5),
    ),
];

/// The expected `(has_recovery_record, recovery_percentage)` pair for a
/// committed fixture.
///
/// Panics for an unlisted path rather than defaulting, so a newly added
/// fixture cannot be asserted against nothing — which is how
/// `test_recovery.rar` sat in the tree for months named after a recovery
/// record it did not have.
#[cfg(feature = "rar-support")]
fn expected_recovery(path: &str) -> (bool, Option<u16>) {
    FIXTURE_RECOVERY
        .iter()
        .find(|(fixture, _, _)| *fixture == path)
        .map(|(_, has_record, percentage)| (*has_record, *percentage))
        .unwrap_or_else(|| {
            panic!(
                "{path} has no FIXTURE_RECOVERY entry; add one, and regenerate the \
                 fixture with scripts/generate-rar-fixtures.sh if it is meant to \
                 carry a recovery record"
            )
        })
}

/// R0001-0088: the documented `has_recovery_record()` /
/// `recovery_percentage()` contract, asserted rather than printed.
///
/// * no recovery flag ⇒ the percentage must be `None`;
/// * recovery flag set ⇒ the percentage is either `None` (the record is
///   present but its percentage could not be read) or a positive value —
///   never 0.
///
/// The upper bound is the format's, not the return type's: RAR 6.10 raised
/// the maximum recovery record from 99% to 1000% and stores the value as a
/// vint, so `1..=100` would reject the archives `rar -rr200p` produces. Until
/// 0.5.0 the accessor returned `Option<u8>` and reported `None` for
/// everything above 255, which put the whole 256..=1000 band on the "cannot
/// be determined" branch; ticgit 7ca208 widened it to `Option<u16>`, so the
/// assertable range is now the format's own `1..=1000`.
///
/// No committed fixture exercises the widened band — that would take
/// `rar -rr256p` and RAR creation is not permitted in this suite. The
/// above-255 case is asserted against synthetic RAR5 blocks in
/// `src/ffi/wrapper/tests.rs`
/// (`rar5_recovery_carries_percentages_above_a_byte`); what this bound does
/// is stop the *fixture* path from silently re-narrowing.
#[cfg(feature = "rar-support")]
fn assert_recovery_contract(label: &str, has_recovery: bool, percentage: Option<u16>) {
    match (has_recovery, percentage) {
        (false, None) => {}
        (false, Some(pct)) => panic!(
            "{label}: has_recovery_record() is false but recovery_percentage() reported {pct}%"
        ),
        (true, Some(pct)) => assert!(
            (1..=1000).contains(&pct),
            "{label}: {pct}% is outside the 1..=1000 a RAR recovery record can \
             declare (RAR 6.10 ceiling); 0 is not a percentage at all"
        ),
        (true, None) => {}
    }
}

/// R0001-0088: assert both the format-level contract *and* the committed
/// fixture's known recovery metadata, so a backend that lost the recovery
/// flag (or invented one) fails instead of printing.
#[cfg(feature = "rar-support")]
fn assert_fixture_recovery_metadata(path: &str) {
    let (expected_has_record, expected_percentage) = expected_recovery(path);
    let archive = Archive::open(path).unwrap_or_else(|e| panic!("Failed to open {path}: {e}"));

    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record failed");
    let percentage = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    assert_recovery_contract(path, has_recovery, percentage);
    assert_eq!(
        has_recovery, expected_has_record,
        "{path}: MHFL_RECOVERY disagrees with FIXTURE_RECOVERY; regenerate the \
         table together with the fixture (scripts/generate-rar-fixtures.sh)"
    );
    assert_eq!(
        percentage, expected_percentage,
        "{path}: recovery percentage disagrees with FIXTURE_RECOVERY"
    );
}

// ============================================================================
// CONSISTENCY TESTS: has_recovery_record() vs recovery_percentage()
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_consistency_rar_recovery_methods() {
    assert_fixture_recovery_metadata("tests/fixtures/test.rar");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_consistency_rar5_recovery_methods() {
    assert_fixture_recovery_metadata("tests/fixtures/test_rar5.rar");
}

/// R0001-0088 / ticgit 8c29f8: `tests/fixtures/test_recovery.rar` now earns
/// its name. It used to be byte-identical to `tests/fixtures/test.rar` (same
/// SHA-256, archive-flags `0`), so the positive branch of the
/// recovery-percentage contract had no fixture anywhere in the repository —
/// every RAR fixture answered `false`, and a backend that simply never
/// reported a recovery record would have passed the whole suite.
///
/// It is now produced by `scripts/generate-rar-fixtures.sh` with
/// `rar a -rr5p`, so its archive-flags vint is `0x0008` and both halves of
/// the contract are exercised: the flag must be `true` *and* the percentage
/// must parse back as the 5 that produced it.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_consistency_recovery_named_fixture() {
    assert_fixture_recovery_metadata("tests/fixtures/test_recovery.rar");
}

#[cfg(feature = "sevenzip")]
#[test]
fn test_consistency_no_recovery_formats() {
    // Formats without recovery support should return consistent results
    let test_cases = vec![
        ("tests/fixtures/test.zip", "ZIP"),
        ("tests/fixtures/test.7z", "7z"),
    ];

    for (path, format) in test_cases {
        let archive = Archive::open(path).unwrap_or_else(|_| panic!("Failed to open {}", format));

        let has_recovery = archive
            .has_recovery_record()
            .expect("has_recovery_record failed");
        let percentage = archive
            .recovery_percentage()
            .expect("recovery_percentage failed");

        assert!(!has_recovery, "{} should not have recovery records", format);
        assert_eq!(
            percentage, None,
            "{} should return None for percentage",
            format
        );
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_recovery_percentage_nonexistent_file() {
    // Should fail gracefully for non-existent file
    let result = Archive::open("tests/fixtures/nonexistent.rar");
    assert!(result.is_err(), "Should fail for non-existent file");
}

#[test]
fn test_recovery_percentage_invalid_rar() {
    // Create a temporary invalid RAR file
    let temp_dir = common::temp_test_dir();
    let temp_path = temp_dir.join("invalid.rar");
    fs::write(&temp_path, b"Not a valid RAR file").expect("Failed to write temp file");

    let result = Archive::open(&temp_path);
    // Should either fail to open or fail during format detection
    assert!(result.is_err(), "Should fail for invalid RAR file");

    // Cleanup
    let _ = fs::remove_file(&temp_path);
}

#[test]
fn test_recovery_percentage_truncated_file() {
    // Create a truncated RAR file (just signature)
    let temp_dir = common::temp_test_dir();
    let temp_path = temp_dir.join("truncated.rar");
    fs::write(&temp_path, b"Rar!\x1A\x07\x00").expect("Failed to write temp file");

    let result = Archive::open(&temp_path);
    if let Ok(archive) = result {
        // Should handle gracefully - either return None or error
        let pct_result = archive.recovery_percentage();
        match pct_result {
            Ok(None) => println!("Truncated file: correctly returns None"),
            Ok(Some(p)) => panic!("Truncated file should not return percentage: {}", p),
            Err(_) => println!("Truncated file: correctly returns error"),
        }
    }

    // Cleanup
    let _ = fs::remove_file(&temp_path);
}

#[test]
fn test_recovery_percentage_empty_file() {
    // Empty file should fail to open
    let temp_dir = common::temp_test_dir();
    let temp_path = temp_dir.join("empty.rar");
    fs::write(&temp_path, b"").expect("Failed to write temp file");

    let result = Archive::open(&temp_path);
    assert!(result.is_err(), "Should fail for empty file");

    // Cleanup
    let _ = fs::remove_file(&temp_path);
}

// ============================================================================
// BOUNDARY CONDITION TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_range_validation() {
    // R0001-0088: `if let Ok(Some(pct))` swallowed both `Err` and `None`,
    // so a backend that stopped answering at all still passed. Require the
    // call to succeed, then hold it to the contract and to the fixture's
    // known metadata.
    let test_files = vec![
        "tests/fixtures/test.rar",
        "tests/fixtures/test_rar5.rar",
        "tests/fixtures/test_recovery.rar",
    ];

    for path in test_files {
        assert_fixture_recovery_metadata(path);
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_multiple_calls() {
    // Multiple calls should return consistent results
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    let pct1 = archive.recovery_percentage().expect("First call failed");
    let pct2 = archive.recovery_percentage().expect("Second call failed");
    let pct3 = archive.recovery_percentage().expect("Third call failed");

    assert_eq!(pct1, pct2, "Multiple calls should return same result");
    assert_eq!(pct2, pct3, "Multiple calls should return same result");
    // R0001-0088: three identical `None`s from a broken accessor would
    // otherwise satisfy the idempotence check — pin the known value too.
    assert_eq!(
        pct1, None,
        "tests/fixtures/test.rar carries no recovery record"
    );
}

// ============================================================================
// FORMAT-SPECIFIC TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_rar4_vs_rar5() {
    // Test that both RAR4 and RAR5 formats are handled correctly
    let rar4 = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR4");
    let rar5 = Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5");

    let pct4 = rar4
        .recovery_percentage()
        .expect("RAR4 recovery_percentage failed");
    let pct5 = rar5
        .recovery_percentage()
        .expect("RAR5 recovery_percentage failed");

    // R0001-0088: the previous body printed both values and only checked
    // the range inside `if let Some(..)`, so `None` from a recovery-bearing
    // archive was indistinguishable from a pass.
    assert_recovery_contract(
        "tests/fixtures/test.rar",
        rar4.has_recovery_record()
            .expect("RAR4 has_recovery_record failed"),
        pct4,
    );
    assert_recovery_contract(
        "tests/fixtures/test_rar5.rar",
        rar5.has_recovery_record()
            .expect("RAR5 has_recovery_record failed"),
        pct5,
    );
    assert_eq!(
        pct4, None,
        "tests/fixtures/test.rar carries no recovery record"
    );
    assert_eq!(
        pct5, None,
        "tests/fixtures/test_rar5.rar carries no recovery record"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_all_supported_formats() {
    // Test all supported archive formats
    let test_cases = vec![
        ("tests/fixtures/test.rar", "RAR4", true),
        ("tests/fixtures/test_rar5.rar", "RAR5", true),
        ("tests/fixtures/test.zip", "ZIP", false),
        ("tests/fixtures/test.7z", "7z", false),
    ];

    for (path, format, supports_recovery) in test_cases {
        let archive =
            Archive::open(path).unwrap_or_else(|_| panic!("Failed to open {} archive", format));

        let pct = archive
            .recovery_percentage()
            .unwrap_or_else(|e| panic!("{} recovery_percentage should not error: {}", format, e));
        let has_recovery = archive
            .has_recovery_record()
            .unwrap_or_else(|e| panic!("{} has_recovery_record should not error: {}", format, e));

        // R0001-0088: the recovery-capable formats used to be exempt from
        // every assertion here ("pct can be Some or None"), which made the
        // RAR rows print-only. None of the committed fixtures carries a
        // recovery record, so both accessors have one right answer.
        assert!(
            !has_recovery,
            "{} fixture {} carries no recovery record",
            format, path
        );
        assert_eq!(
            pct, None,
            "{} fixture {} must report no recovery percentage (supports_recovery={})",
            format, path, supports_recovery
        );
    }
}

// ============================================================================
// EDGE CASE: SPECIAL ARCHIVE TYPES
// ============================================================================

/// Recovery metadata of a **data-encrypted** RAR archive
/// (`test_encrypted_data.rar`, password `test123`, headers readable without
/// one). Both accessors read the main archive header, which encryption of the
/// *data* streams leaves in the clear, so the answer must be the same as for
/// the plaintext fixtures.
///
/// R0001-0089: this test was named `..._encrypted_archive` but opened the
/// plaintext `test.rar` and its own comments admitted it, so the named edge
/// case had no coverage at all. It now exercises a genuinely encrypted
/// fixture — with the honest name.
///
/// This fixture is *data*-encrypted (`rar -p`), so its headers are readable
/// without the password. The header-encrypted × recovery-record combination
/// is covered separately by
/// `test_recovery_percentage_header_encrypted_recovery_archive`, which closes
/// the gap this doc comment used to record (R0001-0089).
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_data_encrypted_archive() {
    assert_fixture_recovery_metadata("tests/fixtures/test_encrypted_data.rar");
}

/// R0001-0089 item 2 / ticgit 8c29f8: the encrypted × recovery-record case
/// that *works*.
///
/// `tests/fixtures/test_encrypted_data_recovery.rar` is produced by
/// `scripts/generate-rar-fixtures.sh` with `rar a -ptest123 -rr5p`: the file
/// data is encrypted but the headers are not, so the second, password-less
/// open inside `recovery_percentage()` can still walk the block structure and
/// read the `FHEXTRA_SUBDATA` record. Before this fixture existed that second
/// open had never met an encrypted archive at all.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_data_encrypted_recovery_archive() {
    assert_fixture_recovery_metadata("tests/fixtures/test_encrypted_data_recovery.rar");
}

/// R0001-0089 / ticgit 8c29f8 / ticgit 3f8790: the header-encrypted ×
/// recovery-record case, which took two fixes to get right.
///
/// `tests/fixtures/test_encrypted_recovery.rar` is produced with
/// `rar a -hptest123 -rr5p`, so the RAR5 main header lives inside an
/// encrypted-header block (HEAD_CRYPT, type 4): the archive cannot be listed
/// without the password, and a raw byte walk from offset 8 lands on
/// ciphertext.
///
/// The two halves of the recovery API reach the file by different routes, and
/// each was wrong in its own way.
///
/// * `has_recovery_record()` reads `ROADF_RECOVERY` from the flags word
///   `RAROpenArchiveEx` produced. That word is derived from the *decrypted*
///   main header, so it needs the password during the open — and
///   `RARSetPassword` used to run afterwards, leaving the flag clear. This
///   crate therefore reported "no recovery record" for an archive
///   `unrar lt -ptest123` describes as "RAR 5, recovery record, encrypted
///   headers". Fixed by supplying the password through `UCM_NEEDPASSWORD`
///   while the header is being read (ticgit 3f8790).
///
/// * `recovery_percentage()` re-opens the path and walks the block structure
///   itself, with no password and no decryption. Fixing the flag above made
///   that walk reachable for the first time, and it promptly read the
///   HEAD_CRYPT ciphertext as a header and reported
///   `Corruption: "RAR5 header extends past end of archive"` — a false damage
///   report on a sound archive. It now short-circuits on `ROADF_ENCHEADERS`.
///
/// So the correct answer for this fixture is "the record is there, its size
/// is not knowable here": `true` and `None`, which
/// `recovery_percentage()`'s documented contract already allows. What must
/// never happen is an `Err` — a header this parser is not permitted to read
/// is not a damaged archive, and reporting corruption would turn every
/// header-encrypted RAR into a false damage report.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_header_encrypted_recovery_archive() {
    let path = "tests/fixtures/test_encrypted_recovery.rar";

    let archive = Archive::open_encrypted(path, "test123")
        .unwrap_or_else(|e| panic!("Failed to open header-encrypted {path}: {e}"));

    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record failed on a header-encrypted archive");

    let percentage = archive.recovery_percentage().unwrap_or_else(|e| {
        panic!(
            "{path}: recovery_percentage() failed on a valid header-encrypted \
             archive: {e}. A header this parser cannot decrypt is not damage."
        )
    });

    assert_recovery_contract(path, has_recovery, percentage);
    assert!(
        has_recovery,
        "{path} was built with `-rr5p`, and `unrar lt -ptest123` reports the \
         record. The password reaches RAROpenArchiveEx now, so the decrypted \
         main header's MHFL_PROTECT bit must be visible here"
    );
    assert_eq!(
        percentage, None,
        "{path}: the percentage lives in the encrypted header, which the \
         raw-byte walk is not permitted to read. `None` is the documented \
         \"cannot be determined\"; anything else means the walk decoded \
         ciphertext"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_solid_archive() {
    // Test recovery percentage on solid archive
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    let is_solid = archive.is_solid().expect("is_solid failed");
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    // R0001-0088: "any combination is valid" made this print-only. Solid
    // and recovery are independent flags in the same RAR5 main header, and
    // this fixture's archive-flags vint is 0 — neither MHFL_SOLID (0x0004)
    // nor MHFL_RECOVERY (0x0008) is set.
    assert!(
        !is_solid,
        "tests/fixtures/test.rar is a single-file non-solid archive"
    );
    assert_eq!(
        pct, None,
        "tests/fixtures/test.rar carries no recovery record"
    );
}

// ============================================================================
// PERFORMANCE AND RESOURCE TESTS
// ============================================================================

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_does_not_extract() {
    // Verify that recovery_percentage doesn't extract the entire archive
    // This is a sanity check - it should only read headers, not extract data

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR");

    // This should be fast (< 100ms typically) since it only reads headers
    let start = std::time::Instant::now();
    // R0001-0088: don't discard the result — a call that errored out
    // immediately would otherwise look like the fastest possible pass.
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");
    let duration = start.elapsed();

    assert_eq!(
        pct, None,
        "tests/fixtures/test.rar carries no recovery record"
    );

    // Should be very fast - less than 1 second even for large archives
    assert!(
        duration.as_secs() < 5,
        "recovery_percentage took too long: {:?}",
        duration
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_read_only_operation() {
    // Verify that calling recovery_percentage doesn't modify the archive file
    let path = "tests/fixtures/test.rar";

    // Get original file metadata
    let metadata_before = fs::metadata(path).expect("Failed to get metadata");
    let modified_before = metadata_before
        .modified()
        .expect("Failed to get modified time");

    // Open and call recovery_percentage
    // R0001-0088: propagate the result — a permanently failing call would
    // trivially satisfy "did not modify the file".
    let archive = Archive::open(path).expect("Failed to open RAR");
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");
    assert_eq!(pct, None, "{path} carries no recovery record");
    drop(archive);

    // Check metadata hasn't changed
    let metadata_after = fs::metadata(path).expect("Failed to get metadata");
    let modified_after = metadata_after
        .modified()
        .expect("Failed to get modified time");

    assert_eq!(
        modified_before, modified_after,
        "recovery_percentage should not modify the archive file"
    );
}

// ============================================================================
// REGRESSION TESTS
// ============================================================================

/// ticgit 7ca208: the public accessor's payload is `u16`, and this test is
/// here to make that a compile error to undo rather than a silent narrowing.
///
/// The type is the whole change. RAR 6.10 stores the RAR5 recovery percentage
/// as a vint and allows up to 1000%, so a `u8` cannot represent the archives
/// `rar -rr256p` and above produce — the old signature answered `None` for
/// every one of them. Narrow it back and the walk starts discarding readable
/// numbers again, which no assertion over the committed fixtures would catch:
/// all of them were built with `-rr5p` or no record at all.
///
/// A ZIP is used deliberately — the binding, not the value, is under test, so
/// this must hold on a build without `rar-support`.
#[test]
fn recovery_percentage_is_typed_wide_enough_for_rar_6_10() {
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");
    let pct: Option<u16> = archive
        .recovery_percentage()
        .expect("recovery_percentage() should succeed");
    assert_eq!(pct, None, "ZIP archives carry no recovery record");
}

/// R0001-0090: `recovery_percentage()` must answer the same before and
/// after `list_files()` has walked (and memoised) the entry list — the
/// listing leaves the UnRAR handle EOF-positioned, so the percentage parse
/// has to work from its own fresh file handle.
///
/// The previous body returned early when the open or the listing failed and
/// accepted both `Ok` and `Err` from the final call, so reaching the end of
/// the function was its only oracle: a backend that had stopped listing, or
/// started erroring on every recovery query, passed unchanged. The fixture
/// is committed, so demand it; the listing must succeed with the known
/// single entry; and the recovery answer is pinned to the fixture's known
/// metadata on both sides of the listing.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_after_list_files() {
    let path = "tests/fixtures/test_rar5.rar";
    let archive = Archive::open(path).expect("committed RAR5 fixture must open");

    let before = archive
        .recovery_percentage()
        .expect("recovery_percentage before list_files failed");

    // List files first — this walks a fresh handle and memoises the result.
    let entries = archive.list_files().expect("list_files must succeed");
    assert_eq!(
        entries.len(),
        1,
        "{path} holds exactly one entry, got {entries:?}"
    );
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].size, Some(18));
    assert_eq!(entries[0].crc32, Some(0x0546_07BC));

    // Now get recovery percentage — this opens a fresh file handle.
    let after = archive
        .recovery_percentage()
        .expect("recovery_percentage after list_files failed");

    assert_eq!(
        after, None,
        "{path} carries no recovery record, so the parse must report None"
    );
    assert_eq!(
        before, after,
        "list_files() must not change the recovery answer"
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_independent_of_format_detection() {
    // Verify recovery_percentage works correctly with format detection
    let path = "tests/fixtures/test.rar";

    let archive = Archive::open(path).expect("Failed to open");
    let format = archive.format();
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage failed");

    // Should work regardless of detected format
    assert!(
        matches!(
            format,
            unified_archive::ArchiveFormat::Rar | unified_archive::ArchiveFormat::Rar5
        ),
        "{path} must be detected as a RAR family format, got {format:?}"
    );
    // R0001-0088: pin the answer too — printing it let a `None` regression
    // through on a fixture whose recovery metadata is known.
    assert_eq!(pct, None, "{path} carries no recovery record");
}
