//! Security utilities for safe archive extraction
//!
//! This module provides security checks and sanitization to prevent common
//! archive-based attacks like path traversal (Zip Slip) and zip bombs.

use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, Result};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Maximum allowed total uncompressed size for extraction (10 GB)
pub const DEFAULT_MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024 * 1024;

/// Maximum allowed single file size (1 GB)
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum allowed compression ratio (1000:1)
pub const DEFAULT_MAX_COMPRESSION_RATIO: f64 = 1000.0;

/// Maximum allowed entry count (100,000 files)
pub const DEFAULT_MAX_ENTRY_COUNT: usize = 100_000;

/// Maximum allowed file size for memory mapping
///
/// Platform-specific limits:
/// - 64-bit systems: 4GB (mmap can handle large files via virtual memory)
/// - 32-bit systems: 100MB (limited address space)
#[cfg(target_pointer_width = "64")]
pub const DEFAULT_MAX_MMAP_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4GB on 64-bit

#[cfg(target_pointer_width = "32")]
pub const DEFAULT_MAX_MMAP_SIZE: u64 = 100 * 1024 * 1024; // 100MB on 32-bit

/// Maximum allowed comment size (64 KB)
pub const MAX_COMMENT_SIZE: u32 = 64 * 1024;

/// Resource limits for extraction operations
#[derive(Debug, Clone)]
pub struct ExtractionLimits {
    /// Maximum total uncompressed size (bytes)
    pub max_total_size: u64,
    /// Maximum single file size (bytes)
    pub max_file_size: u64,
    /// Maximum compression ratio (uncompressed/compressed)
    pub max_compression_ratio: f64,
    /// Maximum number of entries
    pub max_entry_count: usize,
    /// Maximum file size for memory mapping (bytes)
    ///
    /// This limit applies to the Piz (ZIP) backend which uses memory mapping.
    /// Set to `None` to use the platform default (4GB on 64-bit, 100MB on 32-bit).
    pub max_mmap_size: Option<u64>,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_total_size: DEFAULT_MAX_TOTAL_SIZE,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_compression_ratio: DEFAULT_MAX_COMPRESSION_RATIO,
            max_entry_count: DEFAULT_MAX_ENTRY_COUNT,
            max_mmap_size: None,
        }
    }
}

impl ExtractionLimits {
    /// Create unlimited extraction limits (use with caution!)
    pub fn unlimited() -> Self {
        Self {
            max_total_size: u64::MAX,
            max_file_size: u64::MAX,
            max_compression_ratio: f64::MAX,
            max_entry_count: usize::MAX,
            max_mmap_size: Some(u64::MAX),
        }
    }

    /// Set the maximum mmap size
    pub fn with_max_mmap_size(mut self, size: u64) -> Self {
        self.max_mmap_size = Some(size);
        self
    }

    /// Reject suspiciously high uncompressed/compressed ratios (zip-bomb gate).
    ///
    /// `label` is interpolated into the error to identify the entry or archive
    /// the violation belongs to. Returns `Ok(())` if the ratio limit is
    /// effectively unlimited or the compressed size is zero.
    pub(crate) fn check_ratio(
        &self,
        uncompressed: u64,
        compressed: u64,
        label: &str,
    ) -> Result<()> {
        if compressed == 0 || self.max_compression_ratio >= f64::MAX {
            return Ok(());
        }
        let ratio = uncompressed as f64 / compressed as f64;
        if ratio > self.max_compression_ratio {
            return Err(ArchiveError::OperationBlocked {
                operation: "extract".to_string(),
                reason: format!(
                    "{} compression ratio {:.1}:1 exceeds limit of {:.1}:1 (possible zip bomb)",
                    label, ratio, self.max_compression_ratio
                ),
            });
        }
        Ok(())
    }
}

/// Get the effective maximum mmap size
///
/// Checks for the `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` environment variable first,
/// then falls back to the platform default (4GB on 64-bit, 100MB on 32-bit).
///
/// The environment variable should be specified in bytes (e.g., `17592186044416` for 16TB).
pub fn get_max_mmap_size() -> u64 {
    if let Ok(value) = std::env::var("UNIFIED_ARCHIVE_MAX_MMAP_SIZE") {
        if let Ok(size) = value.parse::<u64>() {
            return size;
        }
    }
    DEFAULT_MAX_MMAP_SIZE
}

/// Normalize entry path by stripping non-normal components (traversal, root, current dir)
fn normalize_entry_components(entry_path: &str) -> Result<PathBuf> {
    let normalized = Path::new(entry_path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<PathBuf>();

    if normalized.as_os_str().is_empty() {
        return Err(ArchiveError::InvalidPath {
            path: entry_path.to_string(),
            reason: "Path contains only traversal components".to_string(),
        });
    }

    Ok(normalized)
}

/// Validate an archive entry path without creating directories or performing I/O
///
/// Pure validation function that checks for path traversal but doesn't create
/// any directories or touch the filesystem beyond the initial canonicalize of `dest`.
/// Use this for conflict checking where directories shouldn't be created yet.
pub fn validate_entry_path(entry_path: &str, dest: &Path) -> Result<PathBuf> {
    let normalized = normalize_entry_components(entry_path)?;
    Ok(dest.join(normalized))
}

/// Validate an archive-internal name at the creation/modification boundary.
///
/// Reject names the extraction side would refuse to honour verbatim — traversal
/// segments (`..`), absolute prefixes (`/`, `C:\\`), NUL bytes, and empty
/// strings. Callers on the write path are expected to pass names that round-trip
/// cleanly through `sanitize_entry_path`; this helper fails fast instead of
/// silently writing an entry whose name the extractor would later rewrite.
pub(crate) fn validate_archive_internal_path(archive_path: &str) -> Result<()> {
    if archive_path.is_empty() {
        return Err(ArchiveError::invalid_path(
            "",
            "archive-internal path must not be empty",
        ));
    }
    if archive_path.contains('\0') {
        return Err(ArchiveError::invalid_path(
            archive_path,
            "archive-internal path must not contain NUL",
        ));
    }
    for component in Path::new(archive_path).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ArchiveError::invalid_path(
                    archive_path,
                    "archive-internal path must not contain '..' segments",
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::invalid_path(
                    archive_path,
                    "archive-internal path must be relative",
                ));
            }
        }
    }
    Ok(())
}

/// Sanitize an archive entry path to prevent path traversal attacks
///
/// This function removes:
/// - Absolute path components (e.g., `/etc/passwd` → `etc/passwd`)
/// - Parent directory references (e.g., `../../etc/passwd` → `etc/passwd`)
/// - Current directory references (e.g., `./file` → `file`)
///
/// # Security
///
/// Prevents Zip Slip attacks where malicious archives contain entries with
/// paths like `../../../../../../tmp/malicious.sh` that escape the extraction
/// directory.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use unified_archive::security::sanitize_entry_path;
///
/// // Using an existing directory (must exist on filesystem)
/// let dest = std::env::temp_dir();
///
/// // Normal path - unchanged
/// let safe = sanitize_entry_path("subdir/file.txt", &dest)?;
/// assert!(safe.starts_with(&dest));
///
/// // Path traversal - sanitized (traversal components removed)
/// let evil = sanitize_entry_path("../../etc/passwd", &dest)?;
/// assert!(evil.starts_with(&dest));
///
/// // Absolute path - converted to relative
/// let abs = sanitize_entry_path("/etc/passwd", &dest)?;
/// assert!(abs.starts_with(&dest));
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub fn sanitize_entry_path(entry_path: &str, dest: &Path) -> Result<PathBuf> {
    let normalized = normalize_entry_components(entry_path)?;

    // Join with destination
    let full_path = dest.join(&normalized);

    // Canonicalize destination to a stable base. After `normalize_entry_components`
    // stripped all `..`/absolute components, `normalized` is guaranteed relative, so
    // `full_path` cannot lexically escape `dest`. We still canonicalize `dest` (and
    // only existing ancestors of `full_path`) so subsequent symlink checks compare
    // against a canonical base — we deliberately do NOT create parent directories
    // here to keep this function side-effect-free (callers create dirs if needed).
    let canonical_dest = dest
        .canonicalize()
        .map_err(|e| ArchiveError::io("canonicalize destination", dest.to_path_buf(), e))?;

    // Find the deepest existing ancestor and verify it is within canonical_dest.
    // If an attacker placed a symlink inside `dest` pointing outside, this catches
    // it without creating any new directories on disk.
    if let Some(parent) = full_path.parent() {
        let mut existing: Option<&Path> = None;
        let mut candidate: Option<&Path> = Some(parent);
        while let Some(p) = candidate {
            if p.exists() {
                existing = Some(p);
                break;
            }
            candidate = p.parent();
        }
        if let Some(p) = existing {
            let canonical_ancestor = p
                .canonicalize()
                .map_err(|e| ArchiveError::io("canonicalize ancestor", p.to_path_buf(), e))?;
            if !canonical_ancestor.starts_with(&canonical_dest) {
                return Err(ArchiveError::InvalidPath {
                    path: entry_path.to_string(),
                    reason: "Path traversal attempt detected".to_string(),
                });
            }
        }
    }

    // Check if destination path is a symlink (FR-022: prevent Zip Slip via destination symlinks)
    match std::fs::symlink_metadata(&full_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(ArchiveError::invalid_path(
                    entry_path,
                    "Destination path is a symlink",
                ));
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(ArchiveError::io("stat", full_path.clone(), err));
        }
    }

    Ok(full_path)
}

/// Check if extraction is safe based on resource limits
///
/// Validates:
/// - Total uncompressed size doesn't exceed limit
/// - Individual file sizes are within limit
/// - Compression ratios are reasonable (zip bomb detection)
/// - Entry count is within limit
///
/// # Security
///
/// Prevents zip bomb attacks where small compressed files expand to
/// enormous sizes (e.g., 42.zip: 42KB → 4.5PB).
///
/// # Examples
///
/// ```no_run
/// use unified_archive::{Archive, security::{check_extraction_safe, ExtractionLimits}};
///
/// let archive = Archive::open("suspicious.zip")?;
/// let entries = archive.list_files()?;
///
/// // Use default limits
/// check_extraction_safe(entries, &ExtractionLimits::default())?;
///
/// // Or custom limits
/// let limits = ExtractionLimits {
///     max_total_size: 100 * 1024 * 1024, // 100 MB
///     max_file_size: 10 * 1024 * 1024,   // 10 MB
///     max_compression_ratio: 100.0,       // 100:1
///     max_entry_count: 1000,
///     max_mmap_size: None,                // Use platform default
/// };
/// check_extraction_safe(entries, &limits)?;
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub fn check_extraction_safe(entries: &[ArchiveEntry], limits: &ExtractionLimits) -> Result<()> {
    // Check entry count
    if entries.len() > limits.max_entry_count {
        return Err(ArchiveError::OperationBlocked {
            operation: "extract".to_string(),
            reason: format!(
                "Too many entries: {} exceeds limit of {}",
                entries.len(),
                limits.max_entry_count
            ),
        });
    }

    let mut total_uncompressed: u64 = 0;

    for entry in entries {
        // Skip directories
        if entry.is_directory() {
            continue;
        }

        // Check individual file size
        if let Some(size) = entry.size {
            if size > limits.max_file_size {
                return Err(ArchiveError::OperationBlocked {
                    operation: "extract".to_string(),
                    reason: format!(
                        "File '{}' too large: {} bytes exceeds limit of {} bytes",
                        entry.path, size, limits.max_file_size
                    ),
                });
            }

            total_uncompressed = total_uncompressed.saturating_add(size);
        }

        // Check compression ratio (zip bomb detection)
        if let (Some(uncompressed), Some(compressed)) = (entry.size, entry.compressed_size) {
            limits.check_ratio(uncompressed, compressed, &format!("File '{}'", entry.path))?;
        }
    }

    // Check total size
    if total_uncompressed > limits.max_total_size {
        return Err(ArchiveError::OperationBlocked {
            operation: "extract".to_string(),
            reason: format!(
                "Total uncompressed size {} bytes exceeds limit of {} bytes",
                total_uncompressed, limits.max_total_size
            ),
        });
    }

    Ok(())
}

/// Check overall archive compression ratio using archive file size.
///
/// This complements `check_extraction_safe` for formats where per-file
/// compressed sizes are unavailable (TAR.GZ, TAR.BZ2, TAR.XZ via libarchive).
/// Uses the archive file size on disk as the compressed size denominator.
pub fn check_archive_ratio(
    entries: &[ArchiveEntry],
    archive_path: &Path,
    limits: &ExtractionLimits,
) -> Result<()> {
    if limits.max_compression_ratio >= f64::MAX {
        return Ok(());
    }

    let archive_size = std::fs::metadata(archive_path)
        .map_err(|e| ArchiveError::io("stat archive", archive_path.to_path_buf(), e))?
        .len();

    if archive_size == 0 {
        return Ok(());
    }

    let total_uncompressed: u64 = entries
        .iter()
        .filter(|e| !e.is_directory())
        .filter_map(|e| e.size)
        .sum();

    limits.check_ratio(total_uncompressed, archive_size, "Archive")
}

/// Combined per-archive safety check: runs `check_extraction_safe` and
/// `check_archive_ratio` against the same entry slice. Use this from
/// extraction backends to avoid two separate traversals.
pub fn check_extraction_safe_with_archive(
    entries: &[ArchiveEntry],
    archive_path: &Path,
    limits: &ExtractionLimits,
) -> Result<()> {
    check_extraction_safe(entries, limits)?;
    check_archive_ratio(entries, archive_path, limits)
}

/// Check a single-entry extraction against resource limits.
///
/// Used by `extract_to_memory` and `extract_to_stream`, which otherwise bypass
/// the `check_extraction_safe` gate that wraps disk-based extraction paths.
/// The entry is identified by `file_path` (normalized archive path).
///
/// Returns `OperationBlocked` if:
/// * the entry is not present in `entries`,
/// * its uncompressed size exceeds `limits.max_file_size`, or
/// * its per-entry compression ratio exceeds `limits.max_compression_ratio`.
///
/// Entries without size metadata are allowed through — the backend stream is
/// still responsible for honoring the limit as bytes arrive.
pub fn check_single_entry_safe(
    entries: &[ArchiveEntry],
    file_path: &str,
    limits: &ExtractionLimits,
) -> Result<()> {
    let entry = entries
        .iter()
        .find(|e| e.path == file_path)
        .ok_or_else(|| ArchiveError::OperationBlocked {
            operation: "extract".to_string(),
            reason: format!("Entry '{}' not found in archive metadata", file_path),
        })?;

    if let Some(size) = entry.size {
        if size > limits.max_file_size {
            return Err(ArchiveError::OperationBlocked {
                operation: "extract".to_string(),
                reason: format!(
                    "File '{}' too large: {} bytes exceeds limit of {} bytes",
                    entry.path, size, limits.max_file_size
                ),
            });
        }

        if let Some(compressed) = entry.compressed_size {
            limits.check_ratio(size, compressed, &format!("File '{}'", entry.path))?;
        }
    }

    Ok(())
}

/// Verify CRC32 checksum of extracted data
///
/// Compares the computed CRC32 of the data against the expected value
/// from archive metadata.
///
/// # Security
///
/// Detects corrupted or tampered archive contents during extraction.
///
/// # Examples
///
/// ```
/// use unified_archive::security::verify_crc32;
///
/// let data = b"Hello, world!";
/// let expected_crc = 0xEBE6C6E6; // CRC32 of "Hello, world!"
///
/// // Valid CRC
/// verify_crc32(data, Some(expected_crc), "file.txt")?;
///
/// // Invalid CRC - returns error
/// let result = verify_crc32(data, Some(0xDEADBEEF), "file.txt");
/// assert!(result.is_err());
///
/// // No CRC to check - succeeds
/// verify_crc32(data, None, "file.txt")?;
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub fn verify_crc32(data: &[u8], expected_crc: Option<u32>, file_path: &str) -> Result<()> {
    if let Some(expected) = expected_crc {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(data);
        let actual = hasher.finalize();

        verify_crc32_value(actual, Some(expected), file_path)?;
    }
    Ok(())
}

pub(crate) fn verify_crc32_value(
    actual: u32,
    expected_crc: Option<u32>,
    file_path: &str,
) -> Result<()> {
    if let Some(expected) = expected_crc {
        if actual != expected {
            return Err(ArchiveError::Corruption {
                path: file_path.to_string(),
                details: format!(
                    "CRC32 mismatch: expected {:08X}, got {:08X}",
                    expected, actual
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sanitize_normal_path() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_normal");
        std::fs::create_dir_all(&dest)?;

        let result =
            sanitize_entry_path("subdir/file.txt", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("subdir/file.txt"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_path_traversal() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_traversal");
        std::fs::create_dir_all(&dest)?;

        // Parent directory references should be removed
        let result =
            sanitize_entry_path("../../etc/passwd", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("etc/passwd"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_absolute_path() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_absolute");
        std::fs::create_dir_all(&dest)?;

        // Absolute path should be converted to relative
        let result =
            sanitize_entry_path("/etc/passwd", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("etc/passwd"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_empty_path() {
        let dest = PathBuf::from("/tmp/test");

        // Path with only traversal components
        let result = sanitize_entry_path("../../..", &dest);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_crc32_valid() {
        let data = b"Hello, world!";
        let expected = 0xEBE6C6E6;
        verify_crc32(data, Some(expected), "test.txt").expect("CRC32 verification should succeed");
    }

    #[test]
    fn test_verify_crc32_invalid() {
        let data = b"Hello, world!";
        let wrong_crc = 0xDEADBEEF;
        let result = verify_crc32(data, Some(wrong_crc), "test.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_crc32_none() {
        let data = b"Hello, world!";
        verify_crc32(data, None, "test.txt").expect("CRC32 verification with None should succeed");
    }

    #[test]
    fn test_check_extraction_safe_normal() {
        let mut entry = ArchiveEntry::new("file.txt".to_string(), 0);
        entry.size = Some(1024);
        entry.compressed_size = Some(512);

        let entries = vec![entry];
        check_extraction_safe(&entries, &ExtractionLimits::default())
            .expect("Normal extraction should be safe");
    }

    #[test]
    fn test_check_extraction_safe_zip_bomb() {
        let mut entry = ArchiveEntry::new("bomb.txt".to_string(), 0);
        entry.size = Some(10 * 1024 * 1024 * 1024); // 10 GB
        entry.compressed_size = Some(1024); // 1 KB - 10,000,000:1 ratio!

        let entries = vec![entry];
        let result = check_extraction_safe(&entries, &ExtractionLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_check_extraction_safe_too_large() {
        let mut entry = ArchiveEntry::new("huge.bin".to_string(), 0);
        entry.size = Some(2 * 1024 * 1024 * 1024); // 2 GB

        let entries = vec![entry];
        let limits = ExtractionLimits {
            max_file_size: 1024 * 1024 * 1024, // 1 GB limit
            ..Default::default()
        };

        let result = check_extraction_safe(&entries, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_archive_internal_path_accepts_normal_names() {
        validate_archive_internal_path("file.txt").unwrap();
        validate_archive_internal_path("dir/file.txt").unwrap();
        validate_archive_internal_path("a/b/c/d.ext").unwrap();
        validate_archive_internal_path("./file.txt").unwrap();
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_empty() {
        assert!(validate_archive_internal_path("").is_err());
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_nul() {
        assert!(validate_archive_internal_path("a\0b").is_err());
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_traversal() {
        assert!(validate_archive_internal_path("../secret").is_err());
        assert!(validate_archive_internal_path("a/../secret").is_err());
        assert!(validate_archive_internal_path("a/b/..").is_err());
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_absolute() {
        assert!(validate_archive_internal_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_check_extraction_safe_too_many_entries() {
        let entries: Vec<_> = (0..1001)
            .map(|i| ArchiveEntry::new(format!("file{}.txt", i), i))
            .collect();

        let limits = ExtractionLimits {
            max_entry_count: 1000,
            ..Default::default()
        };

        let result = check_extraction_safe(&entries, &limits);
        assert!(result.is_err());
    }
}
