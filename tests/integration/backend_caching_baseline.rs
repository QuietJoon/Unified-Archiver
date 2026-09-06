//! AD 0065 / OI-0075-002: backend caching baseline (Option A).
//!
//! Two consecutive `list_files()` calls on the same `Archive` handle
//! must return the snapshot taken at the first observation, even if
//! the underlying file is rewritten in place between the two calls.
//! This pins the contract for the libarchive-backed reader (TAR
//! family), which historically reopened the file on every call.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use unified_archive::{Archive, CompressionOptions, WritableFormat};

#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn build_tar_gz(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    // `CompressionOptions` is `#[non_exhaustive]`; `..Default::default()` does
    // not escape that, so the checked constructor is the way in from a test
    // crate. It already defaults `level` to `Normal`.
    let opts = CompressionOptions::for_writable(WritableFormat::TAR_GZIP);
    let mut creator = Archive::create(path, opts).expect("create tar.gz");
    for (name, data) in entries {
        creator
            .add_file_from_data(name, data)
            .expect("add_file_from_data");
    }
    creator.finish().expect("finish tar.gz");
}

#[cfg(feature = "libarchive")]
#[test]
fn libarchive_listing_is_frozen_at_first_observation() {
    let tmp = tempfile::tempdir().unwrap();
    let archive_path: PathBuf = tmp.path().join("snapshot.tar.gz");

    build_tar_gz(&archive_path, &[("alpha.txt", b"first")]);

    let archive = Archive::open(&archive_path).expect("open tar.gz");
    let listing_first = archive.list_files().expect("list_files first");
    assert_eq!(listing_first.len(), 1);
    assert_eq!(listing_first[0].path, "alpha.txt");

    // Replace the archive on disk with one that has a different entry.
    // Under lazy-reopen semantics this would surface as a different
    // listing on the next call. Under AD 0065 (Option A) the cached
    // snapshot wins.
    fs::remove_file(&archive_path).expect("remove archive");
    build_tar_gz(&archive_path, &[("beta.txt", b"second")]);

    let listing_second = archive
        .list_files()
        .expect("list_files second on same handle");

    assert_eq!(
        listing_second.len(),
        1,
        "cached listing must still report a single entry"
    );
    assert_eq!(
        listing_second[0].path, "alpha.txt",
        "cached listing must keep the first observation, not the on-disk replacement"
    );

    // A *fresh* Archive::open observes the new on-disk content — the
    // cache is per-handle, not per-path.
    let fresh = Archive::open(&archive_path).expect("reopen tar.gz");
    let listing_fresh = fresh.list_files().expect("list_files on fresh handle");
    assert_eq!(listing_fresh.len(), 1);
    assert_eq!(
        listing_fresh[0].path, "beta.txt",
        "reopening the archive must observe the on-disk replacement"
    );
}

#[test]
fn zip_listing_is_frozen_at_first_observation() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path: PathBuf = tmp.path().join("snapshot.zip");

    {
        use std::fs::File;
        use zip::CompressionMethod;
        use zip::write::{SimpleFileOptions, ZipWriter};

        let file = File::create(&zip_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("alpha.txt", opts).unwrap();
        writer.write_all(b"first").unwrap();
        writer.finish().unwrap();
    }

    let archive = Archive::open(&zip_path).expect("open zip");
    let listing_first = archive.list_files().expect("list zip first");
    assert_eq!(listing_first.len(), 1);
    assert_eq!(listing_first[0].path, "alpha.txt");

    // Replace with a different ZIP. The sole `zip`-crate backend caches its
    // open handle for the handle lifetime; AD 0065 also pins the parsed
    // listing (the live-mmap portion of AD 0065 went away with piz — DCR-009).
    fs::remove_file(&zip_path).expect("remove zip");
    {
        use std::fs::File;
        use zip::CompressionMethod;
        use zip::write::{SimpleFileOptions, ZipWriter};

        let file = File::create(&zip_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("beta.txt", opts).unwrap();
        writer.write_all(b"second").unwrap();
        writer.finish().unwrap();
    }

    let listing_second = archive.list_files().expect("list zip second");
    assert_eq!(listing_second.len(), 1);
    assert_eq!(
        listing_second[0].path, "alpha.txt",
        "ZIP listing must stay pinned to first observation"
    );
}
