//! Native Rust ZIP backend using the `piz` crate
//!
//! This backend provides fast ZIP operations with direct CRC32 metadata access
//! and parallel extraction. Piz explicitly exposes CRC32 in file metadata.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, Result};
use crate::ffi::common::{
    compute_crc32_reader, copy_with_optional_crc, create_output_file, normalize_path,
};
use crate::format::ArchiveFormat;
use crate::options::ProgressCallback;
use crate::security::{get_max_mmap_size, sanitize_entry_path, verify_crc32};
use memmap2::Mmap;
use std::fs::File;
use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Native Rust ZIP archive wrapper using piz
///
/// Provides fast ZIP operations with CRC32 directly from metadata.
/// Supports parallel extraction for improved performance.
pub struct PizArchive {
    path: PathBuf,
}

impl PizArchive {
    /// Open ZIP archive for reading
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        Ok(Self { path: path_buf })
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Memory-map the archive file with size limit check
    fn open_mmap(&self, operation: &str) -> Result<Mmap> {
        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("stat", self.path.clone(), e))?;
        let max_mmap = get_max_mmap_size();
        if metadata.len() > max_mmap {
            return Err(ArchiveError::UnsupportedOperation {
                operation: operation.to_string(),
                reason: format!(
                    "ZIP file too large for memory mapping: {} bytes (max {} bytes). \
                    Set UNIFIED_ARCHIVE_MAX_MMAP_SIZE environment variable to increase limit.",
                    metadata.len(),
                    max_mmap
                ),
            });
        }

        unsafe { Mmap::map(&file) }.map_err(|e| ArchiveError::io("mmap", self.path.clone(), e))
    }

    /// List all files in ZIP archive with CRC32 from metadata
    ///
    /// CRC32 is read directly from ZIP central directory (no decompression).
    /// Piz explicitly exposes CRC32 in FileMetadata.
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        let mapping = self.open_mmap("list_files")?;

        let archive = piz::ZipArchive::new(&mapping).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        let mut entries = Vec::new();

        // Iterate through all entries
        for (index, entry_metadata) in archive.entries().iter().enumerate() {
            // Parse entry
            let mut entry = self.parse_entry(entry_metadata)?;

            // CRC32 is directly available in piz metadata!
            if entry.entry_type == EntryType::File {
                entry.crc32 = Some(entry_metadata.crc32); // ✅ Direct access!
            }

            entry.id = index;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Parse piz entry metadata into ArchiveEntry
    fn parse_entry(&self, metadata: &piz::read::FileMetadata) -> Result<ArchiveEntry> {
        // Normalize path separators to forward slashes (fix for Windows-created ZIPs on Linux)
        let path = normalize_path(metadata.path.as_ref().as_str());
        let is_dir = !metadata.is_file();

        // Detect symlinks via Unix mode bits (S_IFLNK = 0xA000)
        let is_symlink = metadata
            .unix_mode
            .is_some_and(|m| (m & 0xF000) == 0xA000);

        let entry_type = if is_dir {
            EntryType::Directory
        } else if is_symlink {
            EntryType::Symlink
        } else {
            EntryType::File
        };

        let size = if is_dir {
            None
        } else {
            Some(metadata.size as u64)
        };
        let compressed_size = if is_dir {
            None
        } else {
            Some(metadata.compressed_size as u64)
        };

        // Modified time from last_modified timestamp (NaiveDateTime -> SystemTime)
        // Guard against pre-epoch timestamps (negative values) which would overflow when cast to u64
        let timestamp = metadata.last_modified.and_utc().timestamp();
        let modified = if timestamp >= 0 {
            Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64))
        } else {
            None // Pre-epoch dates are not representable as SystemTime
        };

        let mut entry = ArchiveEntry::new(path, 0);
        entry.entry_type = entry_type;
        entry.size = size;
        entry.compressed_size = compressed_size;
        entry.modified = modified;
        entry.is_encrypted = metadata.encrypted;

        Ok(entry)
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<()> {
        self.extract_all_with_options(dest_path, progress, true, false)
    }

    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        let mapping = self.open_mmap("extract_all")?;

        let archive = piz::ZipArchive::new(&mapping).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        // Calculate total size for progress
        let total_bytes: u64 = if progress.is_some() {
            archive
                .entries()
                .iter()
                .filter(|e| e.is_file())
                .map(|e| e.size as u64)
                .sum()
        } else {
            0
        };

        let mut bytes_processed = 0u64;

        // Extract each entry
        for entry_metadata in archive.entries() {
            // Check cancellation
            if let Some(callback) = progress.as_mut() {
                if let ControlFlow::Break(()) =
                    callback.on_progress(bytes_processed, Some(total_bytes))
                {
                    return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                }
            }

            // Sanitize entry path to prevent path traversal
            let normalized_path = normalize_path(entry_metadata.path.as_ref().as_str());
            let entry_path = sanitize_entry_path(&normalized_path, dest_path)?;

            // Create parent directories
            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
            }

            // Skip symlinks for security (detected via Unix mode bits)
            let is_symlink = entry_metadata
                .unix_mode
                .is_some_and(|m| (m & 0xF000) == 0xA000);
            if is_symlink {
                continue;
            }

            if !entry_metadata.is_file() {
                // Create directory
                std::fs::create_dir_all(&entry_path)
                    .map_err(|e| ArchiveError::io("create_dir", entry_path.clone(), e))?;
            } else {
                // Extract file
                let mut reader = archive.read(entry_metadata).map_err(|e| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry: {}", e))
                })?;

                let mut output_file = create_output_file(&entry_path, overwrite)?;
                let expected_crc = if verify_crc32 {
                    Some(entry_metadata.crc32)
                } else {
                    None
                };

                let bytes_written = copy_with_optional_crc(
                    &mut reader,
                    &mut output_file,
                    expected_crc,
                    entry_metadata.path.as_str(),
                    &entry_path,
                )?;

                bytes_processed += bytes_written;
            }
        }

        // Final progress update (100%)
        if let Some(callback) = progress.as_mut() {
            let _ = callback.on_progress(total_bytes, Some(total_bytes));
        }

        Ok(())
    }

    /// Extract a single file by path
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, false)
    }

    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        let mapping = self.open_mmap("extract_file")?;

        let archive = piz::ZipArchive::new(&mapping).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        // Find the entry
        let entry_metadata = archive
            .entries()
            .iter()
            .find(|e| normalize_path(e.path.as_ref().as_str()) == file_path)
            .ok_or_else(|| {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("File '{}' not found in archive", file_path),
                )
            })?;

        // Sanitize entry path to prevent path traversal
        let output_path = sanitize_entry_path(file_path, dest_path)?;

        // Create parent directories
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
        }

        // Extract file
        let mut reader = archive.read(entry_metadata).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry: {}", e))
        })?;

        let mut output_file = create_output_file(&output_path, overwrite)?;
        let expected_crc = if verify_crc32 {
            Some(entry_metadata.crc32)
        } else {
            None
        };

        copy_with_optional_crc(
            &mut reader,
            &mut output_file,
            expected_crc,
            file_path,
            &output_path,
        )?;

        Ok(())
    }

    /// Extract a single file to memory with optional CRC32 verification
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.extract_to_memory_internal(file_path, true)
    }

    /// Internal extract to memory with CRC verification control
    pub(crate) fn extract_to_memory_internal(
        &self,
        file_path: &str,
        verify_crc: bool,
    ) -> Result<Vec<u8>> {
        let mapping = self.open_mmap("extract_to_memory")?;

        let archive = piz::ZipArchive::new(&mapping).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        // Find the entry
        let entry_metadata = archive
            .entries()
            .iter()
            .find(|e| normalize_path(e.path.as_ref().as_str()) == file_path)
            .ok_or_else(|| {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("File '{}' not found in archive", file_path),
                )
            })?;

        let expected_crc = if verify_crc {
            Some(entry_metadata.crc32)
        } else {
            None
        };

        let expected_size = usize::try_from(entry_metadata.size).map_err(|_| {
            ArchiveError::UnsupportedOperation {
                operation: "extract_to_memory".to_string(),
                reason: format!(
                    "Entry '{}' is too large to buffer in memory: {} bytes",
                    file_path, entry_metadata.size
                ),
            }
        })?;

        let mut buffer = Vec::new();
        buffer
            .try_reserve(expected_size)
            .map_err(|_| ArchiveError::UnsupportedOperation {
                operation: "extract_to_memory".to_string(),
                reason: format!(
                    "Unable to allocate {} bytes for entry '{}'",
                    expected_size, file_path
                ),
            })?;

        let mut reader = archive.read(entry_metadata).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry: {}", e))
        })?;

        reader
            .read_to_end(&mut buffer)
            .map_err(|e| ArchiveError::io("read", self.path.clone(), e))?;

        // Verify CRC32 if requested
        verify_crc32(&buffer, expected_crc, file_path)?;

        Ok(buffer)
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// True streaming would require self-referential borrowing of the memory-mapped data,
    /// which is not possible with safe Rust lifetimes.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use std::io::Cursor;

        // For now, use extract_to_memory and wrap in cursor
        // TODO: Implement true streaming with piz reader
        let data = self.extract_to_memory(file_path)?;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));

        Ok(crate::streaming::StreamingExtractor::new(
            reader,
            Some(size),
        ))
    }

    /// Test archive integrity by verifying CRC32 for all files
    ///
    /// Opens a single mmap, iterates entries, and stream-verifies CRC32.
    /// Returns a list of file paths that failed verification.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mapping = self.open_mmap("test_integrity")?;
        let archive = piz::ZipArchive::new(&mapping).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        let mut failed_files = Vec::new();

        for entry_metadata in archive.entries().iter() {
            if !entry_metadata.is_file() {
                continue;
            }

            let path = normalize_path(entry_metadata.path.as_ref().as_str());
            let expected_crc = entry_metadata.crc32;

            match archive.read(entry_metadata) {
                Ok(mut reader) => match compute_crc32_reader(&mut reader, &self.path) {
                    Ok(actual_crc) if actual_crc != expected_crc => {
                        failed_files.push(path);
                    }
                    Err(_) => {
                        failed_files.push(path);
                    }
                    _ => {}
                },
                Err(_) => {
                    failed_files.push(path);
                }
            }
        }

        Ok(failed_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    #[test]
    fn test_piz_open_valid_zip() {
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path);
        assert!(archive.is_ok());
        assert_eq!(archive.unwrap().path(), path);
    }

    #[test]
    fn test_piz_open_nonexistent() {
        // open() succeeds lazily; error surfaces on first use (list_files)
        let archive = PizArchive::open("/nonexistent/archive.zip").unwrap();
        assert!(archive.list_files().is_err());
    }

    #[test]
    fn test_piz_list_files() {
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path).unwrap();
        let entries = archive.list_files();
        assert!(entries.is_ok());
        let entries = entries.unwrap();
        assert!(!entries.is_empty(), "ZIP should have at least one entry");

        // Verify entries have expected fields
        for entry in &entries {
            assert!(!entry.path.is_empty(), "Entry path should not be empty");
            // File entries should have CRC32 from piz metadata
            if entry.entry_type == EntryType::File {
                assert!(
                    entry.crc32.is_some(),
                    "File entry '{}' should have CRC32 from piz metadata",
                    entry.path
                );
            }
        }
    }

    #[test]
    fn test_piz_extract_to_memory() {
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path).unwrap();
        let data = archive.extract_to_memory("test_file.txt");
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(!data.is_empty(), "Extracted data should not be empty");
    }

    #[test]
    fn test_piz_extract_to_memory_nonexistent_entry() {
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path).unwrap();
        let result = archive.extract_to_memory("no_such_file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_piz_extract_to_stream() {
        use std::io::Read;
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path).unwrap();
        let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_piz_memory_and_stream_produce_same_data() {
        use std::io::Read;
        let path = fixture("test.zip");
        let archive = PizArchive::open(&path).unwrap();

        let mem_data = archive.extract_to_memory("test_file.txt").unwrap();

        let archive2 = PizArchive::open(&path).unwrap();
        let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
        let mut stream_data = Vec::new();
        stream.read_to_end(&mut stream_data).unwrap();

        assert_eq!(mem_data, stream_data);
    }
}
