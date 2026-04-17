//! Safe Rust wrappers around FFI bindings
//!
//! Provides memory-safe and ergonomic interfaces with RAII patterns

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, Result};
use crate::ffi::unrar::*;
use crate::format::ArchiveFormat;
use crate::options::{ProgressCallback, RateLimiter};
use crate::security::sanitize_entry_path;
use std::ffi::CString;
use std::ops::ControlFlow;
use std::os::raw::{c_int, c_uint};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use super::common::{TempDirGuard, normalize_path};

/// Seconds between Windows FILETIME epoch (1601-01-01) and Unix epoch (1970-01-01)
const FILETIME_UNIX_EPOCH_DIFF_SECS: u64 = 11_644_473_600;

/// Process-wide serialization lock for UnRAR FFI calls.
///
/// The UnRAR C library maintains mutable global state (parser buffers,
/// encryption tables, error codes) that is not safe to touch from multiple
/// threads even when the threads operate on *different* archive handles.
/// Guard every FFI entry point with this mutex to prevent the cross-thread
/// CRC / header corruption observed in concurrent-use testing.
///
/// The lock is acquired at small scopes (one `unsafe { ffi_call() }` each),
/// never re-entrantly, so throughput is only affected when multiple threads
/// actually contend. Poisoning is recovered by reading past the poison — each
/// FFI call is stateless with respect to the poisoned thread's archive
/// handle, so there is no cross-contamination risk.
static UNRAR_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the UnRAR FFI lock, recovering from poison.
#[inline]
fn unrar_lock() -> MutexGuard<'static, ()> {
    UNRAR_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Safe wrapper around UnRAR archive handle
///
/// Automatically closes archive on drop (RAII pattern)
pub struct UnrarArchive {
    handle: RARHandle,
    path: String,
    password: Option<String>,
    /// Archive header flags (includes solid, volume, locked flags)
    flags: c_uint,
}

impl UnrarArchive {
    /// Open RAR/RAR5 archive with specific mode
    fn open_with_mode(path: impl AsRef<Path>, mode: c_uint) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Try opening with UTF-8 path first
        let c_path = CString::new(path_str.clone())
            .map_err(|_| ArchiveError::invalid_path(&path_str, "Contains null byte"))?;

        unsafe {
            let mut open_data = RAROpenArchiveDataEx {
                arc_name: c_path.as_ptr(),
                open_mode: mode,
                ..Default::default()
            };

            let _guard = unrar_lock();
            let handle = RAROpenArchiveEx(&mut open_data);

            if handle.is_null() || open_data.open_result != ERAR_SUCCESS as u32 {
                return Err(map_unrar_error(open_data.open_result as c_int, &path_str));
            }

            Ok(Self {
                handle,
                path: path_str,
                password: None,
                flags: open_data.flags,
            })
        }
    }

    /// Open RAR/RAR5 archive for reading and extraction
    ///
    /// Uses RAR_OM_EXTRACT mode to support both listing headers and extracting files.
    /// This mode allows iteration through entries and extraction operations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_mode(path, RAR_OM_EXTRACT)
    }

    /// Open encrypted RAR/RAR5 archive with password
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let mut archive = Self::open(path)?;

        let c_password = CString::new(password)
            .map_err(|_| ArchiveError::password("Password contains null byte"))?;

        unsafe {
            let _guard = unrar_lock();
            RARSetPassword(archive.handle, c_password.as_ptr());
        }

        // Store password for fresh handle creation
        archive.password = Some(password.to_string());

        Ok(archive)
    }

    /// Create a fresh handle for extraction operations
    ///
    /// UnRAR handles get exhausted after list_files(), so extraction operations
    /// need a fresh handle. This creates a new handle with the same path/password.
    fn fresh_handle(&self) -> Result<Self> {
        // Create a new handle with same path/password
        if let Some(pwd) = &self.password {
            Self::open_with_password(&self.path, pwd)
        } else {
            Self::open(&self.path)
        }
    }

    /// Read next entry header with CRC32 and metadata
    pub fn read_header(&self) -> Result<Option<ArchiveEntry>> {
        unsafe {
            let mut header = RARHeaderDataEx::default();
            let _guard = unrar_lock();
            let result = RARReadHeaderEx(self.handle, &mut header);
            drop(_guard);

            match result {
                ERAR_SUCCESS => Ok(Some(parse_header(&header)?)),
                ERAR_END_ARCHIVE => Ok(None),
                ERAR_BAD_PASSWORD => Err(ArchiveError::password("Wrong password")),
                ERAR_MISSING_PASSWORD => Err(ArchiveError::password("Password required")),
                _ => Err(map_unrar_error(result, &self.path)),
            }
        }
    }

    /// Skip current entry (move to next)
    pub fn skip_entry(&self) -> Result<()> {
        unsafe {
            let _guard = unrar_lock();
            let result = RARProcessFile(self.handle, RAR_SKIP, std::ptr::null(), std::ptr::null());

            if result == ERAR_SUCCESS {
                Ok(())
            } else {
                Err(map_unrar_error(result, &self.path))
            }
        }
    }

    /// List all files in archive with CRC32
    pub fn list_files(&self) -> Result<Vec<ArchiveEntry>> {
        let mut entries = Vec::new();
        let mut index = 0;

        while let Some(mut entry) = self.read_header()? {
            entry.id = index;
            entries.push(entry);
            self.skip_entry()?;
            index += 1;
        }

        Ok(entries)
    }

    /// Get archive path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Check if archive uses solid compression
    ///
    /// Solid compression compresses all files together as a single stream,
    /// providing better compression ratios but slower random access.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive uses solid compression
    /// * `Ok(false)` - Archive uses non-solid compression
    pub fn is_solid(&self) -> Result<bool> {
        Ok((self.flags & ROADF_SOLID) != 0)
    }

    /// Check if archive has recovery records
    ///
    /// Recovery records allow repairing corrupted archives.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive has recovery records
    /// * `Ok(false)` - Archive has no recovery records
    pub fn has_recovery_record(&self) -> Result<bool> {
        Ok((self.flags & ROADF_RECOVERY) != 0)
    }

    /// Get recovery record percentage
    ///
    /// Extracts the recovery percentage from RAR recovery record blocks.
    /// Common values are 2%, 3%, 5%, 10%, etc.
    ///
    /// # Returns
    /// * `Ok(Some(percentage))` - Recovery records present with known percentage
    /// * `Ok(None)` - No recovery records or percentage cannot be determined
    /// * `Err(...)` - I/O error during parsing
    ///
    /// # Implementation
    /// Parses the RAR file directly to locate and extract recovery record block headers,
    /// which contain the recovery percentage value.
    pub fn recovery_percentage(&self) -> Result<Option<u8>> {
        // Quick check: if no recovery flag, return None immediately
        if (self.flags & ROADF_RECOVERY) == 0 {
            return Ok(None);
        }

        // Recovery records exist, try to extract percentage
        // This requires parsing the RAR file to find recovery record blocks
        self.parse_recovery_percentage()
    }

    /// Parse recovery percentage from RAR file structure
    ///
    /// RAR recovery records are stored as special blocks: type 0x78 in RAR4,
    /// service header (type 3, name "RR") in RAR5.
    fn parse_recovery_percentage(&self) -> Result<Option<u8>> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(&self.path)
            .map_err(|e| ArchiveError::io("open", std::path::Path::new(&self.path), e))?;

        // Read first 16 bytes to check RAR signature and version
        let mut header = [0u8; 16];
        file.read_exact(&mut header)
            .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

        // Check if RAR5 format (signature: "Rar!\x1A\x07\x01\x00")
        let is_rar5 = header.starts_with(b"Rar!\x1A\x07\x01\x00");

        if is_rar5 {
            // RAR5 format: parse modern block structure
            self.parse_rar5_recovery(&mut file)
        } else {
            // RAR4 format: parse legacy block structure
            self.parse_rar4_recovery(&mut file)
        }
    }

    /// Parse RAR5 recovery record blocks
    ///
    /// RAR5 uses a new block format with variable-length headers.
    /// Recovery records are service headers (type 3) with name "RR".
    /// End of archive is type 5.
    fn parse_rar5_recovery(&self, file: &mut std::fs::File) -> Result<Option<u8>> {
        use std::io::{Read, Seek, SeekFrom};

        // Skip RAR5 signature (8 bytes: "Rar!\x1A\x07\x01\x00")
        file.seek(SeekFrom::Start(8))
            .map_err(|e| ArchiveError::io("seek", std::path::Path::new(&self.path), e))?;

        loop {
            let block_pos = file
                .stream_position()
                .map_err(|e| ArchiveError::io("tell", std::path::Path::new(&self.path), e))?;

            // Read enough for CRC32(4) + several max-length vints
            let mut block_start = [0u8; 50];
            let bytes_read = file
                .read(&mut block_start)
                .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

            if bytes_read < 7 {
                return Ok(None);
            }

            // Parse RAR5 block header:
            // CRC32(4) | HeaderSize(vint) | HeaderType(vint) | HeaderFlags(vint) | ...
            // HeaderSize counts bytes from HeaderType to end of header (excludes CRC32 and itself).
            // Data area (if HFLAGS_DATA) follows the header and is NOT in HeaderSize.
            let (header_size, offset1) = decode_vint(&block_start[4..bytes_read])?;

            let type_start = 4 + offset1;
            if type_start >= bytes_read {
                return Ok(None);
            }
            let (header_type, offset2) = decode_vint(&block_start[type_start..bytes_read])?;

            let flags_start = type_start + offset2;
            if flags_start >= bytes_read {
                return Ok(None);
            }
            let (header_flags, offset3) = decode_vint(&block_start[flags_start..bytes_read])?;

            // Parse optional ExtraSize and DataSize vints from the header
            let mut next_field = flags_start + offset3;
            if header_flags & 0x0001 != 0 && next_field < bytes_read {
                // HFLAGS_EXTRA: skip ExtraSize vint
                let (_, extra_vint_len) = decode_vint(&block_start[next_field..bytes_read])?;
                next_field += extra_vint_len;
            }
            let mut data_area_size: u64 = 0;
            if header_flags & 0x0002 != 0 && next_field < bytes_read {
                // HFLAGS_DATA: parse DataSize vint
                let (ds, _) = decode_vint(&block_start[next_field..bytes_read])?;
                data_area_size = ds;
            }

            // End of archive (type 5)
            if header_type == 5 {
                return Ok(None);
            }

            // Service header (type 3) — recovery records are service blocks named "RR"
            if header_type == 3 {
                // Compute offset of remaining header content after standard fields
                let header_overhead = (offset2 + offset3) as u64;
                let remaining = header_size.saturating_sub(header_overhead);

                // Seek to start of type-specific header content
                let content_pos = block_pos + 4 + offset1 as u64 + header_overhead;
                file.seek(SeekFrom::Start(content_pos))
                    .map_err(|e| ArchiveError::io("seek", std::path::Path::new(&self.path), e))?;

                let read_size = remaining.min(1024) as usize;
                let mut rec_data = vec![0u8; read_size];
                file.read_exact(&mut rec_data)
                    .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

                // Check for "RR" service name in the header content
                if rec_data.windows(2).any(|w| w == b"RR") {
                    // Recovery record found — scan for percentage value (1-15%)
                    for &byte in rec_data.iter().take(64) {
                        if (1..=15).contains(&byte) {
                            return Ok(Some(byte));
                        }
                    }
                    // Recovery record exists but couldn't determine percentage
                    return Ok(None);
                }
            }

            // Skip to next block: CRC(4) + HeaderSize_vint(offset1) + header(header_size) + data(data_area_size)
            let next_block = block_pos + 4 + offset1 as u64 + header_size + data_area_size;
            file.seek(SeekFrom::Start(next_block))
                .map_err(|e| ArchiveError::io("seek", std::path::Path::new(&self.path), e))?;
        }
    }

    /// Parse RAR4 recovery record blocks
    ///
    /// RAR4 uses a legacy block format with fixed-size headers.
    /// Recovery records have block type 0x78.
    fn parse_rar4_recovery(&self, file: &mut std::fs::File) -> Result<Option<u8>> {
        use std::io::{Read, Seek, SeekFrom};

        // RAR4 signature is 7 bytes: "Rar!\x1A\x07\x00"
        // After signature comes the main archive header, then file/service blocks

        // Seek past signature (already read 16 bytes, go back to start of blocks at offset 7)
        file.seek(SeekFrom::Start(7))
            .map_err(|e| ArchiveError::io("seek", std::path::Path::new(&self.path), e))?;

        // Parse blocks until we find recovery record (0x78) or EOF
        loop {
            // Read RAR4 block header (7 bytes minimum)
            // Format: HEAD_CRC(2) | HEAD_TYPE(1) | HEAD_FLAGS(2) | HEAD_SIZE(2)
            let mut block_header = [0u8; 7];
            let bytes_read = file
                .read(&mut block_header)
                .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

            if bytes_read < 7 {
                // End of file
                return Ok(None);
            }

            let _head_crc = u16::from_le_bytes([block_header[0], block_header[1]]);
            let head_type = block_header[2];
            let head_flags = u16::from_le_bytes([block_header[3], block_header[4]]);
            let head_size = u16::from_le_bytes([block_header[5], block_header[6]]);

            // Check if this is a recovery record block (type 0x78)
            if head_type == 0x78 {
                // Recovery record found
                // The data starts after the header
                let data_size = head_size.saturating_sub(7); // Subtract header size

                // Read recovery record data
                let mut rec_data = vec![0u8; data_size.min(1024) as usize];
                file.read_exact(&mut rec_data)
                    .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

                // RAR4 recovery record structure:
                // Bytes 0-3: Total blocks
                // Bytes 4-7: Recovery blocks
                // Percentage = (recovery_blocks / total_blocks) * 100
                if rec_data.len() >= 8 {
                    let total_blocks =
                        u32::from_le_bytes([rec_data[0], rec_data[1], rec_data[2], rec_data[3]]);
                    let recovery_blocks =
                        u32::from_le_bytes([rec_data[4], rec_data[5], rec_data[6], rec_data[7]]);

                    if total_blocks > 0 {
                        // Prevent integer overflow - cap at 100%
                        let percentage =
                            ((recovery_blocks as u64 * 100) / total_blocks as u64).min(100) as u8;
                        return Ok(Some(percentage));
                    }
                }

                // Could not determine exact percentage
                return Ok(None);
            }

            // Skip to next block.
            // head_size includes the 7-byte basic header.
            // LONG_BLOCK flag (0x8000) means an ADD_SIZE (u32) data area follows the header.
            let mut skip = (head_size as i64) - 7;
            if head_flags & 0x8000 != 0 && head_size >= 11 {
                // ADD_SIZE is at header bytes 7-10 (right after basic header)
                let mut add_bytes = [0u8; 4];
                file.read_exact(&mut add_bytes)
                    .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;
                let add_size = u32::from_le_bytes(add_bytes);
                // We already read 4 bytes of ADD_SIZE from the remaining header
                skip = (head_size as i64) - 11 + add_size as i64;
            }
            if skip > 0 {
                file.seek(SeekFrom::Current(skip))
                    .map_err(|e| ArchiveError::io("seek", std::path::Path::new(&self.path), e))?;
            }

            // Check for end of archive marker (type 0x7B)
            if head_type == 0x7B {
                return Ok(None);
            }

            // Check for volume end (flag 0x8000)
            if (head_flags & 0x8000) != 0 {
                return Ok(None);
            }
        }
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &std::path::Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<()> {
        self.extract_all_with_options(dest_path, progress, true)
    }

    pub fn extract_all_with_options(
        &self,
        dest_path: &std::path::Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
    ) -> Result<()> {
        // Compute total_bytes from a dedicated fresh handle. The original
        // `self.handle` may already be EOF-positioned (e.g., after
        // `list_files_for_limits()` during open), so calling `self.list_files()`
        // here yields zero entries and `total_bytes == 0`, breaking progress.
        let total_bytes: u64 = if progress.is_some() {
            let prescan = self.fresh_handle()?;
            prescan
                .list_files()?
                .iter()
                .filter_map(|e| e.size)
                .sum()
        } else {
            0
        };

        // Fresh handle for extraction (the pre-scan handle above is exhausted).
        let fresh = self.fresh_handle()?;

        let abs_dest = resolve_dest_path(dest_path)?;

        let mut bytes_processed = 0u64;
        let mut rate_limiter = RateLimiter::new();

        while let Some(entry) = fresh.read_header()? {
            // Check cancellation before extracting
            if let Some(callback) = progress.as_mut() {
                if rate_limiter.should_call() {
                    if let ControlFlow::Break(()) =
                        callback.on_progress(bytes_processed, Some(total_bytes))
                    {
                        return Err(ArchiveError::format(None, "Extraction cancelled by user"));
                    }
                }
            }

            // Skip symlinks and hardlinks for security
            if entry.entry_type == EntryType::Symlink || entry.entry_type == EntryType::HardLink {
                unsafe {
                    let _guard = unrar_lock();
                    RARProcessFile(fresh.handle, RAR_SKIP, std::ptr::null(), std::ptr::null());
                }
                continue;
            }

            // Sanitize path to prevent traversal attacks
            let safe_path = sanitize_entry_path(&entry.path, &abs_dest)?;

            // Create parent directories if needed
            if let Some(parent) = safe_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| ArchiveError::io("create_dir_all", parent, e))?;
                }
            }

            if !overwrite && entry.is_file() && safe_path.exists() {
                return Err(ArchiveError::OperationBlocked {
                    operation: "extract_all".to_string(),
                    reason: format!("Destination file already exists: {}", safe_path.display()),
                });
            }

            // Extract using the full absolute path as dest_name
            let safe_path_str = safe_path.to_string_lossy();
            let dest_name_cstr = CString::new(safe_path_str.as_bytes()).map_err(|_| {
                ArchiveError::invalid_path(safe_path_str.as_ref(), "Contains null byte")
            })?;

            // Extract current file using full absolute path as the destination filename
            unsafe {
                // Serialize UnRAR FFI calls (global state is not thread-safe).
                let _guard = unrar_lock();
                let result = RARProcessFile(
                    fresh.handle,
                    RAR_EXTRACT,
                    std::ptr::null(), // NULL for directory (use full path in dest_name)
                    dest_name_cstr.as_ptr(), // Full absolute path
                );

                if result != ERAR_SUCCESS {
                    return Err(map_unrar_error(result, &self.path));
                }
            }

            // Update progress after extraction
            if progress.is_some() {
                bytes_processed += entry.size.unwrap_or(0);
            }
        }

        // Final progress update (100%)
        if let Some(callback) = progress.as_mut() {
            let _ = callback.on_progress(total_bytes, Some(total_bytes));
        }

        Ok(())
    }

    /// Extract a single file by path
    ///
    /// Creates a fresh handle to avoid state exhaustion issues.
    pub fn extract_file(&self, file_path: &str, dest_path: &std::path::Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true)
    }

    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &std::path::Path,
        overwrite: bool,
    ) -> Result<()> {
        // Create fresh handle for extraction (avoid state exhaustion)
        let fresh = self.fresh_handle()?;

        let abs_dest = resolve_dest_path(dest_path)?;

        loop {
            match fresh.read_header()? {
                Some(entry) => {
                    if entry.path == file_path {
                        // Reject symlinks/hardlinks for security
                        if entry.entry_type == EntryType::Symlink
                            || entry.entry_type == EntryType::HardLink
                        {
                            return Err(ArchiveError::format(
                                Some(ArchiveFormat::Rar),
                                format!(
                                    "Refusing to extract link entry: {}",
                                    file_path
                                ),
                            ));
                        }

                        // Sanitize path to prevent traversal attacks
                        let safe_path = sanitize_entry_path(&entry.path, &abs_dest)?;

                        // Create parent directories if needed
                        if let Some(parent) = safe_path.parent() {
                            if !parent.exists() {
                                std::fs::create_dir_all(parent)
                                    .map_err(|e| ArchiveError::io("create_dir_all", parent, e))?;
                            }
                        }

                        if !overwrite && entry.is_file() && safe_path.exists() {
                            return Err(ArchiveError::OperationBlocked {
                                operation: "extract_file".to_string(),
                                reason: format!(
                                    "Destination file already exists: {}",
                                    safe_path.display()
                                ),
                            });
                        }

                        // Extract using the full absolute path as dest_name
                        let safe_path_str = safe_path.to_string_lossy();
                        let dest_name_cstr =
                            CString::new(safe_path_str.as_bytes()).map_err(|_| {
                                ArchiveError::invalid_path(
                                    safe_path_str.as_ref(),
                                    "Contains null byte",
                                )
                            })?;

                        // Extract this file using full absolute path as the destination filename
                        unsafe {
                            // Serialize UnRAR FFI calls (global state is not thread-safe).
                            let _guard = unrar_lock();
                            let result = RARProcessFile(
                                fresh.handle,
                                RAR_EXTRACT,
                                std::ptr::null(), // NULL for directory (use full path in dest_name)
                                dest_name_cstr.as_ptr(), // Full absolute path
                            );

                            if result != ERAR_SUCCESS {
                                return Err(map_unrar_error(result, &self.path));
                            }
                        }
                        return Ok(());
                    } else {
                        // Skip this file
                        fresh.skip_entry()?;
                    }
                }
                None => {
                    return Err(ArchiveError::format(
                        Some(ArchiveFormat::Rar),
                        format!("File '{}' not found in archive", file_path),
                    ));
                }
            }
        }
    }

    /// Extract a single file to memory
    ///
    /// Note: The UnRAR API requires extraction to disk first, so this method
    /// extracts to a temporary directory and reads the result into memory.
    /// This is a fundamental limitation of the UnRAR SDK.
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
        let temp_dir = temp_base.join(format!("unrar_mem_{}_{}", std::process::id(), timestamp));

        // Ensure temp directory exists
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| ArchiveError::io("create_temp_dir", temp_dir.clone(), e))?;

        // RAII guard ensures cleanup on all exit paths (success or error)
        let _guard = TempDirGuard::new(temp_dir.clone());

        // Open a fresh archive handle to avoid state issues
        let fresh = if let Some(pwd) = &self.password {
            Self::open_with_password(&self.path, pwd)?
        } else {
            Self::open(&self.path)?
        };

        // Extract using the fresh handle
        fresh.extract_file(file_path, &temp_dir)?;

        // Read file into memory (use sanitized path to locate extracted file)
        let extracted_path = sanitize_entry_path(file_path, &temp_dir)?;
        let mut file = std::fs::File::open(&extracted_path)
            .map_err(|e| ArchiveError::io("open_extracted", extracted_path.clone(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("stat_extracted", extracted_path.clone(), e))?;
        let len = metadata.len();

        if len > usize::MAX as u64 {
            return Err(ArchiveError::OperationBlocked {
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
            .map_err(|_| ArchiveError::OperationBlocked {
                operation: "extract_to_memory".to_string(),
                reason: format!("Unable to allocate {} bytes for file '{}'", len, file_path),
            })?;

        file.read_to_end(&mut buffer)
            .map_err(|e| ArchiveError::io("read_extracted", extracted_path, e))?;

        // Guard handles cleanup on drop
        Ok(buffer)
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// The UnRAR API requires extraction to disk first, so true streaming is not possible
    /// without a fundamental change to the extraction pipeline.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use std::io::Cursor;

        // For simplicity and to avoid path issues, just use extract_to_memory
        // and wrap in a cursor. This still provides the streaming API.
        let data = self.extract_to_memory(file_path)?;
        let size = data.len() as u64;
        let reader = Box::new(Cursor::new(data));

        Ok(crate::streaming::StreamingExtractor::new(
            reader,
            Some(size),
        ))
    }

    /// Test archive integrity without extracting to disk
    ///
    /// Uses RAR's test mode to verify CRC32 checksums for all files.
    /// Returns a list of file paths that failed verification.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut failed_files = Vec::new();

        // Open a fresh handle for testing (preserves password for encrypted archives)
        let fresh = self.fresh_handle()?;

        while let Some(entry) = fresh.read_header()? {
            // Skip directories
            if entry.is_directory() {
                fresh.skip_entry()?;
                continue;
            }

            // Test the file using RAR_TEST mode
            unsafe {
                // Serialize UnRAR FFI calls (global state is not thread-safe).
                let _guard = unrar_lock();
                let result = RARProcessFile(
                    fresh.handle,
                    RAR_TEST, // Test mode - verifies CRC32 without extracting
                    std::ptr::null(),
                    std::ptr::null(),
                );

                if result != ERAR_SUCCESS {
                    // Test failed - add to failed list
                    failed_files.push(entry.path.clone());

                    // Continue testing other files instead of failing immediately
                    // Skip to next entry
                    if result != ERAR_BAD_DATA {
                        // If it's not a CRC error, we still need to skip
                        continue;
                    }
                }
            }
        }

        Ok(failed_files)
    }
}

impl Drop for UnrarArchive {
    fn drop(&mut self) {
        unsafe {
            // Drop still runs on panic paths — recovering from poison is important.
            let _guard = unrar_lock();
            RARCloseArchive(self.handle);
        }
    }
}

/// Parse UnRAR header into ArchiveEntry with CRC32
fn parse_header(header: &RARHeaderDataEx) -> Result<ArchiveEntry> {
    // Extract filename (UTF-16 on Windows, UTF-32 on macOS)
    let file_name = {
        #[cfg(target_os = "macos")]
        {
            // On macOS, wchar_t is UTF-32 (4 bytes per character)
            let mut len = 0;
            while len < header.file_name_w.len() && header.file_name_w[len] != 0 {
                len += 1;
            }
            // Convert UTF-32 to UTF-32 code points, then to String
            let chars: Vec<char> = header.file_name_w[..len]
                .iter()
                .filter_map(|&code| std::char::from_u32(code))
                .collect();
            chars.iter().collect::<String>()
        }
        #[cfg(not(target_os = "macos"))]
        {
            // On other platforms, wchar_t is UTF-16 (2 bytes per character)
            let mut len = 0;
            while len < header.file_name_w.len() && header.file_name_w[len] != 0 {
                len += 1;
            }
            String::from_utf16_lossy(&header.file_name_w[..len])
        }
    };

    // Normalize path (convert backslashes to forward slashes)
    let normalized_path = normalize_path(&file_name);

    // Determine entry type
    let is_directory = (header.flags & RHDF_DIRECTORY) != 0;
    // RAR5 redir_type: FSREDIR_UNIXSYMLINK=1, FSREDIR_WINSYMLINK=2,
    //   FSREDIR_JUNCTION=3, FSREDIR_HARDLINK=4, FSREDIR_FILECOPY=5
    let entry_type = if is_directory {
        EntryType::Directory
    } else if header.redir_type == 1 || header.redir_type == 2 || header.redir_type == 3 {
        EntryType::Symlink
    } else if header.redir_type == 4 || header.redir_type == 5 {
        EntryType::HardLink
    } else {
        EntryType::File
    };

    // Combine 64-bit sizes
    let unp_size = ((header.unp_size_high as u64) << 32) | (header.unp_size as u64);
    let pack_size = ((header.pack_size_high as u64) << 32) | (header.pack_size as u64);

    // Convert DOS timestamp to SystemTime
    let modified = dos_time_to_system_time(header.file_time);

    // Use high-resolution mtime if available
    let modified = if header.mtime_low != 0 || header.mtime_high != 0 {
        let mtime_secs = ((header.mtime_high as u64) << 32) | (header.mtime_low as u64);
        Some(
            UNIX_EPOCH
                + std::time::Duration::from_secs(
                    (mtime_secs / 10_000_000).saturating_sub(FILETIME_UNIX_EPOCH_DIFF_SECS),
                ),
        )
    } else {
        modified
    };

    let mut entry = ArchiveEntry::new(normalized_path, 0);
    entry.entry_type = entry_type;
    entry.size = if is_directory { None } else { Some(unp_size) };
    entry.compressed_size = if is_directory { None } else { Some(pack_size) };
    entry.modified = modified;
    // CRC32 value 0 is valid (e.g. empty files) — only skip for directories
    entry.crc32 = if is_directory {
        None
    } else {
        Some(header.file_crc)
    };
    entry.permissions = Some(header.file_attr);

    // Phase 1: Enhanced metadata

    // Creation time (ctime)
    entry.created = if header.ctime_low != 0 || header.ctime_high != 0 {
        let ctime_secs = ((header.ctime_high as u64) << 32) | (header.ctime_low as u64);
        Some(
            UNIX_EPOCH
                + std::time::Duration::from_secs(
                    (ctime_secs / 10_000_000).saturating_sub(FILETIME_UNIX_EPOCH_DIFF_SECS),
                ),
        )
    } else {
        None
    };

    // Access time (atime)
    entry.accessed = if header.atime_low != 0 || header.atime_high != 0 {
        let atime_secs = ((header.atime_high as u64) << 32) | (header.atime_low as u64);
        Some(
            UNIX_EPOCH
                + std::time::Duration::from_secs(
                    (atime_secs / 10_000_000).saturating_sub(FILETIME_UNIX_EPOCH_DIFF_SECS),
                ),
        )
    } else {
        None
    };

    // Encryption status
    entry.is_encrypted = (header.flags & RHDF_ENCRYPTED) != 0;

    // Comment is not available via RARHeaderDataEx (cmt_buf/cmt_size are not populated by UnRAR)
    entry.comment = None;

    // Platform-specific attributes
    use crate::entry::FileAttributes;
    entry.attributes = Some(FileAttributes {
        windows: if header.host_os == 0 || header.host_os == 2 {
            Some(header.file_attr)
        } else {
            None
        },
        unix_xattr: None, // UnRAR doesn't expose xattrs directly
        archive_specific: Some(format!(
            "host_os={} method={} unp_ver={}",
            header.host_os, header.method, header.unp_ver
        )),
    });

    Ok(entry)
}

/// Convert DOS timestamp to SystemTime
fn dos_time_to_system_time(dos_time: u32) -> Option<SystemTime> {
    if dos_time == 0 {
        return None;
    }

    // DOS time format: bits 0-4=seconds/2, 5-10=minutes, 11-15=hours
    // DOS date format: bits 16-20=day, 21-24=month, 25-31=year-1980
    let seconds = ((dos_time & 0x1F) * 2) as u64;
    let minutes = ((dos_time >> 5) & 0x3F) as u64;
    let hours = ((dos_time >> 11) & 0x1F) as u64;
    let day = ((dos_time >> 16) & 0x1F) as u64;
    let month = ((dos_time >> 21) & 0x0F) as u64;
    let year = (((dos_time >> 25) & 0x7F) + 1980) as u64;

    super::common::ymd_hms_to_system_time(year, month, day, hours, minutes, seconds)
}

/// Convert dest_path to an absolute path (thread-safe, avoids changing cwd)
fn resolve_dest_path(dest_path: &Path) -> Result<std::path::PathBuf> {
    dest_path.canonicalize().or_else(|_| {
        if dest_path.is_absolute() {
            Ok(dest_path.to_path_buf())
        } else {
            Ok(std::env::current_dir()
                .map_err(|e| ArchiveError::io("getcwd", dest_path.to_path_buf(), e))?
                .join(dest_path))
        }
    })
}

/// Map UnRAR error code to ArchiveError
fn map_unrar_error(code: c_int, path: &str) -> ArchiveError {
    match code {
        ERAR_NO_MEMORY => ArchiveError::format(Some(ArchiveFormat::Rar), "Out of memory"),
        ERAR_BAD_DATA => ArchiveError::corruption(
            path.to_string(),
            "CRC32 checksum verification failed - archive is corrupted",
        ),
        ERAR_BAD_ARCHIVE => {
            ArchiveError::format(Some(ArchiveFormat::Rar), "Not a valid RAR archive")
        }
        ERAR_UNKNOWN_FORMAT => ArchiveError::format(None, "Unknown archive format"),
        ERAR_EOPEN => ArchiveError::io(
            "open",
            path.to_string(),
            std::io::Error::from_raw_os_error(2), // ENOENT
        ),
        ERAR_MISSING_PASSWORD => ArchiveError::password("Password required"),
        ERAR_BAD_PASSWORD => ArchiveError::password("Wrong password"),
        _ => ArchiveError::format(
            Some(ArchiveFormat::Rar),
            format!("UnRAR error code: {}", code),
        ),
    }
}

/// Decode RAR5 variable-length integer (vint)
///
/// RAR5 uses LEB128-style encoding:
/// - Each byte contributes 7 bits of data
/// - High bit (0x80) is continuation flag: 1 = more bytes follow, 0 = last byte
/// - Bytes are in little-endian order (least significant 7 bits first)
///
/// Returns (decoded_value, bytes_consumed)
fn decode_vint(data: &[u8]) -> Result<(u64, usize)> {
    let mut value = 0u64;
    let mut shift: u32 = 0;
    let mut bytes_consumed = 0;

    for &byte in data {
        bytes_consumed += 1;
        let data_bits = (byte & 0x7F) as u64;

        if shift < 64 {
            value |= data_bits.checked_shl(shift).unwrap_or(0);
        }

        if (byte & 0x80) == 0 {
            return Ok((value, bytes_consumed));
        }
        shift += 7;

        if bytes_consumed >= 10 {
            break;
        }
    }

    Err(ArchiveError::format(
        Some(ArchiveFormat::Rar5),
        "Invalid vint encoding: too many bytes or unexpected EOF",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dos_time_conversion() {
        // Test a known DOS timestamp: 2024-01-15 14:30:00
        let dos_time = (2024 - 1980) << 25 | 1 << 21 | 15 << 16 | 14 << 11 | 30 << 5;
        let sys_time = dos_time_to_system_time(dos_time);
        assert!(sys_time.is_some());
    }
}
