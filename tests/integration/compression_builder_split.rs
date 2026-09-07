//! Regression tests for R0075-0081 — per-format compression option
//! builders (`ZipCompressionOptions` / `SevenZCompressionOptions` /
//! `LibarchiveCompressionOptions`).
//!
//! These pin the additive v0.3 API: each builder produces a
//! `CompressionOptions` that drives the existing `Archive::create`
//! path through a format-specific entry point. Round-trip tests
//! confirm the resulting archives are readable.

// Different names here belong to different backends, so the import block is
// partly unused whenever either feature is off. One `cfg_attr` covers both —
// two separate ones expand to the same `allow` when both are off, which
// clippy rejects as a duplicated attribute.
#[cfg_attr(
    not(all(feature = "create", feature = "sevenzip", feature = "libarchive")),
    allow(unused_imports)
)]
use unified_archive::{
    Archive, ArchiveFormat, CompressionLevel, LibarchiveCompressionOptions,
    SevenZCompressionOptions, WritableFormat, ZipCompressionOptions,
};

#[cfg(feature = "create")]
fn unique_dest(prefix: &str, ext: &str) -> std::path::PathBuf {
    // Per-test scratch path under TMPDIR (set by the project's test
    // harness to a machine-local scratch volume). Strictly per-test so
    // parallel runs don't collide.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{nonce}.{ext}"))
}

#[cfg(feature = "create")]
#[test]
fn zip_compression_options_round_trips() {
    let opts = ZipCompressionOptions::new().level(CompressionLevel::Maximum);
    let archive_path = unique_dest("split_zip", "zip");
    let _ = std::fs::remove_file(&archive_path);

    let mut archive = Archive::create_zip(&archive_path, opts).expect("create_zip");
    archive
        .add_file_from_data("hi.txt", b"hello")
        .expect("add_file");
    archive.finish().expect("finish");

    let read = Archive::open(&archive_path).expect("open");
    let entries = read.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "hi.txt");

    let _ = std::fs::remove_file(&archive_path);
}

#[cfg(feature = "create")]
#[cfg(all(feature = "sevenzip", feature = "libarchive"))]
#[test]
fn seven_zip_compression_options_round_trips() {
    let opts = SevenZCompressionOptions::new().level(CompressionLevel::Normal);
    let archive_path = unique_dest("split_7z", "7z");
    let _ = std::fs::remove_file(&archive_path);

    let mut archive = Archive::create_seven_zip(&archive_path, opts).expect("create_seven_zip");
    archive
        .add_file_from_data("hi.txt", b"hello")
        .expect("add_file");
    archive.finish().expect("finish");

    let read = Archive::open(&archive_path).expect("open");
    let entries = read.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);

    let _ = std::fs::remove_file(&archive_path);
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn libarchive_compression_options_round_trips_tar() {
    let opts = LibarchiveCompressionOptions::for_writable(WritableFormat::TAR);
    let archive_path = unique_dest("split_libarchive", "tar");
    let _ = std::fs::remove_file(&archive_path);

    let mut archive = Archive::create_libarchive(&archive_path, opts).expect("create_libarchive");
    archive
        .add_file_from_data("hi.txt", b"hello")
        .expect("add_file");
    archive.finish().expect("finish");

    let read = Archive::open(&archive_path).expect("open");
    let entries = read.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);

    let _ = std::fs::remove_file(&archive_path);
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn libarchive_compression_options_round_trips_tar_gz() {
    let opts = LibarchiveCompressionOptions::for_writable(WritableFormat::TAR_GZIP)
        .level(CompressionLevel::Fast);
    let archive_path = unique_dest("split_libarchive_gz", "tar.gz");
    let _ = std::fs::remove_file(&archive_path);

    let mut archive = Archive::create_libarchive(&archive_path, opts).expect("create_libarchive");
    archive
        .add_file_from_data("hi.txt", b"hello")
        .expect("add_file");
    archive.finish().expect("finish");

    let read = Archive::open(&archive_path).expect("open");
    assert_eq!(read.format(), ArchiveFormat::TarGzip);

    let _ = std::fs::remove_file(&archive_path);
}

#[cfg(feature = "create")]
#[test]
// The deprecated loose constructor is this test's SUBJECT, not an oversight:
// it is the only path that can hold a non-creatable format long enough to
// reach `create_libarchive`. `for_writable` cannot express Rar at all, and
// `try_new` fails at construction — either would delete what is under test.
#[allow(deprecated)]
fn libarchive_options_with_uncreatable_format_fails_at_create_time() {
    // Formats that aren't libarchive-creatable surface the same
    // `OperationBlocked` as the legacy `Archive::create` path.
    let opts = LibarchiveCompressionOptions::new(ArchiveFormat::Rar);
    let archive_path = unique_dest("split_libarchive_rar", "rar");
    let _ = std::fs::remove_file(&archive_path);

    let result = Archive::create_libarchive(&archive_path, opts);
    assert!(
        result.is_err(),
        "RAR creation must be rejected — no native writer"
    );
    let _ = std::fs::remove_file(&archive_path);
}
