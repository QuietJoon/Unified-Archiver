//! Shared inode-identity primitive for the by-name revalidation guards.
//!
//! Three read/modify paths capture a file's `(dev, ino)` at one open and
//! refuse the operation if the pathname's identity has drifted under a
//! non-cooperating process before a later by-name reopen:
//!
//! - [`crate::modification`] — the advisory-locked archive
//!   (`LockedFileIdentity`; DCR-007).
//! - [`crate::archive`] — the read-side detect -> construct binding
//!   (`ReadFileIdentity`; OI-0081-001).
//! - `crate::ffi::wrapper` — the UnRAR recovery-metadata reader
//!   (`UnrarFileIdentity`; R0080-0061).
//!
//! Each wraps the same Unix `(dev, ino)` extraction, which lived in
//! triplicate (three separate `MetadataExt` sites) before this type
//! collapsed it to one audited spot. The revalidation *semantics* stay
//! per-site — the read guard also compares length; the modify and UnRAR
//! guards compare `(dev, ino)` only and skip the check off Unix — but the
//! capture is now single-sourced.
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
