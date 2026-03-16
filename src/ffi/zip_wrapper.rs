//! Native Rust ZIP backend using the `zip` crate
//!
//! This backend provides fast ZIP operations with direct CRC32 access from metadata.
//! Falls back to computing CRC32 when null/missing in the archive.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::options::ProgressCallback;
use crate::security::sanitize_entry_path;
use std::fs::File;
use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive as RawZipArchive;

/// Convert zip crate DateTime to SystemTime without requiring the "time" feature
fn datetime_to_system_time(dt: zip::DateTime) -> Option<SystemTime> {
    // Days from year 0 to Unix epoch (1970-01-01)
    // Use a simplified calculation: days in each month (non-leap default)
    let days_in_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let year = dt.year() as u64;

    if year < 1970 {
        return None;
    }

    // Count days from 1970 to the given year
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Add days for months in the current year
    let month = dt.month() as usize;
    for m in 1..month {
        days += days_in_month[m - 1];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }

    // Add day of month (1-based)
    days += (dt.day() as u64).saturating_sub(1);

    let secs = days * 86400
        + dt.hour() as u64 * 3600
        + dt.minute() as u64 * 60
        + dt.second() as u64;

    Some(UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Open a ZIP entry by index, using password decryption if provided
fn open_entry_by_index<'a>(
    zip: &'a mut RawZipArchive<File>,
    index: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a>> {
    if let Some(pw) = password {
        zip.by_index_decrypt(index, pw.as_bytes()).map_err(|e| {
            if matches!(e, zip::result::ZipError::InvalidPassword) {
                ArchiveError::Password {
                    message: format!("Invalid password for ZIP entry {}", index),
                }
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("Read entry {}: {}", index, e),
                )
            }
        })
    } else {
        zip.by_index(index).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry {}: {}", index, e))
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
                ArchiveError::Password {
                    message: format!("Invalid password for ZIP entry '{}'", name),
                }
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("File '{}' not found in archive", name),
                )
            }
        })
    } else {
        zip.by_name(name).map_err(|_| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("File '{}' not found in archive", name),
            )
        })
    }
}

/// Native Rust ZIP archive wrapper
///
/// Provides fast ZIP operations with CRC32 from metadata (no decompression needed).
/// Automatically computes CRC32 when missing/null in archive.
pub struct ZipArchive {
    path: PathBuf,
    password: Option<String>,
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
        archive.password = Some(password.to_string());
        Ok(archive)
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// List all files in ZIP archive with CRC32 from metadata
    ///
    /// CRC32 Strategy:
    /// 1. Read CRC32 from ZIP central directory (fast, no decompression)
    /// 2. If CRC32 is null/0, compute it by decompressing (fallback)
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        let mut entries = Vec::new();

        for i in 0..zip.len() {
            let mut zip_file = zip.by_index(i).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Read entry {}: {}", i, e))
            })?;

            // Parse entry metadata
            let mut entry = self.parse_entry(&zip_file)?;

            // CRC32 strategy: read from metadata first, compute if missing
            if entry.entry_type == EntryType::File {
                let metadata_crc = zip_file.crc32();

                if metadata_crc != 0 {
                    // CRC32 available in metadata (common case - fast!)
                    entry.crc32 = Some(metadata_crc);
                } else if !entry.is_encrypted {
                    // CRC32 is null/0 and not encrypted - compute by decompressing
                    entry.crc32 = Some(self.compute_crc32(&mut zip_file)?);
                }
                // else: encrypted with no CRC32 in metadata - leave as None
            }

            entry.id = i;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Compute CRC32 by decompressing file data (fallback for null CRC32)
    fn compute_crc32<R: Read>(&self, reader: &mut R) -> Result<u32> {
        use crc32fast::Hasher;

        let mut hasher = Hasher::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|e| ArchiveError::io("read", self.path.clone(), e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hasher.finalize())
    }

    /// Parse ZIP entry into ArchiveEntry
    fn parse_entry(&self, zip_file: &zip::read::ZipFile) -> Result<ArchiveEntry> {
        let path = zip_file.name().to_string();
        let is_dir = zip_file.is_dir();

        let entry_type = if is_dir {
            EntryType::Directory
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
            datetime_to_system_time(dt)
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
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<()> {
        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

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
        let num_entries = zip.len();

        // Extract each entry
        for i in 0..num_entries {
            // Check cancellation
            if let Some(callback) = progress.as_mut() {
                if let ControlFlow::Break(()) =
                    callback.on_progress(bytes_processed, Some(total_bytes))
                {
                    return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                }
            }

            let mut zip_file = open_entry_by_index(&mut zip, i, self.password.as_deref())?;

            let entry_path = sanitize_entry_path(zip_file.name(), dest_path)?;

            // Create parent directories
            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
            }

            // Extract file or create directory
            if zip_file.is_dir() {
                std::fs::create_dir_all(&entry_path)
                    .map_err(|e| ArchiveError::io("create_dir", entry_path.clone(), e))?;
            } else {
                let mut output_file = File::create(&entry_path)
                    .map_err(|e| ArchiveError::io("create", entry_path.clone(), e))?;

                std::io::copy(&mut zip_file, &mut output_file)
                    .map_err(|e| ArchiveError::io("write", entry_path.clone(), e))?;

                bytes_processed += zip_file.size();
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
        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        // Find the file in archive (with optional password decryption)
        let mut zip_file = open_entry_by_name(&mut zip, file_path, self.password.as_deref())?;

        let output_path = sanitize_entry_path(zip_file.name(), dest_path)?;

        // Create parent directories
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
        }

        // Extract file
        let mut output_file = File::create(&output_path)
            .map_err(|e| ArchiveError::io("create", output_path.clone(), e))?;

        std::io::copy(&mut zip_file, &mut output_file)
            .map_err(|e| ArchiveError::io("write", output_path, e))?;

        Ok(())
    }

    /// Extract a single file to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        let mut zip_file = open_entry_by_name(&mut zip, file_path, self.password.as_deref())?;

        let mut buffer = Vec::with_capacity(zip_file.size() as usize);
        zip_file
            .read_to_end(&mut buffer)
            .map_err(|e| ArchiveError::io("read", self.path.clone(), e))?;

        Ok(buffer)
    }

    /// Extract all files with options (overwrite, verify_crc32)
    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

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

        for i in 0..num_entries {
            if let Some(callback) = progress.as_mut() {
                if let ControlFlow::Break(()) =
                    callback.on_progress(bytes_processed, Some(total_bytes))
                {
                    return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                }
            }

            let mut zip_file = open_entry_by_index(&mut zip, i, self.password.as_deref())?;

            let entry_path = sanitize_entry_path(zip_file.name(), dest_path)?;

            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
            }

            if zip_file.is_dir() {
                std::fs::create_dir_all(&entry_path)
                    .map_err(|e| ArchiveError::io("create_dir", entry_path.clone(), e))?;
            } else {
                if !overwrite && entry_path.exists() {
                    return Err(ArchiveError::UnsupportedOperation {
                        operation: "extract_all".to_string(),
                        reason: format!(
                            "Destination file already exists: '{}'",
                            entry_path.display()
                        ),
                    });
                }

                let mut output_file = File::create(&entry_path)
                    .map_err(|e| ArchiveError::io("create", entry_path.clone(), e))?;

                std::io::copy(&mut zip_file, &mut output_file)
                    .map_err(|e| ArchiveError::io("write", entry_path.clone(), e))?;

                if verify_crc32 {
                    let expected_crc = zip_file.crc32();
                    if expected_crc != 0 {
                        let data = std::fs::read(&entry_path)
                            .map_err(|e| ArchiveError::io("read", entry_path.clone(), e))?;
                        let actual_crc = crc32fast::hash(&data);
                        if actual_crc != expected_crc {
                            return Err(ArchiveError::corruption(
                                entry_path.to_string_lossy(),
                                format!(
                                    "CRC32 mismatch: expected {:08X}, got {:08X}",
                                    expected_crc, actual_crc
                                ),
                            ));
                        }
                    }
                }

                bytes_processed += zip_file.size();
            }
        }

        if let Some(callback) = progress.as_mut() {
            let _ = callback.on_progress(total_bytes, Some(total_bytes));
        }

        Ok(())
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

        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;

        let mut zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;

        let mut zip_file = open_entry_by_name(&mut zip, file_path, self.password.as_deref())?;

        let output_path = sanitize_entry_path(zip_file.name(), dest_path)?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
        }

        if !overwrite && output_path.exists() {
            return Err(ArchiveError::UnsupportedOperation {
                operation: "extract_file".to_string(),
                reason: format!(
                    "Destination file already exists: '{}'",
                    output_path.display()
                ),
            });
        }

        let mut output_file = File::create(&output_path)
            .map_err(|e| ArchiveError::io("create", output_path.clone(), e))?;

        std::io::copy(&mut zip_file, &mut output_file)
            .map_err(|e| ArchiveError::io("write", output_path.clone(), e))?;

        if verify_crc32 {
            let expected_crc = zip_file.crc32();
            if expected_crc != 0 {
                let data = std::fs::read(&output_path)
                    .map_err(|e| ArchiveError::io("read", output_path.clone(), e))?;
                let actual_crc = crc32fast::hash(&data);
                if actual_crc != expected_crc {
                    return Err(ArchiveError::corruption(
                        output_path.to_string_lossy(),
                        format!(
                            "CRC32 mismatch: expected {:08X}, got {:08X}",
                            expected_crc, actual_crc
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Test integrity of all entries by extracting to memory and verifying CRC32
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let entries = self.list_files()?;
        let mut failed = Vec::new();

        for entry in &entries {
            if entry.entry_type == EntryType::File {
                match self.extract_to_memory(&entry.path) {
                    Ok(data) => {
                        if let Some(expected_crc) = entry.crc32 {
                            let actual_crc = crc32fast::hash(&data);
                            if actual_crc != expected_crc {
                                failed.push(entry.path.clone());
                            }
                        }
                    }
                    Err(_) => {
                        failed.push(entry.path.clone());
                    }
                }
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
