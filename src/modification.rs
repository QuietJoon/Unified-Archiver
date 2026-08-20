//! Archive modification operations
//!
//! This module provides methods for modifying existing archives by adding, removing,
//! or replacing entries using a copy-on-write strategy.

use crate::archive::{Archive, ArchiveBackend, ArchiveMode};
use crate::entry::ArchiveEntry;
use crate::error::ops;
use crate::error::{
    ArchiveError, ArchiveWarning, Result, ResultWithWarnings, UnsupportedEntryKind,
};
use crate::format::ArchiveFormat;
use fs4::FileExt;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Configuration options for archive modification operations.
///
/// Does not implement `Clone` because `compression: Option<CompressionOptions>`
/// embeds a non-cloneable progress callback (`Box<dyn ProgressCallback>`).
/// Callers that need to hand these options to multiple operations should
/// build a fresh value or call `CompressionOptions::strip_progress` explicitly.
#[derive(Debug)]
pub struct ModificationOptions {
    /// Whether to preserve original file metadata on retained entries.
    /// When `true`, `commit_changes()` re-emits `modified`, `accessed`,
    /// `created` (where the source listing exposes them) and Unix
    /// permissions on backends that support them. The libarchive
    /// writer uses `archive_entry_set_atime` / `archive_entry_set_birthtime`;
    /// the ZIP writer attaches a 0x5455 ("Universal Time") extra-field
    /// block so atime/btime survive the rewrite (OI-0065-002).
    ///
    /// Applies to **regular file entries only**. Retained directory
    /// entries are re-emitted with backend-default permissions
    /// (`0o755` on libarchive) and the rewrite's wall-clock time
    /// regardless of this flag (R0071-0021); directory-metadata
    /// preservation needs metadata-aware directory-add helpers in
    /// both writers and is still tracked separately.
    pub preserve_metadata: bool,

    /// Whether to create a backup of the original archive
    pub create_backup: bool,

    /// Suffix for backup files
    pub backup_suffix: String,

    /// Compression settings for the recreated archive.
    /// When `None`, uses format defaults (`CompressionLevel::Normal`, no password).
    ///
    /// `level` and `progress` participate in modify mode (R0073-0013).
    /// `password` is rejected by the create facade per MADR-0027, and
    /// `split_size` remains reserved/deferred (DEF-002). The rewrite must
    /// keep the source's container format — `commit_changes()` rejects an
    /// override whose `format` field disagrees with the archive's detected
    /// format rather than silently substituting either side, so a stray
    /// `CompressionOptions::new(ArchiveFormat::Tar)` cannot replace a ZIP
    /// container by accident (R0071-0002).
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

    /// Enable backup creation with specified suffix.
    ///
    /// `suffix` must be a sibling-file suffix (e.g. `.bak`, `.orig`).
    /// Path separators (`/`, `\`) are forbidden — the backup is always
    /// produced as `<archive_path><suffix>` next to the source. An empty
    /// suffix or one containing separators causes the call to ignore the
    /// supplied value and fall back to `.bak` (R0069-0068). In debug
    /// builds the misuse is also surfaced via `debug_assert!` so it is
    /// caught during development; release builds keep the silent
    /// fallback so a programmer mistake never escalates to a panic in
    /// the field.
    ///
    /// **Silent-repair caveat.** The fallback is
    /// deliberate but visible only in debug builds; if you need the
    /// invalid input to fail loudly in release builds too, validate
    /// the suffix yourself (`!suffix.is_empty() && !suffix.contains('/')
    /// && !suffix.contains('\\')`) before calling this builder. The
    /// `commit_changes` re-validation is the load-bearing check —
    /// `with_backup`'s additive-builder contract trades a strict
    /// constructor signature for that defence-in-depth.
    pub fn with_backup(mut self, suffix: &str) -> Self {
        let invalid = suffix.is_empty() || suffix.contains('/') || suffix.contains('\\');
        debug_assert!(
            !invalid,
            "with_backup: suffix {:?} is empty or contains a path separator; \
             release builds will silently fall back to `.bak`",
            suffix
        );
        self.create_backup = true;
        self.backup_suffix = if invalid {
            ".bak".to_string()
        } else {
            suffix.to_string()
        };
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

/// Source of bytes for a queued add (R0069-0058 / R0069-0059, AD 0062 A.4).
///
/// Replaces the prior `Vec<u8>`-only buffer so a 5 GB add no longer
/// allocates 5 GB up front. Each variant has distinct commit-time
/// semantics:
///
/// - **Snapshot (`Buffered`):** bytes captured at `add_entry` time.
///   Caller can mutate or drop the source slice immediately. Commit
///   is retryable indefinitely because the bytes are owned by the
///   tracker.
/// - **Live filesystem path (`Path`):** opened and streamed at
///   commit time. When `preserve_metadata = true` the writer's
///   path-based helper preserves source mtime / atime / btime /
///   Unix permissions automatically. Only the path is captured, not
///   the bytes: commit reopens the path and archives whatever it
///   names then (the live-path design, AD 0062 A.4). A same-path
///   replacement written before commit is therefore picked up
///   silently — the current contents are archived, not the contents
///   present at `add_entry_from_path` time. Commit fails with the
///   underlying I/O error only when the source is deleted or
///   rendered unreadable by commit time; other queued entries remain
///   unaffected.
/// - **Live reader (`Reader`):** consumed during commit. `size`:
///   `Some(n)` is trusted as the libarchive-header value;
///   length-mismatch surfaces as `ArchiveError::Corruption` at
///   commit time. `None` triggers tempfile staging via
///   `stage_unknown_size_entry` (already used for retained
///   streaming under OI-0069-002 R0069-0065).
///
///   **Retry is not possible.** [`Archive::commit_changes`]
///   takes `self` by value, so any failure during commit consumes the
///   archive handle along with the partially-drained reader. Plan
///   reader-source replacement *before* calling `commit_changes`:
///   construct a fresh `Archive::modify()` handle, queue a
///   replacement reader, and try again. The previous wording
///   suggesting `clear_operations` could be used after a failed
///   commit was incorrect — the handle is gone.
pub(crate) enum EntrySource {
    Buffered(Vec<u8>),
    Path(PathBuf),
    Reader {
        reader: Box<dyn Read + Send + 'static>,
        size: Option<u64>,
    },
}

/// Two-set namespace tracker shared between Modify mode's
/// `commit_changes` gate and Write mode's per-add gate (R0069-0052,
/// AD 0062 A.4).
///
/// `file_paths` collects normalised paths that will land as files;
/// `dir_paths` collects every directory path implied by the
/// namespace (explicit directory entries plus all proper ancestors
/// of file paths). The two sets must remain disjoint — an
/// intersection means the same archive-internal path is being
/// claimed as both a file and a directory, which is structurally
/// impossible.
#[derive(Default)]
pub(crate) struct NamespaceTracker {
    pub(crate) file_paths: HashSet<String>,
    pub(crate) dir_paths: HashSet<String>,
}

impl NamespaceTracker {
    /// Record a file-typed path. Errors on duplicate-or-conflict per
    /// the rules in [`record_file`].
    ///
    /// `op` labels the originating operation so the error message and
    /// `OperationBlocked.operation` field reference the caller's
    /// public method (e.g. `add_file_from_data`, `commit_changes`)
    /// instead of always reporting `commit_changes`.
    pub(crate) fn record_file(&mut self, op: &'static str, path: &str) -> Result<()> {
        record_file(op, &mut self.file_paths, &mut self.dir_paths, path)
    }

    /// Record a directory-typed path. Errors on
    /// directory-vs-existing-file conflict per the rules in
    /// [`record_dir`].
    pub(crate) fn record_dir(&mut self, op: &'static str, path: &str) -> Result<()> {
        record_dir(op, &mut self.file_paths, &mut self.dir_paths, path)
    }

    /// Whether `path` (after dup-check normalization) has already been
    /// recorded as a directory. Lets the create-side `add_directory`
    /// skip re-emitting a directory entry whose normalized path was
    /// already emitted (R0081-0038) without reaching into the private
    /// `normalize_dup_check_path` helper from another module.
    pub(crate) fn contains_dir(&self, path: &str) -> bool {
        self.dir_paths.contains(&normalize_dup_check_path(path))
    }
}

/// Tracks modifications to an archive in Modify mode
#[derive(Default)]
pub(crate) struct ModificationTracker {
    /// Entry IDs (positional indices matching `list_files()` order) to remove.
    /// Indexing by ID preserves identity across archives that legitimately
    /// contain duplicate entry paths (ZIP/7z central-directory records).
    pub(crate) removed: HashSet<usize>,
    /// Files to add to archive (path, source).
    /// `source` is a typed enum so commit time can branch over
    /// snapshot / live-path / live-reader without buffering the
    /// payload up-front (AD 0062 A.4).
    pub(crate) added: Vec<(String, EntrySource)>,
    /// Directories to add to archive
    pub(crate) added_directories: Vec<String>,
    /// Identity of the archive inode locked when `modify()` opened this
    /// handle, captured from the already-open advisory-lock descriptor.
    /// `None` on non-Unix (and before an identity is recorded). Revalidated
    /// against `self.path` before every pathname-based operation so a path
    /// whose inode changed under a non-cooperating writer is refused rather
    /// than operated on (MADR-0009 threat model; R0080-0005/0041-0046).
    pub(crate) locked_identity: Option<LockedFileIdentity>,
}

/// Identity of the archive file locked at [`Archive::modify`] open, captured
/// from the already-open advisory-lock descriptor.
///
/// Unix-only: identity is `(dev, ino)` read via
/// [`std::os::unix::fs::MetadataExt`]. On non-Unix the captured value is
/// `None` and revalidation is skipped — fs4's lock descriptor exposes no
/// portable identity here, and Windows handle-based identity
/// (`GetFileInformationByHandle` / `FILE_ID_INFO`) is a tracked follow-up.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LockedFileIdentity(crate::fs_identity::InodeId);

#[cfg(not(unix))]
#[expect(
    dead_code,
    reason = "non-Unix zero-sized identity marker; revalidation is Unix-only, so this variant is never constructed off-Unix"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LockedFileIdentity;

/// Capture the locked archive's identity from the already-open advisory-lock
/// descriptor. See [`LockedFileIdentity`]. Returns `Ok(None)` on non-Unix.
#[cfg(unix)]
fn capture_locked_identity(
    lock_file: &std::fs::File,
    path: &Path,
) -> Result<Option<LockedFileIdentity>> {
    let meta = lock_file
        .metadata()
        .map_err(|e| ArchiveError::io("modify-lock-identity", path, e))?;
    Ok(Some(LockedFileIdentity(
        crate::fs_identity::InodeId::from_metadata(&meta),
    )))
}

#[cfg(not(unix))]
fn capture_locked_identity(
    _lock_file: &std::fs::File,
    _path: &Path,
) -> Result<Option<LockedFileIdentity>> {
    Ok(None)
}

/// Revalidate that `path` still names the inode locked when
/// [`Archive::modify`] opened this handle, comparing a fresh `stat` of `path`
/// against the captured `(dev, ino)` identity.
///
/// Returns [`ArchiveError::OperationBlocked`] labelled `op` on mismatch, so a
/// pathname whose inode drifted — e.g. a non-cooperating process renamed the
/// archive aside and dropped a different file at the pathname — is refused
/// before any read / backup / permission-clone / atomic replace touches the
/// unrelated inode (MADR-0009 threat model; R0080-0005/0041-0046). A `None`
/// expected identity (non-Unix) skips the check.
#[cfg(unix)]
pub(crate) fn revalidate_locked_identity(
    op: &'static str,
    path: &Path,
    expected: Option<LockedFileIdentity>,
) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let meta = std::fs::metadata(path)
        .map_err(|e| ArchiveError::io("modify-identity-revalidate", path, e))?;
    let found = LockedFileIdentity(crate::fs_identity::InodeId::from_metadata(&meta));
    if found != expected {
        return Err(ArchiveError::operation_blocked(
            op,
            format!(
                "archive path {} identity changed since modify() opened it \
                 (expected dev/ino {}/{}, found {}/{}); aborting to avoid \
                 touching an unrelated file",
                path.display(),
                expected.0.dev,
                expected.0.ino,
                found.0.dev,
                found.0.ino
            ),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn revalidate_locked_identity(
    _op: &'static str,
    _path: &Path,
    _expected: Option<LockedFileIdentity>,
) -> Result<()> {
    Ok(())
}

impl Archive {
    /// Open an archive for modification
    ///
    /// Opens an existing archive in Modify mode. Modifications are tracked in memory
    /// and applied when `commit_changes()` is called.
    ///
    /// # Concurrency and file identity
    ///
    /// `modify()` acquires an fs4 advisory exclusive lock on the archive
    /// descriptor and records that inode identity (Unix `(dev, ino)`). Every
    /// later pathname-based step — the encryption probe, the read backend,
    /// retained-entry replay, permission preservation, backups, and the
    /// atomic replace in `commit_changes()` — revalidates that the path still
    /// names the locked inode before touching it, aborting with
    /// `OperationBlocked` on drift. This shrinks but does **not** close the
    /// check-to-use window: a non-cooperating writer that ignores the
    /// advisory lock can still swap the pathname in the gap between a
    /// revalidation and the immediately following use. The lock is advisory
    /// by design (MADR-0009); identity revalidation narrows the residual race
    /// rather than eliminating it. On non-Unix the identity is not captured
    /// and revalidation is skipped (tracked follow-up).
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

        // Acquire the advisory exclusive lock *before* format detection,
        // capability check, encryption probe, or backend open
        // (MADR-0009, MADR-0016, R0069-0056, R0070-0003). Locking first
        // closes the TOCTOU window where a concurrent writer could swap
        // the file between detect/can_modify and the modify backend's
        // first read.
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path_buf)
            .map_err(|e| ArchiveError::io("modify-lock-open", &path_buf, e))?;
        // fs4's `try_lock` (exclusive, non-blocking) reports contention as
        // `Err(TryLockError::WouldBlock)` and I/O failure as `Err(_)` too —
        // the same "any non-acquisition is an error" shape as the fs2 call
        // it replaced, so every failure keeps mapping to `OperationBlocked`.
        FileExt::try_lock(&lock_file).map_err(|_| {
            ArchiveError::operation_blocked(
                ops::MODIFY,
                format!(
                    "Another Modify session holds the advisory lock on {}",
                    path_buf.display()
                ),
            )
        })?;

        // Capture the locked inode's identity from the already-open lock
        // descriptor. Every later pathname-based operation revalidates
        // `self.path` against this so a non-cooperating writer that renames
        // the archive aside and drops a different file at the pathname is
        // caught instead of silently operated on (R0080-0005/0041-0046).
        let locked_identity = capture_locked_identity(&lock_file, &path_buf)?;

        // Detect format from the *locked* handle. Reading magic from a
        // separate `ArchiveFormat::detect` call would re-open the file
        // and reintroduce the TOCTOU window we just closed.
        let format = detect_format_from_locked(&lock_file, &path_buf)?;

        // Check if format supports modification
        if !format.can_modify() {
            return Err(ArchiveError::operation_blocked(
                ops::MODIFY,
                format!("{:?} archives do not support modification", format),
            ));
        }

        // Reject encrypted archives — password-aware modification not yet
        // supported. Header-encrypted formats refuse to even surface a
        // listing without a password, so `Archive::open` itself can fail
        // with a `Password` error or with a `Format`/`Corruption` carrying
        // an "encrypt"/"passphrase" tell. Remap those into the same clean
        // `OperationBlocked(MODIFY, ...)` the metadata-encrypted case
        // produces.
        let encrypted_modify_blocked = || {
            ArchiveError::operation_blocked(
                ops::MODIFY,
                "Encrypted archives cannot be modified (password-aware modification not yet supported)",
            )
        };
        // Reuse the format the locked handle already detected — a plain
        // `Archive::open` would re-open the file and re-run magic
        // detection that can theoretically disagree with the locked
        // result.
        //
        // R0080-0041: the encryption probe reopens the pathname; make sure
        // it still names the locked inode before probing.
        revalidate_locked_identity(ops::MODIFY, &path_buf, locked_identity)?;
        match Archive::open_as_format(&path_buf, format) {
            Ok(check) => match check.is_encrypted() {
                Ok(true) => return Err(encrypted_modify_blocked()),
                Ok(false) => {}
                // A listing failure on the encryption probe is not free:
                // it usually means the format is header-encrypted (in
                // which case we already block), or it means the archive
                // is corrupt (in which case the caller must hear about
                // it before we start a copy-on-write modify). Either way
                // we propagate rather than swallowing to `false`
                // (R0070-0004). Encryption-shaped errors are remapped to
                // the same clean modify-blocked reason.
                Err(err) if looks_like_encryption(&err) => {
                    return Err(encrypted_modify_blocked());
                }
                Err(err) => return Err(err),
            },
            Err(err) if looks_like_encryption(&err) => {
                return Err(encrypted_modify_blocked());
            }
            Err(err) => return Err(err),
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
                // R0080-0042: the read backend reopens the pathname;
                // revalidate it still names the locked inode before binding.
                revalidate_locked_identity(ops::MODIFY, &path_buf, locked_identity)?;
                let archive = LibarchiveArchive::open(&path_buf)?;
                ArchiveBackend::Libarchive(Box::new(archive))
            }
        };

        Ok(Self {
            backend,
            path: path_buf,
            mode: ArchiveMode::Modify,
            format,
            entry_cache: once_cell::sync::OnceCell::new(),
            modifications: Some(ModificationTracker {
                locked_identity,
                ..Default::default()
            }),
            mod_options: None,
            _backing_tempfile: None,
            _lock_file: Some(lock_file),
            write_namespace: None,
            write_poisoned: false,
            finalized: false,
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
    /// - `preserve_metadata`: when true, `commit_changes()` preserves
    ///   modification time, access time, creation/birth time, and Unix
    ///   permissions via metadata-aware add helpers on both ZipWriter
    ///   and libarchive backends. The ZIP writer attaches a 0x5455
    ///   ("Universal Time") extra-field block; the libarchive writer
    ///   uses `archive_entry_set_atime` / `archive_entry_set_birthtime`.
    ///   `Archive::open(...).list_files()` surfaces these timestamps
    ///   back through `accessed` and `created` for ZIP archives: the
    ///   sole `zip`-crate backend reads the 0x5455 extended-timestamp
    ///   extra field from each central-directory record (OI-0065-002).
    /// - `compression`: overrides compression settings during archive recreation.
    pub fn modify_with_options(
        path: impl AsRef<Path>,
        options: ModificationOptions,
    ) -> Result<Self> {
        let mut archive = Self::modify(path)?;
        archive.mod_options = Some(options);
        Ok(archive)
    }

    /// Borrow the modification tracker, asserting Modify mode. `Archive::open_modify`
    /// establishes the invariant that `mode == Modify` iff `modifications.is_some()`,
    /// so both failure branches map to the same `OperationBlocked` error labelled
    /// with the caller's op name.
    fn require_modify_tracker(&mut self, op: &'static str) -> Result<&mut ModificationTracker> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                op,
                "Only available in Modify mode",
            ));
        }
        self.modifications
            .as_mut()
            .ok_or_else(|| ArchiveError::operation_blocked(op, "Archive not in Modify mode"))
    }

    /// Add an entry to archive (for Modify mode) — snapshot semantic.
    ///
    /// The byte slice is captured into an owned `Vec<u8>` at call
    /// time, so the caller can mutate or drop the source slice
    /// immediately and the data is safe inside the tracker. Commit
    /// is retryable indefinitely because the bytes are owned.
    /// Changes are applied when `commit_changes()` is called.
    /// In Write mode, use `add_file_from_data()` instead.
    ///
    /// For a 5 GB add that would blow RAM through the snapshot path,
    /// prefer [`Archive::add_entry_from_path`] (live filesystem
    /// path) or [`Archive::add_entry_from_reader`] (owned reader);
    /// both stream the payload at commit time without buffering the
    /// entire entry.
    pub fn add_entry(&mut self, path: &str, data: &[u8]) -> Result<()> {
        // R0001-0071: mode gate first, then operation inputs, so a
        // read/write handle reports the mode error instead of
        // `InvalidPath` — the precedence `remove_entry` already uses.
        let modifications = self.require_modify_tracker(ops::ADD_ENTRY)?;
        crate::security::validate_archive_internal_path(path)?;
        modifications
            .added
            .push((path.to_string(), EntrySource::Buffered(data.to_vec())));
        Ok(())
    }

    /// Add an entry from a filesystem path (for Modify mode) — live
    /// path captured at call time and streamed at commit time
    /// (AD 0062 A.4).
    ///
    /// Routes through the writer's `add_file_from_path` helper so
    /// when `preserve_metadata = true` on the active
    /// [`ModificationOptions`], source mtime / atime / btime / Unix
    /// permissions flow into the new archive entry automatically.
    /// Only the path is captured here — commit reopens it and
    /// archives its contents then (the live-path design, AD 0062
    /// A.4). A same-path replacement written before `commit_changes()`
    /// is picked up silently: the current contents are archived. The
    /// commit returns the underlying I/O error only when the source is
    /// deleted or rendered unreadable by commit time; other queued
    /// entries are unaffected.
    ///
    /// This is the recommended path for large file adds — the
    /// payload never buffers in RAM and the writer's per-backend
    /// fast path handles compression directly from the file.
    pub fn add_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()> {
        // R0001-0071: mode gate before input validation (see `add_entry`).
        let modifications = self.require_modify_tracker(ops::ADD_ENTRY)?;
        crate::security::validate_archive_internal_path(archive_path)?;
        modifications.added.push((
            archive_path.to_string(),
            EntrySource::Path(fs_path.to_path_buf()),
        ));
        Ok(())
    }

    /// Add an entry from an owned reader (for Modify mode) — live
    /// stream captured at call time and consumed at commit time
    /// (AD 0062 A.4).
    ///
    /// `size`:
    /// - `Some(n)` is trusted as the libarchive-header value.
    ///   Length mismatches between the declared `n` and the actual
    ///   stream length surface as
    ///   [`ArchiveError::Corruption`](crate::ArchiveError) at
    ///   commit time.
    /// - `None` triggers tempfile staging via
    ///   `stage_unknown_size_entry` (also used for retained-entry
    ///   streaming) so the reader is drained chunk-by-chunk into a
    ///   tempfile to learn the actual length before the libarchive
    ///   header is written. RAM stays bounded by the streaming
    ///   chunk size regardless of entry size.
    ///
    /// **Reader is consumed; retry requires a fresh `modify()` handle.**
    /// [`Archive::commit_changes`] takes `self` by
    /// value, so a commit failure consumes both the partially-drained
    /// reader and the modify handle. Use
    /// [`Archive::clear_operations`] *before* calling
    /// `commit_changes` if you need to swap out a reader-source entry;
    /// after a failed commit, open a fresh `Archive::modify()` and
    /// requeue.
    pub fn add_entry_from_reader<R>(
        &mut self,
        archive_path: &str,
        reader: R,
        size: Option<u64>,
    ) -> Result<()>
    where
        R: Read + Send + 'static,
    {
        // R0001-0071: mode gate before input validation (see `add_entry`).
        let modifications = self.require_modify_tracker(ops::ADD_ENTRY)?;
        crate::security::validate_archive_internal_path(archive_path)?;
        modifications.added.push((
            archive_path.to_string(),
            EntrySource::Reader {
                reader: Box::new(reader),
                size,
            },
        ));
        Ok(())
    }

    /// Add a directory entry to archive (for Modify mode)
    ///
    /// In Modify mode, this tracks the directory addition. Changes are applied
    /// when `commit_changes()` is called.
    pub fn add_directory_entry(&mut self, path: &str) -> Result<()> {
        // R0001-0071: mode gate before input validation (see `add_entry`).
        let modifications = self.require_modify_tracker(ops::ADD_DIRECTORY_ENTRY)?;
        crate::security::validate_archive_internal_path(path)?;
        modifications.added_directories.push(path.to_string());
        Ok(())
    }

    /// Remove an entry from archive (for Modify mode) by path.
    ///
    /// Marks **every** source entry carrying this path for removal. Archives
    /// that legitimately contain duplicate paths (ZIP/7z) have each record
    /// removed — use [`Archive::remove_entry_by_id`] to target a single
    /// record.
    ///
    /// Returns the number of source entries that matched and were queued for
    /// removal. Zero means the path did not appear in the source listing —
    /// callers who need strict removal semantics should check the return
    /// value. Changes are applied when `commit_changes()` is called.
    pub fn remove_entry(&mut self, path: &str) -> Result<usize> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::REMOVE_ENTRY,
                "Only available in Modify mode",
            ));
        }

        crate::security::validate_archive_internal_path(path)?;

        // Resolve path → IDs against the source archive's listing. Paths
        // that don't match any source entry are accepted and return 0 so
        // callers can freely queue removes before adds (an added-only
        // path has no source entry to target — the existing contract
        // documented in `test_modify_add_and_remove_queues_are_independent`).
        let matching_ids: Vec<usize> = self
            .list_files()?
            .iter()
            .filter(|e| e.path == path)
            .map(|e| e.id)
            .collect();
        let removed_count = matching_ids.len();

        let modifications = self.require_modify_tracker(ops::REMOVE_ENTRY)?;
        modifications.removed.extend(matching_ids);
        Ok(removed_count)
    }

    /// Remove a single entry from archive (for Modify mode) by its ID.
    ///
    /// Targets the exact entry at positional index `id` within `list_files()`
    /// order — the precise removal API for archives containing duplicate
    /// paths.
    pub fn remove_entry_by_id(&mut self, id: usize) -> Result<()> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::REMOVE_ENTRY,
                "Only available in Modify mode",
            ));
        }

        let entry_count = self.list_files()?.len();
        if id >= entry_count {
            return Err(ArchiveError::operation_blocked(
                ops::REMOVE_ENTRY,
                crate::error::invalid_id_reason(id, entry_count),
            ));
        }

        let modifications = self.require_modify_tracker(ops::REMOVE_ENTRY)?;
        modifications.removed.insert(id);
        Ok(())
    }

    /// Replace an entry in archive (for Modify mode).
    ///
    /// Convenience method that removes **every** entry with the given path
    /// and adds a single new one. For archives with duplicate paths use
    /// [`Archive::remove_entry_by_id`] + [`Archive::add_entry`] explicitly.
    ///
    /// **No-match note:** if `path` does not exist in the source listing,
    /// the underlying [`remove_entry`](Self::remove_entry) silently
    /// returns zero matches and the new entry is added regardless,
    /// leaving the archive shape equivalent to a plain `add_entry(path,
    /// data)`. Callers that need strict replace-or-fail semantics
    /// should call `remove_entry(path)?` themselves and check its
    /// return value before adding. The intentional silent-add shape
    /// mirrors `tar --update` and matches the contract documented on
    /// `remove_entry`.
    pub fn replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()> {
        // Zero-match remove is intentionally silent — see method doc.
        let _ = self.remove_entry(path)?;
        self.add_entry(path, data)?;
        Ok(())
    }

    /// Replace an entry from a filesystem path (AD 0062 A.4).
    ///
    /// Symmetric `_from_path` companion to [`Self::replace_entry`].
    /// Equivalent to `remove_entry(path)` followed by
    /// `add_entry_from_path(path, fs_path)`. The same zero-match-is-
    /// silent semantic applies — if the path does not exist in the
    /// source listing, the new entry is added regardless.
    pub fn replace_entry_from_path(&mut self, path: &str, fs_path: &Path) -> Result<()> {
        let _ = self.remove_entry(path)?;
        self.add_entry_from_path(path, fs_path)?;
        Ok(())
    }

    /// Replace an entry from an owned reader (AD 0062 A.4).
    ///
    /// Symmetric `_from_reader` companion to [`Self::replace_entry`].
    /// `size` follows the same `Some` / `None` contract as
    /// [`Self::add_entry_from_reader`].
    pub fn replace_entry_from_reader<R>(
        &mut self,
        path: &str,
        reader: R,
        size: Option<u64>,
    ) -> Result<()>
    where
        R: Read + Send + 'static,
    {
        let _ = self.remove_entry(path)?;
        self.add_entry_from_reader(path, reader, size)?;
        Ok(())
    }

    /// Get pending operations count.
    ///
    /// Returns the total queued additions, removals, and directory entries,
    /// or `OperationBlocked` if the archive is not in Modify mode (rather
    /// than silently returning zero, which hid misuse from read/write
    /// handles).
    pub fn pending_operations(&self) -> Result<usize> {
        let modifications = self.modifications.as_ref().ok_or_else(|| {
            ArchiveError::operation_blocked(
                ops::PENDING_OPERATIONS,
                "pending_operations is only available in Modify mode",
            )
        })?;
        Ok(modifications.added.len()
            + modifications.removed.len()
            + modifications.added_directories.len())
    }

    /// Clear pending operations.
    ///
    /// Returns `OperationBlocked` if the archive is not in Modify mode
    /// rather than silently no-oping.
    pub fn clear_operations(&mut self) -> Result<()> {
        let modifications = self.modifications.as_mut().ok_or_else(|| {
            ArchiveError::operation_blocked(
                ops::CLEAR_OPERATIONS,
                "clear_operations is only available in Modify mode",
            )
        })?;
        modifications.added.clear();
        modifications.removed.clear();
        modifications.added_directories.clear();
        Ok(())
    }

    /// Validate that the currently-queued modifications are committable
    /// (R0075-0039 dry-run gate).
    ///
    /// Runs the same checks that `commit_changes` performs at its
    /// pre-write boundary, but does not touch the filesystem:
    ///
    /// - Statically-knowable `mod_options` gates (R0079-0014): the
    ///   R0071-0002 container-format mismatch and the create facade's
    ///   [`CompressionOptions::validate_for_format`](crate::options::CompressionOptions::validate_for_format)
    ///   policy, run on the effective compression options.
    /// - Retained-path shape check (R0079-0020). Every retained file
    ///   and directory path is re-validated with the same write-side
    ///   `validate_archive_internal_path` policy queued adds go
    ///   through, so a wild-but-readable source path fails loudly
    ///   here — before any temp file exists — instead of surfacing
    ///   from the replay loop under an unrelated operation label.
    /// - Namespace duplicate-path check (R0069-0062). Catches
    ///   retained+added, added+added, and the directory-vs-file family
    ///   (`a` vs `a/b`) before any temp archive is written. Link and
    ///   special entries claim no namespace slot — the rewrite drops
    ///   them per FR-022 (R0079-0037).
    /// - ZIP source-extras cross-check (R0069-0064 / R0075-0034) when
    ///   the source format is ZIP. Confirms the facade's own listing
    ///   agrees with the central-directory walker that
    ///   `commit_changes` will trust at write time.
    ///
    /// Used by [`crate::v2::ModifyArchive::try_commit_changes`] to give
    /// callers a non-consuming validation path. `commit_changes` itself
    /// calls this helper at the same point in its sequencing.
    ///
    /// This is **snapshot-time** validation only: it reflects the queued
    /// operations, the source archive, and the archive's on-disk identity
    /// as they stand at the moment of the call. It leases nothing and
    /// freezes nothing — path/reader sources, backup targets, and the
    /// archive's identity at `self.path` can all change before commit runs.
    /// Passing here therefore does not guarantee the equivalent commit will
    /// succeed: `commit_changes` re-runs these gates (and revalidates the
    /// locked archive identity before every pathname use) rather than
    /// trusting a prior dry run (R0080-0047).
    ///
    /// I/O errors during the cross-check (corrupt source archive)
    /// surface as the underlying [`ArchiveError`].
    ///
    /// Returns the ZIP source extras loaded for the cross-check
    /// (`Some` only for ZIP sources) so `commit_changes` can reuse
    /// them instead of re-walking the central directory (DEF-005).
    pub(crate) fn validate_pending_commit(
        &self,
        modifications: &ModificationTracker,
    ) -> Result<Option<ZipSourceExtras>> {
        // Empty-modification short-circuit mirrors `commit_changes`:
        // there is nothing to validate, and "Ok" matches the
        // semantics of a real commit that would simply early-return.
        if modifications.added.is_empty()
            && modifications.removed.is_empty()
            && modifications.added_directories.is_empty()
        {
            return Ok(None);
        }

        // R0079-0014: statically-knowable `mod_options` rejections must
        // fail the dry-run too. `commit_changes` snapshots `mod_options`
        // before calling here, so on that path `self.mod_options` is
        // already `None` and the defaults below trivially pass — its own
        // post-validate gates remain load-bearing. For
        // `try_commit_changes` this is the only place these gates can
        // fire without consuming the handle.
        match self
            .mod_options
            .as_ref()
            .and_then(|m| m.compression.as_ref())
        {
            Some(compression) => {
                check_rewrite_format_override(compression.format, self.format)?;
                compression.validate_for_format()?;
            }
            None => {
                // Mirror the defaults `commit_changes` builds so the
                // dry-run stays a full preflight even if a format's
                // default options ever stop validating.
                crate::options::CompressionOptions::new(self.format).validate_for_format()?;
            }
        }

        // Retained-path shape + dup-path namespace check.
        let entries = self.list_files()?;
        let mut tracker = NamespaceTracker::default();
        for entry in entries {
            if modifications.removed.contains(&entry.id) {
                continue;
            }
            // FR-022: the rewrite drops link/special entries entirely,
            // so the gate must not claim namespace slots for them —
            // otherwise an add at a link's path is falsely rejected as
            // a duplicate of an entry the commit never writes
            // (R0079-0037).
            if rewrite_drops_entry_type(entry.entry_type) {
                continue;
            }
            // R0079-0020: strict-upfront path policy. Retained entries
            // are re-validated with the same write-side validator
            // queued adds go through, so a wild-but-readable source
            // path (`./dir/`, absolute, `..` segments) fails here —
            // before any temp file exists — labeled with the commit
            // gate instead of the replay loop's `add_directory`.
            if let Err(err) = crate::security::validate_archive_internal_path(&entry.path) {
                return Err(ArchiveError::operation_blocked(
                    ops::COMMIT_CHANGES,
                    format!(
                        "retained entry '{}' (id {}) cannot be re-emitted by the rewrite: {}",
                        entry.path, entry.id, err
                    ),
                ));
            }
            if entry.is_directory() {
                tracker.record_dir(ops::COMMIT_CHANGES, entry.path.as_str())?;
            } else {
                tracker.record_file(ops::COMMIT_CHANGES, entry.path.as_str())?;
            }
        }
        for dir_path in &modifications.added_directories {
            tracker.record_dir(ops::COMMIT_CHANGES, dir_path)?;
        }
        for (path, _) in &modifications.added {
            tracker.record_file(ops::COMMIT_CHANGES, path)?;
        }

        // ZIP source-extras cross-check. `commit_changes` reuses the
        // returned extras so the side-car central-directory walk and
        // cross-check run once per commit instead of once here and
        // again in the write path.
        if self.format == ArchiveFormat::Zip {
            // R0080-0043: the ZIP extras walk reopens the pathname directly;
            // revalidate it still names the locked inode before trusting its
            // central directory as the cross-check source.
            revalidate_locked_identity(
                ops::COMMIT_CHANGES,
                &self.path,
                modifications.locked_identity,
            )?;
            let extras = load_zip_source_extras(&self.path)?;
            let listing = self.list_files()?;
            cross_check_source_listing(listing, &extras.per_index)?;
            return Ok(Some(extras));
        }

        Ok(None)
    }

    /// Commit changes to archive (for Modify mode).
    ///
    /// Applies all tracked modifications (additions, removals, replacements) to the archive.
    /// This creates a new archive with the modifications and replaces the original.
    ///
    /// # Process
    /// 1. Validate the pending commit (namespace conflicts, retained-path
    ///    shape, statically-knowable option gates).
    /// 2. Create a temporary archive in the same format.
    /// 3. Copy retained entries from the original — except symlinks,
    ///    hard links and other non-regular entries, which the rewrite
    ///    **drops with a warning** (see below).
    /// 4. Materialise queued sources (buffered / path / reader) and write them.
    /// 5. Atomically rename the temp file over the original (with optional
    ///    backup), carrying the original file's own permissions across the
    ///    replace (R0079-0021).
    ///
    /// # Dropped link / special entries (FR-022)
    ///
    /// The rewrite can only re-emit regular files and directories, so a
    /// retained symlink, hard link, or other non-regular entry
    /// (FIFO, socket, device node) is **dropped** — the committed
    /// archive does not contain it. This is lossy and it is not
    /// silent: one
    /// [`ArchiveWarning::SkippedUnsupportedEntry`](crate::error::ArchiveWarning)
    /// carrying the entry path and kind is emitted per dropped entry.
    /// The operation is **not** refused and the entry is **not**
    /// re-emitted in some substitute form.
    ///
    /// This method discards those warnings to keep its `Result<()>`
    /// signature. Call
    /// [`Archive::commit_changes_with_warnings`] instead to receive
    /// them; that is the only way to learn which entries the round-trip
    /// lost.
    ///
    /// # Crash safety and temporary files
    ///
    /// The rewrite is copy-on-write: every byte lands in a sibling
    /// staging file named `<archive>.tmp.<pid>.<nanos>.<counter>` and
    /// the original is only replaced by a single atomic rename at the
    /// very end. There is deliberately **no journal** (DEF-005): a
    /// crash, kill, or power loss mid-rewrite therefore leaves the
    /// original archive byte-for-byte intact — either the rename
    /// happened and the new archive is installed, or it did not and
    /// nothing was touched.
    ///
    /// The accepted cost is an orphaned staging file: a process that
    /// dies mid-rewrite cannot clean up after itself. Every *error*
    /// path removes its own staging file (and a partially written
    /// backup, and a backup orphaned by a failed rename), so a
    /// returned `Err` never leaks one; only an abnormal process exit
    /// does. Such orphans are inert, are never read back, and are
    /// recognizable by the `.tmp.<pid>.<nanos>.<counter>` suffix
    /// appended after the original extension.
    ///
    /// # Metadata preservation on the replaced archive
    ///
    /// The atomic rename installs a **new inode** in place of the
    /// original, so only metadata this method explicitly re-applies
    /// survives. The contract is **mode bits only**: the original
    /// archive's Unix permission bits are copied onto the replacement
    /// before the rename (R0079-0021) so the swap cannot silently
    /// widen access (e.g. a `0600` archive resurfacing as `0644`).
    /// That is a security guarantee (anti-widening), not a general
    /// metadata-faithfulness guarantee.
    ///
    /// Everything else tied to the original inode is **lost**:
    /// ownership (uid/gid), POSIX ACLs, extended attributes / security
    /// labels (SELinux, xattrs), and the file's own timestamps (the
    /// replacement carries the rewrite's wall-clock times). Callers
    /// that depend on any of these must re-apply them after commit.
    /// See `Limitations.md` (modification section) for the
    /// user-facing statement of this behavior.
    ///
    /// # Consume-on-error contract
    ///
    /// `commit_changes` takes `self` by value. The handle is consumed
    /// regardless of whether the commit succeeds or fails — including
    /// for **recoverable** failures like duplicate planned outputs,
    /// invalid backup suffixes, or mid-rewrite I/O errors. Callers
    /// who need preflight validation should:
    ///
    /// - Use [`Archive::pending_operations`] to inspect the queue before
    ///   committing.
    /// - Call [`Archive::clear_operations`] to discard queued reader
    ///   sources without materialising them (helpful when a reader
    ///   has gone bad).
    /// - For the consumed handle path: open a fresh
    ///   [`Archive::modify`] handle after a failed commit and
    ///   re-queue. The advisory lock from the previous handle is
    ///   released on drop, so the new `modify()` call can re-acquire
    ///   it.
    ///
    /// A non-consuming `try_commit_changes(&mut self)` is exposed via
    /// [`crate::v2::ModifyArchive`] (behind the `v2-api` feature) for
    /// callers that want to validate their queued operations without
    /// throwing the handle away on rejection (R0075-0039).
    ///
    /// # Errors
    /// Returns error if not in Modify mode or if I/O operations fail.
    pub fn commit_changes(self) -> Result<()> {
        self.commit_changes_with_warnings()
            .map(|result| result.value)
    }

    /// [`Archive::commit_changes`] that also hands back the warnings the
    /// rewrite produced.
    ///
    /// Identical in behaviour and error contract to `commit_changes` —
    /// same validation, same copy-on-write staging, same atomic
    /// replace, same consume-on-error semantics; read that method's
    /// documentation for all of it. The only difference is the return
    /// type: on success the caller receives a
    /// [`ResultWithWarnings`](crate::error::ResultWithWarnings) whose
    /// `warnings` vector holds one
    /// [`ArchiveWarning::SkippedUnsupportedEntry`](crate::error::ArchiveWarning)
    /// per retained symlink / hard link / special entry the rewrite
    /// dropped (FR-022), in source-listing order.
    ///
    /// An empty vector means the commit was lossless with respect to
    /// entry kinds. Warnings are only produced on the success path: a
    /// failed commit returns `Err` and replaces nothing, so there is no
    /// lossy result to describe.
    pub fn commit_changes_with_warnings(mut self) -> Result<ResultWithWarnings<()>> {
        if self.mode != ArchiveMode::Modify {
            return Err(ArchiveError::operation_blocked(
                ops::COMMIT_CHANGES,
                "Only available in Modify mode",
            ));
        }

        let modifications = self.modifications.take().ok_or_else(|| {
            ArchiveError::operation_blocked(ops::COMMIT_CHANGES, "No modification tracker")
        })?;

        // Snapshot modification options (defaults if unset). Declared mutable
        // so the owned progress callback in `compression` can be `take()`-ed
        // and handed to the new archive without a silent clone.
        let mut mod_options = self.mod_options.take().unwrap_or_default();

        // Re-validate `backup_suffix` at the commit boundary even when the
        // builder set it: the field is `pub`, so a caller can mutate it
        // through a struct literal or a direct assignment after
        // `with_backup` ran the original suffix sanitizer (R0070-0009).
        // Apply the same fall-back-to-`.bak`-on-bad-suffix policy
        // documented on `ModificationOptions::with_backup`.
        if mod_options.create_backup {
            let bad_suffix = mod_options.backup_suffix.is_empty()
                || mod_options.backup_suffix.contains('/')
                || mod_options.backup_suffix.contains('\\');
            if bad_suffix {
                mod_options.backup_suffix = ".bak".to_string();
            }
        }

        // If no modifications, return early. We deliberately do *not* create a
        // backup in this branch — there is nothing to roll back to.
        if modifications.added.is_empty()
            && modifications.removed.is_empty()
            && modifications.added_directories.is_empty()
        {
            return Ok(ResultWithWarnings::ok(()));
        }

        // Unique temp path beside the original. nanos+pid handles cross-process
        // collisions; the AtomicU64 counter guarantees uniqueness for parallel
        // commits within the same process (test harnesses, async runtimes).
        // Append (not replace) a suffix so the original archive extension
        // survives in the temp name, making leftover temps easy to recognize
        // by humans and recovery tooling.
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
            let suffix = format!(".tmp.{pid}.{nanos}.{counter}");
            let mut name = self.path.as_os_str().to_owned();
            name.push(&suffix);
            std::path::PathBuf::from(name)
        };

        // R0075-0039: dup-path rejection + ZIP source-extras
        // cross-check are extracted into [`Self::validate_pending_commit`]
        // so `ModifyArchive::try_commit_changes` can run them without
        // entering the actual write path. The extras it loaded are
        // reused by the write closure below instead of re-walking the
        // ZIP central directory (DEF-005).
        let validated_zip_extras = self.validate_pending_commit(&modifications)?;

        // FR-022 drop warnings, collected by the retained-entry replay
        // loop below. Declared outside the write closure (and passed in
        // by `&mut`) so they survive to the return statement instead of
        // being logged and lost inside it: the whole point of the
        // warning is that it reaches the caller.
        let mut warnings: Vec<ArchiveWarning> = Vec::new();

        let result = (|warnings: &mut Vec<ArchiveWarning>| -> Result<()> {
            // Use caller-supplied compression settings, or format defaults.
            // `take()` lets us move the single owned `CompressionOptions`
            // (progress callback and all) into the new archive; cloning
            // would have silently dropped the callback.
            let options = mod_options
                .compression
                .take()
                .unwrap_or_else(|| crate::options::CompressionOptions::new(self.format));

            // R0071-0002: a modify-mode rewrite must keep the source's
            // container format — see `check_rewrite_format_override`,
            // shared with the `validate_pending_commit` dry-run
            // (R0079-0014). The override remains useful for `level` and
            // `progress`; `password` is rejected by the create facade
            // (MADR-0027) and `split_size` is reserved/deferred (DEF-002,
            // R0073-0013).
            check_rewrite_format_override(options.format, self.format)?;

            let mut new_archive = Self::create(&temp_path, options)?;

            let preserve = mod_options.preserve_metadata;

            // For ZIP sources, the archive comment and per-entry
            // compression methods were loaded — and cross-checked
            // against the facade listing (R0069-0064 / R0075-0034) —
            // by `validate_pending_commit` above. The modify path
            // routes source reads through libarchive, which does not
            // surface this metadata; the side-car lookup keeps
            // round-trips faithful without switching backends.
            let zip_extras = validated_zip_extras
                .filter(|_| matches!(new_archive.backend, ArchiveBackend::ZipWriter(_)));

            // Copy all entries from original except removed ones. The
            // `ValidatedSource` token (D3) replaces the prior
            // `_unchecked` calls — its existence is the type-level
            // proof that the listing the loop iterates was sourced
            // from `self`. Removal is keyed by entry ID so
            // duplicate-named entries stay distinct.
            //
            // Branch on `EntryType` (R0070-0006 / R0070-0007). The
            // previous loop ran every retained entry through the
            // file-payload pipeline, so retained directories became
            // zero-byte files and retained symlinks/hardlinks had
            // their link payloads materialized as regular files. Both
            // violate the FR-022 link-skip policy and the round-trip
            // contract documented on `commit_changes`.
            // R0080-0044: the retained-entry replay reads the source
            // through the pathname-based backend. Revalidate once before the
            // loop that the pathname still names the locked inode so retained
            // bytes cannot be sourced from a post-validation replacement.
            revalidate_locked_identity(
                ops::COMMIT_CHANGES,
                &self.path,
                modifications.locked_identity,
            )?;
            use crate::entry::EntryType;
            let src = self.validated_source();
            let entries = self.list_files()?;
            for entry in entries {
                if modifications.removed.contains(&entry.id) {
                    continue;
                }

                // link/special entries are dropped per FR-022;
                // documented on the public `commit_changes` rustdoc.
                // The shared predicate keeps this loop and the
                // `validate_pending_commit` namespace gate in lockstep
                // (R0079-0037).
                //
                // Dropping is lossy, so it must not be silent: emit one
                // warning per dropped entry naming the path and the
                // kind. `dropped_entry_kind` is the same function
                // `rewrite_drops_entry_type` is defined in terms of, so
                // a dropped entry can never fail to produce a warning.
                if let Some(kind) = dropped_entry_kind(entry.entry_type) {
                    warnings.push(ArchiveWarning::dropped_unsupported_entry(
                        entry.path.as_str(),
                        kind,
                    ));
                    continue;
                }

                match entry.entry_type {
                    EntryType::Directory => {
                        new_archive.add_directory(&entry.path)?;
                        continue;
                    }
                    // Handled by the FR-022 skip above; the arm stays
                    // so a new `EntryType` variant forces a decision
                    // here.
                    EntryType::Symlink | EntryType::HardLink | EntryType::Other => continue,
                    EntryType::File => {}
                }

                let streamable_size = entry.size;
                let compression_override = zip_extras
                    .as_ref()
                    .and_then(|extras| extras.per_index.get(entry.id).map(|v| v.compression));

                // One match on the backend routing; `preserve` only
                // selects the writer method at each call site, so the
                // streaming/staging decisions cannot drift between the
                // metadata-preserving and plain variants.
                match &mut new_archive.backend {
                    ArchiveBackend::ZipWriter(w) => {
                        let mut stream = src.extract_to_stream(&entry.path)?;
                        if preserve {
                            w.add_file_from_reader_with_metadata(
                                &entry.path,
                                &mut stream,
                                entry,
                                compression_override,
                            )?;
                        } else {
                            w.add_file_from_reader(&entry.path, &mut stream)?;
                        }
                    }
                    ArchiveBackend::Libarchive(b) => match streamable_size {
                        Some(size) => {
                            let mut stream = src.extract_to_stream(&entry.path)?;
                            if preserve {
                                b.add_file_from_reader_with_metadata(
                                    &entry.path,
                                    &mut stream,
                                    size,
                                    entry,
                                )?;
                            } else {
                                b.add_file_from_reader(&entry.path, &mut stream, size)?;
                            }
                        }
                        None => {
                            // R0069-0065: libarchive needs a known
                            // entry size in the header, but the
                            // source format reports `None`
                            // (streaming-only formats whose
                            // uncompressed size isn't known until
                            // the decoder finishes). Stage to a
                            // tempfile so RAM is bounded by the
                            // streaming chunk, learn the size, then
                            // hand the staged file to the writer.
                            let (mut staged, size) = stage_unknown_size_entry(&src, &entry.path)?;
                            if preserve {
                                b.add_file_from_reader_with_metadata(
                                    &entry.path,
                                    &mut staged,
                                    size,
                                    entry,
                                )?;
                            } else {
                                b.add_file_from_reader(&entry.path, &mut staged, size)?;
                            }
                        }
                    },
                    _ => {
                        let data = src.extract_to_memory(&entry.path)?;
                        new_archive.add_file_from_data(&entry.path, &data)?;
                    }
                }
            }

            // Add new directory entries. A directory path is emitted
            // at most once (R0081-0038): the namespace tracker
            // coalesces duplicate directory reservations, but
            // `added_directories` still retains each queued occurrence,
            // so dedup the normalized path here before the backend
            // writes a redundant central-directory record.
            let mut emitted_dirs = std::collections::HashSet::new();
            for dir_path in modifications.added_directories {
                if emitted_dirs.insert(normalize_dup_check_path(&dir_path)) {
                    new_archive.add_directory(&dir_path)?;
                }
            }

            // Add new entries. Branches over the typed `EntrySource`
            // (AD 0062 A.4) so each source kind takes its own most-
            // efficient writer path:
            //
            // - `Buffered`: writer's `add_file_from_data` (snapshot
            //   semantic preserved).
            // - `Path`: writer's `add_file_from_path` so source
            //   mtime / atime / btime / Unix permissions flow
            //   through automatically.
            // - `Reader { size: Some(n) }`: writer's
            //   `add_file_from_reader(... size = n)`.
            // - `Reader { size: None }`: drain the reader through
            //   `stage_unknown_size_entry` to learn the actual length
            //   first, then route to `add_file_from_reader`.
            for (path, source) in modifications.added {
                match source {
                    EntrySource::Buffered(data) => {
                        new_archive.add_file_from_data(&path, &data)?;
                    }
                    EntrySource::Path(fs_path) => {
                        new_archive.add_file_from_path_as(&fs_path, &path)?;
                    }
                    EntrySource::Reader {
                        mut reader,
                        size: Some(size),
                    } => match &mut new_archive.backend {
                        ArchiveBackend::ZipWriter(w) => {
                            // R0075-0032: route through the size-aware
                            // ZipWriter helper so a reader that under-
                            // or over-produces against the declared
                            // size is rejected loudly rather than
                            // silently committed under the wrong size.
                            w.add_file_from_reader_with_size(&path, &mut reader, size)?;
                        }
                        ArchiveBackend::Libarchive(b) => {
                            b.add_file_from_reader(&path, &mut reader, size)?;
                        }
                        _ => buffered_ingest_reader(
                            &mut new_archive,
                            &path,
                            &mut reader,
                            size,
                            "reader-source declared",
                            "commit_add_reader",
                        )?,
                    },
                    EntrySource::Reader { reader, size: None } => {
                        // R0081-0037: an unknown-size reader source is
                        // attacker-influenced input, so bound the drain
                        // — an infinite/hostile reader must not fill the
                        // disk or block the commit forever.
                        let (mut staged, learned_size) = drain_into_tempfile(
                            reader,
                            &path,
                            crate::security::ExtractionLimits::default()
                                .max_file_size()
                                .get(),
                        )?;
                        match &mut new_archive.backend {
                            ArchiveBackend::ZipWriter(w) => {
                                w.add_file_from_reader(&path, &mut staged)?;
                            }
                            ArchiveBackend::Libarchive(b) => {
                                b.add_file_from_reader(&path, &mut staged, learned_size)?;
                            }
                            _ => buffered_ingest_reader(
                                &mut new_archive,
                                &path,
                                &mut staged,
                                learned_size,
                                "staged reader-source",
                                "commit_add_reader_staged",
                            )?,
                        }
                    }
                }
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

            // R0079-0021: the temp archive was created with
            // umask-default permissions; carry the original archive's
            // mode across so the atomic replace below cannot silently
            // widen access (e.g. a 0600 archive resurfacing as 0644).
            // Fatal on Unix, where the mode is the access-control
            // surface this exists to preserve; best-effort elsewhere.
            // R0080-0045: read the mode to preserve from the locked
            // descriptor itself, not a fresh `self.path` stat that a
            // non-cooperating writer could have re-pointed at an unrelated
            // file. The lock descriptor is the inode this session has held
            // since `modify()` opened it.
            {
                let lock_file = self._lock_file.as_ref().ok_or_else(|| {
                    ArchiveError::operation_blocked(
                        ops::COMMIT_CHANGES,
                        "modify handle is missing its advisory-lock descriptor",
                    )
                })?;
                #[cfg(unix)]
                {
                    let original_meta = lock_file
                        .metadata()
                        .map_err(|e| ArchiveError::io("commit_preserve_mode", &self.path, e))?;
                    std::fs::set_permissions(&temp_path, original_meta.permissions())
                        .map_err(|e| ArchiveError::io("commit_preserve_mode", &temp_path, e))?;
                }
                #[cfg(not(unix))]
                if let Ok(original_meta) = lock_file.metadata() {
                    let _ = std::fs::set_permissions(&temp_path, original_meta.permissions());
                }
            }

            // R0080-0005: final identity gate. Everything below either
            // backs up the original or atomically replaces `self.path`; make
            // sure the pathname still names the locked inode before we
            // clobber it so a post-validation swap cannot redirect the
            // replace onto an unrelated file.
            revalidate_locked_identity(
                ops::COMMIT_CHANGES,
                &self.path,
                modifications.locked_identity,
            )?;

            // If a backup is requested, copy the original aside before the
            // atomic replace. We use copy (not rename) so the new file can
            // still take the original's path. A failure here aborts the
            // commit so the caller can decide whether to retry without
            // backups; we have not touched the original yet.
            //
            // On copy failure we also remove the partial backup file
            // (R0069-0066) so retry-without-backups doesn't have to clean
            // up a half-written `<path>.bak` first. The `create_new`
            // call above ensures we only delete files we actually created.
            let backup_path_committed: Option<std::path::PathBuf> = if mod_options.create_backup {
                let backup_path = backup_path_for(&self.path, &mod_options.backup_suffix);
                // Noclobber: atomically claim the backup path via O_CREAT|O_EXCL.
                // If another process/thread wins the race, `create_new` returns
                // AlreadyExists without touching the existing file — closing
                // the TOCTOU window a plain exists()+copy() would leave open.
                let mut dst = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&backup_path)
                    .map_err(|e| {
                        let wrapped = if e.kind() == std::io::ErrorKind::AlreadyExists {
                            std::io::Error::new(
                                std::io::ErrorKind::AlreadyExists,
                                "backup target already exists; remove or rename it before committing",
                            )
                        } else {
                            e
                        };
                        ArchiveError::io("backup", &backup_path, wrapped)
                    })?;
                // R0081-0036: `create_new` opened the backup with
                // umask-default permissions, so a restrictive original
                // (e.g. 0600) would otherwise be copied into a wider
                // 0644 backup — widening access to the archive's bytes,
                // the same anti-widening violation R0079-0021 closes on
                // the committed archive. Carry the locked descriptor's
                // mode onto the backup before any bytes are written.
                // Fatal on Unix (the mode is the access-control surface
                // this preserves); best-effort elsewhere. Ownership and
                // xattrs are out of scope — the load-bearing guarantee
                // is denying the widen.
                {
                    let lock_file = self._lock_file.as_ref().ok_or_else(|| {
                        ArchiveError::operation_blocked(
                            ops::COMMIT_CHANGES,
                            "modify handle is missing its advisory-lock descriptor",
                        )
                    })?;
                    #[cfg(unix)]
                    {
                        let perms = lock_file
                            .metadata()
                            .map_err(|e| ArchiveError::io("backup", &self.path, e))?
                            .permissions();
                        std::fs::set_permissions(&backup_path, perms)
                            .map_err(|e| ArchiveError::io("backup", &backup_path, e))?;
                    }
                    #[cfg(not(unix))]
                    if let Ok(meta) = lock_file.metadata() {
                        let _ = std::fs::set_permissions(&backup_path, meta.permissions());
                    }
                }
                // R0080-0046: copy the backup from a clone of the locked
                // descriptor (rewound to the start) rather than reopening
                // `self.path`, so the backup captures the archive this
                // session locked even if the pathname now names a different
                // inode.
                let mut src = {
                    let lock_file = self._lock_file.as_ref().ok_or_else(|| {
                        ArchiveError::operation_blocked(
                            ops::COMMIT_CHANGES,
                            "modify handle is missing its advisory-lock descriptor",
                        )
                    })?;
                    let mut clone = lock_file
                        .try_clone()
                        .map_err(|e| ArchiveError::io("backup", &self.path, e))?;
                    std::io::Seek::seek(&mut clone, std::io::SeekFrom::Start(0))
                        .map_err(|e| ArchiveError::io("backup", &self.path, e))?;
                    clone
                };
                if let Err(copy_err) = std::io::copy(&mut src, &mut dst) {
                    drop(dst);
                    let _ = std::fs::remove_file(&backup_path);
                    return Err(ArchiveError::io("backup", &backup_path, copy_err));
                }
                // R0075-0035: flush and durably persist the backup
                // before the original is replaced. Without
                // `sync_data` a power loss between the rename below
                // and the kernel's writeback could leave a backup
                // whose payload was never durably written, defeating
                // the recovery contract `create_backup = true`
                // implies.
                if let Err(e) = dst.sync_data() {
                    drop(dst);
                    let _ = std::fs::remove_file(&backup_path);
                    return Err(ArchiveError::io("backup_sync", &backup_path, e));
                }
                drop(dst);
                Some(backup_path)
            } else {
                None
            };

            // Replace original file with new one. If the rename fails
            // after the backup succeeded, remove the now-orphaned backup
            // (R0069-0067) so the caller doesn't end up with a `<path>.bak`
            // that doesn't correspond to any committed swap.
            match rename_with_overwrite(&temp_path, &self.path) {
                Ok(()) => {
                    // R0075-0036: best-effort parent-directory sync
                    // so the rename swap is durably observable after
                    // a crash. Errors here are non-fatal — the rename
                    // already succeeded, and on platforms where the
                    // sync is unsupported we silently degrade.
                    let _ = crate::ffi::common::sync_parent_dir(&self.path);
                    let _ = backup_path_committed
                        .as_ref()
                        .map(|p| crate::ffi::common::sync_parent_dir(p));
                    Ok(())
                }
                Err(rename_err) => {
                    if let Some(backup_path) = backup_path_committed.as_ref() {
                        let _ = std::fs::remove_file(backup_path);
                    }
                    Err(rename_err)
                }
            }
        })(&mut warnings);

        // Clean up the staging file on any failure (finish, write, or
        // rename) so a returned `Err` never leaves an orphan behind —
        // the DEF-005 "no journaling" decision accepts orphans only
        // from an abnormal process exit, not from a handled error.
        // `new_archive` was scoped to the closure above, so its handle
        // on the staging file is already closed and the removal cannot
        // be refused for a sharing violation on Windows.
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
            // R0075-0005: a failed commit leaves the backend in an
            // undefined state; mark the handle poisoned so a stray
            // Drop won't try to re-finalize.
            self.write_poisoned = true;
        } else {
            // R0075-0005: successful rename swap is the durable
            // commitment for Modify mode — flag the handle as
            // finalized so the Drop impl skips its warning when
            // `self` falls out of scope at function return.
            self.finalized = true;
        }

        // The FR-022 drop warnings only describe a committed archive,
        // so they ride out on the success path; a failed commit
        // replaced nothing and has no lossy result to report.
        result.map(|()| ResultWithWarnings::with_warnings((), warnings))
    }
}

/// Side-car ZIP metadata read directly from the source file via the `zip`
/// crate during `commit_changes`. Used to preserve archive-level comments and
/// per-entry compression methods that the libarchive-backed source reader
/// does not surface. The `per_index` Vec is ordered by ZIP central-directory
/// index so duplicate-named entries retain their individual compression
/// methods; [`cross_check_source_listing`] verifies the index alignment with
/// the facade listing before the consumer trusts it (R0069-0064 / R0075-0034).
#[derive(Debug)]
pub(crate) struct ZipSourceExtras {
    archive_comment: Vec<u8>,
    per_index: Vec<ZipSourceEntryView>,
}

/// Snapshot of one ZIP central-directory entry — name, CRC, size, and
/// compression method. Used by [`cross_check_source_listing`] to verify
/// the side-car `zip` crate's index alignment with the facade's
/// libarchive-backed listing before [`commit_changes`] trusts the
/// per-index compression-method lookup (R0069-0064 / R0075-0034).
#[derive(Debug)]
struct ZipSourceEntryView {
    name: String,
    crc32: u32,
    size: u64,
    compression: zip::CompressionMethod,
}

/// Verify that the ZIP central-directory listing (read via the side-car
/// `zip` crate) agrees with the facade's listing (read via libarchive)
/// at every index. Catches the case where the two readers disagree —
/// either due to listing drift between the two reopens or due to a
/// malformed archive whose central directory diverges from its local
/// file headers (R0069-0064 / R0075-0034). The pre-trust check converts
/// "silent index-key drift" into a typed `Format` error before
/// `commit_changes` propagates a stale compression method.
fn cross_check_source_listing(
    facade_listing: &[ArchiveEntry],
    zip_extras: &[ZipSourceEntryView],
) -> Result<()> {
    // The facade listing includes directories that the central-directory
    // walker also enumerates, so the lengths should match exactly.
    if facade_listing.len() != zip_extras.len() {
        return Err(ArchiveError::format(
            Some(ArchiveFormat::Zip),
            format!(
                "ZIP source listing drift: facade reports {} entries, central directory has {}",
                facade_listing.len(),
                zip_extras.len()
            ),
        ));
    }
    for (i, (facade, zip)) in facade_listing.iter().zip(zip_extras.iter()).enumerate() {
        if facade.path != zip.name {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!(
                    "ZIP source listing drift at index {}: facade='{}', central directory='{}'",
                    i, facade.path, zip.name
                ),
            ));
        }
        // Link/special entries are dropped by the rewrite (FR-022), so
        // their compression method is never replayed and CRC/size drift
        // is meaningless — and the two readers legitimately disagree:
        // libarchive models a symlink as size 0 with a target field,
        // while the ZIP central directory stores the target bytes as
        // payload (R0079-0037).
        if rewrite_drops_entry_type(facade.entry_type) {
            continue;
        }
        if let Some(crc) = facade.crc32
            && crc != zip.crc32
        {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!(
                    "ZIP source listing CRC drift at index {} ({}): facade=0x{:08x}, central directory=0x{:08x}",
                    i, facade.path, crc, zip.crc32
                ),
            ));
        }
        if let Some(size) = facade.size
            && size != zip.size
        {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!(
                    "ZIP source listing size drift at index {} ({}): facade={}, central directory={}",
                    i, facade.path, size, zip.size
                ),
            ));
        }
    }
    Ok(())
}

// PERF(DEF-005): this reopens the ZIP and walks its central directory a second
// time (libarchive already did the first walk for list_files). Tolerable for
// thousand-entry archives but the real fix is to switch the ZIP modify source
// from libarchive to the zip crate so both walks collapse into one.
/// Canonicalise an archive-internal path for duplicate-detection
/// comparison: route through the shared `ffi::common::normalize_path`
/// (backslash → forward slash) and then trim any trailing `/` so
/// directory entries `dir/` and `dir` compare equal.
/// Used by `commit_changes` to detect paths that would land at the same
/// writer output even when their input form differs (R0069-0060 /
/// R0069-0061). Single allocation per call: `normalize_path` produces
/// the `String`, the truncation is in-place.
fn normalize_dup_check_path(path: &str) -> String {
    let mut normalized = crate::ffi::common::normalize_path(path);
    let trimmed_len = normalized.trim_end_matches('/').len();
    normalized.truncate(trimmed_len);
    normalized
}

/// Stage an entry whose uncompressed size is unknown into a tempfile so
/// libarchive's writer (which needs an entry size in the header) can
/// pull it without the rewrite buffering the whole payload in RAM
/// (R0069-0065). The streaming reader is drained chunk-by-chunk into
/// the staging file; the file is rewound and returned alongside the
/// observed size.
fn stage_unknown_size_entry(
    src: &crate::extraction::ValidatedSource<'_>,
    entry_path: &str,
) -> Result<(std::fs::File, u64)> {
    let stream = src.extract_to_stream(entry_path)?;
    // R0081-0037: bound the staging of an unknown-size retained entry
    // at the standard unknown-size fallback ceiling so a
    // streaming-only source cannot fill the disk during the rewrite.
    drain_into_tempfile(
        stream,
        entry_path,
        crate::security::ExtractionLimits::default()
            .max_file_size()
            .get(),
    )
}

/// Buffered fallback for writer backends that don't yet implement
/// `add_file_from_reader`. Reads `size` bytes from `reader` into a
/// `Vec` and emits the entry via `add_file_from_data`. Checked
/// `usize::try_from` so a declared size that overflows the platform's
/// `usize` (32-bit targets) surfaces as `OperationBlocked` instead of
/// silently truncating into an undersized capacity.
///
/// R0075-0033: refuse a reader whose actual byte count differs from
/// the declared `size` so a contract violation is caught here rather
/// than silently committed. The `take(size + 1)` cap upper-bounds the
/// read so an over-producing reader is detected; the post-read length
/// check catches short reads. The mismatch is reported through
/// [`ArchiveError::declared_length_mismatch`] — the same `Corruption`
/// classification the streaming routes use (DCR-011) — so this
/// fallback is not a third spelling of the same violation.
fn buffered_ingest_reader<R: Read>(
    new_archive: &mut Archive,
    path: &str,
    reader: R,
    size: u64,
    size_label: &str,
    io_op: &'static str,
) -> Result<()> {
    let cap = usize::try_from(size).map_err(|_| ArchiveError::OperationBlocked {
        operation: ops::COMMIT_CHANGES.to_string(),
        reason: format!("{size_label} size {size} exceeds usize on this platform"),
    })?;
    let mut buf = Vec::with_capacity(cap);
    let probe_cap = size.saturating_add(1);
    let mut bounded = reader.take(probe_cap);
    bounded
        .read_to_end(&mut buf)
        .map_err(|e| ArchiveError::io(io_op, path, e))?;
    if buf.len() as u64 != size {
        return Err(ArchiveError::declared_length_mismatch(
            path,
            size,
            buf.len() as u64,
        ));
    }
    new_archive.add_file_from_data(path, &buf)
}

/// Drain a `Read` source into a fresh tempfile chunk-by-chunk,
/// rewind, and report the observed byte count. RAM stays bounded by
/// the 64 KiB chunk regardless of payload size.
///
/// `cap` bounds the staging itself (R0081-0037). An unknown-size
/// source — an added reader with `size: None`, or a retained
/// streaming-only entry drained through the unbounded
/// `extract_to_stream` — is read until EOF, so without a ceiling an
/// infinite or hostile reader would fill the disk (or, if it blocks
/// after delivering data, stall the commit). Once the drained total
/// would exceed `cap` the staging is abandoned and an
/// `OperationBlocked` error is returned; the partial tempfile is
/// dropped. Callers pass
/// [`crate::security::ExtractionLimits`]`::max_file_size` (1 GiB by
/// default), the codebase's established unknown-size fallback ceiling.
///
/// Cooperative *cancellation* of the drain is intentionally not wired
/// here: by the time staging runs, the modify-commit progress /
/// cancellation callback has already been moved into the writer
/// backend (`new_archive`), and reaching it would need a backend
/// progress-poll accessor outside this module's file scope. The `cap`
/// already bounds the disk-exhaustion and infinite-data cases;
/// threading the callback through for early cooperative abort is
/// tracked as a follow-up.
fn drain_into_tempfile<R: Read>(
    mut source: R,
    entry_path: &str,
    cap: u64,
) -> Result<(std::fs::File, u64)> {
    use std::io::{Seek, SeekFrom, Write};

    let mut staging = tempfile::tempfile()
        .map_err(|e| ArchiveError::io("modify_stage_unknown_size", entry_path, e))?;
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = source
            .read(&mut buf)
            .map_err(|e| ArchiveError::io("modify_stage_read", entry_path, e))?;
        if n == 0 {
            break;
        }
        // R0081-0037: reject before writing the chunk that would push
        // the staged total past the cap, so the tempfile never holds
        // more than `cap` bytes and an unbounded reader terminates.
        total = total.saturating_add(n as u64);
        if total > cap {
            return Err(ArchiveError::operation_blocked(
                "modify_stage_unknown_size",
                format!(
                    "unknown-size source for '{entry_path}' exceeded the {cap}-byte staging cap; \
                     declare an explicit size or reduce the input",
                ),
            ));
        }
        staging
            .write_all(&buf[..n])
            .map_err(|e| ArchiveError::io("modify_stage_write", entry_path, e))?;
    }
    staging
        .seek(SeekFrom::Start(0))
        .map_err(|e| ArchiveError::io("modify_stage_seek", entry_path, e))?;
    Ok((staging, total))
}

/// Yield every proper ancestor directory path for an already-normalised
/// dup-check key (forward slashes, no trailing `/`). Used by
/// `commit_changes`'s namespace-collision gate to catch a vs a/b style
/// directory/file conflicts (R0069-0062). For input `"a/b/c"` returns
/// `["a", "a/b"]` in shallow-to-deep order. Empty input yields nothing.
fn ancestor_paths(normalized: &str) -> Vec<String> {
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut acc = Vec::new();
    let mut cursor = 0;
    while let Some(slash) = normalized[cursor..].find('/') {
        cursor += slash;
        acc.push(normalized[..cursor].to_string());
        cursor += 1;
        if cursor >= normalized.len() {
            break;
        }
    }
    acc
}

/// Record a file-typed path in the dup-check namespace. Rejects
/// retained-or-added-twice collisions, file-vs-directory collisions
/// at the leaf path, and file-under-leaf-as-file collisions
/// (R0069-0062). On success, also marks every proper ancestor as a
/// directory so a later file at the same ancestor key fails.
///
/// Promoted to `pub(crate)` so the create-side namespace gate
/// (R0069-0052, AD 0062 A.4) can share the modify-side helper. `op`
/// is threaded through every error so write-mode (`add_file_from_*`)
/// and modify-mode (`commit_changes`) callers each see their own
/// public-method name in the diagnostic instead of always reading
/// `commit_changes:`.
pub(crate) fn record_file(
    op: &'static str,
    file_paths: &mut std::collections::HashSet<String>,
    dir_paths: &mut std::collections::HashSet<String>,
    path: &str,
) -> Result<()> {
    let key = normalize_dup_check_path(path);
    if dir_paths.contains(&key) {
        return Err(ArchiveError::operation_blocked(
            op,
            format!("{op}: path '{path}' is claimed as both a file and a directory",),
        ));
    }
    if !file_paths.insert(key.clone()) {
        return Err(ArchiveError::operation_blocked(
            op,
            format!("{op}: duplicate output path '{path}' (retained or added twice)",),
        ));
    }
    for ancestor in ancestor_paths(&key) {
        if file_paths.contains(&ancestor) {
            return Err(ArchiveError::operation_blocked(
                op,
                format!(
                    "{op}: path '{ancestor}' is claimed as both a file and a directory (file '{path}' lives under it)",
                ),
            ));
        }
        dir_paths.insert(ancestor);
    }
    Ok(())
}

/// Record a directory-typed entry in the dup-check namespace. Rejects
/// directory-vs-existing-file collisions at the leaf path and
/// directory-under-leaf-as-file collisions, and — mirroring
/// [`record_file`] — reserves every proper ancestor as a directory so a
/// later file at an ancestor key fails (R0001-0011). Recording the same
/// directory path more than once is accepted and coalesced into the
/// single `dir_paths` slot — but that coalescing only keeps the
/// *namespace* unique; the emission paths must still avoid writing the
/// same directory entry twice, so callers skip re-emitting a directory
/// whose normalized path was already emitted (the modification commit
/// loop and `Archive::add_directory` both do this, R0081-0038). See
/// [`record_file`] for the rationale of the `op` argument.
pub(crate) fn record_dir(
    op: &'static str,
    file_paths: &mut std::collections::HashSet<String>,
    dir_paths: &mut std::collections::HashSet<String>,
    path: &str,
) -> Result<()> {
    let key = normalize_dup_check_path(path);
    if file_paths.contains(&key) {
        return Err(ArchiveError::operation_blocked(
            op,
            format!("{op}: path '{path}' is claimed as both a file and a directory",),
        ));
    }
    // R0001-0011: without the ancestor sweep a file 'a' plus a directory
    // 'a/b' was accepted in either order — `record_file` reserved 'a' as
    // a directory but `record_dir` neither checked nor reserved its
    // ancestors, so the rewrite could emit an impossible namespace.
    for ancestor in ancestor_paths(&key) {
        if file_paths.contains(&ancestor) {
            return Err(ArchiveError::operation_blocked(
                op,
                format!(
                    "{op}: path '{ancestor}' is claimed as both a file and a directory (directory '{path}' lives under it)",
                ),
            ));
        }
        dir_paths.insert(ancestor);
    }
    dir_paths.insert(key);
    Ok(())
}

/// R0071-0002 container-format gate: a modify-mode rewrite must keep
/// the source's container format, so a caller-supplied
/// `ModificationOptions.compression` whose `format` disagrees is
/// refused rather than silently swapping the container — otherwise
/// modifying a ZIP with `CompressionOptions::new(ArchiveFormat::Tar)`
/// would write a TAR over the original path and call it an in-place
/// edit. Shared between `commit_changes`'s write path and
/// `validate_pending_commit`'s dry-run (R0079-0014) so the two stay
/// message-for-message identical.
fn check_rewrite_format_override(
    override_format: ArchiveFormat,
    archive_format: ArchiveFormat,
) -> Result<()> {
    if override_format != archive_format {
        return Err(ArchiveError::operation_blocked(
            ops::COMMIT_CHANGES,
            format!(
                "ModificationOptions.compression.format ({:?}) does not match the archive's format ({:?}); \
                 a modify-mode rewrite cannot change the container format. \
                 Use CompressionOptions::new(archive.format()) and adjust the supported rewrite options (level, progress) only.",
                override_format, archive_format
            ),
        ));
    }
    Ok(())
}

/// FR-022 link-skip predicate: entry types the modify rewrite drops
/// entirely. Shared between `commit_changes`'s retained-entry replay
/// loop and `validate_pending_commit`'s namespace gate so the dry-run
/// models exactly the namespace the commit will produce (R0079-0037).
///
/// Defined in terms of [`dropped_entry_kind`] so the predicate and the
/// warning can never disagree — a type this returns `true` for always
/// has a warning kind to report, and vice versa.
fn rewrite_drops_entry_type(entry_type: crate::entry::EntryType) -> bool {
    dropped_entry_kind(entry_type).is_some()
}

/// The warning kind to report for an entry type the rewrite drops, or
/// `None` for a type the rewrite re-emits faithfully.
///
/// Dropping a retained entry is lossy, so `commit_changes` reports one
/// [`ArchiveWarning::SkippedUnsupportedEntry`] per dropped entry
/// instead of deleting it silently. The `EntryType::Other` bucket maps
/// to [`UnsupportedEntryKind::Other`] rather than a specific special
/// kind: `ArchiveEntry::permissions` is masked to `0o7777` by every
/// backend (the `S_IFMT` bits are stripped before the field is
/// populated), so there is nothing left to refine a FIFO from a socket
/// from a device node with at this layer.
fn dropped_entry_kind(entry_type: crate::entry::EntryType) -> Option<UnsupportedEntryKind> {
    use crate::entry::EntryType;
    match entry_type {
        EntryType::Symlink => Some(UnsupportedEntryKind::Symlink),
        EntryType::HardLink => Some(UnsupportedEntryKind::HardLink),
        EntryType::Other => Some(UnsupportedEntryKind::Other),
        EntryType::File | EntryType::Directory => None,
    }
}

/// Detect archive format from an already-locked file handle without
/// re-opening the path. Used by [`Archive::modify`] to keep the
/// detect → capability check → encryption probe sequence inside the
/// advisory-lock window opened on the path (R0070-0003). Reads the
/// same 512-byte magic prefix [`ArchiveFormat::detect`] uses
/// (R0071-0015 — the previous comment claimed 32 KiB but the
/// detector itself only reads 512 bytes for magic; ISO PVD detection
/// at offset 32769 has always required a separate ISO-aware path),
/// then applies the same centralized extension fallback
/// (`format_from_extension` filtered by `is_extension_fallback`,
/// DCR-003 / R0079-0034) so the fallback set has a single owner
/// shared with `ArchiveFormat::detect`.
fn detect_format_from_locked(file: &std::fs::File, path: &Path) -> Result<ArchiveFormat> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = file
        .try_clone()
        .map_err(|e| ArchiveError::io("modify-detect-clone", path, e))?;
    handle
        .seek(SeekFrom::Start(0))
        .map_err(|e| ArchiveError::io("modify-detect-seek", path, e))?;
    let mut magic = [0u8; 512];
    let bytes_read = handle
        .read(&mut magic)
        .map_err(|e| ArchiveError::io("modify-detect-read", path, e))?;
    if bytes_read < 4 {
        return Err(ArchiveError::format(
            None,
            "modify: archive too small to detect format",
        ));
    }
    if let Ok(format) = ArchiveFormat::detect_from_bytes(&magic[..bytes_read]) {
        // Reuse the central `promote_to_compound_tar` helper so the
        // locked-handle detection path doesn't carry its own copy of
        // the `.tar.gz` / `.tar.bz2` / `.tar.xz` extension routing.
        // Adding a new compound to `format::detect` will
        // flow through here automatically.
        return Ok(crate::format::promote_to_compound_tar(format, path));
    }
    // DCR-003 (R0079-0034): route through the centralized fallback
    // policy so this mirror tracks `ArchiveFormat::detect`'s set
    // (`Tar | Iso | Lzma | TarLzma`) instead of carrying its own
    // ladder — the {tar, iso}-only clone here drifted once already
    // (R0071-0015).
    if let Some(format) = crate::format::format_from_extension(path)
        && format.is_extension_fallback()
    {
        return Ok(format);
    }
    Err(ArchiveError::format(None, "Unknown archive format"))
}

/// True if `err` is a header-encryption / password-required failure
/// from `Archive::open`.
///
/// Used by `modify()` to remap the heterogeneous error shapes that
/// header-encrypted formats raise when probed without a password into a
/// single `OperationBlocked(MODIFY, ...)` reason.
///
/// R0075-0038 / R0069-0057 closure: every backend now classifies
/// encryption errors at its FFI seam (`classify_libarchive_error` for
/// libarchive; the `zip`/`sevenz`/`unrar` wrappers raise
/// `ArchiveError::Password` directly when the underlying crate
/// signals an encryption / wrong-password failure). The substring
/// fallback that lived here through R0069-0057 was retired in
/// Review 0075 — corrupt archives mentioning the word "password" no
/// longer get reclassified as encryption blocks. The function now
/// dispatches purely on the typed `ArchiveError::Password` variant.
fn looks_like_encryption(err: &ArchiveError) -> bool {
    matches!(err, ArchiveError::Password { .. })
}

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

    let mut per_index = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read ZIP entry {}: {}", i, e),
            )
        })?;
        per_index.push(ZipSourceEntryView {
            name: entry.name().to_string(),
            crc32: entry.crc32(),
            size: entry.size(),
            compression: entry.compression(),
        });
    }

    Ok(ZipSourceExtras {
        archive_comment,
        per_index,
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

// Atomic rename helper lives in `crate::ffi::common::rename_with_overwrite` —
// imported and called below.
use crate::ffi::common::rename_with_overwrite;

#[cfg(test)]
mod tests;

/// FR-022 drop warnings, the shared `Corruption` classification of a
/// declared-vs-actual length mismatch, and the DEF-005 staging-file
/// cleanup guarantee. Inline module (not `modification/tests.rs`) so
/// the cases live next to `dropped_entry_kind`,
/// `buffered_ingest_reader`, and the `commit_changes` cleanup arm they
/// pin down.
#[cfg(test)]
mod commit_warning_and_cleanup_tests {
    use super::{Archive, ArchiveError, buffered_ingest_reader};
    use crate::error::{ArchiveWarning, EntrySkipReason, UnsupportedEntryKind};
    use std::io::Write;

    /// A ZIP carrying one regular file and one symlink, plus a
    /// hard-link-free baseline the rewrite can re-emit.
    fn zip_with_symlink(path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        writer.start_file("keep.txt", opts).unwrap();
        writer.write_all(b"keep").unwrap();
        writer.add_symlink("link_path", "keep.txt", opts).unwrap();
        writer.finish().unwrap();
    }

    /// The rewrite drops the symlink (FR-022) but must not do it
    /// silently: exactly one warning naming the path and the kind
    /// reaches the caller through `ResultWithWarnings`.
    #[test]
    fn dropped_symlink_surfaces_as_a_warning_to_the_caller() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("warn_symlink.zip");
        zip_with_symlink(&archive_path);

        let mut archive = Archive::modify(&archive_path).unwrap();
        archive.add_entry("added.txt", b"added").unwrap();
        let committed = archive.commit_changes_with_warnings().unwrap();

        assert_eq!(
            committed.warnings.len(),
            1,
            "expected exactly one drop warning, got: {:?}",
            committed.warnings
        );
        match &committed.warnings[0] {
            ArchiveWarning::SkippedUnsupportedEntry { path, kind, reason } => {
                assert_eq!(path, "link_path");
                assert_eq!(*kind, UnsupportedEntryKind::Symlink);
                assert_eq!(*reason, EntrySkipReason::DroppedDuringModify);
            }
            other => panic!("expected SkippedUnsupportedEntry, got: {other:?}"),
        }
        // The warning describes a *committed* archive: the drop really
        // happened, and the operation was not refused.
        let reopened = Archive::open(&archive_path).unwrap();
        let entries = reopened.list_files().unwrap();
        assert!(
            entries.iter().any(|e| e.path == "added.txt"),
            "the commit must still succeed, not be refused"
        );
        assert!(
            !entries.iter().any(|e| e.path == "link_path"),
            "the dropped symlink must not be re-emitted in any form"
        );
    }

    /// The warning must name the kind it dropped, so a rendered
    /// diagnostic tells the user what they lost.
    #[test]
    fn drop_warning_renders_the_kind_and_the_path() {
        let warning =
            ArchiveWarning::dropped_unsupported_entry("link_path", UnsupportedEntryKind::Symlink);
        let rendered = warning.to_string();
        assert!(rendered.contains("link_path"), "{rendered}");
        assert!(rendered.contains("symbolic link"), "{rendered}");
    }

    /// A kind-lossless rewrite reports no warnings at all, so a caller
    /// can treat a non-empty vector as "this round-trip lost entries".
    #[test]
    fn lossless_commit_reports_no_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("lossless.zip");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            writer.start_file("keep.txt", opts).unwrap();
            writer.write_all(b"keep").unwrap();
            writer.add_directory("dir/", opts).unwrap();
            writer.finish().unwrap();
        }

        let mut archive = Archive::modify(&archive_path).unwrap();
        archive.add_entry("added.txt", b"added").unwrap();
        let committed = archive.commit_changes_with_warnings().unwrap();
        assert!(
            committed.warnings.is_empty(),
            "a files-and-directories-only rewrite must warn about nothing, got: {:?}",
            committed.warnings
        );
    }

    /// The plain `commit_changes` keeps its `Result<()>` shape and
    /// still commits; it discards the warnings by design, which is why
    /// the `_with_warnings` sibling exists.
    #[test]
    fn plain_commit_changes_still_commits_a_dropping_rewrite() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("plain_commit.zip");
        zip_with_symlink(&archive_path);

        let mut archive = Archive::modify(&archive_path).unwrap();
        archive.add_entry("added.txt", b"added").unwrap();
        archive.commit_changes().unwrap();

        let reopened = Archive::open(&archive_path).unwrap();
        assert_eq!(reopened.list_files().unwrap().len(), 2);
    }

    /// DEF-005 third caveat: journaling is deliberately absent, so the
    /// accepted guarantee is "original intact, staging file removed".
    /// A commit that fails *after* the staging archive exists must not
    /// leak it, and must leave the original byte-for-byte unchanged.
    #[test]
    fn failed_commit_leaves_no_staging_file_and_keeps_the_original() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("failing.zip");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            writer.start_file("keep.txt", opts).unwrap();
            writer.write_all(b"keep").unwrap();
            writer.finish().unwrap();
        }
        let original = std::fs::read(&archive_path).unwrap();

        let mut archive = Archive::modify(&archive_path).unwrap();
        // Passes every pre-write gate (valid archive-internal path) and
        // fails only once the replay loop opens the source — i.e. after
        // `Self::create` has already materialised the staging file.
        archive
            .add_entry_from_path("missing.txt", &temp.path().join("does_not_exist.bin"))
            .unwrap();
        let err = archive.commit_changes().unwrap_err();
        assert!(
            matches!(err, ArchiveError::Io { .. }),
            "expected the missing source to surface as I/O, got: {err:?}"
        );

        assert_eq!(
            std::fs::read(&archive_path).unwrap(),
            original,
            "a failed commit must leave the original archive untouched"
        );
        let leaked: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp."))
            .collect();
        assert!(
            leaked.is_empty(),
            "a failed commit must remove its staging file, found: {leaked:?}"
        );
    }

    /// The buffered fallback route classifies a declared-vs-actual
    /// length mismatch exactly like the streaming routes: `Corruption`
    /// via the shared constructor (DCR-011), not a third spelling.
    #[test]
    fn buffered_fallback_length_mismatch_is_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("buffered.zip");
        let mut archive = Archive::create(
            &out,
            crate::options::CompressionOptions::new(super::ArchiveFormat::Zip),
        )
        .unwrap();

        let under = buffered_ingest_reader(
            &mut archive,
            "short.txt",
            &b"abc"[..],
            5,
            "reader-source declared",
            "commit_add_reader",
        )
        .unwrap_err();
        match &under {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "short.txt");
                assert!(details.contains("under-produced"), "{details}");
            }
            other => panic!("expected Corruption, got: {other:?}"),
        }

        let over = buffered_ingest_reader(
            &mut archive,
            "long.txt",
            &b"abcde"[..],
            2,
            "reader-source declared",
            "commit_add_reader",
        )
        .unwrap_err();
        match &over {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "long.txt");
                assert!(details.contains("over-produced"), "{details}");
            }
            other => panic!("expected Corruption, got: {other:?}"),
        }
    }

    /// The ZIP commit route (`add_file_from_reader_with_size`) reports
    /// the same `Corruption`, so `add_entry_from_reader`'s rustdoc
    /// promise holds for a real modify commit and not just for the
    /// helper.
    #[test]
    fn zip_commit_route_length_mismatch_is_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("mismatch.zip");
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            writer.start_file("keep.txt", opts).unwrap();
            writer.write_all(b"keep").unwrap();
            writer.finish().unwrap();
        }

        let mut archive = Archive::modify(&archive_path).unwrap();
        archive
            .add_entry_from_reader("liar.txt", std::io::Cursor::new(b"abc".to_vec()), Some(99))
            .unwrap();
        let err = archive.commit_changes().unwrap_err();
        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "liar.txt");
                assert!(details.contains("under-produced"), "{details}");
            }
            other => panic!("expected Corruption, got: {other:?}"),
        }
    }

    /// `dropped_entry_kind` and `rewrite_drops_entry_type` are one
    /// decision: nothing can be dropped without a warning kind, and
    /// nothing re-emitted can carry one.
    #[test]
    fn drop_predicate_and_warning_kind_agree() {
        use crate::entry::EntryType;
        for entry_type in [
            EntryType::File,
            EntryType::Directory,
            EntryType::Symlink,
            EntryType::HardLink,
            EntryType::Other,
        ] {
            assert_eq!(
                super::rewrite_drops_entry_type(entry_type),
                super::dropped_entry_kind(entry_type).is_some(),
                "predicate and warning kind disagree for {entry_type:?}"
            );
        }
        assert_eq!(
            super::dropped_entry_kind(EntryType::HardLink),
            Some(UnsupportedEntryKind::HardLink)
        );
    }
}

/// R0001-0011 / R0001-0071: the directory-side ancestor reservation and
/// the mode-before-input error precedence on the queued-add methods.
/// Inline module (not `modification/tests.rs`) so the cases live next to
/// the `record_dir` helper and the `add_*` gate they pin down.
#[cfg(test)]
mod namespace_and_mode_gate_tests {
    use super::{Archive, ArchiveError, NamespaceTracker, ops};
    use crate::test_utils::fixture;

    /// File `a` recorded first: the directory's ancestor sweep must
    /// catch the already-claimed file when `a/b` follows (R0001-0011).
    #[test]
    fn record_dir_rejects_directory_under_file_claimed_ancestor() {
        let mut ns = NamespaceTracker::default();
        ns.record_file(ops::COMMIT_CHANGES, "a").unwrap();
        let err = ns.record_dir(ops::COMMIT_CHANGES, "a/b").unwrap_err();
        assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("claimed as both a file and a directory"),
            "expected file/dir collision diagnostic, got: {msg}"
        );
    }

    /// Reverse order: directory `a/b` recorded first reserves `a` as a
    /// directory, so the later file `a` collides at its leaf key
    /// (R0001-0011 — before the fix this order was accepted too).
    #[test]
    fn record_file_rejects_file_at_ancestor_of_recorded_directory() {
        let mut ns = NamespaceTracker::default();
        ns.record_dir(ops::COMMIT_CHANGES, "a/b").unwrap();
        let err = ns.record_file(ops::COMMIT_CHANGES, "a").unwrap_err();
        assert!(matches!(err, ArchiveError::OperationBlocked { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("claimed as both a file and a directory"),
            "expected file/dir collision diagnostic, got: {msg}"
        );
    }

    /// The ancestor sweep must not break the coalescing contract: the
    /// same directory recorded twice (in both `dir` and `dir/` spelling),
    /// its own ancestor recorded explicitly, and a file nested under it
    /// all still succeed.
    #[test]
    fn record_dir_still_coalesces_repeats_and_nested_paths() {
        let mut ns = NamespaceTracker::default();
        ns.record_dir(ops::COMMIT_CHANGES, "a/b").unwrap();
        ns.record_dir(ops::COMMIT_CHANGES, "a/b/").unwrap();
        ns.record_dir(ops::COMMIT_CHANGES, "a").unwrap();
        ns.record_file(ops::COMMIT_CHANGES, "a/b/leaf.txt").unwrap();
        assert!(ns.contains_dir("a"));
        assert!(ns.contains_dir("a/b"));
    }

    /// A read-mode handle handed an invalid archive-internal path must
    /// report the mode failure, not `InvalidPath` — the precedence
    /// `remove_entry` already had (R0001-0071).
    #[test]
    fn add_variants_report_mode_error_before_path_validation() {
        let mut archive = Archive::open(fixture("test.zip")).unwrap();
        let from_data = archive.add_entry("../escape.txt", b"x").unwrap_err();
        let from_path = archive
            .add_entry_from_path("../escape.txt", std::path::Path::new("/nonexistent"))
            .unwrap_err();
        let from_reader = archive
            .add_entry_from_reader("../escape.txt", std::io::empty(), Some(0))
            .unwrap_err();
        let directory = archive.add_directory_entry("../escape").unwrap_err();

        for err in [from_data, from_path, from_reader, directory] {
            assert!(
                matches!(err, ArchiveError::OperationBlocked { .. }),
                "expected the mode gate to fire before path validation, got: {err:?}"
            );
            assert!(
                err.to_string().contains("Modify mode"),
                "expected the mode diagnostic, got: {err}"
            );
        }
    }
}
