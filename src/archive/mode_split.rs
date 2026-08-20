//! D2 typed-handle split (AD 0053) — read/write/modify surfaces as
//! distinct types behind the `v2-api` feature flag.
//!
//! v0.3 ships the new types alongside the existing
//! [`crate::Archive`] sum type. v0.4 will flip `v2-api` on by default
//! and (eventually) collapse the legacy facade. The new types
//! delegate to the underlying `Archive` rather than duplicating
//! logic — D2 is a structural change to the public surface, not a
//! behavioural one.
//!
//! ```ignore
//! // requires --features v2-api
//! use unified_archive::ZipCompressionOptions;
//! use unified_archive::v2::{ReadArchive, WriteArchive};
//!
//! let read = ReadArchive::open("backup.zip")?;
//! let entries = read.list_files()?;
//!
//! let write = WriteArchive::create_zip("out.zip", ZipCompressionOptions::new())?;
//! ```
//!
//! The split eliminates whole categories of facade-level
//! mis-routing: a `WriteArchive` cannot be passed to an extract
//! site, a `ReadArchive` cannot accept add-file calls, and
//! `WriteArchive::finish` is a consuming method — there is no
//! "commit happened then later a different commit happened" code
//! path because the type itself is gone after `finish`.

#![cfg(feature = "v2-api")]

use crate::archive::{Archive, ArchiveMode};
use crate::entry::ArchiveEntry;
use crate::error::{ArchiveWarning, Result, ResultWithWarnings};
use crate::format::ArchiveFormat;
use crate::inspection::{MultipartLayout, ValidationReport};
use crate::modification::ModificationOptions;
use crate::options::{
    CompressionOptions, ExtractionOptions, LibarchiveCompressionOptions, SevenZCompressionOptions,
    SfxStagingProgress, ZipCompressionOptions,
};
use crate::sfx::SfxDetectionResult;
use crate::streaming::{StreamBound, StreamingExtractor};
use std::path::{Path, PathBuf};

// ───────────────────────────────────────────────────────────────────
// ReadArchive — typed read-mode handle.
// ───────────────────────────────────────────────────────────────────

/// Read-mode archive handle (D2 / R0068-0027 / AD 0053).
///
/// Constructed via [`ReadArchive::open`] (and friends). Cannot accept
/// write or modify operations — those require [`WriteArchive`] /
/// [`ModifyArchive`] respectively. Wraps a regular [`Archive`] handle
/// internally so every read path benefits from the same backend
/// implementations and option plumbing.
pub struct ReadArchive {
    inner: Archive,
}

impl ReadArchive {
    /// Open an existing archive for reading (mirrors
    /// [`Archive::open`]).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let inner = Archive::open(path)?;
        debug_assert_eq!(inner.mode, ArchiveMode::Read);
        Ok(Self { inner })
    }

    /// Open an encrypted archive with the supplied password
    /// (mirrors [`Archive::open_encrypted`]).
    pub fn open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Self> {
        let inner = Archive::open_encrypted(path, password)?;
        // R0076-0084: assert read-mode in every typed constructor so a
        // future drift in the legacy facade can't slip through unnoticed.
        debug_assert_eq!(inner.mode, ArchiveMode::Read);
        Ok(Self { inner })
    }

    /// Open an archive that begins at a non-zero byte offset
    /// (mirrors [`Archive::open_at_offset`]).
    pub fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> {
        let inner = Archive::open_at_offset(path, offset)?;
        debug_assert_eq!(inner.mode, ArchiveMode::Read);
        Ok(Self { inner })
    }

    /// Open a self-extracting archive (mirrors
    /// [`Archive::open_sfx`]).
    pub fn open_sfx(path: impl AsRef<Path>) -> Result<Self> {
        let inner = Archive::open_sfx(path)?;
        debug_assert_eq!(inner.mode, ArchiveMode::Read);
        Ok(Self { inner })
    }

    /// On-disk path the archive was opened from.
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Detected archive format.
    pub fn format(&self) -> ArchiveFormat {
        self.inner.format()
    }

    /// List every entry's metadata.
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.inner.list_files()
    }

    /// Extract every file to `options.destination`.
    pub fn extract_all(&self, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> {
        self.inner.extract_all(options)
    }

    /// Extract `file_path` into memory.
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.inner.extract_to_memory(file_path)
    }

    /// Extract `file_path` as a streaming reader under an explicit
    /// [`StreamBound`] (mirrors [`Archive::extract_to_stream`]).
    pub fn extract_to_stream(
        &self,
        file_path: &str,
        bound: StreamBound,
    ) -> Result<StreamingExtractor> {
        self.inner.extract_to_stream(file_path, bound)
    }

    /// Walk every file entry and report integrity failures.
    pub fn validate_integrity(&self) -> Result<ValidationReport> {
        self.inner.validate_integrity()
    }

    /// Detect the archive's multipart layout (typed return per
    /// R0075-0083).
    pub fn multipart_layout(&self) -> Result<MultipartLayout> {
        self.inner.multipart_layout()
    }

    /// Compute the archive's content-multiset digest.
    pub fn calculate_manifest_digest(&self) -> Result<String> {
        self.inner.calculate_manifest_digest()
    }

    /// Probe whether the archive uses encryption (best-effort).
    pub fn is_encrypted(&self) -> Result<bool> {
        self.inner.is_encrypted()
    }

    /// Detect whether `path` is a self-extracting archive (mirrors
    /// the static [`Archive::detect_sfx`] helper, exposed here for
    /// API symmetry with the typed handle).
    pub fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult> {
        Archive::detect_sfx(path)
    }

    // ── Parity additions (R0076-0085): full read surface ────────────

    /// Open a self-extracting archive while observing — and optionally
    /// cancelling — the payload-staging copy (mirrors
    /// [`Archive::open_with_sfx_progress`]).
    pub fn open_with_sfx_progress(
        path: impl AsRef<Path>,
        progress: Option<SfxStagingProgress>,
    ) -> Result<Self> {
        let inner = Archive::open_with_sfx_progress(path, progress)?;
        debug_assert_eq!(inner.mode, ArchiveMode::Read);
        Ok(Self { inner })
    }

    /// Extract the executable stub from an SFX archive for analysis
    /// (mirrors the static [`Archive::extract_stub`]).
    pub fn extract_stub(path: impl AsRef<Path>, detection: &SfxDetectionResult) -> Result<Vec<u8>> {
        Archive::extract_stub(path, detection)
    }

    /// Pure extension-derived format guess (mirrors
    /// [`Archive::extension_format`]). Compare against
    /// [`ReadArchive::format`] to detect extension/content mismatches.
    pub fn extension_format(&self) -> Option<ArchiveFormat> {
        self.inner.extension_format()
    }

    /// Whether the archive uses solid compression (mirrors
    /// [`Archive::is_solid`]).
    pub fn is_solid(&self) -> Result<bool> {
        self.inner.is_solid()
    }

    /// Whether the archive carries a recovery record — RAR/RAR5 only
    /// (mirrors [`Archive::has_recovery_record`]).
    pub fn has_recovery_record(&self) -> Result<bool> {
        self.inner.has_recovery_record()
    }

    /// Recovery-record percentage when present — RAR/RAR5 only
    /// (mirrors [`Archive::recovery_percentage`]).
    pub fn recovery_percentage(&self) -> Result<Option<u8>> {
        self.inner.recovery_percentage()
    }

    /// Count of entries in the archive (mirrors
    /// [`Archive::entry_count`]).
    pub fn entry_count(&self) -> Result<usize> {
        self.inner.entry_count()
    }

    /// Find the first entry matching `path` (mirrors
    /// [`Archive::find_entry`]).
    pub fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>> {
        self.inner.find_entry(path)
    }

    /// Find every entry matching `path` (mirrors
    /// [`Archive::find_entries`]).
    pub fn find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>> {
        self.inner.find_entries(path)
    }

    /// Owned, uncached listing for safety-limit pre-checks (mirrors
    /// [`Archive::list_files_for_limits`]).
    pub fn list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>> {
        self.inner.list_files_for_limits()
    }

    /// Archive-level CRC32 summary (mirrors
    /// [`Archive::calculate_archive_crc`]).
    pub fn calculate_archive_crc(&self) -> Result<u32> {
        self.inner.calculate_archive_crc()
    }

    /// Content-multiset digest plus total uncompressed size (mirrors
    /// [`Archive::calculate_content_multiset_digest_and_size`]).
    pub fn calculate_content_multiset_digest_and_size(&self) -> Result<(String, u64)> {
        self.inner.calculate_content_multiset_digest_and_size()
    }

    /// Legacy manifest-summary shim (mirrors
    /// [`Archive::calculate_manifest_summary`]).
    pub fn calculate_manifest_summary(&self) -> Result<(String, u64)> {
        self.inner.calculate_manifest_summary()
    }

    /// Untyped multipart detection (mirrors
    /// [`Archive::detect_multipart`]). Prefer
    /// [`ReadArchive::multipart_layout`] for new code.
    pub fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)> {
        self.inner.detect_multipart()
    }

    /// Surface symlink/hardlink warnings without extracting (mirrors
    /// [`Archive::check_symlinks`]).
    pub fn check_symlinks(&self) -> Result<Vec<ArchiveWarning>> {
        self.inner.check_symlinks()
    }

    /// Extract a single file to `options.destination` (mirrors
    /// [`Archive::extract_file`]).
    pub fn extract_file(&self, file_path: &str, options: ExtractionOptions) -> Result<()> {
        self.inner.extract_file(file_path, options)
    }

    /// Extract a name-selected subset to disk (mirrors
    /// [`Archive::extract_files`]).
    pub fn extract_files(
        &self,
        paths: &[&str],
        options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>> {
        self.inner.extract_files(paths, options)
    }

    /// Extract an id-selected subset to disk, preserving entry
    /// identity across duplicate paths (mirrors
    /// [`Archive::extract_by_ids`]).
    pub fn extract_by_ids(
        &self,
        ids: &[usize],
        options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>> {
        self.inner.extract_by_ids(ids, options)
    }

    /// Extract entries matching `predicate` in a single pass (mirrors
    /// [`Archive::extract_some`]).
    pub fn extract_some<F>(
        &self,
        predicate: F,
        options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>>
    where
        F: FnMut(&ArchiveEntry) -> bool,
    {
        self.inner.extract_some(predicate, options)
    }

    /// Alias for [`ReadArchive::extract_some`] (mirrors
    /// [`Archive::extract_filtered`]).
    pub fn extract_filtered<F>(
        &self,
        predicate: F,
        options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>>
    where
        F: FnMut(&ArchiveEntry) -> bool,
    {
        self.inner.extract_filtered(predicate, options)
    }

    /// Extract a single file into memory honoring `options` (mirrors
    /// [`Archive::extract_to_memory_with_options`]).
    pub fn extract_to_memory_with_options(
        &self,
        file_path: &str,
        options: &ExtractionOptions,
    ) -> Result<Vec<u8>> {
        self.inner
            .extract_to_memory_with_options(file_path, options)
    }

    /// Streaming read honoring `options` under an explicit
    /// [`StreamBound`] (mirrors
    /// [`Archive::extract_to_stream_with_options`]).
    pub fn extract_to_stream_with_options(
        &self,
        file_path: &str,
        options: &ExtractionOptions,
        bound: StreamBound,
    ) -> Result<StreamingExtractor> {
        self.inner
            .extract_to_stream_with_options(file_path, options, bound)
    }
}

// ───────────────────────────────────────────────────────────────────
// WriteArchive — typed write-mode handle with consuming finish.
// ───────────────────────────────────────────────────────────────────

/// Write-mode archive handle (D2 / R0068-0027 / AD 0053).
///
/// Constructed via [`WriteArchive::create`] and the per-format
/// helpers. The terminal operation is [`WriteArchive::finish`],
/// which **consumes** the handle — there is no "did I forget to
/// finalise?" race window because the type is gone afterwards.
///
/// `finish()` is the durable, **error-surfacing** commit path
/// (R0076-0088): it is the only way to learn whether the final flush
/// succeeded. Dropping a `WriteArchive` without `finish()` still runs
/// a best-effort finalize — the underlying writer is flushed and its
/// resources released rather than leaked — but any error is swallowed
/// behind a single stderr warning. The typed handle is the sole
/// finalization owner: the inner [`Archive`]'s legacy `Drop`-finalize
/// is suppressed so finalization, and its warning, happen exactly once.
pub struct WriteArchive {
    inner: Option<Archive>,
}

impl WriteArchive {
    /// Create a new archive with the supplied options.
    pub fn create(path: impl AsRef<Path>, options: CompressionOptions) -> Result<Self> {
        let inner = Archive::create(path, options)?;
        Ok(Self { inner: Some(inner) })
    }

    /// Create a ZIP archive (mirrors [`Archive::create_zip`]).
    pub fn create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Self> {
        let inner = Archive::create_zip(path, opts)?;
        Ok(Self { inner: Some(inner) })
    }

    /// Create a 7-Zip archive (mirrors
    /// [`Archive::create_seven_zip`]).
    pub fn create_seven_zip(
        path: impl AsRef<Path>,
        opts: SevenZCompressionOptions,
    ) -> Result<Self> {
        let inner = Archive::create_seven_zip(path, opts)?;
        Ok(Self { inner: Some(inner) })
    }

    /// Create a libarchive-backed archive (mirrors
    /// [`Archive::create_libarchive`]).
    pub fn create_libarchive(
        path: impl AsRef<Path>,
        opts: LibarchiveCompressionOptions,
    ) -> Result<Self> {
        let inner = Archive::create_libarchive(path, opts)?;
        Ok(Self { inner: Some(inner) })
    }

    fn inner_mut(&mut self) -> &mut Archive {
        self.inner
            .as_mut()
            .expect("WriteArchive has been finished; this is a logic bug")
    }

    fn inner_ref(&self) -> &Archive {
        self.inner
            .as_ref()
            .expect("WriteArchive has been finished; this is a logic bug")
    }

    /// On-disk path of the output archive.
    pub fn path(&self) -> &Path {
        self.inner_ref().path()
    }

    /// Output archive format.
    pub fn format(&self) -> ArchiveFormat {
        self.inner_ref().format()
    }

    /// Add a file to the archive from in-memory bytes.
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        self.inner_mut().add_file_from_data(archive_path, data)
    }

    /// Add a file to the archive from a filesystem path.
    pub fn add_file_from_path(&mut self, fs_path: impl AsRef<Path>) -> Result<()> {
        self.inner_mut().add_file_from_path(fs_path)
    }

    /// Add a file to the archive from a filesystem path with a
    /// custom archive-internal name.
    pub fn add_file_from_path_as(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        self.inner_mut()
            .add_file_from_path_as(fs_path, archive_path)
    }

    /// Add a directory entry to the archive.
    pub fn add_directory(&mut self, archive_path: &str) -> Result<()> {
        self.inner_mut().add_directory(archive_path)
    }

    /// Add a directory and all its contents recursively (mirrors
    /// [`Archive::add_directory_recursive`]). R0076-0086.
    pub fn add_directory_recursive(&mut self, fs_path: impl AsRef<Path>) -> Result<()> {
        self.inner_mut().add_directory_recursive(fs_path)
    }

    /// Number of entries the writer has emitted so far (write-progress
    /// count; mirrors [`Archive::entry_count`] in Write mode).
    pub fn entry_count(&self) -> Result<usize> {
        self.inner_ref().entry_count()
    }

    /// Finalise the archive and return its on-disk path. This
    /// consumes the handle — calling any method on it afterwards is
    /// a compile error.
    pub fn finish(mut self) -> Result<PathBuf> {
        let inner = self
            .inner
            .take()
            .expect("WriteArchive has already been finished");
        let path = inner.path().to_path_buf();
        inner.finish()?;
        Ok(path)
    }

    /// Alias for [`Self::finish`]; kept for source-compat with
    /// callers that prefer the "close" verb.
    pub fn close(self) -> Result<PathBuf> {
        self.finish()
    }
}

impl Drop for WriteArchive {
    fn drop(&mut self) {
        // R0076-0088: single finalization owner. `finish()` is the
        // durable, error-surfacing commit path; a `WriteArchive`
        // dropped without it runs a best-effort finalize HERE — so the
        // libarchive write handle is freed and the ZIP central
        // directory flushed rather than leaked — emits exactly one
        // warning, and (via `finalize_write_on_drop` marking the inner
        // handle finalized) suppresses the inner `Archive`'s own
        // `Drop`-finalize so finalization never happens twice.
        if let Some(inner) = self.inner.as_mut() {
            let path = inner.path().to_path_buf();
            match inner.finalize_write_on_drop() {
                Ok(()) => eprintln!(
                    "unified-archive: WriteArchive for `{}` dropped without finish() — a \
                     best-effort finalize ran, but commit errors are not surfaced this way. \
                     Call WriteArchive::finish() for a checked, durable commit.",
                    path.display()
                ),
                Err(e) => eprintln!(
                    "unified-archive: WriteArchive for `{}` dropped without finish() and the \
                     best-effort finalize failed: {e}. The archive may be incomplete; call \
                     WriteArchive::finish() to surface this error explicitly.",
                    path.display()
                ),
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// ModifyArchive — typed modify-mode handle.
// ───────────────────────────────────────────────────────────────────

/// Modify-mode archive handle (D2 / R0068-0027 / AD 0053).
///
/// Holds an exclusive advisory lock on the archive path for the
/// lifetime of the handle (MADR-0009 / MADR-0016). The terminal operation
/// is [`ModifyArchive::commit_changes`], which **consumes** the
/// handle and writes a new archive atomically over the original.
/// Drops without `commit_changes` leave the original archive on disk
/// unchanged — there is no auto-commit boundary at Drop.
///
/// Callers that want to validate their queued operations without
/// throwing the handle away on rejection use
/// [`ModifyArchive::try_commit_changes`] (R0075-0039), which runs
/// the same dup-path namespace check + ZIP source-extras
/// cross-check that `commit_changes` performs at its pre-write gate.
pub struct ModifyArchive {
    inner: Option<Archive>,
}

impl ModifyArchive {
    /// Open `path` for modification with default options.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let inner = Archive::modify(path)?;
        Ok(Self { inner: Some(inner) })
    }

    /// Open `path` for modification with the supplied options
    /// (compression overrides, backup policy, metadata-preservation
    /// flags).
    pub fn open_with_options(path: impl AsRef<Path>, opts: ModificationOptions) -> Result<Self> {
        let inner = Archive::modify_with_options(path, opts)?;
        Ok(Self { inner: Some(inner) })
    }

    fn inner_mut(&mut self) -> &mut Archive {
        self.inner
            .as_mut()
            .expect("ModifyArchive has been committed; this is a logic bug")
    }

    fn inner_ref(&self) -> &Archive {
        self.inner
            .as_ref()
            .expect("ModifyArchive has been committed; this is a logic bug")
    }

    /// On-disk path of the archive being modified.
    pub fn path(&self) -> &Path {
        self.inner_ref().path()
    }

    /// Format of the archive being modified.
    pub fn format(&self) -> ArchiveFormat {
        self.inner_ref().format()
    }

    /// Queue an entry addition from in-memory bytes.
    pub fn add_entry(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        self.inner_mut().add_entry(archive_path, data)
    }

    /// Queue an entry addition by reading from a filesystem path.
    pub fn add_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()> {
        self.inner_mut().add_entry_from_path(archive_path, fs_path)
    }

    /// Queue an entry addition from an arbitrary reader (mirrors
    /// [`Archive::add_entry_from_reader`]). `size: Some(n)` is trusted
    /// as the declared size; `None` triggers bounded tempfile staging.
    /// R0076-0087.
    pub fn add_entry_from_reader<R>(
        &mut self,
        archive_path: &str,
        reader: R,
        size: Option<u64>,
    ) -> Result<()>
    where
        R: std::io::Read + Send + 'static,
    {
        self.inner_mut()
            .add_entry_from_reader(archive_path, reader, size)
    }

    /// Queue a directory-entry addition.
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> {
        self.inner_mut().add_directory_entry(archive_path)
    }

    /// Queue an entry replacement from in-memory bytes (mirrors
    /// [`Archive::replace_entry`]). A path with no match silently adds.
    /// R0076-0087.
    pub fn replace_entry(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        self.inner_mut().replace_entry(archive_path, data)
    }

    /// Queue an entry replacement from a filesystem path (mirrors
    /// [`Archive::replace_entry_from_path`]). R0076-0087.
    pub fn replace_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()> {
        self.inner_mut()
            .replace_entry_from_path(archive_path, fs_path)
    }

    /// Queue an entry replacement from an arbitrary reader (mirrors
    /// [`Archive::replace_entry_from_reader`]); size contract matches
    /// [`ModifyArchive::add_entry_from_reader`]. R0076-0087.
    pub fn replace_entry_from_reader<R>(
        &mut self,
        archive_path: &str,
        reader: R,
        size: Option<u64>,
    ) -> Result<()>
    where
        R: std::io::Read + Send + 'static,
    {
        self.inner_mut()
            .replace_entry_from_reader(archive_path, reader, size)
    }

    /// List the source archive's entries (mirrors
    /// [`Archive::list_files`]). Needed to discover the
    /// [`ArchiveEntry::id`]s that [`ModifyArchive::remove_entry_by_id`]
    /// consumes.
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.inner_ref().list_files()
    }

    /// Count entries in the source archive (mirrors
    /// [`Archive::entry_count`]).
    pub fn entry_count(&self) -> Result<usize> {
        self.inner_ref().entry_count()
    }

    /// Find the first source entry matching `path` (mirrors
    /// [`Archive::find_entry`]).
    pub fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>> {
        self.inner_ref().find_entry(path)
    }

    /// Find every source entry matching `path` (mirrors
    /// [`Archive::find_entries`]); pair the returned ids with
    /// [`ModifyArchive::remove_entry_by_id`] to target one occurrence.
    pub fn find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>> {
        self.inner_ref().find_entries(path)
    }

    /// Queue an entry removal by archive-internal path. Returns the
    /// number of entries marked for removal (1 for unique paths,
    /// possibly more for archives with duplicate-named entries).
    pub fn remove_entry(&mut self, archive_path: &str) -> Result<usize> {
        self.inner_mut().remove_entry(archive_path)
    }

    /// Queue an entry removal by stable per-listing entry id.
    pub fn remove_entry_by_id(&mut self, id: usize) -> Result<()> {
        self.inner_mut().remove_entry_by_id(id)
    }

    /// Drop every queued operation without touching the archive.
    pub fn clear_operations(&mut self) -> Result<()> {
        self.inner_mut().clear_operations()
    }

    /// Number of operations currently queued.
    pub fn pending_operations(&self) -> Result<usize> {
        self.inner_ref().pending_operations()
    }

    /// **R0075-0039**: dry-run validation — confirm the queued
    /// operations would pass `commit_changes`'s pre-write gates
    /// without touching the filesystem.
    ///
    /// Runs the dup-path namespace check (R0069-0062) and, for ZIP
    /// archives, the source-extras cross-check (R0069-0064 /
    /// R0075-0034). I/O errors during the cross-check (corrupt
    /// source archive) surface as the underlying error.
    ///
    /// On success the handle is unchanged — the queued operations
    /// remain ready for a real `commit_changes`. On failure the
    /// caller can drop / reset operations / retry without the
    /// handle replacement that the legacy consuming `commit_changes`
    /// requires on rejection.
    pub fn try_commit_changes(&mut self) -> Result<()> {
        let inner = self.inner_ref();
        let modifications = inner.modifications.as_ref().ok_or_else(|| {
            crate::ArchiveError::operation_blocked(
                crate::error::ops::COMMIT_CHANGES,
                "No modification tracker — handle is not in modify mode",
            )
        })?;
        inner.validate_pending_commit(modifications).map(|_| ())
    }

    /// Apply every queued operation. **Consumes** the handle and the
    /// advisory lock; the original archive is replaced atomically
    /// with the new one. On failure the original is left intact and
    /// the handle is gone — call [`ModifyArchive::open`] again to
    /// retry.
    pub fn commit_changes(mut self) -> Result<()> {
        let inner = self
            .inner
            .take()
            .expect("ModifyArchive has already been committed");
        inner.commit_changes()
    }
}

// `ModifyArchive` deliberately has NO Drop warning — the original
// archive is still on disk if the handle is dropped without commit.
// Compare `WriteArchive::Drop`, which leaves a partially-written
// new archive and warns the user.

// ───────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dest(prefix: &str, ext: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}_{nonce}.{ext}"))
    }

    #[test]
    fn read_archive_lists_zip_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip");
        let r = ReadArchive::open(&path).expect("open");
        let entries = r.list_files().expect("list");
        assert!(!entries.is_empty());
        assert_eq!(r.format(), ArchiveFormat::Zip);
    }

    #[test]
    fn write_archive_round_trips_zip() {
        let path = unique_dest("v2_write_zip", "zip");
        let _ = std::fs::remove_file(&path);

        let mut w =
            WriteArchive::create_zip(&path, ZipCompressionOptions::new()).expect("create_zip");
        w.add_file_from_data("hi.txt", b"hello").expect("add_file");
        let final_path = w.finish().expect("finish");
        assert_eq!(final_path, path);

        let r = ReadArchive::open(&path).expect("read back");
        let entries = r.list_files().expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hi.txt");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_archive_finish_consumes_handle() {
        // Compile-time test: this `let _ = w.add_file_from_data(...);`
        // after a `w.finish()` would not compile. We can't verify
        // compile failures inline, so just exercise the method
        // dispatch to confirm `finish()` returns the path and the
        // value is moved.
        let path = unique_dest("v2_finish_consume", "zip");
        let _ = std::fs::remove_file(&path);
        let w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
        let returned = w.finish().unwrap();
        assert_eq!(returned, path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn modify_archive_round_trip_zip() {
        // Build a seed archive, open it for modification through the
        // typed handle, queue an addition, commit, verify.
        let path = unique_dest("v2_modify_zip", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("seed.txt", b"x").unwrap();
            w.finish().unwrap();
        }

        let mut m = ModifyArchive::open(&path).expect("open for modify");
        m.add_entry("added.txt", b"y").expect("add_entry");
        assert_eq!(m.pending_operations().unwrap(), 1);
        m.commit_changes().expect("commit");

        let r = ReadArchive::open(&path).expect("read back");
        let entries = r.list_files().expect("list");
        let names: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"seed.txt"));
        assert!(names.contains(&"added.txt"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn modify_archive_try_commit_does_not_write_to_disk() {
        // R0075-0039: try_commit_changes must not mutate the archive.
        let path = unique_dest("v2_modify_try_commit", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("seed.txt", b"x").unwrap();
            w.finish().unwrap();
        }
        let original_modtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        // Sleep briefly so a write would produce a distinguishable
        // mtime even on coarse-grained filesystems.
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut m = ModifyArchive::open(&path).expect("open");
        m.add_entry("added.txt", b"y").expect("add_entry");

        // Dry-run validation passes (no dup-path conflicts).
        m.try_commit_changes().expect("try_commit_changes");

        // The on-disk archive is unchanged.
        let new_modtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            original_modtime, new_modtime,
            "try_commit_changes must not write to disk"
        );

        // The handle is still usable — pending operations are still
        // queued and a real commit_changes will apply them.
        assert_eq!(m.pending_operations().unwrap(), 1);
        m.commit_changes().expect("real commit after try");

        let r = ReadArchive::open(&path).expect("read back");
        assert_eq!(r.list_files().unwrap().len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn modify_archive_try_commit_rejects_duplicate_path() {
        // R0075-0039: try_commit_changes surfaces the same dup-path
        // error that a real commit_changes would.
        let path = unique_dest("v2_modify_try_dup", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("seed.txt", b"x").unwrap();
            w.finish().unwrap();
        }

        let mut m = ModifyArchive::open(&path).expect("open");
        // Queue an addition with the same path as an existing entry,
        // without removing the original — this is the
        // retained-vs-added duplicate that `commit_changes` rejects.
        m.add_entry("seed.txt", b"y").expect("add_entry");

        let result = m.try_commit_changes();
        assert!(
            result.is_err(),
            "Duplicate path 'seed.txt' must surface as Err from try_commit_changes"
        );

        // Handle is still alive; clear and confirm.
        m.clear_operations().expect("clear");
        let _ = std::fs::remove_file(&path);
    }

    // ── Parity additions (OI-0076-004) ──────────────────────────────

    #[test]
    fn read_archive_parity_methods_on_zip() {
        let path = unique_dest("v2_read_parity", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("a.txt", b"alpha").unwrap();
            w.add_file_from_data("b.txt", b"beta").unwrap();
            w.add_file_from_data("c.log", b"gamma").unwrap();
            w.finish().unwrap();
        }
        let r = ReadArchive::open(&path).unwrap();

        // entry_count / find_entry / find_entries / list_files_for_limits
        assert_eq!(r.entry_count().unwrap(), 3);
        assert!(r.find_entry("a.txt").unwrap().is_some());
        assert!(r.find_entry("missing").unwrap().is_none());
        assert_eq!(r.find_entries("b.txt").unwrap().len(), 1);
        assert_eq!(r.list_files_for_limits().unwrap().len(), 3);

        // extract_to_memory_with_options
        let bytes = r
            .extract_to_memory_with_options("a.txt", &ExtractionOptions::default())
            .unwrap();
        assert_eq!(bytes, b"alpha");

        // extract_file to a temp dir
        let dest = unique_dest("v2_read_parity_dest", "d");
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&dest).unwrap();
        let fopts = ExtractionOptions {
            destination: dest.clone(),
            ..Default::default()
        };
        r.extract_file("a.txt", fopts).unwrap();
        assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"alpha");

        // extract_some (predicate on extension)
        let dest2 = unique_dest("v2_read_parity_some", "d");
        let _ = std::fs::remove_dir_all(&dest2);
        std::fs::create_dir_all(&dest2).unwrap();
        let sopts = ExtractionOptions {
            destination: dest2.clone(),
            ..Default::default()
        };
        r.extract_some(|e| e.path.ends_with(".txt"), sopts).unwrap();
        assert!(dest2.join("a.txt").exists());
        assert!(dest2.join("b.txt").exists());
        assert!(!dest2.join("c.log").exists());

        // extract_by_ids
        let ids: Vec<usize> = r
            .list_files()
            .unwrap()
            .iter()
            .filter(|e| e.path == "c.log")
            .map(|e| e.id)
            .collect();
        let dest3 = unique_dest("v2_read_parity_ids", "d");
        let _ = std::fs::remove_dir_all(&dest3);
        std::fs::create_dir_all(&dest3).unwrap();
        let iopts = ExtractionOptions {
            destination: dest3.clone(),
            ..Default::default()
        };
        r.extract_by_ids(&ids, iopts).unwrap();
        assert!(dest3.join("c.log").exists());

        // detect_multipart (single archive) + crc + symlink scan
        let (is_multi, parts) = r.detect_multipart().unwrap();
        assert!(!is_multi);
        assert_eq!(parts.len(), 1);
        let _ = r.calculate_archive_crc().unwrap();
        assert!(r.check_symlinks().unwrap().is_empty());
        assert_eq!(r.format(), ArchiveFormat::Zip);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_dir_all(&dest2);
        let _ = std::fs::remove_dir_all(&dest3);
    }

    #[test]
    fn write_archive_add_directory_recursive_and_entry_count() {
        let src = unique_dest("v2_recursive_src", "d");
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("root.txt"), b"r").unwrap();
        std::fs::write(src.join("sub").join("leaf.txt"), b"l").unwrap();

        let path = unique_dest("v2_recursive_zip", "zip");
        let _ = std::fs::remove_file(&path);
        let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
        w.add_directory_recursive(&src).unwrap();
        // Write-mode entry_count reflects write progress.
        assert!(w.entry_count().unwrap() > 0);
        w.finish().unwrap();

        let r = ReadArchive::open(&path).unwrap();
        let names: Vec<String> = r
            .list_files()
            .unwrap()
            .iter()
            .map(|e| e.path.clone())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("root.txt")));
        assert!(names.iter().any(|n| n.ends_with("leaf.txt")));

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn modify_archive_reader_replace_and_source_inspection() {
        let path = unique_dest("v2_modify_parity", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("keep.txt", b"keep").unwrap();
            w.add_file_from_data("old.txt", b"old").unwrap();
            w.finish().unwrap();
        }

        let mut m = ModifyArchive::open(&path).unwrap();
        // Source inspection — needed to discover entries/ids before editing.
        assert_eq!(m.entry_count().unwrap(), 2);
        assert!(m.find_entry("keep.txt").unwrap().is_some());
        assert_eq!(m.list_files().unwrap().len(), 2);
        assert_eq!(m.find_entries("old.txt").unwrap().len(), 1);

        // add_entry_from_reader + replace_entry
        m.add_entry_from_reader(
            "reader.txt",
            std::io::Cursor::new(b"from-reader".to_vec()),
            Some(11),
        )
        .unwrap();
        m.replace_entry("old.txt", b"new").unwrap();
        m.commit_changes().unwrap();

        let r = ReadArchive::open(&path).unwrap();
        assert_eq!(r.extract_to_memory("reader.txt").unwrap(), b"from-reader");
        assert_eq!(r.extract_to_memory("old.txt").unwrap(), b"new");
        assert_eq!(r.extract_to_memory("keep.txt").unwrap(), b"keep");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_archive_drop_without_finish_best_effort_finalizes_zip() {
        // R0076-0088: dropping without finish() runs a best-effort
        // finalize (single owner, no double-finalize, no leak), so a ZIP
        // is still readable. finish() is the path that SURFACES errors.
        let path = unique_dest("v2_write_drop_zip", "zip");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_zip(&path, ZipCompressionOptions::new()).unwrap();
            w.add_file_from_data("d.txt", b"dropped").unwrap();
            // no finish(): Drop best-effort finalizes and warns once.
        }
        let r = ReadArchive::open(&path).expect("best-effort finalize leaves a readable zip");
        assert_eq!(r.extract_to_memory("d.txt").unwrap(), b"dropped");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_archive_drop_without_finish_finalizes_libarchive_tar() {
        // R0076-0088: the libarchive backend has NO Drop of its own, so
        // the typed handle's best-effort finalize is what frees the
        // write handle and closes the archive. Without it the handle
        // would leak and the tar would be truncated/unreadable.
        let path = unique_dest("v2_write_drop_tar", "tar");
        let _ = std::fs::remove_file(&path);
        {
            let mut w = WriteArchive::create_libarchive(
                &path,
                LibarchiveCompressionOptions::new(ArchiveFormat::Tar),
            )
            .unwrap();
            w.add_file_from_data("t.txt", b"tar-dropped").unwrap();
            // no finish()
        }
        let r = ReadArchive::open(&path)
            .expect("libarchive best-effort finalize leaves a readable tar");
        assert_eq!(r.extract_to_memory("t.txt").unwrap(), b"tar-dropped");
        let _ = std::fs::remove_file(&path);
    }
}
