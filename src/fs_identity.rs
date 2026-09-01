//! Shared file-identity primitive for the by-name revalidation guards.
//!
//! Every guard here answers one question — *is the file behind this
//! pathname still the file this handle was built from?* — because this
//! crate resolves archives by name repeatedly: once at detection, again
//! at every backend re-open. A non-cooperating process that drops a
//! different inode at the same path between two of those resolutions
//! passed every name-based guard, which compares the normalised entry
//! *name* and nothing else.
//!
//! # The read-handle binding (OI-0001-002) — what this file is mostly about
//!
//! All four read backends bind their handle to a [`FileIdentity`] and
//! re-check it at every by-pathname re-open, so the AD 0065 listing
//! snapshot and every safety-gate verdict taken from it provably
//! describe the bytes the next operation reads:
//!
//! - `crate::ffi::zip_wrapper` — bound from an `fstat` of the cached
//!   descriptor itself, so ZIP has no stat-to-open window at all; the
//!   single re-open (`open_zip_bound`) compares.
//! - `crate::ffi::sevenz_wrapper` — `open_reader` brackets the native
//!   open with a compare-before / bind-or-compare-after pair.
//! - `crate::ffi::libarchive_wrapper` — the widest window in the crate
//!   (the read handle is iterator-shaped, so *every* operation re-opens
//!   by name) and the weakest per-entry metadata to guard it with;
//!   `bound_read_handle` brackets each one.
//! - `crate::ffi::wrapper` — the UnRAR `fresh_handle` re-open, bracketed
//!   the same way.
//!
//! The facade adds one more: [`crate::archive`]'s
//! `payload_size_for_ratio` revalidates before the `stat` that produces
//! the compression-ratio *denominator*, so a smaller file cannot be
//! swapped in to talk the zip-bomb gate down while the numerator still
//! comes from the cached listing.
//!
//! # The older, narrower guards, which still stand
//!
//! - [`crate::modification`] — the advisory-locked archive
//!   (`LockedFileIdentity`; DCR-007), `(dev, ino)` only, skipped off
//!   Unix.
//! - [`crate::archive`] — the read-side detect -> construct binding
//!   (`ReadFileIdentity`; OI-0081-001).
//! - `crate::ffi::wrapper` — the UnRAR recovery-metadata reader
//!   (`revalidate_identity`; R0080-0061) and the open-time bracket
//!   (R0081-0064), both deliberately comparing the `(dev, ino)` half
//!   only so they keep exactly the reach they were reviewed with.
//!
//! Each wraps the same Unix `(dev, ino)` extraction, which lived in
//! triplicate (three separate `MetadataExt` sites) before this type
//! collapsed it to one audited spot. The revalidation *semantics* stay
//! per-site — the read-handle bindings compare `(dev, ino)` *and*
//! length, the modify and UnRAR-legacy guards compare `(dev, ino)` only
//! and skip the check off Unix — but the capture is now single-sourced,
//! and so is the refusal wording ([`identity_drift`]).
//!
//! # Why not an owned-fd / `archive_read_open_fd` hand-off?
//!
//! The revalidation only narrows a check-to-use window; the obvious
//! stronger fix (DCR-007 / OI-0081-001 both name it) is to capture one
//! identity-verified descriptor at open and hand *that* fd to the backend
//! so no pathname is ever re-resolved. `archive_read_open_fd` exists and
//! links, but the hand-off does not work for this crate's libarchive
//! backend and was rejected (DCR-007 amendment 2026-07-22, R0081 I6):
//!
//! - libarchive's read handle is iterator-shaped and cannot be rewound, so
//!   the backend reopens the file once per operation (list, extract,
//!   integrity, ...). There is no single fd to hold for the handle's
//!   lifetime — the fd would have to be re-handed per operation.
//! - Re-handing one owned fd per operation shares a single file offset
//!   across every reopen, but the public API hands out `StreamingExtractor`
//!   values that outlive the call and permits further reads on the same
//!   handle (reads are allowed in modify mode too), so two live libarchive
//!   handles reading from one offset would corrupt each other. The
//!   path-based reopen gives each handle an independent offset today.
//! - There is no portable way to derive an *independent* open-file
//!   description from an existing fd: `dup(2)` shares the offset by
//!   definition, and on macOS `/dev/fd/N` shares it too (verified on the
//!   dev host). Only Linux `/proc/self/fd/N` yields a fresh description,
//!   and macOS + Windows are first-class targets.
//! - The one fd-anchored reopen that *would* keep independent offsets —
//!   open by name, then `fstat` the fresh fd against the pinned identity —
//!   is exactly this revalidation. The modify guard already captures from
//!   the authoritative advisory-lock descriptor (`File::metadata` is an
//!   `fstat`), so the identity baseline is already as strong as an fd
//!   hand-off would make it; only the residual re-resolution window
//!   remains, and it is within MADR-0009's accepted cooperating-process
//!   threat model.

/// Unix `(dev, ino)` inode identity — the single capture point for the
/// revalidation guards. See the module docs.
///
/// Only defined on Unix: the off-Unix guards either skip revalidation
/// (modify, UnRAR) or fall back to a length-only check (read), so no
/// non-Unix inode primitive is needed.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InodeId {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
}

#[cfg(unix)]
impl InodeId {
    /// Extract `(dev, ino)` from already-obtained metadata.
    ///
    /// Callers pass [`std::fs::File::metadata`] (an `fstat` of an open
    /// descriptor — authoritative, immune to a path swap) where they hold
    /// the descriptor, and [`std::fs::metadata`] (a path `stat`) at the
    /// revalidation point.
    pub(crate) fn from_metadata(meta: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        InodeId {
            dev: meta.dev(),
            ino: meta.ino(),
        }
    }
}

/// Identity of the archive file a *read handle* is bound to, for the
/// window this crate could not previously guard: the AD 0065 listing
/// snapshot is taken once, and every later operation re-opens the archive
/// **by pathname**.
///
/// A same-name replacement between those two moments passed every drift
/// guard, because those guards compare the normalised entry *name* and
/// nothing else (OI-0001-002). Binding the handle to the file makes the
/// question "is this the same file?" instead of "is the n-th entry still
/// called the same thing?", which is what the guards were being asked to
/// answer and could not.
///
/// # What it is, and where it degrades
///
/// - **Unix:** `(dev, ino)` plus the byte length.
/// - **Everywhere else:** the byte length alone — `InodeId` is Unix-only,
///   because the stable-`std` surface has no inode analogue
///   (`volume_serial_number` / `file_index` are nightly
///   `windows_by_handle`). A replacement that changes the length is still
///   refused; a *same-length* replacement off Unix is not caught here.
///   That is the same degradation [`crate::archive`]'s open-time guard
///   already documents, and it is deliberately not papered over with
///   `last_write_time`: a timestamp would strengthen the heuristic while
///   being trivially forgeable, and selling forgeable metadata as
///   identity is worse than a documented gap.
///
/// # `len` is part of the identity, on purpose
///
/// `(dev, ino)` alone would be blind to the two most convenient ways to
/// replace an archive in place. `std::fs::copy` onto an existing path
/// opens and truncates the destination rather than replacing it, so the
/// inode survives (verified on the dev host: same inode, different
/// length). And appending to a tar keeps the inode while adding entries
/// the safety gate never saw — the bulk-walk cardinality guard would
/// catch the extra entries, but a single-entry seek stops at its target
/// and would not.
///
/// The cost is accepted rather than hidden: a *cooperating* writer that
/// appends to an archive under a live read handle now gets a refusal
/// instead of an undefined mixed read. That is the correct answer under
/// MADR-0009's cooperating-process model — this crate's own writers never
/// mutate in place (they stage and atomically install), so the only party
/// refused is one mutating an archive under a live reader, which is the
/// shape of the attack.
///
/// # The residual, stated rather than buried
///
/// A same-inode, same-length in-place rewrite — open, truncate, write the
/// same byte count, or `pwrite` into the middle — is invisible to any
/// stat-based identity. The name and cardinality guards, the DCR-006
/// declared-size exactness bound, and ZIP/7z/RAR CRC verification remain
/// the defence for that case, which is why none of them was removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    #[cfg(unix)]
    pub(crate) inode: InodeId,
    pub(crate) len: u64,
}

impl FileIdentity {
    /// Build an identity from metadata that was already read.
    ///
    /// Prefer metadata obtained from an *open descriptor*
    /// ([`std::fs::File::metadata`], an `fstat`) where the caller holds
    /// one: it names the exact inode that will be read, with no
    /// stat-to-open window at all. A path `stat`
    /// ([`std::fs::metadata`]) is the fallback for the backends that
    /// only ever re-open by name.
    pub(crate) fn from_metadata(meta: &std::fs::Metadata) -> Self {
        FileIdentity {
            #[cfg(unix)]
            inode: InodeId::from_metadata(meta),
            len: meta.len(),
        }
    }

    /// Path `stat` that *reports* its failure — the comparison side's
    /// primitive.
    ///
    /// A `stat` that errors is not evidence of a swap, it is evidence of
    /// nothing: a transient `EIO`/`ESTALE` on a network mount, or a tmp
    /// reaper unlinking a staged payload, produce exactly this. Those
    /// reach the caller as [`crate::error::ArchiveError::Io`] with the `"stat"`
    /// operation, which is what a retry policy keys on, rather than as a
    /// false "identity changed" alarm. The operation is refused either
    /// way — this is about *which* refusal, not about whether to refuse
    /// (compare [`FileIdentity::revalidate`]).
    pub(crate) fn stat(path: &std::path::Path) -> crate::error::Result<Self> {
        std::fs::metadata(path)
            .map(|m| Self::from_metadata(&m))
            .map_err(|e| crate::error::ArchiveError::io("stat", path.to_path_buf(), e))
    }

    /// Path `stat` variant. `None` when the file cannot be stat'd.
    ///
    /// Capture is deliberately best-effort: a backend that cannot stat
    /// its own archive still opens (the native open is the authority on
    /// whether the file is usable), it simply carries no binding. What
    /// is *not* best-effort is the comparison — see
    /// [`FileIdentity::revalidate`], which fails closed on a stat error
    /// once a binding exists.
    pub(crate) fn capture(path: &std::path::Path) -> Option<Self> {
        Self::stat(path).ok()
    }

    /// Re-`stat` `path` and refuse `op` unless it still matches `expected`.
    ///
    /// Fails closed twice over: on a mismatch, and on a stat that no
    /// longer succeeds at all. The second case matters — a file that
    /// vanished between the binding and here is exactly as disqualifying
    /// as one that was swapped, and treating an unreadable stat as
    /// "unchanged" would turn the guard off precisely when something is
    /// wrong.
    ///
    /// The two are *refused alike but reported apart*. A mismatch is the
    /// [`identity_drift`] refusal ([`crate::error::ArchiveError::OperationBlocked`],
    /// "identity changed"); a `stat` that errored is
    /// [`crate::error::ArchiveError::Io`] naming the `"stat"` operation, because the
    /// crate does not know that anything was swapped and saying so would
    /// both mislead the operator and hide the condition from a
    /// retry-on-`Io` caller.
    pub(crate) fn revalidate(
        expected: Self,
        path: &std::path::Path,
        op: &str,
    ) -> crate::error::Result<()> {
        let found = Self::stat(path)?;
        if found == expected {
            Ok(())
        } else {
            Err(identity_drift(expected, Some(found), path, op))
        }
    }
}

/// The single phrasing for read-handle identity drift.
///
/// One constructor, so the four backends cannot drift into four
/// vocabularies for one condition. Deliberately
/// [`ArchiveError::OperationBlocked`] rather than a new variant, and
/// deliberately the same variant the open-time guard
/// (`archive.rs`, OI-0081-001) and the UnRAR recovery-metadata guard
/// (R0080-0061) already use: it is the same condition caught at a later
/// altitude, and a fourth guard speaking a fifth variant would be worse
/// than one variant with one message.
///
/// It stays distinguishable from *name* drift in both axes: name and
/// cardinality drift is [`ArchiveError::Format`] carrying the phrase
/// "listing drift", this is `OperationBlocked` carrying "identity
/// changed", and the two vocabularies do not overlap.
pub(crate) fn identity_drift(
    expected: FileIdentity,
    found: Option<FileIdentity>,
    path: &std::path::Path,
    op: &str,
) -> crate::error::ArchiveError {
    let found = match found {
        Some(found) => format!("{found:?}"),
        None => "unreadable".to_string(),
    };
    crate::error::ArchiveError::operation_blocked(
        op.to_string(),
        format!(
            "archive {} identity changed since this handle was opened \
             (bound to {expected:?}, now {found}); the cached listing no \
             longer describes this file, so {op} is refused rather than \
             run against bytes the safety gate never saw — reopen the \
             archive to operate on the current file",
            path.display()
        ),
    )
}
