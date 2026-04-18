//! Native Rust 7z backend using the `sevenz-rust2` crate
//!
//! This backend provides native 7z operations with CRC32 support.
//! sevenz-rust2 is a maintained fork with Rust 2024 edition support.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::ffi::common::{AtomicOutputFile, copy_with_optional_crc, normalize_path};
use crate::format::ArchiveFormat;
use crate::options::ProgressCallback;
use crate::security::sanitize_entry_path;
use secstr::SecStr;
use sevenz_rust2::{ArchiveReader, Password};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

/// Native Rust 7z archive wrapper
///
/// Provides 7z operations with CRC32 from metadata when available.
pub struct SevenZArchive {
    path: PathBuf,
    password: Option<SecStr>,
}

impl SevenZArchive {
    /// Open 7z archive for reading
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        Ok(Self {
            path: path_buf,
            password: None,
        })
    }

    /// Open encrypted 7z archive with password
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let mut archive = Self::open(path)?;
        archive.password = Some(SecStr::from(password));
        Ok(archive)
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Open a 7z ArchiveReader with the stored path and password
    fn open_reader(&self) -> Result<ArchiveReader<std::fs::File>> {
        let password = crate::options::password_as_str(&self.password)?
            .map_or_else(Password::empty, Password::from);
        ArchiveReader::open(&self.path, password).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::SevenZip), format!("Invalid 7z: {}", e))
        })
    }

    /// Check if archive uses solid compression
    ///
    /// 7z solid compression compresses multiple files together in a single stream,
    /// providing better compression ratios but requiring sequential decompression.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive uses solid compression
    /// * `Ok(false)` - Archive uses non-solid (independent file) compression
    /// * `Err(...)` - I/O error or invalid archive
    pub fn is_solid(&self) -> Result<bool> {
        let reader = self.open_reader()?;

        let archive = reader.archive();

        // sevenz-rust2 provides direct access to solid flag
        Ok(archive.is_solid)
    }

    /// List all files in 7z archive with CRC32 from metadata
    ///
    /// CRC32 is read from 7z headers when available.
    /// Encryption status is determined from block coders (not password presence).
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        let reader = self.open_reader()?;
        let archive = reader.archive();

        // Pre-compute which blocks use AES encryption
        let aes_id = sevenz_rust2::EncoderMethod::ID_AES256_SHA256;
        let encrypted_blocks: Vec<bool> = archive
            .blocks
            .iter()
            .map(|block| {
                block
                    .coders
                    .iter()
                    .any(|coder| coder.encoder_method_id() == aes_id)
            })
            .collect();

        let mut entries = Vec::new();

        for (index, entry) in archive.files.iter().enumerate() {
            let is_encrypted = archive
                .stream_map
                .file_block_index
                .get(index)
                .and_then(|opt| opt.as_ref())
                .is_some_and(|&block_idx| {
                    encrypted_blocks.get(block_idx).copied().unwrap_or(false)
                });

            let mut parsed_entry = self.parse_entry(entry, is_encrypted)?;

            // CRC32 from 7z metadata (0 is valid — it's the CRC32 of empty content)
            // Note: entry.crc is u64; validate it fits in u32 before casting
            if entry.crc <= u32::MAX as u64 {
                parsed_entry.crc32 = Some(entry.crc as u32);
            }

            parsed_entry.id = index;
            entries.push(parsed_entry);
        }

        Ok(entries)
    }

    /// Parse 7z entry into ArchiveEntry
    fn parse_entry(
        &self,
        entry: &sevenz_rust2::ArchiveEntry,
        is_encrypted: bool,
    ) -> Result<ArchiveEntry> {
        // Normalize path separators to forward slashes
        let path = normalize_path(&entry.name);
        let is_dir = entry.is_directory;

        // Detect symlinks via windows_attributes:
        // - Unix: upper 16 bits contain Unix mode; S_IFLNK = 0xA000
        // - Windows: FILE_ATTRIBUTE_REPARSE_POINT = 0x0400
        let is_symlink = if entry.has_windows_attributes {
            let unix_mode = (entry.windows_attributes >> 16) as u16;
            let unix_symlink = (unix_mode & 0xF000) == 0xA000;
            let win_reparse = (entry.windows_attributes & 0x0400) != 0;
            unix_symlink || win_reparse
        } else {
            false
        };

        let entry_type = if is_dir {
            EntryType::Directory
        } else if is_symlink {
            EntryType::Symlink
        } else {
            EntryType::File
        };

        let size = if is_dir { None } else { Some(entry.size) };

        // 7z doesn't always store compressed size per file
        let compressed_size = Some(entry.compressed_size);

        // Modified time (convert NtTime to SystemTime)
        let modified = if entry.has_last_modified_date {
            // NtTime is 100-nanosecond intervals since 1601-01-01
            // Convert to Unix timestamp (seconds since 1970-01-01)
            let nt_time = u64::from(entry.last_modified_date);
            let unix_epoch_nt = 116444736000000000u64; // NT time at Unix epoch
            if nt_time >= unix_epoch_nt {
                let unix_nanos = (nt_time - unix_epoch_nt) * 100;
                Some(std::time::UNIX_EPOCH + std::time::Duration::from_nanos(unix_nanos))
            } else {
                None
            }
        } else {
            None
        };

        let mut entry_parsed = ArchiveEntry::new(path, 0);
        entry_parsed.entry_type = entry_type;
        entry_parsed.size = size;
        entry_parsed.compressed_size = compressed_size;
        entry_parsed.modified = modified;
        entry_parsed.is_encrypted = is_encrypted;

        Ok(entry_parsed)
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, false)
    }

    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<Vec<ArchiveWarning>> {
        let mut warnings: Vec<ArchiveWarning> = Vec::new();
        let mut reader = self.open_reader()?;

        // Calculate total size for progress
        let total_bytes: u64 = if progress.is_some() {
            reader
                .archive()
                .files
                .iter()
                .filter(|e| !e.is_directory)
                .map(|e| e.size)
                .sum()
        } else {
            0
        };

        let mut bytes_processed = 0u64;
        let mut extraction_error: Option<ArchiveError> = None;

        // Extract using for_each_entries
        let result = reader.for_each_entries(|entry, entry_reader| {
            // Check cancellation
            if let Some(callback) = progress.as_mut() {
                if let ControlFlow::Break(()) =
                    callback.on_progress(bytes_processed, Some(total_bytes))
                {
                    extraction_error =
                        Some(ArchiveError::format(None, "Extraction cancelled by user"));
                    return Ok(false);
                }
            }

            // Sanitize entry path to prevent path traversal
            let normalized_path = normalize_path(&entry.name);
            let entry_path = match sanitize_entry_path(&normalized_path, dest_path) {
                Ok(path) => path,
                Err(e) => {
                    extraction_error = Some(e);
                    return Ok(false);
                }
            };

            // Create parent directories
            if let Some(parent) = entry_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    extraction_error =
                        Some(ArchiveError::io("create_dir", parent.to_path_buf(), e));
                    return Ok(false);
                }
            }

            // Skip symlinks for security (detected via windows_attributes)
            if entry.has_windows_attributes {
                let unix_mode = (entry.windows_attributes >> 16) as u16;
                let is_link =
                    (unix_mode & 0xF000) == 0xA000 || (entry.windows_attributes & 0x0400) != 0;
                if is_link {
                    warnings.push(ArchiveWarning::SkippedSymlink {
                        path: entry.name.clone(),
                        target: None,
                    });
                    return Ok(true); // skip, continue to next entry
                }
            }

            if entry.is_directory {
                // Create directory
                if let Err(e) = std::fs::create_dir_all(&entry_path) {
                    extraction_error = Some(ArchiveError::io("create_dir", entry_path.clone(), e));
                    return Ok(false);
                }
            } else {
                // Extract file
                let mut output_file = match AtomicOutputFile::create(&entry_path, overwrite) {
                    Ok(f) => f,
                    Err(e) => {
                        extraction_error = Some(e);
                        return Ok(false);
                    }
                };

                let expected_crc = if verify_crc32 {
                    Some(entry.crc as u32)
                } else {
                    None
                };

                match copy_with_optional_crc(
                    entry_reader,
                    output_file.file_mut(),
                    expected_crc,
                    &normalized_path,
                    &entry_path,
                ) {
                    Ok(bytes_written) => {
                        if let Err(e) = output_file.commit() {
                            extraction_error = Some(e);
                            return Ok(false);
                        }
                        bytes_processed += bytes_written;
                    }
                    Err(e) => {
                        extraction_error = Some(e);
                        return Ok(false);
                    }
                }
            }

            Ok(true) // Continue extraction
        });

        result.map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::SevenZip), format!("Extract: {}", e))
        })?;

        if let Some(err) = extraction_error {
            return Err(err);
        }

        // Final progress update (100%)
        if let Some(callback) = progress.as_mut() {
            let _ = callback.on_progress(total_bytes, Some(total_bytes));
        }

        Ok(warnings)
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
        let mut reader = self.open_reader()?;

        let mut found = false;
        // Sanitize entry path to prevent path traversal
        let output_path = sanitize_entry_path(file_path, dest_path)?;
        let mut extraction_error: Option<ArchiveError> = None;

        let result = reader.for_each_entries(|entry, entry_reader| {
            let normalized_path = normalize_path(&entry.name);
            if normalized_path == file_path {
                found = true;

                // Create parent directories
                if let Some(parent) = output_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        extraction_error =
                            Some(ArchiveError::io("create_dir", parent.to_path_buf(), e));
                        return Ok(false);
                    }
                }

                // Extract file
                let mut output_file = match AtomicOutputFile::create(&output_path, overwrite) {
                    Ok(f) => f,
                    Err(e) => {
                        extraction_error = Some(e);
                        return Ok(false);
                    }
                };

                let expected_crc = if verify_crc32 {
                    Some(entry.crc as u32)
                } else {
                    None
                };

                if let Err(e) = copy_with_optional_crc(
                    entry_reader,
                    output_file.file_mut(),
                    expected_crc,
                    file_path,
                    &output_path,
                ) {
                    extraction_error = Some(e);
                    return Ok(false);
                }
                if let Err(e) = output_file.commit() {
                    extraction_error = Some(e);
                    return Ok(false);
                }

                Ok(false) // Stop extraction
            } else {
                Ok(true) // Continue
            }
        });

        result.map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::SevenZip), format!("Extract: {}", e))
        })?;

        if let Some(err) = extraction_error {
            return Err(err);
        }

        if !found {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::SevenZip),
                format!("File '{}' not found in archive", file_path),
            ));
        }

        Ok(())
    }

    /// Extract a single file to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        let mut reader = self.open_reader()?;

        let mut result: Option<Vec<u8>> = None;
        let mut extraction_error: Option<ArchiveError> = None;

        let extract_result = reader.for_each_entries(|entry, entry_reader| {
            let normalized_path = normalize_path(&entry.name);
            if normalized_path == file_path {
                let expected_size = entry.size;
                if expected_size > usize::MAX as u64 {
                    extraction_error = Some(ArchiveError::OperationBlocked {
                        operation: crate::error::ops::EXTRACT_TO_MEMORY.to_string(),
                        reason: format!(
                            "Entry '{}' is too large to buffer in memory: {} bytes",
                            file_path, expected_size
                        ),
                    });
                    return Ok(false);
                }

                let mut buffer = Vec::new();
                if buffer.try_reserve(expected_size as usize).is_err() {
                    extraction_error = Some(ArchiveError::OperationBlocked {
                        operation: crate::error::ops::EXTRACT_TO_MEMORY.to_string(),
                        reason: format!(
                            "Unable to allocate {} bytes for entry '{}'",
                            expected_size, file_path
                        ),
                    });
                    return Ok(false);
                }

                if let Err(e) = entry_reader.read_to_end(&mut buffer) {
                    extraction_error = Some(ArchiveError::io("read", PathBuf::from(file_path), e));
                    return Ok(false);
                }

                result = Some(buffer);
                Ok(false) // Stop extraction
            } else {
                Ok(true) // Continue
            }
        });

        extract_result.map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::SevenZip), format!("Extract: {}", e))
        })?;

        if let Some(err) = extraction_error {
            return Err(err);
        }

        result.ok_or_else(|| {
            ArchiveError::format(
                Some(ArchiveFormat::SevenZip),
                format!("File '{}' not found in archive", file_path),
            )
        })
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// The sevenz-rust2 API uses a callback-based extraction model that doesn't
    /// support streaming reads — see AD 0035 for the investigation and deferral
    /// of true entry-level streaming.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use std::io::Cursor;

        // Buffered adapter: sevenz-rust2 0.19.4 exposes no owned entry-level Read
        // (all public APIs are either callback-scoped &mut dyn Read or return Vec<u8>).
        // Deferred per AD 0035 until upstream exposes an owned reader.
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
    /// Streams each entry through an 8KB buffer; sevenz-rust2 verifies
    /// CRC32 during decompression. Returns a list of paths that failed.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut reader = self.open_reader()?;

        let mut failed_files = Vec::new();

        // Single-pass: sevenz-rust2 verifies CRC32 during decompression
        reader
            .for_each_entries(|entry, entry_reader| {
                if entry.is_directory {
                    return Ok(true);
                }

                let path = normalize_path(&entry.name);
                let mut buf = [0u8; 8192];
                loop {
                    match entry_reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_) => continue,
                        Err(_) => {
                            // Drain remaining data to keep decompression stream aligned
                            // (required for solid archives)
                            while entry_reader.read(&mut buf).unwrap_or(0) > 0 {}
                            failed_files.push(path);
                            return Ok(true);
                        }
                    }
                }
                Ok(true)
            })
            .map_err(|e| {
                ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    format!("test_integrity: {}", e),
                )
            })?;

        Ok(failed_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    #[test]
    fn test_sevenz_open_valid() {
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path);
        assert!(archive.is_ok());
        assert_eq!(archive.unwrap().path(), path);
    }

    #[test]
    fn test_sevenz_open_nonexistent() {
        // open() succeeds lazily; error surfaces on first use (list_files)
        let archive = SevenZArchive::open("/nonexistent/archive.7z").unwrap();
        assert!(archive.list_files().is_err());
    }

    #[test]
    fn test_sevenz_list_files() {
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();
        let entries = archive.list_files();
        assert!(entries.is_ok());
        let entries = entries.unwrap();
        assert!(!entries.is_empty(), "7z should have at least one entry");

        for entry in &entries {
            assert!(!entry.path.is_empty(), "Entry path should not be empty");
        }
    }

    #[test]
    fn test_sevenz_list_files_have_crc32() {
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();

        for entry in &entries {
            if entry.entry_type == EntryType::File {
                // 7z should provide CRC32 in metadata
                assert!(
                    entry.crc32.is_some(),
                    "File entry '{}' should have CRC32",
                    entry.path
                );
            }
        }
    }

    #[test]
    fn test_sevenz_extract_to_memory() {
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();
        let data = archive.extract_to_memory("test_file.txt");
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(!data.is_empty(), "Extracted data should not be empty");
    }

    #[test]
    fn test_sevenz_extract_to_memory_nonexistent_entry() {
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();
        let result = archive.extract_to_memory("no_such_file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_sevenz_extract_to_stream() {
        use std::io::Read;
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();
        let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_sevenz_memory_and_stream_produce_same_data() {
        use std::io::Read;
        let path = fixture("test.7z");
        let archive = SevenZArchive::open(&path).unwrap();

        let mem_data = archive.extract_to_memory("test_file.txt").unwrap();

        let archive2 = SevenZArchive::open(&path).unwrap();
        let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
        let mut stream_data = Vec::new();
        stream.read_to_end(&mut stream_data).unwrap();

        assert_eq!(mem_data, stream_data);
    }
}
