//! Streaming memory verification tests (FR-012, T117a)
//!
//! Verifies that extraction uses bounded memory even for moderately large archives.
//! Target: <20MB memory delta for extracting a 10MB archive.
//!
//! OI-0056-010: the fixtures here used to be built by the external `zip`
//! CLI, and both lanes `eprintln!`-skipped when it was absent — so a host
//! without `zip` produced a green run indistinguishable from a covered one.
//! Fixture creation is now in-process (`common::build_zip`, STORE method,
//! matching the old `-0`), so neither lane can skip and every step asserts.

use super::common;

use std::fs;
use std::path::Path;

/// Number of 1 MiB members `create_test_archive` writes for `total_size_bytes`.
fn member_count(total_size_bytes: usize) -> usize {
    total_size_bytes.div_ceil(1024 * 1024)
}

/// Create a STORE-method ZIP holding `total_size_bytes` of 1 MiB members.
///
/// Infallible by design: no host binary is consulted, so a fixture problem
/// surfaces as a panic inside the builder rather than as a skipped lane.
fn create_test_archive(archive_path: &Path, total_size_bytes: usize) {
    let file_size = 1024 * 1024; // 1MB per file
    let num_files = member_count(total_size_bytes);
    // Repeating pattern, same bytes the CLI fixture carried.
    let pattern: Vec<u8> = (0..=255u8).cycle().take(file_size).collect();

    let names: Vec<String> = (0..num_files)
        .map(|i| format!("file_{:04}.bin", i))
        .collect();
    let members: Vec<common::ZipMember<'_>> = names
        .iter()
        .map(|n| common::ZipMember::File(n.as_str(), &pattern))
        .collect();
    common::build_zip(archive_path, &members, true);
}

#[test]
fn test_streaming_memory_bounded() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test_10mb.zip");
    let extract_dir = temp_dir.path().join("extracted");

    fs::create_dir_all(&extract_dir).expect("Failed to create extract dir");

    // Create a 10MB archive
    let archive_size = 10 * 1024 * 1024; // 10MB
    create_test_archive(&archive_path, archive_size);

    // Verify archive was created
    let archive_metadata = fs::metadata(&archive_path).expect("Archive should exist");
    assert!(archive_metadata.len() > 0, "Archive should not be empty");

    // Open and extract using unified-archive
    let archive =
        unified_archive::Archive::open(&archive_path).expect("streaming fixture must open");

    // List files to verify structure. The count is pinned, not merely
    // non-zero, so a fixture that silently lost members cannot pass.
    let entries = archive.list_files().expect("Failed to list files");
    assert_eq!(
        entries.len(),
        member_count(archive_size),
        "fixture must list every member it was built with"
    );

    // Extract all files
    // Note: We can't directly measure memory here without external tools,
    // but we verify the operation completes successfully using streaming.
    let options = common::default_extraction_options(extract_dir.clone());
    let result = archive.extract_all(options);
    assert!(result.is_ok(), "Extraction should succeed: {:?}", result);

    // Verify files were extracted
    let extracted_files: Vec<_> = fs::read_dir(&extract_dir)
        .expect("Failed to read extract dir")
        .filter_map(|e| e.ok())
        .collect();

    assert!(
        !extracted_files.is_empty(),
        "Should have extracted some files"
    );

    // Verify extracted file content is valid (spot check first file)
    if let Some(first_file) = extracted_files.first() {
        let content = fs::read(first_file.path()).expect("Should read extracted file");
        assert!(!content.is_empty(), "Extracted file should have content");
    }
}

#[test]
fn test_streaming_extraction_single_file() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("test_single.zip");
    let extract_dir = temp_dir.path().join("extracted");

    fs::create_dir_all(&extract_dir).expect("Failed to create extract dir");

    // Create a small archive for basic streaming test
    let archive_size = 1024 * 1024; // 1MB
    create_test_archive(&archive_path, archive_size);

    let archive =
        unified_archive::Archive::open(&archive_path).expect("streaming fixture must open");

    // Extract using streaming
    let options = common::default_extraction_options(extract_dir.clone());
    let result = archive.extract_all(options);
    assert!(result.is_ok(), "Streaming extraction should succeed");
    assert!(
        extract_dir.join("file_0000.bin").is_file(),
        "the single member must land on disk"
    );
}

// R0079-0046: the former `test_memory_efficiency_claim` (comment-only)
// and `test_buffer_size_constants` (asserted local literals against
// themselves; the library exports no read-buffer-size constant) were
// removed — they claimed FR-012 coverage but could not fail. Real
// memory-bound verification requires an external profiling harness
// (valgrind/heaptrack); the streaming tests above cover the
// chunked-extraction behavior itself.
