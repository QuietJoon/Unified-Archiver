//! Archive creation operations
//!
//! This module provides methods for creating new archives and adding files to them.

use crate::archive::{Archive, ArchiveBackend, ArchiveMode};
use crate::error::{ArchiveError, Result};
use crate::ffi::libarchive_wrapper::LibarchiveArchive;
use crate::ffi::zip_writer::ZipWriter;
use crate::format::ArchiveFormat;
use once_cell::sync::OnceCell;
use std::path::Path;

impl Archive {
    /// Create a new archive (stub - not yet implemented)
    ///
    /// # Arguments
    /// * `path` - Output archive path
    /// * `options` - Compression settings
    ///
    /// # Returns
    /// * `Ok(Archive)` - Archive handle in Write mode
    /// * `Err` - If creation fails
    pub fn create(
        path: impl AsRef<Path>,
        options: crate::options::CompressionOptions,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let format = options.format;

        // Route to appropriate backend based on format
        let backend = match format {
            ArchiveFormat::Zip => {
                // Use native Rust zip crate for ZIP creation
                let writer = ZipWriter::create(&path_buf, &options)?;
                ArchiveBackend::ZipWriter(writer)
            }
            _ => {
                // Use libarchive for other formats
                let libarchive = LibarchiveArchive::create(&path_buf, format, &options)?;
                ArchiveBackend::Libarchive(libarchive)
            }
        };

        Ok(Self {
            backend,
            path: path_buf,
            mode: ArchiveMode::Write,
            format,
            entry_cache: OnceCell::new(),
            modifications: None,
        })
    }

    /// Add a file to archive from byte data
    ///
    /// # Arguments
    /// * `path` - Path within archive
    /// * `data` - File content bytes
    ///
    /// # Returns
    /// * `Ok(())` - File added successfully
    /// * `Err` - If archive not in Write mode or add fails
    pub fn add_file_from_data(&mut self, path: &str, data: &[u8]) -> Result<()> {
        match &mut self.backend {
            ArchiveBackend::ZipWriter(writer) => writer.add_file_from_data(path, data),
            ArchiveBackend::Libarchive(backend) => backend.add_file_from_data(path, data),
            ArchiveBackend::Unrar(_) | ArchiveBackend::Piz(_) | ArchiveBackend::SevenZ(_) | ArchiveBackend::ZipReader(_) => {
                Err(ArchiveError::read_only_backend("add_file_from_data"))
            }
        }
    }

    /// Add a file to archive from filesystem path
    ///
    /// The file will be stored with its original filename
    pub fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let fs_path = path.as_ref();
        let archive_path = fs_path
            .file_name()
            .ok_or_else(|| {
                ArchiveError::invalid_path(fs_path.to_string_lossy().as_ref(), "No filename")
            })?
            .to_string_lossy();
        self.add_file_from_path_as(fs_path, &archive_path)
    }

    /// Add a file to archive from filesystem path with custom archive path
    ///
    /// # Arguments
    /// * `fs_path` - Path on filesystem to read from
    /// * `archive_path` - Path to store as within archive
    pub fn add_file_from_path_as(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        match &mut self.backend {
            ArchiveBackend::ZipWriter(writer) => writer.add_file_from_path(fs_path, archive_path),
            ArchiveBackend::Libarchive(backend) => {
                backend.add_file_from_path(fs_path, archive_path)
            }
            ArchiveBackend::Unrar(_) | ArchiveBackend::Piz(_) | ArchiveBackend::SevenZ(_) | ArchiveBackend::ZipReader(_) => {
                Err(ArchiveError::read_only_backend("add_file_from_path_as"))
            }
        }
    }

    /// Add a directory entry to archive (without contents)
    pub fn add_directory(&mut self, path: &str) -> Result<()> {
        match &mut self.backend {
            ArchiveBackend::ZipWriter(writer) => writer.add_directory_entry(path),
            ArchiveBackend::Libarchive(backend) => backend.add_directory_entry(path),
            ArchiveBackend::Unrar(_) | ArchiveBackend::Piz(_) | ArchiveBackend::SevenZ(_) | ArchiveBackend::ZipReader(_) => {
                Err(ArchiveError::read_only_backend("add_directory"))
            }
        }
    }

    /// Add a directory recursively to archive
    ///
    /// All files within the directory tree are added to the archive
    pub fn add_directory_recursive(&mut self, path: impl AsRef<Path>) -> Result<()> {
        match &mut self.backend {
            ArchiveBackend::ZipWriter(writer) => writer.add_directory_recursive(path),
            ArchiveBackend::Libarchive(backend) => backend.add_directory_recursive(path),
            ArchiveBackend::Unrar(_) | ArchiveBackend::Piz(_) | ArchiveBackend::SevenZ(_) | ArchiveBackend::ZipReader(_) => {
                Err(ArchiveError::read_only_backend("add_directory_recursive"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ArchiveFormat;
    use crate::options::CompressionOptions;

    // ── Archive::create tests ──

    #[test]
    fn test_create_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
        assert_eq!(archive.mode, ArchiveMode::Write);
        assert!(archive.modifications.is_none());
    }

    #[test]
    fn test_create_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar");
        let options = CompressionOptions::new(ArchiveFormat::Tar);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Tar);
        assert_eq!(archive.mode, ArchiveMode::Write);
    }

    #[test]
    fn test_create_tar_gz() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.gz");
        let options = CompressionOptions::new(ArchiveFormat::TarGzip);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarGzip);
    }

    #[test]
    fn test_create_tar_bz2() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.bz2");
        let options = CompressionOptions::new(ArchiveFormat::TarBzip2);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
    }

    #[test]
    fn test_create_tar_xz() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.xz");
        let options = CompressionOptions::new(ArchiveFormat::TarXz);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarXz);
    }

    // ── add_file_from_data tests ──

    #[test]
    fn test_add_file_from_data_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_add.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        assert!(archive
            .add_file_from_data("hello.txt", b"Hello, World!")
            .is_ok());
        archive.finish().unwrap();
    }

    #[test]
    fn test_add_file_from_data_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_add.tar");
        let options = CompressionOptions::new(ArchiveFormat::Tar);
        let mut archive = Archive::create(&path, options).unwrap();
        assert!(archive
            .add_file_from_data("hello.txt", b"Hello, World!")
            .is_ok());
        archive.finish().unwrap();
    }

    #[test]
    fn test_add_file_from_data_empty_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_empty.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        assert!(archive.add_file_from_data("empty.txt", b"").is_ok());
        archive.finish().unwrap();
    }

    #[test]
    fn test_add_multiple_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_multi.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        archive
            .add_file_from_data("file1.txt", b"Content 1")
            .unwrap();
        archive
            .add_file_from_data("file2.txt", b"Content 2")
            .unwrap();
        archive
            .add_file_from_data("subdir/file3.txt", b"Content 3")
            .unwrap();
        archive.finish().unwrap();

        // Verify by re-opening
        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 3);
    }

    // ── add_file_from_path tests ──

    #[test]
    fn test_add_file_from_path() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        std::fs::write(&source, b"Source content").unwrap();

        let archive_path = temp.path().join("test_from_path.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        assert!(archive.add_file_from_path(&source).is_ok());
        archive.finish().unwrap();
    }

    #[test]
    fn test_add_file_from_path_as() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        std::fs::write(&source, b"Custom path content").unwrap();

        let archive_path = temp.path().join("test_custom_path.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive
            .add_file_from_path_as(&source, "custom/path.txt")
            .unwrap();
        archive.finish().unwrap();

        // Verify the custom path
        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "custom/path.txt");
    }

    // ── add_directory tests ──

    #[test]
    fn test_add_directory_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_dir.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_directory("mydir").unwrap();
        archive
            .add_file_from_data("mydir/file.txt", b"content")
            .unwrap();
        archive.finish().unwrap();

        // Verify by re-opening
        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        // Should have directory entry + file
        assert!(
            entries.len() >= 2,
            "Expected at least 2 entries (dir + file), got {}",
            entries.len()
        );
        assert!(entries.iter().any(|e| e.path.starts_with("mydir")));
    }

    #[test]
    fn test_add_directory_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_dir.tar");
        let options = CompressionOptions::new(ArchiveFormat::Tar);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_directory("mydir").unwrap();
        archive
            .add_file_from_data("mydir/file.txt", b"content")
            .unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert!(
            entries.len() >= 2,
            "Expected at least 2 entries (dir + file), got {}",
            entries.len()
        );
    }

    // ── add_directory_recursive tests ──

    #[test]
    fn test_add_directory_recursive() {
        let temp = tempfile::tempdir().unwrap();

        // Create source directory structure
        let src_dir = temp.path().join("src_dir");
        std::fs::create_dir_all(src_dir.join("subdir")).unwrap();
        std::fs::write(src_dir.join("root.txt"), b"Root file").unwrap();
        std::fs::write(src_dir.join("subdir/nested.txt"), b"Nested file").unwrap();

        let archive_path = temp.path().join("test_recursive.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_directory_recursive(&src_dir).unwrap();
        archive.finish().unwrap();

        // Verify contents
        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        assert!(
            entries.len() >= 2,
            "Expected at least 2 entries, got {}",
            entries.len()
        );
    }

    // ── finish tests ──

    #[test]
    fn test_finish_write_mode() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_finish.zip");
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        archive
            .add_file_from_data("test.txt", b"test data")
            .unwrap();
        assert!(archive.finish().is_ok());
        assert!(path.exists());
    }

    // ── Roundtrip test ──

    #[test]
    fn test_create_and_read_roundtrip_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("roundtrip.zip");
        let content = b"Hello, roundtrip!";

        // Create
        let options = CompressionOptions::new(ArchiveFormat::Zip);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("greeting.txt", content).unwrap();
        archive.finish().unwrap();

        // Read back
        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "greeting.txt");

        // Extract to memory and verify content
        let data = reader.extract_to_memory("greeting.txt").unwrap();
        assert_eq!(data, content);
    }

    #[test]
    fn test_create_and_read_roundtrip_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("roundtrip.tar");
        let content = b"TAR roundtrip content";

        let options = CompressionOptions::new(ArchiveFormat::Tar);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("data.txt", content).unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 1);

        let data = reader.extract_to_memory("data.txt").unwrap();
        assert_eq!(data, content);
    }
}
