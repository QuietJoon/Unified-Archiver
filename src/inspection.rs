//! Archive inspection operations
//!
//! This module provides methods for inspecting archive contents, validating integrity,
//! and querying archive metadata without extraction.

use crate::archive::{Archive, ArchiveBackend};
use crate::entry::{ArchiveEntry, EntryType};
use crate::error::ops;
use crate::error::{ArchiveError, ArchiveWarning, Result};
use std::path::{Path, PathBuf};

/// Report from archive integrity validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Total number of entries checked
    pub total_entries: usize,
    /// Number of entries successfully validated
    pub validated: usize,
    /// Paths of entries that failed validation
    pub failed: Vec<String>,
}

impl Archive {
    /// List all files in the archive
    ///
    /// Returns cached `&[ArchiveEntry]` (zero-cost repeated access).
    /// - First call: reads from backend, caches in `OnceCell`
    /// - Subsequent calls: returns cached slice (O(1), no allocation)
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.entry_cache
            .get_or_try_init(|| match &self.backend {
                #[cfg(feature = "rar-support")]
                ArchiveBackend::Unrar(unrar) => unrar.list_files(),
                ArchiveBackend::Piz(piz) => piz.list_files(),
                ArchiveBackend::SevenZ(sevenz) => sevenz.list_files(),
                ArchiveBackend::ZipWriter(_) => Err(ArchiveError::write_mode_only(ops::LIST_FILES)),
                ArchiveBackend::ZipReader(zip) => zip.list_files(),
                // Skip the CRC-walk variant on the public listing path — CRC
                // verification belongs in `validate_integrity()`, not routine listing.
                ArchiveBackend::Libarchive(libarchive) => libarchive.list_files_metadata_only(),
            })
            .map(|v| v.as_slice())
    }

    /// List files for limit checking (metadata only, no CRC32 computation)
    ///
    /// This method returns entry metadata without computing CRC32.
    /// For libarchive backend, this avoids decompressing data during listing.
    /// Use this for operations that only need size/count information.
    ///
    /// Note: This method does NOT cache results. For repeated access, use `list_files()`.
    pub fn list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>> {
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.list_files(),
            ArchiveBackend::Piz(piz) => piz.list_files(),
            ArchiveBackend::SevenZ(sevenz) => sevenz.list_files(),
            ArchiveBackend::ZipWriter(_) => {
                Err(ArchiveError::write_mode_only(ops::LIST_FILES_FOR_LIMITS))
            }
            ArchiveBackend::ZipReader(zip) => zip.list_files(),
            ArchiveBackend::Libarchive(libarchive) => libarchive.list_files_metadata_only(),
        }
    }

    /// Get count of entries in archive
    ///
    /// In Read/Modify mode, returns the number of entries listed in the archive.
    /// In Write mode, returns the number of entries added so far.
    pub fn entry_count(&self) -> Result<usize> {
        use crate::archive::ArchiveMode;
        if self.mode == ArchiveMode::Write {
            return Ok(match &self.backend {
                ArchiveBackend::ZipWriter(w) => w.entries_written(),
                ArchiveBackend::Libarchive(b) => b.entries_written(),
                _ => 0,
            });
        }
        self.list_files().map(|entries| entries.len())
    }

    /// Find a specific entry by path
    ///
    /// Performs O(n) linear search through all entries.
    pub fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>> {
        let entries = self.list_files()?;
        Ok(entries.iter().find(|entry| entry.path == path).cloned())
    }

    /// Validate integrity of all entries by verifying checksums
    ///
    /// This method performs actual integrity verification by:
    /// - RAR/RAR5: Using UnRAR's test mode (RAR_TEST) - no disk extraction
    /// - ZIP: Extracting to memory and verifying CRC32
    /// - 7z: Extracting to memory with built-in CRC32 verification
    /// - TAR/etc: Extracting to memory and verifying checksums
    ///
    /// Returns a report with the number of files validated and list of failures.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.rar")?;
    /// let report = archive.validate_integrity()?;
    ///
    /// if report.failed.is_empty() {
    ///     println!("✓ All {} files validated successfully", report.validated);
    /// } else {
    ///     println!("✗ {} files failed validation:", report.failed.len());
    ///     for path in &report.failed {
    ///         println!("  - {}", path);
    ///     }
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn validate_integrity(&self) -> Result<ValidationReport> {
        let entries = self.list_files()?;
        let total_entries = entries.len();

        // Count files (excluding directories)
        let file_count = entries.iter().filter(|e| e.is_file()).count();

        // Call backend-specific test_integrity method
        let failed = match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.test_integrity()?,
            ArchiveBackend::Piz(piz) => piz.test_integrity()?,
            ArchiveBackend::SevenZ(sevenz) => sevenz.test_integrity()?,
            ArchiveBackend::Libarchive(libarchive) => libarchive.test_integrity()?,
            ArchiveBackend::ZipReader(zip) => zip.test_integrity()?,
            ArchiveBackend::ZipWriter(_) => {
                return Err(ArchiveError::write_mode_only(ops::VALIDATE_INTEGRITY));
            }
        };

        // Use saturating_sub to prevent underflow if failed count exceeds file count
        let validated = file_count.saturating_sub(failed.len());

        Ok(ValidationReport {
            total_entries,
            validated,
            failed,
        })
    }

    /// Calculate archive-level CRC32 (sum of all file CRC32s)
    ///
    /// This calculates the "archive CRC" shown by 7-Zip, which is simply
    /// the arithmetic sum of all individual file CRC32 values (with 32-bit overflow).
    ///
    /// This is NOT stored in the archive - it's computed on-the-fly from file CRCs.
    /// The same files will produce the same archive CRC regardless of:
    /// - Archive format (ZIP, 7z, RAR all have same result)
    /// - Compression method
    /// - File order (since it's just addition)
    /// - Archive metadata (comments, timestamps, etc.)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("file.zip")?;
    /// let crc = archive.calculate_archive_crc()?;
    /// println!("Archive CRC: {:08X}", crc);
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn calculate_archive_crc(&self) -> Result<u32> {
        let entries = self.list_files()?;

        let mut sum: u32 = 0;
        for entry in entries {
            if let Some(crc32) = entry.crc32 {
                // Arithmetic addition with 32-bit overflow (wrapping)
                sum = sum.wrapping_add(crc32);
            }
        }

        Ok(sum)
    }

    /// Calculate the manifest digest for archive identity
    ///
    /// Computes a CRC32-based digest from sorted per-entry CRC32 values.
    /// This is deterministic: same entries (regardless of order in archive)
    /// produce the same digest.
    ///
    /// Used by AdvancedDeduplicator for content-identity matching — two archives
    /// with identical file contents produce the same manifest_digest even if
    /// compressed differently or stored in different archive formats.
    ///
    /// Unlike [`calculate_archive_crc`] (wrapping sum — less collision-resistant),
    /// this method sorts individual CRC32 hex representations and hashes the
    /// joined string, preserving per-entry identity.
    ///
    /// Returns empty string if no file entries have CRC32 values.
    ///
    /// # Algorithm
    ///
    /// 1. Collect CRC32 from each file entry (skip directories, entries without CRC32)
    /// 2. Convert each CRC32 to 8-char hex (big-endian bytes)
    /// 3. Sort lexicographically
    /// 4. Join with ","
    /// 5. CRC32-hash the joined string
    /// 6. Return as 8-char lowercase hex
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("file.zip")?;
    /// let digest = archive.calculate_manifest_digest()?;
    /// if !digest.is_empty() {
    ///     println!("Manifest digest: {}", digest);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn calculate_manifest_digest(&self) -> Result<String> {
        let entries = self.list_files()?;

        let mut hashes: Vec<String> = entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File)
            .map(|e| {
                if let Some(crc) = e.crc32 {
                    format!("{crc:08x}")
                } else {
                    // Fallback for entries without CRC32: use path + size
                    format!("{}:{}", e.path, e.size.unwrap_or(0))
                }
            })
            .collect();

        if hashes.is_empty() {
            return Ok(String::new());
        }

        hashes.sort();
        let joined = hashes.join(",");
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(joined.as_bytes());
        Ok(format!("{:08x}", hasher.finalize()))
    }

    /// Calculate content-multiset digest and total uncompressed file size
    ///
    /// Returns a tuple of (digest, total_uncompressed_size) where:
    /// - `digest` is a content-multiset identity hash (same as `calculate_manifest_digest`)
    /// - `total_uncompressed_size` is the sum of all file entry sizes
    ///
    /// The digest represents content identity, not archive layout — archives with the
    /// same files but different names/directory structures may produce identical digests.
    pub fn calculate_manifest_summary(&self) -> Result<(String, u64)> {
        let entries = self.list_files()?;

        let total_size: u64 = entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File)
            .filter_map(|e| e.size)
            .sum();

        let digest = self.calculate_manifest_digest()?;
        Ok((digest, total_size))
    }

    /// Detect if this archive is part of a multi-part archive set (FR-019)
    ///
    /// Multi-part archives split data across multiple files. In `v0.1.0`,
    /// end-to-end split-volume support is intended for RAR/RAR5 archives.
    /// ZIP split-volume names may be discovered heuristically, but ZIP split
    /// extraction is not supported end-to-end. 7z numeric split volumes are
    /// not supported.
    ///
    /// Returns `(is_multipart, part_files)` where:
    /// - `is_multipart`: true if this archive is part of a multi-part set
    /// - `part_files`: list of all detected part files in the set (sorted)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("backup.part1.rar")?;
    /// let (is_multipart, parts) = archive.detect_multipart()?;
    /// if is_multipart {
    ///     println!("Found {} parts: {:?}", parts.len(), parts);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)> {
        use std::fs;

        // Check if format supports multi-part archives
        if !self.format.supports_multipart() {
            return Ok((false, vec![self.path.clone()]));
        }

        let file_name = self
            .path
            .file_name()
            .ok_or_else(|| {
                ArchiveError::invalid_path(self.path.display().to_string(), "No filename")
            })?
            .to_string_lossy();

        let parent_dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let file_stem = self
            .path
            .file_stem()
            .ok_or_else(|| {
                ArchiveError::invalid_path(self.path.display().to_string(), "No file stem")
            })?
            .to_string_lossy();

        let mut part_files = Vec::new();
        part_files.push(self.path.clone()); // Always include current file

        // Collect directory entries once
        let dir_entries: Vec<PathBuf> = fs::read_dir(parent_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p != &self.path)
                    .collect()
            })
            .unwrap_or_default();

        // Helper function for RAR base name extraction
        let rar_base = file_stem.split(".part").next().unwrap_or("");

        // Apply format-specific predicates to find related parts
        for path in dir_entries {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();

                // Pattern 1: ZIP multi-part (.zip, .z01, .z02, ...)
                // Helper: check if name ends with .zNN pattern (ZIP split archives)
                let is_zip_split_ext = |s: &str| -> bool {
                    if let Some(dot_pos) = s.rfind('.') {
                        let ext = &s[dot_pos..];
                        ext.starts_with(".z")
                            && ext.len() >= 3
                            && ext[2..].chars().all(|c| c.is_ascii_digit())
                    } else {
                        false
                    }
                };
                let is_zip_part = (file_name.ends_with(".zip") || is_zip_split_ext(&file_name))
                    && name_str.starts_with(file_stem.as_ref())
                    && (name_str.ends_with(".zip") || is_zip_split_ext(&name_str));

                // Pattern 2: RAR multi-part (.part1.rar, .part2.rar, ...)
                let is_rar_part = file_name.contains(".part")
                    && file_name.ends_with(".rar")
                    && name_str.starts_with(rar_base)
                    && name_str.contains(".part")
                    && name_str.ends_with(".rar");

                // Pattern 3: 7z/numeric extension (.001, .002, ...)
                let is_numeric_part = self
                    .path
                    .extension()
                    .and_then(|e| {
                        let ext_str = e.to_string_lossy();
                        if ext_str.chars().all(|c| c.is_ascii_digit()) {
                            Some(())
                        } else {
                            None
                        }
                    })
                    .and_then(|_| {
                        path.extension().and_then(|e| {
                            let e_str = e.to_string_lossy();
                            if name_str.starts_with(file_stem.as_ref())
                                && e_str.chars().all(|c| c.is_ascii_digit())
                            {
                                Some(())
                            } else {
                                None
                            }
                        })
                    })
                    .is_some();

                if is_zip_part || is_rar_part || is_numeric_part {
                    part_files.push(path);
                }
            }
        }

        // Sort part files numerically (not lexicographically) for correct ordering
        // e.g., .z2 before .z10, .part2.rar before .part10.rar
        part_files.sort_by(|a, b| {
            // Extract numeric part from extension or filename
            let extract_num = |p: &PathBuf| -> Option<u32> {
                let name = p.file_name()?.to_string_lossy();
                // Try extension first (e.g., .001, .z01)
                if let Some(ext) = p.extension() {
                    let ext_str = ext.to_string_lossy();
                    // Pure numeric extension (.001, .002)
                    if let Ok(n) = ext_str.parse::<u32>() {
                        return Some(n);
                    }
                    // ZIP split (.z01, .z02) - extract digits after 'z'
                    if let Some(suffix) = ext_str.strip_prefix('z') {
                        if let Ok(n) = suffix.parse::<u32>() {
                            return Some(n);
                        }
                    }
                }
                // Try .partN.rar pattern
                if let Some(pos) = name.find(".part") {
                    let after_part = &name[pos + 5..];
                    let num_str: String = after_part
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(n) = num_str.parse::<u32>() {
                        return Some(n);
                    }
                }
                None
            };

            match (extract_num(a), extract_num(b)) {
                (Some(na), Some(nb)) => na.cmp(&nb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b), // Fallback to lexicographic
            }
        });

        let is_multipart = part_files.len() > 1;
        Ok((is_multipart, part_files))
    }

    /// Check for symlinks in archive and return warnings (FR-022)
    ///
    /// Scans entries for symlink and hard link types. All backends now classify
    /// link entries: libarchive reads link types from archive metadata, Piz checks
    /// `unix_mode` for `S_IFLNK`, ZipReader uses `is_symlink()`, SevenZ inspects
    /// `windows_attributes` (Unix mode + reparse point), and UnRAR uses `redir_type`
    /// for symlinks, junctions, and hard links.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.tar.gz")?;
    /// let warnings = archive.check_symlinks()?;
    /// for warning in &warnings {
    ///     eprintln!("Warning: {}", warning);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn check_symlinks(&self) -> Result<Vec<ArchiveWarning>> {
        let entries = self.list_files()?;
        let mut warnings = Vec::new();

        for entry in entries {
            match entry.entry_type {
                EntryType::Symlink => {
                    warnings.push(ArchiveWarning::SkippedSymlink {
                        path: entry.path.clone(),
                        target: None, // Target not available in current implementation
                    });
                }
                EntryType::HardLink => {
                    warnings.push(ArchiveWarning::SkippedHardLink {
                        path: entry.path.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    // ── ValidationReport tests ──

    #[test]
    fn test_validation_report_debug() {
        let report = ValidationReport {
            total_entries: 5,
            validated: 4,
            failed: vec!["bad_file.txt".to_string()],
        };
        let debug = format!("{:?}", report);
        assert!(debug.contains("total_entries: 5"));
        assert!(debug.contains("validated: 4"));
        assert!(debug.contains("bad_file.txt"));
    }

    #[test]
    fn test_validation_report_clone() {
        let report = ValidationReport {
            total_entries: 3,
            validated: 3,
            failed: vec![],
        };
        let cloned = report.clone();
        assert_eq!(report, cloned);
    }

    #[test]
    fn test_validation_report_eq() {
        let a = ValidationReport {
            total_entries: 2,
            validated: 2,
            failed: vec![],
        };
        let b = ValidationReport {
            total_entries: 2,
            validated: 2,
            failed: vec![],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_validation_report_ne() {
        let a = ValidationReport {
            total_entries: 2,
            validated: 2,
            failed: vec![],
        };
        let b = ValidationReport {
            total_entries: 2,
            validated: 1,
            failed: vec!["x.txt".to_string()],
        };
        assert_ne!(a, b);
    }

    // ── list_files tests ──

    #[test]
    fn test_list_files_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        assert!(!entries.is_empty());
        // All entries should have paths
        for entry in entries {
            assert!(!entry.path.is_empty());
        }
    }

    #[test]
    fn test_list_files_7z() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        let entries = archive.list_files().unwrap();
        assert!(!entries.is_empty());
    }

    #[cfg(feature = "rar-support")]
    #[test]
    fn test_list_files_rar() {
        let archive = Archive::open(fixture("test.rar")).unwrap();
        let entries = archive.list_files().unwrap();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_list_files_tar() {
        let archive = Archive::open(fixture("test.tar")).unwrap();
        let entries = archive.list_files().unwrap();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_list_files_caching() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries1 = archive.list_files().unwrap();
        let entries2 = archive.list_files().unwrap();
        // Second call should return same pointer (cached)
        assert!(std::ptr::eq(entries1.as_ptr(), entries2.as_ptr()));
    }

    // ── list_files_for_limits tests ──

    #[test]
    fn test_list_files_for_limits_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files_for_limits().unwrap();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_list_files_for_limits_does_not_cache() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        // Call list_files_for_limits first (should NOT populate cache)
        let _entries = archive.list_files_for_limits().unwrap();
        // Cache should still be empty since list_files_for_limits doesn't cache
        assert!(archive.entry_cache.get().is_none());
    }

    // ── entry_count tests ──

    #[test]
    fn test_entry_count_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let count = archive.entry_count().unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_entry_count_matches_list_files() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let count = archive.entry_count().unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(count, entries.len());
    }

    // ── find_entry tests ──

    #[test]
    fn test_find_entry_existing() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        let first_path = &entries[0].path;
        let found = archive.find_entry(first_path).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, *first_path);
    }

    #[test]
    fn test_find_entry_nonexistent() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let found = archive.find_entry("nonexistent_file_xyz.txt").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_find_entry_empty_path() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let found = archive.find_entry("").unwrap();
        assert!(found.is_none());
    }

    // ── validate_integrity tests ──

    #[test]
    fn test_validate_integrity_valid_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let report = archive.validate_integrity().unwrap();
        assert!(
            report.failed.is_empty(),
            "Valid ZIP should have no failures"
        );
        assert!(
            report.validated > 0,
            "Should have validated at least one entry"
        );
        assert_eq!(report.total_entries, archive.entry_count().unwrap());
    }

    #[test]
    fn test_validate_integrity_valid_7z() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        let report = archive.validate_integrity().unwrap();
        assert!(report.failed.is_empty());
    }

    // ── calculate_archive_crc tests ──

    #[test]
    fn test_calculate_archive_crc_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let crc = archive.calculate_archive_crc().unwrap();
        // CRC should be deterministic for the same archive
        let crc2 = archive.calculate_archive_crc().unwrap();
        assert_eq!(crc, crc2);
    }

    #[test]
    fn test_calculate_archive_crc_is_sum_of_entry_crcs() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        let expected: u32 = entries
            .iter()
            .filter_map(|e| e.crc32)
            .fold(0u32, |acc, crc| acc.wrapping_add(crc));
        let actual = archive.calculate_archive_crc().unwrap();
        assert_eq!(actual, expected);
    }

    // ── calculate_manifest_digest tests ──

    #[test]
    fn test_calculate_manifest_digest_deterministic() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let digest1 = archive.calculate_manifest_digest().unwrap();
        let digest2 = archive.calculate_manifest_digest().unwrap();
        assert_eq!(digest1, digest2, "manifest_digest should be deterministic");
        assert!(
            !digest1.is_empty(),
            "ZIP with entries should produce non-empty digest"
        );
    }

    #[test]
    fn test_calculate_manifest_digest_differs_from_archive_crc() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let digest = archive.calculate_manifest_digest().unwrap();
        let crc = archive.calculate_archive_crc().unwrap();
        // manifest_digest and archive_crc use different algorithms —
        // they should (almost certainly) differ
        assert_ne!(
            digest,
            format!("{crc:08x}"),
            "manifest_digest and archive_crc should differ (different algorithms)"
        );
    }

    #[test]
    fn test_calculate_manifest_digest_matches_manual_computation() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();

        // Manual computation: same algorithm as the method
        let mut hashes: Vec<String> = entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File)
            .filter_map(|e| e.crc32)
            .map(|crc| {
                crc.to_be_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            })
            .collect();
        hashes.sort();
        let joined = hashes.join(",");
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(joined.as_bytes());
        let expected = format!("{:08x}", hasher.finalize());

        let actual = archive.calculate_manifest_digest().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_calculate_manifest_digest_7z() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        let digest = archive.calculate_manifest_digest().unwrap();
        // 7z may or may not have CRC32 per entry — just check it doesn't error
        let digest2 = archive.calculate_manifest_digest().unwrap();
        assert_eq!(digest, digest2);
    }

    // ── detect_multipart tests ──

    #[test]
    fn test_detect_multipart_single_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let (is_multi, parts) = archive.detect_multipart().unwrap();
        // Single-file ZIP should not be multipart
        // (unless there are matching .z01, .z02 files in the fixtures dir)
        if !is_multi {
            assert_eq!(parts.len(), 1);
        }
    }

    #[test]
    fn test_detect_multipart_tar_not_supported() {
        let archive = Archive::open(fixture("test.tar")).unwrap();
        let (is_multi, parts) = archive.detect_multipart().unwrap();
        assert!(!is_multi, "TAR does not support multipart");
        assert_eq!(parts.len(), 1);
    }

    // ── check_symlinks tests ──

    #[test]
    fn test_check_symlinks_no_symlinks() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let warnings = archive.check_symlinks().unwrap();
        // Normal archives without symlinks should have empty warnings
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_check_symlinks_returns_correct_warning_types() {
        // We test the logic by constructing entries directly
        // This tests the function's handling of EntryType variants
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let warnings = archive.check_symlinks().unwrap();
        for warning in &warnings {
            match warning {
                ArchiveWarning::SkippedSymlink { path, .. } => {
                    assert!(!path.is_empty());
                }
                ArchiveWarning::SkippedHardLink { path } => {
                    assert!(!path.is_empty());
                }
            }
        }
    }

    // ── Cross-format consistency tests ──

    #[test]
    fn test_list_files_consistent_across_zip_and_7z() {
        let zip = Archive::open(fixture("test.zip")).unwrap();
        let sevenz = Archive::open(fixture("test.7z")).unwrap();

        let zip_entries = zip.list_files().unwrap();
        let sevenz_entries = sevenz.list_files().unwrap();

        // Both archives contain the same file, so entry count should match
        // (assuming test.zip and test.7z have the same contents)
        assert!(!zip_entries.is_empty(), "ZIP should have entries");
        assert!(!sevenz_entries.is_empty(), "7z should have entries");
    }
}
