//! Archive modification operations
//!
//! This module provides methods for modifying existing archives by adding, removing,
//! or replacing entries using a copy-on-write strategy.

use crate::archive::{Archive, ArchiveBackend, ArchiveMode};
use crate::error::ops;
use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use std::collections::HashSet;
use std::path::Path;

/// Configuration options for archive modification operations
#[derive(Debug, Clone)]
pub struct ModificationOptions {
    /// Whether to preserve original file metadata (timestamps, permissions)
    pub preserve_metadata: bool,

    /// Whether to create a backup of the original archive
    pub create_backup: bool,

    /// Suffix for backup files
    pub backup_suffix: String,

    /// Compression settings for the recreated archive.
    /// When `None`, uses format defaults (`CompressionLevel::Normal`, no password).
    pub compression: Option<crate::options::CompressionOptions>,
}

impl ModificationOptions {
    /// Create new modification options with defaults
    pub fn new() -> Self {
        Self {
            preserve_metadata: true,
            create_backup: false,
            backup_suffix: ".bak".to_string(),
            compression: None,
        }
    }

    /// Enable backup creation with specified suffix
    pub fn with_backup(mut self, suffix: &str) -> Self {
        self.create_backup = true;
        self.backup_suffix = suffix.to_string();
        self
    }

    /// Disable metadata preservation
    pub fn without_metadata_preservation(mut self) -> Self {
        self.preserve_metadata = false;
        self
    }
}

impl Default for ModificationOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks modifications to an archive in Modify mode
#[derive(Default)]
pub(crate) struct ModificationTracker {
    /// Files to remove from archive
    pub(crate) removed: HashSet<String>,
    /// Files to add to archive (path, data)
    pub(crate) added: Vec<(String, Vec<u8>)>,
    /// Directories to add to archive
    pub(crate) added_directories: Vec<String>,
}

impl Archive {
    /// Open an archive for modification
    ///
    /// Opens an existing archive in Modify mode. Modifications are tracked in memory
    /// and applied when `commit_changes()` is called.
    ///
    /// # Arguments
    /// * `path` - Archive file path
    ///
    /// # Returns
    /// * `Ok(Archive)` - Archive in Modify mode
    /// * `Err` - If archive doesn't exist or format doesn't support modification
    ///
    /// # Supported Formats
    /// - ZIP ✅
    /// - 7z ✅
    /// - TAR variants ❌ (not yet implemented)
    /// - RAR ❌ (read-only)
    pub fn modify(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let format = ArchiveFormat::detect(&path_buf)?;

        // Check if format supports modification
        if !format.can_modify() {
            return Err(ArchiveError::operation_blocked(
                ops::MODIFY,
                format!("{:?} archives do not support modification", format),
            ));
        }

        // Reject encrypted archives — password-aware modification not yet supported
        {
            let check = Archive::open(&path_buf)?;
            if check.is_encrypted()? {
                return Err(ArchiveError::operation_blocked(
                    ops::MODIFY,
                    "Encrypted archives cannot be modified (password-aware modification not yet supported)",
                ));
            }
        }

        // Open for reading to validate
        let backend = match format {
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                return Err(ArchiveError::operation_blocked(
                    ops::MODIFY,
                    "RAR archives are read-only",
                ));
            }
            _ => {
                use crate::ffi::libarchive_wrapper::LibarchiveArchive;
                let archive = LibarchiveArchive::open(&path_buf)?;
                ArchiveBackend::Libarchive(archive)
            }
        };

        Ok(Self {
            backend,
            path: path_buf,
            mode: ArchiveMode::Modify,
            format,
            entry_cache: once_cell::sync::OnceCell::new(),
            modifications: Some(ModificationTracker::default()),
            mod_options: None,
            _backing_tempfile: None,
        })
    }

    /// Open an archive for modification with explicit [`ModificationOptions`].
    ///
    /// Behaves identically to [`Archive::modify`] except the supplied
    /// `options` are honored at `commit_changes()` time. When `None` is
    /// desired (i.e. defaults), call `modify(path)` instead.
    ///
    /// Honored fields:
    /// - `create_backup` + `backup_suffix`: original archive is copied to
    ///   `<path>.<backup_suffix>` immediately before the temp file is renamed
    ///   into place.
    /// - `preserve_metadata`: when true, `commit_changes()` preserves timestamps
    ///   and Unix permissions via metadata-aware add helpers on both ZipWriter
    ///   and libarchive backends.
    /// - `compression`: overrides compression settings during archive recreation.
    pub fn modify_with_options(
        path: impl AsRef<Path>,
        options: ModificationOptions,
    ) -> Result<Self> {
        let mut archive = Self::modify(path)?;
        archive.mod_options = Some(options);
        Ok(archive)
    }

    /// Add an entry to archive (for Modify mode)
    ///
    /// In Modify mode, this tracks the addition. Changes are applied when `commit_changes()` is called.
    /// In Write mode, use `add_file_from_data()` instead.
    pub fn add_entry(&mut self, path: &str, data: &[u8]) -> Result<()> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::ADD_ENTRY,
                "Only available in Modify mode. Use add_file_from_data() for Write mode",
            ));
        }

        crate::security::validate_archive_internal_path(path)?;

        let modifications = self.modifications.as_mut().ok_or_else(|| {
            ArchiveError::operation_blocked(ops::ADD_ENTRY, "Archive not in Modify mode")
        })?;

        modifications.added.push((path.to_string(), data.to_vec()));
        Ok(())
    }

    /// Add a directory entry to archive (for Modify mode)
    ///
    /// In Modify mode, this tracks the directory addition. Changes are applied
    /// when `commit_changes()` is called.
    pub fn add_directory_entry(&mut self, path: &str) -> Result<()> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::ADD_DIRECTORY_ENTRY,
                "Only available in Modify mode",
            ));
        }

        crate::security::validate_archive_internal_path(path)?;

        let modifications = self.modifications.as_mut().ok_or_else(|| {
            ArchiveError::operation_blocked(ops::ADD_DIRECTORY_ENTRY, "Archive not in Modify mode")
        })?;

        modifications.added_directories.push(path.to_string());
        Ok(())
    }

    /// Remove an entry from archive (for Modify mode)
    ///
    /// Marks an entry for removal. Changes are applied when `commit_changes()` is called.
    pub fn remove_entry(&mut self, path: &str) -> Result<()> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::REMOVE_ENTRY,
                "Only available in Modify mode",
            ));
        }

        crate::security::validate_archive_internal_path(path)?;

        let modifications = self.modifications.as_mut().ok_or_else(|| {
            ArchiveError::operation_blocked(ops::REMOVE_ENTRY, "Archive not in Modify mode")
        })?;

        modifications.removed.insert(path.to_string());
        Ok(())
    }

    /// Replace an entry in archive (for Modify mode)
    ///
    /// Convenience method that removes the old entry and adds a new one.
    pub fn replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.remove_entry(path)?;
        self.add_entry(path, data)?;
        Ok(())
    }

    /// Get pending operations count
    pub fn pending_operations(&self) -> usize {
        self.modifications
            .as_ref()
            .map(|m| m.added.len() + m.removed.len() + m.added_directories.len())
            .unwrap_or(0)
    }

    /// Clear pending operations
    pub fn clear_operations(&mut self) {
        if let Some(modifications) = self.modifications.as_mut() {
            modifications.added.clear();
            modifications.removed.clear();
            modifications.added_directories.clear();
        }
    }

    /// Commit changes to archive (for Modify mode)
    ///
    /// Applies all tracked modifications (additions, removals, replacements) to the archive.
    /// This creates a new archive with the modifications and replaces the original.
    ///
    /// # Process
    /// 1. Create temporary archive with same format
    /// 2. Copy entries from original (except removed ones)
    /// 3. Add new entries
    /// 4. Replace original file
    ///
    /// # Errors
    /// Returns error if not in Modify mode or if I/O operations fail.
    pub fn commit_changes(mut self) -> Result<()> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::COMMIT_CHANGES,
                "Only available in Modify mode",
            ));
        }

        let modifications = self.modifications.take().ok_or_else(|| {
            ArchiveError::operation_blocked(ops::COMMIT_CHANGES, "No modification tracker")
        })?;

        // Snapshot modification options (defaults if unset).
        let mod_options = self.mod_options.take().unwrap_or_default();

        // If no modifications, return early. We deliberately do *not* create a
        // backup in this branch — there is nothing to roll back to.
        if modifications.added.is_empty()
            && modifications.removed.is_empty()
            && modifications.added_directories.is_empty()
        {
            return Ok(());
        }

        // Unique temp path beside the original. nanos+pid handles cross-process
        // collisions; the AtomicU64 counter guarantees uniqueness for parallel
        // commits within the same process (test harnesses, async runtimes).
        let temp_path = {
            use std::sync::atomic::{AtomicU64, Ordering};
            use std::time::{SystemTime, UNIX_EPOCH};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let pid = std::process::id();
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            self.path
                .with_extension(format!("tmp.{pid}.{nanos}.{counter}"))
        };

        // Use a closure so that temp file is cleaned up on any failure
        let result = (|| -> Result<()> {
            // Use caller-supplied compression settings, or format defaults
            let options = mod_options
                .compression
                .clone()
                .unwrap_or_else(|| crate::options::CompressionOptions::new(self.format));
            let mut new_archive = Self::create(&temp_path, options)?;

            let preserve = mod_options.preserve_metadata;

            // For ZIP sources, load the archive comment and per-entry
            // compression method directly via the `zip` crate. The modify path
            // routes source reads through libarchive, which does not surface
            // this metadata — a side-car lookup is the cheapest way to keep
            // round-trips faithful without switching backends.
            let zip_extras = if self.format == ArchiveFormat::Zip
                && matches!(new_archive.backend, ArchiveBackend::ZipWriter(_))
            {
                Some(load_zip_source_extras(&self.path)?)
            } else {
                None
            };

            // Copy all entries from original except removed ones. Stream large
            // entries via `_unchecked` extraction to avoid the per-entry
            // list_files() rebuild + safety pre-check that the public methods
            // perform — we already have the listing in hand and trust paths
            // sourced from the archive itself.
            let entries = self.list_files()?;
            for entry in entries {
                if modifications.removed.contains(&entry.path) {
                    continue;
                }

                let streamable_size = entry.size;
                let compression_override = zip_extras
                    .as_ref()
                    .and_then(|extras| extras.per_entry_compression.get(&entry.path).copied());

                if preserve {
                    match &mut new_archive.backend {
                        ArchiveBackend::ZipWriter(w) => {
                            let mut stream = self.extract_to_stream_unchecked(&entry.path)?;
                            w.add_file_from_reader_with_metadata(
                                &entry.path,
                                &mut stream,
                                entry,
                                compression_override,
                            )?;
                        }
                        ArchiveBackend::Libarchive(b) => match streamable_size {
                            Some(size) => {
                                let mut stream = self.extract_to_stream_unchecked(&entry.path)?;
                                b.add_file_from_reader_with_metadata(
                                    &entry.path,
                                    &mut stream,
                                    size,
                                    entry,
                                )?;
                            }
                            None => {
                                let data = self.extract_to_memory_unchecked(&entry.path)?;
                                b.add_file_from_data_with_metadata(&entry.path, &data, entry)?;
                            }
                        },
                        _ => {
                            let data = self.extract_to_memory_unchecked(&entry.path)?;
                            new_archive.add_file_from_data(&entry.path, &data)?;
                        }
                    }
                } else {
                    match &mut new_archive.backend {
                        ArchiveBackend::ZipWriter(w) => {
                            let mut stream = self.extract_to_stream_unchecked(&entry.path)?;
                            w.add_file_from_reader(&entry.path, &mut stream)?;
                        }
                        ArchiveBackend::Libarchive(b) => match streamable_size {
                            Some(size) => {
                                let mut stream = self.extract_to_stream_unchecked(&entry.path)?;
                                b.add_file_from_reader(&entry.path, &mut stream, size)?;
                            }
                            None => {
                                let data = self.extract_to_memory_unchecked(&entry.path)?;
                                b.add_file_from_data(&entry.path, &data)?;
                            }
                        },
                        _ => {
                            let data = self.extract_to_memory_unchecked(&entry.path)?;
                            new_archive.add_file_from_data(&entry.path, &data)?;
                        }
                    }
                }
            }

            // Add new directory entries
            for dir_path in modifications.added_directories {
                new_archive.add_directory(&dir_path)?;
            }

            // Add new entries
            for (path, data) in modifications.added {
                new_archive.add_file_from_data(&path, &data)?;
            }

            // Propagate ZIP archive-level comment (if any) before finalizing.
            if let (Some(extras), ArchiveBackend::ZipWriter(w)) =
                (zip_extras.as_ref(), &mut new_archive.backend)
            {
                if !extras.archive_comment.is_empty() {
                    w.set_archive_comment(&extras.archive_comment)?;
                }
            }

            // Finalize new archive
            new_archive.finish()?;

            // If a backup is requested, copy the original aside before the
            // atomic replace. We use copy (not rename) so the new file can
            // still take the original's path. A failure here aborts the
            // commit so the caller can decide whether to retry without
            // backups; we have not touched the original yet.
            if mod_options.create_backup {
                let backup_path = backup_path_for(&self.path, &mod_options.backup_suffix);
                std::fs::copy(&self.path, &backup_path)
                    .map_err(|e| ArchiveError::io("backup", &backup_path, e))?;
            }

            // Replace original file with new one
            rename_with_overwrite(&temp_path, &self.path)
        })();

        // Clean up temp file on any failure (finish, write, or rename)
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }

        result
    }
}

/// Side-car ZIP metadata read directly from the source file via the `zip`
/// crate during `commit_changes`. Used to preserve archive-level comments and
/// per-entry compression methods that the libarchive-backed source reader
/// does not surface.
struct ZipSourceExtras {
    archive_comment: Vec<u8>,
    per_entry_compression: std::collections::HashMap<String, zip::CompressionMethod>,
}

// PERF(DEF-005): this reopens the ZIP and walks its central directory a second
// time (libarchive already did the first walk for list_files). Tolerable for
// thousand-entry archives but the real fix is to switch the ZIP modify source
// from libarchive to the zip crate so both walks collapse into one.
fn load_zip_source_extras(path: &Path) -> Result<ZipSourceExtras> {
    let file =
        std::fs::File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| {
        ArchiveError::format(
            Some(ArchiveFormat::Zip),
            format!("Read ZIP metadata: {}", e),
        )
    })?;

    let archive_comment = zip.comment().to_vec();

    let mut per_entry_compression = std::collections::HashMap::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read ZIP entry {}: {}", i, e),
            )
        })?;
        // Normalize to forward slashes to match the listing paths.
        let name = entry.name().replace('\\', "/");
        per_entry_compression.insert(name, entry.compression());
    }

    Ok(ZipSourceExtras {
        archive_comment,
        per_entry_compression,
    })
}

/// Compose the backup path for a given archive path and suffix.
///
/// Suffixes that already begin with a dot are appended verbatim
/// (e.g. `.bak` → `archive.zip.bak`); suffixes without a leading dot get
/// one added (`bak` → `archive.zip.bak`). An empty suffix is treated as
/// `.bak` to avoid the surprising case of overwriting the original.
fn backup_path_for(archive_path: &Path, suffix: &str) -> std::path::PathBuf {
    let normalized = if suffix.is_empty() {
        ".bak".to_string()
    } else if suffix.starts_with('.') {
        suffix.to_string()
    } else {
        format!(".{suffix}")
    };
    let mut path = archive_path.as_os_str().to_owned();
    path.push(&normalized);
    std::path::PathBuf::from(path)
}

/// Platform-specific rename that overwrites existing files
///
/// On Unix: `std::fs::rename()` atomically replaces existing files
/// On Windows: `std::fs::rename()` fails if destination exists, so we use `MoveFileExW`
#[cfg(not(windows))]
fn rename_with_overwrite(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| ArchiveError::io("rename", from, e))
}

#[cfg(windows)]
fn rename_with_overwrite(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    // Convert paths to wide strings (null-terminated UTF-16)
    let from_wide: Vec<u16> = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to_wide: Vec<u16> = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // MOVEFILE_REPLACE_EXISTING = 0x1
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;

    // Link to kernel32.dll MoveFileExW
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;

        fn GetLastError() -> u32;
    }

    let result = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING,
        )
    };

    if result == 0 {
        let error_code = unsafe { GetLastError() };
        return Err(ArchiveError::io(
            "rename",
            from,
            std::io::Error::from_raw_os_error(error_code as i32),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    // ── ModificationOptions tests ──

    #[test]
    fn test_modification_options_new_defaults() {
        let opts = ModificationOptions::new();
        assert!(opts.preserve_metadata);
        assert!(!opts.create_backup);
        assert_eq!(opts.backup_suffix, ".bak");
    }

    #[test]
    fn test_modification_options_default_matches_new() {
        let from_new = ModificationOptions::new();
        let from_default = ModificationOptions::default();
        assert_eq!(from_new.preserve_metadata, from_default.preserve_metadata);
        assert_eq!(from_new.create_backup, from_default.create_backup);
        assert_eq!(from_new.backup_suffix, from_default.backup_suffix);
    }

    #[test]
    fn test_modification_options_with_backup() {
        let opts = ModificationOptions::new().with_backup(".backup");
        assert!(opts.create_backup);
        assert_eq!(opts.backup_suffix, ".backup");
        // preserve_metadata should remain unchanged
        assert!(opts.preserve_metadata);
    }

    #[test]
    fn test_modification_options_without_metadata_preservation() {
        let opts = ModificationOptions::new().without_metadata_preservation();
        assert!(!opts.preserve_metadata);
        // Other fields should remain unchanged
        assert!(!opts.create_backup);
        assert_eq!(opts.backup_suffix, ".bak");
    }

    #[test]
    fn test_modification_options_chained_builders() {
        let opts = ModificationOptions::new()
            .with_backup(".orig")
            .without_metadata_preservation();
        assert!(opts.create_backup);
        assert_eq!(opts.backup_suffix, ".orig");
        assert!(!opts.preserve_metadata);
    }

    #[test]
    fn test_modification_options_debug() {
        let opts = ModificationOptions::new();
        let debug = format!("{:?}", opts);
        assert!(debug.contains("ModificationOptions"));
        assert!(debug.contains("preserve_metadata"));
        assert!(debug.contains("create_backup"));
    }

    #[test]
    fn test_modification_options_clone() {
        let opts = ModificationOptions::new().with_backup(".bak2");
        let cloned = opts.clone();
        assert_eq!(cloned.preserve_metadata, opts.preserve_metadata);
        assert_eq!(cloned.create_backup, opts.create_backup);
        assert_eq!(cloned.backup_suffix, opts.backup_suffix);
    }

    // ── ModificationTracker tests ──

    #[test]
    fn test_modification_tracker_default_empty() {
        let tracker = ModificationTracker::default();
        assert!(tracker.removed.is_empty());
        assert!(tracker.added.is_empty());
    }

    #[test]
    fn test_modification_tracker_add_entries() {
        let mut tracker = ModificationTracker::default();
        tracker
            .added
            .push(("file.txt".to_string(), b"data".to_vec()));
        assert_eq!(tracker.added.len(), 1);
        assert_eq!(tracker.added[0].0, "file.txt");
        assert_eq!(tracker.added[0].1, b"data");
    }

    #[test]
    fn test_modification_tracker_remove_entries() {
        let mut tracker = ModificationTracker::default();
        tracker.removed.insert("old.txt".to_string());
        assert_eq!(tracker.removed.len(), 1);
        assert!(tracker.removed.contains("old.txt"));
    }

    // ── Archive::modify() tests ──

    #[test]
    fn test_modify_zip_archive() {
        let archive = Archive::modify(fixture("test.zip"));
        assert!(archive.is_ok());
        let archive = archive.unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
    }

    #[test]
    fn test_modify_7z_archive() {
        let archive = Archive::modify(fixture("test.7z"));
        assert!(archive.is_ok());
        let archive = archive.unwrap();
        assert_eq!(archive.format(), ArchiveFormat::SevenZip);
    }

    #[test]
    fn test_modify_rar_archive_fails() {
        let result = Archive::modify(fixture("test.rar"));
        assert!(result.is_err());
        let err = result.err().unwrap();
        let msg = format!("{}", err);
        assert!(
            msg.contains("read-only") || msg.contains("modify") || msg.contains("RAR"),
            "Error should mention RAR limitation: {msg}"
        );
    }

    #[test]
    fn test_modify_nonexistent_archive() {
        let result = Archive::modify("/nonexistent/path/archive.zip");
        assert!(result.is_err());
    }

    #[test]
    fn test_modify_tar_archive() {
        // TAR doesn't support can_modify() (only ZIP and 7z do)
        let result = Archive::modify(fixture("test.tar"));
        assert!(result.is_err());
    }

    // ── add_entry() tests ──

    #[test]
    fn test_add_entry_in_modify_mode() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        let result = archive.add_entry("new.txt", b"hello world");
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_entry_not_in_modify_mode() {
        // Open in read mode
        let mut archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.add_entry("new.txt", b"hello");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Modify mode") || msg.contains("add_entry"));
    }

    #[test]
    fn test_add_entry_multiple() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.add_entry("a.txt", b"aaa").unwrap();
        archive.add_entry("b.txt", b"bbb").unwrap();
        archive.add_entry("c.txt", b"ccc").unwrap();
        assert_eq!(archive.pending_operations(), 3);
    }

    #[test]
    fn test_add_entry_empty_data() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        let result = archive.add_entry("empty.txt", b"");
        assert!(result.is_ok());
    }

    // ── add_directory_entry() tests ──

    #[test]
    fn test_add_directory_entry_in_modify_mode() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        let result = archive.add_directory_entry("subdir");
        assert!(result.is_ok());
        assert_eq!(archive.pending_operations(), 1);
    }

    #[test]
    fn test_add_directory_entry_not_in_modify_mode() {
        let mut archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.add_directory_entry("subdir");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Modify mode") || msg.contains("add_directory_entry"));
    }

    #[test]
    fn test_add_directory_entry_tracks_in_pending() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.add_directory_entry("dir1").unwrap();
        archive.add_directory_entry("dir2").unwrap();
        assert_eq!(archive.pending_operations(), 2);
    }

    #[test]
    fn test_clear_operations_clears_directories() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.add_directory_entry("dir1").unwrap();
        archive.add_entry("file.txt", b"data").unwrap();
        assert_eq!(archive.pending_operations(), 2);
        archive.clear_operations();
        assert_eq!(archive.pending_operations(), 0);
    }

    // ── remove_entry() tests ──

    #[test]
    fn test_remove_entry_in_modify_mode() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        let result = archive.remove_entry("some_file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_entry_not_in_modify_mode() {
        let mut archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.remove_entry("file.txt");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Modify mode") || msg.contains("remove_entry"));
    }

    #[test]
    fn test_remove_entry_tracks_path() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.remove_entry("to_remove.txt").unwrap();
        assert_eq!(archive.pending_operations(), 1);
    }

    // ── replace_entry() tests ──

    #[test]
    fn test_replace_entry_in_modify_mode() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        let result = archive.replace_entry("file.txt", b"replacement");
        assert!(result.is_ok());
        // replace = remove + add, so 2 pending operations
        assert_eq!(archive.pending_operations(), 2);
    }

    #[test]
    fn test_replace_entry_not_in_modify_mode() {
        let mut archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.replace_entry("file.txt", b"new data");
        assert!(result.is_err());
    }

    // ── pending_operations() tests ──

    #[test]
    fn test_pending_operations_initially_zero() {
        let archive = Archive::modify(fixture("test.zip")).unwrap();
        assert_eq!(archive.pending_operations(), 0);
    }

    #[test]
    fn test_pending_operations_counts_adds_and_removes() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.add_entry("new.txt", b"data").unwrap();
        assert_eq!(archive.pending_operations(), 1);
        archive.remove_entry("old.txt").unwrap();
        assert_eq!(archive.pending_operations(), 2);
    }

    #[test]
    fn test_pending_operations_read_mode_returns_zero() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        // No modifications tracker in read mode
        assert_eq!(archive.pending_operations(), 0);
    }

    // ── clear_operations() ──

    #[test]
    fn test_clear_operations_clears_pending() {
        let mut archive = Archive::modify(fixture("test.zip")).unwrap();
        archive.add_entry("file.txt", b"data").unwrap();
        archive.remove_entry("old.txt").unwrap();
        assert_eq!(archive.pending_operations(), 2);
        archive.clear_operations();
        assert_eq!(archive.pending_operations(), 0);
    }

    // ── commit_changes() tests ──

    #[test]
    fn test_commit_changes_not_in_modify_mode() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let result = archive.commit_changes();
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Modify mode") || msg.contains("commit_changes"));
    }

    #[test]
    fn test_commit_changes_no_modifications_succeeds() {
        // When there are no pending modifications, commit should succeed immediately
        let temp = tempfile::tempdir().unwrap();
        let test_path = temp.path().join("test_commit_noop.zip");

        // Create a valid archive first
        let options = crate::options::CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&test_path, options).unwrap();
        archive.add_file_from_data("file.txt", b"content").unwrap();
        archive.finish().unwrap();

        // Open in modify mode and commit with no changes
        let archive = Archive::modify(&test_path).unwrap();
        assert_eq!(archive.pending_operations(), 0);
        let result = archive.commit_changes();
        assert!(result.is_ok());
    }

    #[test]
    fn test_commit_changes_add_entry_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let test_path = temp.path().join("test_commit_add.zip");

        let options = crate::options::CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&test_path, options).unwrap();
        archive
            .add_file_from_data("original.txt", b"original")
            .unwrap();
        archive.finish().unwrap();

        let mut archive = Archive::modify(&test_path).unwrap();
        archive.add_entry("added.txt", b"added content").unwrap();
        archive.commit_changes().unwrap();

        let archive = Archive::open(&test_path).unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_commit_changes_remove_entry_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let test_path = temp.path().join("test_commit_remove.zip");

        let options = crate::options::CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&test_path, options).unwrap();
        archive.add_file_from_data("keep.txt", b"keep").unwrap();
        archive.add_file_from_data("remove.txt", b"remove").unwrap();
        archive.finish().unwrap();

        let mut archive = Archive::modify(&test_path).unwrap();
        archive.remove_entry("remove.txt").unwrap();
        archive.commit_changes().unwrap();

        let archive = Archive::open(&test_path).unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "keep.txt");
    }

    // ── rename_with_overwrite() tests (Unix) ──

    #[cfg(not(windows))]
    mod rename_tests {
        use super::*;

        #[test]
        fn test_rename_with_overwrite_basic() {
            let temp = tempfile::tempdir().unwrap();
            let src = temp.path().join("source.txt");
            let dst = temp.path().join("dest.txt");

            std::fs::write(&src, b"hello").unwrap();
            rename_with_overwrite(&src, &dst).unwrap();

            assert!(!src.exists());
            assert!(dst.exists());
            assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
        }

        #[test]
        fn test_rename_with_overwrite_replaces_existing() {
            let temp = tempfile::tempdir().unwrap();
            let src = temp.path().join("source.txt");
            let dst = temp.path().join("dest.txt");

            std::fs::write(&src, b"new content").unwrap();
            std::fs::write(&dst, b"old content").unwrap();

            rename_with_overwrite(&src, &dst).unwrap();

            assert!(!src.exists());
            assert_eq!(std::fs::read(&dst).unwrap(), b"new content");
        }

        #[test]
        fn test_rename_with_overwrite_nonexistent_source() {
            let temp = tempfile::tempdir().unwrap();
            let src = temp.path().join("nonexistent.txt");
            let dst = temp.path().join("dest.txt");

            let result = rename_with_overwrite(&src, &dst);
            assert!(result.is_err());
        }
    }
}
