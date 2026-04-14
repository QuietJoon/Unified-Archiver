//! Archive extraction operations
//!
//! This module provides methods for extracting files from archives, including
//! full extraction, single file extraction, streaming extraction, and filtered extraction.

use crate::archive::{Archive, ArchiveBackend};
use crate::entry::ArchiveEntry;
use crate::error::ops;
use crate::error::{ArchiveError, Result};
use crate::options::ExtractionOptions;
use crate::security::{check_archive_ratio, check_extraction_safe, validate_entry_path};
use std::collections::HashSet;
use std::path::Path;

/// Ensure destination directory exists
///
/// Creates the directory and all parent directories if they don't exist.
/// Succeeds silently if the directory already exists.
fn ensure_destination(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| ArchiveError::io("create_dir", path, e))
}

fn check_overwrite_conflicts(
    entries: &[ArchiveEntry],
    destination: &Path,
    overwrite: bool,
    operation: &str,
) -> Result<()> {
    let mut seen_outputs = HashSet::new();

    for entry in entries.iter().filter(|entry| entry.is_file()) {
        let output_path = validate_entry_path(&entry.path, destination)?;

        // Detect in-batch collisions (multiple entries mapping to same output path)
        if !seen_outputs.insert(output_path.clone()) {
            return Err(ArchiveError::UnsupportedOperation {
                operation: operation.to_string(),
                reason: format!(
                    "Multiple entries map to same output path: '{}' (archive entry: '{}')",
                    output_path.display(),
                    entry.path
                ),
            });
        }

        if !overwrite && output_path.exists() {
            return Err(ArchiveError::UnsupportedOperation {
                operation: operation.to_string(),
                reason: format!(
                    "Destination file already exists: '{}' (archive entry: '{}')",
                    output_path.display(),
                    entry.path
                ),
            });
        }
    }

    Ok(())
}

fn open_archive_for_extraction(path: &Path, password: Option<&str>) -> Result<Archive> {
    if let Some(password) = password {
        match Archive::open_encrypted(path, password) {
            Ok(archive) => Ok(archive),
            Err(ArchiveError::Unsupported { .. }) => Archive::open(path),
            Err(err) => Err(err),
        }
    } else {
        Archive::open(path)
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_single_entry(
    archive_path: &std::path::Path,
    password: Option<&str>,
    entry_path: &str,
    dest: &std::path::Path,
    overwrite: bool,
    verify_crc32: bool,
    preserve_permissions: bool,
    preserve_times: bool,
) -> Result<()> {
    let extract_archive = open_archive_for_extraction(archive_path, password)?;
    match &extract_archive.backend {
        ArchiveBackend::Unrar(unrar) => {
            unrar.extract_file_with_options(entry_path, dest, overwrite)
        }
        ArchiveBackend::Piz(piz) => {
            piz.extract_file_with_options(entry_path, dest, overwrite, verify_crc32)
        }
        ArchiveBackend::SevenZ(sevenz) => {
            sevenz.extract_file_with_options(entry_path, dest, overwrite, verify_crc32)
        }
        ArchiveBackend::ZipWriter(_) => Err(ArchiveError::write_mode_only(ops::EXTRACT_FILTERED)),
        ArchiveBackend::ZipReader(zip) => {
            zip.extract_file_with_options(entry_path, dest, overwrite, verify_crc32)
        }
        ArchiveBackend::Libarchive(libarchive) => libarchive.extract_file_with_options(
            entry_path,
            dest,
            overwrite,
            preserve_permissions,
            preserve_times,
        ),
    }
}

impl Archive {
    /// Extract all files from archive (FR-019: Multi-part, FR-022: Symlink warnings)
    ///
    /// Extracts all files to the destination directory, preserving directory structure.
    ///
    /// **Symlinks and Hard Links (FR-022)**: On libarchive-backed formats (TAR, TAR.GZ,
    /// TAR.BZ2, TAR.XZ), symbolic links and hard links are **skipped** during extraction
    /// with warnings. Native backends (ZIP via Piz/ZipReader, 7z via SevenZ, RAR via
    /// UnRAR) do not yet classify link entries — they report all entries as `File` or
    /// `Directory`, so links may be extracted as regular files. Use
    /// [`Archive::check_symlinks()`] to scan for links (reliable on libarchive-backed
    /// formats only).
    ///
    /// # Multi-Part Archives
    ///
    /// For multi-part archives (`.z01`, `.001`, `.part1.rar`, etc.), extraction works
    /// seamlessly:
    /// - If you open the **first/main part** (e.g., `archive.zip`, `archive.001`, `archive.part1.rar`),
    ///   extraction reads data from all parts automatically
    /// - If you open a **middle/later part** (e.g., `archive.z02`, `archive.003`),
    ///   extraction may fail or be incomplete - always open the first part
    ///
    /// Use [`Archive::detect_multipart()`] to find all parts if needed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    /// use std::path::PathBuf;
    ///
    /// // Check for symlinks before extraction
    /// let archive = Archive::open("data.tar.gz")?;
    /// let warnings = archive.check_symlinks()?;
    /// if !warnings.is_empty() {
    ///     eprintln!("Note: {} symlinks will be skipped", warnings.len());
    /// }
    ///
    /// // Extract (symlinks automatically skipped)
    /// let options = ExtractionOptions {
    ///     destination: PathBuf::from("output"),
    ///     ..Default::default()
    /// };
    /// archive.extract_all(options)?;
    ///
    /// // Multi-part archive (open first part)
    /// let archive = Archive::open("large.zip")?; // Opens .zip (first part)
    /// let (is_multipart, parts) = archive.detect_multipart()?;
    /// if is_multipart {
    ///     println!("Extracting {} parts...", parts.len());
    /// }
    /// let options = ExtractionOptions {
    ///     destination: PathBuf::from("output"),
    ///     ..Default::default()
    /// };
    /// archive.extract_all(options)?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_all(&self, mut options: ExtractionOptions) -> Result<()> {
        ensure_destination(&options.destination)?;

        let extraction_archive = if let Some(password) = options.password.as_deref() {
            Some(open_archive_for_extraction(&self.path, Some(password))?)
        } else {
            None
        };
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // Security: Check extraction safety (zip bomb protection)
        // Use metadata-only listing to avoid decompressing data before limit checks
        let entries = archive.list_files_for_limits()?;
        check_extraction_safe(&entries, &options.limits)?;
        check_archive_ratio(&entries, &archive.path, &options.limits)?;
        check_overwrite_conflicts(
            &entries,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_ALL,
        )?;

        // Multi-part note: libarchive and UnRAR backends automatically handle
        // multi-part archives when opening the first part. The backend reads
        // subsequent parts (.z01, .z02, etc.) transparently.
        match &archive.backend {
            ArchiveBackend::Unrar(unrar) => unrar.extract_all_with_options(
                &options.destination,
                options.progress.as_mut(),
                options.overwrite,
            ),
            ArchiveBackend::Piz(piz) => piz.extract_all_with_options(
                &options.destination,
                options.progress.as_mut(),
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_all_with_options(
                &options.destination,
                options.progress.as_mut(),
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::ZipWriter(_) => Err(ArchiveError::write_mode_only(ops::EXTRACT_ALL)),
            ArchiveBackend::ZipReader(zip) => zip.extract_all_with_options(
                &options.destination,
                options.progress.as_mut(),
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::Libarchive(libarchive) => libarchive.extract_all_with_options(
                &options.destination,
                options.progress.as_mut(),
                options.overwrite,
                options.preserve_permissions,
                options.preserve_times,
            ),
        }
    }

    /// Extract a single file from archive
    pub fn extract_file(&self, file_path: &str, options: ExtractionOptions) -> Result<()> {
        ensure_destination(&options.destination)?;

        let extraction_archive = if let Some(password) = options.password.as_deref() {
            Some(open_archive_for_extraction(&self.path, Some(password))?)
        } else {
            None
        };
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // Use metadata-only listing to avoid decompressing data before limit checks
        let entries = archive.list_files_for_limits()?;
        let entry = entries
            .iter()
            .find(|entry| entry.path == file_path)
            .ok_or_else(|| {
                ArchiveError::format(
                    Some(archive.format()),
                    format!("File '{}' not found in archive", file_path),
                )
            })?;
        let to_extract = [entry.clone()];
        check_extraction_safe(&to_extract, &options.limits)?;
        check_overwrite_conflicts(
            &to_extract,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_FILE,
        )?;

        // Use the current archive handle for extraction
        match &archive.backend {
            ArchiveBackend::Unrar(unrar) => {
                unrar.extract_file_with_options(file_path, &options.destination, options.overwrite)
            }
            ArchiveBackend::Piz(piz) => piz.extract_file_with_options(
                file_path,
                &options.destination,
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_file_with_options(
                file_path,
                &options.destination,
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::ZipWriter(_) => Err(ArchiveError::write_mode_only(ops::EXTRACT_FILE)),
            ArchiveBackend::ZipReader(zip) => zip.extract_file_with_options(
                file_path,
                &options.destination,
                options.overwrite,
                options.verify_crc32,
            ),
            ArchiveBackend::Libarchive(libarchive) => libarchive.extract_file_with_options(
                file_path,
                &options.destination,
                options.overwrite,
                options.preserve_permissions,
                options.preserve_times,
            ),
        }
    }

    /// Extract a single file to memory
    ///
    /// Returns the file contents as a Vec<u8> without writing to disk.
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        match &self.backend {
            ArchiveBackend::Unrar(unrar) => unrar.extract_to_memory(file_path),
            ArchiveBackend::Piz(piz) => piz.extract_to_memory(file_path),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_to_memory(file_path),
            ArchiveBackend::ZipWriter(_) => {
                Err(ArchiveError::write_mode_only(ops::EXTRACT_TO_MEMORY))
            }
            ArchiveBackend::ZipReader(zip) => zip.extract_to_memory(file_path),
            ArchiveBackend::Libarchive(libarchive) => libarchive.extract_to_memory(file_path),
        }
    }

    /// Extract a single file to a stream
    ///
    /// Phase 2.4: Returns a StreamingExtractor that implements Read trait,
    /// allowing memory-efficient processing of large files without loading
    /// entire contents into RAM.
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::Archive;
    /// use std::io::Read;
    ///
    /// let archive = Archive::open("large.rar")?;
    /// let mut stream = archive.extract_to_stream("large_file.bin")?;
    ///
    /// let mut buffer = [0u8; 8192];
    /// while let Ok(n) = stream.read(&mut buffer) {
    ///     if n == 0 { break; }
    ///     // Process chunk without loading entire file
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        match &self.backend {
            ArchiveBackend::Unrar(unrar) => unrar.extract_to_stream(file_path),
            ArchiveBackend::Piz(piz) => piz.extract_to_stream(file_path),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_to_stream(file_path),
            ArchiveBackend::ZipWriter(_) => {
                Err(ArchiveError::write_mode_only(ops::EXTRACT_TO_STREAM))
            }
            ArchiveBackend::ZipReader(zip) => zip.extract_to_stream(file_path),
            ArchiveBackend::Libarchive(libarchive) => libarchive.extract_to_stream(file_path),
        }
    }

    /// Extract files matching a predicate
    ///
    /// Only files for which the predicate returns true will be extracted.
    ///
    /// Phase 2.7: Uses parallel extraction when multiple files match.
    /// For 4+ files, uses rayon to extract in parallel for improved performance.
    pub fn extract_filtered<F>(&self, predicate: F, options: ExtractionOptions) -> Result<()>
    where
        F: Fn(&ArchiveEntry) -> bool + Sync,
    {
        ensure_destination(&options.destination)?;

        // Get list of files to extract
        let listing_archive = if let Some(password) = options.password.as_deref() {
            Some(open_archive_for_extraction(&self.path, Some(password))?)
        } else {
            None
        };
        let archive = listing_archive.as_ref().unwrap_or(self);

        let entries = archive.list_files()?;
        let to_extract: Vec<_> = entries.iter().filter(|e| predicate(e)).collect();

        if to_extract.is_empty() {
            return Ok(());
        }

        let to_extract_entries: Vec<ArchiveEntry> =
            to_extract.iter().map(|entry| (*entry).clone()).collect();
        check_extraction_safe(&to_extract_entries, &options.limits)?;
        check_archive_ratio(&to_extract_entries, &self.path, &options.limits)?;
        check_overwrite_conflicts(
            &to_extract_entries,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_FILTERED,
        )?;

        // Phase 2.7: Parallel extraction for multiple files
        use rayon::prelude::*;

        let archive_path = self.path.clone();
        let extraction_dest = options.destination.clone();
        let password = options.password.clone();
        let overwrite = options.overwrite;
        let verify_crc32 = options.verify_crc32;
        let preserve_permissions = options.preserve_permissions;
        let preserve_times = options.preserve_times;

        let extract_one = |entry: &&ArchiveEntry| -> Result<()> {
            extract_single_entry(
                &archive_path,
                password.as_deref(),
                &entry.path,
                &extraction_dest,
                overwrite,
                verify_crc32,
                preserve_permissions,
                preserve_times,
            )
        };

        // Disable parallel extraction for solid archives (sequential decompression required)
        // Use the password-aware archive handle for solidness check to avoid
        // incorrect results when self is an unauthenticated handle
        let use_parallel = to_extract.len() >= 4 && !archive.is_solid().unwrap_or(false);

        if use_parallel {
            // Parallel extraction with early error return
            to_extract
                .par_iter()
                .map(extract_one)
                .collect::<Vec<Result<()>>>()
                .into_iter()
                .try_for_each(|r| r)?;
        } else {
            // Sequential extraction with early error return
            to_extract.iter().try_for_each(extract_one)?;
        }

        Ok(())
    }

    /// Extract multiple files by their paths
    ///
    /// Extracts only the specified files from the archive. Uses parallel extraction
    /// for 4+ files (rayon), sequential extraction for fewer files.
    ///
    /// # Arguments
    ///
    /// * `paths` - Array of file paths within the archive to extract
    /// * `options` - Extraction options including destination directory
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    /// use std::path::PathBuf;
    ///
    /// let archive = Archive::open("backup.zip")?;
    /// archive.extract_files(
    ///     &["readme.txt", "src/main.rs", "config.json"],
    ///     ExtractionOptions {
    ///         destination: PathBuf::from("./output"),
    ///         ..Default::default()
    ///     }
    /// )?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_files(&self, paths: &[&str], options: ExtractionOptions) -> Result<()> {
        ensure_destination(&options.destination)?;

        if paths.is_empty() {
            return Ok(());
        }

        // Deduplicate requested paths
        let mut seen = HashSet::new();
        let paths: Vec<&str> = paths.iter().copied().filter(|p| seen.insert(*p)).collect();

        let listing_archive = if let Some(password) = options.password.as_deref() {
            Some(open_archive_for_extraction(&self.path, Some(password))?)
        } else {
            None
        };
        let archive = listing_archive.as_ref().unwrap_or(self);

        let entries = archive.list_files()?;
        let mut to_extract_entries = Vec::with_capacity(paths.len());
        for &path in &paths {
            let entry = entries
                .iter()
                .find(|entry| entry.path == path)
                .ok_or_else(|| {
                    ArchiveError::format(
                        Some(self.format()),
                        format!("File '{}' not found in archive", path),
                    )
                })?;
            to_extract_entries.push(entry.clone());
        }
        check_extraction_safe(&to_extract_entries, &options.limits)?;
        check_archive_ratio(&to_extract_entries, &self.path, &options.limits)?;
        check_overwrite_conflicts(
            &to_extract_entries,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_FILES,
        )?;

        // Use parallel extraction for 4+ files, sequential for fewer
        use rayon::prelude::*;

        let archive_path = self.path.clone();
        let extraction_dest = options.destination.clone();
        let password = options.password.clone();
        let overwrite = options.overwrite;
        let verify_crc32 = options.verify_crc32;
        let preserve_permissions = options.preserve_permissions;
        let preserve_times = options.preserve_times;

        let extract_one = |path: &&str| -> Result<()> {
            extract_single_entry(
                &archive_path,
                password.as_deref(),
                path,
                &extraction_dest,
                overwrite,
                verify_crc32,
                preserve_permissions,
                preserve_times,
            )
        };

        // Disable parallel extraction for solid archives (sequential decompression required)
        // Use the password-aware archive handle for solidness check
        let use_parallel = paths.len() >= 4 && !archive.is_solid().unwrap_or(false);

        if use_parallel {
            // Parallel extraction with early error return
            paths
                .par_iter()
                .map(extract_one)
                .collect::<Vec<Result<()>>>()
                .into_iter()
                .try_for_each(|r| r)?;
        } else {
            // Sequential extraction with early error return
            paths.iter().try_for_each(extract_one)?;
        }

        Ok(())
    }

    /// Extract files by their IDs
    ///
    /// Extracts files using their sequential indices (0-based) from the archive's
    /// file list. The ID is consistent across all archive formats and represents
    /// the position returned by `list_files()`.
    ///
    /// Uses parallel extraction for 4+ files, sequential extraction for fewer.
    ///
    /// # Arguments
    ///
    /// * `ids` - Array of file IDs (indices) to extract
    /// * `options` - Extraction options including destination directory
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    /// use std::path::PathBuf;
    ///
    /// let archive = Archive::open("backup.rar")?;
    /// let entries = archive.list_files()?;
    ///
    /// // Print files with IDs
    /// for entry in entries.iter() {
    ///     println!("[{}] {}", entry.id, entry.path);
    /// }
    ///
    /// // Extract files with IDs 0 and 2
    /// archive.extract_by_ids(
    ///     &[0, 2],
    ///     ExtractionOptions {
    ///         destination: PathBuf::from("./output"),
    ///         ..Default::default()
    ///     }
    /// )?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_by_ids(&self, ids: &[usize], options: ExtractionOptions) -> Result<()> {
        ensure_destination(&options.destination)?;

        if ids.is_empty() {
            return Ok(());
        }

        // Deduplicate requested IDs
        let mut seen = HashSet::new();
        let ids: Vec<usize> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();

        let listing_archive = if let Some(password) = options.password.as_deref() {
            Some(open_archive_for_extraction(&self.path, Some(password))?)
        } else {
            None
        };
        let archive = listing_archive.as_ref().unwrap_or(self);

        // Get cached entry list
        let entries = archive.list_files()?;

        let mut paths = Vec::with_capacity(ids.len());
        let mut to_extract_entries = Vec::with_capacity(ids.len());
        for &id in &ids {
            let entry = entries
                .get(id)
                .ok_or_else(|| ArchiveError::UnsupportedOperation {
                    operation: ops::EXTRACT_BY_IDS.to_string(),
                    reason: format!(
                        "Invalid ID {}: archive has {} entries (valid IDs: 0-{})",
                        id,
                        entries.len(),
                        entries.len().saturating_sub(1)
                    ),
                })?;
            paths.push(entry.path.as_str());
            to_extract_entries.push(entry.clone());
        }

        check_extraction_safe(&to_extract_entries, &options.limits)?;
        check_archive_ratio(&to_extract_entries, &self.path, &options.limits)?;
        check_overwrite_conflicts(
            &to_extract_entries,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_BY_IDS,
        )?;

        // Use parallel extraction for 4+ files, sequential for fewer
        use rayon::prelude::*;

        let archive_path = self.path.clone();
        let extraction_dest = options.destination.clone();
        let password = options.password.clone();
        let overwrite = options.overwrite;
        let verify_crc32 = options.verify_crc32;
        let preserve_permissions = options.preserve_permissions;
        let preserve_times = options.preserve_times;

        let extract_one = |path: &&str| -> Result<()> {
            extract_single_entry(
                &archive_path,
                password.as_deref(),
                path,
                &extraction_dest,
                overwrite,
                verify_crc32,
                preserve_permissions,
                preserve_times,
            )
        };

        // Disable parallel extraction for solid archives (sequential decompression required)
        // Use the password-aware archive handle for solidness check
        let use_parallel = paths.len() >= 4 && !archive.is_solid().unwrap_or(false);

        if use_parallel {
            // Parallel extraction with early error return
            paths
                .par_iter()
                .map(extract_one)
                .collect::<Vec<Result<()>>>()
                .into_iter()
                .try_for_each(|r| r)?;
        } else {
            // Sequential extraction with early error return
            paths.iter().try_for_each(extract_one)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;
    use std::path::PathBuf;

    // ── ensure_destination tests ──

    #[test]
    fn test_ensure_destination_creates_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("new_subdir");
        assert!(!dest.exists());
        ensure_destination(&dest).unwrap();
        assert!(dest.exists());
    }

    #[test]
    fn test_ensure_destination_nested() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("a").join("b").join("c");
        ensure_destination(&dest).unwrap();
        assert!(dest.exists());
    }

    #[test]
    fn test_ensure_destination_existing_dir() {
        let temp = tempfile::tempdir().unwrap();
        // Already exists
        ensure_destination(temp.path()).unwrap();
    }

    // ── validate_entry_path tests ──

    #[test]
    fn test_validate_entry_path_simple() {
        let dest = Path::new("/output");
        let result = validate_entry_path("file.txt", dest).unwrap();
        assert_eq!(result, PathBuf::from("/output/file.txt"));
    }

    #[test]
    fn test_validate_entry_path_with_subdir() {
        let dest = Path::new("/output");
        let result = validate_entry_path("subdir/file.txt", dest).unwrap();
        assert_eq!(result, PathBuf::from("/output/subdir/file.txt"));
    }

    #[test]
    fn test_validate_entry_path_strips_traversal() {
        let dest = Path::new("/output");
        let result = validate_entry_path("../../../etc/passwd", dest).unwrap();
        // Path traversal components should be stripped
        assert_eq!(result, PathBuf::from("/output/etc/passwd"));
    }

    #[test]
    fn test_validate_entry_path_strips_absolute() {
        let dest = Path::new("/output");
        let result = validate_entry_path("/absolute/path.txt", dest).unwrap();
        assert_eq!(result, PathBuf::from("/output/absolute/path.txt"));
    }

    #[test]
    fn test_validate_entry_path_only_traversal() {
        let dest = Path::new("/output");
        let result = validate_entry_path("../../..", dest);
        assert!(
            result.is_err(),
            "Path with only traversal components should fail"
        );
    }

    // ── check_overwrite_conflicts tests ──

    // Helper to create a test ArchiveEntry
    fn make_entry(path: &str, entry_type: crate::entry::EntryType) -> ArchiveEntry {
        ArchiveEntry {
            id: 0,
            path: path.to_string(),
            size: Some(100),
            compressed_size: Some(50),
            modified: None,
            created: None,
            accessed: None,
            crc32: None,
            is_encrypted: false,
            entry_type,
            permissions: None,
            comment: None,
            attributes: None,
        }
    }

    #[test]
    fn test_check_overwrite_conflicts_no_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let entries = vec![make_entry("new_file.txt", crate::entry::EntryType::File)];
        assert!(check_overwrite_conflicts(&entries, temp.path(), false, "test").is_ok());
    }

    #[test]
    fn test_check_overwrite_conflicts_existing_file_no_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.txt");
        std::fs::write(&existing, b"content").unwrap();

        let entries = vec![make_entry("existing.txt", crate::entry::EntryType::File)];
        let result = check_overwrite_conflicts(&entries, temp.path(), false, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_overwrite_conflicts_existing_file_with_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.txt");
        std::fs::write(&existing, b"content").unwrap();

        let entries = vec![make_entry("existing.txt", crate::entry::EntryType::File)];
        // With overwrite=true, should succeed
        assert!(check_overwrite_conflicts(&entries, temp.path(), true, "test").is_ok());
    }

    #[test]
    fn test_check_overwrite_conflicts_directory_ignored() {
        let temp = tempfile::tempdir().unwrap();
        // Directories should be ignored (only files checked)
        let entries = vec![make_entry("some_dir/", crate::entry::EntryType::Directory)];
        assert!(check_overwrite_conflicts(&entries, temp.path(), false, "test").is_ok());
    }

    // ── open_archive_for_extraction tests ──

    #[test]
    fn test_open_archive_for_extraction_no_password() {
        let archive = open_archive_for_extraction(&fixture("test.zip"), None).unwrap();
        assert_eq!(archive.format(), crate::format::ArchiveFormat::Zip);
    }

    #[test]
    fn test_open_archive_for_extraction_with_password_unsupported() {
        // ZIP with password is unsupported in piz backend, should fall back to open()
        let archive = open_archive_for_extraction(&fixture("test.zip"), Some("password")).unwrap();
        assert_eq!(archive.format(), crate::format::ArchiveFormat::Zip);
    }

    // ── extract_all tests ──

    #[test]
    fn test_extract_all_zip() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_all(options).unwrap();
        // Verify at least one file was extracted
        let mut has_files = false;
        for entry in std::fs::read_dir(temp.path()).unwrap() {
            if entry.unwrap().path().is_file() {
                has_files = true;
                break;
            }
        }
        // Check either direct files or subdirectories
        assert!(
            std::fs::read_dir(temp.path()).unwrap().count() > 0,
            "Expected extracted files in output directory"
        );
        let _ = has_files;
    }

    #[test]
    fn test_extract_all_tar() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.tar")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_all(options).unwrap();
    }

    #[test]
    fn test_extract_all_creates_destination() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("auto_created");
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: dest.clone(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_all(options).unwrap();
        assert!(dest.exists());
    }

    // ── extract_file tests ──

    #[test]
    fn test_extract_file_zip() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        let first_file = entries.iter().find(|e| e.is_file()).unwrap();

        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_file(&first_file.path, options).unwrap();
    }

    #[test]
    fn test_extract_file_nonexistent() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        let result = archive.extract_file("nonexistent_file_xyz.txt", options);
        assert!(result.is_err());
    }

    // ── extract_to_memory tests ──

    #[test]
    fn test_extract_to_memory_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        let first_file = entries.iter().find(|e| e.is_file()).unwrap();
        let data = archive.extract_to_memory(&first_file.path).unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_extract_to_memory_nonexistent() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.extract_to_memory("nonexistent_xyz.txt");
        assert!(result.is_err());
    }

    // ── extract_files tests ──

    #[test]
    fn test_extract_files_empty_list() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        // Empty list should succeed (no-op)
        archive.extract_files(&[], options).unwrap();
    }

    #[test]
    fn test_extract_files_nonexistent_path() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        let result = archive.extract_files(&["nonexistent.txt"], options);
        assert!(result.is_err());
    }

    // ── extract_by_ids tests ──

    #[test]
    fn test_extract_by_ids_empty_list() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        // Empty list should succeed (no-op)
        archive.extract_by_ids(&[], options).unwrap();
    }

    #[test]
    fn test_extract_by_ids_invalid_id() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        let result = archive.extract_by_ids(&[99999], options);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_by_ids_valid_id() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_by_ids(&[0], options).unwrap();
    }

    // ── extract_filtered tests ──

    #[test]
    fn test_extract_filtered_no_matches() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            ..Default::default()
        };
        // Filter that matches nothing
        archive.extract_filtered(|_| false, options).unwrap();
    }

    #[test]
    fn test_extract_filtered_all_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive
            .extract_filtered(|entry| entry.is_file(), options)
            .unwrap();
    }

    // ── Overwrite behavior tests ──

    #[test]
    fn test_extract_all_no_overwrite_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();

        // First extraction
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            ..Default::default()
        };
        archive.extract_all(options).unwrap();

        // Second extraction without overwrite should fail
        let options2 = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: false,
            ..Default::default()
        };
        let result = archive.extract_all(options2);
        assert!(
            result.is_err(),
            "Should fail when files already exist and overwrite=false"
        );
    }
}
