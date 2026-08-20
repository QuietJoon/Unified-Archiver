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
use zip::write::{FullFileOptions, SimpleFileOptions, ZipWriter as RawZipWriter};

/// ZIP64 opt-in policy (R0079-0027): entries whose payload size is
/// unknown up front (reader-based adds) or known to exceed the 4 GiB
/// classic-field limit get `large_file(true)` so the zip crate reserves
/// ZIP64 local-header space instead of aborting the entry mid-write
/// when it crosses the threshold (which would also poison the writer).
/// Known-small entries stay non-ZIP64 for maximum reader compatibility.
fn needs_zip64(known_size: Option<u64>) -> bool {
    known_size.is_none_or(|size| size >= zip::ZIP64_BYTES_THR)
}

/// Write an in-memory payload to the current entry in 64 KiB chunks,
/// firing `notify` per chunk so the progress/cancellation hook can
/// interrupt mid-entry (R0075-0041). Shared by `add_file_from_data`
/// and its metadata-preserving variant so both observe the same
/// per-chunk notify cadence (R0076-0015). The chunk size matches
/// `copy_with_progress` (and the libarchive write buffer) so
/// cross-backend progress cadence is consistent.
fn write_data_chunked(
    writer: &mut RawZipWriter<File>,
    path: &Path,
    data: &[u8],
    notify: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<()> {
    const CHUNK: usize = 64 * 1024;
    for chunk in data.chunks(CHUNK) {
        writer
            .write_all(chunk)
            .map_err(|e| ArchiveError::io("write", path, e))?;
        notify(chunk.len() as u64)?;
    }
    // Always emit a final 0-byte progress tick so callers observing
    // per-entry completion see the boundary even for empty inputs.
    if data.is_empty() {
        notify(0)?;
    }
    Ok(())
}

/// Stream `reader` into the current ZIP entry, enforcing that exactly
/// `expected_size` bytes are produced (R0075-0032, R0080-0038,
/// R0080-0039). The reader is wrapped in `Read::take(expected_size + 1)`
/// (saturating) so an over-producing source hits the cap instead of
/// being silently committed under a size that disagrees with the entry
/// header / ZIP64 planning.
///
/// A byte count that differs from `expected_size` surfaces as
/// [`ArchiveError::declared_length_mismatch`] — i.e.
/// `ArchiveError::Corruption` — because a source that hands the writer
/// a byte count other than the length it declared is a declared-size
/// violation, and DCR-011 already classifies those as `Corruption`.
/// The classification is shared with every other commit route through
/// that one constructor; before it existed this site raised `Format`
/// and the libarchive writer raised `Io` for the same condition, so no
/// caller could match on it. Detection is unchanged — only the variant
/// is.
///
/// `label` identifies the source in diagnostics (the archive path for
/// reader adds, the filesystem path for `add_file_from_path`) and
/// becomes the `path` field of the `Corruption` error.
fn stream_reader_exact<R: Read + ?Sized>(
    reader: &mut R,
    writer: &mut RawZipWriter<File>,
    write_error_path: &Path,
    label: &str,
    expected_size: u64,
    notify: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<()> {
    // `expected_size + 1` panics in debug / wraps in release at
    // `u64::MAX`; saturate so the overrun probe still bounds the read.
    let cap = expected_size.saturating_add(1);
    let mut bounded = std::io::Read::take(reader, cap);
    let written =
        super::common::copy_with_progress(&mut bounded, writer, write_error_path, notify)?;
    if written != expected_size {
        return Err(ArchiveError::declared_length_mismatch(
            label,
            expected_size,
            written,
        ));
    }
    Ok(())
}

/// Native Rust ZIP archive writer
///
/// Provides reliable ZIP creation using the zip crate, avoiding
/// libarchive's known issues with central directory structure.
///
/// **Poisoning (R0069-0053):** if any entry-write returns an error
/// after the underlying `start_file` succeeded, the writer is marked
/// poisoned. Subsequent `add_*` calls will short-circuit with an
/// `OperationBlocked` error so a partially-written archive cannot be
/// used to append more entries on top of a half-emitted one. `finish`
/// / `close` still drain to a well-formed file containing the entries
/// successfully written before the error.
///
/// **Terminal finalize failure (R0001-0018):** `finish` consumes the raw
/// writer before asking it to emit the central directory, so a failed
/// finalize leaves nothing for a retry to act on. The failure is recorded
/// and replayed by every later `finish` call instead of letting the
/// already-closed path report success for an archive that was never
/// durably written.
pub struct ZipWriter {
    writer: Option<RawZipWriter<File>>,
    path: PathBuf,
    options: SimpleFileOptions,
    /// Compression method/level mirrored as plain fields so the
    /// metadata-aware paths can construct a fresh `FullFileOptions`
    /// (which carries `add_extra_data`) without losing the writer's
    /// configured compression settings (OI-0065-002).
    compression_method: CompressionMethod,
    compression_level: Option<i64>,
    progress: Option<Box<dyn ProgressCallback>>,
    bytes_written: u64,
    entries_written: usize,
    poisoned: bool,
    /// R0001-0018: rendered reason of a finalize that already failed.
    /// `ArchiveError` is not `Clone`, so the message is kept and replayed
    /// as a typed `OperationBlocked` from every subsequent `finish`.
    finish_failure: Option<String>,
}

impl ZipWriter {
    /// Create a new ZIP archive for writing
    pub fn create(
        path: impl AsRef<Path>,
        compression_options: &mut CompressionOptions,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        // This library does not produce encrypted archives (AD 0007 amended by MADR-0027).
        // Reject passwords loudly instead of silently writing a plaintext archive.
        if compression_options.password.is_some() {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::CREATE,
                "Encrypted ZIP creation is not supported by this library. \
                 Use open_encrypted() to read existing encrypted archives.",
            ));
        }

        // `create_new(true)` is `O_CREAT|O_EXCL` — atomically reject any
        // existing destination instead of truncating it (R0069-0050).
        // Closes the TOCTOU race between the facade's `path.exists()`
        // pre-check and this `open` call.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path_buf)
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

        let compression_level = deflate_level.map(|l| l as i64);
        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .compression_level(compression_level);

        Ok(Self {
            writer: Some(writer),
            path: path_buf,
            options,
            compression_method: compression,
            compression_level,
            progress: compression_options.progress.take(),
            bytes_written: 0,
            entries_written: 0,
            poisoned: false,
            finish_failure: None,
        })
    }

    /// Build a `FullFileOptions` carrying the writer's configured
    /// compression method/level. Used by the metadata-aware add paths
    /// when they need to attach an extra-field block (e.g.,
    /// 0x5455 Universal Time for atime/btime preservation per
    /// OI-0065-002).
    fn fresh_full_options(&self) -> FullFileOptions<'static> {
        FullFileOptions::default()
            .compression_method(self.compression_method)
            .compression_level(self.compression_level)
    }

    /// If a prior entry-write left the writer in an undefined state, refuse
    /// further writes with `OperationBlocked` instead of producing a
    /// structurally-confusing archive (R0069-0053). `finish`/`close` still
    /// run to drain whatever entries were successfully written before the
    /// poison condition was raised.
    fn assert_not_poisoned(&self, op: &'static str) -> Result<()> {
        if self.poisoned {
            return Err(ArchiveError::operation_blocked(
                op,
                "ZIP writer is poisoned after a prior entry-write failure; \
                 call finish() to drain or drop the writer to discard",
            ));
        }
        Ok(())
    }

    /// Assert not poisoned, run `f`, and mark poisoned on error so a
    /// subsequent `add_*` call short-circuits instead of writing on top
    /// of a half-emitted entry. Used by every public `add_*` method to
    /// keep the poisoning discipline uniform (R0069-0053).
    ///
    /// `f` receives `(writer, path, options, notify)` borrowed directly
    /// so the closure body doesn't have to clone `path` defensively for
    /// the borrow checker. Closures must call `notify(chunk)` for every
    /// chunk of payload they emit (or `notify(0)` for zero-byte
    /// entries) so progress / cancellation fire mid-entry rather than
    /// only after the whole entry is on disk (R0071-0007). After `f`
    /// returns, `with_writer` increments `entries_written`; the byte
    /// accounting and per-chunk callback are entirely the closure's
    /// responsibility.
    fn with_writer(
        &mut self,
        op: &'static str,
        f: impl FnOnce(
            &mut RawZipWriter<File>,
            &Path,
            SimpleFileOptions,
            &mut dyn FnMut(u64) -> Result<()>,
        ) -> Result<()>,
    ) -> Result<()> {
        self.assert_not_poisoned(op)?;
        let options = self.options;
        // `&Path` view: `f` can pass it into `ArchiveError::io` lazily so
        // the `PathBuf::clone` only happens on the error branch.
        let path: &Path = self.path.as_path();
        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;
        // Borrow split: `writer` only borrows `self.writer`, leaving
        // `self.progress` and `self.bytes_written` available for the
        // notify closure.
        let progress_ref = &mut self.progress;
        let bytes_written_ref = &mut self.bytes_written;
        let mut notify = |additional: u64| -> Result<()> {
            super::common::notify_creation_progress(progress_ref, bytes_written_ref, additional)
        };
        match f(writer, path, options, &mut notify) {
            Ok(()) => {
                self.entries_written += 1;
                Ok(())
            }
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of entries written so far
    pub fn entries_written(&self) -> usize {
        self.entries_written
    }

    /// Add a file from byte data.
    ///
    /// Entries of more than 4 GiB are written in ZIP64 form; smaller
    /// ones stay classic for compatibility (see [`needs_zip64`]).
    ///
    /// R0075-0041: write the buffer in 64 KiB chunks so the
    /// progress/cancellation hook can fire mid-entry. The previous
    /// implementation called `write_all(data)` and only invoked
    /// `notify` once at the end, so a `ControlFlow::Break` could not
    /// interrupt a multi-gigabyte add until the entire buffer had
    /// been compressed. See [`write_data_chunked`].
    pub fn add_file_from_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        self.with_writer("add_file_from_data", |writer, path, options, notify| {
            let options = options.large_file(needs_zip64(Some(data.len() as u64)));
            writer.start_file(archive_path, options).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
            })?;
            write_data_chunked(writer, path, data, notify)
        })
    }

    /// Add a file from byte data, preserving metadata from an existing `ArchiveEntry`.
    ///
    /// Entries of more than 4 GiB are written in ZIP64 form; smaller
    /// ones stay classic for compatibility (see [`needs_zip64`]).
    ///
    /// Used by `commit_changes()` to round-trip entries without losing timestamps
    /// and permissions.
    ///
    /// R0076-0015: chunk-writes through [`write_data_chunked`] like the
    /// plain variant, so progress/cancellation can interrupt mid-entry
    /// instead of only after the whole payload is compressed.
    pub fn add_file_from_data_with_metadata(
        &mut self,
        archive_path: &str,
        data: &[u8],
        metadata: &crate::entry::ArchiveEntry,
    ) -> Result<()> {
        let mut full = self.fresh_full_options();
        apply_metadata_to_full_options(&mut full, metadata)?;
        full = full.large_file(needs_zip64(Some(data.len() as u64)));
        self.with_writer(
            "add_file_from_data_with_metadata",
            |writer, path, _options, notify| {
                writer.start_file(archive_path, full).map_err(|e| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
                })?;
                write_data_chunked(writer, path, data, notify)
            },
        )
    }

    /// Stream a file's contents from a reader without preserving source metadata.
    ///
    /// The payload size is unknown up front, so the entry is always
    /// written in ZIP64 form — otherwise a reader crossing the 4 GiB
    /// threshold would abort mid-write and poison the writer (see
    /// [`needs_zip64`]).
    ///
    /// Used by `commit_changes()` on the non-preserving path to round-trip
    /// retained entries without buffering them fully in memory. Applies only
    /// the writer's default `FileOptions`.
    pub fn add_file_from_reader<R: Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
    ) -> Result<()> {
        self.with_writer("add_file_from_reader", |writer, path, options, notify| {
            let options = options.large_file(needs_zip64(None));
            writer.start_file(archive_path, options).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
            })?;
            super::common::copy_with_progress(reader, writer, path, notify).map(|_| ())
        })
    }

    /// Stream a file's contents from a reader with an expected size,
    /// rejecting under- or over-production loudly (R0075-0032).
    ///
    /// Entries declared larger than 4 GiB are written in ZIP64 form;
    /// smaller ones stay classic for compatibility (see [`needs_zip64`]).
    ///
    /// The reader is wrapped in `Read::take(expected_size + 1)` so an
    /// over-producing source yields a clean EOF at the cap instead of
    /// being silently committed under the wrong size. After streaming,
    /// the actual byte count is compared against `expected_size`; any
    /// mismatch (over- or under-production) surfaces as
    /// [`ArchiveError::Corruption`] via
    /// [`ArchiveError::declared_length_mismatch`], the one constructor
    /// every commit route shares for a declared-size violation
    /// (DCR-011).
    pub fn add_file_from_reader_with_size<R: Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
        expected_size: u64,
    ) -> Result<()> {
        self.with_writer(
            "add_file_from_reader_with_size",
            |writer, path, options, notify| {
                let options = options.large_file(needs_zip64(Some(expected_size)));
                writer.start_file(archive_path, options).map_err(|e| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
                })?;
                stream_reader_exact(reader, writer, path, archive_path, expected_size, notify)
            },
        )
    }

    /// Stream a file's contents from a reader, preserving metadata. An explicit
    /// `compression_override` pins the central-directory compression method so
    /// mixed Stored/Deflated archives round-trip unchanged.
    ///
    /// The source entry's declared size drives the ZIP64 decision: a
    /// size above 4 GiB — or no declared size at all — opts the entry
    /// into ZIP64 form (see [`needs_zip64`]).
    pub fn add_file_from_reader_with_metadata<R: Read>(
        &mut self,
        archive_path: &str,
        reader: &mut R,
        metadata: &crate::entry::ArchiveEntry,
        compression_override: Option<CompressionMethod>,
    ) -> Result<()> {
        let mut full = self.fresh_full_options();
        apply_metadata_to_full_options(&mut full, metadata)?;
        full = full.large_file(needs_zip64(metadata.size));
        if let Some(method) = compression_override {
            full = full.compression_method(method);
            if method == CompressionMethod::Stored {
                full = full.compression_level(None);
            }
        }
        let expected_size = metadata.size;
        self.with_writer(
            "add_file_from_reader_with_metadata",
            |writer, path, _options, notify| {
                writer.start_file(archive_path, full).map_err(|e| {
                    ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
                })?;
                // R0080-0038: when the source declares a size, enforce it
                // exactly — `metadata.size` drives the ZIP64 decision, so a
                // stale or changed retained source whose length disagrees
                // must fail loudly (as `Corruption`, see
                // `stream_reader_exact`) instead of committing content of
                // the wrong size. An unknown size opts the entry into ZIP64
                // (see `needs_zip64`) and streams unbounded like
                // `add_file_from_reader`.
                match expected_size {
                    Some(expected) => {
                        stream_reader_exact(reader, writer, path, archive_path, expected, notify)
                    }
                    None => {
                        super::common::copy_with_progress(reader, writer, path, notify).map(|_| ())
                    }
                }
            },
        )
    }

    /// Set the archive-level (EOCD) comment. Pass an empty slice to clear.
    /// Honors the same poisoning gate as the entry-write methods so a
    /// comment cannot be applied to a writer in an undefined state.
    ///
    /// R0001-0033: comments longer than [`crate::security::MAX_COMMENT_SIZE`]
    /// are rejected. The EOCD record stores the comment length in a `u16`, so
    /// a longer comment would be serialized under a truncated length with the
    /// remaining bytes trailing the record — leaving readers to disagree about
    /// where the archive ends.
    pub fn set_archive_comment(&mut self, comment: &[u8]) -> Result<()> {
        self.assert_not_poisoned("set_archive_comment")?;
        // Reject before touching writer state so an oversized comment leaves
        // the archive exactly as it was.
        if comment.len() > crate::security::MAX_COMMENT_SIZE as usize {
            return Err(ArchiveError::operation_blocked(
                "set_archive_comment",
                format!(
                    "Archive comment of {} bytes exceeds the ZIP end-of-central-directory limit of {} bytes",
                    comment.len(),
                    crate::security::MAX_COMMENT_SIZE
                ),
            ));
        }
        let writer = self.writer.as_mut().ok_or_else(|| {
            ArchiveError::format(Some(ArchiveFormat::Zip), "Archive already closed")
        })?;
        writer.set_raw_comment(comment.to_vec().into_boxed_slice());
        Ok(())
    }

    /// Add a file from filesystem path with custom archive path
    ///
    /// Files of more than 4 GiB are written in ZIP64 form; smaller
    /// ones stay classic for compatibility (see [`needs_zip64`]).
    ///
    /// Preserves the source file's modification time and Unix permissions.
    /// Symlinks are rejected up-front (R0069-0054 / MADR-0021) so the
    /// single-file path matches `add_directory_recursive`'s behaviour
    /// instead of silently following the link and archiving the target's
    /// bytes under the link's name.
    pub fn add_file_from_path(
        &mut self,
        fs_path: impl AsRef<Path>,
        archive_path: &str,
    ) -> Result<()> {
        let fs_path = fs_path.as_ref();
        super::common::reject_symlink_path(fs_path, "add_file_from_path")?;

        let mut file =
            File::open(fs_path).map_err(|e| ArchiveError::io("open", fs_path.to_path_buf(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("metadata", fs_path.to_path_buf(), e))?;

        let expected_len = metadata.len();
        self.with_writer("add_file_from_path", |writer, path, options, notify| {
            let mut file_options = options.large_file(needs_zip64(Some(expected_len)));
            if let Ok(mtime) = metadata.modified() {
                if let Some(dt) = super::common::system_time_to_zip_datetime(mtime) {
                    file_options = file_options.last_modified_time(dt);
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file_options = file_options.unix_permissions(metadata.permissions().mode());
            }
            writer.start_file(archive_path, file_options).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Start file: {}", e))
            })?;
            // R0080-0039: the file was stat'd once for the ZIP64/size
            // decision; bound the copy to that length and require exact
            // equality so concurrent growth or truncation between the stat
            // and the stream fails loudly rather than silently archiving a
            // different byte count than the header and central directory
            // were planned for. The stat'd length is the declared size
            // here, so the mismatch is classified like every other
            // declared-size violation: `Corruption` (DCR-011).
            let label = fs_path.to_string_lossy();
            stream_reader_exact(&mut file, writer, path, &label, expected_len, notify)
        })
    }

    /// Add a single directory entry (without contents)
    pub fn add_directory_entry(&mut self, archive_path: &str) -> Result<()> {
        self.with_writer("add_directory_entry", |writer, _path, options, notify| {
            let dir_path = super::common::ensure_trailing_slash(archive_path);
            writer.add_directory(&dir_path, options).map_err(|e| {
                ArchiveError::format(Some(ArchiveFormat::Zip), format!("Add directory: {}", e))
            })?;
            // Directory entries contribute zero payload bytes but must
            // still notify so per-entry callbacks fire (matches the
            // file-add cadence).
            notify(0)
        })
    }

    /// Add a directory recursively.
    ///
    /// Entries are written under the source directory's **own name** so the
    /// resulting archive faithfully describes the input tree
    /// (`add_directory_recursive("foo/bar")` produces `bar/<...>` entries,
    /// not bare `<...>`). This matches the libarchive backend's recursive
    /// behavior and the convention used by `tar`, `zip -r`, and `7z a -r`,
    /// keeping cross-backend round-trips lossless.
    pub fn add_directory_recursive(&mut self, dir_path: impl AsRef<Path>) -> Result<()> {
        use super::common::{DirWalkKind, walk_directory_tree};

        let dir_path = dir_path.as_ref();

        let mut emitted_any = false;
        walk_directory_tree(dir_path, "walk", |item| {
            // Skip the source directory itself — its name survives via
            // the children's relative paths under the parent-rooted
            // prefix. Libarchive's recursive add does the same so the
            // two backends emit equivalent layouts.
            if item.is_root {
                return Ok(());
            }
            let archive_path = super::common::normalize_path(&item.archive_path);
            match item.kind {
                DirWalkKind::File => {
                    self.add_file_from_path(item.fs_path, &archive_path)?;
                    emitted_any = true;
                }
                DirWalkKind::Dir => {
                    // R0070-0046: emit only leaf empty directories so the ZIP
                    // backend matches libarchive — non-empty directories are
                    // implied by their children's archive paths.
                    if item.is_leaf_dir {
                        self.add_directory_entry(&archive_path)?;
                        emitted_any = true;
                    }
                }
                // MADR-0021: symlinks are rejected at creation time to avoid silent data loss
                // and to keep creation-side link policy aligned with extraction rejection.
                DirWalkKind::Special { is_symlink: true } => {
                    return Err(ArchiveError::operation_blocked(
                        "add_directory_recursive",
                        format!(
                            "Refusing to archive symlink '{}': symlinks are not supported for ZIP creation",
                            item.fs_path.display()
                        ),
                    ));
                }
                DirWalkKind::Special { .. } => {
                    return Err(ArchiveError::operation_blocked(
                        "add_directory_recursive",
                        format!(
                            "Refusing to archive '{}': unsupported file type",
                            item.fs_path.display()
                        ),
                    ));
                }
            }
            Ok(())
        })?;

        // R0070-0047: an empty source directory should still produce a
        // single entry so the recursive-add round-trip is non-trivial
        // ("here is an empty directory") instead of silently emitting
        // zero entries. Emit a single directory entry naming the
        // source root.
        if !emitted_any {
            let root_name = super::common::normalize_path(
                &dir_path.file_name().unwrap_or_default().to_string_lossy(),
            );
            if !root_name.is_empty() {
                self.add_directory_entry(&root_name)?;
            }
        }

        Ok(())
    }

    /// Finish writing and close the archive.
    ///
    /// R0076-0017: ZIP creation writes directly to the final path
    /// (unlike extraction/modify, which stage through
    /// `AtomicOutputFile` and fsync at commit), so this is the
    /// durability boundary. Once the central directory is written we
    /// `sync_data()` the file (fatal on failure) and best-effort fsync
    /// the parent directory, so a crash immediately after `finish()`
    /// returns cannot leave a torn archive — matching the
    /// crash-consistency the extract and modify paths already provide.
    ///
    /// R0001-0018: finalization is one-shot. The raw writer is consumed
    /// before the central directory is emitted, so a failure cannot be
    /// retried — without a record of it, a second call would fall through
    /// the already-taken (`writer == None`) path and report `Ok(())` for an
    /// archive that was never durably written. The first failure returns its
    /// precise typed error; every later attempt replays it as an
    /// `OperationBlocked` naming the recorded reason. Repeated calls after a
    /// *successful* finish stay `Ok(())`, keeping the `Drop`-finalize
    /// idempotency from OI-0076-003 item 3 intact.
    pub fn finish(&mut self) -> Result<()> {
        if let Some(reason) = &self.finish_failure {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::FINISH,
                format!("ZIP finalization already failed and cannot be retried: {reason}"),
            ));
        }
        let Some(writer) = self.writer.take() else {
            return Ok(());
        };
        let result = self.finalize_writer(writer);
        if let Err(e) = &result {
            // Render now: `ArchiveError` is not `Clone`, and the caller
            // still receives the original typed error below.
            self.finish_failure = Some(e.to_string());
        }
        result
    }

    /// Emit the central directory for an already-taken raw writer and make
    /// the result durable. Split out of [`ZipWriter::finish`] so the sticky
    /// failure bookkeeping (R0001-0018) has a single place to observe.
    fn finalize_writer(&self, writer: RawZipWriter<File>) -> Result<()> {
        let file = writer.finish().map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Finish: {}", e))
        })?;
        file.sync_data()
            .map_err(|e| ArchiveError::io(crate::error::ops::CREATE, self.path.clone(), e))?;
        // R0079-0038: parent-directory sync is best-effort, matching
        // the settled policy at every other parent-sync site
        // (R0075-0037 / R0076-0013). The archive itself is already
        // durable via sync_data(); the directory-entry sync is a
        // durability hint, not a correctness gate, and opening the
        // parent can fail for reasons unrelated to archive
        // integrity (e.g. a write+execute-only directory on Unix).
        let _ = super::common::sync_parent_dir(&self.path);
        Ok(())
    }
}

impl Drop for ZipWriter {
    fn drop(&mut self) {
        // R0001-0018: a finalize failure that already surfaced to the caller
        // is recorded and replayed by `finish`; don't re-report it here as
        // though it were a fresh drop-time failure.
        if self.finish_failure.is_some() {
            return;
        }
        // Try to finish properly on drop. R0076-0018: `Drop` cannot
        // return errors, so a failed finalize is warned about instead of
        // silently swallowed (same pattern as `WriteArchive`'s drop in
        // `src/archive/mode_split.rs`). Kept panic-free.
        if let Err(e) = self.finish() {
            eprintln!(
                "unified-archive: ZipWriter for `{}` failed to finalize on drop: {e}. The \
                 archive may be incomplete; call finish() to surface this error explicitly.",
                self.path.display()
            );
        }
    }
}

/// Apply mtime / Unix permissions / Universal Time extra-field to a
/// `FullFileOptions` from the source `ArchiveEntry`. The Universal Time
/// extra (header id `0x5455`) carries `mtime`, `atime`, and `ctime`
/// (creation, in PKZIP wording) as 32-bit Unix timestamps so a
/// `commit_changes()` round-trip with `preserve_metadata=true` no
/// longer drops `created` / `accessed` (OI-0065-002).
fn apply_metadata_to_full_options(
    options: &mut FullFileOptions<'static>,
    metadata: &crate::entry::ArchiveEntry,
) -> Result<()> {
    if let Some(mtime) = metadata.modified {
        if let Some(dt) = super::common::system_time_to_zip_datetime(mtime) {
            *options = std::mem::take(options).last_modified_time(dt);
        }
    }
    #[cfg(unix)]
    if let Some(perm) = metadata.permissions {
        *options = std::mem::take(options).unix_permissions(perm);
    }
    if let Some(extra) =
        encode_universal_time_extra(metadata.modified, metadata.accessed, metadata.created)
    {
        // The 0x5455 block lives in both the local header and the
        // central directory record; set `central_only=false` so the
        // local copy is emitted for readers that only consult that.
        options.add_extra_data(0x5455, extra, false).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("attach Universal Time extra: {e}"),
            )
        })?;
    }
    Ok(())
}

/// Build a 0x5455 ("Universal Time" / Extended Timestamp) extra-field
/// payload from optional UNIX-epoch timestamps. Layout (libzip
/// specifications/extrafld.txt):
///
/// ```text
///   1 byte  flags  (bit 0 = mtime, bit 1 = atime, bit 2 = ctime)
///   4 byte  mtime  (if flag 0)
///   4 byte  atime  (if flag 1)
///   4 byte  ctime  (if flag 2)
/// ```
///
/// All times are little-endian signed 32-bit Unix seconds. Returns
/// `None` if no fields are present so the caller can skip the block
/// entirely (a zero-length 0x5455 record is technically legal but
/// adds bytes for no benefit). Any timestamp before 1970 or beyond
/// 2038 is silently dropped — that is the wire-level format limit.
fn encode_universal_time_extra(
    mtime: Option<std::time::SystemTime>,
    atime: Option<std::time::SystemTime>,
    ctime: Option<std::time::SystemTime>,
) -> Option<Box<[u8]>> {
    let to_secs = |t: Option<std::time::SystemTime>| -> Option<i32> {
        let t = t?;
        let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        if secs <= i32::MAX as u64 {
            Some(secs as i32)
        } else {
            None
        }
    };
    let m = to_secs(mtime);
    let a = to_secs(atime);
    let c = to_secs(ctime);
    if m.is_none() && a.is_none() && c.is_none() {
        return None;
    }
    let mut flags: u8 = 0;
    if m.is_some() {
        flags |= 0b0000_0001;
    }
    if a.is_some() {
        flags |= 0b0000_0010;
    }
    if c.is_some() {
        flags |= 0b0000_0100;
    }
    let mut buf = Vec::with_capacity(1 + 4 * flags.count_ones() as usize);
    buf.push(flags);
    for v in [m, a, c].into_iter().flatten() {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    Some(buf.into_boxed_slice())
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

    /// R0079-0027: ZIP64 policy — unknown sizes always opt in; known
    /// sizes only at/above the 4 GiB classic-field limit.
    #[test]
    fn test_needs_zip64_thresholds() {
        assert!(needs_zip64(None), "unknown size must opt into ZIP64");
        assert!(!needs_zip64(Some(0)));
        assert!(
            !needs_zip64(Some(zip::ZIP64_BYTES_THR - 1)),
            "one byte below the classic-field limit stays non-ZIP64"
        );
        assert!(
            needs_zip64(Some(zip::ZIP64_BYTES_THR)),
            "the classic-field sentinel value requires ZIP64"
        );
        assert!(needs_zip64(Some(zip::ZIP64_BYTES_THR + 1)));
    }

    /// R0079-0027: reader-based adds have no size hint, so their entries
    /// carry the ZIP64 local-header reservation; verify they still
    /// round-trip through the zip crate's reader.
    #[test]
    fn test_zip_writer_reader_add_roundtrips_with_zip64() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_zip64_reader.zip");

        let content: &[u8] = b"streamed content";
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer
            .add_file_from_reader("streamed.txt", &mut &content[..])
            .unwrap();
        writer.finish().unwrap();

        let file = File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name("streamed.txt").unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
        assert_eq!(buf, content);
    }

    /// R0079-0038: a parent directory that cannot be opened for reading
    /// (write+execute only) breaks `sync_parent_dir`, but the archive is
    /// already durable via `sync_data()`, so `finish()` must succeed.
    #[cfg(unix)]
    #[test]
    fn test_zip_writer_finish_succeeds_when_parent_dir_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("wx_only");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("out.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("a.txt", b"payload").unwrap();

        // Write+execute, no read: the archive file stays writable but
        // File::open(parent) for the directory fsync fails with EACCES.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
        let result = writer.finish();
        // Restore before asserting so TempDir cleanup can list the dir.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        result.expect("best-effort parent fsync must not fail finish()");
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

    /// R0080-0038: a metadata-preserving reader add must enforce the
    /// declared size (which also drives ZIP64 planning); a source whose
    /// byte count disagrees fails loudly instead of committing content of
    /// the wrong length.
    #[test]
    fn test_reader_with_metadata_enforces_declared_size() {
        let tmp = tempfile::tempdir().unwrap();

        // Under-production: declares 5 bytes, yields 3.
        let path_under = tmp.path().join("under.zip");
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path_under, &mut opts).unwrap();
        let meta = crate::entry::ArchiveEntry::file("short.txt", 0)
            .size(5)
            .build();
        let err = writer
            .add_file_from_reader_with_metadata("short.txt", &mut &b"abc"[..], &meta, None)
            .unwrap_err();
        assert!(
            matches!(err, ArchiveError::Corruption { .. }),
            "a declared-size violation must be Corruption (DCR-011), got: {err:?}"
        );
        assert!(
            err.to_string().contains("under-produced"),
            "expected under-production error, got: {err}"
        );
        assert!(
            err.to_string().contains("short.txt"),
            "the error must name the offending entry, got: {err}"
        );

        // Over-production: declares 2 bytes, yields 5.
        let path_over = tmp.path().join("over.zip");
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path_over, &mut opts).unwrap();
        let meta = crate::entry::ArchiveEntry::file("long.txt", 0)
            .size(2)
            .build();
        let err = writer
            .add_file_from_reader_with_metadata("long.txt", &mut &b"abcde"[..], &meta, None)
            .unwrap_err();
        assert!(
            matches!(err, ArchiveError::Corruption { .. }),
            "a declared-size violation must be Corruption (DCR-011), got: {err:?}"
        );
        assert!(
            err.to_string().contains("over-produced"),
            "expected over-production error, got: {err}"
        );
    }

    /// The declared-size gate on the plain (non-metadata) reader route
    /// classifies the same way: `add_file_from_reader_with_size` is the
    /// method `commit_changes` uses for a `size: Some(n)` reader source,
    /// and `add_entry_from_reader`'s rustdoc promises `Corruption` for a
    /// length mismatch (DCR-011).
    #[test]
    fn test_reader_with_size_length_mismatch_is_corruption() {
        let tmp = tempfile::tempdir().unwrap();

        let path_under = tmp.path().join("under.zip");
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path_under, &mut opts).unwrap();
        let err = writer
            .add_file_from_reader_with_size("short.txt", &mut &b"abc"[..], 5)
            .unwrap_err();
        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "short.txt");
                assert!(details.contains("under-produced"), "{details}");
                assert!(details.contains('5') && details.contains('3'), "{details}");
            }
            other => panic!("expected Corruption, got: {other:?}"),
        }

        let path_over = tmp.path().join("over.zip");
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path_over, &mut opts).unwrap();
        let err = writer
            .add_file_from_reader_with_size("long.txt", &mut &b"abcde"[..], 2)
            .unwrap_err();
        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "long.txt");
                assert!(details.contains("over-produced"), "{details}");
            }
            other => panic!("expected Corruption, got: {other:?}"),
        }
    }

    /// R0080-0038: an unknown declared size still streams the whole
    /// reader unbounded (ZIP64 opt-in), so a metadata add without a size
    /// round-trips its full payload.
    #[test]
    fn test_reader_with_metadata_unknown_size_streams_unbounded() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("unknown.zip");
        let content: &[u8] = b"payload of unknown declared length";

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        // `ArchiveEntry::file` leaves `size` as `None` unless `.size()` is
        // called, so this exercises the unbounded branch.
        let meta = crate::entry::ArchiveEntry::file("blob.bin", 0).build();
        writer
            .add_file_from_reader_with_metadata("blob.bin", &mut &content[..], &meta, None)
            .unwrap();
        writer.finish().unwrap();

        let file = File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name("blob.bin").unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
        assert_eq!(buf, content);
    }

    /// R0001-0033 / R0001-0065: the EOCD stores the comment length in a
    /// `u16`, so `MAX_COMMENT_SIZE` bytes is the largest comment that can be
    /// serialized honestly; one more byte is refused without disturbing the
    /// writer, and the exact field width still round-trips.
    #[test]
    fn test_set_archive_comment_bounded_to_eocd_field() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_comment_bound.zip");
        let limit = crate::security::MAX_COMMENT_SIZE as usize;

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("a.txt", b"payload").unwrap();

        let oversized = vec![b'x'; limit + 1];
        let err = writer.set_archive_comment(&oversized).unwrap_err();
        assert!(
            err.to_string()
                .contains("exceeds the ZIP end-of-central-directory limit"),
            "expected an oversized-comment rejection, got: {err}"
        );

        // The rejection must not have left the writer unusable.
        let at_limit = vec![b'x'; limit];
        writer.set_archive_comment(&at_limit).unwrap();
        writer.finish().unwrap();

        let file = File::open(&path).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        assert_eq!(zip.comment().len(), limit);
    }

    /// R0001-0018: repeated `finish` calls after a *successful* finalize stay
    /// `Ok(())` — the `Drop`-finalize idempotency from OI-0076-003 item 3.
    #[test]
    fn test_finish_is_idempotent_after_success() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_finish_idempotent.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("a.txt", b"payload").unwrap();
        writer.finish().unwrap();
        writer
            .finish()
            .expect("a second finish after success is a no-op");
    }

    /// R0001-0018: a finalize failure is terminal. The raw writer is consumed
    /// before `writer.finish()` runs, so the retry must replay the recorded
    /// failure instead of taking the already-closed path and reporting
    /// success for an archive that was never durably written. The failed
    /// state is installed directly here because the underlying
    /// `RawZipWriter::finish` / `sync_data` errors cannot be provoked
    /// portably from a unit test.
    #[test]
    fn test_finish_replays_recorded_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test_finish_sticky.zip");

        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        let mut writer = ZipWriter::create(&path, &mut opts).unwrap();
        writer.add_file_from_data("a.txt", b"payload").unwrap();

        // Exactly the state a failed `finish` leaves behind: writer consumed,
        // reason recorded.
        writer.writer = None;
        writer.finish_failure = Some("Finish: simulated write failure".to_string());

        for attempt in 0..2 {
            let err = writer
                .finish()
                .expect_err("a failed finalize must never report success");
            assert!(
                err.to_string().contains("simulated write failure"),
                "attempt {attempt} must replay the recorded failure, got: {err}"
            );
        }
    }
}
