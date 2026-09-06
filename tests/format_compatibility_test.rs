//! Format compatibility integration tests
//!
//! Verifies that the unified API works consistently across different archive formats.
//!
//! **Companion suite (R0074-0073).** A second format-compatibility
//! suite lives at [`tests/integration/format_compatibility.rs`]. This
//! root-level file focuses on root-level fixture coverage (smoke
//! tests, regression cases added since v0.2.0); the integration
//! variant carries the broader cross-format expectations. Both share
//! the `tests/fixtures/` data set. Consolidation into a single
//! contract+integration suite is tracked as test-suite reorganization
//! work (R0074-0079).

#[cfg_attr(not(feature = "libarchive"), allow(unused_imports))]
use unified_archive::{Archive, ArchiveFormat, EntryType};

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_format_detection() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    assert_eq!(archive.format(), ArchiveFormat::Rar5);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_list_files() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].entry_type, EntryType::File);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_entry_count() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let count = archive.entry_count().expect("Failed to get entry count");

    assert_eq!(count, 1);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_find_entry() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    // Find existing entry
    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");

    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));

    // Try to find non-existent entry
    let not_found = archive
        .find_entry("nonexistent.txt")
        .expect("Failed to search");

    assert!(not_found.is_none());
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_validate_integrity() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let report = archive
        .validate_integrity()
        .expect("Failed to validate integrity");

    assert_eq!(report.total_entries, 1);
    assert_eq!(report.validated, 1);
    assert!(report.failed.is_empty());
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_rar5_metadata_consistency() {
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

    let entries = archive.list_files().expect("Failed to list files");

    assert_eq!(entries.len(), 1);

    let entry = &entries[0];

    // Verify all metadata fields are consistent
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.entry_type, EntryType::File);
    assert_eq!(entry.size, Some(18));
    assert_eq!(entry.crc32, Some(0x054607BC));
    assert!(entry.compressed_size.is_some());
    assert!(entry.modified.is_some());
}

#[test]
fn test_unsupported_format_error() {
    // Try to open a non-archive file
    let result = Archive::open("Cargo.toml");

    assert!(result.is_err());
    // The error should indicate unknown/unsupported format
}

#[test]
fn test_nonexistent_file_error() {
    let result = Archive::open("tests/fixtures/nonexistent.rar");

    assert!(result.is_err());
    // The error should be an I/O error
}

// `test_validation_report_structure` lived here: it built a `ValidationReport`
// with a struct literal and then asserted the four values it had just written.
// `ValidationReport` is `#[non_exhaustive]` as of OI-0076-005, so an external
// test crate cannot fabricate one — which is the point, it is an output type.
// Nothing was lost: `src/inspection/tests.rs` covers `Debug`, `Clone` and
// equality in-crate, and `tests/integration/format_compatibility.rs` checks a
// report produced by a real `validate_integrity()` call against `entry_count()`.

/// Test that the same API works for both RAR and RAR5
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_api_consistency_rar_and_rar5() {
    // Note: Due to UnRAR's sequential iteration, we need to reopen archives
    // between operations for reliable results

    // Test list_files consistency
    {
        let rar_archive =
            Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
        let rar5_archive =
            Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");

        let rar_entries = rar_archive.list_files().expect("Failed to list RAR files");
        let rar5_entries = rar5_archive
            .list_files()
            .expect("Failed to list RAR5 files");

        // Both should have the same file
        assert_eq!(rar_entries.len(), rar5_entries.len());
        assert_eq!(rar_entries[0].path, rar5_entries[0].path);
        assert_eq!(rar_entries[0].size, rar5_entries[0].size);
        assert_eq!(rar_entries[0].crc32, rar5_entries[0].crc32);
    }

    // Test entry_count consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        assert_eq!(
            rar_archive.entry_count().unwrap(),
            rar5_archive.entry_count().unwrap()
        );
    }

    // Test find_entry consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        let rar_found = rar_archive.find_entry("test_file.txt").unwrap();
        let rar5_found = rar5_archive.find_entry("test_file.txt").unwrap();
        assert!(rar_found.is_some());
        assert!(rar5_found.is_some());
    }

    // Test validation consistency
    {
        let rar_archive = Archive::open("tests/fixtures/test.rar").unwrap();
        let rar5_archive = Archive::open("tests/fixtures/test_rar5.rar").unwrap();

        let rar_report = rar_archive.validate_integrity().unwrap();
        let rar5_report = rar5_archive.validate_integrity().unwrap();
        assert_eq!(rar_report.validated, rar5_report.validated);
    }
}

// ── TAR format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_format_detection() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    assert_eq!(archive.format(), ArchiveFormat::Tar);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_list_files() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let entries = archive.list_files().expect("Failed to list files");

    assert!(!entries.is_empty());
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_entry_count() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let count = archive.entry_count().expect("Failed to get entry count");
    assert!(count >= 1);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_find_entry() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let entry = archive
        .find_entry("test_file.txt")
        .expect("Failed to find entry")
        .expect("Entry not found");
    assert_eq!(entry.path, "test_file.txt");
    assert_eq!(entry.size, Some(18));
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.tar").expect("Failed to open TAR archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.GZ format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_gz_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_gz_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_gz_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.gz").expect("Failed to open TAR.GZ archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.BZ2 format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_bz2_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_bz2_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_bz2_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.bz2").expect("Failed to open TAR.BZ2 archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── TAR.XZ format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_xz_format_detection() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    assert_eq!(archive.format(), ArchiveFormat::TarXz);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_xz_list_files() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert_eq!(file_entries[0].path, "test_file.txt");
}

#[cfg(feature = "libarchive")]
#[test]
fn test_tar_xz_extract_to_memory() {
    let archive =
        Archive::open("tests/fixtures/test.tar.xz").expect("Failed to open TAR.XZ archive");
    let data = archive
        .extract_to_memory("test_file.txt")
        .expect("Failed to extract to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── GZIP format tests ──
// Plain .gz / .bz2 / .xz files use archive_read_support_format_raw() via
// libarchive. Per MADR-0019, the single "data" entry is renamed to the archive's
// file stem (e.g., test.gz → "test") for a stable, predictable API.

#[cfg(feature = "libarchive")]
#[test]
fn test_gzip_format_detection() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    assert_eq!(archive.format(), ArchiveFormat::Gzip);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_gzip_list_files() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    let entries = archive.list_files().expect("Failed to list files");
    // GZIP wraps a single file; libarchive may report it as one entry
    assert!(!entries.is_empty());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_gzip_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.gz").expect("Failed to open GZIP archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract GZIP to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── BZIP2 format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_bzip2_format_detection() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    assert_eq!(archive.format(), ArchiveFormat::Bzip2);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_bzip2_list_files() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_bzip2_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.bz2").expect("Failed to open BZIP2 archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract BZIP2 to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── XZ format tests ──

#[cfg(feature = "libarchive")]
#[test]
fn test_xz_format_detection() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    assert_eq!(archive.format(), ArchiveFormat::Xz);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_xz_list_files() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_xz_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.xz").expect("Failed to open XZ archive");
    let entries = archive.list_files().expect("Failed to list files");
    assert!(!entries.is_empty());
    let data = archive
        .extract_to_memory(&entries[0].path)
        .expect("Failed to extract XZ to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── ISO format tests ──
//
// R0079-0033: ISO is advertised read/extract-supported but previously
// had no end-to-end coverage. tests/fixtures/test.iso was produced via
// `hdiutil makehybrid -iso -joliet` over a staging dir containing
// test_file.txt with the shared fixture content.

#[cfg(feature = "libarchive")]
#[test]
fn test_iso_format_detection() {
    let archive = Archive::open("tests/fixtures/test.iso").expect("Failed to open ISO archive");
    assert_eq!(archive.format(), ArchiveFormat::Iso);
}

#[cfg(feature = "libarchive")]
#[test]
fn test_iso_list_files() {
    let archive = Archive::open("tests/fixtures/test.iso").expect("Failed to open ISO archive");
    let entries = archive.list_files().expect("Failed to list files");

    assert!(!entries.is_empty());
    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .collect();
    assert_eq!(file_entries.len(), 1);
    assert!(
        file_entries[0].path.ends_with("test_file.txt"),
        "unexpected ISO entry path: {}",
        file_entries[0].path
    );
}

#[cfg(feature = "libarchive")]
#[test]
fn test_iso_extract_to_memory() {
    let archive = Archive::open("tests/fixtures/test.iso").expect("Failed to open ISO archive");
    let entries = archive.list_files().expect("Failed to list files");
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == EntryType::File)
        .expect("ISO should contain a file entry");
    let data = archive
        .extract_to_memory(&file_entry.path)
        .expect("Failed to extract ISO entry to memory");
    assert_eq!(String::from_utf8_lossy(&data), "Hello, RAR World!\n");
}

// ── Cross-format consistency tests ──

/// All libarchive-backed formats with test_file.txt should produce
/// identical extracted content
#[cfg(feature = "libarchive")]
#[test]
fn test_cross_format_content_consistency() {
    let tar_formats = vec![
        ("tests/fixtures/test.tar", "TAR"),
        ("tests/fixtures/test.tar.gz", "TAR.GZ"),
        ("tests/fixtures/test.tar.bz2", "TAR.BZ2"),
        ("tests/fixtures/test.tar.xz", "TAR.XZ"),
    ];

    let expected = b"Hello, RAR World!\n";

    for (path, label) in &tar_formats {
        let archive =
            Archive::open(path).unwrap_or_else(|e| panic!("Failed to open {}: {}", label, e));
        let data = archive
            .extract_to_memory("test_file.txt")
            .unwrap_or_else(|e| panic!("Failed to extract from {}: {}", label, e));
        assert_eq!(data, expected, "Content mismatch for {} format", label);
    }
}

/// All single-stream formats should produce identical decompressed content
/// via libarchive's format_raw (MADR-0019).
#[cfg(feature = "libarchive")]
#[test]
fn test_single_stream_content_consistency() {
    let stream_formats = vec![
        "tests/fixtures/test.gz",
        "tests/fixtures/test.bz2",
        "tests/fixtures/test.xz",
    ];

    let expected = b"Hello, RAR World!\n";

    let mut results = Vec::new();
    for path in &stream_formats {
        let archive =
            Archive::open(path).unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));
        let entries = archive
            .list_files()
            .unwrap_or_else(|e| panic!("Failed to list {}: {}", path, e));
        assert!(!entries.is_empty(), "No entries for {}", path);
        let data = archive
            .extract_to_memory(&entries[0].path)
            .unwrap_or_else(|e| panic!("Failed to extract {}: {}", path, e));
        assert_eq!(data, expected, "Content mismatch for {}", path);
        results.push(data);
    }

    // All should be identical
    for (i, data) in results.iter().enumerate().skip(1) {
        assert_eq!(data, &results[0], "Stream format {} differs from first", i);
    }
}

/// AD 0062 A.6: opening a plain gzip file with a `.tar.gz` filename
/// must surface a precise `ArchiveError::Format` instead of producing
/// a malformed listing later. The detect path stays optimistic on the
/// extension hint; the open path's confirmation pass refuses to
/// accept the file as TarGzip when libarchive's first header reports
/// a non-tar format.
#[cfg(feature = "libarchive")]
#[test]
fn test_open_plain_gzip_with_tar_gz_extension_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let misnamed = temp.path().join("masquerader.tar.gz");
    // Copy the plain-gzip fixture under a `.tar.gz` filename.
    std::fs::copy("tests/fixtures/test.gz", &misnamed).unwrap();

    let err = match Archive::open(&misnamed) {
        Ok(_) => panic!("expected open to reject a plain gzip wearing a .tar.gz filename"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("compressed-tar") || msg.contains("not a tar stream") || msg.contains("tar"),
        "expected tar-confirmation diagnostic, got: {msg}"
    );
}

/// AD 0062 A.6: a real tar.gz archive must still open cleanly through
/// the confirmation path — the check must not flag genuine
/// compressed tar inputs.
#[cfg(feature = "libarchive")]
#[test]
fn test_open_real_tar_gz_passes_confirmation() {
    let archive = Archive::open("tests/fixtures/test.tar.gz")
        .expect("real tar.gz must open through the AD 0062 A.6 confirmation pass");
    assert_eq!(archive.format(), ArchiveFormat::TarGzip);
}

/// AD 0062 A.3: requesting `verify_crc32 = true` against a
/// libarchive-backed format (TAR family / ISO / raw gzip-bzip2-xz)
/// must surface `ArchiveError::Unsupported` instead of silently
/// no-opping. The flag previously had per-backend-defined behaviour
/// that varied silently between formats.
#[cfg(feature = "libarchive")]
#[test]
fn test_extract_all_verify_crc32_rejected_on_tar_gz() {
    use unified_archive::{ArchiveError, ExtractionOptions};
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open("tests/fixtures/test.tar.gz").expect("open tar.gz");
    let opts = ExtractionOptions::new(temp.path()).verify_crc32(true);
    let err = match archive.extract_all(opts) {
        Ok(_) => panic!(
            "extract_all with verify_crc32=true must fail on TAR.GZ (libarchive cannot honour it)"
        ),
        Err(e) => e,
    };
    assert!(
        matches!(err, ArchiveError::Unsupported { .. }),
        "expected Unsupported variant, got: {err}"
    );
    assert!(
        err.to_string().to_lowercase().contains("crc"),
        "diagnostic should mention CRC: {err}"
    );
}

/// AD 0062 A.3: `verify_crc32 = false` on the same TAR.GZ fixture
/// must extract cleanly — only the `true` request is gated.
#[cfg(feature = "libarchive")]
#[test]
fn test_extract_all_verify_crc32_false_on_tar_gz_succeeds() {
    use unified_archive::ExtractionOptions;
    let temp = tempfile::tempdir().unwrap();
    let archive = Archive::open("tests/fixtures/test.tar.gz").expect("open tar.gz");
    let opts = ExtractionOptions::new(temp.path()).verify_crc32(false);
    archive
        .extract_all(opts)
        .expect("verify_crc32=false on TAR.GZ must succeed");
}

// ── AD 0062 heuristic-open: extension lies, magic wins ──
//
// `Archive::open` is content-first. When a file's extension and its
// magic bytes disagree, magic-byte detection wins and the archive
// opens as the actual format. `Archive::extension_format()` lets a
// caller observe the mismatch.

/// Heuristic case 1: RAR bytes carrying a `.zip` filename open as
/// RAR. The mismatch is observable via
/// `extension_format()` ≠ `format()`.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_open_rar_bytes_with_zip_extension_routes_to_rar() {
    let temp = tempfile::tempdir().unwrap();
    let misnamed = temp.path().join("masquerader.zip");
    std::fs::copy("tests/fixtures/test.rar", &misnamed)
        .expect("copy RAR fixture under .zip filename");

    let archive = Archive::open(&misnamed).expect("magic-first detection must beat extension");
    assert_eq!(
        archive.format(),
        ArchiveFormat::Rar5,
        "actual format from magic bytes"
    );
    assert_eq!(
        archive.extension_format(),
        Some(ArchiveFormat::Zip),
        "extension claim is ZIP"
    );
    assert_ne!(
        archive.format(),
        archive.extension_format().unwrap(),
        "mismatch must be observable"
    );

    // Functional check: listing reports the embedded RAR entry.
    let entries = archive.list_files().expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
}

/// Heuristic case 2: 7z bytes carrying a `.zip` filename open as 7z.
#[cfg(feature = "sevenzip")]
#[test]
fn test_open_7z_bytes_with_zip_extension_routes_to_7z() {
    let temp = tempfile::tempdir().unwrap();
    let misnamed = temp.path().join("masquerader.zip");
    std::fs::copy("tests/fixtures/test.7z", &misnamed)
        .expect("copy 7z fixture under .zip filename");

    let archive = Archive::open(&misnamed).expect("magic-first detection must beat extension");
    assert_eq!(archive.format(), ArchiveFormat::SevenZip);
    assert_eq!(archive.extension_format(), Some(ArchiveFormat::Zip));

    let entries = archive.list_files().expect("list");
    assert!(!entries.is_empty(), "7z fixture has at least one entry");
}

/// Heuristic case 3: ZIP bytes carrying a `.exe` filename open as
/// ZIP. The `.exe` extension would have triggered the SFX fallback,
/// but magic-first detection finds `PK..` at offset 0 before the
/// fallback runs.
#[test]
fn test_open_zip_bytes_with_exe_extension_routes_to_zip() {
    let temp = tempfile::tempdir().unwrap();
    let misnamed = temp.path().join("masquerader.exe");
    std::fs::copy("tests/fixtures/test.zip", &misnamed)
        .expect("copy ZIP fixture under .exe filename");

    let archive = Archive::open(&misnamed).expect("plain ZIP renamed to .exe must open as ZIP");
    assert_eq!(archive.format(), ArchiveFormat::Zip);
    // `.exe` doesn't map to a single archive format in
    // `format_from_extension`, so the extension claim is `None`.
    assert!(archive.extension_format().is_none());
}

/// SFX-fallback: a real self-extracting archive (executable header
/// followed by an embedded archive at non-zero offset) opens
/// transparently through `Archive::open` thanks to the
/// `.exe`-extension fallback path that retries via `Archive::open_sfx`.
#[cfg(feature = "sfx")]
#[test]
fn test_open_real_sfx_with_exe_extension_falls_back_to_sfx() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let sfx_path = temp.path().join("installer.exe");

    // Synthetic SFX: shell-script stub + padding + real ZIP fixture.
    // Mirrors the shape `tests/integration/sfx_detection.rs` uses for
    // its synthetic SFX cases.
    let stub = b"#!/bin/sh\necho 'self-extracting wrapper'\nexit 0\n";
    let padding = vec![0u8; 100];
    let zip_bytes = std::fs::read("tests/fixtures/test.zip").expect("read fixture");

    let mut f = std::fs::File::create(&sfx_path).unwrap();
    f.write_all(stub).unwrap();
    f.write_all(&padding).unwrap();
    f.write_all(&zip_bytes).unwrap();
    f.flush().unwrap();
    drop(f);

    let archive = Archive::open(&sfx_path)
        .expect("magic-first fails for SFX header; SFX fallback must open the embedded ZIP");
    assert_eq!(archive.format(), ArchiveFormat::Zip);

    let entries = archive.list_files().expect("list embedded entries");
    assert!(
        !entries.is_empty(),
        "embedded ZIP must surface its entries through the facade"
    );
}

/// Negative SFX-fallback: an `.exe` whose content is neither a known
/// archive nor an SFX must surface the original
/// `Unknown archive format` diagnostic, not a generic "executable"
/// message. The fallback only adds detection paths; it doesn't mask
/// content failures.
#[test]
fn test_open_plain_executable_with_exe_extension_propagates_unknown() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let plain_exe = temp.path().join("plain.exe");
    let mut f = std::fs::File::create(&plain_exe).unwrap();
    // MZ DOS header followed by a sparse section table — looks like
    // an executable but contains no embedded archive signatures.
    f.write_all(b"MZ").unwrap();
    f.write_all(&[0u8; 8192]).unwrap();
    f.flush().unwrap();
    drop(f);

    let err = match Archive::open(&plain_exe) {
        Ok(_) => panic!("plain executable must not open through Archive::open"),
        Err(e) => e,
    };
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unknown") || msg.contains("not an archive") || msg.contains("format"),
        "expected detection-failure diagnostic, got: {err}"
    );
}

/// `extension_format()` returns the right format guess for the
/// supported extensions and `None` for ambiguous ones.
#[test]
fn test_extension_format_extension_guess_table() {
    use std::path::Path;
    use unified_archive::format::format_from_extension;

    assert_eq!(
        format_from_extension(Path::new("foo.zip")),
        Some(ArchiveFormat::Zip)
    );
    // R0074-0009: `.rar` is ambiguous between Rar (4.x) and Rar5 — the
    // helper now refuses to guess. Magic-byte detection in
    // `ArchiveFormat::detect_from_bytes` distinguishes the two via the
    // RAR5 / RAR4 marker byte.
    assert_eq!(format_from_extension(Path::new("foo.rar")), None);
    assert_eq!(
        format_from_extension(Path::new("foo.7z")),
        Some(ArchiveFormat::SevenZip)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tar")),
        Some(ArchiveFormat::Tar)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tar.gz")),
        Some(ArchiveFormat::TarGzip)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tgz")),
        Some(ArchiveFormat::TarGzip)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tar.zst")),
        Some(ArchiveFormat::TarZst)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tzst")),
        Some(ArchiveFormat::TarZst)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tar.lz4")),
        Some(ArchiveFormat::TarLz4)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tar.lzma")),
        Some(ArchiveFormat::TarLzma)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.tlz")),
        Some(ArchiveFormat::TarLzma)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.zst")),
        Some(ArchiveFormat::Zst)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.lz4")),
        Some(ArchiveFormat::Lz4)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.lzma")),
        Some(ArchiveFormat::Lzma)
    );
    assert_eq!(
        format_from_extension(Path::new("foo.iso")),
        Some(ArchiveFormat::Iso)
    );
    // Ambiguous / unsupported extensions return None.
    assert_eq!(format_from_extension(Path::new("foo.exe")), None);
    assert_eq!(format_from_extension(Path::new("foo")), None);
    assert_eq!(format_from_extension(Path::new("foo.dat")), None);
}
