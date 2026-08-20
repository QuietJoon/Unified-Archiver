//! Regression test for R0075-0079 — `ArchiveEntry::permissions` is
//! Unix permission bits only.
//!
//! After R0075-0024 / 0044 / 0045 / 0056 every backend masks file-type
//! and platform-specific attribute bits before populating
//! `permissions`, so callers can rely on `permissions & !0o7777 == 0`.
//! This test pins that contract across the backends we have fixtures
//! for.
//!
//! R0001-0093: the masking check used to live entirely inside
//! `if let Some(perms)`, so a backend that dropped permission metadata for
//! every entry passed vacuously — complete loss of the field was
//! indistinguishable from a clean archive. Each lane below now also
//! requires the metadata to survive listing, and pins the exact masked
//! mode for the backends that decode it.

use unified_archive::Archive;

/// Every fixture in this file is the same 18-byte `test_file.txt` staged
/// with the repository's default umask, so each archive records Unix mode
/// `0o100644` and the masked contract value is `0o644`.
const FIXTURE_MODE: u32 = 0o644;

/// Assert the R0075-0079 masking contract for `archive`, and that the
/// permission metadata actually survived listing (R0001-0093).
///
/// `expected` pins the exact masked mode every permission-bearing entry
/// must carry. `None` means "presence only": the fixture provably records a
/// Unix mode, so `permissions` must not be `None`, but this backend's
/// decoded value is not pinned here.
fn assert_permissions_unix_bits_only(archive: &Archive, label: &str, expected: Option<u32>) {
    let entries = archive.list_files().unwrap();
    assert!(
        !entries.is_empty(),
        "{label}: fixture must list at least one entry"
    );

    let mut with_permissions = 0usize;
    for entry in entries {
        let Some(perms) = entry.permissions else {
            continue;
        };
        with_permissions += 1;
        assert_eq!(
            perms & !0o7777,
            0,
            "permissions field on {} carries non-Unix bits: {:o}",
            entry.path,
            perms
        );
        if let Some(expected) = expected {
            assert_eq!(
                perms, expected,
                "{label}: {} should carry mode {:o}, got {:o}",
                entry.path, expected, perms
            );
        }
    }

    assert!(
        with_permissions > 0,
        "{label}: no entry exposed permission metadata — the fixture records a \
         Unix mode, so losing it entirely is a backend regression, not a \
         format limitation"
    );
}

#[test]
fn permissions_is_unix_bits_only_for_zip() {
    let archive = Archive::open("tests/fixtures/test.zip").unwrap();
    assert_permissions_unix_bits_only(&archive, "zip", Some(FIXTURE_MODE));
}

#[test]
fn permissions_is_unix_bits_only_for_tar() {
    let archive = Archive::open("tests/fixtures/test.tar").unwrap();
    assert_permissions_unix_bits_only(&archive, "tar", Some(FIXTURE_MODE));
}

#[test]
fn permissions_is_unix_bits_only_for_seven_zip() {
    let archive = Archive::open("tests/fixtures/test.7z").unwrap();
    assert_permissions_unix_bits_only(&archive, "7z", Some(FIXTURE_MODE));
}

/// `tests/fixtures/test.rar` is a RAR5 archive written by a Unix host.
/// RAR5 stores the Unix mode unshifted in the file header's Attributes
/// vint — the fixture's vint is `a4 83 02` = 33188 = `0o100644` — so
/// `parse_header` selects the unpacking by `unp_ver` (RAR5 sentinels are
/// `>= 50`; RAR4 carries the raw archived byte and needs `>> 16`). This
/// lane pins the decoded `0o644`, matching the ZIP/TAR/7z lanes
/// (ticgit b75cafb4, 2026-08-16).
#[cfg(feature = "rar-support")]
#[test]
fn permissions_is_unix_bits_only_for_rar() {
    let path = std::path::Path::new("tests/fixtures/test.rar");
    assert!(
        path.exists(),
        "tests/fixtures/test.rar is committed — a missing fixture is a \
         checkout problem, not a reason to skip the lane"
    );
    let archive = Archive::open(path).unwrap();
    assert_permissions_unix_bits_only(&archive, "rar", Some(FIXTURE_MODE));
}

/// Second RAR lane over `tests/fixtures/test_rar5.rar`, whose file header
/// is byte-identical to `test.rar`'s (verified by hexdump). It exists so a
/// regression in the shared RAR5 decode is caught on both committed RAR
/// fixtures rather than only the one the other lanes happen to name.
#[cfg(feature = "rar-support")]
#[test]
fn permissions_is_unix_bits_only_for_rar5_fixture() {
    let path = std::path::Path::new("tests/fixtures/test_rar5.rar");
    assert!(
        path.exists(),
        "tests/fixtures/test_rar5.rar is committed — a missing fixture is a \
         checkout problem, not a reason to skip the lane"
    );
    let archive = Archive::open(path).unwrap();
    assert_permissions_unix_bits_only(&archive, "rar5", Some(FIXTURE_MODE));
}

/// Empirical anchor for the permission decode above. The whole design
/// rests on two pieces of vendored-UnRAR behaviour: `dll.cpp` normalises
/// the RAR5 host OS to `HOST_UNIX` (3), and `ReadHeader50` replaces the
/// archived algorithm byte with the `VER_PACK5` sentinel (50), which is
/// the only signal the DLL API gives us for "this is a RAR5 header".
///
/// If a vendored-unrar upgrade ever changes that normalisation, this test
/// fails loudly and names the culprit, instead of the permission decode
/// silently reverting to `Some(0)` behind a still-green contract lane.
#[cfg(feature = "rar-support")]
#[test]
fn rar_header_reports_unix_host_and_rar5_unp_ver() {
    let path = std::path::Path::new("tests/fixtures/test.rar");
    assert!(path.exists(), "tests/fixtures/test.rar is committed");
    let archive = Archive::open(path).unwrap();
    let entries = archive.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path.ends_with("test_file.txt"))
        .expect("the fixture holds test_file.txt");
    let specific = entry
        .attributes
        .as_ref()
        .and_then(|a| a.archive_specific.as_deref())
        .expect("the RAR reader always records archive_specific metadata");
    assert!(
        specific.contains("host_os=3"),
        "UnRAR must still normalise the RAR5 Unix host to HOST_UNIX (3); got: {specific}"
    );
    assert!(
        specific.contains("unp_ver=50"),
        "UnRAR must still report the VER_PACK5 sentinel for a RAR5 header — \
         the permission decode's format discriminator; got: {specific}"
    );
}
