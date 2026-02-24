//! Archive entry metadata

use std::time::SystemTime;

/// Type of archive entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    HardLink,
    Other,
}

/// Platform-specific file attributes
///
/// Phase 1: Extended metadata for platform-specific attributes
#[derive(Clone, Debug, Default)]
pub struct FileAttributes {
    /// Windows file attributes (FILE_ATTRIBUTE_*)
    pub windows: Option<u32>,

    /// Unix extended attributes (name-value pairs)
    pub unix_xattr: Option<Vec<(String, Vec<u8>)>>,

    /// Archive-specific attributes (format-dependent string)
    pub archive_specific: Option<String>,
}

/// Metadata for a single file or directory within an archive
///
/// This structure provides consistent metadata across all archive formats.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Full path within archive (UTF-8, forward slashes)
    pub path: String,

    /// File size in bytes (uncompressed), None for directories
    pub size: Option<u64>,

    /// Compressed size in bytes, None for uncompressed formats or directories
    pub compressed_size: Option<u64>,

    /// Last modification time (UTC)
    pub modified: Option<SystemTime>,

    /// CRC32 checksum, None if not available
    pub crc32: Option<u32>,

    /// Entry type
    pub entry_type: EntryType,

    /// Unix permissions (if preserved by format)
    pub permissions: Option<u32>,

    // Phase 1: Enhanced metadata fields
    /// Creation time (UTC)
    pub created: Option<SystemTime>,

    /// Last access time (UTC)
    pub accessed: Option<SystemTime>,

    /// Compression ratio (compressed_size / size), computed lazily
    pub compression_ratio: Option<f64>,

    /// Whether entry is encrypted/password-protected
    pub is_encrypted: bool,

    /// Entry comment (if format supports)
    pub comment: Option<String>,

    /// Platform-specific file attributes
    pub attributes: Option<FileAttributes>,

    /// Sequential index/ID within archive (0-based)
    ///
    /// This ID is consistent across all archive formats and represents
    /// the position of this entry in the archive's file list.
    /// Use this ID with `extract_by_ids()` for efficient batch extraction.
    pub id: usize,
}

impl ArchiveEntry {
    /// Create a new archive entry
    pub fn new(path: String, id: usize) -> Self {
        Self {
            path,
            size: None,
            compressed_size: None,
            modified: None,
            crc32: None,
            entry_type: EntryType::File,
            permissions: None,
            // Phase 1: Enhanced metadata
            created: None,
            accessed: None,
            compression_ratio: None,
            is_encrypted: false,
            comment: None,
            attributes: None,
            id,
        }
    }

    /// Compute and cache compression ratio if both sizes are available
    pub fn compute_compression_ratio(&mut self) {
        if let (Some(compressed), Some(original)) = (self.compressed_size, self.size) {
            if original > 0 {
                self.compression_ratio = Some(compressed as f64 / original as f64);
            }
        }
    }

    /// Check if this entry is a directory
    pub fn is_directory(&self) -> bool {
        self.entry_type == EntryType::Directory
    }

    /// Check if this entry is a file
    pub fn is_file(&self) -> bool {
        self.entry_type == EntryType::File
    }

    /// Check if this entry is a symlink
    pub fn is_symlink(&self) -> bool {
        self.entry_type == EntryType::Symlink
    }

    /// Check if this entry is a hard link
    pub fn is_hardlink(&self) -> bool {
        self.entry_type == EntryType::HardLink
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ArchiveEntry::new defaults ──

    #[test]
    fn test_new_entry_defaults() {
        let entry = ArchiveEntry::new("docs/readme.md".into(), 0);
        assert_eq!(entry.path, "docs/readme.md");
        assert_eq!(entry.id, 0);
        assert_eq!(entry.entry_type, EntryType::File);
        assert!(entry.size.is_none());
        assert!(entry.compressed_size.is_none());
        assert!(entry.modified.is_none());
        assert!(entry.crc32.is_none());
        assert!(entry.permissions.is_none());
        assert!(entry.created.is_none());
        assert!(entry.accessed.is_none());
        assert!(entry.compression_ratio.is_none());
        assert!(!entry.is_encrypted);
        assert!(entry.comment.is_none());
        assert!(entry.attributes.is_none());
    }

    #[test]
    fn test_new_entry_with_id() {
        let entry = ArchiveEntry::new("file.txt".into(), 42);
        assert_eq!(entry.id, 42);
    }

    // ── Type checks ──

    #[test]
    fn test_is_file() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        assert!(entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());

        entry.entry_type = EntryType::Directory;
        assert!(!entry.is_file());
    }

    #[test]
    fn test_is_directory() {
        let mut entry = ArchiveEntry::new("folder/".into(), 0);
        entry.entry_type = EntryType::Directory;
        assert!(entry.is_directory());
        assert!(!entry.is_file());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());
    }

    #[test]
    fn test_is_symlink() {
        let mut entry = ArchiveEntry::new("link".into(), 0);
        entry.entry_type = EntryType::Symlink;
        assert!(entry.is_symlink());
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_hardlink());
    }

    #[test]
    fn test_is_hardlink() {
        let mut entry = ArchiveEntry::new("hlink".into(), 0);
        entry.entry_type = EntryType::HardLink;
        assert!(entry.is_hardlink());
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
    }

    #[test]
    fn test_other_entry_type() {
        let mut entry = ArchiveEntry::new("special".into(), 0);
        entry.entry_type = EntryType::Other;
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());
    }

    // ── Compression ratio ──

    #[test]
    fn test_compute_compression_ratio_normal() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(1000);
        entry.compressed_size = Some(400);
        entry.compute_compression_ratio();
        let ratio = entry.compression_ratio.unwrap();
        assert!((ratio - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_compression_ratio_store() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(500);
        entry.compressed_size = Some(500);
        entry.compute_compression_ratio();
        let ratio = entry.compression_ratio.unwrap();
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_compression_ratio_zero_original() {
        let mut entry = ArchiveEntry::new("empty.txt".into(), 0);
        entry.size = Some(0);
        entry.compressed_size = Some(0);
        entry.compute_compression_ratio();
        // Division by zero protection: should remain None
        assert!(entry.compression_ratio.is_none());
    }

    #[test]
    fn test_compute_compression_ratio_missing_compressed() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(1000);
        // compressed_size is None
        entry.compute_compression_ratio();
        assert!(entry.compression_ratio.is_none());
    }

    #[test]
    fn test_compute_compression_ratio_missing_original() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        // size is None
        entry.compressed_size = Some(400);
        entry.compute_compression_ratio();
        assert!(entry.compression_ratio.is_none());
    }

    #[test]
    fn test_compute_compression_ratio_both_missing() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.compute_compression_ratio();
        assert!(entry.compression_ratio.is_none());
    }

    #[test]
    fn test_compute_compression_ratio_larger_compressed() {
        // Incompressible data can have ratio > 1.0
        let mut entry = ArchiveEntry::new("random.bin".into(), 0);
        entry.size = Some(100);
        entry.compressed_size = Some(120);
        entry.compute_compression_ratio();
        let ratio = entry.compression_ratio.unwrap();
        assert!((ratio - 1.2).abs() < f64::EPSILON);
    }

    // ── EntryType traits ──

    #[test]
    fn test_entry_type_clone_copy() {
        let t = EntryType::File;
        let cloned = t.clone();
        let copied = t; // Copy
        assert_eq!(t, cloned);
        assert_eq!(t, copied);
    }

    #[test]
    fn test_entry_type_debug() {
        assert_eq!(format!("{:?}", EntryType::File), "File");
        assert_eq!(format!("{:?}", EntryType::Directory), "Directory");
        assert_eq!(format!("{:?}", EntryType::Symlink), "Symlink");
        assert_eq!(format!("{:?}", EntryType::HardLink), "HardLink");
        assert_eq!(format!("{:?}", EntryType::Other), "Other");
    }

    #[test]
    fn test_entry_type_equality() {
        assert_eq!(EntryType::File, EntryType::File);
        assert_ne!(EntryType::File, EntryType::Directory);
        assert_ne!(EntryType::Symlink, EntryType::HardLink);
    }

    // ── FileAttributes ──

    #[test]
    fn test_file_attributes_default() {
        let attrs = FileAttributes::default();
        assert!(attrs.windows.is_none());
        assert!(attrs.unix_xattr.is_none());
        assert!(attrs.archive_specific.is_none());
    }

    #[test]
    fn test_file_attributes_with_values() {
        let attrs = FileAttributes {
            windows: Some(0x20), // FILE_ATTRIBUTE_ARCHIVE
            unix_xattr: Some(vec![("user.mime_type".into(), b"text/plain".to_vec())]),
            archive_specific: Some("solid".into()),
        };
        assert_eq!(attrs.windows, Some(0x20));
        assert_eq!(attrs.unix_xattr.as_ref().unwrap().len(), 1);
        assert_eq!(attrs.archive_specific.as_deref(), Some("solid"));
    }

    #[test]
    fn test_file_attributes_clone() {
        let attrs = FileAttributes {
            windows: Some(0x01),
            unix_xattr: None,
            archive_specific: Some("test".into()),
        };
        let cloned = attrs.clone();
        assert_eq!(cloned.windows, Some(0x01));
        assert_eq!(cloned.archive_specific.as_deref(), Some("test"));
    }

    // ── ArchiveEntry clone ──

    #[test]
    fn test_archive_entry_clone() {
        let mut entry = ArchiveEntry::new("file.txt".into(), 5);
        entry.size = Some(1024);
        entry.crc32 = Some(0xDEADBEEF);
        entry.is_encrypted = true;
        entry.comment = Some("test comment".into());
        entry.entry_type = EntryType::File;

        let cloned = entry.clone();
        assert_eq!(cloned.path, "file.txt");
        assert_eq!(cloned.id, 5);
        assert_eq!(cloned.size, Some(1024));
        assert_eq!(cloned.crc32, Some(0xDEADBEEF));
        assert!(cloned.is_encrypted);
        assert_eq!(cloned.comment.as_deref(), Some("test comment"));
    }

    // ── Edge cases ──

    #[test]
    fn test_entry_with_empty_path() {
        let entry = ArchiveEntry::new(String::new(), 0);
        assert_eq!(entry.path, "");
    }

    #[test]
    fn test_entry_with_unicode_path() {
        let entry = ArchiveEntry::new("日本語/ファイル.txt".into(), 0);
        assert_eq!(entry.path, "日本語/ファイル.txt");
    }

    #[test]
    fn test_entry_with_large_id() {
        let entry = ArchiveEntry::new("file.txt".into(), usize::MAX);
        assert_eq!(entry.id, usize::MAX);
    }

    #[test]
    fn test_entry_with_large_sizes() {
        let mut entry = ArchiveEntry::new("huge.bin".into(), 0);
        entry.size = Some(u64::MAX);
        entry.compressed_size = Some(u64::MAX);
        entry.compute_compression_ratio();
        // u64::MAX / u64::MAX = 1.0
        let ratio = entry.compression_ratio.unwrap();
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }
}
