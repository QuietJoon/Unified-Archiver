//! Safe wrapper for libarchive operations

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
    progress: Option<Box<dyn crate::options::ProgressCallback>>,
    bytes_written: u64,
    entries_written: usize,
}

impl LibarchiveArchive {
    /// Open a libarchive read handle with all formats/filters enabled
    ///
    /// Encapsulates: archive_read_new, null check, support_format_all,
    /// support_filter_all, open_filename, error check with archive_read_free on failure.
    unsafe fn open_read_handle(c_path: &std::ffi::CStr) -> Result<*mut Archive> {
        let archive = unsafe { archive_read_new() };
        if archive.is_null() {
            return Err(ArchiveError::format(
                None,
                "Failed to create libarchive instance",
            ));
        }

        unsafe { archive_read_support_format_all(archive) };
        // Enable "raw" pseudo-format as lowest-priority fallback: standalone
        // compressed files (.gz, .bz2, .xz) that aren't tar archives are
        // exposed as a single-entry archive.
        unsafe { archive_read_support_format_raw(archive) };
        unsafe { archive_read_support_filter_all(archive) };

        let result = unsafe { archive_read_open_filename(archive, c_path.as_ptr(), 10240) };
        if result != ARCHIVE_OK {
            let error_msg = if !archive.is_null() {
                let err_str = unsafe { archive_error_string(archive) };
                if !err_str.is_null() {
                    unsafe { std::ffi::CStr::from_ptr(err_str) }
                        .to_string_lossy()
                        .to_string()
                } else {
                    format!("libarchive error code: {}", result)
                }
            } else {
                format!("libarchive error code: {}", result)
            };
            unsafe { archive_read_free(archive) };
            return Err(ArchiveError::format(None, error_msg));
        }

        Ok(archive)
    }

    /// Open an archive with libarchive
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Verify file exists by attempting to open
        let c_path = CString::new(path_str.clone())
            .map_err(|_| ArchiveError::invalid_path(&path_str, "Contains null byte"))?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;

            // Close immediately - we'll reopen for each operation
            archive_read_free(archive);
        }

        Ok(Self {
            path: path_str,
            write_handle: None,
            progress: None,
            bytes_written: 0,
            entries_written: 0,
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
            let archive = Self::open_read_handle(&c_path)?;

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
                    // Raw format (standalone .gz/.bz2/.xz) returns "data" as the
                    // entry name. Replace with the archive filename sans compression
                    // extension for a meaningful path.
                    if entry.path == "data" {
                        if let Some(stem) = Path::new(&self.path).file_stem() {
                            entry.path = stem.to_string_lossy().to_string();
                        }
                    }

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
            let entries = self.list_files_metadata_only()?;
            let total: u64 = entries.iter().filter_map(|e| e.size).sum();
            (total, entries)
        } else {
            (0, Vec::new())
        };

        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;

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
                        let raw_name = CStr::from_ptr(pathname_ptr).to_string_lossy();
                        // Raw format (standalone .gz/.bz2/.xz) returns "data";
                        // replace with archive filename sans compression extension.
                        let pathname = if raw_name == "data" {
                            Path::new(&self.path)
                                .file_stem()
                                .map(|s| std::borrow::Cow::Owned(s.to_string_lossy().to_string()))
                                .unwrap_or(raw_name)
                        } else {
                            raw_name
                        };
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
            let archive = Self::open_read_handle(&c_path)?;

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

    /// Extract to memory using direct in-memory extraction (no temp files)
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;

            let mut entry: *mut LibarchiveEntry = std::ptr::null_mut();

            // Iterate entries to find the target file
            loop {
                let r = archive_read_next_header(archive, &mut entry);
                if r == ARCHIVE_EOF {
                    break;
                }
                if r != ARCHIVE_OK && r != ARCHIVE_WARN {
                    let error_msg = get_archive_error(archive);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                let pathname = archive_entry_pathname(entry);
                if pathname.is_null() {
                    archive_read_data_skip(archive);
                    continue;
                }

                let entry_name = CStr::from_ptr(pathname).to_string_lossy();
                if entry_name.as_ref() == file_path {
                    // Read data blocks directly into memory
                    let size_hint = archive_entry_size(entry);
                    let size_usize = if size_hint > 0 {
                        match usize::try_from(size_hint) {
                            Ok(s) => Some(s),
                            Err(_) => {
                                archive_read_free(archive);
                                return Err(ArchiveError::OperationBlocked {
                                    operation: "extract_to_memory".to_string(),
                                    reason: format!(
                                        "Entry too large for memory: {} bytes",
                                        size_hint
                                    ),
                                });
                            }
                        }
                    } else {
                        None
                    };

                    let mut buffer = Vec::new();
                    if let Some(size) = size_usize {
                        if buffer.try_reserve(size).is_err() {
                            archive_read_free(archive);
                            return Err(ArchiveError::OperationBlocked {
                                operation: "extract_to_memory".to_string(),
                                reason: format!("Failed to allocate {} bytes", size),
                            });
                        }
                    }

                    let mut buff: *const c_void = std::ptr::null();
                    let mut size: usize = 0;
                    let mut offset: c_longlong = 0;

                    loop {
                        let r = archive_read_data_block(archive, &mut buff, &mut size, &mut offset);
                        if r == ARCHIVE_EOF {
                            break;
                        }
                        if r != ARCHIVE_OK {
                            let error_msg = get_archive_error(archive);
                            archive_read_free(archive);
                            return Err(ArchiveError::format(None, error_msg));
                        }

                        if size > 0 && !buff.is_null() {
                            let slice = std::slice::from_raw_parts(buff as *const u8, size);
                            buffer.extend_from_slice(slice);
                        }
                    }

                    archive_read_free(archive);
                    return Ok(buffer);
                }

                archive_read_data_skip(archive);
            }

            archive_read_free(archive);

            Err(ArchiveError::format(
                None,
                format!("File '{}' not found in archive", file_path),
            ))
        }
    }

    /// Get archive path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Extract a single file to a stream (Phase 2.4)
    ///
    /// Returns a StreamingExtractor that reads data directly from the archive
    /// using `archive_read_data`, without loading the entire file into memory.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        let reader = LibarchiveStreamReader::open(&self.path, file_path)?;
        let size = reader.entry_size;
        Ok(crate::streaming::StreamingExtractor::new(
            Box::new(reader),
            size,
        ))
    }

    /// Create a new archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        format: crate::ArchiveFormat,
        options: &mut crate::options::CompressionOptions,
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
                    return Err(ArchiveError::OperationBlocked {
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

            // Set compression level for all supported formats
            {
                use crate::options::CompressionLevel;
                let compression_value = match options.level {
                    CompressionLevel::Store => "0",
                    CompressionLevel::Fastest => "1",
                    CompressionLevel::Fast => "3",
                    CompressionLevel::Normal => "6",
                    CompressionLevel::Maximum => "8",
                    CompressionLevel::Ultra => "9",
                };

                match format {
                    crate::ArchiveFormat::TarGzip
                    | crate::ArchiveFormat::TarBzip2
                    | crate::ArchiveFormat::TarXz => {
                        // TAR variants: set filter compression level
                        if let (Ok(opt), Ok(val)) = (
                            CString::new("compression-level"),
                            CString::new(compression_value),
                        ) {
                            archive_write_set_filter_option(
                                archive,
                                std::ptr::null(),
                                opt.as_ptr(),
                                val.as_ptr(),
                            );
                        }
                    }
                    crate::ArchiveFormat::Zip => {
                        // ZIP: set deflate compression level via format option
                        if let (Ok(opt), Ok(val)) = (
                            CString::new("compression-level"),
                            CString::new(compression_value),
                        ) {
                            archive_write_set_format_option(
                                archive,
                                std::ptr::null(),
                                opt.as_ptr(),
                                val.as_ptr(),
                            );
                        }
                    }
                    crate::ArchiveFormat::SevenZip => {
                        // 7z: set LZMA compression level via format option
                        if let (Ok(opt), Ok(val)) = (
                            CString::new("compression-level"),
                            CString::new(compression_value),
                        ) {
                            archive_write_set_format_option(
                                archive,
                                std::ptr::null(),
                                opt.as_ptr(),
                                val.as_ptr(),
                            );
                        }
                    }
                    _ => {}
                }
            }

            // Set password if provided
            if let Some(password_str) = crate::options::password_as_str(&options.password) {
                let c_password = CString::new(password_str)
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
                progress: options.progress.take(),
                bytes_written: 0,
                entries_written: 0,
            })
        }
    }

    fn notify_progress(&mut self, additional: u64) -> Result<()> {
        super::common::notify_creation_progress(
            &mut self.progress,
            &mut self.bytes_written,
            additional,
            None,
        )
    }

    /// Add a file to the archive from byte data
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::write_mode_only("add_file_from_data"))?;

        unsafe {
            // Create entry
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Set pathname
            let c_path = match CString::new(archive_path) {
                Ok(c) => c,
                Err(_) => {
                    archive_entry_free(entry);
                    return Err(ArchiveError::invalid_path(
                        archive_path,
                        "Contains null byte",
                    ));
                }
            };
            archive_entry_set_pathname(entry, c_path.as_ptr());

            // Set file type and size
            archive_entry_set_filetype(entry, AE_IFREG);
            archive_entry_set_size(entry, data.len() as c_longlong);
            archive_entry_set_perm(entry, 0o644);

            // Set modification time to now
            let now = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            archive_entry_set_mtime(entry, now.as_secs() as i64, now.subsec_nanos() as c_longlong);

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

                if bytes_written as usize != data.len() {
                    archive_entry_free(entry);
                    return Err(ArchiveError::io(
                        "write_data",
                        archive_path,
                        std::io::Error::other(format!(
                            "Short write: requested {} bytes, wrote {} bytes",
                            data.len(),
                            bytes_written
                        )),
                    ));
                }
            }

            // Finish entry
            let finish_result = archive_write_finish_entry(write_handle);
            if finish_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }
            archive_entry_free(entry);
        }

        self.entries_written += 1;
        self.notify_progress(data.len() as u64)
    }

    /// Add a file from byte data, preserving metadata from an existing `ArchiveEntry`.
    ///
    /// Used by `commit_changes()` to round-trip entries without losing timestamps
    /// and permissions.
    pub fn add_file_from_data_with_metadata(
        &mut self,
        archive_path: &str,
        data: &[u8],
        metadata: &crate::entry::ArchiveEntry,
    ) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::write_mode_only("add_file_from_data_with_metadata"))?;

        unsafe {
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            let c_path = match CString::new(archive_path) {
                Ok(c) => c,
                Err(_) => {
                    archive_entry_free(entry);
                    return Err(ArchiveError::invalid_path(archive_path, "Contains null byte"));
                }
            };
            archive_entry_set_pathname(entry, c_path.as_ptr());

            archive_entry_set_filetype(entry, AE_IFREG);
            archive_entry_set_size(entry, data.len() as c_longlong);

            // Restore permissions from original entry, default to 0o644
            let perm = metadata.permissions.unwrap_or(0o644);
            archive_entry_set_perm(entry, perm as i32);

            // Restore modification time from original entry, fall back to now
            let duration = metadata
                .modified
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .unwrap_or_else(|| {
                    std::time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                });
            archive_entry_set_mtime(
                entry,
                duration.as_secs() as i64,
                duration.subsec_nanos() as c_longlong,
            );

            let header_result = archive_write_header(write_handle, entry);
            if header_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }

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
                if bytes_written as usize != data.len() {
                    archive_entry_free(entry);
                    return Err(ArchiveError::io(
                        "write_data",
                        archive_path,
                        std::io::Error::other(format!(
                            "Short write: requested {} bytes, wrote {} bytes",
                            data.len(),
                            bytes_written
                        )),
                    ));
                }
            }

            let finish_result = archive_write_finish_entry(write_handle);
            if finish_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }
            archive_entry_free(entry);
        }

        self.entries_written += 1;
        self.notify_progress(data.len() as u64)
    }

    /// Add a directory entry to the archive (without contents)
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> {
        let write_handle = self
            .write_handle
            .ok_or_else(|| ArchiveError::write_mode_only("add_directory_entry"))?;

        unsafe {
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Ensure trailing slash for directory path
            let dir_path = super::common::ensure_trailing_slash(archive_path);

            let c_path = match CString::new(dir_path) {
                Ok(c) => c,
                Err(_) => {
                    archive_entry_free(entry);
                    return Err(ArchiveError::invalid_path(
                        archive_path,
                        "Contains null byte",
                    ));
                }
            };
            archive_entry_set_pathname(entry, c_path.as_ptr());

            archive_entry_set_filetype(entry, AE_IFDIR);
            archive_entry_set_size(entry, 0);
            archive_entry_set_perm(entry, 0o755);

            let now = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            archive_entry_set_mtime(entry, now.as_secs() as i64, now.subsec_nanos() as c_longlong);

            let header_result = archive_write_header(write_handle, entry);
            if header_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }

            let finish_result = archive_write_finish_entry(write_handle);
            if finish_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }
            archive_entry_free(entry);
        }

        self.entries_written += 1;
        Ok(())
    }

    /// Number of entries written so far (write mode only)
    pub fn entries_written(&self) -> usize {
        self.entries_written
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
            .ok_or_else(|| ArchiveError::write_mode_only("add_file_from_path"))?;

        unsafe {
            // Create entry
            let entry = archive_entry_new();
            if entry.is_null() {
                return Err(ArchiveError::format(None, "Failed to create entry"));
            }

            // Set pathname
            let c_path = match CString::new(archive_path) {
                Ok(c) => c,
                Err(_) => {
                    archive_entry_free(entry);
                    return Err(ArchiveError::invalid_path(
                        archive_path,
                        "Contains null byte",
                    ));
                }
            };
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

                if bytes_written as usize != bytes_read {
                    archive_entry_free(entry);
                    return Err(ArchiveError::io(
                        "write_data",
                        archive_path,
                        std::io::Error::other(format!(
                            "Short write: requested {} bytes, wrote {} bytes",
                            bytes_read, bytes_written
                        )),
                    ));
                }
            }

            // Finish entry
            let finish_result = archive_write_finish_entry(write_handle);
            if finish_result != ARCHIVE_OK {
                let error_msg = get_archive_error(write_handle);
                archive_entry_free(entry);
                return Err(ArchiveError::format(None, error_msg));
            }
            archive_entry_free(entry);
        }

        self.entries_written += 1;
        self.notify_progress(metadata.len())
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
            if entry.file_type().is_file() {
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
    /// Streams each file and verifies checksums (libarchive validates during read).
    /// Returns a list of file paths that failed verification.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let c_path = CString::new(self.path.clone())
            .map_err(|_| ArchiveError::invalid_path(&self.path, "Contains null byte"))?;

        let mut failed_files = Vec::new();

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;

            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);
                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK && result != ARCHIVE_WARN {
                    let error_msg = get_archive_error(archive);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, error_msg));
                }

                let Some(entry) = parse_entry(entry_ptr) else {
                    archive_read_data_skip(archive);
                    continue;
                };

                if entry.entry_type != EntryType::File {
                    archive_read_data_skip(archive);
                    continue;
                }

                let path = entry.path;

                // Stream through data — libarchive validates checksums during read
                let mut buf: *const c_void = std::ptr::null();
                let mut size: usize = 0;
                let mut offset: c_longlong = 0;
                let mut failed = false;
                loop {
                    let r = archive_read_data_block(archive, &mut buf, &mut size, &mut offset);
                    if r == ARCHIVE_EOF {
                        break;
                    }
                    if r != ARCHIVE_OK {
                        failed = true;
                        // Drain remaining blocks so libarchive state stays consistent
                        while archive_read_data_block(archive, &mut buf, &mut size, &mut offset)
                            == ARCHIVE_OK
                        {}
                        break;
                    }
                }

                if failed {
                    failed_files.push(path);
                }
            }

            archive_read_free(archive);
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
            // libarchive error strings for checksum failures vary; match on substring
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

/// Streaming reader that reads directly from a libarchive handle via `archive_read_data`.
///
/// Owns the archive handle and frees it on drop. Positioned at the target entry
/// after construction — subsequent `Read::read` calls pull decompressed data
/// without buffering the entire file in memory.
pub(crate) struct LibarchiveStreamReader {
    archive: *mut Archive,
    /// Entry size if known (from archive_entry_size)
    pub(crate) entry_size: Option<u64>,
    eof: bool,
}

// SAFETY: The archive handle is exclusively owned by this struct.
// No other code accesses it after construction until drop.
unsafe impl Send for LibarchiveStreamReader {}

impl LibarchiveStreamReader {
    /// Open the archive and position at the target entry for streaming reads.
    fn open(archive_path: &str, target_entry: &str) -> Result<Self> {
        let c_path = CString::new(archive_path.to_string())
            .map_err(|_| ArchiveError::invalid_path(archive_path, "Contains null byte"))?;

        unsafe {
            let archive = LibarchiveArchive::open_read_handle(&c_path)?;

            let mut entry: *mut LibarchiveEntry = std::ptr::null_mut();
            loop {
                let r = archive_read_next_header(archive, &mut entry);
                if r == ARCHIVE_EOF {
                    archive_read_free(archive);
                    return Err(ArchiveError::format(
                        None,
                        format!("File '{}' not found in archive", target_entry),
                    ));
                }
                if r != ARCHIVE_OK && r != ARCHIVE_WARN {
                    let msg = get_archive_error(archive);
                    archive_read_free(archive);
                    return Err(ArchiveError::format(None, msg));
                }

                let pathname = archive_entry_pathname(entry);
                if pathname.is_null() {
                    archive_read_data_skip(archive);
                    continue;
                }

                let name = CStr::from_ptr(pathname).to_string_lossy();
                if name.as_ref() == target_entry {
                    let raw_size = archive_entry_size(entry);
                    let entry_size = if raw_size > 0 {
                        Some(raw_size as u64)
                    } else {
                        None
                    };
                    return Ok(Self {
                        archive,
                        entry_size,
                        eof: false,
                    });
                }

                archive_read_data_skip(archive);
            }
        }
    }
}

impl std::io::Read for LibarchiveStreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.eof || buf.is_empty() {
            return Ok(0);
        }

        let n =
            unsafe { archive_read_data(self.archive, buf.as_mut_ptr() as *mut c_void, buf.len()) };

        if n == 0 {
            self.eof = true;
            Ok(0)
        } else if n < 0 {
            self.eof = true;
            Err(std::io::Error::other(format!(
                "libarchive read error: {}",
                unsafe { get_archive_error(self.archive) }
            )))
        } else {
            Ok(n as usize)
        }
    }
}

impl Drop for LibarchiveStreamReader {
    fn drop(&mut self) {
        if !self.archive.is_null() {
            unsafe {
                archive_read_free(self.archive);
            }
            self.archive = std::ptr::null_mut();
        }
    }
}
