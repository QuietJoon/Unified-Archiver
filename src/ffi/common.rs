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
    options.open(path).map_err(|e| {
        if !overwrite && e.kind() == std::io::ErrorKind::AlreadyExists {
            ArchiveError::UnsupportedOperation {
                operation: "extract".to_string(),
                reason: format!("Destination file already exists: {}", path.display()),
            }
        } else {
            ArchiveError::io("create", path.to_path_buf(), e)
        }
    })
}

/// Convert year/month/day/hour/minute/second to SystemTime
///
/// Returns None for pre-1970 dates.
pub(crate) fn ymd_hms_to_system_time(
    year: u64,
    month: u64,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};

    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Cumulative days before each month (non-leap year)
    const DAYS_BEFORE_MONTH: [u64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;

    // Days from 1970 to start of year, using the correct Gregorian leap-year formula
    fn leap_days_before(year: u64) -> u64 {
        let y = year - 1;
        let from = 1970u64 - 1;
        (y / 4 - y / 100 + y / 400) - (from / 4 - from / 100 + from / 400)
    }
    let days_in_prior_years = (year - 1970) * 365 + leap_days_before(year);
    let mut days = days_in_prior_years + DAYS_BEFORE_MONTH[month as usize] + day - 1;

    if is_leap && month > 2 {
        days += 1;
    }

    let total_seconds = days * 86400 + hour * 3600 + minute * 60 + second;
    Some(UNIX_EPOCH + Duration::from_secs(total_seconds))
}

/// Compute CRC32 by streaming through a reader (no full buffering)
pub(crate) fn compute_crc32_reader<R: Read>(reader: &mut R, error_path: &Path) -> Result<u32> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| ArchiveError::io("read", error_path.to_path_buf(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize())
}

/// Ensure a directory path has a trailing slash
pub(crate) fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{}/", path)
    }
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
