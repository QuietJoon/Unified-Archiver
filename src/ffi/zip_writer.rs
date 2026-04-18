//! Native Rust ZIP creation backend using the `zip` crate
//!
//! This backend provides ZIP archive creation with proper central directory
//! structure, fixing libarchive's known issues with ZIP format.

use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::options::{CompressionOptions, ProgressCallback};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter as RawZipWriter};

/// Native Rust ZIP archive writer
///
/// Provides reliable ZIP creation using the zip crate, avoiding
/// libarchive's known issues with central directory structure.
pub struct ZipWriter {
    writer: Option<RawZipWriter<File>>,
    path: PathBuf,
    options: SimpleFileOptions,
    progress: Option<Box<dyn ProgressCallback>>,
    bytes_written: u64,
    entries_written: usize,
}

impl ZipWriter {
    /// Create a new ZIP archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        compression_options: &mut CompressionOptions,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        // This library does not produce encrypted archives (AD 0007 amended by AD 0027).
        // Reject passwords loudly instead of silently writing a plaintext archive.
        if compression_options.password.is_some() {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::CREATE,
                "Encrypted ZIP creation is not supported by this library. \
                 Use open_encrypted() to read existing encrypted archives.",
            ));
        }

        let file = File::create(&path_buf)
            .map_err(|e| ArchiveError::io(crate::error::ops::CREATE, path_buf.clone(), e))?;

        let writer = RawZipWriter::new(file);

        // Map compression level to zip crate's compression method and deflate level
        let (compression, deflate_level) = match compression_options.level {
            crate::options::CompressionLevel::Store => (CompressionMethod::Stored, None),
            crate::options::CompressionLevel::Fastest => (CompressionMethod::Deflated, Some(1)),
            crate::options::CompressionLevel::Fast => (CompressionMethod::Deflated, Some(3)),
            crate::options::CompressionLevel::Normal => (CompressionMethod::Deflated, Some(6)),
            crate::options::CompressionLevel::Maximum => (CompressionMethod::Deflated, Some(8)),
            crate::options::CompressionLevel::Ultra => (CompressionMethod::Deflated, Some(9)),
        };

        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .compression_level(deflate_level.map(|l| l as i64));

        Ok(Self {
            writer: Some(writer),
            path: path_buf,
            options,
            progress: compression_options.progress.take(),
            bytes_written: 0,
            entries_written: 0,
        })
    }

    fn notify_progress(&mut self, additional: u64) -> Result<()> {
        super::common::notify_creation_progress(
            &mut self.progress,
            &mut self.bytes_written,
            additional,
            Some(ArchiveFormat::Zip),
        )
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of entries written so far
    pub fn entries_written(&self) -> usize {
        self.entries_written
    }

    /// Add a file from byte data
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, self.options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        writer
            .write_all(data)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

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
        let mut file_options = self.options;

        // Restore modification time
        if let Some(mtime) = metadata.modified {
            if let Some(dt) = super::common::system_time_to_zip_datetime(mtime) {
                file_options = file_options.last_modified_time(dt);
            }
        }

        // Restore Unix permissions
        #[cfg(unix)]
        if let Some(perm) = metadata.permissions {
            file_options = file_options.unix_permissions(perm);
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, file_options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        writer
            .write_all(data)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

        self.entries_written += 1;
        self.notify_progress(data.len() as u64)
    }

    /// Stream a file's contents from a reader without preserving source metadata.
    ///
    /// Used by `commit_changes()` on the non-preserving path to round-trip
    /// retained entries without buffering them fully in memory. Applies only
    /// the writer's default `FileOptions`.
    pub fn add_file_from_reader<R: Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
    ) -> Result<()> {
        let file_options = self.options;

        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, file_options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        let copied = std::io::copy(reader, writer)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

        self.entries_written += 1;
        self.notify_progress(copied)
    }

    /// Stream a file's contents from a reader, preserving metadata. An explicit
    /// `compression_override` pins the central-directory compression method so
    /// mixed Stored/Deflated archives round-trip unchanged.
    pub fn add_file_from_reader_with_metadata<R: Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
        metadata: &crate::entry::ArchiveEntry,
        compression_override: Option<CompressionMethod>,
    ) -> Result<()> {
        let mut file_options = self.options;

        if let Some(mtime) = metadata.modified {
            if let Some(dt) = super::common::system_time_to_zip_datetime(mtime) {
                file_options = file_options.last_modified_time(dt);
            }
        }

        #[cfg(unix)]
        if let Some(perm) = metadata.permissions {
            file_options = file_options.unix_permissions(perm);
        }

        if let Some(method) = compression_override {
            file_options = file_options.compression_method(method);
            if method == CompressionMethod::Stored {
                file_options = file_options.compression_level(None);
            }
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, file_options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        let copied = std::io::copy(reader, writer)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

        self.entries_written += 1;
        self.notify_progress(copied)
    }

    /// Set the archive-level (EOCD) comment. Pass an empty slice to clear.
    pub fn set_archive_comment(&mut self, comment: &[u8]) -> Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;
        writer.set_raw_comment(comment.to_vec().into_boxed_slice());
        Ok(())
    }

    /// Add a file from filesystem path with custom archive path
    ///
    /// Preserves the source file's modification time and Unix permissions.
    pub fn add_file_from_path(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        let fs_path = fs_path.as_ref();

        let mut file =
            File::open(fs_path).map_err(|e| ArchiveError::io("open", fs_path.to_path_buf(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("metadata", fs_path.to_path_buf(), e))?;

        let mut file_options = self.options;

        // Preserve modification time
        if let Ok(mtime) = metadata.modified() {
            if let Some(dt) = super::common::system_time_to_zip_datetime(mtime) {
                file_options = file_options.last_modified_time(dt);
            }
        }

        // Preserve Unix permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file_options = file_options.unix_permissions(metadata.permissions().mode());
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, file_options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        let copied = std::io::copy(&mut file, writer)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

        self.entries_written += 1;
        self.notify_progress(copied)
    }

    /// Add a single directory entry (without contents)
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        let dir_path = super::common::ensure_trailing_slash(archive_path);

        writer.add_directory(&dir_path, self.options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Add directory: {}", e))
        })?;

        self.entries_written += 1;
        // Emit a zero-byte progress event so per-entry semantics match file additions.
        self.notify_progress(0)
    }

    /// Add a directory recursively
    pub fn add_directory_recursive(&mut self, dir_path: impl AsRef<Path>) -> Result<()> {
        let dir_path = dir_path.as_ref();

        if !dir_path.is_dir() {
            return Err(ArchiveError::invalid_path(
                dir_path.to_string_lossy().as_ref(),
                "Not a directory",
            ));
        }

        // Walk directory tree
        for entry in walkdir::WalkDir::new(dir_path).follow_links(false) {
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
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(dir_path).map_err(|_| {
                ArchiveError::invalid_path(
                    entry_path.to_string_lossy().as_ref(),
                    "Invalid relative path",
                )
            })?;

            // Skip the root directory itself
            if relative_path.as_os_str().is_empty() {
                continue;
            }

            let archive_path = super::common::normalize_path(&relative_path.to_string_lossy());

            if entry.file_type().is_file() {
                self.add_file_from_path(entry_path, &archive_path)?;
            } else if entry.file_type().is_dir() {
                self.add_directory_entry(&archive_path)?;
            } else if entry.file_type().is_symlink() {
                // AD 0021: symlinks are rejected at creation time to avoid silent data loss
                // and to keep creation-side link policy aligned with extraction rejection.
                return Err(ArchiveError::operation_blocked(
                    "add_directory_recursive",
                    format!(
                        "Refusing to archive symlink '{}': symlinks are not supported for ZIP creation",
                        entry_path.display()
                    ),
                ));
            } else {
                return Err(ArchiveError::operation_blocked(
                    "add_directory_recursive",
                    format!(
                        "Refusing to archive '{}': unsupported file type",
                        entry_path.display()
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Finish writing and close the archive
    pub fn finish(&mut self) -> Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.finish().map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Finish: {}", e))
            })?;
        }
        Ok(())
    }
}

impl Drop for ZipWriter {
    fn drop(&mut self) {
        // Try to finish properly on drop
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{CompressionLevel, CompressionOptions};

    #[test]
    fn test_zip_writer_create_and_finish() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_create.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer
            .add_file_from_data("hello.txt", b"Hello, world!")
            .unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_zip_writer_multiple_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_multi.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer
            .add_file_from_data("file1.txt", b"content 1")
            .unwrap();
        writer
            .add_file_from_data("file2.txt", b"content 2")
            .unwrap();
        writer
            .add_file_from_data("subdir/file3.txt", b"content 3")
            .unwrap();
        writer.finish().unwrap();

        // Verify by opening with zip crate
        let file = File::open(&path).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        assert_eq!(zip.len(), 3);
    }

    #[test]
    fn test_zip_writer_store_compression() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_store.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        opts.level = CompressionLevel::Store;
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer
            .add_file_from_data("stored.txt", b"stored content")
            .unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_zip_writer_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_empty_file.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("empty.txt", b"").unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_zip_writer_add_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_dir.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("mydir/", b"").unwrap(); // Directory entry
        writer
            .add_file_from_data("mydir/file.txt", b"content")
            .unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_zip_writer_drop_finishes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_drop.zip");

        {
            let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
            let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
            writer
                .add_file_from_data("auto.txt", b"auto finish")
                .unwrap();
            // Drop should call finish()
        }

        assert!(path.exists());
    }

    #[test]
    fn test_zip_writer_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_roundtrip.zip");

        let test_content = b"Hello from roundtrip test!";

        // Write
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer
            .add_file_from_data("roundtrip.txt", test_content)
            .unwrap();
        writer.finish().unwrap();

        // Read back with zip crate
        let file = File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name("roundtrip.txt").unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();

        assert_eq!(buf, test_content);
    }
}
