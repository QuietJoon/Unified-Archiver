//! R0076-0074 / R0076-0075: the executable-extension SFX fallback must
//! surface BOTH the primary detection failure and the SFX probe
//! failure when the two stages fail, in `open` and `open_encrypted`
//! alike. Its own child rather than a section of `archive/tests.rs`, so
//! the fallback tests stay identifiable as a set — the AD 0057 D10
//! move-out took it out of an inline block in `archive.rs`, which is a
//! named large-file target, without changing the module path or any
//! test name.

use super::Archive;
use std::io::Write;

/// `.exe`-named file that is a plausible executable but neither a
/// known archive nor an SFX: detection fails AND the SFX probe
/// finds no payload.
fn write_plain_exe(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("not_an_archive.exe");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"MZ").unwrap();
    f.write_all(&[0u8; 4096]).unwrap();
    f.flush().unwrap();
    path
}

#[test]
fn open_exe_non_archive_reports_both_failures() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_plain_exe(dir.path());

    let err = match Archive::open(&path) {
        Ok(_) => panic!("non-archive .exe must not open"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("Unknown archive format"),
        "error must carry the original detection failure, got: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("self-extracting"),
        "error must carry the SFX fallback failure, got: {msg}"
    );
}

#[test]
fn open_encrypted_exe_non_archive_reports_both_failures() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_plain_exe(dir.path());

    let err = match Archive::open_encrypted(&path, "pw") {
        Ok(_) => panic!("non-archive .exe must not open"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("Unknown archive format"),
        "error must carry the original detection failure, got: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("self-extracting"),
        "error must carry the SFX probe verdict, got: {msg}"
    );
}
