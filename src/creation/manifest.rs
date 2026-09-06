//! Single-walk source manifest for recursive archive creation
//! (OI-0080-005).
//!
//! # Why a manifest
//!
//! `Archive::add_directory_recursive` used to observe the source tree
//! **twice**: the facade walked it once to reserve archive paths in the
//! write-mode namespace tracker (rejecting conflicts, symlinks, special
//! files and self-ingestion before any byte was written), and then each
//! writer backend walked it *again* to do the I/O. The set of entries
//! that ended up in the archive was therefore decided by the *second*
//! observation, so a source that changed between the two walks was
//! neither archived consistently nor reported:
//!
//! * a file created after the first walk was written to the archive
//!   without ever being reserved in the namespace;
//! * a file removed after the first walk vanished from the archive with
//!   no diagnostic at all — the second walk simply did not see it;
//! * a file replaced or rewritten in between was archived with the new
//!   bytes under a plan made for the old ones.
//!
//! This module makes the *first* observation authoritative. One walk
//! records every entry — path, kind, size, mtime, unix mode, and (on
//! Unix) inode identity — and the write phase emits exactly what the
//! manifest holds, in walk order, verifying each source against its
//! recorded observation immediately before it is handed to the backend.
//!
//! # What the pin does and does not promise
//!
//! The manifest pins the *set of entries*: nothing that appeared after
//! the walk can enter the archive, and nothing recorded by the walk can
//! silently drop out. Detecting an appearance would require a second
//! walk, which is precisely what this design removes, so a file created
//! after the walk is excluded by design rather than reported.
//!
//! Verification narrows, but cannot close, the window between the
//! observation and the read: the backends take a path and open it
//! themselves, so a change landing between `verify_unchanged` and the
//! backend's own stat is still possible. That residual window is
//! covered by the writers' declared-size guards (the ZIP writer bounds
//! the copy to its own stat and requires exact equality; the libarchive
//! writer classifies over/under-production as
//! `ArchiveError::declared_length_mismatch`), so it fails loudly rather
//! than silently archiving a byte count the header was not planned for.
//!
//! # How a change is reported
//!
//! A caller restoring a backup needs to tell *disappeared* from *grew*,
//! so each drift class carries its own error variant rather than one
//! generic rejection:
//!
//! | drift | error |
//! |---|---|
//! | source no longer exists | `Io` with [`std::io::ErrorKind::NotFound`] |
//! | file size differs from the manifest | `Corruption` ("grew"/"shrank") |
//! | a different object now occupies the path (type or inode changed) | `OperationBlocked` ("was replaced") |
//! | file rewritten in place at the same size | `OperationBlocked` ("was modified in place") |
//!
//! Directory entries are checked for existence, kind, and identity, but
//! not for mtime: a directory's mtime moves whenever any child is
//! created or removed, and the children have their own checks. The
//! metadata written for a directory is the *manifest's* observation, not
//! a fresh stat, so a directory's recorded timestamp and mode are the
//! ones the single walk saw.

use super::WriteBackend;
use crate::error::{ArchiveError, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Kind of a recorded source entry. Symlinks, sockets, FIFOs and device
/// nodes never reach the manifest — they are rejected during the walk.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ManifestKind {
    File,
    Dir,
}

impl ManifestKind {
    /// Noun used in drift diagnostics.
    fn noun(self) -> &'static str {
        match self {
            ManifestKind::File => "file",
            ManifestKind::Dir => "directory",
        }
    }
}

/// One entry as the single source walk observed it.
#[derive(Debug)]
pub(crate) struct ManifestEntry {
    /// Filesystem path the entry was observed at.
    pub(crate) fs_path: PathBuf,
    /// Separator-normalized, parent-rooted archive path.
    pub(crate) archive_path: String,
    pub(crate) kind: ManifestKind,
    /// Recorded length. Always 0 for directories.
    pub(crate) size: u64,
    /// Recorded mtime, `None` only where the platform does not expose
    /// one (the same best-effort treatment the writers give it).
    pub(crate) mtime: Option<SystemTime>,
    /// Recorded unix mode, `None` off Unix.
    pub(crate) mode: Option<u32>,
    /// Recorded `(dev, ino)`, so a path swap is caught even when the
    /// replacement has the same kind, size and mtime.
    #[cfg(unix)]
    identity: crate::fs_identity::InodeId,
}

impl ManifestEntry {
    /// Confirm the source still matches what the walk recorded, or
    /// report *how* it drifted (see the module docs for the mapping).
    ///
    /// Called immediately before the entry is handed to the backend, so
    /// the check covers the whole span from the walk up to the write.
    pub(crate) fn verify_unchanged(&self, op: &'static str) -> Result<()> {
        let meta = match std::fs::symlink_metadata(&self.fs_path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Kept as `Io`/`NotFound` so a caller can match the
                // "it went away" case without parsing text.
                return Err(ArchiveError::io(
                    op,
                    self.fs_path.clone(),
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "source {} disappeared after the source walk recorded it; \
                             the archive would have omitted '{}' silently",
                            self.kind.noun(),
                            self.archive_path
                        ),
                    ),
                ));
            }
            Err(e) => return Err(ArchiveError::io(op, self.fs_path.clone(), e)),
        };

        let file_type = meta.file_type();
        let same_kind = match self.kind {
            ManifestKind::File => file_type.is_file(),
            ManifestKind::Dir => file_type.is_dir(),
        };
        if file_type.is_symlink() || !same_kind {
            return Err(ArchiveError::operation_blocked(
                op,
                format!(
                    "Source {} '{}' was replaced after the source walk recorded it: \
                     the path now holds a different kind of object",
                    self.kind.noun(),
                    self.fs_path.display()
                ),
            ));
        }

        #[cfg(unix)]
        if crate::fs_identity::InodeId::from_metadata(&meta) != self.identity {
            return Err(ArchiveError::operation_blocked(
                op,
                format!(
                    "Source {} '{}' was replaced after the source walk recorded it: \
                     a different filesystem object now occupies the path",
                    self.kind.noun(),
                    self.fs_path.display()
                ),
            ));
        }

        if self.kind == ManifestKind::Dir {
            // A directory's mtime moves on every child create/remove and
            // its length is meaningless here; the children carry their
            // own checks.
            return Ok(());
        }

        let now = meta.len();
        if now != self.size {
            // DCR-011 classifies a source that no longer matches its
            // declared length as `Corruption`; this is the same
            // condition observed one step earlier.
            let direction = if now > self.size { "grew" } else { "shrank" };
            return Err(ArchiveError::corruption(
                self.archive_path.clone(),
                format!(
                    "source file '{}' {direction} after the source walk recorded it: \
                     recorded {} bytes, now {} bytes",
                    self.fs_path.display(),
                    self.size,
                    now
                ),
            ));
        }

        if meta.modified().ok() != self.mtime {
            return Err(ArchiveError::operation_blocked(
                op,
                format!(
                    "Source file '{}' was modified in place after the source walk \
                     recorded it: same {} bytes, different modification time",
                    self.fs_path.display(),
                    self.size
                ),
            ));
        }

        Ok(())
    }
}

/// Every entry one source walk observed, in walk order (pre-order,
/// children sorted by file name — a directory always precedes its
/// contents).
#[derive(Debug)]
pub(crate) struct SourceManifest {
    entries: Vec<ManifestEntry>,
}

impl SourceManifest {
    /// Walk `dir_path` **once** and record the tree.
    ///
    /// Every policy rejection the pre-walk used to perform stays here,
    /// in the same order, so diagnostics and the namespace-undo
    /// behaviour are unchanged: special files (including symlinks, which
    /// the walker never follows) are rejected in preflight rather than
    /// mid-emission (R0080-0033), and the writer's own output archive is
    /// rejected if it appears anywhere inside the source tree
    /// (R0080-0036).
    ///
    /// `reserve` is called for each recorded entry as it is observed, so
    /// the caller reserves archive paths in the *same* traversal — no
    /// second walk, and a namespace conflict still fails before the
    /// first byte is written.
    pub(crate) fn build(
        dir_path: &Path,
        output_path: &Path,
        op: &'static str,
        mut reserve: impl FnMut(&ManifestEntry) -> Result<()>,
    ) -> Result<Self> {
        use crate::ffi::common::{DirWalkKind, normalize_path, walk_directory_tree};

        let mut entries: Vec<ManifestEntry> = Vec::new();
        walk_directory_tree(dir_path, op, |item| {
            let kind = match item.kind {
                DirWalkKind::Dir => ManifestKind::Dir,
                DirWalkKind::File => ManifestKind::File,
                DirWalkKind::Special { is_symlink } => {
                    return Err(super::reject_special_entry(op, item.fs_path, is_symlink));
                }
            };
            if kind == ManifestKind::File && super::same_file_as_output(output_path, item.fs_path) {
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "Refusing to archive the destination archive '{}' found inside the source tree (self-ingestion)",
                        item.fs_path.display()
                    ),
                ));
            }

            // The walker reports the kind from the readdir entry; this
            // is the observation the manifest pins, so it is taken with
            // one `symlink_metadata` per entry. `walk_directory_tree`
            // does not surface the metadata it already holds, and that
            // signature lives outside this module — see the note in
            // `add_directory_recursive`.
            let meta = std::fs::symlink_metadata(item.fs_path)
                .map_err(|e| ArchiveError::io(op, item.fs_path.to_path_buf(), e))?;
            let observed_kind = if meta.file_type().is_dir() {
                Some(ManifestKind::Dir)
            } else if meta.file_type().is_file() {
                Some(ManifestKind::File)
            } else {
                None
            };
            if observed_kind != Some(kind) {
                // The entry changed kind between the walker's readdir
                // and this stat. Reject rather than record an
                // observation that was never coherent.
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "Source '{}' changed kind during the source walk: \
                         it can not be recorded consistently",
                        item.fs_path.display()
                    ),
                ));
            }

            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                Some(meta.permissions().mode())
            };
            #[cfg(not(unix))]
            let mode = None;

            let entry = ManifestEntry {
                fs_path: item.fs_path.to_path_buf(),
                // Normalized once here, so the path the namespace
                // reserves is exactly the path the backend is asked to
                // emit. The libarchive backend used to emit the raw
                // `to_string_lossy` form while the facade reserved the
                // normalized one, which differed on Windows.
                archive_path: normalize_path(&item.archive_path),
                kind,
                size: if kind == ManifestKind::File {
                    meta.len()
                } else {
                    0
                },
                mtime: meta.modified().ok(),
                mode,
                #[cfg(unix)]
                identity: crate::fs_identity::InodeId::from_metadata(&meta),
            };
            reserve(&entry)?;
            entries.push(entry);
            Ok(())
        })?;

        Ok(Self { entries })
    }

    /// Emit the manifest through `backend`, verifying each source
    /// against its recorded observation immediately before it is
    /// written.
    ///
    /// Every recorded directory is emitted, not just leaf ones: the
    /// walk observed the mtime and mode of *each* directory, and a
    /// non-empty directory that contributes no entry loses that
    /// metadata for good (extraction then recreates it with whatever
    /// defaults the extractor picks). The libarchive backend carries the
    /// recorded mtime/mode; the ZIP writer has no metadata-carrying
    /// directory emit yet, so ZIP directory entries still land with the
    /// writer's default options.
    ///
    /// Entries are emitted in walk order, so a directory always
    /// precedes its contents — including the source root itself, whose
    /// name the children's archive paths are rooted at. An empty source
    /// directory therefore still yields exactly one entry (R0070-0047)
    /// without a special case.
    pub(crate) fn write_into(&self, backend: WriteBackend<'_>, op: &'static str) -> Result<()> {
        match backend {
            WriteBackend::Zip(w) => {
                for entry in &self.entries {
                    entry.verify_unchanged(op)?;
                    match entry.kind {
                        ManifestKind::File => {
                            w.add_file_from_path(&entry.fs_path, &entry.archive_path)?
                        }
                        ManifestKind::Dir => w.add_directory_entry_with_metadata(
                            &entry.archive_path,
                            entry.mtime,
                            entry.mode,
                        )?,
                    }
                }
            }
            #[cfg(feature = "libarchive")]
            WriteBackend::Libarchive(b) => {
                for entry in &self.entries {
                    entry.verify_unchanged(op)?;
                    match entry.kind {
                        ManifestKind::File => {
                            b.add_file_from_path(&entry.fs_path, &entry.archive_path)?
                        }
                        ManifestKind::Dir => b.add_directory_entry_with_metadata(
                            &entry.archive_path,
                            entry.mtime,
                            entry.mode,
                        )?,
                    }
                }
            }
        }
        Ok(())
    }

    /// Recorded entries, in walk order.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests;
