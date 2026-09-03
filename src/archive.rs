//! Archive handle and core operations

use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, Result};
use crate::ffi::libarchive_wrapper::LibarchiveArchive;
use crate::ffi::sevenz_wrapper::SevenZArchive;
#[cfg(feature = "rar-support")]
use crate::ffi::wrapper::UnrarArchive;
use crate::ffi::zip_wrapper::ZipArchive;
use crate::ffi::zip_writer::ZipWriter;
use crate::format::ArchiveFormat;
use crate::security::{Cap, ExtractionLimits};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

/// Access mode for archive
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveMode {
    Read,
    Write,
    Modify,
}

/// Internal archive backend.
///
/// Every variant holds a `Box` so the enum stays the size of a single
/// pointer regardless of which backend is active. Pattern matches bind
/// `&Box<T>` / `&mut Box<T>`; auto-deref lets the match arms call methods
/// directly on the inner value without explicit `&**x` gymnastics.
pub(crate) enum ArchiveBackend {
    #[cfg(feature = "rar-support")]
    Unrar(Box<UnrarArchive>),
    SevenZ(Box<SevenZArchive>),
    ZipWriter(Box<ZipWriter>),
    ZipReader(Box<ZipArchive>),
    Libarchive(Box<LibarchiveArchive>),
}

/// How an archive that does **not** start at byte zero of the file the
/// caller named is reached (ticket `1ddc37ec`).
///
/// An SFX file is `[stub program][archive bytes]`. Two ways to hand the
/// archive bytes to a backend:
///
/// * [`Self::Staged`] — copy `file_len - offset` bytes into a tempfile
///   and open that. Works for every backend, and is why the AD 0040
///   payload ceiling exists: the copy is a disk write driven by
///   user-supplied input.
/// * [`Self::InPlace`] — hand the backend the original file and let it
///   start reading at `offset`. Nothing is copied, so no ceiling
///   applies.
///
/// The public projection of this distinction is [`PayloadAccess`],
/// returned by [`Archive::payload_access`].
pub(crate) enum PayloadSource {
    /// Payload copied into a tempfile (AD 0040 ceiling applied). The
    /// [`tempfile::TempPath`] deletes the copy when the `Archive` drops.
    Staged(tempfile::TempPath),
    /// Payload read directly out of the caller's file, starting at
    /// `offset`. No bytes were copied and no ceiling was consulted.
    InPlace {
        /// Byte offset within the caller's file where the archive
        /// begins. Kept so size accounting that must exclude the stub —
        /// [`Archive::payload_size_for_ratio`], the compression-ratio
        /// denominator — stays on the payload rather than silently
        /// widening to `stub + payload` (R0069-0006).
        offset: u64,
    },
}

/// Whether an [`Archive`]'s bytes were copied to a tempfile before the
/// backend saw them, or are being read straight out of the file the
/// caller named.
///
/// This is the caller-visible half of the AD 0040 payload ceiling: the
/// ceiling bounds a *copy*, so it binds on [`Self::Staged`] and cannot
/// bind on [`Self::InPlace`]. Read it with
/// [`Archive::payload_access`] when the distinction matters — a caller
/// that budgets temp-volume space, or one that wants to know whether
/// [`crate::ExtractionLimits::max_sfx_payload_size`] had anything to
/// say about the handle it just got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadAccess {
    /// No copy was made. Either an ordinary archive opened at offset
    /// zero, or an offset/SFX open that the backend could serve from
    /// the original file. The AD 0040 staging ceiling did not apply.
    ///
    /// # What the staged path was quietly buying
    ///
    /// A staged handle read a private copy, so it was **immune to a
    /// concurrent rewrite of the source** — stronger than an ordinary
    /// `Archive::open` has ever been. An in-place handle reads the live
    /// file, so that extra strength is what this trades away. Three
    /// things bound the exposure: the open-time identity binding refuses
    /// a handle whose file was replaced (OI-0001-002), the payload window
    /// freezes its length so post-open appends are invisible, and
    /// per-entry CRC verification still catches mid-read tampering at
    /// extraction. The net position is that an in-place SFX open is
    /// *exactly* as exposed as `Archive::open("file.zip")` is today —
    /// not a new class of exposure, but not the copy's guarantee either.
    ///
    /// Two more consequences worth knowing before relying on this:
    ///
    /// - **The ceiling doubled as a size refusal.** A caller using
    ///   [`crate::ExtractionLimits::max_sfx_payload_size`] as a *size
    ///   gate* rather than a disk-budget gate loses it here, because
    ///   there is no copy for it to bound. If you need a size refusal,
    ///   check the payload size yourself.
    /// - **The outer file stays open** for the backend's lifetime. On
    ///   Windows that blocks deleting or replacing the SFX while the
    ///   [`Archive`] lives, where a staged handle released it after the
    ///   copy. Again identical to how an ordinary open behaves on the
    ///   same OS.
    InPlace,
    /// The archive bytes were copied into a tempfile before opening,
    /// under the AD 0040 staging ceiling
    /// ([`crate::ExtractionLimits::max_sfx_payload_size`]). The copy is
    /// removed when the `Archive` drops.
    Staged,
}

/// Handle to an opened archive file
///
/// Provides unified access to inspection, extraction, creation, and modification
/// operations across all supported formats.
///
/// # Snapshot semantics — frozen at first observation (AD 0065)
///
/// An `Archive` is a **snapshot** of the file as observed on the
/// first listing/extraction call. Every read backend memoises its
/// listing on first use; subsequent operations on the same handle
/// return the cached view, even if the file is rewritten on disk in
/// the meantime:
///
/// - **ZipReader (the sole ZIP backend — AD 0007 collapse)** caches an
///   open `zip::ZipArchive<File>` after the first call, freezing the
///   central directory, and memoises the parsed `Vec<ArchiveEntry>`.
///   Both encrypted and unencrypted ZIPs use this backend.
/// - **7z** caches the parsed table-of-contents on first call.
/// - **UnRAR** keeps the FFI handle alive between operations.
/// - **Libarchive** memoises the entry list on first
///   `list_files_metadata_only()` call (AD 0065). Per-byte payload
///   reads still reopen the file because libarchive's read iterator
///   is one-shot and cannot be rewound, but the metadata snapshot is
///   stable.
///
/// Mutations through this same handle in `ArchiveMode::Modify` do not
/// become visible to other readers until
/// [`Archive::commit_changes`] swaps the file in place atomically.
/// Callers that need a guaranteed-fresh view after an external rewrite
/// drop the handle and call [`Archive::open`] again — the cache is
/// per-handle, not per-path.
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
/// - RAR: UnRAR has process-wide global state. Every UnRAR FFI call is
///   serialised through an internal `UNRAR_LOCK` mutex so the SDK
///   itself stays safe in the presence of multiple `Archive` handles —
///   **but this is a safety lock, not a parallel-throughput primitive**
///   (R0070-0091). Two threads each holding a RAR `Archive` will run
///   one at a time, not concurrently, even when both archives are
///   different files. Use the ZIP / 7z / TAR backends for genuine
///   parallel reads of independent archives.
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
    /// Cached entry list (Phase 1: zero-cost repeated access).
    /// Shares storage with the backend's own listing cache via `Arc`
    /// (OI-0065-003), so the listing is pinned once per handle instead
    /// of once per cache layer.
    pub(crate) entry_cache: OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// Modification tracker (for Modify mode)
    pub(crate) modifications: Option<crate::modification::ModificationTracker>,
    /// Modification options (Modify mode only). `None` means defaults are used.
    pub(crate) mod_options: Option<crate::modification::ModificationOptions>,
    /// How the embedded payload of an archive opened at a non-zero
    /// offset is reached — see [`PayloadSource`] and
    /// [`Archive::open_at_offset`]. `None` for an archive opened from a
    /// real on-disk file at offset zero, which is the overwhelming
    /// majority of handles.
    ///
    /// [`PayloadSource::Staged`] owns the [`tempfile::TempPath`] holding
    /// the copied payload; it is dropped with the `Archive`, removing
    /// the copy. [`PayloadSource::InPlace`] owns no file at all — the
    /// backend reads the original file starting at the recorded offset.
    ///
    /// `path` retains the caller-facing outer path so [`Archive::path`] and
    /// multipart-sibling discovery still see the original SFX/installer
    /// filename. Internal reopens (password-aware extraction, ratio
    /// preflight) must route through [`Archive::source_path_for_reopen`]
    /// so the staged payload is used as the actual archive source —
    /// reopening from `path` would target the outer executable instead
    /// (R0071-0001). For an in-place handle `path` *is* the source, so
    /// that helper returns it unchanged.
    ///
    /// **Field name.** Retained as `_backing_tempfile` even though the
    /// type is no longer a bare tempfile: `src/creation.rs` and
    /// `src/modification.rs` build `Archive` with struct literals that
    /// name this field (`_backing_tempfile: None`, which still compiles
    /// against the `Option`), and renaming it there is outside this
    /// change's file ownership. Renaming to `payload_source` is a
    /// mechanical follow-up.
    pub(crate) _backing_tempfile: Option<PayloadSource>,
    /// Advisory file lock held across a Modify session (MADR-0009, MADR-0016).
    /// The `File` handle holds an exclusive `flock`/`LockFileEx` for the
    /// archive path and releases on drop. `None` outside Modify mode.
    pub(crate) _lock_file: Option<std::fs::File>,
    /// Write-mode namespace tracker (R0069-0052, AD 0062 A.4).
    /// `Some(...)` only when the handle is in Write mode; populated by
    /// every `add_file_*` / `add_directory*` call so duplicate paths
    /// or directory-vs-file conflicts surface a typed
    /// `OperationBlocked` *before* the writer backend produces a
    /// format-specific error. `None` outside Write mode.
    pub(crate) write_namespace: Option<crate::modification::NamespaceTracker>,
    /// Set to `true` once a write-mode operation has failed in a way
    /// that leaves the backend in an undefined state. Subsequent
    /// `add_*` calls return [`ArchiveError::OperationBlocked`] so
    /// callers can't keep stacking writes on a broken handle. Drop
    /// uses this flag to suppress its silent-finalize attempt — see
    /// the [`Drop`] impl below for the durability contract
    /// (R0075-0005).
    pub(crate) write_poisoned: bool,
    /// Set to `true` once `finish()` / `close()` runs successfully
    /// for a Write- or Modify-mode handle. The `Drop` impl checks
    /// this flag and, when `false` *and* the handle is in Write
    /// mode, emits a structured `eprintln!` warning (write paths
    /// must call `finish()` explicitly per R0075-0005). The flag is
    /// also flipped by `commit_changes()` for Modify-mode handles
    /// since their durable point lives in the rename swap.
    pub(crate) finalized: bool,
}

// `Archive` is `Send` by auto-derivation: every field, including every
// `ArchiveBackend` variant, is `Send`. There is deliberately no blanket
// `unsafe impl Send for Archive` here (R0079-0015) — the only `unsafe
// impl Send` in the ownership chain sit next to the raw FFI handles
// they justify: `UnrarArchive` (`src/ffi/wrapper.rs`) and
// `LibarchiveArchive` (`src/ffi/libarchive_wrapper.rs`). This satisfies
// FR-020/FR-021: concurrent operations on **different** Archive
// instances from different threads, but not concurrent operations on
// the **same** Archive instance.

// R0075-0004 / R0079-0015: compile-time anchor for the Send claim
// above. Because `Send` is auto-derived (no blanket unsafe impl),
// introducing a non-Send field anywhere in `Archive` or its backends
// (e.g. `Rc<…>`, a raw pointer outside the two narrowly-justified
// wrapper impls) will fail this assertion at compile time before
// reviewers have to spot the drift. Using a `const _` typed as
// `fn()` so the body is type-checked but never executed and the
// generic `assert_send` is reachable for monomorphization.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Archive>();
};

// Archive is NOT Sync - it cannot be safely shared between threads via &Archive
// because the underlying FFI operations are not reentrant.

/// AD 0040 staging ceiling for the SFX entry points that accept no
/// [`ExtractionLimits`] — [`Archive::open_sfx`],
/// [`Archive::open_with_sfx_progress`], [`Archive::open_at_offset`],
/// and the encrypted-SFX staging path behind
/// [`Archive::open_encrypted`].
///
/// It bounds a *copy*, so it is consulted only on the staging path.
/// An open that reads the payload in place
/// ([`Archive::try_open_in_place`]) never reaches
/// [`stage_sfx_payload`] and is admitted regardless of this value or
/// of a caller-supplied one — [`Archive::payload_access`] is how a
/// caller tells the two apart (ticket `1ddc37ec`).
///
/// This is the *default* of
/// [`ExtractionLimits::max_sfx_payload_size`], read through the
/// `crate::sfx::limits` alias so the SFX size-relationship
/// documentation keeps one home. It is **not** the gate: the gate is
/// the `max_payload` argument [`stage_sfx_payload`] receives, which
/// the `*_with_limits` entry points source from the caller's limits.
/// `default_sfx_cap_matches_extraction_limits_default` pins the two
/// values together.
const DEFAULT_SFX_PAYLOAD_CAP: Cap = Cap::Limited(crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE);

/// Stage `payload_size = file_len - offset` bytes from `path_ref` into
/// a tempfile so the returned [`tempfile::TempPath`] can be re-opened
/// by a backend that expects offset-zero input. Shared by
/// [`Archive::open_at_offset_with_format_hint`] and
/// [`Archive::open_sfx_payload_for_encrypted`] so the staging policy
/// lives in one place.
///
/// Reached only when no backend can read the payload where it already
/// lies — see [`Archive::try_open_in_place`], which is tried first and
/// bypasses this function (and therefore this ceiling) entirely.
///
/// `max_payload` is the AD 0040 staging ceiling **as the caller
/// configured it** — [`ExtractionLimits::max_sfx_payload_size`], which
/// defaults to [`crate::security::DEFAULT_MAX_SFX_PAYLOAD_SIZE`] (the
/// value `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE` aliases). This
/// parameter exists because reading that constant here ignored a
/// caller who *lowered* the cap: the limits-free entry points
/// ([`Archive::open_sfx`], [`Archive::open_with_sfx_progress`],
/// [`Archive::open_at_offset`]) pass the default, while their
/// `*_with_limits` siblings pass the caller's value. Compared with
/// [`Cap::exceeded_by`] so [`Cap::Unlimited`] means "no ceiling"
/// rather than `u64::MAX`.
///
/// When `expected_identity` is `Some`, the copy-source open is
/// revalidated against that detection-time [`ReadFileIdentity`] before
/// any bytes are copied (OI-0081-001); `None` skips the check for
/// callers with no detection open to bind to. R0001-0002: the SFX
/// callers pass the identity `detect_sfx` captured from its *own*
/// descriptor, so the binding covers the whole detect→stage window
/// rather than starting at a re-stat taken after detection closed.
///
/// When `progress` is `Some`, the callback is invoked after each
/// chunk-write with the running cumulative byte count. Returning
/// `false` from the callback aborts the copy and surfaces
/// [`ArchiveError::Cancelled { operation: "sfx_staging" }`](ArchiveError::Cancelled)
/// (R0075-0003).
fn stage_sfx_payload(
    path_ref: &Path,
    offset: u64,
    max_payload: Cap,
    format_hint: Option<ArchiveFormat>,
    expected_identity: Option<ReadFileIdentity>,
    prefix: &str,
    mut progress: Option<&mut crate::options::SfxStagingProgress>,
) -> Result<tempfile::TempPath> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};

    let mut source = File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref, e))?;
    let meta = source
        .metadata()
        .map_err(|e| ArchiveError::io("stat", path_ref, e))?;
    // OI-0081-001: `detect_sfx` verified the payload offset from a
    // *separate* open of the pathname; this `File::open` is a second one.
    // When the caller passed a detection-time identity, bind the copy
    // source to it by comparing this just-opened handle's identity (an
    // fstat, so it names the exact inode we will read) — a same-offset
    // replacement between probe and staging is refused before any bytes
    // are copied. The R0075-0002 take+size assertion below already
    // catches a size change mid-copy; this adds the inode check for the
    // same-size-replacement case (Unix). Callers staging a raw
    // caller-supplied offset (no detection open to bind to, e.g.
    // `open_at_offset`) pass `None`.
    if let Some(expected) = expected_identity {
        let found = read_file_identity(&meta);
        if found != expected {
            return Err(ArchiveError::operation_blocked(
                "open_sfx",
                format!(
                    "SFX payload source identity changed between detection and \
                     staging of {} (detected {expected:?}, now {found:?}); \
                     aborting rather than staging bytes that were never detected",
                    path_ref.display()
                ),
            ));
        }
    }
    let file_len = meta.len();
    if offset >= file_len {
        return Err(ArchiveError::format(
            None,
            format!("offset {offset} is at or beyond end of file ({file_len} bytes)"),
        ));
    }
    let payload_size = file_len - offset;
    if max_payload.exceeded_by(payload_size) {
        return Err(ArchiveError::format(
            None,
            format!(
                "payload size {payload_size} bytes exceeds maximum {} bytes",
                max_payload.get()
            ),
        ));
    }
    // Fail fast when the temp volume observably cannot hold the copy.
    //
    // The worst failure mode of staging is writing multiple GiB and only
    // then hitting ENOSPC, which costs the caller the whole copy and tells
    // them nothing until it is spent. One `statvfs` answers it up front.
    //
    // Deliberately ADVISORY, in both directions. If the question cannot be
    // answered — an exotic filesystem, a sandbox, a permission problem —
    // staging proceeds, because refusing to stage because the *check*
    // failed would turn working setups into regressions. And a mid-copy
    // ENOSPC still surfaces exactly as it does today, an
    // `ArchiveError::io("copy", ..)` with the `NamedTempFile` cleaning
    // itself up on drop; the space vanishing between this check and the
    // write is accepted for the same reason. This is a courtesy, not a
    // gate.
    //
    // `OperationBlocked` rather than a synthesised `Io` with
    // `ErrorKind::StorageFull`: there is no OS error here to wrap, and
    // faking one would attribute a refusal the crate made to a syscall
    // that never failed.
    #[cfg(any(unix, windows))]
    {
        // What `tempfile::Builder` picks by default, so this measures the
        // volume the copy will actually land on.
        let staging_dir = std::env::temp_dir();
        if let Ok(available) = fs4::available_space(&staging_dir)
            && available < payload_size
        {
            return Err(ArchiveError::operation_blocked(
                "sfx_staging",
                format!(
                    "staging the {payload_size}-byte SFX payload needs more free space than \
                     {} has available ({available} bytes); the payload can be read in place \
                     without staging for archive formats that support it — see \
                     Archive::payload_access",
                    staging_dir.display()
                ),
            ));
        }
    }

    source
        .seek(SeekFrom::Start(offset))
        .map_err(|e| ArchiveError::io("seek", path_ref, e))?;

    let suffix = match format_hint {
        Some(fmt) => std::borrow::Cow::Borrowed(fmt.suffix()),
        None => crate::format::stage_suffix_for(path_ref),
    };
    let mut temp = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix.as_ref())
        .tempfile()
        .map_err(|e| ArchiveError::io("create_tempfile", path_ref, e))?;
    {
        let sink = temp.as_file_mut();
        // R0075-0002: bound the copy at exactly `payload_size` so a
        // concurrently-appended SFX cannot push the staged payload
        // past the size that the ceiling check (above) just accepted.
        // `Read::take` upper-bounds the byte count; verifying the
        // post-copy total catches both growth and a truncation that
        // landed mid-copy.
        let mut bounded = (&mut source).take(payload_size);
        let mut buf = [0u8; 64 * 1024];
        let mut copied: u64 = 0;
        loop {
            let n = bounded
                .read(&mut buf)
                .map_err(|e| ArchiveError::io("copy", path_ref, e))?;
            if n == 0 {
                break;
            }
            sink.write_all(&buf[..n])
                .map_err(|e| ArchiveError::io("copy", path_ref, e))?;
            copied += n as u64;
            if let Some(p) = progress.as_deref_mut()
                && !p.emit(copied)
            {
                return Err(ArchiveError::Cancelled {
                    operation: "sfx_staging",
                });
            }
        }
        sink.flush()
            .map_err(|e| ArchiveError::io("flush", path_ref, e))?;
        if copied != payload_size {
            return Err(ArchiveError::format(
                None,
                format!(
                    "SFX payload size changed during staging: expected {} bytes, copied {}",
                    payload_size, copied
                ),
            ));
        }
    }
    Ok(temp.into_temp_path())
}

/// Stable identity of a file, used across the read paths to bind a
/// detection/probe open to the later backend or staging open of the
/// same pathname (OI-0081-001; originally the SFX-stub-only
/// `StubFileIdentity` of R0081-0017). On Unix this is `(dev, ino, len)`;
/// on other platforms only the length is available, so the guard
/// degrades to a size-drift check — a same-length replacement is not
/// caught off Unix. Mirrors the `MetadataExt`-based identity pattern
/// already used for modification locking and UnRAR recovery
/// revalidation.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReadFileIdentity {
    inode: crate::fs_identity::InodeId,
    len: u64,
}

#[cfg(not(unix))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReadFileIdentity {
    len: u64,
}

/// Build a [`ReadFileIdentity`] from metadata that was already read from
/// an *open handle*. R0001-0002 made this `pub(crate)`: SFX detection
/// snapshots the identity from its own descriptor (an `fstat`, so it names
/// the exact inode detection read) instead of the caller re-`stat`ing the
/// pathname after detection has closed it.
#[cfg(unix)]
pub(crate) fn read_file_identity(meta: &std::fs::Metadata) -> ReadFileIdentity {
    ReadFileIdentity {
        inode: crate::fs_identity::InodeId::from_metadata(meta),
        len: meta.len(),
    }
}

#[cfg(not(unix))]
pub(crate) fn read_file_identity(meta: &std::fs::Metadata) -> ReadFileIdentity {
    ReadFileIdentity { len: meta.len() }
}

/// Stat `path` and snapshot its [`ReadFileIdentity`] (OI-0081-001).
/// The read entry points call this at the detection/probe open so a
/// later reopen of the same pathname can be revalidated against it.
fn capture_read_identity(path: &Path) -> Result<ReadFileIdentity> {
    std::fs::metadata(path)
        .map(|m| read_file_identity(&m))
        .map_err(|e| ArchiveError::io("stat", path, e))
}

/// Re-stat `path` and fail closed if its identity drifted from
/// `expected` (OI-0081-001). Returns [`ArchiveError::OperationBlocked`]
/// labelled `op` on drift, mirroring the modify-side
/// `revalidate_locked_identity` wording (DCR-007). A same-pathname
/// replacement between the detection open and the backend open is
/// refused here rather than handing the backend bytes that were never
/// detected.
///
/// Residual window: this shrinks but does not close the check-to-use
/// gap. The backends open `path` by name internally with no descriptor
/// hand-off, so an inode swap between this revalidation and the
/// backend's own open is still possible (DCR-007 pattern; MADR-0009
/// cooperating-process threat model — plain read has no third-inode
/// escalation, so this stays within the accepted read-side class). An
/// owned-fd hand-off (`archive_read_open_fd`) was evaluated to close
/// this window and rejected as unworkable for the libarchive backend on
/// a macOS-first-class target; see [`crate::fs_identity`] and the
/// DCR-007 amendment (2026-07-22, R0081 I6). The residual is therefore
/// accepted-permanent for this architecture, not a pending fd follow-up.
fn revalidate_read_identity(
    op: &'static str,
    path: &Path,
    expected: ReadFileIdentity,
) -> Result<()> {
    let found = capture_read_identity(path)?;
    if found != expected {
        return Err(ArchiveError::operation_blocked(
            op,
            format!(
                "archive file identity changed while opening {} \
                 (detected {expected:?}, now {found:?}); aborting rather \
                 than handing the backend bytes that were never detected",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// Combined diagnostic for the executable-extension SFX fallback
/// (R0076-0074 / R0076-0075): when primary format detection fails AND
/// the SFX probe also fails, the two causes are independent —
/// surfacing only one silently discards what the other stage learned.
/// `Archive::open` used to drop the SFX failure while
/// `Archive::open_encrypted` dropped the detection failure; both now
/// report the pair through this helper.
fn sfx_fallback_failure(
    detect_err: &ArchiveError,
    sfx_cause: impl std::fmt::Display,
) -> ArchiveError {
    ArchiveError::format(
        None,
        format!(
            "format detection failed ({detect_err}); SFX fallback for \
             executable extension also failed ({sfx_cause})"
        ),
    )
}

impl Archive {
    /// Shared-ownership listing threaded with a parse-time entry-count budget
    /// (OI-0080-003). Mirrors [`Self::list_files_shared_budgeted`] but bounds the first
    /// materialization: `budget = Some(n)` aborts the backend parse before
    /// allocating past `n` entries. AD 0065: the budget applies only while the
    /// `entry_cache` OnceCell is empty — a later call that hits the cache
    /// returns the cached `Arc` unchanged. `get_or_try_init` stores only on
    /// `Ok`, so a budgeted parse that aborts does NOT poison the cache; a
    /// later unbudgeted (or larger-budget) call can still populate it.
    pub(crate) fn list_files_shared_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.entry_cache
            .get_or_try_init(|| self.list_entries_budgeted(crate::error::ops::LIST_FILES, budget))
            .map(std::sync::Arc::clone)
    }

    /// Budgeted, limits-carrying listing (OI-0080-003): the
    /// extraction/inspection paths that carry
    /// [`crate::security::ExtractionLimits`] call this with
    /// `Some(limits.max_entry_count)` so parsing a crafted, over-limit record
    /// count aborts before the full `Vec<ArchiveEntry>` is allocated. The
    /// post-materialization gate ([`crate::security::check_extraction_safe`])
    /// still runs afterwards as the per-operation policy check.
    ///
    /// `budget = None` is exactly the pre-existing unbudgeted behavior. Since
    /// the AD 0007 collapse removed the memory-mapped ZIP reader, there is no
    /// backend-specific mmap routing left; every backend shares the cached
    /// listing snapshot (AD 0065 / OI-0065-003).
    pub(crate) fn list_files_for_limits_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.list_files_shared_budgeted(budget)
    }
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
            _lock_file: None,
            write_namespace: None,
            write_poisoned: false,
            // Read-mode archives have nothing to finalize; pretend
            // they're already-finalized so the Drop warning never
            // fires for them.
            finalized: true,
        }
    }

    /// Open an existing archive for reading.
    ///
    /// Detection is content-first: magic bytes win when present (a
    /// RAR file renamed `.zip` opens as RAR), and the extension
    /// promotes ambiguous bare-codec magic to its compound-tar
    /// variant (`.tar.gz` ↔ Gzip → TarGzip). When no magic matches,
    /// a narrow extension fallback covers `.tar`, `.iso`, and the
    /// raw LZMA family (`.lzma`, `.tar.lzma`, `.tlz`); executable
    /// extensions route through [`Archive::open_sfx`]. Compare
    /// [`Archive::extension_format`] against [`Archive::format`] to
    /// detect extension/content mismatches.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        // OI-0081-001: the format is detected from one open of the
        // pathname and the backend is constructed from a second. Capture
        // the file's stable identity at the detection open and revalidate
        // it once the backend exists, so a same-pathname replacement
        // between the two opens is refused rather than handing the backend
        // bytes detection never saw. The SFX-fallback branch below does
        // its own detection+staging identity binding (see
        // `open_with_sfx_progress`), so this snapshot guards only the
        // magic-detected → `open_as_format` flow.
        let identity = capture_read_identity(&path_buf)?;
        let format = ArchiveFormat::detect(&path_buf);

        match format {
            Ok(format) => {
                let archive = Self::open_as_format(&path_buf, format)?;
                revalidate_read_identity("open", &path_buf, identity)?;
                Ok(archive)
            }
            Err(detect_err) => {
                if crate::format::extension_suggests_executable(&path_buf) {
                    // R0076-0074: a failed SFX probe no longer collapses
                    // into the bare detection error — both causes surface.
                    return match Self::open_sfx(&path_buf) {
                        Ok(archive) => Ok(archive),
                        Err(sfx_err) => Err(sfx_fallback_failure(&detect_err, sfx_err)),
                    };
                }
                Err(detect_err)
            }
        }
    }

    /// Open `path` as a specific format, routing to the matching
    /// backend. Shared by [`Archive::open`], the SFX-aware open
    /// paths, and `Archive::modify`'s encryption probe (which already
    /// detected the format from its locked handle) so a successful
    /// detection always reaches the same backend constructor.
    ///
    /// OI-0081-001: this reopens `path` afresh in the backend, so
    /// detection→backend identity binding is the caller's job:
    /// [`Archive::open`] wraps this in a `capture_read_identity` /
    /// `revalidate_read_identity` pair, and `Archive::modify` binds it
    /// through the advisory-lock inode identity (DCR-007). A future
    /// caller that skips both would reintroduce the detect-then-reopen
    /// window this OI closed.
    pub(crate) fn open_as_format(path: &Path, format: ArchiveFormat) -> Result<Self> {
        let backend = match format {
            #[cfg(feature = "rar-support")]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                let unrar = UnrarArchive::open(path)?;
                ArchiveBackend::Unrar(Box::new(unrar))
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
                // AD 0007 (2026-07-23 collapse) / DCR-009: the `zip` crate is
                // the sole ZIP backend for both encrypted and unencrypted
                // archives. `ZipArchive::open` handles the no-password case as
                // a functional superset of the former piz reader.
                let zip = ZipArchive::open(path)?;
                ArchiveBackend::ZipReader(Box::new(zip))
            }
            ArchiveFormat::SevenZip => {
                let sevenz = SevenZArchive::open(path)?;
                ArchiveBackend::SevenZ(Box::new(sevenz))
            }
            ArchiveFormat::Tar
            | ArchiveFormat::TarGzip
            | ArchiveFormat::TarBzip2
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZst
            | ArchiveFormat::TarLz4
            | ArchiveFormat::TarLzma
            | ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Zst
            | ArchiveFormat::Lz4
            | ArchiveFormat::Lzma
            | ArchiveFormat::Iso => {
                let libarchive = LibarchiveArchive::open(path)?;
                ArchiveBackend::Libarchive(Box::new(libarchive))
            }
        };

        Ok(Self::new_read(backend, path.to_path_buf(), format))
    }

    /// Open an encrypted archive with password
    ///
    /// Mirrors [`Archive::open`]'s detection flow: magic bytes win when
    /// present, and an executable extension (`.exe`, `.com`, `.scr`,
    /// `.app`, `.run`, `.sh`, `.bash`) routes through an SFX-aware path
    /// so an encrypted SFX archive can be opened with a password
    /// directly via this method. Pre-staged SFX payloads
    /// are materialised into a tempfile, then re-opened against the
    /// password-capable backend (UnRAR / encrypted ZIP / 7z).
    pub fn open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let pwd_str = password.as_ref();
        // OI-0081-001: mirror `Archive::open` — capture identity at the
        // detection open so the password-capable backend, constructed
        // from a second open of the pathname, is bound to the bytes
        // detection saw. The SFX-fallback branch stages via its own
        // detection-bound path (`open_sfx_payload_for_encrypted`).
        let identity = capture_read_identity(&path_buf)?;
        let format = match ArchiveFormat::detect(&path_buf) {
            Ok(format) => format,
            Err(detect_err) => {
                // SFX fallback for executable-extension paths — mirrors
                // `Archive::open`'s flow so encrypted SFX archives can
                // be opened directly here without a separate detect-then-
                // re-open dance.
                if crate::format::extension_suggests_executable(&path_buf) {
                    return match Self::open_sfx_payload_for_encrypted(&path_buf, pwd_str) {
                        Ok(Some(staged)) => Ok(staged),
                        // R0076-0075: the probe ran fine but found no SFX
                        // payload — report that verdict alongside the
                        // detection failure instead of only `detect_err`.
                        Ok(None) => Err(sfx_fallback_failure(
                            &detect_err,
                            "file is not a self-extracting archive",
                        )),
                        // Probe itself failed (I/O, staging, or the staged
                        // payload would not open) — carry that cause too.
                        Err(sfx_err) => Err(sfx_fallback_failure(&detect_err, sfx_err)),
                    };
                }
                return Err(detect_err);
            }
        };

        let backend = match format {
            #[cfg(feature = "rar-support")]
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => {
                let unrar = UnrarArchive::open_with_password(&path_buf, pwd_str)?;
                ArchiveBackend::Unrar(Box::new(unrar))
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
                // AD 0007 (2026-07-23 collapse): the `zip` crate backend serves
                // every ZIP; the password path is the same backend as the plain
                // `Archive::open` ZIP path, just constructed with a password.
                let zip = ZipArchive::open_with_password(&path_buf, pwd_str)?;
                ArchiveBackend::ZipReader(Box::new(zip))
            }
            ArchiveFormat::SevenZip => {
                let sevenz = SevenZArchive::open_with_password(&path_buf, pwd_str)?;
                ArchiveBackend::SevenZ(Box::new(sevenz))
            }
            // Formats that do not encode encryption at the archive level —
            // any "password" the caller supplies is simply meaningless here.
            // Surface a precise reason instead of the catch-all
            // "Format not yet supported", which used to lead callers to
            // wonder whether a future version would support it. The
            // capability table owns the truth: a hand-enumerated variant
            // list here drifted from `supports_encryption_read()` the
            // moment a format landed in only one of the two places.
            other => {
                debug_assert!(
                    !other.supports_encryption_read(),
                    "{other:?} supports encrypted reads but open_encrypted has no backend arm for it"
                );
                return Err(ArchiveError::unsupported(
                    "open_encrypted",
                    other,
                    Some(format!(
                        "{:?} archives do not support encryption — open with `Archive::open` instead",
                        other
                    )),
                ));
            }
        };

        // OI-0081-001: backend is built; confirm the pathname still names
        // the inode detection saw before handing the handle back.
        revalidate_read_identity("open_encrypted", &path_buf, identity)?;
        Ok(Self::new_read(backend, path_buf, format))
    }

    /// Detect if a file is a self-extracting archive (SFX)
    ///
    /// Phase 7: Identifies executable files containing embedded archives.
    /// Uses 3-stage detection: executable validation → signature scan →
    /// heuristic offset screening (R0071-0016 — Stage 3 is a cheap
    /// plausibility check that the per-format prefix at the candidate
    /// offset has the expected shape; full archive validation runs
    /// later via [`Archive::open_sfx`]).
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
    /// if result.is_sfx() {
    ///     println!("Archive offset: {:?}", result.data_offset());
    ///     println!("Format: {:?}", result.archive_format());
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn detect_sfx(path: impl AsRef<Path>) -> Result<crate::sfx::SfxDetectionResult> {
        crate::sfx::detect_sfx(path)
    }

    /// Open a self-extracting archive (SFX) for reading
    ///
    /// Convenience method that detects SFX and forwards to `open_at_offset()`.
    ///
    /// # In place, or staged
    ///
    /// A ZIP, RAR or 7z payload behind an SFX-shaped path (one with an
    /// executable extension — `.exe`, `.sh`, ...) is read **in place**:
    /// the backend opens the file you named and starts at the detected
    /// offset, so nothing is copied and no ceiling applies (DCR-015; the
    /// RAR arm needs the `rar-support` feature). Everything else — the
    /// libarchive-backed formats (TAR family, ISO), and any payload
    /// behind a path that does not look executable — is copied
    /// ("staged") into a temporary file
    /// first, which is then opened through the normal archive pipeline
    /// and deleted when the returned handle drops.
    /// [`Archive::payload_access`] reports which one you got, and
    /// [`Archive::open_at_offset`] documents the exact conditions.
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
    ///
    /// # Staging ceiling
    ///
    /// When the payload *is* staged, the copy is capped at
    /// [`ExtractionLimits::default`]'s
    /// [`max_sfx_payload_size`](ExtractionLimits::max_sfx_payload_size)
    /// (AD 0040). Use [`Archive::open_sfx_with_limits`] to supply your
    /// own ceiling. The ceiling bounds a copy, so it does not apply to
    /// an in-place open.
    pub fn open_sfx(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_sfx_progress(path, None)
    }

    /// [`Archive::open_sfx`] with a caller-supplied staging ceiling.
    ///
    /// Only [`ExtractionLimits::max_sfx_payload_size`] participates —
    /// it is the one limit the staging copy can honour, because staging
    /// happens before any listing exists to apply the entry-count,
    /// size, or ratio gates to. Those still apply later, on the
    /// extraction call made against the returned handle.
    ///
    /// A ceiling bounds a copy, so it has nothing to say about a payload
    /// that is never copied: a ZIP, RAR or 7z payload opened in place is admitted
    /// whatever this value is. Check [`Archive::payload_access`] if that
    /// distinction matters to you.
    ///
    /// ```no_run
    /// use unified_archive::{Archive, Cap, ExtractionLimits};
    ///
    /// // Refuse to stage more than 256 MiB of embedded payload.
    /// let limits = ExtractionLimits::builder()
    ///     .max_sfx_payload_size(Cap::Limited(256 * 1024 * 1024))
    ///     .build();
    /// let archive = Archive::open_sfx_with_limits("installer.exe", &limits)?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn open_sfx_with_limits(path: impl AsRef<Path>, limits: &ExtractionLimits) -> Result<Self> {
        Self::open_sfx_staged(path.as_ref(), None, limits.max_sfx_payload_size())
    }

    /// Open a self-extracting archive while observing or cancelling
    /// the staging copy (R0075-0003).
    ///
    /// `Archive::open` on an SFX whose payload has to be staged copies
    /// that payload into a tempfile so the underlying backend can
    /// re-open it at offset zero. For multi-GB installers that copy is
    /// observable; pass an
    /// [`SfxStagingProgress`](crate::SfxStagingProgress) hook to
    /// surface running-byte progress and (optionally) cancel the copy
    /// partway through.
    ///
    /// Returns [`ArchiveError::Cancelled`]
    /// with `operation == "sfx_staging"` when the callback signals
    /// cancellation; the partial tempfile is dropped automatically.
    ///
    /// # The hook is silent when there is no copy
    ///
    /// When the payload is read in place — a ZIP, RAR or 7z payload
    /// behind a path with an executable extension, per
    /// [`Archive::open_at_offset`] (tickets `1ddc37ec`, `5858e17b`;
    /// DCR-015) — no bytes move, so the callback is **never
    /// invoked** (not even once with a zero total) and there is nothing
    /// for it to cancel. That is the intended outcome: the copy this
    /// hook exists to watch did not happen. Callers driving a progress
    /// bar off it should treat "no emissions, `Ok` returned" as instant
    /// completion, and can confirm with [`Archive::payload_access`].
    /// Every input that still stages emits exactly as before — the TAR
    /// family and ISO always, plus any ZIP, RAR or 7z payload whose
    /// in-place conditions decline. As of the RAR and 7z arms that is a
    /// smaller set than it was, which is worth saying plainly: this is a
    /// *staging* observer, not an open observer, and the fraction of SFX
    /// opens it can see anything at all has shrunk.
    ///
    /// `path` must be an SFX. Non-SFX inputs surface the same error
    /// as [`Archive::open_sfx`].
    ///
    /// The staging copy is capped at [`ExtractionLimits::default`]'s
    /// [`max_sfx_payload_size`](ExtractionLimits::max_sfx_payload_size);
    /// [`Archive::open_with_sfx_progress_and_limits`] takes the
    /// caller's ceiling instead.
    pub fn open_with_sfx_progress(
        path: impl AsRef<Path>,
        progress: Option<crate::options::SfxStagingProgress>,
    ) -> Result<Self> {
        Self::open_sfx_staged(path.as_ref(), progress, DEFAULT_SFX_PAYLOAD_CAP)
    }

    /// [`Archive::open_with_sfx_progress`] with a caller-supplied
    /// staging ceiling.
    ///
    /// The progress hook observes (and may cancel) the same copy the
    /// ceiling bounds: the cap is checked against the payload's size
    /// *before* the first byte is copied, so an over-cap payload fails
    /// without the callback ever firing. Only
    /// [`ExtractionLimits::max_sfx_payload_size`] participates — see
    /// [`Archive::open_sfx_with_limits`].
    ///
    /// Both are inert on an in-place open, where there is no copy to
    /// bound, watch, or cancel — see
    /// [`Archive::open_with_sfx_progress`].
    pub fn open_with_sfx_progress_and_limits(
        path: impl AsRef<Path>,
        progress: Option<crate::options::SfxStagingProgress>,
        limits: &ExtractionLimits,
    ) -> Result<Self> {
        Self::open_sfx_staged(path.as_ref(), progress, limits.max_sfx_payload_size())
    }

    /// Single implementation behind every SFX open: detect, bind the
    /// payload open to the detection open's identity, then either read
    /// the payload in place or stage it under `max_payload`. The public
    /// entry points differ only in where that ceiling and the progress
    /// hook come from.
    ///
    /// The name is historical — since ticket `1ddc37ec` this does not
    /// always stage.
    fn open_sfx_staged(
        path_ref: &Path,
        progress: Option<crate::options::SfxStagingProgress>,
        max_payload: Cap,
    ) -> Result<Self> {
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

        // OI-0081-001 / R0001-0002: take the identity `detect_sfx`
        // captured from its *own* open handle, so the staging copy (a
        // separate open of `path_ref`) is bound to the bytes detection
        // actually read. This used to be a `capture_read_identity`
        // re-stat *after* detection had closed its handle, which left a
        // window where a replacement became the trusted baseline. Fail
        // closed when the identity is absent rather than falling back to
        // a fresh stat that would prove nothing about detection's read.
        let identity = detection.source_identity().ok_or_else(|| {
            ArchiveError::operation_blocked(
                "open_sfx",
                format!(
                    "SFX detection for {} produced no source identity; \
                     aborting rather than staging bytes that cannot be bound \
                     to the detection open",
                    path_ref.display()
                ),
            )
        })?;

        // R0070-0012: route the staged payload through the detected
        // archive format's extension so a `.exe` SFX wrapping a
        // `.tar.gz` payload re-detects as `TarGzip` (not bare
        // `Gzip`). `open_at_offset` falls back to the source's
        // compound-extension preservation when the format hint is
        // absent.
        let mut progress = progress;
        Self::open_at_offset_with_format_hint_and_progress(
            path_ref,
            offset,
            max_payload,
            detection.archive_format,
            Some(identity),
            progress.as_mut(),
        )
    }

    /// Open an archive from a specific byte offset.
    ///
    /// Opens an archive that doesn't start at byte 0 of the file — primarily
    /// SFX payloads, where the archive data is embedded after an executable
    /// stub.
    ///
    /// `offset == 0` is equivalent to [`Archive::open`].
    /// Offsets greater than or equal to the file's length return an error.
    ///
    /// # In place, or staged (ticket `1ddc37ec`)
    ///
    /// Two ways to reach the embedded archive, and
    /// [`Archive::payload_access`] tells you which one you got:
    ///
    /// * **In place** — the backend opens `path` and starts reading at
    ///   `offset`. Nothing is copied and no ceiling applies. Requires
    ///   an SFX-shaped `path` (executable extension) and a ZIP, RAR or
    ///   7z payload whose signature is found at `offset`. ZIP
    ///   additionally requires `offset` to agree with where the ZIP's
    ///   own end-of-central-directory record says the archive starts;
    ///   RAR relocates itself by scanning for its signature; 7z is
    ///   handed a window that makes `offset` look like byte 0. Each arm
    ///   declines to staging rather than failing if its constructor
    ///   does not accept the payload (DCR-015).
    /// * **Staged** — otherwise, `file_len - offset` bytes are copied
    ///   into a temporary file that is removed when the returned
    ///   [`Archive`] is dropped, and the AD 0040 ceiling bounds that
    ///   copy. This is the path for the libarchive-backed formats (TAR
    ///   family, ISO) and for any input the in-place gates decline.
    ///
    /// The observable archive is the same either way — the same
    /// entries, the same [`Archive::path`], the same
    /// [`Archive::format`], the same extraction results — and so is the
    /// ratio denominator, which excludes the stub on both paths. What
    /// differs is the disk cost, the ceiling, and one exposure:
    ///
    /// # Snapshot exposure
    ///
    /// A staged handle reads a private copy taken at open time, so a
    /// later rewrite of `path` cannot change what it sees. An in-place
    /// handle keeps reading `path` itself, so between this call and the
    /// first operation that parses the archive (AD 0065 freezes the
    /// listing at *first observation*, not at open) a concurrent
    /// rewrite of that file is visible — exactly as it is for an
    /// ordinary [`Archive::open`] of any archive. The
    /// detection→open inode revalidation still applies, so a
    /// same-pathname *replacement* up to the open is refused; what is
    /// no longer copied out of harm's way is the payload itself. Read
    /// the archive promptly, or accept the same exposure every
    /// non-SFX open already has.
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
    ///
    /// # Staging ceiling
    ///
    /// When the payload is staged, the `file_len - offset` bytes copied
    /// are capped at [`ExtractionLimits::default`]'s
    /// [`max_sfx_payload_size`](ExtractionLimits::max_sfx_payload_size)
    /// (AD 0040). Use [`Archive::open_at_offset_with_limits`] to
    /// tighten (or lift) that ceiling. An in-place open copies nothing,
    /// so the ceiling cannot bind on it.
    pub fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> {
        Self::open_at_offset_with_format_hint(path.as_ref(), offset, DEFAULT_SFX_PAYLOAD_CAP, None)
    }

    /// [`Archive::open_at_offset`] with a caller-supplied staging
    /// ceiling.
    ///
    /// Only [`ExtractionLimits::max_sfx_payload_size`] participates:
    /// the offset payload is staged before any listing exists, so the
    /// entry-count, size and ratio gates have nothing to run against
    /// yet — they apply to the extraction calls made against the
    /// returned handle.
    ///
    /// `offset == 0` short-circuits to [`Archive::open`] and copies
    /// nothing, so the ceiling is not consulted there. Neither is it
    /// consulted on an in-place open — see
    /// [`Archive::open_at_offset`], and [`Archive::payload_access`] to
    /// find out which path a handle took.
    ///
    /// ```no_run
    /// use unified_archive::{Archive, Cap, ExtractionLimits};
    ///
    /// let limits = ExtractionLimits::builder()
    ///     .max_sfx_payload_size(Cap::Limited(64 * 1024 * 1024))
    ///     .build();
    /// // Fails with `ArchiveError::Format` when the payload after the
    /// // offset is larger than 64 MiB, instead of staging it.
    /// let archive = Archive::open_at_offset_with_limits("file.bin", 65536, &limits)?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn open_at_offset_with_limits(
        path: impl AsRef<Path>,
        offset: u64,
        limits: &ExtractionLimits,
    ) -> Result<Self> {
        Self::open_at_offset_with_format_hint(
            path.as_ref(),
            offset,
            limits.max_sfx_payload_size(),
            None,
        )
    }

    /// Internal variant of [`Self::open_at_offset`] that takes an
    /// optional [`ArchiveFormat`] hint. When the caller already knows
    /// the payload format (e.g. SFX detection), the hint is used to
    /// pick the staging tempfile's suffix so re-detection on the
    /// tempfile still distinguishes compound formats (R0070-0012).
    ///
    /// **Hint at `offset == 0`.** The hint is ignored when
    /// the caller passes `offset == 0` — the helper short-circuits to
    /// [`Self::open`], which performs its own magic-byte detection.
    /// Forcing a format hint at offset zero would bypass detection's
    /// magic-vs-extension consistency check, so the helper deliberately
    /// drops the hint in that case. SFX flows always pass a non-zero
    /// offset, so the elision is invisible to them.
    pub(crate) fn open_at_offset_with_format_hint(
        path_ref: &Path,
        offset: u64,
        max_payload: Cap,
        format_hint: Option<ArchiveFormat>,
    ) -> Result<Self> {
        // OI-0081-001: the public `open_at_offset` path takes a raw
        // caller-supplied offset with no detection open to bind to, so no
        // identity is threaded (`None`); the R0075-0002 take+size
        // assertion in `stage_sfx_payload` still bounds the copy.
        Self::open_at_offset_with_format_hint_and_progress(
            path_ref,
            offset,
            max_payload,
            format_hint,
            None,
            None,
        )
    }

    /// Internal variant that threads an optional staging progress hook
    /// (R0075-0003) and the caller's staging ceiling (`max_payload`)
    /// through to [`stage_sfx_payload`].
    pub(crate) fn open_at_offset_with_format_hint_and_progress(
        path_ref: &Path,
        offset: u64,
        max_payload: Cap,
        format_hint: Option<ArchiveFormat>,
        expected_identity: Option<ReadFileIdentity>,
        progress: Option<&mut crate::options::SfxStagingProgress>,
    ) -> Result<Self> {
        if offset == 0 {
            return Self::open(path_ref);
        }

        // Tickets 1ddc37ec and 5858e17b: before committing to a copy,
        // ask whether a backend can read the payload where it already
        // lies. ZIP, RAR and 7z can (see `try_open_in_place`), and only
        // for an SFX-shaped input; the libarchive family and every
        // gate-declined open fall through to staging with the AD 0040
        // ceiling in force.
        if let Some(archive) = Self::try_open_in_place(path_ref, offset, format_hint)? {
            // OI-0081-001: the in-place backend reopened `path_ref` by
            // name, exactly as `Archive::open` does after detection, so
            // it gets the same post-construction revalidation. Nothing
            // was copied, so there is no copy-source open to bind
            // instead.
            if let Some(expected) = expected_identity {
                revalidate_read_identity("open_sfx", path_ref, expected)?;
            }
            return Ok(archive);
        }

        // OI-0081-001: `expected_identity` (`Some` only on the
        // detection-bound SFX path) is revalidated at the copy-source
        // open inside `stage_sfx_payload`.
        let temp_path = stage_sfx_payload(
            path_ref,
            offset,
            max_payload,
            format_hint,
            expected_identity,
            "unified-archive-sfx-",
            progress,
        )?;
        let mut archive = Self::open(&temp_path)?;
        archive.path = path_ref.to_path_buf();
        archive._backing_tempfile = Some(PayloadSource::Staged(temp_path));
        Ok(archive)
    }

    /// Open the archive embedded at `offset` **without copying it**,
    /// or return `Ok(None)` when that cannot be proven safe and the
    /// caller should fall back to staging (ticket `1ddc37ec`).
    ///
    /// # Which backend
    ///
    /// ZIP, RAR and 7z. Each reaches the same place by a different
    /// route, which is why the arms do not share a mechanism:
    ///
    /// - **ZIP** relocates itself. The `zip` crate derives the archive's
    ///   start from the end-of-central-directory record and folds that
    ///   offset into every local-header position (`ZipArchive::offset()`
    ///   reports it), and this crate's wrapper already reads its raw
    ///   central-directory index from the absolute
    ///   `central_directory_start()`. Handing the wrapper the *outer*
    ///   file is enough.
    /// - **RAR** relocates itself too, inside the SDK: UnRAR's
    ///   `Archive::IsArchive` reads seven bytes at position 0 and, when
    ///   they are not a marker, scans up to `MAXSFXSIZE` for the first
    ///   RAR signature and treats that as the start. So the SDK has
    ///   always been able to read a RAR SFX in place — the crate simply
    ///   never routed offset opens to it.
    /// - **7z** does *not* relocate: `Archive::read` requires the
    ///   signature at stream position 0. It is reached instead through
    ///   [`crate::payload_window::PayloadWindow`], a `Read + Seek` view
    ///   that makes byte `offset` look like byte 0.
    ///
    /// libarchive (the TAR family and ISO) still stages, and that is a
    /// decision rather than an omission: its formats have no strong
    /// magic at the payload offset — plain tar's `ustar` sits at +257,
    /// gzip's is two bytes, the raw filter accepts nearly anything — so
    /// a gate cheap enough to run would admit garbage, and committing to
    /// an in-place open that then failed format bidding would need a
    /// fall-back-after-failed-open path this function deliberately does
    /// not have. See the note on `LibarchiveArchive::open_read_handle`.
    ///
    /// The arms' order is irrelevant by construction: each requires its
    /// own magic at `offset`, and the three signatures are mutually
    /// exclusive.
    ///
    /// # The gates, and why each one exists
    ///
    /// **Common to every arm — SFX-shaped name.** The outer path must
    /// carry an executable extension. This is not cosmetic. When the
    /// caller supplies a password, extraction reopens the archive
    /// through [`Archive::open_encrypted`] (`reopen_with_password_if_set`)
    /// against [`Archive::source_path_for_reopen`] — the staged tempfile
    /// for a staged handle, but the outer file for an in-place one.
    /// `open_encrypted` resolves an embedded payload only through its SFX
    /// fallback, and an executable extension is what admits a file to
    /// that fallback. Without this gate, `open_at_offset("blob.bin", n)`
    /// followed by a password-bearing extraction would fail where staging
    /// succeeded. Lifting it needs an offset-aware reopen in
    /// `src/extraction.rs`.
    ///
    /// **Common to every arm — magic at `offset`.** The staged path used
    /// to re-detect the format from the tempfile's suffix; in place there
    /// is nothing to re-detect, so the bytes at `offset` are the only
    /// authority and each arm demands its own signature there.
    ///
    /// **ZIP — offset agreement.** The `zip` crate is asked, through the
    /// same default configuration the wrapper uses, where *it* thinks the
    /// archive starts. Disagreement means reading in place would hand
    /// back a different archive than the caller addressed, so the answer
    /// is "stage" — staging slices at exactly the caller's offset. Costs
    /// one throwaway central-directory parse (metadata only, no payload
    /// decode) against a copy of up to the whole payload.
    ///
    /// **RAR — offset agreement, and reach.** Agreement matters more here
    /// than for ZIP, because the SDK binds to the *first* signature it
    /// finds: if the stub happens to contain an earlier `Rar!` byte run,
    /// unrar would open a different archive than `offset` addresses. So
    /// the span before `offset` is scanned and any earlier RAR signature
    /// declines to staging. Reach matters because the SDK's scan stops at
    /// `MAXSFXSIZE`; a payload past that is invisible to it, so an offset
    /// beyond [`crate::ffi::wrapper::UNRAR_MAX_SFX_SCAN`] stages instead.
    /// Detection-driven offsets always qualify — `MAX_SCAN_SIZE` is far
    /// smaller — but a raw `open_at_offset` caller can name any number.
    ///
    /// **7z — no agreement gate, and none is owed.** `Archive::read`
    /// requires the signature at stream position 0 and searches for
    /// nothing, so the window's base *is* the archive start. A wrong
    /// offset fails the signature check and falls back to staging; there
    /// is no "some other archive" for it to find.
    ///
    /// Any I/O or parse failure here also returns `Ok(None)`: the
    /// staging path is then responsible for producing the error, so a
    /// bad offset keeps reporting exactly what it reported before.
    fn try_open_in_place(
        path_ref: &Path,
        offset: u64,
        format_hint: Option<ArchiveFormat>,
    ) -> Result<Option<Self>> {
        // The one gate every arm shares. See the doc comment: without an
        // executable extension a password-bearing extraction cannot reopen
        // an in-place handle, so admitting one here would trade a copy for
        // a failure.
        if !crate::format::extension_suggests_executable(path_ref) {
            return Ok(None);
        }

        if matches!(format_hint, None | Some(ArchiveFormat::Zip))
            && let Some(archive) = Self::try_open_zip_in_place(path_ref, offset)?
        {
            return Ok(Some(archive));
        }

        #[cfg(feature = "rar-support")]
        if matches!(
            format_hint,
            None | Some(ArchiveFormat::Rar) | Some(ArchiveFormat::Rar5)
        ) && let Some(archive) = Self::try_open_rar_in_place(path_ref, offset)?
        {
            return Ok(Some(archive));
        }

        if matches!(format_hint, None | Some(ArchiveFormat::SevenZip))
            && let Some(archive) = Self::try_open_sevenz_in_place(path_ref, offset)?
        {
            return Ok(Some(archive));
        }

        Ok(None)
    }

    /// Read the first `N` bytes at `offset`, or `None` if that cannot be
    /// done — a short file, an unreadable one, an offset past the end.
    ///
    /// Every arm's magic check goes through here so that "the bytes do not
    /// say what we need" and "we could not look" collapse to the same
    /// answer: decline, and let staging produce the error.
    fn magic_at<const N: usize>(path_ref: &Path, offset: u64) -> Option<[u8; N]> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(path_ref).ok()?;
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut magic = [0u8; N];
        file.read_exact(&mut magic).ok()?;
        Some(magic)
    }

    /// ZIP arm. See the doc comment on [`Self::try_open_in_place`].
    fn try_open_zip_in_place(path_ref: &Path, offset: u64) -> Result<Option<Self>> {
        use std::io::{Seek, SeekFrom};

        const LOCAL_FILE_HEADER: [u8; 4] = *b"PK\x03\x04";
        // An archive with no entries starts with its EOCD instead of a
        // local header; that degenerate case stages, which costs a copy of
        // a few dozen bytes.
        if Self::magic_at::<4>(path_ref, offset) != Some(LOCAL_FILE_HEADER) {
            return Ok(None);
        }

        let Ok(mut file) = std::fs::File::open(path_ref) else {
            return Ok(None);
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Ok(None);
        }

        // Offset agreement. Built with `ZipArchive::new`, i.e. the same
        // default `ArchiveOffset` resolution the wrapper's `open_zip`
        // uses, so agreement here means agreement there.
        let probe = match zip::ZipArchive::new(std::io::BufReader::new(&mut file)) {
            Ok(probe) => probe,
            Err(_) => return Ok(None),
        };
        if probe.offset() != offset {
            return Ok(None);
        }
        drop(probe);
        drop(file);

        // The wrapper opens `path_ref` itself and re-derives the same
        // offset; `path` is already the caller-facing path, so nothing
        // has to be patched up afterwards.
        // Declines rather than propagates. The doc comment already
        // promises that any failure here yields `Ok(None)` and lets
        // staging report it; the `?` that used to be here contradicted
        // that for the one case where the wrapper's own open failed after
        // the gates had passed.
        let Ok(zip) = ZipArchive::open(path_ref) else {
            return Ok(None);
        };
        Ok(Some(Self::in_place_handle(
            ArchiveBackend::ZipReader(Box::new(zip)),
            path_ref,
            ArchiveFormat::Zip,
            offset,
        )))
    }

    /// RAR arm. See the doc comment on [`Self::try_open_in_place`].
    ///
    /// The SDK re-derives the payload offset itself, so unlike 7z this
    /// needs no reader adapter — but for the same reason it must be shown
    /// that the SDK will land on the *caller's* offset and not an earlier
    /// signature it happens to meet first.
    #[cfg(feature = "rar-support")]
    fn try_open_rar_in_place(path_ref: &Path, offset: u64) -> Result<Option<Self>> {
        const RAR4_MARKER: [u8; 7] = *b"Rar!\x1a\x07\x00";
        const RAR5_MARKER: [u8; 8] = *b"Rar!\x1a\x07\x01\x00";

        // Reach, both ends. The SDK's SFX scan starts at byte 7 — it reads
        // the marker at position 0 first, and only then scans from where
        // that read left it — so a payload at 1..=6 is unreachable to
        // unrar however well-formed it is. And the scan stops at
        // `MAXSFXSIZE`, so a payload past that is invisible too. Both ends
        // stage instead; neither is an error.
        if (1..7).contains(&offset)
            || offset.saturating_add(8) > crate::ffi::wrapper::UNRAR_MAX_SFX_SCAN
        {
            return Ok(None);
        }

        let Some(head) = Self::magic_at::<8>(path_ref, offset) else {
            return Ok(None);
        };
        let format = if head == RAR5_MARKER {
            ArchiveFormat::Rar5
        } else if head[..7] == RAR4_MARKER {
            ArchiveFormat::Rar
        } else {
            return Ok(None);
        };

        // Offset agreement, and the reason this arm needs a scan where 7z
        // does not: unrar binds to the FIRST marker in the file. An
        // earlier `Rar!` run in the stub — a string constant, another
        // embedded archive — would silently redirect the open, so any
        // earlier signature declines and lets staging slice at exactly the
        // offset the caller named.
        match Self::first_rar_signature_before(path_ref, offset) {
            Ok(Some(_)) | Err(()) => return Ok(None),
            Ok(None) => {}
        }

        // A failed native open declines to staging rather than
        // propagating. This function's contract is that `Ok(None)` means
        // "could not prove it, let staging try", and staging then owns the
        // error — so a stub carrying a marker this crate's signature table
        // does not know (unrar's `IsSignature` also accepts RARFMT14's
        // `RE~^` and the RAR 2..4 future markers, which
        // `scan_for_signatures` does not) fails closed into a copy rather
        // than into a hard error the caller never got before.
        let Ok(rar) = crate::ffi::wrapper::UnrarArchive::open_at_offset(path_ref, offset) else {
            return Ok(None);
        };
        Ok(Some(Self::in_place_handle(
            ArchiveBackend::Unrar(Box::new(rar)),
            path_ref,
            format,
            offset,
        )))
    }

    /// The offset of the first RAR marker strictly before `limit`, if any.
    ///
    /// `Err(())` when the span could not be read, which the caller treats
    /// as "decline" rather than as "no earlier signature" — an unreadable
    /// stub is exactly when guessing is wrong.
    ///
    /// Chunked with an overlap so a marker straddling a chunk boundary is
    /// still seen. `limit` is bounded by the reach gate before this runs,
    /// so the whole scan is bounded too.
    #[cfg(feature = "rar-support")]
    fn first_rar_signature_before(
        path_ref: &Path,
        limit: u64,
    ) -> std::result::Result<Option<u64>, ()> {
        use crate::sfx::signatures::scan_for_signatures;
        use std::io::Read;

        if limit == 0 {
            return Ok(None);
        }
        const CHUNK: usize = 64 * 1024;
        const OVERLAP: usize = 8;

        let mut file = std::fs::File::open(path_ref).map_err(|_| ())?;
        let mut window = vec![0u8; CHUNK];
        let mut filled = 0usize;
        let mut base = 0u64;
        let mut remaining = limit;

        while remaining > 0 {
            let want = usize::try_from(remaining)
                .unwrap_or(CHUNK)
                .min(CHUNK - filled);
            let read = file
                .read(&mut window[filled..filled + want])
                .map_err(|_| ())?;
            if read == 0 {
                break;
            }
            filled += read;
            remaining -= read as u64;

            // `scan_for_signatures` reports every known format; only the
            // RAR family can misdirect unrar, so the rest are ignored here.
            // Results come back sorted by offset, so the first RAR hit is
            // the earliest one.
            if let Some((found, _)) = scan_for_signatures(&window[..filled])
                .into_iter()
                .find(|(_, format)| matches!(format, ArchiveFormat::Rar | ArchiveFormat::Rar5))
                && base + found as u64 <= limit
            {
                return Ok(Some(base + found as u64));
            }

            let keep = filled.min(OVERLAP);
            window.copy_within(filled - keep..filled, 0);
            base += (filled - keep) as u64;
            filled = keep;
        }
        Ok(None)
    }

    /// 7z arm. See the doc comment on [`Self::try_open_in_place`].
    fn try_open_sevenz_in_place(path_ref: &Path, offset: u64) -> Result<Option<Self>> {
        const SEVENZ_SIGNATURE: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

        if Self::magic_at::<6>(path_ref, offset) != Some(SEVENZ_SIGNATURE) {
            return Ok(None);
        }

        // No agreement gate: `Archive::read` requires the signature at
        // stream position 0 and searches for nothing, so the window's base
        // *is* the archive start. AD 0052 is preserved too — `open_at_offset`
        // parses nothing, so a 7z-magic'd but corrupt payload still fails at
        // the first operation, exactly like an ordinary corrupt `.7z`.
        // Declines rather than propagates, for the same reason as the
        // other arms: staging owns the error.
        let Ok(sevenz) =
            crate::ffi::sevenz_wrapper::SevenZArchive::open_at_offset(path_ref, offset)
        else {
            return Ok(None);
        };
        Ok(Some(Self::in_place_handle(
            ArchiveBackend::SevenZ(Box::new(sevenz)),
            path_ref,
            ArchiveFormat::SevenZip,
            offset,
        )))
    }

    /// A read handle over a payload that was opened where it lies.
    ///
    /// One constructor for all three arms so `PayloadSource::InPlace`
    /// cannot be forgotten on a new one — it is what
    /// [`Archive::payload_access`] reports and what keeps
    /// `payload_size_for_ratio` measuring the payload rather than
    /// `stub + payload` (R0069-0006).
    fn in_place_handle(
        backend: ArchiveBackend,
        path_ref: &Path,
        format: ArchiveFormat,
        offset: u64,
    ) -> Self {
        let mut archive = Self::new_read(backend, path_ref.to_path_buf(), format);
        archive._backing_tempfile = Some(PayloadSource::InPlace { offset });
        archive
    }

    /// Stage an SFX payload to a tempfile and open it via
    /// [`Archive::open_encrypted`]. Returns `Ok(None)`
    /// when the source is not an SFX so the caller can surface its
    /// own non-SFX error.
    fn open_sfx_payload_for_encrypted(path_ref: &Path, password: &str) -> Result<Option<Self>> {
        let detection = Self::detect_sfx(path_ref)?;
        let Some((format, offset, _stub)) = detection.payload_coordinates() else {
            return Ok(None);
        };
        // OI-0081-001 / R0001-0002: bind the staging copy to the identity
        // `detect_sfx` captured from its own handle, not to a re-stat of
        // the pathname taken after that handle was closed.
        let identity = detection.source_identity().ok_or_else(|| {
            ArchiveError::operation_blocked(
                "open_sfx",
                format!(
                    "SFX detection for {} produced no source identity; \
                     aborting rather than staging bytes that cannot be bound \
                     to the detection open",
                    path_ref.display()
                ),
            )
        })?;
        // `open_encrypted` takes no `ExtractionLimits` (it predates the
        // limits-accepting SFX opens and adding one would change a
        // public signature), so the staging ceiling is the AD 0040
        // default here — the same value this path used before the cap
        // became caller-configurable.
        let temp_path = stage_sfx_payload(
            path_ref,
            offset,
            DEFAULT_SFX_PAYLOAD_CAP,
            Some(format),
            Some(identity),
            "unified-archive-sfx-enc-",
            None,
        )?;
        let mut archive = Self::open_encrypted(&temp_path, password)?;
        archive.path = path_ref.to_path_buf();
        archive._backing_tempfile = Some(PayloadSource::Staged(temp_path));
        Ok(Some(archive))
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
    /// if detection.is_sfx() {
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

        let claimed_offset = detection
            .data_offset
            .ok_or_else(|| ArchiveError::format(None, "SFX detection missing offset"))?;

        // R0069-0007: re-run detection against `path` and only honour an
        // offset that survives a fresh probe. The caller-supplied
        // `SfxDetectionResult` is treated as a hint, not as authority —
        // a forged detection (or a stale one for a file that was
        // overwritten between the original probe and this call) cannot
        // make us read at an attacker-chosen offset.
        let verified = crate::sfx::detect_sfx(path.as_ref())?;
        if !verified.is_sfx {
            return Err(ArchiveError::format(
                None,
                "Cannot extract stub: file is no longer detected as SFX",
            ));
        }
        let verified_offset = verified.data_offset.ok_or_else(|| {
            ArchiveError::format(None, "SFX re-detection produced no data offset")
        })?;
        if verified_offset != claimed_offset {
            return Err(ArchiveError::format(
                None,
                format!(
                    "SFX stub-offset mismatch: caller asserted {} bytes, fresh detection found {}",
                    claimed_offset, verified_offset
                ),
            ));
        }
        let offset = verified_offset;

        // Guard against malicious/corrupted files with unreasonably large
        // stubs. The ceiling lives in `crate::sfx::limits`.
        //
        // **This check cannot fire today — keep it anyway** (ti-9909d449,
        // 2026-09-03). `offset` now comes from the fresh `detect_sfx`
        // probe above, whose signature scan never looks past
        // `limits::MAX_SCAN_SIZE` (1 MiB), so the binding ceiling is
        // already 1 MiB and this 50 MiB test is subsumed. It stays
        // because the value it bounds is still parsed out of an
        // untrusted executable, and only *one* upstream property makes
        // it redundant: that `extract_stub` accepts no offset except a
        // freshly-scanned one. The moment that changes — a caller offset
        // verified some other way (e.g. probing the format header at the
        // claimed offset), a widened scan window, or a detector that can
        // report an offset it did not scan to — this becomes the binding
        // ceiling again, with nothing else between an attacker-chosen
        // length and the `Vec` allocated below. Deleting it as "dead"
        // would remove the guard and leave no diagnostic behind.
        let max_stub = crate::sfx::limits::MAX_STUB_SIZE;
        if offset > max_stub {
            return Err(ArchiveError::format(
                None,
                format!("SFX stub size {offset} exceeds maximum allowed size of {max_stub} bytes",),
            ));
        }

        use std::fs::File;
        use std::io::Read;

        let path_ref = path.as_ref();

        // R0081-0017: the offset above was verified by a fresh
        // `detect_sfx` probe that opened `path` on its own descriptor.
        // Reading the stub is a *separate* open, so a same-offset
        // replacement (or in-place resize) between verify and read would
        // otherwise hand back bytes that never passed detection. Bind the
        // read to what verification saw: snapshot the file's stable
        // identity (dev/ino/len on Unix; len elsewhere) right after the
        // probe, then require the read handle to still name that same
        // instance both before and after `read_exact`, failing closed on
        // any drift. Reusing the exact descriptor `detect_sfx` itself
        // opened would be tighter still, but that lives behind the
        // path-based detection API (tracked with the R0081-0015/0016
        // handle-binding work); this revalidation is the in-scope guard
        // R0081-0017 accepts.
        let verified_identity = std::fs::metadata(path_ref)
            .map(|m| read_file_identity(&m))
            .map_err(|e| ArchiveError::io("stat", path_ref, e))?;

        let mut file = File::open(path_ref).map_err(|e| ArchiveError::io("open", path_ref, e))?;
        let open_identity = file
            .metadata()
            .map(|m| read_file_identity(&m))
            .map_err(|e| ArchiveError::io("stat", path_ref, e))?;
        if open_identity != verified_identity {
            return Err(ArchiveError::format(
                None,
                "SFX source changed between offset verification and stub read; \
                 refusing to emit an unverified stub",
            ));
        }

        let mut stub = vec![0u8; offset as usize];
        file.read_exact(&mut stub)
            .map_err(|e| ArchiveError::io("read", path_ref, e))?;

        // Re-stat the same handle after the copy: a truncation or in-place
        // resize that raced the read shows up as a length/identity drift
        // and invalidates the stub we just materialised.
        let post_identity = file
            .metadata()
            .map(|m| read_file_identity(&m))
            .map_err(|e| ArchiveError::io("stat", path_ref, e))?;
        if post_identity != verified_identity {
            return Err(ArchiveError::format(
                None,
                "SFX source changed during stub read; \
                 refusing to emit an unverified stub",
            ));
        }

        Ok(stub)
    }

    /// Get the actually-detected archive format. Determined by
    /// magic-byte inspection during [`Archive::open`] (with the SFX
    /// fallback for executable extensions); the file's filename
    /// extension is **not** consulted unless magic-byte detection
    /// failed. Compare against [`Archive::extension_format`] to
    /// detect cases where the extension lies about content.
    pub fn format(&self) -> ArchiveFormat {
        self.format
    }

    /// Pure extension-derived format guess for the path this archive
    /// was opened from. Returns `Some(format)` when the extension
    /// cleanly maps to a single supported format, `None` otherwise
    /// (no extension, ambiguous like `.bin`, or unsupported).
    ///
    /// Callers compare this against [`Archive::format`] to detect
    /// extension/content mismatches:
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    /// let archive = Archive::open("download.zip")?;
    /// if let Some(claimed) = archive.extension_format() {
    ///     if claimed != archive.format() {
    ///         eprintln!(
    ///             "warning: extension says {:?} but content is {:?}",
    ///             claimed, archive.format()
    ///         );
    ///     }
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    ///
    /// **SFX caveat.** For an archive opened via
    /// [`Archive::open_sfx`] / [`Archive::open_at_offset`],
    /// [`Archive::format`] returns the *payload* format
    /// (`Zip` / `Rar5` / `SevenZip` / `TarGzip` / …) while
    /// `extension_format` looks at the **outer** SFX path's extension
    /// — typically `.exe`/`.com`/`.scr`/etc., none of which map
    /// cleanly to a single archive format. A genuine SFX therefore
    /// usually returns `None` here. Treat that combination
    /// (`format` = a real archive, `extension_format` = `None`,
    /// caller-facing path has an executable extension) as "outer
    /// claims executable, payload is an archive" — not as a
    /// content/extension mismatch.
    pub fn extension_format(&self) -> Option<ArchiveFormat> {
        crate::format::format_from_extension(&self.path)
    }

    /// Best-effort check that the archive contains at least one encrypted
    /// entry.
    ///
    /// Walks the archive's metadata-only listing and returns `true` if any
    /// entry's `is_encrypted` flag is set. This is **not** a guaranteed
    /// "password required?" probe:
    ///
    /// - **Header-encrypted archives** (e.g. 7z with `-mhe`, RAR with `-hp`)
    ///   refuse to surface a listing without the password. Calling
    ///   `is_encrypted` on such a handle will fail when listing fails — the
    ///   error bubbles up rather than producing `false`.
    /// - **Mixed archives** (some entries encrypted, some not) return
    ///   `true`. A `false` result therefore means "no encrypted entries
    ///   were observed in metadata", not "extraction will succeed without a
    ///   password".
    ///
    /// For a guaranteed answer, use [`Archive::open_encrypted`] with the
    /// candidate password and treat
    /// [`ArchiveError::Password`] as
    /// "wrong password / encryption confirmed".
    pub fn is_encrypted(&self) -> Result<bool> {
        // The cached listing answers this — both listings surface the
        // same metadata snapshot, and the uncached
        // `list_files_for_limits` walk forced a full backend re-listing
        // (7z: file reopen + TOC re-parse) on every call.
        Ok(self.list_files()?.iter().any(|e| e.is_encrypted))
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
        // R0071-0012: read-side capability queries reject write-mode
        // handles instead of silently answering `false` for an
        // in-progress writer. Modify mode still answers truthfully
        // even though `Archive::modify` wraps every format in the
        // Libarchive backend: RAR cannot be modified, so a
        // Modify-mode handle never carries a recovery record and all
        // non-RAR arms agree on the answer (contrast `is_solid`,
        // where the wrapping required a format dispatch —
        // R0079-0035).
        if self.mode == ArchiveMode::Write {
            return Err(ArchiveError::write_mode_only("has_recovery_record"));
        }
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.has_recovery_record(),
            // Other formats don't support recovery records
            ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::SevenZ(_)
            | ArchiveBackend::Libarchive(_) => Ok(false),
        }
    }

    /// Take the advisories the backend produced while reading this
    /// archive, emptying the buffer.
    ///
    /// A backend can hit a condition it *recovers* from — libarchive
    /// returns `ARCHIVE_WARN` for a header field it had to repair or a
    /// format quirk it worked around, and hands back a usable entry. This
    /// crate accepts those, because refusing a read the library completed
    /// would make it stricter than the library it wraps. Accepting them
    /// must not mean hiding them, so the message is kept and reaches you
    /// here as [`ArchiveWarning::BackendAdvisory`](crate::error::ArchiveWarning::BackendAdvisory).
    ///
    /// [`extract_all`](Self::extract_all) already returns a warning list
    /// and appends these to it, so that path needs no extra call. Every
    /// other read — [`list_files`](Self::list_files),
    /// [`extract_file`](Self::extract_file),
    /// [`extract_to_memory`](Self::extract_to_memory),
    /// [`validate_integrity`](Self::validate_integrity) — returns through a
    /// signature with no warning channel. Call this after one of those to
    /// see whether the backend had anything to say.
    ///
    /// Returns an empty vector when there was nothing to report, which is
    /// the overwhelmingly common case, and always for backends other than
    /// libarchive: the ZIP, 7z and UnRAR readers have no equivalent
    /// recoverable-status channel.
    ///
    /// ```no_run
    /// # use unified_archive::Archive;
    /// # fn main() -> unified_archive::Result<()> {
    /// let archive = Archive::open("backup.tar.gz")?;
    /// let entries = archive.list_files()?;
    /// for advisory in archive.take_backend_warnings() {
    ///     eprintln!("{advisory}");
    /// }
    /// # let _ = entries;
    /// # Ok(())
    /// # }
    /// ```
    pub fn take_backend_warnings(&self) -> Vec<crate::error::ArchiveWarning> {
        match &self.backend {
            ArchiveBackend::Libarchive(backend) => backend.take_backend_warnings(),
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(_) => Vec::new(),
            ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::SevenZ(_) => Vec::new(),
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
    /// # Range
    /// `1..=1000` for RAR5. The percentage is stored as a vint since RAR
    /// 6.10, which raised the maximum recovery record from 99% to 1000%, so
    /// `rar -rr1000p` produces an ordinary readable archive whose percentage
    /// does not fit a byte. This returned `Option<u8>` until 0.5.0 and
    /// answered `None` for every such archive; widening it to `Option<u16>`
    /// (ticgit 7ca208) is a breaking change taken inside the 0.5.0 window
    /// rather than a major bump of its own.
    ///
    /// A RAR4 archive still cannot report more than 100: that format stores
    /// no percentage field, so the value is derived from the recovery/total
    /// block counts and capped. On the RAR4 path the wider type is
    /// representational only.
    ///
    /// # Returns
    /// - `Ok(Some(percentage))` - Recovery records present with known percentage
    /// - `Ok(None)` - No recovery records present, *or* the record is present
    ///   but its size is not readable here — which is the answer for a
    ///   header-encrypted (`rar -hp`) archive, whose percentage lives in a
    ///   block this password-less byte walk is not permitted to decrypt
    ///   (ticgit 3f8790). `has_recovery_record()` still reports `true` for it.
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
    pub fn recovery_percentage(&self) -> Result<Option<u16>> {
        // R0071-0012: same write-mode rejection as
        // `has_recovery_record` so a write handle never silently
        // answers `None`.
        if self.mode == ArchiveMode::Write {
            return Err(ArchiveError::write_mode_only("recovery_percentage"));
        }
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.recovery_percentage(),
            // Other formats don't support recovery records
            ArchiveBackend::ZipWriter(_)
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
    /// Modify-mode handles answer for the on-disk source archive;
    /// pending (uncommitted) operations are not reflected. After
    /// [`Archive::commit_changes`] the file is rewritten and the
    /// source's solid layout is not necessarily preserved.
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
        // R0071-0012: write-mode handles can't answer for an archive
        // that doesn't exist yet — reject explicitly instead of
        // silently returning `false` for a `ZipWriter`.
        if self.mode == ArchiveMode::Write {
            return Err(ArchiveError::write_mode_only("is_solid"));
        }
        // R0079-0035: `Archive::modify` wraps every format in the
        // Libarchive backend, so backend-variant dispatch alone would
        // silently answer `false` for a solid 7z source. Answer for
        // the on-disk pre-commit archive through a transient SevenZ
        // read-side probe — the same reopen-from-source pattern the
        // modify-open encryption probe uses. No password is needed:
        // encrypted archives are rejected by `Archive::modify` up
        // front.
        if self.mode == ArchiveMode::Modify && self.format == ArchiveFormat::SevenZip {
            // R0001-0003: the probe is a pathname reopen, so it is subject to
            // the same DCR-007 hazard every other modify-mode pathname step
            // is — a non-cooperating process can drop a different inode at the
            // path and have `is_solid` answer for an archive this handle never
            // locked and will not commit over. Bracket the reopen with the
            // shared modify-side revalidation: before, so the probe reads the
            // locked inode, and after, so a swap *during* the probe cannot
            // leave a stale answer in the caller's hands.
            let locked = self
                .modifications
                .as_ref()
                .and_then(|tracker| tracker.locked_identity);
            let path = self.source_path_for_reopen().to_path_buf();
            crate::modification::revalidate_locked_identity("is_solid", &path, locked)?;
            let solid = SevenZArchive::open(&path)?.is_solid()?;
            crate::modification::revalidate_locked_identity("is_solid", &path, locked)?;
            return Ok(solid);
        }
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.is_solid(),
            ArchiveBackend::SevenZ(sevenz) => sevenz.is_solid(),
            ArchiveBackend::ZipWriter(_)
            | ArchiveBackend::ZipReader(_)
            | ArchiveBackend::Libarchive(_) => {
                // ZIP, TAR, and other formats don't support solid compression
                Ok(false)
            }
        }
    }

    /// Check that this handle actually points at a usable archive.
    ///
    /// [`Archive::open`] does not parse the archive — it detects the
    /// format and builds a handle. `open` returning `Ok` therefore does
    /// **not** mean the file is a valid archive; parsing happens on the
    /// first call that needs the contents, so a corrupt ZIP opens
    /// successfully and fails at [`Archive::list_files`]. That
    /// first-operation timing is the documented contract (AD 0052). Two
    /// backends check a little more at open-time and so fail earlier:
    /// libarchive-backed formats probe the first entry header inside
    /// `open`, and RAR reads the archive's main header. Neither looks
    /// past that point, so damage further in still surfaces here.
    ///
    /// `validate` is the explicit way to ask the question `open` leaves
    /// open. It forces that first parse now and reports its outcome:
    /// `Ok(())` means the directory / TOC / header stream parsed and the
    /// archive is readable; an `Err` is the same error the next real
    /// operation would have returned.
    ///
    /// # `validate` vs [`Archive::validate_integrity`]
    ///
    /// Two similar names, two very different costs — pick deliberately:
    ///
    /// - **`validate`** reads *metadata only*: one central-directory /
    ///   TOC / header-stream parse, `O(entries)`. No payload byte is
    ///   decoded. Answers **"can this archive be read at all?"**
    /// - **[`Archive::validate_integrity`]** decodes and checksums
    ///   **every entry's payload**, `O(uncompressed bytes)`, and returns
    ///   a per-entry [`crate::ValidationReport`]. Answers **"is every
    ///   stored byte intact?"**
    ///
    /// The gap between them is orders of magnitude on any real archive.
    /// Reaching for `validate_integrity` to answer a well-formedness
    /// question pays a full decompression pass for it.
    ///
    /// # Cost is paid once, not twice
    ///
    /// The parse `validate` forces is the very parse
    /// [`Archive::list_files`] would have forced, and it is memoised for
    /// the lifetime of the handle (AD 0065 frozen listing cache), so a
    /// later `list_files` is served from that cache rather than parsing
    /// again. Calling `validate` before working with an archive costs
    /// nothing beyond what the first operation was going to cost anyway.
    ///
    /// # Errors
    ///
    /// - the format error the first parse produces for a corrupt,
    ///   truncated, misdetected, or missing input;
    /// - [`ArchiveError::WriteModeOnly`] for a write-mode handle — there
    ///   is no archive to validate until [`Archive::finish`] has run.
    ///   Modify-mode handles validate normally.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("maybe-corrupt.zip")?;
    /// // `open` succeeded, which says nothing about the contents:
    /// match archive.validate() {
    ///     Ok(()) => println!("readable; listing is already parsed and cached"),
    ///     Err(e) => println!("not a usable archive: {e}"),
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn validate(&self) -> Result<()> {
        // The probe is supplied by the backend
        // ([`crate::backend::ReadBackend::validate`]) so the per-backend
        // timing differences live in one place, and it goes through the
        // mode-aware dispatcher so a Libarchive-backed *write* handle
        // surfaces `WriteModeOnly` instead of reopening the in-progress
        // output file (R0070-0001).
        //
        // Op label: `list_files` because that is literally the parse this
        // forces, so a failure names the operation that produced it. A
        // dedicated `Operation::Validate` label would read better in the
        // `WriteModeOnly` case but lives in `src/error.rs`, outside this
        // change's ownership.
        crate::backend::dispatch_read_archive(self, crate::error::ops::LIST_FILES, |b| b.validate())
    }

    /// Get the archive file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path that internal reopens (password-aware extraction, ratio
    /// preflight, fresh listings) must use when the archive needs to be
    /// reopened from disk. For SFX/offset archives this is the staged
    /// payload tempfile; for ordinary archives it is the caller-facing
    /// path. Always present and stable for the lifetime of the
    /// `Archive` (R0071-0001).
    pub(crate) fn source_path_for_reopen(&self) -> &Path {
        match &self._backing_tempfile {
            Some(PayloadSource::Staged(temp)) => temp,
            // Nothing was copied: the caller's own file *is* the source,
            // and the backend re-derives the payload offset from it.
            Some(PayloadSource::InPlace { .. }) | None => self.path.as_path(),
        }
    }

    /// Whether this handle's bytes were copied to a tempfile before the
    /// backend saw them, or are read straight out of the file
    /// [`Archive::path`] names.
    ///
    /// The answer is [`PayloadAccess::InPlace`] for every ordinary
    /// archive (nothing to copy) and for an SFX/offset open that a
    /// backend could serve from the original file;
    /// [`PayloadAccess::Staged`] when the embedded payload was copied
    /// out under the AD 0040 ceiling
    /// ([`crate::ExtractionLimits::max_sfx_payload_size`]).
    ///
    /// Why it is public: the ceiling bounds a copy, so it binds on one
    /// of these and cannot bind on the other. Without a way to ask,
    /// "did my `max_sfx_payload_size` have any effect on this handle?"
    /// is unanswerable — and the answer decides whether a multi-GB SFX
    /// needs temp-volume headroom.
    ///
    /// ```no_run
    /// use unified_archive::{Archive, archive::PayloadAccess};
    ///
    /// let archive = Archive::open_sfx("installer.exe")?;
    /// match archive.payload_access() {
    ///     PayloadAccess::InPlace => println!("read in place; no temp space used"),
    ///     PayloadAccess::Staged => println!("payload copied to a tempfile"),
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn payload_access(&self) -> PayloadAccess {
        match &self._backing_tempfile {
            Some(PayloadSource::Staged(_)) => PayloadAccess::Staged,
            Some(PayloadSource::InPlace { .. }) | None => PayloadAccess::InPlace,
        }
    }

    /// The archive file the active read backend is bound to
    /// (OI-0001-002), or `None` when nothing is bound: a write-mode
    /// handle, a backend whose open-time `stat` failed, or a read
    /// backend no operation has warmed yet.
    ///
    /// All four read backends carry a binding and re-check it at their
    /// own by-path re-opens; this accessor exists for the one by-path
    /// re-resolution that happens at the *facade* altitude instead —
    /// [`Archive::payload_size_for_ratio`].
    pub(crate) fn bound_file_identity(&self) -> Option<crate::fs_identity::FileIdentity> {
        match &self.backend {
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar.bound_identity(),
            ArchiveBackend::SevenZ(sevenz) => sevenz.bound_identity(),
            ArchiveBackend::ZipReader(zip) => zip.bound_identity(),
            ArchiveBackend::Libarchive(libarchive) => libarchive.bound_identity(),
            // Write mode: there is no archive on disk yet to bind to.
            ArchiveBackend::ZipWriter(_) => None,
        }
    }

    /// Compressed-size denominator for ratio gates. For SFX archives
    /// this is the embedded payload's size — the staged tempfile's
    /// length, or, for an in-place handle, the source file's length
    /// minus the payload offset. Outer-executable stub bytes are
    /// excluded either way: counting them would inflate the denominator
    /// and quietly loosen the zip-bomb ratio gate by the size of the
    /// stub (R0069-0006). For ordinary archives it's the file size on
    /// disk.
    ///
    /// # Why this stat is identity-guarded (OI-0001-002)
    ///
    /// This is the compression-ratio *denominator*, and the numerator
    /// comes from the AD 0065 cached listing. Re-resolving the pathname
    /// here therefore straddles two files whenever the archive is
    /// replaced after the listing snapshot — and a *smaller* replacement
    /// talks the zip-bomb gate down rather than up:
    ///
    /// > `a.zip` holds one entry declaring 900 MiB uncompressed in a
    /// > 100 KiB archive; the ratio is ~9216 and the 1000 default
    /// > refuses it. The caller opens and lists, warming the listing and
    /// > the identity binding. A 2 MiB blob is renamed over `a.zip` — it
    /// > need not even be a valid ZIP, nothing parses it. `extract_all`
    /// > still sees 900 MiB in the numerator, this stat returns 2 MiB,
    /// > the ratio reads 450, the gate passes, and extraction proceeds
    /// > through the *cached* descriptor — the original bomb — writing
    /// > 900 MiB.
    ///
    /// ZIP is the worst case, because once its descriptor is cached
    /// there is no second by-path open in that backend for an identity
    /// refusal to come from. On the other three the same straddle
    /// happens here and is only masked by the later re-open refusing.
    /// So the guard belongs at the stat, not downstream of it.
    ///
    /// The bare stat is still the answer when there is **no** binding:
    /// write mode, a backend whose capture failed, and the modify-mode
    /// handles that never bound. Both SFX shapes are guarded rather than
    /// excepted — the staged arm's binding is on the tempfile and so is
    /// the stat, and the in-place arm compares whole-file identities and
    /// only then subtracts the payload offset, so an offset-adjusted
    /// length is never compared against a whole-file one.
    pub(crate) fn payload_size_for_ratio(&self, op: &str) -> Result<u64> {
        let path = self.source_path_for_reopen();
        // One `stat`, used for both the comparison and the answer: a
        // second round-trip would reopen the very window this closes.
        // A failed `stat` is `Io { operation: "stat", .. }`, not a swap
        // report — the crate does not know anything was replaced.
        let found = crate::fs_identity::FileIdentity::stat(path)?;
        if let Some(expected) = self.bound_file_identity() {
            if found != expected {
                // The caller's own op, threaded from the six ratio-gate
                // call sites. A fixed label was tried first and was
                // wrong: this gate runs *before* the backend re-opens,
                // so its label wins the race, and a fixed one silently
                // downgraded refusals that used to name `extract_file`
                // or `extract_to_memory` precisely. Two 7z tests caught
                // it — see the note on `Archive::payload_size_for_ratio`.
                return Err(crate::fs_identity::identity_drift(
                    expected,
                    Some(found),
                    path,
                    op,
                ));
            }
        }
        match &self._backing_tempfile {
            Some(PayloadSource::InPlace { offset }) => Ok(found.len.saturating_sub(*offset)),
            Some(PayloadSource::Staged(_)) | None => Ok(found.len),
        }
    }

    /// Finish and close archive (for Write mode)
    ///
    /// This method finalizes the archive and writes all pending data.
    /// It must be called for archives in Write mode to ensure data is flushed.
    ///
    /// For Read mode archives this is a no-op. For Modify mode archives
    /// with pending operations it returns
    /// [`ArchiveError::OperationBlocked`]
    /// rather than silently dropping queued additions / removals
    /// (R0070-0005). Modify-mode handles with no pending operations are
    /// closed silently — they are equivalent to a `Drop` in that case.
    /// Call [`Archive::commit_changes`] to apply queued operations or
    /// [`Archive::clear_operations`] to discard them before `finish` /
    /// `close`.
    pub fn finish(mut self) -> Result<()> {
        if self.write_poisoned {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::FINISH,
                "Archive write handle is poisoned by an earlier failure; close it without calling finish() or recreate the archive",
            ));
        }
        if self.mode == ArchiveMode::Modify {
            if let Some(modifications) = self.modifications.as_ref() {
                let pending = modifications.added.len()
                    + modifications.removed.len()
                    + modifications.added_directories.len();
                if pending > 0 {
                    return Err(ArchiveError::operation_blocked(
                        crate::error::ops::FINISH,
                        format!(
                            "{} pending modify operation(s); call commit_changes() to apply or clear_operations() to discard before close()/finish()",
                            pending
                        ),
                    ));
                }
            }
        }
        if self.mode == ArchiveMode::Write {
            // R0001-0018 knock-on: finalization now records a sticky
            // terminal failure, so a second attempt re-reports the error
            // instead of returning `Ok(())`. Mark the handle finalized on
            // the failure path too — the caller *did* call `finish()` and
            // already holds the error, so letting `Drop` re-enter would
            // print the "silent finalize during Drop … Call
            // Archive::finish()" advice to someone who followed it.
            let result = finalize_write_backend(&mut self.backend);
            self.finalized = true;
            result?;
            return Ok(());
        }
        // R0075-0005: mark as finalized so the Drop impl does not
        // print the "did not call finish()" warning when this success
        // path runs.
        self.finalized = true;
        Ok(())
    }

    /// Close the archive (called automatically on drop).
    ///
    /// Same contract as [`Archive::finish`] including the
    /// pending-modify-operations rejection.
    pub fn close(self) -> Result<()> {
        self.finish()
    }
}

/// Shared finalize routine used by [`Archive::finish`] and the `Drop` impl.
/// Centralizing the dispatch keeps the error-propagating and error-ignoring
/// paths in lockstep — previously each path carried its own match block that
/// could drift.
fn finalize_write_backend(backend: &mut ArchiveBackend) -> Result<()> {
    match backend {
        ArchiveBackend::ZipWriter(writer) => writer.finish(),
        ArchiveBackend::Libarchive(b) => b.close_write(),
        #[cfg(feature = "rar-support")]
        ArchiveBackend::Unrar(_) => Err(ArchiveError::read_only_backend(crate::error::ops::FINISH)),
        ArchiveBackend::SevenZ(_) | ArchiveBackend::ZipReader(_) => {
            Err(ArchiveError::read_only_backend(crate::error::ops::FINISH))
        }
    }
}

#[cfg(feature = "v2-api")]
impl Archive {
    /// Finalize an unfinished Write-mode handle on behalf of the typed
    /// [`crate::v2::WriteArchive`]'s `Drop` (R0076-0088).
    ///
    /// [`crate::v2::WriteArchive::finish`] is the durable,
    /// error-surfacing commit path. When a `WriteArchive` is instead
    /// dropped without `finish`, its `Drop` calls this so the typed
    /// handle is the **single** finalization owner: it runs the same
    /// best-effort finalize the legacy [`Archive`] `Drop` would (so the
    /// libarchive write handle is freed and the ZIP central directory
    /// flushed rather than leaked), returns the result for the typed
    /// handle's one-line warning, and marks the handle `finalized` so
    /// the inner `Archive`'s `Drop` neither finalizes nor warns a
    /// second time. A poisoned backend is not force-finalized (its
    /// state is undefined); the error is returned for the warning only
    /// and never silently re-tried.
    pub(crate) fn finalize_write_on_drop(&mut self) -> Result<()> {
        if self.finalized || self.mode != ArchiveMode::Write {
            self.finalized = true;
            return Ok(());
        }
        self.finalized = true;
        if self.write_poisoned {
            return Err(ArchiveError::operation_blocked(
                crate::error::ops::FINISH,
                "write handle is poisoned; backend state is undefined, not finalizing on drop",
            ));
        }
        finalize_write_backend(&mut self.backend)
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        // R0075-0005: Drop is best-effort and CANNOT propagate
        // errors. The contract documented on `Archive::finish` (and
        // mirrored by `Archive::close`) is that callers MUST call
        // one of those methods to commit a Write- or Modify-mode
        // archive. When Drop runs without an explicit finalize:
        //
        // - Write mode: try the finalize anyway (legacy best-effort
        //   behavior so a panicking caller doesn't leave a corrupt
        //   half-written archive on disk if the backend can recover).
        //   Print to stderr so the missed-finalize is at least
        //   observable. Suppress when `write_poisoned` is set
        //   because the backend state is undefined and a forced
        //   finalize could re-trip the same failure.
        // - Modify mode: nothing to do — `commit_changes()` is the
        //   durability boundary and the open Modify handle has no
        //   on-disk side effect to commit.
        // - Read mode: no-op.
        if self.mode == ArchiveMode::Write && !self.finalized {
            if self.write_poisoned {
                eprintln!(
                    "unified-archive: dropping a poisoned write handle for `{}` without finalize. \
                     Output may be incomplete; the file should not be considered durable.",
                    self.path.display()
                );
            } else if let Err(e) = finalize_write_backend(&mut self.backend) {
                eprintln!(
                    "unified-archive: silent finalize during Drop for `{}` failed: {}. \
                     Call Archive::finish() / Archive::close() to surface this error explicitly.",
                    self.path.display(),
                    e
                );
            }
        }
    }
}

#[cfg(feature = "v2-api")]
pub mod mode_split;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod in_place_payload_tests;

#[cfg(test)]
mod sfx_fallback_error_tests;

#[cfg(test)]
mod sfx_payload_cap_tests;

/// OI-0081-001: read-side file-identity capture/revalidation. Exercises
/// the `capture_read_identity` / `revalidate_read_identity` helpers
/// directly (a stable file passes; an inode swap at the same pathname is
/// refused) plus the happy path through `Archive::open`. Unix-gated: the
/// drift assertion relies on `rename` handing the pathname a fresh inode,
/// which the `(dev, ino)` guard catches even at an identical length; the
/// non-Unix fallback is length-only and cannot see a same-size swap.
#[cfg(all(test, unix))]
mod read_identity_tests {
    use super::{
        Archive, DEFAULT_SFX_PAYLOAD_CAP, capture_read_identity, revalidate_read_identity,
    };
    use crate::error::ArchiveError;
    use crate::format::ArchiveFormat;
    use crate::test_utils::fixture;
    use std::io::Write;

    fn write_file(path: &std::path::Path, bytes: &[u8]) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }

    #[test]
    fn stable_file_revalidates_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.bin");
        write_file(&path, b"payload-bytes-unchanged");

        let id = capture_read_identity(&path).expect("capture");
        // No mutation between capture and revalidate: identity holds.
        revalidate_read_identity("open", &path, id).expect("stable file must revalidate");
    }

    #[test]
    fn same_length_inode_swap_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.bin");
        write_file(&path, b"original-detected-bytes");

        // Capture identity from the file detection would have seen.
        let id = capture_read_identity(&path).expect("capture");

        // A non-cooperating process swaps a *different* inode of the SAME
        // length into the pathname between the two opens. Length-only
        // guards would miss this; the Unix (dev, ino) identity catches it.
        let replacement = dir.path().join("replacement.bin");
        write_file(&replacement, b"replacement-attack!!!!!");
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            std::fs::metadata(&replacement).unwrap().len(),
            "test precondition: replacement must match the original length",
        );
        std::fs::rename(&replacement, &path).unwrap();

        let err = revalidate_read_identity("open", &path, id)
            .expect_err("inode swap at the pathname must be refused");
        assert!(
            matches!(err, ArchiveError::OperationBlocked { .. }),
            "expected OperationBlocked, got: {err:?}",
        );
        assert!(
            err.to_string().contains("identity changed while opening"),
            "message must name the identity drift, got: {err}",
        );
    }

    #[test]
    fn sfx_staging_binds_to_the_detection_open_identity() {
        // R0001-0002: `detect_sfx` captures the identity of the inode it
        // actually read, and staging is bound to *that* value — so an inode
        // swapped in after detection returned is refused instead of becoming
        // the trusted baseline. The previous flow re-stat'ed the pathname
        // after detection had closed its handle, which adopted the
        // replacement.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.sh");

        let mut sfx = b"#!/bin/sh\n".to_vec();
        sfx.extend(vec![0u8; 100]); // stub padding
        let mut zip_header = [0u8; 30];
        zip_header[..4].copy_from_slice(b"PK\x03\x04");
        zip_header[8] = 8; // deflate
        sfx.extend_from_slice(&zip_header);
        sfx.extend(vec![0u8; 200]);
        write_file(&path, &sfx);

        let detection = Archive::detect_sfx(&path).expect("detect");
        assert!(detection.is_sfx(), "fixture must screen as a probable SFX");
        let detected_id = detection
            .source_identity()
            .expect("detection must carry the identity of its own open");
        assert_eq!(
            detected_id,
            capture_read_identity(&path).expect("capture"),
            "an untouched file keeps the identity detection recorded",
        );

        // A non-cooperating process drops a different inode of the SAME
        // length at the pathname after detection returned.
        let replacement = dir.path().join("replacement.sh");
        let mut swapped = sfx.clone();
        *swapped.last_mut().unwrap() = 0xFF;
        write_file(&replacement, &swapped);
        std::fs::rename(&replacement, &path).unwrap();

        let offset = detection.data_offset().expect("detected payload offset");
        // `Archive` does not implement `Debug`, so `expect_err` is unavailable —
        // match instead of unwrapping.
        let err = match Archive::open_at_offset_with_format_hint_and_progress(
            &path,
            offset,
            DEFAULT_SFX_PAYLOAD_CAP,
            detection.archive_format(),
            Some(detected_id),
            None,
        ) {
            Ok(_) => panic!("staging must refuse a post-detection inode swap"),
            Err(e) => e,
        };
        assert!(
            matches!(err, ArchiveError::OperationBlocked { .. }),
            "expected OperationBlocked, got: {err:?}",
        );
        assert!(
            err.to_string()
                .contains("identity changed between detection and staging"),
            "message must name the detect→stage drift, got: {err}",
        );
    }

    #[test]
    fn open_stable_archive_succeeds() {
        // Happy path through the public API: a file that is not touched
        // between detection and backend construction opens normally, so
        // the added capture+revalidate does not regress the common case.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copy.zip");
        std::fs::copy(fixture("test.zip"), &path).unwrap();

        let archive = Archive::open(&path).expect("stable archive must open");
        assert_eq!(archive.format(), ArchiveFormat::Zip);
    }
}
