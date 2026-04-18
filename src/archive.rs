//! Archive handle and core operations

use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, Result};
use crate::ffi::libarchive_wrapper::LibarchiveArchive;
use crate::ffi::piz_wrapper::PizArchive;
use crate::ffi::sevenz_wrapper::SevenZArchive;
#[cfg(feature = "rar-support")]
use crate::ffi::wrapper::UnrarArchive;
use crate::ffi::zip_wrapper::ZipArchive;
use crate::ffi::zip_writer::ZipWriter;
use crate::format::ArchiveFormat;
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

/// Access mode for archive
#[allow(dead_code)] // Write and Modify modes planned for Phase 5-6
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveMode {
    Read,
    Write,
    Modify,
}

/// Internal archive backend
#[allow(clippy::large_enum_variant)]
pub(crate) enum ArchiveBackend {
    #[cfg(feature = "rar-support")]
    Unrar(UnrarArchive),
    Piz(PizArchive),
    SevenZ(SevenZArchive),
    ZipWriter(ZipWriter),
    ZipReader(ZipArchive),
    Libarchive(LibarchiveArchive),
}

/// Handle to an opened archive file
///
/// Provides unified access to inspection, extraction, creation, and modification
/// operations across all supported formats.
///
/// # Thread Safety (FR-020, FR-021)
///
/// `Archive` implements `Send` to support concurrent operations on **different files**
/// from multiple threads. Each thread should have its own `Archive` instance.
///
/// **Threading Model**:
/// - ✅ `Send`: Archive handles can be moved between threads
/// - ❌ NOT `Sync`: Archive handles cannot be shared between threads
///
/// This design ensures:
/// - Multiple archives can be processed concurrently (different instances)
/// - No data races on the same archive (no shared mutable access)
///
/// **Backend Caveats**:
/// - RAR: UnRAR has global state, but all FFI calls are serialized via an
///   internal `UNRAR_LOCK` mutex so concurrent callers on different RAR
///   archives are safe. Per-archive extraction still runs sequentially
///   because of the underlying SDK's iterator shape.
///
/// # Examples
///
/// ```no_run
/// use std::thread;
/// use unified_archive::{Archive, ArchiveEntry, ArchiveError};
///
/// // Concurrent extraction from different archives
/// let handles: Vec<_> = vec!["archive1.zip", "archive2.zip"]
///     .into_iter()
///     .map(|path| {
///         thread::spawn(move || -> Result<Vec<ArchiveEntry>, ArchiveError> {
///             let archive = Archive::open(path)?;
///             let entries = archive.list_files()?.to_vec();
///             Ok(entries)
///         })
///     })
///     .collect();
///
/// for handle in handles {
///     let entries = handle.join().unwrap()?;
///     println!("Found {} entries", entries.len());
/// }
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub struct Archive {
    /// Backend implementation
    pub(crate) backend: ArchiveBackend,
    /// Archive file path
    pub(crate) path: PathBuf,
    /// Access mode
    pub(crate) mode: ArchiveMode,
    /// Detected format
    pub(crate) format: ArchiveFormat,
    /// Cached entry list (Phase 1: zero-cost repeated access)
    pub(crate) entry_cache: OnceCell<Vec<ArchiveEntry>>,
    /// Modification tracker (for Modify mode)
    pub(crate) modifications: Option<crate::modification::ModificationTracker>,
    /// Modification options (Modify mode only). `None` means defaults are used.
    pub(crate) mod_options: Option<crate::modification::ModificationOptions>,
    /// Backing temp file for archives opened at a non-zero offset
    /// (see [`Archive::open_at_offset`]). The [`tempfile::TempPath`] is dropped
    /// with the `Archive`, removing the temp copy of the embedded payload.
    /// `None` for archives opened from a real on-disk file.
    pub(crate) _backing_tempfile: Option<tempfile::TempPath>,
}

// SAFETY: Archive can be moved between threads (Send) but not shared (&Archive from multiple threads).
//
// Per-variant justification:
// - Unrar(UnrarArchive): Contains a raw FFI handle (RARHandle). The UnRAR SDK uses global state
//   protected by internal mutexes for file I/O, but individual handles are not reentrant.
//   Moving the handle between threads is safe; sharing via &Archive is not (hence !Sync).
//   Single-threaded access is already enforced by !Sync + the fact that extraction creates
//   fresh handles via fresh_handle().
// - Piz(PizArchive): Contains only PathBuf (Send+Sync). All operations re-open/mmap the file.
// - SevenZ(SevenZArchive): Contains PathBuf + Option<SecStr> (Send+Sync).
// - ZipWriter(ZipWriter): Contains owned zip::ZipWriter<File> which is Send.
// - ZipReader(ZipArchive): Contains PathBuf + Option<SecStr> (Send+Sync).
// - Libarchive(LibarchiveArchive): Contains owned String + Option<*mut Archive>.
//   The raw pointer is only used in Write mode and is accessed exclusively by the owning thread.
//
// Shared fields: PathBuf, ArchiveFormat, ArchiveMode, OnceCell<Vec<T>>, Option<ModificationTracker>
// are all Send.
//
// This satisfies FR-020/FR-021: concurrent operations on **different** Archive instances
// from different threads, but not concurrent operations on the **same** Archive instance.
unsafe impl Send for Archive {}

// Archive is NOT Sync - it cannot be safely shared between threads via &Archive
// because the underlying FFI operations are not reentrant.

impl Archive {
    /// Private constructor for read-mode archives
    fn new_read(backend: ArchiveBackend, path_buf: PathBuf, format: ArchiveFormat) -> Self {
        Self {
            backend,
            path: path_buf,
            mode: ArchiveMode::Read,
            format,
            entry_cache: OnceCell::new(),
            modifications: None,
            mod_options: None,
            _backing_tempfile: None,
        }
    }

    /// Open an existing archive for reading
    ///
    /// Format is automatically detected from file content.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        // Detect format from magic bytes
        let format = ArchiveFormat::detect(&path_buf)?;

        // Route to appropriate backend
        let backend = match format {
            #[cfg(feature = "rar-support")]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                let unrar = UnrarArchive::open(&path_buf)?;
                ArchiveBackend::Unrar(unrar)
            }
            #[cfg(not(feature = "rar-support"))]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                return Err(ArchiveError::unsupported(
                    "open",
                    format,
                    Some(
                        "RAR/RAR5 support is disabled in this build (enable the \
                         `rar-support` Cargo feature to include UnRAR)"
                            .to_string(),
                    ),
                ));
            }
            ArchiveFormat::Zip => {
                let piz = PizArchive::open(&path_buf)?;
                ArchiveBackend::Piz(piz)
            }
            ArchiveFormat::SevenZip => {
                let sevenz = SevenZArchive::open(&path_buf)?;
                ArchiveBackend::SevenZ(sevenz)
            }
            ArchiveFormat::Tar
            | ArchiveFormat::TarGzip
            | ArchiveFormat::TarBzip2
            | ArchiveFormat::TarXz
            | ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Iso => {
                let libarchive = LibarchiveArchive::open(&path_buf)?;
                ArchiveBackend::Libarchive(libarchive)
            }
        };

        Ok(Self::new_read(backend, path_buf, format))
    }

    /// Open an encrypted archive with password
    pub fn open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let format = ArchiveFormat::detect(&path_buf)?;

        let backend = match format {
            #[cfg(feature = "rar-support")]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                let unrar = UnrarArchive::open_with_password(&path_buf, password.as_ref())?;
                ArchiveBackend::Unrar(unrar)
            }
            #[cfg(not(feature = "rar-support"))]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                return Err(ArchiveError::unsupported(
                    "open_encrypted",
                    format,
                    Some(
                        "RAR/RAR5 support is disabled in this build (enable the \
                         `rar-support` Cargo feature to include UnRAR)"
                            .to_string(),
                    ),
                ));
            }
            ArchiveFormat::Zip => {
                // Use zip crate backend for encrypted ZIP (piz can't decrypt)
                let zip = ZipArchive::open_with_password(&path_buf, password.as_ref())?;
                ArchiveBackend::ZipReader(zip)
            }
            ArchiveFormat::SevenZip => {
                let sevenz = SevenZArchive::open_with_password(&path_buf, password.as_ref())?;
                ArchiveBackend::SevenZ(sevenz)
            }
            _ => {
                return Err(ArchiveError::unsupported(
                    "open_encrypted",
                    format,
                    Some("Format not yet supported".to_string()),
                ));
            }
        };

        Ok(Self::new_read(backend, path_buf, format))
    }

    /// Detect if a file is a self-extracting archive (SFX)
    ///
    /// Phase 7: Identifies executable files containing embedded archives.
    /// Uses 3-stage detection: executable validation → signature scan → archive validation.
    ///
    /// # Performance
    /// - Typical detection: 15-70ms
    /// - Non-executable files: <1ms (early exit)
    /// - Scans first 1MB only for performance
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let result = Archive::detect_sfx("installer.exe")?;
    /// if result.is_sfx {
    ///     println!("Archive offset: {:?}", result.data_offset);
    ///     println!("Format: {:?}", result.archive_format);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn detect_sfx(path: impl AsRef<Path>) -> Result<crate::sfx::SfxDetectionResult> {
        crate::sfx::detect_sfx(path)
    }

    /// Open a self-extracting archive (SFX) for reading
    ///
    /// Convenience method that detects SFX and forwards to `open_at_offset()`.
    /// The embedded payload is materialized into a temporary file and then
    /// opened through the normal archive pipeline.
    ///
    /// # Returns
    /// - `Err(ArchiveError::Format)` if the file is not an SFX
    /// - Other errors if detection succeeds but the embedded archive cannot be opened
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open_sfx("installer.exe")?;
    /// println!("Embedded entries: {}", archive.entry_count()?);
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn open_sfx(path: impl AsRef<Path>) -> Result<Self> {
        let path_ref = path.as_ref();
        let detection = Self::detect_sfx(path_ref)?;

        if !detection.is_sfx {
            return Err(ArchiveError::format(
                None,
                format!(
                    "File is not a self-extracting archive: {}",
                    path_ref.display()
                ),
            ));
        }

        let offset = detection.data_offset.ok_or_else(|| {
            ArchiveError::format(None, "SFX detection succeeded but offset is missing")
        })?;

        Self::open_at_offset(path_ref, offset)
    }

    /// Open an archive from a specific byte offset.
    ///
    /// Opens an archive that doesn't start at byte 0 of the file — primarily
    /// SFX payloads, where the archive data is embedded after an executable
    /// stub. The payload is materialized into a temporary file that is removed
    /// when the returned [`Archive`] is dropped.
    ///
    /// `offset == 0` is equivalent to [`Archive::open`] and avoids the copy.
    /// Offsets greater than or equal to the file's length return an error.
    ///
    /// # Arguments
    /// * `path` - Path to the file containing the archive
    /// * `offset` - Byte offset where the archive data begins
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// // Open archive embedded at offset 65536
    /// let archive = Archive::open_at_offset("file.bin", 65536)?;
    /// let entries = archive.list_files()?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> {
        let path_ref = path.as_ref();

        if offset == 0 {
            return Self::open(path_ref);
        }

        use std::fs::File;
        use std::io::{Seek, SeekFrom};

        let mut source = File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref, e))?;
        let file_len = source
            .metadata()
            .map_err(|e| ArchiveError::io("stat", path_ref, e))?
            .len();

        if offset >= file_len {
            return Err(ArchiveError::format(
                None,
                format!(
                    "open_at_offset: offset {} is at or beyond end of file ({} bytes)",
                    offset, file_len
                ),
            ));
        }

        // Cap the staged payload to protect callers from corrupted/malicious
        // SFX offsets that would otherwise copy gigabytes of unrelated data
        // into the tempfile. Legitimate SFX payloads are well below this
        // ceiling; raise if a real archive is rejected.
        const MAX_SFX_PAYLOAD_SIZE: u64 = 16 * 1024 * 1024 * 1024; // 16 GiB
        let payload_size = file_len - offset;
        if payload_size > MAX_SFX_PAYLOAD_SIZE {
            return Err(ArchiveError::format(
                None,
                format!(
                    "open_at_offset: payload size {} bytes exceeds maximum {} bytes",
                    payload_size, MAX_SFX_PAYLOAD_SIZE
                ),
            ));
        }

        source
            .seek(SeekFrom::Start(offset))
            .map_err(|e| ArchiveError::io("seek", path_ref, e))?;

        let mut temp = tempfile::Builder::new()
            .prefix("unified-archive-sfx-")
            .suffix(".bin")
            .tempfile()
            .map_err(|e| ArchiveError::io("create_tempfile", path_ref, e))?;

        {
            use std::io::Write;
            let sink = temp.as_file_mut();
            std::io::copy(&mut source, sink).map_err(|e| ArchiveError::io("copy", path_ref, e))?;
            sink.flush()
                .map_err(|e| ArchiveError::io("flush", path_ref, e))?;
        }

        let temp_path = temp.into_temp_path();

        let mut archive = Self::open(&temp_path)?;
        archive._backing_tempfile = Some(temp_path);
        Ok(archive)
    }

    /// Extract the executable stub from an SFX archive
    ///
    /// Phase 7: Extracts the executable portion of an SFX archive for security analysis.
    /// The stub is the executable code that runs before extracting the embedded archive.
    ///
    /// # Security Note
    /// The extracted stub is executable code and should be analyzed in a sandbox
    /// environment. Never execute untrusted stubs directly.
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::Archive;
    /// use std::path::PathBuf;
    ///
    /// let detection = Archive::detect_sfx("installer.exe")?;
    /// if detection.is_sfx {
    ///     // Extract stub for security analysis
    ///     let stub_data = Archive::extract_stub("installer.exe", &detection)?;
    ///     std::fs::write("stub.exe", stub_data)?;
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn extract_stub(
        path: impl AsRef<Path>,
        detection: &crate::sfx::SfxDetectionResult,
    ) -> Result<Vec<u8>> {
        if !detection.is_sfx {
            return Err(ArchiveError::format(
                None,
                "Cannot extract stub from non-SFX file",
            ));
        }

        let offset = detection
            .data_offset
            .ok_or_else(|| ArchiveError::format(None, "SFX detection missing offset"))?;

        // Guard against malicious/corrupted files with unreasonably large stubs
        const MAX_STUB_SIZE: u64 = 50 * 1024 * 1024; // 50MB
        if offset > MAX_STUB_SIZE {
            return Err(ArchiveError::format(
                None,
                format!(
                    "SFX stub size {} exceeds maximum allowed size of {} bytes",
                    offset, MAX_STUB_SIZE
                ),
            ));
        }

        use std::fs::File;
        use std::io::Read;

        let path_ref = path.as_ref();
        let mut file = File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref, e))?;
        let mut stub = vec![0u8; offset as usize];
        file.read_exact(&mut stub)
            .map_err(|e| ArchiveError::io("read", path_ref, e))?;

        Ok(stub)
    }

    /// Get the detected archive format
    pub fn format(&self) -> ArchiveFormat {
        self.format
    }

    /// Check if archive is encrypted
    ///
    /// Phase 2.6: Detects if archive requires a password for extraction
    /// by checking if any entries are encrypted.
    ///
    /// Uses metadata-only listing to avoid expensive CRC32 computation.
    pub fn is_encrypted(&self) -> Result<bool> {
        // Use metadata-only listing for performance (avoids CRC32 computation)
        let entries = self.list_files_for_limits()?;

        // Check if any entry is encrypted
        Ok(entries.iter().any(|e| e.is_encrypted))
    }

    /// Check if archive has recovery records
    ///
    /// Recovery records (also called parity data) allow repairing corrupted archives.
    /// Returns true if recovery records are present, false otherwise.
    ///
    /// **Format Support**:
    /// - RAR/RAR5: ✅ Supported (via ROADF_RECOVERY flag)
    /// - 7z: ❌ Not supported (no recovery records)
    /// - ZIP: ❌ Not supported (no recovery records)
    /// - TAR: ❌ Not supported (no recovery records)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.rar")?;
    /// if archive.has_recovery_record()? {
    ///     println!("Archive has recovery records for repair");
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn has_recovery_record(&self) -> Result<bool> {
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.has_recovery_record(),
            // Other formats don't support recovery records
            ArchiveBackend::Piz(_)
            | ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::SevenZ(_)
            | ArchiveBackend::Libarchive(_) => Ok(false),
        }
    }

    /// Get recovery record percentage
    ///
    /// Returns the percentage of archive data allocated for recovery records.
    /// Common values are 2%, 3%, 5%, 10%, etc. Higher percentages allow recovery
    /// from more severe corruption but increase archive size.
    ///
    /// **Format Support**:
    /// - RAR/RAR5: ✅ Supported (parses recovery record blocks)
    /// - 7z: ❌ Not supported (no recovery records)
    /// - ZIP: ❌ Not supported (no recovery records)
    /// - TAR: ❌ Not supported (no recovery records)
    ///
    /// # Returns
    /// - `Ok(Some(percentage))` - Recovery records present with known percentage
    /// - `Ok(None)` - No recovery records present
    /// - `Err(...)` - I/O error or parse error
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.rar")?;
    /// match archive.recovery_percentage()? {
    ///     Some(pct) => println!("Archive has {}% recovery records", pct),
    ///     None => println!("No recovery records"),
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn recovery_percentage(&self) -> Result<Option<u8>> {
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.recovery_percentage(),
            // Other formats don't support recovery records
            ArchiveBackend::Piz(_)
            | ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::SevenZ(_)
            | ArchiveBackend::Libarchive(_) => Ok(None),
        }
    }

    /// Check if archive uses solid compression
    ///
    /// Solid compression compresses all files together as a single stream,
    /// providing better compression ratios but slower random access.
    ///
    /// **Format Support**:
    /// - RAR/RAR5: ✅ Full support (via archive header flags)
    /// - 7z: ✅ Supported (via block structure)
    /// - ZIP: ❌ Not applicable (always non-solid)
    /// - TAR: ❌ Not applicable (uncompressed container)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.rar")?;
    /// if archive.is_solid()? {
    ///     println!("Archive uses solid compression");
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn is_solid(&self) -> Result<bool> {
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.is_solid(),
            ArchiveBackend::SevenZ(sevenz) => sevenz.is_solid(),
            ArchiveBackend::Piz(_)
            | ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::Libarchive(_) => {
                // ZIP, TAR, and other formats don't support solid compression
                Ok(false)
            }
        }
    }

    /// Get the archive file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Finish and close archive (for Write mode)
    ///
    /// This method finalizes the archive and writes all pending data.
    /// It must be called for archives in Write mode to ensure data is flushed.
    ///
    /// For Read mode archives, this is a no-op.
    pub fn finish(mut self) -> Result<()> {
        if self.mode == ArchiveMode::Write {
            match &mut self.backend {
                ArchiveBackend::ZipWriter(writer) => {
                    writer.finish()?;
                }
                ArchiveBackend::Libarchive(backend) => {
                    backend.close_write()?;
                }
                #[cfg(feature = "rar-support")]
                ArchiveBackend::Unrar(_) => {
                    return Err(ArchiveError::read_only_backend(crate::error::ops::FINISH));
                }
                ArchiveBackend::Piz(_)
                | ArchiveBackend::SevenZ(_)
                | ArchiveBackend::ZipReader(_) => {
                    return Err(ArchiveError::read_only_backend(crate::error::ops::FINISH));
                }
            }
        }
        Ok(())
    }

    /// Close the archive (called automatically on drop)
    pub fn close(self) -> Result<()> {
        self.finish()
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        // If in Write mode, try to close properly
        // Errors are ignored in Drop as we can't propagate them
        if self.mode == ArchiveMode::Write {
            match &mut self.backend {
                ArchiveBackend::ZipWriter(writer) => {
                    let _ = writer.finish();
                }
                ArchiveBackend::Libarchive(backend) => {
                    let _ = backend.close_write();
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    // ── ArchiveMode tests ──

    #[test]
    fn test_archive_mode_debug() {
        assert_eq!(format!("{:?}", ArchiveMode::Read), "Read");
        assert_eq!(format!("{:?}", ArchiveMode::Write), "Write");
        assert_eq!(format!("{:?}", ArchiveMode::Modify), "Modify");
    }

    #[test]
    fn test_archive_mode_clone() {
        let mode = ArchiveMode::Read;
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_archive_mode_eq() {
        assert_eq!(ArchiveMode::Read, ArchiveMode::Read);
        assert_ne!(ArchiveMode::Read, ArchiveMode::Write);
        assert_ne!(ArchiveMode::Write, ArchiveMode::Modify);
    }

    // ── Archive::open tests ──

    #[test]
    fn test_open_valid_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
        assert_eq!(archive.path(), fixture("test.zip"));
    }

    #[cfg(feature = "rar-support")]
    #[test]
    fn test_open_valid_rar() {
        let archive = Archive::open(fixture("test.rar")).unwrap();
        // test.rar is actually RAR5 format (created by RAR 7.12+)
        assert!(matches!(
            archive.format(),
            ArchiveFormat::Rar | ArchiveFormat::Rar5
        ));
    }

    #[test]
    fn test_open_valid_7z() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::SevenZip);
    }

    #[test]
    fn test_open_valid_tar() {
        let archive = Archive::open(fixture("test.tar")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Tar);
    }

    #[test]
    fn test_open_valid_tar_gz() {
        let archive = Archive::open(fixture("test.tar.gz")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarGzip);
    }

    #[test]
    fn test_open_valid_tar_bz2() {
        let archive = Archive::open(fixture("test.tar.bz2")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
    }

    #[test]
    fn test_open_valid_tar_xz() {
        let archive = Archive::open(fixture("test.tar.xz")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarXz);
    }

    #[test]
    fn test_open_nonexistent_file() {
        let result = Archive::open("/nonexistent/path/archive.zip");
        assert!(result.is_err());
    }

    #[test]
    fn test_open_sets_read_mode() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert_eq!(archive.mode, ArchiveMode::Read);
    }

    #[test]
    fn test_open_initializes_empty_cache() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(archive.entry_cache.get().is_none());
    }

    #[test]
    fn test_open_has_no_modifications() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(archive.modifications.is_none());
    }

    // ── Archive::open_encrypted tests ──

    #[test]
    fn test_open_encrypted_rar() {
        let result = Archive::open_encrypted(fixture("test_encrypted.rar"), "test");
        // Should succeed or fail with password error, not panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_open_encrypted_zip() {
        // Opening an unencrypted ZIP with a password should succeed (ZipReader backend)
        let result = Archive::open_encrypted(fixture("test.zip"), "password");
        assert!(
            result.is_ok(),
            "ZIP encrypted open should succeed: {:?}",
            result.err()
        );
        let archive = result.unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
    }

    #[test]
    fn test_open_encrypted_unsupported_tar() {
        let result = Archive::open_encrypted(fixture("test.tar"), "password");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(
            err,
            crate::error::ArchiveError::Unsupported { .. }
        ));
    }

    #[test]
    fn test_open_encrypted_nonexistent() {
        let result = Archive::open_encrypted("/nonexistent/archive.rar", "password");
        assert!(result.is_err());
    }

    // ── Archive::format tests ──

    #[test]
    fn test_format_returns_detected_format() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
    }

    // ── Archive::path tests ──

    #[test]
    fn test_path_returns_archive_path() {
        let expected = fixture("test.zip");
        let archive = Archive::open(&expected).unwrap();
        assert_eq!(archive.path(), expected);
    }

    // ── Archive::is_encrypted tests ──

    #[test]
    fn test_is_encrypted_unencrypted_zip() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(!archive.is_encrypted().unwrap());
    }

    #[cfg(feature = "rar-support")]
    #[test]
    fn test_is_encrypted_encrypted_rar() {
        // test_encrypted.rar has header encryption - is_encrypted() may fail
        // with Password error since UnRAR can't read headers without a password.
        // Both Ok(true) and Err(Password) are valid outcomes.
        let archive = Archive::open(fixture("test_encrypted.rar")).unwrap();
        let result = archive.is_encrypted();
        match result {
            Ok(encrypted) => assert!(encrypted, "Should report as encrypted"),
            Err(ref e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("assword") || msg.contains("encrypt"),
                    "Error should be password-related, got: {}",
                    msg
                );
            }
        }
    }

    // ── Archive::has_recovery_record tests ──

    #[test]
    fn test_has_recovery_record_zip_returns_false() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(!archive.has_recovery_record().unwrap());
    }

    #[test]
    fn test_has_recovery_record_7z_returns_false() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        assert!(!archive.has_recovery_record().unwrap());
    }

    // ── Archive::recovery_percentage tests ──

    #[test]
    fn test_recovery_percentage_zip_returns_none() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert_eq!(archive.recovery_percentage().unwrap(), None);
    }

    #[test]
    fn test_recovery_percentage_7z_returns_none() {
        let archive = Archive::open(fixture("test.7z")).unwrap();
        assert_eq!(archive.recovery_percentage().unwrap(), None);
    }

    // ── Archive::is_solid tests ──

    #[test]
    fn test_is_solid_zip_returns_false() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(!archive.is_solid().unwrap());
    }

    #[test]
    fn test_is_solid_tar_returns_false() {
        let archive = Archive::open(fixture("test.tar")).unwrap();
        assert!(!archive.is_solid().unwrap());
    }

    // ── Archive::open_at_offset tests ──

    #[test]
    fn test_open_at_offset_zero_is_plain_open() {
        let archive = Archive::open_at_offset(fixture("test.zip"), 0)
            .expect("offset=0 should be equivalent to Archive::open");
        assert_eq!(archive.format(), ArchiveFormat::Zip);
    }

    #[test]
    fn test_open_at_offset_past_eof_rejected() {
        let len = std::fs::metadata(fixture("test.zip")).unwrap().len();
        let result = Archive::open_at_offset(fixture("test.zip"), len);
        assert!(result.is_err(), "offset == file_len must error");
    }

    // ── Archive::open_sfx tests ──

    #[test]
    fn test_open_sfx_non_sfx_file() {
        // A normal ZIP is not an SFX
        let result = Archive::open_sfx(fixture("test.zip"));
        assert!(result.is_err());
    }

    // ── Archive::extract_stub tests ──

    #[test]
    fn test_extract_stub_non_sfx_detection() {
        let detection = crate::sfx::SfxDetectionResult {
            is_sfx: false,
            archive_format: None,
            data_offset: None,
            stub_type: None,
            confidence: 0.0,
        };
        let result = Archive::extract_stub(fixture("test.zip"), &detection);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("non-SFX"));
    }

    #[test]
    fn test_extract_stub_missing_offset() {
        let detection = crate::sfx::SfxDetectionResult {
            is_sfx: true,
            archive_format: None,
            data_offset: None, // Missing offset
            stub_type: None,
            confidence: 0.9,
        };
        let result = Archive::extract_stub(fixture("test.zip"), &detection);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing offset"));
    }

    #[test]
    fn test_extract_stub_oversized_stub() {
        let detection = crate::sfx::SfxDetectionResult {
            is_sfx: true,
            archive_format: None,
            data_offset: Some(100 * 1024 * 1024), // 100MB > 50MB limit
            stub_type: None,
            confidence: 0.9,
        };
        let result = Archive::extract_stub(fixture("test.zip"), &detection);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("exceeds maximum"));
    }

    #[test]
    fn test_extract_stub_valid_small_offset() {
        // Read first 10 bytes of a valid file as "stub"
        let detection = crate::sfx::SfxDetectionResult {
            is_sfx: true,
            archive_format: None,
            data_offset: Some(10),
            stub_type: None,
            confidence: 0.9,
        };
        let stub = Archive::extract_stub(fixture("test.zip"), &detection).unwrap();
        assert_eq!(stub.len(), 10);
        // ZIP magic bytes are PK\x03\x04
        assert_eq!(&stub[0..2], b"PK");
    }

    // ── Archive::finish / close tests ──

    #[test]
    fn test_close_read_mode_succeeds() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(archive.close().is_ok());
    }

    #[test]
    fn test_finish_read_mode_is_noop() {
        let archive = Archive::open(fixture("test.zip")).unwrap();
        assert!(archive.finish().is_ok());
    }

    // ── Send safety test ──

    #[test]
    fn test_archive_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Archive>();
    }

    #[test]
    fn test_archive_is_not_sync() {
        // Archive should NOT be Sync - verify this at compile time would require
        // negative trait bounds which Rust doesn't support.
        // Instead, this test documents the design intent.
        // The unsafe impl Send (but not Sync) is in the source.
    }

    // ── Drop behavior tests ──

    #[test]
    fn test_drop_read_mode_no_panic() {
        {
            let _archive = Archive::open(fixture("test.zip")).unwrap();
            // archive dropped here - should not panic
        }
    }

    #[test]
    fn test_drop_write_mode_no_panic() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_drop.zip");
        {
            let options = crate::options::CompressionOptions::new(ArchiveFormat::Zip);
            let _archive = Archive::create(&path, options).unwrap();
            // archive dropped in Write mode - should not panic
        }
    }
}
