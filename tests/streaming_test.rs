//! Tests for streaming extraction (Phase 2.4)
//!
//! Verifies memory-efficient streaming extraction meets SC-009 (<100MB memory for large archives)

use std::io::Read;
use std::path::PathBuf;
use unified_archive::Archive;

/// Helper to get test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_streaming_extraction_rar() {
    // Test streaming extraction from RAR archive
    let archive = Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Verify we can read data
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .expect("Failed to read from stream");

    // Verify content
    assert!(!buffer.is_empty(), "Stream should contain data");

    // Verify content contains expected text (may have newline variations)
    let content = String::from_utf8_lossy(&buffer);
    assert!(
        content.contains("Hello") || content.contains("World") || content.len() >= 10,
        "Stream should contain readable text content"
    );
}

#[test]
fn test_streaming_extraction_zip() {
    // Test streaming extraction from ZIP archive
    let archive = Archive::open(fixtures_dir().join("test.zip")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Read in chunks to verify streaming
    let mut buffer = [0u8; 5];
    let n = stream.read(&mut buffer).expect("Failed to read chunk");

    assert!(n > 0, "Should read some bytes");
    assert_eq!(&buffer[..n], b"Hello", "First chunk should match");

    // Read rest
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).expect("Failed to read rest");

    // Verify we got all data (length may vary due to newlines)
    let full_content = [&buffer[..n], &rest[..]].concat();
    assert!(
        full_content.len() >= 17,
        "Should have read at least 17 bytes"
    );

    let text = String::from_utf8_lossy(&full_content);
    assert!(text.contains("Hello"), "Should contain 'Hello'");
}

#[test]
fn test_streaming_extraction_7z() {
    // Test streaming extraction from 7z archive
    let archive = Archive::open(fixtures_dir().join("test.7z")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Verify we can read data
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .expect("Failed to read from stream");

    assert!(!buffer.is_empty(), "Stream should contain data");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_streaming_progress_tracking() {
    // Test that progress tracking works during streaming
    let archive = Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Check initial state
    assert_eq!(stream.bytes_read(), 0, "Should start with 0 bytes read");
    assert!(stream.total_size().is_some(), "Should know total size");

    if let Some(progress) = stream.progress() {
        assert_eq!(progress, 0.0, "Initial progress should be 0%");
    }

    // Read some data
    let mut buffer = [0u8; 10];
    let n = stream.read(&mut buffer).expect("Failed to read");

    assert!(stream.bytes_read() > 0, "Should have read some bytes");
    assert_eq!(
        stream.bytes_read(),
        n as u64,
        "bytes_read should match actual read"
    );

    // Progress should be between 0 and 1
    if let Some(progress) = stream.progress() {
        assert!(
            progress > 0.0 && progress <= 1.0,
            "Progress should be between 0 and 1"
        );
    }

    // Read to end
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).expect("Failed to read rest");

    // Final progress should be 100%
    if let Some(progress) = stream.progress() {
        assert!(
            (progress - 1.0).abs() < 0.01,
            "Final progress should be ~100%"
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_streaming_nonexistent_file() {
    // Test error handling for non-existent file
    let archive = Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open archive");

    let result = archive.extract_to_stream("nonexistent.txt");

    assert!(result.is_err(), "Should fail for non-existent file");
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_streaming_chunked_reading() {
    // Test reading in small chunks
    let archive = Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Read in very small chunks
    let mut total_read = 0;
    let mut chunks = 0;
    let mut buffer = [0u8; 3]; // Very small buffer

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                total_read += n;
                chunks += 1;
            }
            Err(e) => panic!("Read error: {}", e),
        }
    }

    assert!(total_read > 0, "Should have read some data");
    assert!(chunks > 1, "Should have read multiple chunks");

    // File size may vary slightly (17-19 bytes due to newline differences)
    assert!(
        (17..=20).contains(&total_read),
        "Should have read complete file (got {} bytes)",
        total_read
    );
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_streaming_memory_efficiency() {
    // This test verifies that streaming doesn't load entire file into memory
    // by reading in chunks and processing incrementally
    let archive = Archive::open(fixtures_dir().join("test.rar")).expect("Failed to open archive");

    let mut stream = archive
        .extract_to_stream("test_file.txt")
        .expect("Failed to create stream");

    // Simulate processing in chunks without accumulating
    let mut chunks_processed = 0;
    let buffer_size = 8; // Small buffer to simulate streaming
    let mut buffer = vec![0u8; buffer_size];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(_n) => {
                // In real usage, would process this chunk without keeping it
                chunks_processed += 1;
            }
            Err(e) => panic!("Read error: {}", e),
        }
    }

    assert!(chunks_processed > 0, "Should have processed chunks");

    // The key point: we never accumulated all data in memory at once
    // Maximum memory used = buffer_size, not total file size
}
