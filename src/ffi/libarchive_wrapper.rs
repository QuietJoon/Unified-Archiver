//! Safe wrapper for libarchive operations

use super::common::TempDirGuard;
use super::libarchive::*;
use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, Result};
use crate::options::{ProgressCallback, RateLimiter};
use crate::security::sanitize_entry_path;
use std::ffi::{CStr, CString, c_void};
use std::ops::ControlFlow;
use std::os::raw::c_longlong;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Safe wrapper for libarchive
pub struct LibarchiveArchive {
    path: String,
    write_handle: Option<*mut Archive>, // For creation mode
}

impl LibarchiveArchive {
    /// Open an archive with libarchive
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Verify file exists by attempting to open
        let c_path = CString::new(path_str.clone())
            .map_err(|_| ArchiveError::invalid_path(&path_str, "Contains null byte"))?;

        unsafe {
            let archive = archive_read_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    None,
                    "Failed to create libarchive instance",
                ));
            }

            // Enable all formats and filters
            archive_read_support_format_all(archive);
            archive_read_support_filter_all(archive);

            // Try to open the file
            let result = archive_read_open_filename(archive, c_path.as_ptr(), 10240);

            if result != ARCHIVE_OK {
                let error_msg = if !archive.is_null() {
                    let err_ptr = archive_error_string(archive);
                    if !err_ptr.is_null() {
                        CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                    } else {
                        "Unknown error".to_string()
                    }
                } else {
                    "Failed to open archive".to_string()
                };

                archive_read_free(archive);
                return Err(ArchiveError::format(None, error_msg));
            }

            // Close immediately - we'll reopen for each operation
            archive_read_free(archive);
        }

        Ok(Self {
            path: path_str,
            write_handle: None,
        })
    }

    /// List all files in the archive (metadata only, no CRC32 computation)
    ///
    /// This method returns entry metadata without computing CRC32.
    /// Use this for operations that don't need CRC32 (e.g., limit checks).
    pub fn list_files_metadata_only(&self) -> Result<Vec<ArchiveEntry>> {
        self.list_files_internal(false)
    }

    /// List all files in the archive
    ///
    /// For formats where CRC32 is not exposed by libarchive (ZIP, 7z, TAR, etc.),
    /// this method computes CRC32 by reading the file data during listing.
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        self.list_files_internal(true)
    }

    fn list_files_internal(&self, compute_crc: bool) -> Result<Vec<ArchiveEntry>> {
        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        unsafe {
            let archive = archive_read_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    None,
                    "Failed to create libarchive instance",
                ));
            }

            archive_read_support_format_all(archive);
            archive_read_support_filter_all(archive);

            let result = archive_read_open_filename(archive, c_path.as_ptr(), 10240);
            if result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_read_free(archive);
                return Err(ArchiveError::format(None, error_msg));
            }

            let mut entries = Vec::new();
            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut index = 0;

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK {
                    let error_msg = get_archive_error(archive);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                if let Some(mut entry) = parse_entry(entry_ptr) {
                    // Compute CRC32 by reading file data (for files only) if requested
                    if compute_crc && entry.entry_type == EntryType::File && entry.size.is_some() {
                        let mut hasher = crc32fast::Hasher::new();
                        let mut buff_ptr: *const c_void = std::ptr::null();
                        let mut size: usize = 0;
                        let mut offset: c_longlong = 0;

                        loop {
                            let r = archive_read_data_block(
                                archive,
                                &mut buff_ptr,
                                &mut size,
                                &mut offset,
                            );

                            if r == ARCHIVE_EOF {
                                break;
                            } else if r != ARCHIVE_OK {
                                let error_msg = get_archive_error(archive);
                                if error_msg.contains("checksum") || error_msg.contains("CRC") {
                                    archive_read_free(archive);
                                    return Err(ArchiveError::corruption(
                                        self.path.clone(),
                                        format!(
                                            "Checksum verification failed during listing: {}",
                                            error_msg
                                        ),
                                    ));
                                }
                                // For other errors, skip CRC32 computation
                                break;
                            }

                            if !buff_ptr.is_null() && size > 0 {
                                let data_slice =
                                    std::slice::from_raw_parts(buff_ptr as *const u8, size);
                                hasher.update(data_slice);
                            }
                        }

                        entry.crc32 = Some(hasher.finalize());
                    } else {
                        // Skip file data for directories, other entry types, or metadata-only mode
                        archive_read_data_skip(archive);
                    }

                    entry.id = index;
                    entries.push(entry);
                    index += 1;
                } else {
                    // Skip file data if entry parsing failed
                    archive_read_data_skip(archive);
                }
            }

            archive_read_free(archive);
            Ok(entries)
        }
    }

    /// Extract all files to destination
    ///
    /// Phase 1: Added progress callback support with cancellation
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<()> {
        self.extract_all_with_options(dest_path, progress, true, true, true)
    }

    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
    ) -> Result<()> {
        // Calculate total size for progress tracking
        let (total_bytes, _entries) = if progress.is_some() {
            let entries = self.list_files()?;
            let total: u64 = entries.iter().filter_map(|e| e.size).sum();
            (total, entries)
        } else {
            (0, Vec::new())
        };

        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        unsafe {
            let archive = archive_read_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    None,
                    "Failed to create libarchive instance",
                ));
            }

            archive_read_support_format_all(archive);
            archive_read_support_filter_all(archive);

            let result = archive_read_open_filename(archive, c_path.as_ptr(), 10240);
            if result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_read_free(archive);
                return Err(ArchiveError::format(None, error_msg));
            }

            // Create disk writer
            let ext = archive_write_disk_new();
            if ext.is_null() {
                archive_read_free(archive);
                return Err(ArchiveError::format(None, "Failed to create disk writer"));
            }

            let flags = extract_flags(overwrite, preserve_permissions, preserve_times);

            archive_write_disk_set_options(ext, flags);

            // Extract all entries
            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut bytes_processed = 0u64;
            let mut rate_limiter = RateLimiter::new();

            loop {
                // Check cancellation before processing next entry
                if let Some(callback) = progress.as_mut() {
                    if rate_limiter.should_call() {
                        if let ControlFlow::Break(()) =
                            callback.on_progress(bytes_processed, Some(total_bytes))
                        {
                            archive_write_free(ext);
                            archive_read_free(archive);
                            return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                        }
                    }
                }

                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK {
                    let error_msg = get_archive_error(archive);
                    archive_write_free(ext);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // Skip symlinks and hard links for security (FR-022)
                // Symlinks and hard links can be used for path traversal attacks
                if !entry_ptr.is_null() {
                    let filetype = archive_entry_filetype(entry_ptr);
                    if (filetype & AE_IFMT) == AE_IFLNK {
                        archive_read_data_skip(archive);
                        continue;
                    }
                    // Hard links are detected by archive_entry_hardlink() returning non-NULL
                    let hardlink_ptr = archive_entry_hardlink(entry_ptr);
                    if !hardlink_ptr.is_null() {
                        archive_read_data_skip(archive);
                        continue;
                    }
                }

                // Get entry size for progress tracking
                let entry_size = if !entry_ptr.is_null() {
                    let size = archive_entry_size(entry_ptr);
                    if size >= 0 { size as u64 } else { 0 }
                } else {
                    0
                };

                // Update pathname to be relative to destination (with path traversal protection)
                if !entry_ptr.is_null() {
                    let pathname_ptr = archive_entry_pathname(entry_ptr);
                    if !pathname_ptr.is_null() {
                        let pathname = CStr::from_ptr(pathname_ptr).to_string_lossy();
                        let full_path = match sanitize_entry_path(pathname.as_ref(), dest_path) {
                            Ok(path) => path,
                            Err(err) => {
                                archive_write_free(ext);
                                archive_read_free(archive);
                                return Err(err);
                            }
                        };
                        let full_path_str = full_path.to_string_lossy();
                        let full_path_cstr = match CString::new(full_path_str.as_bytes()) {
                            Ok(cstr) => cstr,
                            Err(_) => {
                                archive_write_free(ext);
                                archive_read_free(archive);
                                return Err(ArchiveError::invalid_path(
                                    full_path_str.as_ref(),
                                    "Contains null byte",
                                ));
                            }
                        };

                        archive_entry_set_pathname(entry_ptr, full_path_cstr.as_ptr());
                    }
                }

                // Write header
                let r = archive_write_header(ext, entry_ptr);
                if r != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    archive_write_free(ext);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // Copy data
                copy_data(archive, ext)?;

                // Finish entry
                let finish_result = archive_write_finish_entry(ext);
                if finish_result != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    archive_write_free(ext);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // Update progress after extraction
                if progress.is_some() {
                    bytes_processed += entry_size;
                }
            }

            // Final progress update (100%)
            if let Some(callback) = progress.as_mut() {
                let _ = callback.on_progress(total_bytes, Some(total_bytes));
            }

            archive_write_free(ext);
            archive_read_free(archive);
            Ok(())
        }
    }

    /// Extract a single file
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, true, true)
    }

    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
    ) -> Result<()> {
        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        unsafe {
            let archive = archive_read_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    None,
                    "Failed to create libarchive instance",
                ));
            }

            archive_read_support_format_all(archive);
            archive_read_support_filter_all(archive);

            let result = archive_read_open_filename(archive, c_path.as_ptr(), 10240);
            if result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_read_free(archive);
                return Err(ArchiveError::format(None, error_msg));
            }

            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut found = false;

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK {
                    let error_msg = get_archive_error(archive);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                if !entry_ptr.is_null() {
                    let pathname_ptr = archive_entry_pathname(entry_ptr);
                    if !pathname_ptr.is_null() {
                        let pathname = CStr::from_ptr(pathname_ptr).to_string_lossy();

                        if pathname == file_path {
                            found = true;

                            // Create disk writer
                            let ext = archive_write_disk_new();
                            if ext.is_null() {
                                archive_read_free(archive);
                                return Err(ArchiveError::format(
                                    None,
                                    "Failed to create disk writer",
                                ));
                            }

                            let flags =
                                extract_flags(overwrite, preserve_permissions, preserve_times);
                            archive_write_disk_set_options(ext, flags);

                            // Update pathname (with path traversal protection)
                            let full_path = match sanitize_entry_path(file_path, dest_path) {
                                Ok(path) => path,
                                Err(err) => {
                                    archive_write_free(ext);
                                    archive_read_free(archive);
                                    return Err(err);
                                }
                            };
                            let full_path_str = full_path.to_string_lossy();
                            let full_path_cstr = match CString::new(full_path_str.as_bytes()) {
                                Ok(cstr) => cstr,
                                Err(_) => {
                                    archive_write_free(ext);
                                    archive_read_free(archive);
                                    return Err(ArchiveError::invalid_path(
                                        full_path_str.as_ref(),
                                        "Contains null byte",
                                    ));
                                }
                            };

                            archive_entry_set_pathname(entry_ptr, full_path_cstr.as_ptr());

                            // Write header
                            let header_result = archive_write_header(ext, entry_ptr);
                            if header_result != ARCHIVE_OK {
                                let error_msg = get_archive_error(ext);
                                archive_write_free(ext);
                                archive_read_free(archive);
                                return Err(ArchiveError::format(None, error_msg));
                            }

                            // Copy data
                            copy_data(archive, ext)?;

                            // Finish
                            let finish_result = archive_write_finish_entry(ext);
                            if finish_result != ARCHIVE_OK {
                                let error_msg = get_archive_error(ext);
                                archive_write_free(ext);
                                archive_read_free(archive);
                                return Err(ArchiveError::format(None, error_msg));
                            }
                            archive_write_free(ext);
                            break;
                        }
                    }
                }

                archive_read_data_skip(archive);
            }

            archive_read_free(archive);

            if !found {
                return Err(ArchiveError::format(
                    None,
                    format!("File '{}' not found in archive", file_path),
                ));
            }

            Ok(())
        }
    }

    /// Extract to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        use std::io::Read;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Use system temp directory (respects TMPDIR/TEMP environment variables)
        // Include timestamp to avoid conflicts between parallel tests
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        let temp_base = std::env::temp_dir();
        let temp_dir = temp_base.join(format!(
            "libarchive_mem_{}_{}",
            std::process::id(),
            timestamp
        ));

        // Ensure temp directory exists
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| ArchiveError::io("create_temp_dir", temp_dir.clone(), e))?;

        // RAII guard ensures cleanup on all exit paths (success or error)
        let _guard = TempDirGuard::new(temp_dir.clone());

        // Extract to temp
        self.extract_file(file_path, &temp_dir)?;

        // Read into memory (use sanitized path to locate extracted file)
        let extracted_path = sanitize_entry_path(file_path, &temp_dir)?;
        let mut file = std::fs::File::open(&extracted_path)
            .map_err(|e| ArchiveError::io("open_extracted", extracted_path.clone(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("stat_extracted", extracted_path.clone(), e))?;
        let len = metadata.len();

        if len > usize::MAX as u64 {
            return Err(ArchiveError::UnsupportedOperation {
                operation: "extract_to_memory".to_string(),
                reason: format!(
                    "File '{}' is too large to buffer in memory: {} bytes",
                    file_path, len
                ),
            });
        }

        let mut buffer = Vec::new();
        buffer
            .try_reserve(len as usize)
            .map_err(|_| ArchiveError::UnsupportedOperation {
                operation: "extract_to_memory".to_string(),
                reason: format!("Unable to allocate {} bytes for file '{}'", len, file_path),
            })?;

        file.read_to_end(&mut buffer)
            .map_err(|e| ArchiveError::io("read_extracted", extracted_path.clone(), e))?;

        // Guard handles cleanup on drop
        Ok(buffer)
    }

    /// Get archive path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Extract a single file to a stream (Phase 2.4)
    ///
    /// Returns a StreamingExtractor that reads data directly from the archive
    /// without loading the entire file into memory.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use std::io::Cursor;

        // For now, use extract_to_memory and wrap in a cursor
        // TODO: Implement true streaming with archive handle
        let data = self.extract_to_memory(file_path)?;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));

        Ok(crate::streaming::StreamingExtractor::new(
            reader,
            Some(size),
        ))
    }

    /// Create a new archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        format: crate::ArchiveFormat,
        options: &crate::options::CompressionOptions,
    ) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let c_path = CString::new(path_str.clone())
            .map_err(|_| ArchiveError::invalid_path(&path_str, "Contains null byte"))?;

        unsafe {
            let archive = archive_write_new();
            if archive.is_null() {
                return Err(ArchiveError::format(
                    Some(format),
                    "Failed to create write archive handle",
                ));
            }

            // Set format
            let format_result = match format {
                crate::ArchiveFormat::Zip => archive_write_set_format_zip(archive),
                crate::ArchiveFormat::SevenZip => archive_write_set_format_7zip(archive),
                crate::ArchiveFormat::Tar => archive_write_set_format_pax_restricted(archive),
                crate::ArchiveFormat::TarGzip => {
                    archive_write_set_format_pax_restricted(archive);
                    archive_write_add_filter_gzip(archive)
                }
                crate::ArchiveFormat::TarBzip2 => {
                    archive_write_set_format_pax_restricted(archive);
                    archive_write_add_filter_bzip2(archive)
                }
                crate::ArchiveFormat::TarXz => {
                    archive_write_set_format_pax_restricted(archive);
                    archive_write_add_filter_xz(archive)
                }
                _ => {
                    archive_write_free(archive);
                    return Err(ArchiveError::UnsupportedOperation {
                        operation: format!("create {:?}", format),
                        reason: "Format not supported for creation".to_string(),
                    });
                }
            };

            if format_result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_write_free(archive);
                return Err(ArchiveError::format(Some(format), error_msg));
            }

            // Set compression level - only for TAR with filters, not for ZIP/7z
            match format {
                crate::ArchiveFormat::TarGzip
                | crate::ArchiveFormat::TarBzip2
                | crate::ArchiveFormat::TarXz => {
                    use crate::options::CompressionLevel;
                    let compression_value = match options.level {
                        CompressionLevel::Store => "0",
                        CompressionLevel::Fastest => "1",
                        CompressionLevel::Fast => "3",
                        CompressionLevel::Normal => "5",
                        CompressionLevel::Maximum => "7",
                        CompressionLevel::Ultra => "9",
                    };

                    // Safety: These are static strings without null bytes
                    if let (Ok(compression_opt), Ok(compression_val)) = (
                        CString::new("compression-level"),
                        CString::new(compression_value),
                    ) {
                        archive_write_set_filter_option(
                            archive,
                            std::ptr::null(),
                            compression_opt.as_ptr(),
                            compression_val.as_ptr(),
                        );
                    }
                }
                _ => {
                    // ZIP and 7z handle compression differently, skip filter options
                }
            }

            // Set password if provided
            if let Some(ref password) = options.password {
                let c_password = CString::new(password.as_str())
                    .map_err(|_| ArchiveError::password("Password contains null byte"))?;
                let pass_result = archive_write_set_passphrase(archive, c_password.as_ptr());
                if pass_result != ARCHIVE_OK {
                    let error_msg = get_archive_error(archive);
                    archive_write_free(archive);
                    return Err(ArchiveError::password(error_msg));
                }
            }

            // Open output file
            let open_result = archive_write_open_filename(archive, c_path.as_ptr());
            if open_result != ARCHIVE_OK {
                let error_msg = get_archive_error(archive);
                archive_write_free(archive);
                return Err(ArchiveError::io(
                    "write_open",
                    path_str.clone(),
                    std::io::Error::other(error_msg),
                ));
            }

            Ok(Self {
                path: path_str,
                write_handle: Some(archive),
            })
        }
    }

    /// Add a file to the archive from byte data
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::UnsupportedOperation {
                operation: "add_file_from_data".to_string(),
                reason: "Archive not opened in write mode".to_string(),
            })?;

        unsafe {
            // Create entry
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Set pathname
            let c_path = CString::new(archive_path)
                .map_err(|_| ArchiveError::invalid_path(archive_path, "Contains null byte"))?;
            archive_entry_set_pathname(entry, c_path.as_ptr());

            // Set file type and size
            archive_entry_set_filetype(entry, AE_IFREG);
            archive_entry_set_size(entry, data.len() as c_longlong);
            archive_entry_set_perm(entry, 0o644);

            // Set modification time to now
            let now = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            archive_entry_set_mtime(entry, now.as_secs() as i64, 0);

            // Write header
            let header_result = archive_write_header(write_handle, entry);
            if header_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }

            // Write data
            if !data.is_empty() {
                let bytes_written =
                    archive_write_data(write_handle, data.as_ptr() as *const c_void, data.len());
                if bytes_written < 0 {
                    let error_msg = get_archive_error(write_handle);
                    archive_entry_free(entry);
                    return Err(ArchiveError::io(
                        "write_data",
                        archive_path,
                        std::io::Error::other(error_msg),
                    ));
                }
            }

            // Finish entry
            archive_write_finish_entry(write_handle);
            archive_entry_free(entry);
        }

        Ok(())
    }

    /// Close and finalize the archive (for write mode)
    pub fn close_write(&mut self) -> Result<()> {
        if let Some(write_handle) = self.write_handle.take() {
            unsafe {
                let close_result = archive_write_close(write_handle);
                archive_write_free(write_handle);

                if close_result != ARCHIVE_OK {
                    return Err(ArchiveError::io(
                        "close",
                        self.path.clone(),
                        std::io::Error::other("Failed to close archive"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Add a file from filesystem path to the archive
    pub fn add_file_from_path(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        use std::fs;
        use std::io::Read;

        // Read file from filesystem
        let mut file = fs::File::open(fs_path.as_ref())
            .map_err(|e| ArchiveError::io("open", fs_path.as_ref(), e))?;

        // Get metadata
        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("metadata", fs_path.as_ref(), e))?;

        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::UnsupportedOperation {
                operation: "add_file_from_path".to_string(),
                reason: "Archive not opened in write mode".to_string(),
            })?;

        unsafe {
            // Create entry
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Set pathname
            let c_path = CString::new(archive_path)
                .map_err(|_| ArchiveError::invalid_path(archive_path, "Contains null byte"))?;
            archive_entry_set_pathname(entry, c_path.as_ptr());

            // Set file type and size
            archive_entry_set_filetype(entry, AE_IFREG);
            archive_entry_set_size(entry, metadata.len() as c_longlong);

            // Set permissions from filesystem
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = metadata.permissions().mode();
                archive_entry_set_perm(entry, mode as i32);
            }
            #[cfg(not(unix))]
            {
                archive_entry_set_perm(entry, 0o644);
            }

            // Set modification time from filesystem
            if let Ok(mtime) = metadata.modified() {
                if let Ok(duration) = mtime.duration_since(UNIX_EPOCH) {
                    archive_entry_set_mtime(
                        entry,
                        duration.as_secs() as i64,
                        duration.subsec_nanos() as c_longlong,
                    );
                }
            }

            // Write header
            let header_result = archive_write_header(write_handle, entry);
            if header_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }

            // Write data using streaming buffer
            let mut buffer = [0u8; 8192];
            loop {
                let bytes_read = file
                    .read(&mut buffer)
                    .map_err(|e| ArchiveError::io("read", fs_path.as_ref(), e))?;

                if bytes_read == 0 {
                    break;
                }

                let bytes_written =
                    archive_write_data(write_handle, buffer.as_ptr() as *const c_void, bytes_read);
                if bytes_written < 0 {
                    let error_msg = get_archive_error(write_handle);
                    archive_entry_free(entry);
                    return Err(ArchiveError::io(
                        "write_data",
                        archive_path,
                        std::io::Error::other(error_msg),
                    ));
                }
            }

            // Finish entry
            archive_write_finish_entry(write_handle);
            archive_entry_free(entry);
        }

        Ok(())
    }

    /// Add directory recursively to archive
    pub fn add_directory_recursive(&mut self, dir_path: impl AsRef<Path>) -> Result<()> {
        let dir_path = dir_path.as_ref();
        if !dir_path.is_dir() {
            return Err(ArchiveError::invalid_path(
                dir_path.to_string_lossy().as_ref(),
                "Not a directory",
            ));
        }

        // Walk directory tree
        let entries = walkdir::WalkDir::new(dir_path).into_iter();

        let base_path = dir_path.parent().unwrap_or(dir_path);

        for entry in entries {
            let entry = entry.map_err(|e| {
                let path = e
                    .path()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| dir_path.to_path_buf());
                let io_error = e
                    .io_error()
                    .map(|err| std::io::Error::new(err.kind(), err.to_string()))
                    .unwrap_or_else(|| std::io::Error::other(e.to_string()));
                ArchiveError::io("walk", path, io_error)
            })?;

            let path = entry.path();
            if path.is_file() {
                // Compute relative path within archive
                let archive_path = path
                    .strip_prefix(base_path)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();

                // Add file
                self.add_file_from_path(path, &archive_path)?;
            }
        }

        Ok(())
    }

    /// Test archive integrity by verifying checksums for all files
    ///
    /// Extracts each file to memory and verifies checksums (CRC32 for TAR, etc.).
    /// Returns a list of file paths that failed verification.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut failed_files = Vec::new();

        // Get all entries
        let entries = self.list_files()?;

        for entry in entries.iter() {
            // Skip directories
            if entry.is_directory() {
                continue;
            }

            // Try to extract - libarchive computes checksums during extraction
            match self.extract_to_memory(&entry.path) {
                Ok(_) => {
                    // Checksum verification happens during extraction
                    // If we got here, verification passed
                }
                Err(_) => {
                    // Extraction/verification failed
                    failed_files.push(entry.path.clone());
                }
            }
        }

        Ok(failed_files)
    }
}

fn extract_flags(
    overwrite: bool,
    preserve_permissions: bool,
    preserve_times: bool,
) -> std::os::raw::c_int {
    let mut flags = ARCHIVE_EXTRACT_SECURE_SYMLINKS | ARCHIVE_EXTRACT_SECURE_NODOTDOT;
    if preserve_times {
        flags |= ARCHIVE_EXTRACT_TIME;
    }
    if preserve_permissions {
        flags |= ARCHIVE_EXTRACT_PERM;
    }
    if !overwrite {
        flags |= ARCHIVE_EXTRACT_NO_OVERWRITE;
    }
    flags
}

/// Copy data from read archive to write archive
unsafe fn copy_data(ar: *mut Archive, aw: *mut Archive) -> Result<()> {
    let mut buff: *const c_void = std::ptr::null();
    let mut size: usize = 0;
    let mut offset: c_longlong = 0;

    loop {
        let r = unsafe { archive_read_data_block(ar, &mut buff, &mut size, &mut offset) };

        if r == ARCHIVE_EOF {
            return Ok(());
        }

        if r != ARCHIVE_OK {
            let error_msg = unsafe { get_archive_error(ar) };
            // Phase 2.5: Detect corruption/checksum errors from libarchive
            if error_msg.contains("checksum")
                || error_msg.contains("CRC")
                || error_msg.contains("Checksum")
            {
                return Err(ArchiveError::corruption(
                    "archive".to_string(),
                    format!("Checksum verification failed: {}", error_msg),
                ));
            }
            return Err(ArchiveError::format(None, error_msg));
        }

        let r = unsafe { archive_write_data_block(aw, buff, size, offset) };

        if r != ARCHIVE_OK {
            let error_msg = unsafe { get_archive_error(aw) };
            return Err(ArchiveError::format(None, error_msg));
        }
    }
}

/// Parse libarchive entry into ArchiveEntry
unsafe fn parse_entry(entry: *mut LibarchiveEntry) -> Option<ArchiveEntry> {
    if entry.is_null() {
        return None;
    }

    let pathname_ptr = unsafe { archive_entry_pathname(entry) };
    if pathname_ptr.is_null() {
        return None;
    }

    let path = unsafe { CStr::from_ptr(pathname_ptr) }
        .to_string_lossy()
        .into_owned();

    let size = unsafe { archive_entry_size(entry) };
    let mtime = unsafe { archive_entry_mtime(entry) };
    let mode = unsafe { archive_entry_mode(entry) };
    let filetype = unsafe { archive_entry_filetype(entry) };

    // Check for hard links first (archive_entry_hardlink returns non-NULL for hard links)
    let hardlink_ptr = unsafe { archive_entry_hardlink(entry) };
    let entry_type = if !hardlink_ptr.is_null() {
        EntryType::HardLink
    } else {
        match filetype & AE_IFMT {
            AE_IFREG => EntryType::File,
            AE_IFDIR => EntryType::Directory,
            AE_IFLNK => EntryType::Symlink,
            _ => EntryType::Other,
        }
    };

    let modified = if mtime > 0 {
        Some(UNIX_EPOCH + std::time::Duration::from_secs(mtime as u64))
    } else {
        None
    };

    let permissions = if mode > 0 { Some(mode as u32) } else { None };

    // Phase 1: Enhanced metadata

    // Creation time (birthtime)
    let birthtime = unsafe { archive_entry_birthtime(entry) };
    let created = if birthtime > 0 {
        Some(UNIX_EPOCH + std::time::Duration::from_secs(birthtime as u64))
    } else {
        None
    };

    // Access time (atime)
    let atime = unsafe { archive_entry_atime(entry) };
    let accessed = if atime > 0 {
        Some(UNIX_EPOCH + std::time::Duration::from_secs(atime as u64))
    } else {
        None
    };

    // Encryption status
    let is_encrypted = unsafe { archive_entry_is_encrypted(entry) } != 0;

    // Note: CRC32 is not directly exposed through libarchive's generic API.
    // It is computed during list_files() by reading and hashing the file data.

    let mut arch_entry = ArchiveEntry::new(path, 0);
    arch_entry.entry_type = entry_type;
    arch_entry.size = if size >= 0 { Some(size as u64) } else { None };
    arch_entry.compressed_size = None; // libarchive doesn't expose this easily
    arch_entry.modified = modified;
    arch_entry.permissions = permissions;
    arch_entry.crc32 = None; // Computed later in list_files() by reading file data

    // Phase 1: Enhanced metadata
    arch_entry.created = created;
    arch_entry.accessed = accessed;
    arch_entry.is_encrypted = is_encrypted;
    arch_entry.comment = None; // Not available in libarchive generic API
    arch_entry.attributes = None; // No platform-specific attributes available

    // Compute compression ratio if sizes available
    arch_entry.compute_compression_ratio();

    Some(arch_entry)
}

/// Get error message from archive
unsafe fn get_archive_error(archive: *mut Archive) -> String {
    if archive.is_null() {
        return "Archive pointer is null".to_string();
    }

    let err_ptr = unsafe { archive_error_string(archive) };
    if err_ptr.is_null() {
        return "Unknown error".to_string();
    }

    unsafe { CStr::from_ptr(err_ptr) }
        .to_string_lossy()
        .into_owned()
}
