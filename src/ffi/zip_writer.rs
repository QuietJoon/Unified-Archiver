//! Native Rust ZIP creation backend using the `zip` crate
//!
//! This backend provides ZIP archive creation with proper central directory
//! structure, fixing libarchive's known issues with ZIP format.

use crate::error::{ArchiveError, Result};
use crate::format::ArchiveFormat;
use crate::options::CompressionOptions;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::{SimpleFileOptions, ZipWriter as RawZipWriter};
use zip::CompressionMethod;

/// Native Rust ZIP archive writer
///
/// Provides reliable ZIP creation using the zip crate, avoiding
/// libarchive's known issues with central directory structure.
pub struct ZipWriter {
    writer: Option<RawZipWriter<File>>,
    path: PathBuf,
    options: SimpleFileOptions,
}

impl ZipWriter {
    /// Create a new ZIP archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        compression_options: &CompressionOptions,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        let file =
            File::create(&path_buf).map_err(|e| ArchiveError::io("create", path_buf.clone(), e))?;

        let writer = RawZipWriter::new(file);

        // Map compression level to zip crate's compression method
        let compression = match compression_options.level {
            crate::options::CompressionLevel::Store => CompressionMethod::Stored,
            crate::options::CompressionLevel::Fastest => CompressionMethod::Deflated,
            crate::options::CompressionLevel::Fast => CompressionMethod::Deflated,
            crate::options::CompressionLevel::Normal => CompressionMethod::Deflated,
            crate::options::CompressionLevel::Maximum => CompressionMethod::Deflated,
            crate::options::CompressionLevel::Ultra => CompressionMethod::Deflated,
        };

        let options = SimpleFileOptions::default().compression_method(compression);

        Ok(Self {
            writer: Some(writer),
            path: path_buf,
            options,
        })
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
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

        Ok(())
    }

    /// Add a file from filesystem path with custom archive path
    pub fn add_file_from_path(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        let fs_path = fs_path.as_ref();

        let mut file =
            File::open(fs_path).map_err(|e| ArchiveError::io("open", fs_path.to_path_buf(), e))?;

        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;

        writer.start_file(archive_path, self.options).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
        })?;

        std::io::copy(&mut file, writer)
            .map_err(|e| ArchiveError::io("write", self.path.clone(), e))?;

        Ok(())
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

            // ZIP format requires forward slashes for cross-platform compatibility
            let archive_path = relative_path
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");

            if entry.file_type().is_file() {
                // Add file
                self.add_file_from_path(entry_path, &archive_path)?;
            } else if entry.file_type().is_dir() {
                // Add directory entry (ZIP requires trailing slash for directories)
                let writer = self.writer.as_mut().ok_or_else(|| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
                })?;

                let dir_path = format!("{}/", archive_path);
                writer.add_directory(&dir_path, self.options).map_err(|e| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), format!("Add directory: {}", e))
                })?;
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

    fn temp_zip_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("unified_archive_zip_writer_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn test_zip_writer_create_and_finish() {
        let path = temp_zip_path("test_create.zip");
        let _ = std::fs::remove_file(&path);

        let opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
        writer
            .add_file_from_data("hello.txt", b"Hello, world!")
            .unwrap();
        writer.finish().unwrap();

        // Verify file was created
        assert!(path.exists());
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_multiple_files() {
        let path = temp_zip_path("test_multi.zip");
        let _ = std::fs::remove_file(&path);

        let opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
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

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_store_compression() {
        let path = temp_zip_path("test_store.zip");
        let _ = std::fs::remove_file(&path);

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        opts.level = CompressionLevel::Store;
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
        writer
            .add_file_from_data("stored.txt", b"stored content")
            .unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_empty_file() {
        let path = temp_zip_path("test_empty_file.zip");
        let _ = std::fs::remove_file(&path);

        let opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
        writer.add_file_from_data("empty.txt", b"").unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_add_directory() {
        let path = temp_zip_path("test_dir.zip");
        let _ = std::fs::remove_file(&path);

        let opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
        writer.add_file_from_data("mydir/", b"").unwrap(); // Directory entry
        writer
            .add_file_from_data("mydir/file.txt", b"content")
            .unwrap();
        writer.finish().unwrap();

        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_drop_finishes() {
        let path = temp_zip_path("test_drop.zip");
        let _ = std::fs::remove_file(&path);

        {
            let opts = CompressionOptions::new(ArchiveFormat::Zip);
            let mut writer = ZipWriter::create(&path, &opts).unwrap();
            writer
                .add_file_from_data("auto.txt", b"auto finish")
                .unwrap();
            // Drop should call finish()
        }

        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_zip_writer_roundtrip() {
        let path = temp_zip_path("test_roundtrip.zip");
        let _ = std::fs::remove_file(&path);

        let test_content = b"Hello from roundtrip test!";

        // Write
        let opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &opts).unwrap();
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
        std::fs::remove_file(&path).ok();
    }
}
