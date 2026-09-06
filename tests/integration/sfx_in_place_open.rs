//! In-place SFX opens — ticket `1ddc37ec`.
//!
//! An SFX file is `[stub program][archive bytes]`. Reading the archive
//! part used to mean copying it into a tempfile first, on every backend,
//! which is the reason AD 0040 caps the payload at 16 GiB by default:
//! the copy is a disk write driven by user-supplied input.
//!
//! The ZIP reader no longer needs the copy — the `zip` crate resolves
//! prepended data itself — so an SFX-shaped file wrapping a ZIP payload
//! is opened straight out of the original file. These tests pin the two
//! halves that the ceiling makes observable:
//!
//! * a payload **above** the caller's staging ceiling opens anyway on the
//!   in-place path (this test fails on the staging path — the ceiling
//!   rejects the open before a byte is copied), and
//! * every backend that still stages keeps the ceiling in force.
//!
//! Which path a handle got is not left implicit: `Archive::payload_access()`
//! reports it.

use std::io::Write;
use std::path::{Path, PathBuf};

use unified_archive::archive::PayloadAccess;
use unified_archive::{Archive, ArchiveFormat, Cap, ExtractionLimits};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A shebang stub (a recognised `ScriptInterpreter` SFX stub) followed
/// by `payload`, written to `dir/name`. The filename matters: the
/// in-place path only engages for an SFX-shaped outer name, because the
/// password-aware extraction reopen resolves an embedded payload only
/// through the SFX route.
fn build_sfx(dir: &Path, name: &str, payload: &[u8]) -> (PathBuf, u64) {
    let stub = b"#!/bin/sh\necho 'sfx stub'\n# payload follows\n";
    let padding = vec![0u8; 512];
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(stub).unwrap();
    file.write_all(&padding).unwrap();
    file.write_all(payload).unwrap();
    file.flush().unwrap();
    (path, (stub.len() + padding.len()) as u64)
}

fn tiny_ceiling() -> ExtractionLimits {
    // 16 bytes: below every fixture payload here, so any staging copy
    // is refused before it starts.
    ExtractionLimits::builder()
        .max_sfx_payload_size(Cap::Limited(16))
        .build()
}

/// The load-bearing test: a ZIP payload far larger than the caller's
/// staging ceiling opens, because nothing is staged. On the staging path
/// this is `Err(ArchiveError::Format)` — "payload size N exceeds
/// maximum 16 bytes" — so a pass here cannot be a pass on both paths.
#[test]
fn zip_sfx_opens_above_the_staging_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).expect("read test.zip");
    assert!(
        zip_bytes.len() as u64 > 16,
        "fixture must exceed the ceiling under test"
    );
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &zip_bytes);

    let archive = Archive::open_at_offset_with_limits(&path, offset, &tiny_ceiling())
        .expect("an in-place open copies nothing, so the staging ceiling cannot bind");

    assert_eq!(
        archive.payload_access(),
        PayloadAccess::InPlace,
        "a ZIP SFX must be read out of the original file"
    );
    assert_eq!(archive.format(), ArchiveFormat::Zip);
    assert_eq!(
        archive.path(),
        path.as_path(),
        "the caller-facing path is the source itself on the in-place path"
    );

    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
}

/// `Archive::open_sfx` (and therefore `Archive::open` on an executable
/// extension) reaches the same in-place path, with real detection
/// supplying the offset.
#[test]
fn open_sfx_reads_a_zip_payload_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).expect("read test.zip");
    let (path, _) = build_sfx(dir.path(), "installer.sh", &zip_bytes);

    let detection = Archive::detect_sfx(&path).expect("detect_sfx");
    assert!(detection.is_sfx(), "fixture must detect as SFX");

    let archive = Archive::open_sfx(&path).expect("open_sfx");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");

    // Extraction reads entry payloads through the same offset-aware
    // handle — the in-place open is not listing-only.
    let out = dir.path().join("out");
    let options = unified_archive::ExtractionOptions::new(&out);
    archive.extract_all(options).expect("extract_all");
    assert!(
        out.join("test_file.txt").is_file(),
        "in-place extraction must materialise the entry"
    );
}

/// AD 0040's ceiling stays in force for every backend that still
/// stages. libarchive covers the TAR family and ISO; it has no
/// callback-based reader here, so a `.tar.gz` payload is still copied
/// and still refused above the caller's cap.
#[test]
fn tar_gz_sfx_still_stages_under_the_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.tar.gz")).expect("read test.tar.gz");
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &bytes);

    let err = match Archive::open_at_offset_with_limits(&path, offset, &tiny_ceiling()) {
        Ok(_) => panic!("a staged payload above the caller's ceiling must be refused"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("exceeds maximum"),
        "the staging ceiling must be the reported cause, got: {err}"
    );
}

/// Same fixture, default ceiling: the tar.gz payload stages, and the
/// handle says so.
#[test]
fn tar_gz_sfx_reports_staged_access() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.tar.gz")).expect("read test.tar.gz");
    // `.tar.gz` on the *outer* name as well, so staging can preserve the
    // compound extension (R0064-0001); the executable extension is what
    // the in-place probe looks at, and this name deliberately has none.
    let (path, offset) = build_sfx(dir.path(), "installer.tar.gz", &bytes);

    let archive = Archive::open_at_offset(&path, offset).expect("open_at_offset for tar.gz");
    assert_eq!(
        archive.payload_access(),
        PayloadAccess::Staged,
        "libarchive-backed payloads are still copied to a tempfile"
    );
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

/// A ZIP payload behind a name that is not SFX-shaped keeps staging.
/// The gate is deliberate: extraction's password-aware reopen resolves
/// an embedded payload only through the SFX route, so an in-place handle
/// on `payload.bin` would break a password-bearing extraction that
/// works today.
#[test]
fn non_executable_extension_keeps_staging() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).expect("read test.zip");
    let (path, offset) = build_sfx(dir.path(), "payload.bin", &zip_bytes);

    let archive = Archive::open_at_offset(&path, offset).expect("open_at_offset");
    assert_eq!(archive.payload_access(), PayloadAccess::Staged);
    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
}

/// A ZIP local-file-header signature at the caller's offset is not on
/// its own enough: the `zip` crate must also agree that the archive
/// *starts* there. Here a decoy header sits at the caller's offset while
/// the real archive (the one the end-of-central-directory record
/// describes) begins 30 bytes later — so reading in place would hand
/// back an archive the caller did not address. The probe declines and
/// staging slices at the caller's literal offset instead.
#[test]
fn decoy_header_at_the_offset_declines_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let zip_bytes = std::fs::read(fixture("test.zip")).expect("read test.zip");

    let mut payload = Vec::new();
    payload.extend_from_slice(b"PK\x03\x04");
    payload.extend_from_slice(&[0u8; 26]);
    payload.extend_from_slice(&zip_bytes);
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &payload);

    let archive = Archive::open_at_offset(&path, offset).expect("open_at_offset");
    assert_eq!(
        archive.payload_access(),
        PayloadAccess::Staged,
        "an offset the zip crate does not agree with must fall back to staging"
    );
}

/// An ordinary archive was never staged and reports so — the accessor is
/// meaningful outside the SFX paths too.
#[test]
fn plain_archive_reports_in_place() {
    let archive = Archive::open(fixture("test.zip")).expect("open test.zip");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);

    // `offset == 0` short-circuits to `Archive::open`, same answer.
    let archive = Archive::open_at_offset(fixture("test.zip"), 0).expect("open_at_offset(0)");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
}

/// The 7z half of the load-bearing pair. 7z is the arm that needed a
/// reader adapter rather than self-relocation — `Archive::read` demands
/// the signature at stream position 0 and searches for nothing — so this
/// is what proves `PayloadWindow` carries a real backend and not just its
/// own unit tests.
///
/// Non-vacuity is the same as the ZIP case: on the staging path this is
/// `Err(ArchiveError::Format)`, "payload size N exceeds maximum 16
/// bytes", so a pass here cannot also be a pass on the staged path.
#[cfg(feature = "sevenzip")]
#[test]
fn sevenz_sfx_opens_above_the_staging_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.7z")).expect("read test.7z");
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &bytes);
    assert!(
        bytes.len() as u64 > 16,
        "the fixture must exceed the tiny ceiling, or this proves nothing"
    );

    let archive = Archive::open_at_offset_with_limits(&path, offset, &tiny_ceiling())
        .expect("a 7z payload read in place is not bounded by a staging ceiling");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
}

#[cfg(feature = "sevenzip")]
#[test]
fn open_sfx_reads_a_sevenz_payload_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.7z")).expect("read test.7z");
    let (path, _) = build_sfx(dir.path(), "installer.sh", &bytes);

    let archive = Archive::open_sfx(&path).expect("open_sfx");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
    let entries = archive.list_files().expect("list_files");
    assert!(!entries.is_empty(), "the payload must list its entries");

    // Extraction decodes through the window too, not just the header
    // walk — the arm is not listing-only.
    let out = dir.path().join("out");
    let options = unified_archive::ExtractionOptions::new(&out);
    archive.extract_all(options).expect("extract_all");
    assert!(
        std::fs::read_dir(&out)
            .expect("destination exists")
            .next()
            .is_some(),
        "in-place extraction must materialise at least one entry"
    );
}

/// The RAR half. RAR needed no adapter and no new FFI — UnRAR's own
/// `IsArchive` scans for the first marker and relocates itself — so what
/// this proves is that the crate now *routes* an offset open there, which
/// it never did before, and that the agreement and reach gates let a
/// legitimate payload through rather than declining everything.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn rar_sfx_opens_above_the_staging_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.rar")).expect("read test.rar");
    let (path, offset) = build_sfx(dir.path(), "installer.sh", &bytes);

    let archive = Archive::open_at_offset_with_limits(&path, offset, &tiny_ceiling())
        .expect("a RAR payload read in place is not bounded by a staging ceiling");
    assert_eq!(archive.payload_access(), PayloadAccess::InPlace);
    let entries = archive.list_files().expect("list_files");
    assert!(!entries.is_empty(), "the payload must list its entries");
}

/// The gate that has no counterpart on the other two arms: unrar binds to
/// the FIRST marker it finds, so a stub carrying an earlier `Rar!` run
/// would silently open a different archive than the caller addressed.
/// That case must decline to staging — and under a ceiling too small to
/// stage under, declining is observable as a refusal rather than as a
/// quietly wrong archive.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn an_earlier_rar_marker_in_the_stub_declines_to_staging() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixture("test.rar")).expect("read test.rar");

    // A stub that itself contains a RAR4 marker, ahead of the real
    // payload — the shape the agreement scan exists to catch.
    let mut stub = b"#!/bin/sh\n# decoy follows\n".to_vec();
    stub.extend_from_slice(b"Rar!\x1a\x07\x00");
    stub.extend_from_slice(&[0u8; 256]);
    let offset = stub.len() as u64;

    let path = dir.path().join("installer.sh");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&stub).unwrap();
    file.write_all(&bytes).unwrap();
    file.flush().unwrap();
    drop(file);

    // `Archive` is not `Debug`, so match rather than `expect_err`.
    let err = match Archive::open_at_offset_with_limits(&path, offset, &tiny_ceiling()) {
        Ok(_) => panic!(
            "an earlier marker must decline the in-place path, and the tiny ceiling then \
             refuses the staging fallback — a success here would mean unrar was handed an \
             archive the caller did not ask for"
        ),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(
        message.contains("exceeds maximum"),
        "the refusal must come from the staging ceiling, proving the in-place arm declined \
         rather than opening the decoy: {message}"
    );
}
