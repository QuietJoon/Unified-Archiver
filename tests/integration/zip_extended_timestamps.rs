//! OI-0065-002: 0x5455 extended-timestamp surfacing on the sole `zip`-crate
//! ZIP backend (re-homed here at the AD 0007 dual-ZIP collapse — the piz
//! reader that previously carried this was removed).
//!
//! Verifies that `Archive::open(...).list_files()` on an unencrypted ZIP
//! surfaces `accessed` and `created` from the 0x5455 extended-timestamp
//! extra field via the `zip` crate's central-directory extra-field parse.

use std::time::{Duration, UNIX_EPOCH};
use unified_archive::Archive;

use super::common::build_zip_with_extended_timestamp;

#[test]
fn archive_open_surfaces_accessed_and_created_via_zip_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("ts.zip");

    let mtime = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let atime = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let btime = UNIX_EPOCH + Duration::from_secs(1_400_000_000);

    build_zip_with_extended_timestamp(&zip_path, mtime, atime, btime);

    let archive = Archive::open(&zip_path).expect("open");
    let entries = archive.list_files().expect("list");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];

    assert_eq!(entry.path, "ts.txt");

    let modified = entry.modified.expect("0x5455 must populate modified");
    assert_eq!(
        modified.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        1_700_000_000,
        "0x5455 mod_time must override the DOS timestamp"
    );

    let accessed = entry.accessed.expect("0x5455 must populate accessed");
    assert_eq!(
        accessed.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        1_600_000_000
    );

    let created = entry.created.expect("0x5455 must populate created");
    assert_eq!(
        created.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        1_400_000_000
    );
}

#[test]
fn archive_open_returns_none_when_no_extended_timestamp_present() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("plain.zip");

    {
        use std::fs::File;
        use std::io::Write as _;
        use zip::CompressionMethod;
        use zip::write::{FullFileOptions, ZipWriter};

        let file = File::create(&zip_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let opts = FullFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        writer.start_file("plain.txt", opts).unwrap();
        writer.write_all(b"no timestamps").unwrap();
        writer.finish().unwrap();
    }

    let archive = Archive::open(&zip_path).expect("open");
    let entries = archive.list_files().expect("list");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];

    assert_eq!(entry.path, "plain.txt");
    // Without 0x5455, accessed/created should remain absent. modified
    // can still come from the DOS timestamp, so we don't assert on it.
    assert!(
        entry.accessed.is_none(),
        "no extended timestamp ⇒ accessed should be None"
    );
    assert!(
        entry.created.is_none(),
        "no extended timestamp ⇒ created should be None"
    );
}
