//! Streaming extraction for memory-efficient archive processing
//!
//! Phase 2.4: Provides streaming extraction to meet SC-009 (<100MB memory for 10GB+ archives)

use crate::error::ArchiveError;
use std::io::{self, Read};

/// Streaming extractor that implements Read trait
///
/// Allows extracting archive entries directly to a stream without loading
/// entire files into memory. Suitable for processing large archives with
/// limited memory.
///
/// # Example
/// ```no_run
/// use unified_archive::Archive;
/// use std::io::Read;
///
/// let archive = Archive::open("large.rar")?;
/// let mut extractor = archive.extract_to_stream("large_file.bin")?;
///
/// let mut buffer = [0u8; 8192];
/// let mut total = 0;
/// while let Ok(n) = extractor.read(&mut buffer) {
///     if n == 0 { break; }
///     // Process chunk without loading entire file
///     total += n;
/// }
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub struct StreamingExtractor {
    /// Internal reader - either from temporary file or direct stream
    reader: Box<dyn Read + Send>,
    /// Total bytes available (if known)
    total_size: Option<u64>,
    /// Bytes read so far
    bytes_read: u64,
}

impl StreamingExtractor {
    /// Create a new streaming extractor from a reader
    pub(crate) fn new(reader: Box<dyn Read + Send>, total_size: Option<u64>) -> Self {
        Self {
            reader,
            total_size,
            bytes_read: 0,
        }
    }

    /// Get total size if known
    pub fn total_size(&self) -> Option<u64> {
        self.total_size
    }

    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Get progress as percentage (0.0 to 1.0) if total size is known
    pub fn progress(&self) -> Option<f64> {
        self.total_size.map(|total| {
            if total == 0 {
                1.0
            } else {
                self.bytes_read as f64 / total as f64
            }
        })
    }
}

impl Read for StreamingExtractor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
}

/// Helper to convert ArchiveError to io::Error
#[allow(dead_code)] // Reserved for future streaming error handling
pub(crate) fn archive_error_to_io(err: ArchiveError) -> io::Error {
    io::Error::other(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_streaming_extractor_basic() {
        let data = b"Hello, streaming world!";
        let cursor = Cursor::new(data.to_vec());
        let mut extractor = StreamingExtractor::new(Box::new(cursor), Some(data.len() as u64));

        // Check initial state
        assert_eq!(extractor.total_size(), Some(data.len() as u64));
        assert_eq!(extractor.bytes_read(), 0);
        assert_eq!(extractor.progress(), Some(0.0));

        // Read some data
        let mut buffer = [0u8; 10];
        let n = extractor.read(&mut buffer).unwrap();
        assert_eq!(n, 10);
        assert_eq!(&buffer[..n], b"Hello, str");
        assert_eq!(extractor.bytes_read(), 10);

        // Check progress
        let progress = extractor.progress().unwrap();
        assert!((progress - 0.434).abs() < 0.01); // ~43.4% progress

        // Read rest
        let mut rest = Vec::new();
        extractor.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"eaming world!");
        assert_eq!(extractor.bytes_read(), data.len() as u64);
        assert_eq!(extractor.progress(), Some(1.0));
    }

    #[test]
    fn test_streaming_extractor_unknown_size() {
        let data = b"Unknown size data";
        let cursor = Cursor::new(data.to_vec());
        let mut extractor = StreamingExtractor::new(Box::new(cursor), None);

        assert_eq!(extractor.total_size(), None);
        assert_eq!(extractor.progress(), None);

        let mut buffer = Vec::new();
        extractor.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, data);
        assert_eq!(extractor.bytes_read(), data.len() as u64);
    }
}
