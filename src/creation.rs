//! Archive creation operations
//!
//! This module provides methods for creating new archives and adding files to them.

mod manifest;

use crate::archive::{Archive, ArchiveBackend, ArchiveMode};
use crate::error::{ArchiveError, Result};
use crate::ffi::libarchive_wrapper::LibarchiveArchive;
use crate::ffi::zip_writer::ZipWriter;
use crate::format::ArchiveFormat;
use manifest::{ManifestKind, SourceManifest};
use once_cell::sync::OnceCell;
use std::path::Path;

/// Borrow the backend as a write-capable variant, or surface
/// `read_only_backend` if the handle is not in Write mode.
///
/// Centralizing the read-only error arm removes four copies of the same
/// `Unrar | SevenZ | ZipReader` boilerplate that used to appear on
/// every write operation.
pub(crate) enum WriteBackend<'a> {
    Zip(&'a mut ZipWriter),
    Libarchive(&'a mut LibarchiveArchive),
}

impl Archive {
    /// Borrow the backend as a [`WriteBackend`] only when the archive is
    /// actually open in `ArchiveMode::Write`. Read- and modify-mode
    /// handles surface `ReadOnlyBackend` up-front instead of falling
    /// through to `Libarchive`'s `WriteBackend::Libarchive` arm and then
    /// erroring late at `archive_entry_set_*` (R0070-0002).
    pub(crate) fn as_write(&mut self, op: &'static str) -> Result<WriteBackend<'_>> {
        if self.mode != ArchiveMode::Write {
            return Err(ArchiveError::read_only_backend(op));
        }
        match &mut self.backend {
            ArchiveBackend::ZipWriter(w) => Ok(WriteBackend::Zip(w)),
            ArchiveBackend::Libarchive(b) => Ok(WriteBackend::Libarchive(b)),
            _ => Err(ArchiveError::read_only_backend(op)),
        }
    }

    /// Non-borrowing variant of the [`Self::as_write`] mode gate.
    /// Lets write-side facade methods
    /// reject read- or modify-mode handles **before** they perform
    /// archive-internal-path validation or filesystem I/O on the
    /// caller's source path. The error variant matches what
    /// [`Self::as_write`] would surface so downstream callers cannot
    /// observe a difference.
    pub(crate) fn require_write_mode(&self, op: &'static str) -> Result<()> {
        if self.mode != ArchiveMode::Write {
            return Err(ArchiveError::read_only_backend(op));
        }
        // R0075-0005: an earlier failure on this handle (e.g. a
        // failed Drop finalize, or a backend-side write that the
        // facade marked poisoned) leaves the writer in undefined
        // state. Refuse further `add_*` calls so callers can't keep
        // appending entries to a broken stream.
        if self.write_poisoned {
            return Err(ArchiveError::operation_blocked(
                op,
                "Archive write handle is poisoned by an earlier failure; recreate the archive",
            ));
        }
        Ok(())
    }
}

/// Validate that `path` exists on the filesystem and is a regular file.
///
/// Rejects symlinks via `symlink_metadata` first so the
/// creation-side policy matches the rest of the archive create paths
/// (R0070-0022). The previous `metadata()` call followed symlinks
/// and silently archived the target's bytes under the link's name.
///
/// `op` labels the originating operation so error messages reference the
/// caller's intent (e.g. `"add_file"`, `"add_directory_recursive"`).
pub(crate) fn validate_file_path(path: &Path, op: &'static str) -> Result<()> {
    let symlink_meta =
        std::fs::symlink_metadata(path).map_err(|e| ArchiveError::io(op, path.to_path_buf(), e))?;
    if symlink_meta.file_type().is_symlink() {
        return Err(ArchiveError::operation_blocked(
            op,
            format!(
                "Refusing to archive symlink '{}': symlinks are not supported for archive creation",
                path.display()
            ),
        ));
    }
    if !symlink_meta.is_file() {
        return Err(ArchiveError::invalid_path(
            path.to_string_lossy().as_ref(),
            format!("{op}: path is not a regular file"),
        ));
    }
    Ok(())
}

/// Validate that `path` exists on the filesystem and is a directory.
///
/// Rejects symlinked roots via `symlink_metadata` (R0070-0022 /
/// R0070-0048): the *root* itself can't be a symlink. The subsequent
/// recursive walk is configured with `follow_links(false)`, so it does
/// **not** descend into symlinked subdirectories either — directory
/// symlinks encountered during traversal are not followed.
pub(crate) fn validate_directory_path(path: &Path, op: &'static str) -> Result<()> {
    let symlink_meta =
        std::fs::symlink_metadata(path).map_err(|e| ArchiveError::io(op, path.to_path_buf(), e))?;
    if symlink_meta.file_type().is_symlink() {
        return Err(ArchiveError::invalid_path(
            path.to_string_lossy().as_ref(),
            format!("{op}: root must be a real directory (not a symlink to a directory)"),
        ));
    }
    if !symlink_meta.is_dir() {
        return Err(ArchiveError::invalid_path(
            path.to_string_lossy().as_ref(),
            format!("{op}: path is not a directory"),
        ));
    }
    Ok(())
}

impl Archive {
    /// Create a new archive
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
        mut options: crate::options::CompressionOptions,
    ) -> Result<Self> {
        // Route through the same gate
        // [`CompressionOptions::validate_for_format`] exposes so
        // password / split_size / can-create rejections are reported
        // by both preflight and the create call itself.
        options.validate_for_format()?;

        let path_buf = path.as_ref().to_path_buf();
        let format = options.format;

        // Pre-existence is detected by the writer constructors via
        // `OpenOptions::create_new(true)` (O_CREAT|O_EXCL on Unix,
        // CREATE_NEW on Windows), which surfaces `AlreadyExists`
        // atomically — no facade-level `exists()` race window.
        let backend = match format {
            ArchiveFormat::Zip => {
                // Use native Rust zip crate for ZIP creation
                let writer = ZipWriter::create(&path_buf, &mut options)?;
                ArchiveBackend::ZipWriter(Box::new(writer))
            }
            _ => {
                // Use libarchive for other formats
                let libarchive = LibarchiveArchive::create(&path_buf, format, &mut options)?;
                ArchiveBackend::Libarchive(Box::new(libarchive))
            }
        };

        Ok(Self {
            backend,
            path: path_buf,
            mode: ArchiveMode::Write,
            format,
            entry_cache: OnceCell::new(),
            modifications: None,
            mod_options: None,
            _backing_tempfile: None,
            _lock_file: None,
            write_namespace: Some(crate::modification::NamespaceTracker::default()),
            write_poisoned: false,
            finalized: false,
        })
    }

    /// Create a new ZIP archive (R0075-0081).
    ///
    /// Type-checked entrypoint that pairs with
    /// [`crate::ZipCompressionOptions`]. Fields that ZIP creation
    /// does not honour (`password`, `split_size`) are absent from
    /// the builder, so misconfigurations rejected at runtime by
    /// the legacy [`Self::create`] are caught at compile time.
    ///
    /// Functionally equivalent to
    /// `Self::create(path, opts.into())` — the lowering goes
    /// through the same writer construction path so any landed
    /// behavioural fix applies to both surfaces.
    pub fn create_zip(
        path: impl AsRef<Path>,
        opts: crate::options::ZipCompressionOptions,
    ) -> Result<Self> {
        Self::create(path, opts.into())
    }

    /// Create a new 7-Zip archive (R0075-0081).
    ///
    /// Type-checked entrypoint that pairs with
    /// [`crate::SevenZCompressionOptions`]. Encrypted 7-Zip creation is
    /// deferred behind an explicit opt-in that has not shipped
    /// (MADR-0027), so the builder exposes no password setter
    /// (R0081-0005) — there is no encrypted-creation state to reject here.
    pub fn create_seven_zip(
        path: impl AsRef<Path>,
        opts: crate::options::SevenZCompressionOptions,
    ) -> Result<Self> {
        Self::create(path, opts.into())
    }

    /// Create a new libarchive-backed archive (R0075-0081).
    ///
    /// Type-checked entrypoint that pairs with
    /// [`crate::LibarchiveCompressionOptions`]. The format is
    /// validated against [`ArchiveFormat::can_create`] inside
    /// `Self::create`; passing a non-libarchive-creatable format
    /// surfaces the same `OperationBlocked` as the legacy path.
    pub fn create_libarchive(
        path: impl AsRef<Path>,
        opts: crate::options::LibarchiveCompressionOptions,
    ) -> Result<Self> {
        if opts.format == ArchiveFormat::Zip {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::CREATE,
                "Archive::create_libarchive does not create ZIP archives; use Archive::create_zip or Archive::create",
            ));
        }
        Self::create(path, opts.into())
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
        const OP: &str = crate::error::ops::ADD_FILE_FROM_DATA;
        self.require_write_mode(OP)?;
        crate::security::validate_archive_internal_path(path)?;
        self.staged_add(
            OP,
            |ns| ns.record_file(OP, path),
            |backend| match backend {
                WriteBackend::Zip(w) => w.add_file_from_data(path, data),
                WriteBackend::Libarchive(b) => b.add_file_from_data(path, data),
            },
        )
    }

    /// Add a file to archive from filesystem path
    ///
    /// The file will be stored with its original filename. Per AD 0064
    /// (non-UTF-8 path policy — Option A), the source file name must
    /// be valid UTF-8: silent lossy substitution at this boundary is
    /// rejected so a non-UTF-8 source filename does not get truncated
    /// to a different archive entry name. Callers with non-UTF-8
    /// filenames should pass an explicit archive path via
    /// [`Self::add_file_from_path_as`].
    pub fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        // R0001-0070: gate the handle's mode before deriving/validating the
        // source filename, so a read- or modify-mode handle reports the mode
        // failure regardless of how the caller spelled the path. The label
        // matches the one `add_file_from_path_as` reports for the same
        // handle, so the delegated arm is indistinguishable.
        self.require_write_mode(crate::error::ops::ADD_FILE_FROM_PATH_AS)?;
        let fs_path = path.as_ref();
        let file_name_os = fs_path.file_name().ok_or_else(|| {
            ArchiveError::invalid_path(fs_path.to_string_lossy().as_ref(), "No filename")
        })?;
        let archive_path = file_name_os.to_str().ok_or_else(|| {
            ArchiveError::invalid_path(
                fs_path.to_string_lossy().as_ref(),
                "Source filename is not valid UTF-8 — pass an explicit name to add_file_from_path_as instead (AD 0064)",
            )
        })?;
        let archive_path = archive_path.to_string();
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
        const OP: &str = crate::error::ops::ADD_FILE_FROM_PATH_AS;
        self.require_write_mode(OP)?;
        crate::security::validate_archive_internal_path(archive_path)?;
        // Reject non-regular files (directories, sockets, FIFOs, …) at the
        // facade boundary so neither writer emits a regular-file header
        // before the type check fails.
        let fs_path_ref = fs_path.as_ref();
        validate_file_path(fs_path_ref, OP)?;
        // R0080-0035: refuse to ingest the writer's own output archive as
        // an input entry. Reading a file the writer is simultaneously
        // growing produces unbounded growth or a corrupt entry. Identity
        // is compared (dev+ino on Unix, canonical path elsewhere) so a
        // differently-spelled path to the same file is still caught.
        if same_file_as_output(&self.path, fs_path_ref) {
            return Err(ArchiveError::operation_blocked(
                OP,
                format!(
                    "Refusing to add the archive's own output file '{}' as an input entry (self-ingestion)",
                    fs_path_ref.display()
                ),
            ));
        }
        self.staged_add(
            OP,
            |ns| ns.record_file(OP, archive_path),
            |backend| match backend {
                WriteBackend::Zip(w) => w.add_file_from_path(fs_path_ref, archive_path),
                WriteBackend::Libarchive(b) => b.add_file_from_path(fs_path_ref, archive_path),
            },
        )
    }

    /// Add a directory entry to archive (without contents)
    ///
    /// A directory path is emitted at most once (R0081-0038): if an
    /// earlier add already staged this normalized path as a directory,
    /// the call is a no-op success rather than a duplicate
    /// central-directory record.
    pub fn add_directory(&mut self, path: &str) -> Result<()> {
        const OP: &str = crate::error::ops::ADD_DIRECTORY;
        self.require_write_mode(OP)?;
        crate::security::validate_archive_internal_path(path)?;
        // R0081-0038: skip re-emitting a directory whose normalized
        // path was already recorded. The namespace tracker coalesces
        // duplicate directory reservations, but the backend emit is
        // per-call, so without this guard `add_directory("d")` twice
        // would write two `d/` entries.
        if self
            .write_namespace
            .as_ref()
            .is_some_and(|ns| ns.contains_dir(path))
        {
            return Ok(());
        }
        self.staged_add(
            OP,
            |ns| ns.record_dir(OP, path),
            |backend| match backend {
                WriteBackend::Zip(w) => w.add_directory_entry(path),
                WriteBackend::Libarchive(b) => b.add_directory_entry(path),
            },
        )
    }

    /// Add a directory recursively to archive
    ///
    /// All files within the directory tree are added to the archive.
    ///
    /// # One observation of the source tree (OI-0080-005)
    ///
    /// The tree is walked **once**. That single walk builds a
    /// `SourceManifest` (a crate-internal type, so this name is not a
    /// link) — every entry with its path, kind, size,
    /// mtime, unix mode and (on Unix) inode identity — reserves each
    /// archive path in the facade's `write_namespace` tracker as it
    /// goes, and rejects symlinks / special files (R0080-0033) and the
    /// writer's own output archive found inside the tree (R0080-0036)
    /// before any byte is written. The write phase then emits *exactly*
    /// what the manifest holds, in walk order.
    ///
    /// Previously the facade pre-walked for path reservations and each
    /// backend walked the tree again to do the I/O, so the archived set
    /// came from the *second* observation: a file removed in between
    /// disappeared from the archive with no diagnostic, and a file
    /// created in between was written without ever being reserved.
    ///
    /// Each source is re-checked against its recorded observation
    /// immediately before it is handed to the backend, and the drift
    /// classes are reported separately so a caller restoring a backup
    /// can tell them apart:
    ///
    /// * gone — `Io` error with [`std::io::ErrorKind::NotFound`];
    /// * different size — `Corruption`, naming both byte counts and
    ///   whether the source grew or shrank;
    /// * different object at the path (kind or inode changed) —
    ///   `OperationBlocked` "was replaced";
    /// * rewritten in place at the same size — `OperationBlocked`
    ///   "was modified in place".
    ///
    /// A file that *appears* after the walk is excluded by design: the
    /// manifest is the pinned snapshot, and noticing an addition would
    /// need the second walk this design removes.
    ///
    /// # Failure atomicity
    ///
    /// R0075-0006 / R0080-0032 / R0001-0042: the reservations live in a
    /// namespace transaction, so a rejection during the walk (namespace
    /// conflict with an earlier `add_file_*` / `add_directory*` entry, a
    /// tree holding both a file at `a` and a directory at `a/`, a
    /// symlink, or the output archive) leaves the live tracker exactly
    /// as the previous adds left it. A failure *during* the write phase
    /// likewise releases the reservations, but entries already emitted
    /// stay in the output stream — as before, a partway recursive add
    /// leaves a partial archive that the caller should discard.
    pub fn add_directory_recursive(&mut self, path: impl AsRef<Path>) -> Result<()> {
        const OP: &str = crate::error::ops::ADD_DIRECTORY_RECURSIVE;
        self.require_write_mode(OP)?;
        let dir_path = path.as_ref();
        validate_directory_path(dir_path, OP)?;

        let output_path = self.path.clone();
        self.staged_add_planned(
            OP,
            |ns| {
                SourceManifest::build(dir_path, &output_path, OP, |entry| match entry.kind {
                    ManifestKind::Dir => ns.record_dir(OP, &entry.archive_path),
                    ManifestKind::File => ns.record_file(OP, &entry.archive_path),
                })
            },
            |backend, manifest| manifest.write_into(backend, OP),
        )
    }

    /// Run a create-side add as a namespace transaction: record the
    /// planned reservation(s) through `record` (so a duplicate /
    /// conflict is still rejected *before* any backend write), invoke
    /// the backend `write`, and undo the reservation(s) when either
    /// step fails (R0080-0031 / R0080-0032). A failed backend add
    /// self-poisons the writer; undoing the reservation means a
    /// same-path retry surfaces the backend poison error rather than a
    /// phantom duplicate for an entry that was never emitted. In
    /// read/modify mode `write_namespace` is `None` and the reservation
    /// step is skipped — those modes are already rejected by
    /// `require_write_mode` before reaching here.
    ///
    /// R0001-0042: the reservations land in the live tracker behind an
    /// undo journal instead of on a full clone of both path sets, so an
    /// add costs O(path depth) rather than O(namespace) and creating an
    /// archive with many entries is no longer quadratic in metadata
    /// bookkeeping.
    fn staged_add(
        &mut self,
        op: &'static str,
        record: impl FnOnce(&mut NamespaceTxn) -> Result<()>,
        write: impl FnOnce(WriteBackend<'_>) -> Result<()>,
    ) -> Result<()> {
        self.staged_add_planned(op, record, |backend, ()| write(backend))
    }

    /// [`Self::staged_add`] for an add whose reservation step also
    /// *produces* the plan the write step executes — the recursive add,
    /// whose one source walk both reserves the archive paths and builds
    /// the [`SourceManifest`] that is then written (OI-0080-005).
    ///
    /// Keeping the two steps in one transaction is what makes the walk
    /// single: `plan` observes the tree while it reserves, and `write`
    /// receives that observation instead of re-deriving it. The
    /// reservation and undo semantics are exactly
    /// [`Self::staged_add`]'s; that method is this one with `T = ()`.
    fn staged_add_planned<T>(
        &mut self,
        op: &'static str,
        plan: impl FnOnce(&mut NamespaceTxn) -> Result<T>,
        write: impl FnOnce(WriteBackend<'_>, T) -> Result<()>,
    ) -> Result<()> {
        let (planned, undo) = match self.write_namespace.take() {
            Some(live) => {
                let mut txn = NamespaceTxn::new(live);
                let staged = plan(&mut txn);
                let (mut live, undo) = txn.finish();
                match staged {
                    Err(e) => {
                        // A rejected reservation must leave the live tracker
                        // exactly as the previous adds left it, so a corrected
                        // retry is not blocked by a phantom entry.
                        undo.apply(&mut live);
                        self.write_namespace = Some(live);
                        return Err(e);
                    }
                    Ok(planned) => {
                        self.write_namespace = Some(live);
                        (planned, Some(undo))
                    }
                }
            }
            None => {
                // Unreachable through the public surface: every caller
                // passes `require_write_mode` first, and a Write-mode
                // handle always carries a tracker. Plan against a
                // throwaway tracker so the step still runs — and still
                // rejects — rather than skipping validation.
                let mut txn = NamespaceTxn::new(crate::modification::NamespaceTracker::default());
                (plan(&mut txn)?, None)
            }
        };
        let result = write(self.as_write(op)?, planned);
        if result.is_err()
            && let Some(undo) = undo
            && let Some(live) = self.write_namespace.as_mut()
        {
            undo.apply(live);
        }
        result
    }
}

/// Namespace transaction over the live write-mode tracker (R0001-0042).
///
/// Reservations are recorded straight into the live tracker — taken by
/// value for the duration of one add and handed back by
/// [`NamespaceTxn::finish`] — and every key the transaction *newly*
/// inserts is journalled, so a rejected reservation or a failed backend
/// write can be undone key by key. The previous design copied both path
/// sets before every add to obtain the same all-or-nothing property
/// (R0080-0031 / R0080-0032), which made an N-entry create quadratic.
struct NamespaceTxn {
    live: crate::modification::NamespaceTracker,
    undo: NamespaceUndo,
}

/// Keys a [`NamespaceTxn`] inserted into the live tracker, kept so they
/// can be removed again. Only keys that were *absent* when the
/// transaction recorded them are journalled, so an undo can never drop
/// a reservation an earlier add already committed.
#[derive(Default)]
struct NamespaceUndo {
    files: Vec<String>,
    dirs: Vec<String>,
}

impl NamespaceTxn {
    fn new(live: crate::modification::NamespaceTracker) -> Self {
        Self {
            live,
            undo: NamespaceUndo::default(),
        }
    }

    /// Reserve `path` as a file. Delegates to the shared modify-side
    /// rule set so the create-side gate cannot drift from it.
    fn record_file(&mut self, op: &'static str, path: &str) -> Result<()> {
        let mut probe = crate::modification::NamespaceTracker::default();
        probe.record_file(op, path)?;
        self.journal(probe);
        self.live.record_file(op, path)
    }

    /// Reserve `path` as a directory. See [`Self::record_file`].
    fn record_dir(&mut self, op: &'static str, path: &str) -> Result<()> {
        let mut probe = crate::modification::NamespaceTracker::default();
        probe.record_dir(op, path)?;
        self.journal(probe);
        self.live.record_dir(op, path)
    }

    /// Journal the keys `probe` — the same reservation recorded against
    /// an empty tracker, which is conflict-free by construction and so
    /// yields exactly the key set the reservation touches — would add to
    /// the live tracker. Keys already present belong to an earlier add
    /// and are skipped; journalling a key the live record then fails to
    /// insert is harmless, because the undo removes an absent key as a
    /// no-op.
    fn journal(&mut self, probe: crate::modification::NamespaceTracker) {
        for key in probe.file_paths {
            if !self.live.file_paths.contains(&key) {
                self.undo.files.push(key);
            }
        }
        for key in probe.dir_paths {
            if !self.live.dir_paths.contains(&key) {
                self.undo.dirs.push(key);
            }
        }
    }

    /// Hand the live tracker back together with the undo journal.
    fn finish(self) -> (crate::modification::NamespaceTracker, NamespaceUndo) {
        (self.live, self.undo)
    }
}

impl NamespaceUndo {
    /// Remove every key the transaction newly inserted, restoring the
    /// tracker to its pre-transaction state.
    fn apply(&self, live: &mut crate::modification::NamespaceTracker) {
        for key in &self.files {
            live.file_paths.remove(key);
        }
        for key in &self.dirs {
            live.dir_paths.remove(key);
        }
    }
}

/// Build the preflight rejection for a symlink / socket / FIFO / device
/// entry found during a recursive add. Mirrors the emission-side
/// messages the backends produce so the preflight and defense-in-depth
/// backend checks read consistently (R0080-0033).
fn reject_special_entry(op: &'static str, fs_path: &Path, is_symlink: bool) -> ArchiveError {
    if is_symlink {
        ArchiveError::operation_blocked(
            op,
            format!(
                "Refusing to archive symlink '{}': symlinks are not supported for archive creation",
                fs_path.display()
            ),
        )
    } else {
        ArchiveError::operation_blocked(
            op,
            format!(
                "Refusing to archive '{}': unsupported file type (sockets, FIFOs, and device files cannot be archived)",
                fs_path.display()
            ),
        )
    }
}

/// Compare an input file's identity with the writer's own output
/// archive so self-ingestion can be rejected (R0080-0035, R0080-0036).
///
/// Identity is resolved from the strongest primitive each platform
/// offers, so path aliasing (relative vs absolute, `..` segments,
/// symlinked parent directories) cannot smuggle the output back in:
///
/// - **Unix:** the `(device, inode)` pair.
/// - **Windows:** the volume serial number plus the 64-bit file index
///   (`nFileIndexHigh`/`nFileIndexLow`) from `GetFileInformationByHandle`,
///   which — like `(dev, ino)` — also matches a *hard-link alias* to the
///   output archive that canonical-path equality would miss
///   (OI-0081-005/R0081-0032).
/// - **Other platforms:** canonicalized-path equality. This is weaker —
///   it resolves symlinks but *not* hard links, so a hard-link alias to
///   the output is not detected.
///
/// A stat / open / query / canonicalize failure on either side means
/// identity cannot be proven — return `false` (fail-open, R0081-0033 as
/// designed) and let the normal open/read path surface any genuine I/O
/// error.
#[cfg(unix)]
fn same_file_as_output(output: &Path, candidate: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(output), std::fs::metadata(candidate)) {
        (Ok(out), Ok(cand)) => out.dev() == cand.dev() && out.ino() == cand.ino(),
        _ => false,
    }
}

#[cfg(windows)]
fn same_file_as_output(output: &Path, candidate: &Path) -> bool {
    use std::os::windows::io::AsRawHandle;

    // On-disk file identity: volume serial number + 64-bit file index.
    // Unlike canonical-path equality this also matches a hard-link alias
    // to the output archive (shared file record, distinct path),
    // mirroring the Unix `(dev, ino)` robustness (OI-0081-005/R0081-0032).
    // Field order/type must match the Win32 struct exactly.
    #[repr(C)]
    #[derive(Default)]
    #[allow(non_snake_case, non_camel_case_types)]
    struct BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: u32,
        ftCreationTime: [u32; 2],
        ftLastAccessTime: [u32; 2],
        ftLastWriteTime: [u32; 2],
        dwVolumeSerialNumber: u32,
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        nNumberOfLinks: u32,
        nFileIndexHigh: u32,
        nFileIndexLow: u32,
    }

    // Link to kernel32.dll GetFileInformationByHandle — function-local
    // extern block, mirroring `ffi::common::rename_noclobber`. Nonzero
    // return = success.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut core::ffi::c_void,
            out: *mut BY_HANDLE_FILE_INFORMATION,
        ) -> i32;
    }

    fn identity(path: &Path) -> Option<(u32, u32, u32)> {
        let file = std::fs::File::open(path).ok()?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `handle` is a live OS handle owned by `file` for the
        // whole call; `info` is a valid, fully-owned struct the callee
        // only writes into.
        let ok = unsafe {
            GetFileInformationByHandle(file.as_raw_handle() as *mut core::ffi::c_void, &mut info)
        };
        if ok == 0 {
            return None;
        }
        Some((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }

    // Fail-open (R0081-0033, as designed): any open/query failure on
    // either side means identity cannot be proven → `false`.
    match (identity(output), identity(candidate)) {
        (Some(out), Some(cand)) => out == cand,
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_file_as_output(output: &Path, candidate: &Path) -> bool {
    match (
        std::fs::canonicalize(output),
        std::fs::canonicalize(candidate),
    ) {
        (Ok(out), Ok(cand)) => out == cand,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryType;
    use crate::format::ArchiveFormat;
    use crate::options::CompressionOptions;
    use crate::options::WritableFormat;

    /// Reopen `path` and assert it holds exactly `expected` as regular
    /// files — same set of archive paths, `EntryType::File` for each, and
    /// byte-for-byte identical payloads (R0001-0085). Asserting only that
    /// `add_*` and `finish` returned `Ok` let an empty or corrupt output
    /// pass, so the writer tests verify the produced archive instead.
    ///
    /// Directory entries are ignored: the writers may or may not emit an
    /// implicit parent directory record for a nested path, which is not
    /// what these tests pin down.
    fn assert_archive_files(path: &Path, expected: &[(&str, &[u8])]) {
        let reader = Archive::open(path).unwrap();
        let entries = reader.list_files().unwrap();
        let mut got: Vec<(&str, EntryType)> = entries
            .iter()
            .filter(|e| e.entry_type != EntryType::Directory)
            .map(|e| (e.path.as_str(), e.entry_type))
            .collect();
        got.sort_by(|a, b| a.0.cmp(b.0));
        let mut want: Vec<(&str, EntryType)> = expected
            .iter()
            .map(|(p, _)| (*p, EntryType::File))
            .collect();
        want.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(
            got,
            want,
            "archive '{}' holds unexpected entries",
            path.display()
        );

        for (name, content) in expected {
            let data = reader.extract_to_memory(name).unwrap();
            assert_eq!(
                data.as_slice(),
                *content,
                "content mismatch for '{name}' in '{}'",
                path.display()
            );
        }
    }

    // ── Archive::create tests ──

    #[test]
    fn test_create_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Zip);
        assert_eq!(archive.mode, ArchiveMode::Write);
        assert!(archive.modifications.is_none());
    }

    #[test]
    fn test_create_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar");
        let options = CompressionOptions::for_writable(WritableFormat::TAR);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Tar);
        assert_eq!(archive.mode, ArchiveMode::Write);
    }

    #[test]
    fn test_create_tar_gz() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.gz");
        let options = CompressionOptions::for_writable(WritableFormat::TAR_GZIP);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarGzip);
    }

    #[test]
    fn test_create_tar_bz2() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.bz2");
        let options = CompressionOptions::for_writable(WritableFormat::TAR_BZIP2);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarBzip2);
    }

    #[test]
    fn test_create_tar_xz() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_create.tar.xz");
        let options = CompressionOptions::for_writable(WritableFormat::TAR_XZ);
        let archive = Archive::create(&path, options).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::TarXz);
    }

    // ── add_file_from_data tests ──

    #[test]
    fn test_add_file_from_data_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_add.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive
            .add_file_from_data("hello.txt", b"Hello, World!")
            .unwrap();
        archive.finish().unwrap();
        assert_archive_files(&path, &[("hello.txt", b"Hello, World!".as_slice())]);
    }

    #[test]
    fn test_add_file_from_data_tar() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_add.tar");
        let options = CompressionOptions::for_writable(WritableFormat::TAR);
        let mut archive = Archive::create(&path, options).unwrap();
        archive
            .add_file_from_data("hello.txt", b"Hello, World!")
            .unwrap();
        archive.finish().unwrap();
        assert_archive_files(&path, &[("hello.txt", b"Hello, World!".as_slice())]);
    }

    #[test]
    fn test_add_file_from_data_empty_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_empty.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("empty.txt", b"").unwrap();
        archive.finish().unwrap();
        assert_archive_files(&path, &[("empty.txt", b"".as_slice())]);
    }

    #[test]
    fn test_add_multiple_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_multi.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
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

        // R0001-0085: the entry count alone accepted an archive whose
        // paths or payloads were wrong — compare both.
        assert_archive_files(
            &path,
            &[
                ("file1.txt", b"Content 1".as_slice()),
                ("file2.txt", b"Content 2".as_slice()),
                ("subdir/file3.txt", b"Content 3".as_slice()),
            ],
        );
    }

    // ── add_file_from_path tests ──

    #[test]
    fn test_add_file_from_path() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        std::fs::write(&source, b"Source content").unwrap();

        let archive_path = temp.path().join("test_from_path.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_file_from_path(&source).unwrap();
        archive.finish().unwrap();
        // The entry keeps the source file's own name and bytes.
        assert_archive_files(
            &archive_path,
            &[("source.txt", b"Source content".as_slice())],
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_add_file_from_path_reports_mode_before_filename_validation() {
        use std::os::unix::ffi::OsStrExt;

        // R0001-0070: on a read-mode handle the mode failure must win over
        // the AD 0064 non-UTF-8 filename rejection, so the same operation
        // does not report a different primary failure per input spelling.
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("read_mode.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut writer = Archive::create(&archive_path, options).unwrap();
        writer.add_file_from_data("a.txt", b"a").unwrap();
        writer.finish().unwrap();

        let mut reader = Archive::open(&archive_path).unwrap();
        let bad_name = std::ffi::OsStr::from_bytes(b"bad\xFFname.txt");
        let err = reader
            .add_file_from_path(temp.path().join(bad_name))
            .unwrap_err();
        assert!(
            matches!(err, ArchiveError::ReadOnlyBackend { .. }),
            "expected the mode failure, got: {err}"
        );

        // A UTF-8 filename on the same handle reports the identical error.
        let err = reader
            .add_file_from_path(temp.path().join("plain.txt"))
            .unwrap_err();
        assert!(
            matches!(err, ArchiveError::ReadOnlyBackend { .. }),
            "expected the mode failure, got: {err}"
        );
    }

    #[test]
    fn test_add_file_from_path_as() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        std::fs::write(&source, b"Custom path content").unwrap();

        let archive_path = temp.path().join("test_custom_path.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive
            .add_file_from_path_as(&source, "custom/path.txt")
            .unwrap();
        archive.finish().unwrap();

        // Verify the custom path *and* the payload behind it.
        assert_archive_files(
            &archive_path,
            &[("custom/path.txt", b"Custom path content".as_slice())],
        );
    }

    // ── add_directory tests ──

    #[test]
    fn test_add_directory_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_dir.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
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
        let options = CompressionOptions::for_writable(WritableFormat::TAR);
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

    #[test]
    fn test_add_directory_dedups_duplicate_path() {
        // Repeating add_directory for the same normalized path (plus a
        // trailing-slash spelling) must emit a single directory entry
        // (R0081-0038).
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dedup_dir.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_directory("dup").unwrap();
        archive.add_directory("dup").unwrap();
        archive.add_directory("dup/").unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        let dup_count = entries
            .iter()
            .filter(|e| e.path.trim_end_matches('/') == "dup")
            .count();
        assert_eq!(
            dup_count, 1,
            "duplicate add_directory calls must emit a single entry, got {dup_count}"
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
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_directory_recursive(&src_dir).unwrap();
        archive.finish().unwrap();

        // Verify contents — entries should be rooted at the source
        // directory's own name (`src_dir/...`), matching libarchive's
        // recursive-create convention.
        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        assert!(
            entries.len() >= 2,
            "Expected at least 2 entries, got {}",
            entries.len()
        );
        assert!(
            entries.iter().any(|e| e.path == "src_dir/root.txt"),
            "expected 'src_dir/root.txt' under root-preserving recursive add, \
             got: {:?}",
            entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert!(
            entries
                .iter()
                .any(|e| e.path == "src_dir/subdir/nested.txt"),
            "expected 'src_dir/subdir/nested.txt' under root-preserving recursive add, \
             got: {:?}",
            entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    /// A recursive add must carry directory metadata for *every*
    /// directory in the tree, not only for empty leaves.
    ///
    /// This used to be unreachable from the facade: both backends walked
    /// the tree themselves and emitted a directory entry only where the
    /// walker reported `DirWalkEntry::is_leaf_dir`, so a *non-empty*
    /// directory contributed no entry at all and its mtime/mode were
    /// lost — extraction recreated it with whatever defaults the
    /// extractor picked. Under the single-walk manifest (OI-0080-005)
    /// the facade emits every recorded directory, carrying the mtime and
    /// mode the one walk observed, so the libarchive backends preserve
    /// them.
    ///
    /// ZIP now carries it too (OI-0001-010's remaining half):
    /// `ZipWriter::add_directory_entry_with_metadata` mirrors the
    /// libarchive method of the same name, and `Manifest::write_into`'s ZIP
    /// arm hands it the recorded mtime and mode instead of dropping both.
    /// `test_add_directory_recursive_emits_nonleaf_zip_directory` still
    /// pins that the *entry* exists; this covers what it carries.
    ///
    /// The tolerance differs by format and is not slack: tar stores
    /// whole-second mtimes, a ZIP local/central record stores the DOS
    /// timestamp with 2-second granularity. Demanding bit equality with the
    /// filesystem stamp would fail on a correct writer.
    fn assert_nonempty_directory_metadata_survives(
        format: ArchiveFormat,
        extension: &str,
        mtime_tolerance_secs: u64,
    ) {
        let temp = tempfile::tempdir().unwrap();

        let src_dir = temp.path().join("src_dir");
        let nonempty = src_dir.join("nonempty");
        std::fs::create_dir_all(&nonempty).unwrap();
        std::fs::write(nonempty.join("child.txt"), b"child payload").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&nonempty, std::fs::Permissions::from_mode(0o750)).unwrap();
        }
        let source_meta = std::fs::metadata(&nonempty).unwrap();

        let archive_path = temp.path().join(format!("dir_metadata.{extension}"));
        let options =
            CompressionOptions::try_new(format).expect("both callers pass a creatable format");
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_directory_recursive(&src_dir).unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        let dir_entry = entries
            .iter()
            .find(|e| {
                e.entry_type == EntryType::Directory
                    && e.path.trim_end_matches('/') == "src_dir/nonempty"
            })
            .unwrap_or_else(|| {
                panic!(
                    "non-empty directory 'src_dir/nonempty' has no entry, so it carries no \
                     metadata; entries: {:?}",
                    entries.iter().map(|e| &e.path).collect::<Vec<_>>()
                )
            });

        let stored = dir_entry
            .modified
            .expect("directory entry carries no mtime");
        let source_mtime = source_meta.modified().unwrap();
        let drift = stored
            .duration_since(source_mtime)
            .or_else(|_| source_mtime.duration_since(stored))
            .unwrap();
        assert!(
            drift.as_secs() <= mtime_tolerance_secs,
            "{format:?} directory mtime drifted by {drift:?} (tolerance {mtime_tolerance_secs}s): \
             stored {stored:?} vs source {source_mtime:?}"
        );

        #[cfg(unix)]
        assert_eq!(
            dir_entry.permissions,
            Some(0o750),
            "{format:?} directory mode was not preserved"
        );
    }

    #[test]
    fn test_add_directory_recursive_preserves_nonempty_directory_metadata() {
        assert_nonempty_directory_metadata_survives(ArchiveFormat::Tar, "tar", 1);
    }

    #[test]
    fn test_add_directory_recursive_preserves_nonempty_zip_directory_metadata() {
        assert_nonempty_directory_metadata_survives(ArchiveFormat::Zip, "zip", 2);
    }

    // ── finish tests ──

    #[test]
    fn test_finish_write_mode() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test_finish.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
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
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
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

        let options = CompressionOptions::for_writable(WritableFormat::TAR);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("data.txt", content).unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 1);

        let data = reader.extract_to_memory("data.txt").unwrap();
        assert_eq!(data, content);
    }

    // ── R0069-0052 / AD 0062 A.4: create-side namespace gate ──

    #[test]
    fn test_create_rejects_duplicate_file_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dup_file.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("dup.txt", b"first").unwrap();
        let err = archive
            .add_file_from_data("dup.txt", b"second")
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("duplicate") || msg.contains("twice"),
            "expected duplicate-path diagnostic, got: {msg}"
        );
    }

    #[test]
    fn test_create_rejects_directory_then_file_at_same_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dup_dir_file.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_directory("collide").unwrap();
        let err = archive
            .add_file_from_data("collide", b"shouldn't merge")
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file") && msg.contains("directory"),
            "expected file-vs-directory diagnostic, got: {msg}"
        );
    }

    #[test]
    fn test_create_rejects_file_under_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file_under_file.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("a.txt", b"leaf").unwrap();
        let err = archive
            .add_file_from_data("a.txt/sub.txt", b"under leaf")
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file") && msg.contains("directory"),
            "expected dir/file conflict diagnostic, got: {msg}"
        );
    }

    #[test]
    fn test_create_accepts_file_under_explicit_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ok_dir_with_file.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_directory("nested").unwrap();
        archive
            .add_file_from_data("nested/leaf.txt", b"under nested")
            .unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&path).unwrap();
        let entries = reader.list_files().unwrap();
        assert!(entries.iter().any(|e| e.path == "nested/leaf.txt"));
    }

    // ── R0001-0042: journalled namespace transaction ──

    #[test]
    fn test_rejected_add_keeps_earlier_reservations() {
        // The undo journal must remove only the keys the *failed* add
        // inserted. The first add's file key and the directory key it
        // reserved for the ancestor both have to survive every later
        // rejection, otherwise a duplicate or a file-vs-directory
        // collision would slip through on a retry (R0001-0042).
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("undo_keeps_reservations.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("dir/dup.txt", b"first").unwrap();

        // Rejected duplicate — the reservation it collided with stays.
        archive
            .add_file_from_data("dir/dup.txt", b"second")
            .unwrap_err();
        let err = archive
            .add_file_from_data("dir/dup.txt", b"third")
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("duplicate") || msg.contains("twice"),
            "expected the duplicate diagnostic to survive the rollback, got: {msg}"
        );

        // Rejected file-over-directory — the ancestor directory key the
        // first add reserved must still be a directory afterwards.
        archive.add_file_from_data("dir", b"clobber").unwrap_err();
        let err = archive.add_file_from_data("dir", b"clobber").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("file") && msg.contains("directory"),
            "expected the file-vs-directory diagnostic to survive the rollback, got: {msg}"
        );

        // Namespace rejections are pre-emission, so the writer is intact.
        archive.add_file_from_data("dir/other.txt", b"ok").unwrap();
        archive.finish().unwrap();
        assert_archive_files(
            &path,
            &[
                ("dir/dup.txt", b"first".as_slice()),
                ("dir/other.txt", b"ok".as_slice()),
            ],
        );
    }

    #[test]
    fn test_rejected_add_leaves_no_phantom_reservation() {
        // The mirror case: 'leaf' is already a file, so 'leaf/under.txt'
        // is rejected — but only *after* the leaf key was inserted, so
        // without the undo journal the path would be remembered as taken
        // and the retry would report a bogus duplicate instead of the
        // real file-vs-directory conflict (R0080-0031 / R0080-0032
        // preserved under the R0001-0042 journal).
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("undo_frees_own_keys.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&path, options).unwrap();
        archive.add_file_from_data("leaf", b"leaf").unwrap();

        let first = archive
            .add_file_from_data("leaf/under.txt", b"nope")
            .unwrap_err()
            .to_string();
        let retry = archive
            .add_file_from_data("leaf/under.txt", b"nope")
            .unwrap_err()
            .to_string();
        assert_eq!(
            first, retry,
            "a rejected add must not change how the same add is rejected next time"
        );
        assert!(
            !retry.contains("duplicate"),
            "expected the file-vs-directory diagnostic, not a phantom duplicate, got: {retry}"
        );

        archive.finish().unwrap();
        assert_archive_files(&path, &[("leaf", b"leaf".as_slice())]);
    }

    #[cfg(unix)]
    #[test]
    fn test_rejected_recursive_add_releases_every_staged_key() {
        // The pre-walk records the root directory and every file it
        // visits before the symlink rejection fires (children are walked
        // in sorted name order, so 'a.txt' precedes 'link.txt'). Every
        // one of those keys must be released, otherwise a corrected retry
        // collides with reservations for entries that were never emitted
        // (R0080-0032 preserved under the R0001-0042 journal).
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("a.txt"), b"data").unwrap();
        std::os::unix::fs::symlink(src_dir.join("a.txt"), src_dir.join("link.txt")).unwrap();

        let archive_path = temp.path().join("recursive_undo.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_directory_recursive(&src_dir).unwrap_err();

        // The staged directory key for 'tree' is gone: the path is free
        // to be claimed as a file.
        archive
            .add_file_from_data("tree", b"file-at-tree")
            .expect("the rejected walk must not keep 'tree' reserved as a directory");
        // ...and so is the file key the walk staged for 'tree/a.txt',
        // which now collides as a file-under-file rather than a duplicate.
        let msg = archive
            .add_file_from_data("tree/a.txt", b"nope")
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("file") && msg.contains("directory") && !msg.contains("duplicate"),
            "expected a file-vs-directory conflict, not a phantom duplicate, got: {msg}"
        );

        archive.finish().unwrap();
        assert_archive_files(&archive_path, &[("tree", b"file-at-tree".as_slice())]);
    }

    // ── R0080-0035 / R0080-0036: self-ingestion guard ──

    #[test]
    fn test_add_file_from_path_rejects_output_self_ingestion() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("self.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();

        // The output archive is a real regular file (created via O_EXCL),
        // so it passes the file-type checks and would otherwise be read
        // while the writer is growing it.
        let err = archive.add_file_from_path(&archive_path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("self-ingestion"),
            "expected self-ingestion rejection, got: {msg}"
        );

        // The rejection is pre-emission: the writer stays usable.
        archive.add_file_from_data("ok.txt", b"ok").unwrap();
        archive.finish().unwrap();
    }

    #[test]
    fn test_add_file_from_path_as_rejects_output_self_ingestion() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("self_as.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();

        let err = archive
            .add_file_from_path_as(&archive_path, "renamed.bin")
            .unwrap_err();
        assert!(
            err.to_string().contains("self-ingestion"),
            "expected self-ingestion rejection, got: {err}"
        );
    }

    #[test]
    fn test_recursive_create_rejects_output_inside_source_tree() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("keep.txt"), b"data").unwrap();

        // Destination archive lives inside the very tree being archived,
        // so the recursive walk would otherwise ingest the growing output.
        let archive_path = src_dir.join("out.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        let err = archive.add_directory_recursive(&src_dir).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("self-ingestion") || msg.contains("destination archive"),
            "expected output-in-tree rejection, got: {msg}"
        );
    }

    // ── R0080-0032 / R0080-0033: recursive special-file preflight ──

    #[cfg(unix)]
    #[test]
    fn test_recursive_create_rejects_symlink_in_preflight() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("a.txt"), b"data").unwrap();
        std::os::unix::fs::symlink(src_dir.join("a.txt"), src_dir.join("link.txt")).unwrap();

        let archive_path = temp.path().join("out.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        let err = archive.add_directory_recursive(&src_dir).unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink rejection, got: {err}"
        );

        // The rejection is a preflight failure: no entry was emitted and
        // the live namespace was never mutated, so the writer is still
        // usable and finishes to an archive holding only the later add
        // (R0080-0032 / R0080-0033).
        archive
            .add_file_from_data("only.txt", b"ok")
            .expect("writer must remain usable after a preflight rejection");
        archive.finish().unwrap();

        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "only.txt");
    }

    // ── OI-0080-005: single-walk manifest pinning ──

    /// ZIP emits an entry for a *non-empty* directory too, so the
    /// directory survives a round-trip even when every child is
    /// filtered out on extraction. The entry carries no mtime/mode yet —
    /// see the note on
    /// `test_add_directory_recursive_preserves_nonempty_directory_metadata`.
    #[test]
    fn test_add_directory_recursive_emits_nonleaf_zip_directory() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("src_dir");
        std::fs::create_dir_all(src_dir.join("nonempty")).unwrap();
        std::fs::write(src_dir.join("nonempty/child.txt"), b"child").unwrap();

        let archive_path = temp.path().join("nonleaf.zip");
        let options = CompressionOptions::for_writable(WritableFormat::ZIP);
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive.add_directory_recursive(&src_dir).unwrap();
        archive.finish().unwrap();

        let reader = Archive::open(&archive_path).unwrap();
        let entries = reader.list_files().unwrap();
        let dirs: Vec<&str> = entries
            .iter()
            .filter(|e| e.entry_type == EntryType::Directory)
            .map(|e| e.path.trim_end_matches('/'))
            .collect();
        assert!(
            dirs.contains(&"src_dir/nonempty"),
            "expected an entry for the non-empty directory, got dirs: {dirs:?}"
        );
        assert!(
            dirs.contains(&"src_dir"),
            "expected an entry for the source root, got dirs: {dirs:?}"
        );
    }

    /// Removing a recorded source *after* the walk and *before* its
    /// write is reported as a disappearance, not silently dropped.
    ///
    /// The progress callback is the hook: it fires per emitted entry, so
    /// deleting `z.txt` from it lands strictly between the manifest walk
    /// (which already recorded `z.txt`) and the write of that entry.
    #[test]
    fn test_recursive_add_reports_source_removed_mid_write() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("a.txt"), b"alpha").unwrap();
        std::fs::write(src_dir.join("z.txt"), b"zulu").unwrap();

        let doomed = src_dir.join("z.txt");
        let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
        options.progress = Some(Box::new(move |_processed: u64, _total: Option<u64>| {
            let _ = std::fs::remove_file(&doomed);
            std::ops::ControlFlow::Continue(())
        }));

        let archive_path = temp.path().join("removed_mid_write.zip");
        let mut archive = Archive::create(&archive_path, options).unwrap();
        let err = archive.add_directory_recursive(&src_dir).unwrap_err();

        match &err {
            ArchiveError::Io { source, .. } => assert_eq!(
                source.kind(),
                std::io::ErrorKind::NotFound,
                "a vanished source must stay matchable as NotFound, got: {err}"
            ),
            other => panic!("expected Io/NotFound for a removed source, got: {other:?}"),
        }
        assert!(err.to_string().contains("disappeared"), "got: {err}");
    }

    /// A recorded source that *grows* before its write is reported as a
    /// size change — a different event from the disappearance above, so
    /// a caller restoring a backup can tell them apart.
    #[test]
    fn test_recursive_add_reports_source_growth_mid_write() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("a.txt"), b"alpha").unwrap();
        std::fs::write(src_dir.join("z.txt"), b"zulu").unwrap();

        let growing = src_dir.join("z.txt");
        let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
        options.progress = Some(Box::new(move |_processed: u64, _total: Option<u64>| {
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&growing) {
                let _ = f.write_all(b"more");
            }
            std::ops::ControlFlow::Continue(())
        }));

        let archive_path = temp.path().join("grew_mid_write.zip");
        let mut archive = Archive::create(&archive_path, options).unwrap();
        let err = archive.add_directory_recursive(&src_dir).unwrap_err();

        match &err {
            ArchiveError::Corruption { details, .. } => assert!(
                details.contains("grew"),
                "expected the growth to be named, got: {details}"
            ),
            other => panic!("expected Corruption for a grown source, got: {other:?}"),
        }
    }

    /// The manifest pins the archived *set*: a file created after the
    /// walk is not archived, and its appearance is not an error. The
    /// parent directory's mtime moves when it is created, which is why
    /// directory mtime is deliberately not part of the drift check.
    #[test]
    fn test_recursive_add_ignores_sources_created_after_the_walk() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("tree");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("a.txt"), b"alpha").unwrap();

        let latecomer = src_dir.join("zz_late.txt");
        let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
        options.progress = Some(Box::new(move |_processed: u64, _total: Option<u64>| {
            let _ = std::fs::write(&latecomer, b"created mid-write");
            std::ops::ControlFlow::Continue(())
        }));

        let archive_path = temp.path().join("late_arrival.zip");
        let mut archive = Archive::create(&archive_path, options).unwrap();
        archive
            .add_directory_recursive(&src_dir)
            .expect("a source appearing after the walk must not fail the add");
        archive.finish().unwrap();

        assert!(
            src_dir.join("zz_late.txt").exists(),
            "the test's own hook did not run — it proves nothing"
        );
        let reader = Archive::open(&archive_path).unwrap();
        let paths: Vec<String> = reader
            .list_files()
            .unwrap()
            .iter()
            .map(|e| e.path.clone())
            .collect();
        assert!(
            paths.iter().any(|p| p == "tree/a.txt"),
            "expected the recorded file, got: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.contains("zz_late")),
            "a file created after the manifest walk must not enter the archive, got: {paths:?}"
        );
    }
}
