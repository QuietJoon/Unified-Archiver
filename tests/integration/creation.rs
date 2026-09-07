//! Integration tests for archive creation functionality
//!
//! Tests the creation API for creating new archives from files and data.

#[cfg_attr(
    not(any(feature = "create", feature = "modify")),
    allow(unused_imports)
)]
use std::fs;
#[cfg_attr(
    not(any(feature = "create", feature = "modify")),
    allow(unused_imports)
)]
use unified_archive::{
    Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions, WritableFormat,
};

#[cfg(feature = "create")]
#[test]
fn test_format_capability_check() {
    let temp = tempfile::tempdir().unwrap();

    let result = Archive::create(
        temp.path().join("test.zip"),
        CompressionOptions::for_writable(WritableFormat::ZIP),
    );
    assert!(result.is_ok());

    // Only the TAR arms need the backend. Gating the whole test would drop the
    // ZIP creation check from the minimal profile — the one format that build
    // is *for* — and leave the lane green while doing it (AD 0058).
    #[cfg(feature = "libarchive")]
    {
        let result = Archive::create(
            temp.path().join("test.tar"),
            CompressionOptions::for_writable(WritableFormat::TAR),
        );
        assert!(result.is_ok());

        let result = Archive::create(
            temp.path().join("test.tar.gz"),
            CompressionOptions::for_writable(WritableFormat::TAR_GZIP),
        );
        assert!(result.is_ok());
    }
}

#[cfg(feature = "create")]
/// Deliberately the deprecated constructor. The subject of this test is that
/// a non-creatable format reaches `Archive::create` and is rejected *there*;
/// `WritableFormat` cannot express `Rar` at all and `try_new` fails at
/// construction, so either replacement would delete what is under test.
#[test]
#[allow(deprecated)]
fn test_unsupported_format_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let rar_opts = CompressionOptions::new(ArchiveFormat::Rar);

    let result = Archive::create(temp.path().join("test.rar"), rar_opts);
    assert!(result.is_err());

    if let Err(e) = result {
        assert!(
            matches!(
                e,
                ArchiveError::Unsupported { .. }
                    | ArchiveError::OperationBlocked { .. }
                    | ArchiveError::ReadOnlyBackend { .. }
            ),
            "Expected Unsupported or OperationBlocked/ReadOnlyBackend, got {:?}",
            e
        );
    }
}

#[cfg(feature = "create")]
#[test]
fn test_existing_file_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let test_file = temp.path().join("existing.zip");

    // Create a file first
    fs::write(&test_file, b"existing").unwrap();

    // Try to create archive with same name — should fail
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let result = Archive::create(&test_file, opts);
    assert!(result.is_err());
}

#[cfg(feature = "create")]
#[test]
fn test_add_file_from_data() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("test_data.zip");

    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    creator
        .add_file_from_data("file1.txt", b"content 1")
        .unwrap();
    creator
        .add_file_from_data("file2.txt", b"content 2")
        .unwrap();
    creator.finish().unwrap();

    // Verify by reopening
    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    assert_eq!(entries.len(), 2);
}

#[cfg(feature = "create")]
#[test]
fn test_add_file_from_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.txt");
    fs::write(&source, b"test content").unwrap();

    let archive_path = temp.path().join("test_path.zip");
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    creator.add_file_from_path(&source).unwrap();
    creator.finish().unwrap();

    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    assert_eq!(entries.len(), 1);
}

#[cfg(feature = "create")]
#[test]
fn test_add_file_from_path_nonexistent() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("test.zip");
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    assert!(creator.add_file_from_path("/nonexistent/file.txt").is_err());
}

#[cfg(feature = "create")]
#[test]
fn test_add_file_from_path_as() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.txt");
    fs::write(&source, b"test content").unwrap();

    let archive_path = temp.path().join("test_path_as.zip");
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    creator
        .add_file_from_path_as(&source, "custom/path/file.txt")
        .unwrap();
    creator.finish().unwrap();

    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "custom/path/file.txt");
}

#[cfg(feature = "create")]
#[test]
fn test_add_directory() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("test_dir.zip");

    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    creator.add_directory("folder1").unwrap();
    creator
        .add_file_from_data("folder1/file.txt", b"content")
        .unwrap();
    creator.finish().unwrap();

    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    assert!(entries.len() >= 2);
    assert!(entries.iter().any(|e| e.path.starts_with("folder1")));
}

#[cfg(feature = "create")]
#[test]
fn test_add_directory_recursive() {
    let temp = tempfile::tempdir().unwrap();
    let src_dir = temp.path().join("src_dir");
    fs::create_dir_all(src_dir.join("subdir")).unwrap();
    fs::write(src_dir.join("file1.txt"), b"content 1").unwrap();
    fs::write(src_dir.join("file2.txt"), b"content 2").unwrap();
    fs::write(src_dir.join("subdir/file3.txt"), b"content 3").unwrap();

    let archive_path = temp.path().join("test_recursive.zip");
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    creator.add_directory_recursive(&src_dir).unwrap();
    creator.finish().unwrap();

    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    assert!(entries.len() >= 3, "Expected at least 3 file entries");
    // ZIP recursive add now preserves the source directory name
    // (`src_dir/...`) to match the libarchive backend and standard tools.
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"src_dir/file1.txt"),
        "expected 'src_dir/file1.txt' under root-preserving recursive add, got: {:?}",
        paths
    );
    assert!(
        paths.contains(&"src_dir/subdir/file3.txt"),
        "expected 'src_dir/subdir/file3.txt' under root-preserving recursive add, got: {:?}",
        paths
    );
}

#[cfg(feature = "create")]
#[test]
fn test_add_directory_recursive_nonexistent() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("test.zip");
    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();

    assert!(
        creator
            .add_directory_recursive("/nonexistent/directory")
            .is_err()
    );
}

#[cfg(feature = "create")]
#[test]
fn test_empty_archive_finish() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("empty.zip");

    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let creator = Archive::create(&archive_path, opts).unwrap();

    // Empty archives are valid — finish should succeed
    assert!(creator.finish().is_ok());
}

#[cfg(feature = "create")]
#[test]
fn test_compression_levels() {
    let levels = vec![
        CompressionLevel::Store,
        CompressionLevel::Fastest,
        CompressionLevel::Fast,
        CompressionLevel::Normal,
        CompressionLevel::Maximum,
        CompressionLevel::Ultra,
    ];

    for level in levels {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("test.zip");

        let mut opts = CompressionOptions::for_writable(WritableFormat::ZIP);
        opts.level = level;

        let mut creator = Archive::create(&archive_path, opts).unwrap();
        creator
            .add_file_from_data("test.txt", b"test content for compression")
            .unwrap();
        creator.finish().unwrap();

        // Verify archive is readable
        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(
            entries.len(),
            1,
            "Level {:?} should produce readable archive",
            level
        );
    }
}

#[cfg(feature = "create")]
#[test]
fn test_finish_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("test_finish.zip");

    let opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    let mut creator = Archive::create(&archive_path, opts).unwrap();
    creator.add_file_from_data("test.txt", b"content").unwrap();

    assert!(creator.finish().is_ok());
    assert!(archive_path.exists());

    // Verify content roundtrips
    let reader = Archive::open(&archive_path).unwrap();
    let data = reader.extract_to_memory("test.txt").unwrap();
    assert_eq!(data, b"content");
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_create_compressed_tar_codec_roundtrip() {
    // R0075-0031 closure: the zstd/lz4/lzma write filters are wired, so
    // TAR.ZST / TAR.LZ4 / TAR.LZMA create end-to-end. Round-trip through
    // open + extract_to_memory to prove both directions agree.
    for (format, file_name) in [
        (ArchiveFormat::TarZst, "roundtrip.tar.zst"),
        (ArchiveFormat::TarLz4, "roundtrip.tar.lz4"),
        (ArchiveFormat::TarLzma, "roundtrip.tar.lzma"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join(file_name);

        let mut creator = Archive::create(
            &archive_path,
            CompressionOptions::try_new(format).expect("every format in this table is creatable"),
        )
        .unwrap();
        creator
            .add_file_from_data("member.txt", b"compressed tar codec roundtrip")
            .unwrap();
        creator.finish().unwrap();

        let reader = Archive::open(&archive_path).unwrap();
        assert_eq!(
            reader.format(),
            format,
            "detection after create for {file_name}"
        );
        let data = reader.extract_to_memory("member.txt").unwrap();
        assert_eq!(
            data, b"compressed tar codec roundtrip",
            "payload mismatch for {file_name}"
        );
    }
}

#[cfg(feature = "create")]
#[cfg(feature = "libarchive")]
#[test]
fn test_create_tar_zst_compression_levels() {
    // zstd has no store mode: `Store` is remapped to level 1 in the
    // libarchive writer rather than 0 (which zstd reads as "default").
    // Every level must still produce a readable archive.
    for level in [
        CompressionLevel::Store,
        CompressionLevel::Fastest,
        CompressionLevel::Ultra,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("levels.tar.zst");

        let mut opts = CompressionOptions::for_writable(WritableFormat::TAR_ZST);
        opts.level = level;

        let mut creator = Archive::create(&archive_path, opts).unwrap();
        creator
            .add_file_from_data("test.txt", b"tar.zst level coverage")
            .unwrap();
        creator.finish().unwrap();

        let reader = Archive::open(&archive_path).unwrap();
        let data = reader.extract_to_memory("test.txt").unwrap();
        assert_eq!(
            data, b"tar.zst level coverage",
            "Level {level:?} should produce a readable tar.zst"
        );
    }
}
