//! Common utilities shared across FFI wrappers

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{ArchiveError, Result};
use crate::security::verify_crc32_value;

/// RAII guard for temporary directories
/// Ensures cleanup on drop, even if an error occurs
pub(crate) struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Normalize archive entry path by converting backslashes to forward slashes
#[inline]
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Create an output file with overwrite control
pub(crate) fn create_output_file(path: &Path, overwrite: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    options
        .open(path)
        .map_err(|e| ArchiveError::io("create", path.to_path_buf(), e))
}

/// Copy data with optional CRC32 verification
pub(crate) fn copy_with_optional_crc<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected_crc: Option<u32>,
    entry_path: &str,
    write_error_path: &Path,
) -> Result<u64> {
    if expected_crc.is_none() {
        return std::io::copy(reader, writer)
            .map_err(|e| ArchiveError::io("write", write_error_path.to_path_buf(), e));
    }

    let mut hasher = crc32fast::Hasher::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| ArchiveError::io("read", PathBuf::from(entry_path), e))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buffer[..n])
            .map_err(|e| ArchiveError::io("write", write_error_path.to_path_buf(), e))?;
        hasher.update(&buffer[..n]);
        total += n as u64;
    }

    verify_crc32_value(hasher.finalize(), expected_crc, entry_path)?;
    Ok(total)
}
