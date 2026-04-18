//! Native Rust ZIP backend using the `zip` crate
//!
//! This backend provides fast ZIP operations with direct CRC32 access from metadata.
//! Falls back to computing CRC32 when null/missing in the archive.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::format::ArchiveFormat;
use crate::options::ProgressCallback;
use crate::security::sanitize_entry_path;
use secstr::SecStr;
use std::fs::File;
use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use zip::ZipArchive as RawZipArchive;

use super::common::{AtomicOutputFile, compute_crc32_reader, copy_with_optional_crc};

/// Open a ZIP entry by index, using password decryption if provided
fn open_entry_by_index<'a>(
    zip: &'a mut RawZipArchive<File>,
    index: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a>> {
    if let Some(pw) = password {
        zip.by_index_decrypt(index, pw.as_bytes()).map_err(|e| {
            if matches!(e, zip::result::ZipError::InvalidPassword) {
                ArchiveError::password(format!("Invalid password for ZIP entry {}", index))
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("Read entry {}: {}", index, e),
                )
            }
        })
    } else {
        zip.by_index(index).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read entry {}: {}", index, e),
            )
        })
    }
}

/// Open a ZIP entry by name, using password decryption if provided
fn open_entry_by_name<'a>(
    zip: &'a mut RawZipArchive<File>,
    name: &str,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a>> {
    if let Some(pw) = password {
        zip.by_name_decrypt(name, pw.as_bytes()).map_err(|e| {
            if matches!(e, zip::result::ZipError::InvalidPassword) {
                ArchiveError::password(format!("Invalid password for ZIP entry '{}'", name))
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("Read entry '{}': {}", name, e),
                )
            }
        })
    } else {
        zip.by_name(name).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read entry '{}': {}", name, e),
            )
        })
    }
}

/// Open a ZIP file and create a RawZipArchive
fn open_zip(path: &Path) -> Result<RawZipArchive<File>> {
    let file = File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;
    RawZipArchive::new(file)
        .map_err(|e| ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e)))
}

/// Native Rust ZIP archive wrapper
///
/// Provides fast ZIP operations with CRC32 from metadata (no decompression needed).
/// Automatically computes CRC32 when missing/null in archive.
pub struct ZipArchive {
    path: PathBuf,
    password: Option<SecStr>,
}

impl ZipArchive {
    /// Open ZIP archive for reading
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        Ok(Self {
            path: path_buf,
            password: None,
        })
    }

    /// Open encrypted ZIP archive with password
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let mut archive = Self::open(path)?;
        archive.password = Some(SecStr::from(password));
        Ok(archive)
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// List all files in ZIP archive with CRC32 from central directory metadata
    ///
    /// CRC32 is read directly from the ZIP central directory (no decompression needed).
    /// A CRC32 value of 0 is preserved as valid (it's the correct CRC32 for empty content).
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        let mut zip = open_zip(&self.path)?;

        let mut entries = Vec::new();

        for i in 0..zip.len() {
            let zip_file = zip.by_index(i).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry {}: {}", i, e))
            })?;

            // Parse entry metadata
            let mut entry = self.parse_entry(&zip_file)?;

            // CRC32 from central directory metadata (0 is valid — it's the CRC32 of empty content)
            if entry.entry_type == EntryType::File {
                entry.crc32 = Some(zip_file.crc32());
            }

            entry.id = i;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Parse ZIP entry into ArchiveEntry
    fn parse_entry(&self, zip_file: &zip::read::ZipFile) -> Result<ArchiveEntry> {
        // Normalize path separators for consistency with Piz backend
        let path = zip_file.name().replace('\\', "/");
        let is_dir = zip_file.is_dir();

        let entry_type = if is_dir {
            EntryType::Directory
        } else if zip_file.is_symlink() {
            EntryType::Symlink
        } else {
            EntryType::File
        };

        let size = if is_dir { None } else { Some(zip_file.size()) };

        let compressed_size = if is_dir {
            None
        } else {
            Some(zip_file.compressed_size())
        };

        // Last modified time - convert DateTime fields to Unix timestamp manually
        // (zip crate's to_time() requires the "time" feature which we don't enable)
        let modified = zip_file.last_modified().and_then(|dt| {
            super::common::ymd_hms_to_system_time(
                dt.year() as u64,
                dt.month() as u64,
                dt.day() as u64,
                dt.hour() as u64,
                dt.minute() as u64,
                dt.second() as u64,
            )
        });

        let mut entry = ArchiveEntry::new(path, 0);
        entry.entry_type = entry_type;
        entry.size = size;
        entry.compressed_size = compressed_size;
        entry.modified = modified;
        entry.is_encrypted = zip_file.encrypted();

        Ok(entry)
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, false)
    }

    /// Extract a single file by path
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, false)
    }

    /// Extract a single file to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        let mut zip = open_zip(&self.path)?;
        let password = crate::options::password_as_str(&self.password);

        let mut zip_file = open_entry_by_name(&mut zip, file_path, password)?;

        let size = zip_file.size();
        let size_usize = usize::try_from(size).map_err(|_| ArchiveError::OperationBlocked {
            operation: crate::error::ops::EXTRACT_TO_MEMORY.to_string(),
            reason: format!("Entry too large for memory: {} bytes", size),
        })?;
        let mut buffer = Vec::new();
        buffer
            .try_reserve(size_usize)
            .map_err(|_| ArchiveError::OperationBlocked {
                operation: crate::error::ops::EXTRACT_TO_MEMORY.to_string(),
                reason: format!("Failed to allocate {} bytes", size),
            })?;
        zip_file
            .read_to_end(&mut buffer)
            .map_err(|e| ArchiveError::io("read", self.path.clone(), e))?;

        // Verify CRC32 to catch corrupt/tampered payloads
        let expected_crc = zip_file.crc32();
        let actual_crc = crc32fast::hash(&buffer);
        if actual_crc != expected_crc {
            return Err(ArchiveError::corruption(
                file_path,
                format!(
                    "CRC32 mismatch: expected {:08x}, got {:08x}",
                    expected_crc, actual_crc
                ),
            ));
        }

        Ok(buffer)
    }

    /// Extract all files with options (overwrite, verify_crc32)
    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<Vec<ArchiveWarning>> {
        let mut warnings: Vec<ArchiveWarning> = Vec::new();
        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let mut zip = open_zip(&self.path)?;

        // Calculate total size for progress
        let total_bytes: u64 = if progress.is_some() {
            let mut total = 0u64;
            for i in 0..zip.len() {
                if let Ok(f) = zip.by_index(i) {
                    total += f.size();
                }
            }
            total
        } else {
            0
        };

        let mut bytes_processed = 0u64;
        let mut progress = progress;
        let num_entries = zip.len();
        let password = crate::options::password_as_str(&self.password);

        for i in 0..num_entries {
            if let Some(callback) = progress.as_mut() {
                if let ControlFlow::Break(()) =
                    callback.on_progress(bytes_processed, Some(total_bytes))
                {
                    return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                }
            }

            let mut zip_file = open_entry_by_index(&mut zip, i, password)?;

            let entry_path = sanitize_entry_path(zip_file.name(), dest_path)?;

            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
            }

            // Skip symlinks for security
            if zip_file.is_symlink() {
                warnings.push(ArchiveWarning::SkippedSymlink {
                    path: zip_file.name().to_string(),
                    target: None,
                });
                continue;
            }

            if zip_file.is_dir() {
                std::fs::create_dir_all(&entry_path)
                    .map_err(|e| ArchiveError::io("create_dir", entry_path.clone(), e))?;
            } else {
                let expected_crc = if verify_crc32 {
                    Some(zip_file.crc32())
                } else {
                    None
                };

                let entry_name = zip_file.name().to_string();
                let entry_size = zip_file.size();
                let mut output_file = AtomicOutputFile::create(&entry_path, overwrite)?;

                copy_with_optional_crc(
                    &mut zip_file,
                    output_file.file_mut(),
                    expected_crc,
                    &entry_name,
                    &entry_path,
                )?;
                output_file.commit()?;

                bytes_processed += entry_size;
            }
        }

        if let Some(callback) = progress.as_mut() {
            let _ = callback.on_progress(total_bytes, Some(total_bytes));
        }

        Ok(warnings)
    }

    /// Extract a single file with options (overwrite, verify_crc32)
    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let mut zip = open_zip(&self.path)?;
        let password = crate::options::password_as_str(&self.password);

        let mut zip_file = open_entry_by_name(&mut zip, file_path, password)?;

        let output_path = sanitize_entry_path(zip_file.name(), dest_path)?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
        }

        let expected_crc = if verify_crc32 {
            Some(zip_file.crc32())
        } else {
            None
        };

        let mut output_file = AtomicOutputFile::create(&output_path, overwrite)?;

        copy_with_optional_crc(
            &mut zip_file,
            output_file.file_mut(),
            expected_crc,
            file_path,
            &output_path,
        )?;
        output_file.commit()?;

        Ok(())
    }

    /// Test integrity of all entries by verifying CRC32
    ///
    /// Opens the ZIP once and stream-verifies each file entry in a single pass.
    /// IO/password errors propagate; only CRC mismatches are collected as failures.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut zip = open_zip(&self.path)?;
        let mut failed = Vec::new();
        let password = crate::options::password_as_str(&self.password);

        for i in 0..zip.len() {
            let mut zip_file = open_entry_by_index(&mut zip, i, password)?;

            if zip_file.is_dir() {
                continue;
            }

            let expected_crc = zip_file.crc32();
            let path = zip_file.name().to_string();
            let actual_crc = compute_crc32_reader(&mut zip_file, &self.path)?;

            if actual_crc != expected_crc {
                failed.push(path);
            }
        }

        Ok(failed)
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// True streaming would require holding a borrow on the ZipArchive reader,
    /// which conflicts with the ownership model.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use std::io::Cursor;

        // For now, use extract_to_memory and wrap in cursor
        // TODO: Implement true streaming with ZipFile reader
        let data = self.extract_to_memory(file_path)?;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));

        Ok(crate::streaming::StreamingExtractor::new(
            reader,
            Some(size),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    #[test]
    fn test_zip_wrapper_open_valid() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path);
        assert!(archive.is_ok());
        assert_eq!(archive.unwrap().path(), path);
    }

    #[test]
    fn test_zip_wrapper_open_nonexistent_list_fails() {
        // open() succeeds (lazy open), but list_files() fails when file doesn't exist
        let archive = ZipArchive::open("/nonexistent/archive.zip").unwrap();
        let result = archive.list_files();
        assert!(result.is_err());
    }

    #[test]
    fn test_zip_wrapper_open_with_password() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open_with_password(&path, "secret");
        assert!(archive.is_ok());
        // Password should be stored
        let archive = archive.unwrap();
        assert_eq!(archive.path(), path);
    }

    #[test]
    fn test_zip_wrapper_list_files() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files();
        assert!(entries.is_ok());
        let entries = entries.unwrap();
        assert!(!entries.is_empty());

        for entry in &entries {
            assert!(!entry.path.is_empty());
            // zip crate provides CRC32 from metadata
            if entry.entry_type == EntryType::File {
                assert!(
                    entry.crc32.is_some(),
                    "File entry '{}' should have CRC32 from zip metadata",
                    entry.path
                );
            }
        }
    }

    #[test]
    fn test_zip_wrapper_extract_to_memory() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let data = archive.extract_to_memory("test_file.txt");
        assert!(data.is_ok());
        assert!(!data.unwrap().is_empty());
    }

    #[test]
    fn test_zip_wrapper_extract_to_memory_nonexistent() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let result = archive.extract_to_memory("does_not_exist.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_zip_wrapper_extract_to_stream() {
        use std::io::Read;
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_zip_wrapper_memory_and_stream_same_data() {
        use std::io::Read;
        let path = fixture("test.zip");

        let archive1 = ZipArchive::open(&path).unwrap();
        let mem_data = archive1.extract_to_memory("test_file.txt").unwrap();

        let archive2 = ZipArchive::open(&path).unwrap();
        let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
        let mut stream_data = Vec::new();
        stream.read_to_end(&mut stream_data).unwrap();

        assert_eq!(mem_data, stream_data);
    }
}
