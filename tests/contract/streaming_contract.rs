//! Contract tests for streaming extraction
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/streaming.md:
//! 1. Basic streaming reader (extract_to_stream)
//! 2. Chunked reading
//! 3. CRC32 verification (via extract_to_memory)
//! 4. Password-protected streaming
//! 5. Extract to writer (via extract_to_stream + io::copy)
//! 6. Memory-efficient streaming
//!
//! Note: The current API uses Archive::extract_to_stream() returning StreamingExtractor
//! (which implements Read), not entry_reader(). The spec's StreamingOptions and
//! entry_reader_with_options are not yet implemented.

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use std::io::Read;
use unified_archive::{Archive, ArchiveFormat, CompressionOptions};

// ── Contract 1: Basic streaming reader ──

#[test]
fn contract_streaming_basic_read_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .expect("Should have at least one file entry");

    let mut reader = archive.extract_to_stream(&first_file.path).unwrap();
    let mut content = Vec::new();
    reader.read_to_end(&mut content).unwrap();

    assert!(
        !content.is_empty(),
        "Streaming read should produce non-empty content for '{}'",
        first_file.path
    );

    // Verify size matches metadata if available
    if let Some(expected_size) = first_file.size {
        assert_eq!(
            content.len() as u64,
            expected_size,
            "Streamed content size should match metadata for '{}'",
            first_file.path
        );
    }
}

#[test]
#[serial_test::file_serial(rar)]
fn contract_streaming_basic_read_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .expect("Should have at least one file entry");

    let mut reader = archive.extract_to_stream(&first_file.path).unwrap();
    let mut content = Vec::new();
    reader.read_to_end(&mut content).unwrap();

    assert!(!content.is_empty());
}

#[test]
fn contract_streaming_basic_read_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .expect("Should have at least one file entry");

    let mut reader = archive.extract_to_stream(&first_file.path).unwrap();
    let mut content = Vec::new();
    reader.read_to_end(&mut content).unwrap();

    assert!(!content.is_empty());
}

#[test]
fn contract_streaming_basic_read_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_file = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .expect("Should have at least one file entry");

    let mut reader = archive.extract_to_stream(&first_file.path).unwrap();
    let mut content = Vec::new();
    reader.read_to_end(&mut content).unwrap();

    assert!(!content.is_empty());
}

// ── Contract 2: Chunked reading ──

#[test]
fn contract_streaming_chunked_read() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .unwrap();

    let mut reader = archive.extract_to_stream(&file_entry.path).unwrap();

    // Read in small chunks
    let mut chunks = Vec::new();
    let mut buffer = [0u8; 4];
    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        chunks.push(buffer[..n].to_vec());
    }

    let assembled: Vec<u8> = chunks.into_iter().flatten().collect();

    // Compare with extract_to_memory
    let full = archive.extract_to_memory(&file_entry.path).unwrap();
    assert_eq!(
        assembled, full,
        "Chunked streaming should produce same result as extract_to_memory"
    );
}

// ── Contract 3: CRC32 verification (via extract_to_memory consistency) ──

#[test]
fn contract_streaming_matches_extract_to_memory() {
    // The contract guarantees that streaming and full extraction produce
    // identical byte-for-byte output
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();

    for entry in entries
        .iter()
        .filter(|e| e.entry_type == unified_archive::EntryType::File)
    {
        let memory_data = archive.extract_to_memory(&entry.path).unwrap();

        let mut reader = archive.extract_to_stream(&entry.path).unwrap();
        let mut stream_data = Vec::new();
        reader.read_to_end(&mut stream_data).unwrap();

        assert_eq!(
            stream_data, memory_data,
            "Streaming and memory extraction should match for '{}'",
            entry.path
        );
    }
}

// ── Contract 4: Password-protected streaming ──

#[test]
#[serial_test::file_serial(rar)]
fn contract_streaming_encrypted_rar() {
    // test_encrypted_data.rar has data encryption only, password "test123"
    let archive = Archive::open_encrypted(fixture("test_encrypted_data.rar"), "test123").unwrap();
    let entries = archive.list_files().unwrap();
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File);

    if let Some(entry) = file_entry {
        let mut reader = archive.extract_to_stream(&entry.path).unwrap();
        let mut content = Vec::new();
        let result = reader.read_to_end(&mut content);
        // Should succeed with correct password provided at open time
        assert!(
            result.is_ok(),
            "Streaming from encrypted RAR with password should work: {:?}",
            result.err()
        );
    }
}

#[test]
fn contract_streaming_without_password_fails() {
    // test_encrypted_data.rar has data encryption (headers readable without password)
    let archive = Archive::open(fixture("test_encrypted_data.rar")).unwrap();
    let entries = archive.list_files();

    // For data-encrypted archives, listing should work but extraction should fail
    if let Ok(entries) = entries {
        let file_entry = entries
            .iter()
            .find(|e| e.entry_type == unified_archive::EntryType::File);

        if let Some(entry) = file_entry {
            let result = archive.extract_to_stream(&entry.path);
            // Should either fail to open stream or fail during read
            if let Ok(mut reader) = result {
                let mut content = Vec::new();
                let read_result = reader.read_to_end(&mut content);
                // Read should fail due to missing password
                assert!(
                    read_result.is_err() || content.is_empty(),
                    "Reading encrypted data without password should fail or return empty"
                );
            }
            // If extract_to_stream fails, that's also acceptable
        }
    }
    // If list_files fails (header-encrypted), that's also acceptable
}

// ── Contract 5: Extract to writer (stream to file) ──

#[test]
fn contract_streaming_to_file() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .unwrap();

    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("streamed_output.txt");

    // Stream to file using std::io::copy
    let mut reader = archive.extract_to_stream(&file_entry.path).unwrap();
    let mut output = std::fs::File::create(&output_path).unwrap();
    let bytes_written = std::io::copy(&mut reader, &mut output).unwrap();

    assert!(bytes_written > 0, "Should write bytes to file");
    assert!(output_path.exists());

    // Verify content matches
    let file_content = std::fs::read(&output_path).unwrap();
    let memory_content = archive.extract_to_memory(&file_entry.path).unwrap();
    assert_eq!(file_content, memory_content);
}

#[test]
fn contract_streaming_to_vec_writer() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .unwrap();

    let mut reader = archive.extract_to_stream(&file_entry.path).unwrap();
    let mut buffer = Vec::new();
    std::io::copy(&mut reader, &mut buffer).unwrap();

    let expected = archive.extract_to_memory(&file_entry.path).unwrap();
    assert_eq!(buffer, expected);
}

// ── Contract 6: StreamingExtractor progress tracking ──

#[test]
fn contract_streaming_extractor_progress() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let file_entry = entries
        .iter()
        .find(|e| e.entry_type == unified_archive::EntryType::File)
        .unwrap();

    let mut reader = archive.extract_to_stream(&file_entry.path).unwrap();

    // Initial state
    assert_eq!(reader.bytes_read(), 0);

    // Read some data
    let mut buffer = [0u8; 4];
    let n = reader.read(&mut buffer).unwrap();
    if n > 0 {
        assert_eq!(reader.bytes_read(), n as u64);
    }

    // Read the rest
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).unwrap();

    // Total bytes_read should match file size
    if let Some(expected_size) = file_entry.size {
        assert_eq!(
            reader.bytes_read(),
            expected_size,
            "bytes_read() should match file size after full read"
        );
    }
}

// ── Extra: Roundtrip create-stream-verify ──

#[test]
fn contract_streaming_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("stream_roundtrip.zip");
    let original = b"Streaming contract test content with special chars: \xc3\xa9\xc3\xa0\xc3\xbc";

    let options = CompressionOptions::new(ArchiveFormat::Zip);
    let mut archive = Archive::create(&archive_path, options).unwrap();
    archive
        .add_file_from_data("stream_test.bin", original)
        .unwrap();
    archive.finish().unwrap();

    let archive = Archive::open(&archive_path).unwrap();
    let mut reader = archive.extract_to_stream("stream_test.bin").unwrap();
    let mut streamed = Vec::new();
    reader.read_to_end(&mut streamed).unwrap();

    assert_eq!(
        streamed, original,
        "Streamed content should match original byte-for-byte"
    );
}
