//! Archive-internal namespace bookkeeping shared by the write paths.
//!
//! `NamespaceTracker` is the two-set file/directory conflict gate used by
//! BOTH Modify mode's `commit_changes` and Write mode's per-add path
//! (R0069-0052, AD 0062 A.4). It lived in `modification` for historical
//! reasons, which meant a `create`-only build had to compile the whole
//! modification module to get it. It is its own module so the two operation
//! features can be selected independently (AD 0058).

use crate::error::{ArchiveError, Result};
use std::collections::HashSet;

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

/// Canonicalise an archive-internal path for duplicate-detection
/// comparison: route through the shared `ffi::common::normalize_path`
/// (backslash → forward slash) and then trim any trailing `/` so
/// directory entries `dir/` and `dir` compare equal.
/// Used by `commit_changes` to detect paths that would land at the same
/// writer output even when their input form differs (R0069-0060 /
/// R0069-0061). Single allocation per call: `normalize_path` produces
/// the `String`, the truncation is in-place.
pub(crate) fn normalize_dup_check_path(path: &str) -> String {
    let mut normalized = crate::ffi::common::normalize_path(path);
    let trimmed_len = normalized.trim_end_matches('/').len();
    normalized.truncate(trimmed_len);
    normalized
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

/// Yield every proper ancestor directory path for an already-normalised
/// dup-check key (forward slashes, no trailing `/`). Used by
/// `commit_changes`'s namespace-collision gate to catch a vs a/b style
/// directory/file conflicts (R0069-0062). For input `"a/b/c"` returns
/// `["a", "a/b"]` in shallow-to-deep order. Empty input yields nothing.
pub(crate) fn ancestor_paths(normalized: &str) -> Vec<String> {
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
