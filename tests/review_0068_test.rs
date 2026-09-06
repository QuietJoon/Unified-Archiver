//! Tests added in response to Review 0068 (R0068-0078..R0068-0090).
//!
//! Each test pins one previously-uncovered behavior or guards against a
//! plausible regression flagged by the review. Tests that would require
//! large new fixtures synthesize their inputs at runtime under the
//! project's `TMPDIR` convention so the suite stays hermetic.
//!
//! **Naming-by-review caveat (R0074-0072).** The file is named after
//! the review that prompted each test, not after the behavior it
//! covers — meaning future maintainers must consult review history
//! to know what is in here. Behavior-driven destinations would be:
//!
//! - `creation_recursive_test.rs` for the recursive-create coverage
//!   (R0068-0078 / R0068-0079).
//! - `extraction_options_test.rs` (already exists) for the
//!   `*_with_options` password propagation cases (R0068-0080).
//! - `sfx_detection_test.rs` for SFX-related cases (R0068-0081 /
//!   R0068-0083).
//! - `modification_backup_test.rs` for backup / commit cases
//!   (R0068-0086 / R0068-0087).
//!
//! The tests are kept here for historical traceability against
//! Review 0068; relocating them is tracked as future test-suite
//! reorganization work alongside R0074-0079 (test taxonomy doc).

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::io::{Read, Write};

use unified_archive::{
    Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions, ExtractionOptions,
    StreamBound, WritableFormat,
};

// ──────────────────────────────────────────────────────────────────────
// R0068-0078 / R0068-0079: per-format recursive create coverage
// ──────────────────────────────────────────────────────────────────────

fn seed_recursive_source(root: &std::path::Path) {
    let src = root.join("rec_src");
    fs::create_dir_all(src.join("subdir")).unwrap();
    fs::write(src.join("a.txt"), b"alpha").unwrap();
    fs::write(src.join("subdir/b.txt"), b"bravo").unwrap();
}

fn assert_recursive_layout(archive_path: &std::path::Path) {
    let reader = Archive::open(archive_path)
        .unwrap_or_else(|e| panic!("open {}: {:?}", archive_path.display(), e));
    let entries = reader.list_files().expect("list_files");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths
            .iter()
            .any(|p| *p == "rec_src/a.txt" || *p == "./rec_src/a.txt"),
        "expected 'rec_src/a.txt' (root-preserving), got: {:?}",
        paths
    );
    assert!(
        paths
            .iter()
            .any(|p| *p == "rec_src/subdir/b.txt" || *p == "./rec_src/subdir/b.txt"),
        "expected 'rec_src/subdir/b.txt', got: {:?}",
        paths
    );
}

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0078_recursive_create_tar() {
    let temp = common::temp_test_dir();
    seed_recursive_source(&temp);
    let archive_path = temp.join("rec.tar");

    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::TAR),
    )
    .unwrap();
    archive
        .add_directory_recursive(temp.join("rec_src"))
        .unwrap();
    archive.finish().unwrap();

    assert_recursive_layout(&archive_path);
    common::cleanup(&temp);
}

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0078_recursive_create_tar_gzip() {
    let temp = common::temp_test_dir();
    seed_recursive_source(&temp);
    let archive_path = temp.join("rec.tar.gz");

    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::TAR_GZIP),
    )
    .unwrap();
    archive
        .add_directory_recursive(temp.join("rec_src"))
        .unwrap();
    archive.finish().unwrap();

    assert_recursive_layout(&archive_path);
    common::cleanup(&temp);
}

#[test]
fn r0068_0079_recursive_create_zip_root_preserving() {
    // Counterpart for the libarchive backends above — confirms the
    // post-Review-0068 ZIP backend now uses the parent-rooted base too.
    let temp = common::temp_test_dir();
    seed_recursive_source(&temp);
    let archive_path = temp.join("rec.zip");

    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    archive
        .add_directory_recursive(temp.join("rec_src"))
        .unwrap();
    archive.finish().unwrap();

    assert_recursive_layout(&archive_path);
    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0080: password propagation in extract_to_memory_with_options /
// extract_to_stream_with_options
// ──────────────────────────────────────────────────────────────────────

#[test]
fn r0068_0080_extract_to_memory_with_options_uses_password() {
    let archive = Archive::open(common::fixture("test_encrypted.zip")).expect("open encrypted zip");

    // Without password: encrypted entry must not yield plaintext.
    let no_pw = ExtractionOptions::default();
    let bare_attempt = archive.extract_to_memory_with_options("test_file.txt", &no_pw);
    assert!(
        bare_attempt.is_err(),
        "extract_to_memory_with_options without password should fail on encrypted ZIP, \
         got Ok({:?} bytes)",
        bare_attempt.as_ref().map(|v| v.len())
    );

    // With password: same call should succeed.
    let with_pw = ExtractionOptions::default().password("test123");
    let bytes = archive
        .extract_to_memory_with_options("test_file.txt", &with_pw)
        .expect("memory extract with password");
    assert!(
        !bytes.is_empty(),
        "expected decrypted plaintext, got empty buffer"
    );
}

#[test]
fn r0068_0080_extract_to_stream_with_options_uses_password() {
    let archive = Archive::open(common::fixture("test_encrypted.zip")).expect("open encrypted zip");

    // Without password: should fail.
    let no_pw = ExtractionOptions::default();
    let bare =
        archive.extract_to_stream_with_options("test_file.txt", &no_pw, StreamBound::DeclaredSize);
    assert!(
        bare.is_err(),
        "extract_to_stream_with_options without password should fail on encrypted ZIP"
    );

    // With password: should succeed and yield non-empty content.
    let with_pw = ExtractionOptions::default().password("test123");
    let mut stream = archive
        .extract_to_stream_with_options("test_file.txt", &with_pw, StreamBound::DeclaredSize)
        .expect("stream extract with password");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read decrypted stream");
    assert!(!buf.is_empty(), "decrypted stream should not be empty");
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0081: Archive::path() survives offset-open
// ──────────────────────────────────────────────────────────────────────

#[test]
fn r0068_0081_open_at_offset_preserves_caller_path() {
    let temp = common::temp_test_dir();
    // Build a self-extracting-style file: 1 KiB of stub padding + a real
    // ZIP archive payload appended. We then open the file at the payload
    // offset and assert that `Archive::path()` still reports the original
    // file, not the staging tempfile.
    let payload_path = temp.join("payload.zip");
    let mut payload = Archive::create(
        &payload_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    payload.add_file_from_data("hello.txt", b"world").unwrap();
    payload.finish().unwrap();
    let payload_bytes = fs::read(&payload_path).expect("read payload");

    let composite_path = temp.join("composite.bin");
    let mut composite = fs::File::create(&composite_path).unwrap();
    composite.write_all(&[0u8; 1024]).unwrap();
    composite.write_all(&payload_bytes).unwrap();
    drop(composite);

    let opened = Archive::open_at_offset(&composite_path, 1024).expect("open_at_offset");
    assert_eq!(
        opened.path(),
        composite_path.as_path(),
        "Archive::path() must remain anchored at the caller-facing source \
         path, not the staging tempfile"
    );
    let entries = opened.list_files().unwrap();
    assert!(
        entries.iter().any(|e| e.path == "hello.txt"),
        "embedded payload should still list cleanly: {:?}",
        entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );

    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0082: multipart detection surfaces unreadable directory errors
// ──────────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn r0068_0082_multipart_detection_surfaces_dir_io_error() {
    use std::os::unix::fs::PermissionsExt;

    let temp = common::temp_test_dir();
    let locked_dir = temp.join("locked");
    fs::create_dir_all(&locked_dir).unwrap();

    // Place a real archive inside the locked directory.
    let archive_path = locked_dir.join("archive.zip");
    let mut a = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    a.add_file_from_data("x.txt", b"x").unwrap();
    a.finish().unwrap();

    let opened = Archive::open(&archive_path).expect("open archive before lockdown");

    // Strip read+exec on the parent so `read_dir` fails.
    //
    // OI-0056-010: this used to `return` when running as root, where the
    // permission bit is ignored — so a root test run reported success
    // having asserted nothing. Root is a property of how the suite was
    // invoked, not of the host, so it now fails with the fix rather than
    // passing quietly.
    assert!(
        !nix_uid_is_root(),
        "this lane needs an unprivileged uid: root ignores the 0o000 mode bit, so `read_dir` \
         would succeed and the I/O-error assertion below would be vacuous. Re-run the suite as \
         a non-root user."
    );
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o000)).unwrap();

    let err = opened.detect_multipart().err();

    // Restore permissions before asserting/cleaning up so cleanup works.
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();
    common::cleanup(&temp);

    let err = err.expect(
        "detect_multipart must surface unreadable-directory I/O failures rather than silently \
         returning empty",
    );
    assert!(
        matches!(err, ArchiveError::Io { .. }),
        "expected ArchiveError::Io for unreadable parent dir, got: {:?}",
        err
    );
}

#[cfg(unix)]
fn nix_uid_is_root() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() == 0 }
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0083: SFX raw-signature negative cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn r0068_0083_executable_with_incidental_gzip_magic_is_not_sfx() {
    // Synthesize a "stubby" file that contains the gzip magic (0x1F 0x8B)
    // at a random offset but no valid gzip stream beyond it. With the
    // Review 0068 raw-signature tightening (CM byte must be 0x08 and
    // reserved flag bits must be clear) this should NOT be reported as a
    // probable SFX gzip archive.
    let temp = common::temp_test_dir();
    let path = temp.join("not_an_sfx.bin");

    // PE-ish header so the SFX preflight sees an executable-shaped file.
    let mut bytes: Vec<u8> = Vec::with_capacity(2048);
    bytes.extend_from_slice(b"MZ"); // 2-byte DOS marker
    bytes.extend_from_slice(&[0u8; 200]);
    // Embed gzip magic but with a non-deflate compression method (not 0x08)
    // and stray reserved-flag bits set. Real gzip never looks like this.
    bytes.extend_from_slice(&[0x1F, 0x8B, 0x42, 0xFF, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&vec![0u8; 1500]);
    fs::write(&path, &bytes).unwrap();

    let detection = Archive::detect_sfx(&path).expect("detect_sfx");
    assert!(
        !detection.is_sfx(),
        "Incidental gzip magic in an executable-shaped file must NOT be classified \
         as an SFX archive (got: {:?})",
        detection
    );

    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0084: empty-directory round-trip on libarchive backends
// ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0084_empty_directory_round_trip_tar() {
    let temp = common::temp_test_dir();
    let src = temp.join("empty_root");
    fs::create_dir_all(src.join("only_an_empty_dir")).unwrap();
    let archive_path = temp.join("empty.tar");

    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::TAR),
    )
    .unwrap();
    archive.add_directory_recursive(&src).unwrap();
    archive.finish().unwrap();

    let reader = Archive::open(&archive_path).unwrap();
    let entries = reader.list_files().unwrap();
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths
            .iter()
            .any(|p| p.ends_with("only_an_empty_dir/") || p.ends_with("only_an_empty_dir")),
        "empty leaf directory must round-trip on libarchive recursive add: {:?}",
        paths
    );

    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0085: recursive symlink rejection parity (Unix-only)
// ──────────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[cfg(feature = "libarchive")]
#[test]
fn r0068_0085_recursive_create_rejects_symlinks_libarchive() {
    use std::os::unix::fs::symlink;

    let temp = common::temp_test_dir();
    let src = temp.join("with_symlink");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("real.txt"), b"hi").unwrap();
    symlink("real.txt", src.join("link.txt")).unwrap();

    let archive_path = temp.join("rejected.tar");
    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::TAR),
    )
    .unwrap();
    let result = archive.add_directory_recursive(&src);
    assert!(
        matches!(
            result,
            Err(ArchiveError::InvalidPath { .. }) | Err(ArchiveError::OperationBlocked { .. })
        ),
        "libarchive recursive add must reject symlinks rather than silently skip them; got: {:?}",
        result
    );
    common::cleanup(&temp);
}

#[cfg(unix)]
#[test]
fn r0068_0085_recursive_create_rejects_symlinks_zip() {
    use std::os::unix::fs::symlink;

    let temp = common::temp_test_dir();
    let src = temp.join("zip_with_symlink");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("real.txt"), b"hi").unwrap();
    symlink("real.txt", src.join("link.txt")).unwrap();

    let archive_path = temp.join("rejected.zip");
    let mut archive = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    let result = archive.add_directory_recursive(&src);
    assert!(
        matches!(
            result,
            Err(ArchiveError::OperationBlocked { .. }) | Err(ArchiveError::InvalidPath { .. })
        ),
        "ZIP recursive add must reject symlinks loudly; got: {:?}",
        result
    );
    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0086: dup-path commit rejection
// ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0086_commit_changes_rejects_added_vs_added_dup() {
    let temp = common::temp_test_dir();
    let archive_path = temp.join("dup.zip");
    let mut creator = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    creator.add_file_from_data("seed.txt", b"seed").unwrap();
    creator.finish().unwrap();

    let mut modifier = Archive::modify(&archive_path).expect("open for modify");
    modifier.add_entry("dup.txt", b"first").unwrap();
    modifier.add_entry("dup.txt", b"second").unwrap();

    let result = modifier.commit_changes();
    assert!(
        matches!(result, Err(ArchiveError::OperationBlocked { .. })),
        "commit_changes must reject duplicate added paths; got: {:?}",
        result
    );

    common::cleanup(&temp);
}

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0086_commit_changes_rejects_retained_vs_added_dup() {
    let temp = common::temp_test_dir();
    let archive_path = temp.join("dup_retained.zip");
    let mut creator = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    creator
        .add_file_from_data("collision.txt", b"original")
        .unwrap();
    creator.finish().unwrap();

    let mut modifier = Archive::modify(&archive_path).expect("open for modify");
    // Add another entry with the same path as a retained one — should be
    // rejected at commit time.
    modifier.add_entry("collision.txt", b"new").unwrap();

    let result = modifier.commit_changes();
    assert!(
        matches!(result, Err(ArchiveError::OperationBlocked { .. })),
        "commit_changes must reject retained-vs-added duplicate paths; got: {:?}",
        result
    );

    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0087: backup noclobber
// ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "libarchive")]
#[test]
fn r0068_0087_commit_changes_refuses_to_clobber_existing_backup() {
    use unified_archive::ModificationOptions;

    let temp = common::temp_test_dir();
    let archive_path = temp.join("backup_clobber.zip");
    let mut creator = Archive::create(
        &archive_path,
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .unwrap();
    creator.add_file_from_data("seed.txt", b"seed").unwrap();
    creator.finish().unwrap();

    // Pre-seed a stale backup file.
    let backup = temp.join("backup_clobber.zip.bak");
    fs::write(&backup, b"old backup content").unwrap();

    let mod_opts = ModificationOptions::new().with_backup(".bak");
    let mut modifier = Archive::modify_with_options(&archive_path, mod_opts).unwrap();
    modifier.add_entry("new.txt", b"new").unwrap();
    let result = modifier.commit_changes();

    assert!(
        result.is_err(),
        "commit_changes must refuse to overwrite an existing backup; got: {:?}",
        result
    );
    // Pre-existing backup must survive intact.
    let preserved = fs::read(&backup).expect("backup must still exist after refusal");
    assert_eq!(preserved, b"old backup content");

    common::cleanup(&temp);
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0088: CompressionOptions::strip_progress yields a progress-free clone
// ──────────────────────────────────────────────────────────────────────

#[test]
fn r0068_0088_strip_progress_drops_callback_explicitly() {
    use std::ops::ControlFlow;
    let mut opts = CompressionOptions::for_writable(WritableFormat::ZIP);
    opts.level = CompressionLevel::Fast;
    opts.progress = Some(Box::new(|_processed: u64, _total: Option<u64>| {
        ControlFlow::Continue(())
    }));

    assert!(opts.progress.is_some(), "test setup");
    let stripped = opts.strip_progress();
    assert!(
        stripped.progress.is_none(),
        "strip_progress must yield a value with no callback"
    );
    assert_eq!(stripped.level, CompressionLevel::Fast);
    assert_eq!(stripped.format(), ArchiveFormat::Zip);
    // Original keeps its callback — strip_progress doesn't mutate self.
    assert!(opts.progress.is_some(), "original retains progress");
}

// ──────────────────────────────────────────────────────────────────────
// R0068-0089: deprecated compression_ratio == compression_fraction
// ──────────────────────────────────────────────────────────────────────

#[test]
fn r0068_0089_compression_ratio_alias_matches_fraction() {
    use unified_archive::ArchiveEntry;
    let mut entry = ArchiveEntry::file("data.bin", 0).build();
    entry.size = Some(1000);
    entry.compressed_size = Some(250);

    #[allow(deprecated)]
    let ratio = entry.compression_ratio();
    let fraction = entry.compression_fraction();
    assert_eq!(
        ratio, fraction,
        "deprecated compression_ratio must remain a thin alias for compression_fraction"
    );
    let expansion = entry.expansion_ratio().unwrap();
    assert!(
        (expansion - 4.0).abs() < f64::EPSILON,
        "expansion_ratio must be size/compressed_size (=4.0 here), got {}",
        expansion
    );
}
