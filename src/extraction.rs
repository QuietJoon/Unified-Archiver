//! Archive extraction operations
//!
//! This module provides methods for extracting files from archives, including
//! full extraction, single file extraction, streaming extraction, and filtered extraction.

use crate::archive::{Archive, ArchiveBackend};
use crate::entry::ArchiveEntry;
use crate::error::ops;
use crate::error::{ArchiveError, ArchiveWarning, Result, ResultWithWarnings};
use crate::options::ExtractionOptions;
use crate::password::Password;
use crate::security::{
    ExtractionLimits, canonicalize_dest_base, check_archive_entry_count,
    check_extraction_safe_with_archive, check_single_entry_safe_with_archive,
    sanitize_entry_path_with_base,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::backend::dispatch_read_archive;

/// Ensure destination directory exists
///
/// Creates the directory and all parent directories if they don't exist.
/// Succeeds silently if the directory already exists.
fn ensure_destination(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| ArchiveError::io("create_dir", path, e))
}

/// AD 0062 A.3: gate `verify_crc32 = true` against backends that
/// cannot honour it. The TAR family / bare gzip / bzip2 / xz / ISO
/// (all libarchive-backed today) do not surface a per-entry CRC32
/// through libarchive's API, so a `true` request was previously
/// silently ignored. Now it returns `ArchiveError::Unsupported` so
/// callers must opt out consciously when working with these formats.
fn assert_verify_crc32_supported(
    archive: &Archive,
    verify_crc32: bool,
    op: &'static str,
) -> Result<()> {
    if !verify_crc32 {
        return Ok(());
    }
    if matches!(archive.backend, ArchiveBackend::Libarchive(_)) {
        return Err(ArchiveError::unsupported(
            op,
            archive.format(),
            Some(
                "Format does not carry a per-entry CRC32; \
                 set ExtractionOptions::verify_crc32 = false for libarchive-backed \
                 formats (TAR family, ISO, raw gzip/bzip2/xz)",
            ),
        ));
    }
    Ok(())
}

/// The runtime byte ceiling for a *single-entry* extraction:
/// `min(max_file_size, max_total_size)` in the backend's `None` = "no cap"
/// convention (same shape as `min_opt` in `crate::ffi::wrapper`).
///
/// R0001-0006 / R0001-0010: a single-entry read materializes exactly one
/// payload, so its extracted total equals that entry's size — handing the
/// backend only `max_file_size` let a caller with a tighter
/// `max_total_size` still have one entry decoded up to the looser per-file
/// budget. The metadata gates already apply both ceilings
/// (`check_single_entry_safe`, `ValidatedSource::extract_to_stream_by_id`);
/// this is their runtime counterpart, which is what covers entries whose
/// declared size is unknown or under-stated.
fn effective_entry_cap(limits: &ExtractionLimits) -> Option<u64> {
    match (
        limits.max_file_size().to_option(),
        limits.max_total_size().to_option(),
    ) {
        (Some(file), Some(total)) => Some(file.min(total)),
        (Some(cap), None) | (None, Some(cap)) => Some(cap),
        (None, None) => None,
    }
}

/// Report one extraction-progress sample for the single-entry disk
/// path and map a `ControlFlow::Break` vote onto
/// [`ArchiveError::Cancelled`] labelled [`ops::EXTRACT_FILE`].
///
/// The bulk paths poll through
/// `crate::ffi::common::check_extraction_cancelled`, which owns the
/// same `(processed, Some(total)) -> Cancelled` contract inside the
/// backend loops. `extract_file` cannot reach that helper: it
/// dispatches to the per-backend single-entry writers, none of which
/// take a progress hook, so the polling lives at the facade instead.
/// The `(processed, Some(total))` shape and the `Cancelled` mapping are
/// deliberately identical, so a callback written for `extract_all`
/// works here unchanged.
fn notify_extract_file_progress(
    progress: &mut Option<Box<dyn crate::options::ProgressCallback>>,
    processed: u64,
    total: u64,
) -> Result<()> {
    if let Some(cb) = progress.as_mut()
        && cb.on_progress(processed, Some(total)).is_break()
    {
        return Err(ArchiveError::Cancelled {
            operation: ops::EXTRACT_FILE,
        });
    }
    Ok(())
}

/// The byte ceiling handed to the backend *materialization* step for a
/// streaming read (R0001-0011 / DEF-004 first step).
///
/// [`effective_entry_cap`] is the hard ceiling — the caller's limits. The
/// chosen [`StreamBound`](crate::streaming::StreamBound) may tighten it
/// and never loosen it, so the result is their minimum in the backend's
/// `None` = "no cap" convention:
///
/// * `DeclaredSize` tightens to the listing's declared size (`None` when
///   the listing has none — raw gzip/bzip2/xz readers and libarchive
///   entries whose size field is unset, where there is no declaration to
///   derive a budget from);
/// * `Cap(n)` tightens to `n`, which is what makes a staging backend
///   (ZIP/7z/RAR buffer or stage the entry before returning the reader)
///   refuse an over-budget entry instead of materializing all of it and
///   then handing back a reader capped at `n`;
/// * `Unbounded` tightens nothing.
///
/// Before this existed the backend received `limits.max_file_size` alone,
/// which both ignored a tighter `max_total_size` (the gap
/// [`effective_entry_cap`] closes for the memory paths) and let
/// `Cap(1024)` on a multi-GiB entry materialize the whole entry.
fn stream_backend_cap(
    bound: crate::streaming::StreamBound,
    declared_size: Option<u64>,
    limits: &ExtractionLimits,
) -> Option<u64> {
    use crate::streaming::StreamBound;

    let ceiling = effective_entry_cap(limits);
    let tightening = match bound {
        StreamBound::DeclaredSize => declared_size,
        StreamBound::Cap(n) => Some(n),
        StreamBound::Unbounded => None,
    };
    match (ceiling, tightening) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(cap), None) | (None, Some(cap)) => Some(cap),
        (None, None) => None,
    }
}

/// Register every implicit parent directory `output_path` requires
/// (strictly between `destination` and the entry itself) into
/// `implicit_dirs`, rejecting when one of them is already claimed by a
/// file entry. Extraction-side mirror of `commit_changes`'s namespace
/// gate ancestor tracking (R0069-0062 / R0079-0018): without it an
/// archive carrying both file `a` and file `a/b` passed the preflight
/// and failed mid-extract with partial output already on disk.
fn register_required_ancestors(
    output_path: &Path,
    destination: &Path,
    entry_path: &str,
    seen_files: &HashSet<PathBuf>,
    implicit_dirs: &mut HashSet<PathBuf>,
    operation: &str,
) -> Result<()> {
    let mut ancestor = output_path.parent();
    while let Some(p) = ancestor {
        if p == destination || !p.starts_with(destination) {
            break;
        }
        if seen_files.contains(p) {
            return Err(ArchiveError::OperationBlocked {
                operation: operation.to_string(),
                reason: format!(
                    "Archive contains a file at '{}', but entry '{}' requires a directory there",
                    p.display(),
                    entry_path
                ),
            });
        }
        implicit_dirs.insert(p.to_path_buf());
        ancestor = p.parent();
    }
    Ok(())
}

fn check_overwrite_conflicts<E: std::borrow::Borrow<ArchiveEntry>>(
    entries: &[E],
    destination: &Path,
    overwrite: bool,
    operation: &str,
) -> Result<Vec<ArchiveWarning>> {
    // R0075-0009: track files and directories in separate sets so an
    // archive that carries both `a` (file) and `a/` (directory) is
    // rejected at the preflight regardless of entry order. The prior
    // implementation only deduplicated file-vs-file collisions; a
    // dir-vs-file pair would slip past the gate and surface as a
    // mid-extract failure with partial output already on disk.
    let mut seen_files = HashSet::new();
    let mut seen_dirs = HashSet::new();
    // R0079-0018: implicit parent directories required by deeper
    // entries (file `a/b` needs directory `a`), tracked separately
    // from explicit directory entries so a file entry colliding with
    // a required ancestor is rejected in both registration orders.
    let mut implicit_dirs: HashSet<PathBuf> = HashSet::new();
    // R0079-0036: the byte-exact checks below cannot see collisions that
    // only materialise on case-folding filesystems (APFS default,
    // Windows), and the destination's sensitivity cannot be known
    // portably — so case-fold collisions warn instead of blocking.
    let mut folded_paths: HashMap<String, String> = HashMap::new();
    let mut warnings: Vec<ArchiveWarning> = Vec::new();
    // Canonicalize the destination once — every entry's sanitize pass
    // compares against the same stable base.
    let canonical_dest = canonicalize_dest_base(destination)?;

    for entry in entries
        .iter()
        .map(|entry| entry.borrow())
        .filter(|entry| entry.is_file() || entry.is_directory())
    {
        // Resolve the would-be output path through the same path-policy
        // pipeline the actual extractors use (component normalization +
        // symlink-ancestor escape rejection).
        let output_path = sanitize_entry_path_with_base(&entry.path, destination, &canonical_dest)?;

        let folded = output_path.to_string_lossy().to_lowercase();
        if let Some(first) = folded_paths.get(&folded) {
            if first != &entry.path {
                warnings.push(ArchiveWarning::OutputPathCaseCollision {
                    first: first.clone(),
                    second: entry.path.clone(),
                });
            }
        } else {
            folded_paths.insert(folded, entry.path.clone());
        }

        // One stat covers every pre-existing-destination check below
        // (`is_dir()` / `exists()` each re-stat the same path).
        // `metadata` follows symlinks, matching the semantics of the
        // calls it replaces. R0080-0012: only `NotFound` counts as
        // absent — every other error kind (permission denied, I/O,
        // transient lookup failure) leaves the destination state unknown
        // and must propagate instead of being silently treated as "no
        // file there."
        let existing_meta = match std::fs::metadata(&output_path) {
            Ok(meta) => Some(meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(ArchiveError::io("metadata", output_path.clone(), e)),
        };

        if entry.is_file() {
            // File-vs-file in-batch dedup.
            if !seen_files.insert(output_path.clone()) {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Multiple entries map to same output path: '{}' (archive entry: '{}')",
                        output_path.display(),
                        entry.path
                    ),
                });
            }
            // File-vs-directory in-batch conflict (R0075-0009).
            // `implicit_dirs` extends the check to ancestors another
            // entry requires (file `a/b` first, then file `a` —
            // R0079-0018).
            if seen_dirs.contains(&output_path) || implicit_dirs.contains(&output_path) {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Archive contains both a directory and a file at '{}' (file entry: '{}')",
                        output_path.display(),
                        entry.path
                    ),
                });
            }
            register_required_ancestors(
                &output_path,
                destination,
                &entry.path,
                &seen_files,
                &mut implicit_dirs,
                operation,
            )?;

            // R0079-0017: a pre-existing directory at a file entry's
            // output path can never be replaced — every backend's
            // final install fails with an opaque OS rename error
            // (EISDIR) mid-extract. Reject regardless of `overwrite`:
            // overwrite means "replace files," not "delete a tree"
            // (mirrors the directory-entry check below).
            if existing_meta.as_ref().is_some_and(|m| m.is_dir()) {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Destination path '{}' exists as a directory; cannot extract file entry '{}' (overwrite replaces files, not trees)",
                        output_path.display(),
                        entry.path
                    ),
                });
            }

            if !overwrite && existing_meta.is_some() {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Destination file already exists: '{}' (archive entry: '{}')",
                        output_path.display(),
                        entry.path
                    ),
                });
            }
        } else {
            // Directory entry. Multiple `foo/` entries are degenerate
            // but harmless, so dir-vs-dir is silently deduplicated.
            seen_dirs.insert(output_path.clone());
            // Directory-vs-file in-batch conflict (R0075-0009).
            if seen_files.contains(&output_path) {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Archive contains both a file and a directory at '{}' (directory entry: '{}')",
                        output_path.display(),
                        entry.path
                    ),
                });
            }
            // R0079-0018: a directory entry `a/b/` requires directory
            // `a` just like a nested file does — register its
            // ancestors so a file entry at `a` is rejected in either
            // order.
            register_required_ancestors(
                &output_path,
                destination,
                &entry.path,
                &seen_files,
                &mut implicit_dirs,
                operation,
            )?;
            // The thing we *do* want to catch is a pre-existing
            // non-directory at the would-be directory path —
            // `create_dir_all` would fail later with an opaque OS
            // error, so surface a structured `OperationBlocked`
            // instead. When `overwrite` is set we still report this:
            // overwrite means "replace files," not "delete a tree."
            if existing_meta.is_some_and(|m| !m.is_dir()) {
                return Err(ArchiveError::OperationBlocked {
                    operation: operation.to_string(),
                    reason: format!(
                        "Destination path '{}' exists as a non-directory; cannot extract directory entry '{}'",
                        output_path.display(),
                        entry.path
                    ),
                });
            }
        }
    }

    Ok(warnings)
}

/// Wrap [`open_archive_for_extraction`] with the
/// `Some(password) => reopen` / `None => self` decision logic that 8+
/// extraction entrypoints repeated verbatim. Returns `None` when the
/// caller's `options.password` is empty (caller uses `self`); returns
/// `Some(reopened)` when a password was supplied (caller uses the
/// fresh handle for password-aware listing/extraction). (R0069-0002)
///
/// `source_path` must be the path the archive was actually opened from
/// — for SFX/offset archives that is the staged tempfile, not the
/// caller-facing outer path. Pass [`Archive::source_path_for_reopen`]
/// rather than `&self.path` so password-aware reopens target the
/// embedded payload, not the outer executable (R0071-0001).
fn reopen_with_password_if_set(
    source_path: &Path,
    options_password: &Option<Password>,
) -> Result<Option<Archive>> {
    match options_password.as_ref() {
        Some(pw) => Ok(Some(open_archive_for_extraction(
            source_path,
            Some(pw.as_str()),
        )?)),
        None => Ok(None),
    }
}

fn open_archive_for_extraction(path: &Path, password: Option<&str>) -> Result<Archive> {
    if let Some(password) = password {
        // R0072-0002: when the caller explicitly supplied a password
        // through `ExtractionOptions`, surface `Archive::open_encrypted`'s
        // `Unsupported` reason instead of silently falling back to the
        // unencrypted opener. The previous behavior produced a successful
        // plaintext extraction for tar/iso/gz/bz2/xz despite a meaningful
        // password being supplied, which masked option mistakes and weakened
        // the `open_encrypted` contract.
        Archive::open_encrypted(path, password)
    } else {
        Archive::open(path)
    }
}

/// Apply safety/overwrite checks to the resolved selection, then dispatch to
/// the backend's unified extraction core with the selection set. Empty
/// selections short-circuit to `Ok(())` with no warnings.
///
/// `ensure_destination` runs **after** the in-memory zip-bomb / ratio
/// safety gate but **before** `check_overwrite_conflicts`, because the
/// overwrite preflight routes through `sanitize_entry_path`, which
/// canonicalises the destination directory to detect symlink escapes
/// and therefore requires the directory to exist. A bomb-gated
/// failure leaves no empty directory behind; an overwrite-conflict
/// failure still does (matching the partial caveat documented for
/// `extract_all` / `extract_file` in AD 0060). Splitting the
/// overwrite preflight into a pure path-shape phase would let us
/// defer `mkdir` past it too — that refactor is the remaining surface
/// of R0071-0005 and is tracked alongside R0070-0028.
fn extract_selected(
    archive: &Archive,
    selected: Vec<&ArchiveEntry>,
    archive_entry_count: usize,
    options: &mut ExtractionOptions,
    operation: &str,
) -> Result<ResultWithWarnings<()>> {
    if selected.is_empty() {
        return Ok(ResultWithWarnings::ok(()));
    }

    // R0080-0011: `check_extraction_safe_with_archive` only sees the
    // selection, so an archive with millions of entries could slip past
    // `max_entry_count` behind a one-entry selection. Gate the full
    // source-listing population here before the per-selection safety gate
    // runs.
    check_archive_entry_count(archive_entry_count, &options.limits)?;

    check_extraction_safe_with_archive(
        &selected,
        archive.payload_size_for_ratio(operation)?,
        &options.limits,
    )?;

    // R0071-0005: bomb / ratio gate can fail without touching the
    // filesystem; only mkdir if it passes. The overwrite gate that
    // follows needs an existing destination for canonicalisation, so
    // the mkdir lands here rather than after the overwrite check.
    ensure_destination(&options.destination)?;

    let mut warnings = check_overwrite_conflicts(
        &selected,
        &options.destination,
        options.overwrite,
        operation,
    )?;

    let selection: HashSet<usize> = selected.iter().map(|e| e.id).collect();
    warnings.extend(dispatch_extract_core(archive, options, Some(&selection))?);
    Ok(ResultWithWarnings::with_warnings((), warnings))
}

/// Dispatch to the backend's unified extraction core with the given selection.
/// `selection = None` means "extract all"; `selection = Some(set)` means
/// "extract only entries whose positional index (matching `list_files()`
/// order) is in the set". Indexing preserves identity across duplicate
/// paths so selective extraction and selective modification stay distinct.
fn dispatch_extract_core(
    archive: &Archive,
    options: &mut ExtractionOptions,
    selection: Option<&HashSet<usize>>,
) -> Result<Vec<ArchiveWarning>> {
    // D1 / R0069-0003: build a uniform `ExtractionPlan` and dispatch
    // through the `ReadBackend::extract_all` trait method. The
    // per-backend match ladder previously lived here and grew
    // incongruent argument shapes (libarchive added two extra
    // parameters past the others over time); the trait-based
    // dispatch carries every option in one struct so backends slice
    // out only what they consume.
    // R0076-0006: route through `dispatch_read_archive` so the mode gate
    // (Write-mode rejection) lives in one place rather than relying on
    // every internal caller to pre-filter the backend variant.
    crate::backend::dispatch_read_archive(archive, ops::EXTRACT_ALL, |backend_view| {
        let mut plan = crate::backend::ExtractionPlan {
            destination: &options.destination,
            progress: options.progress.as_mut(),
            overwrite: options.overwrite,
            preserve_permissions: options.preserve_permissions,
            preserve_times: options.preserve_times,
            verify_crc32: options.verify_crc32,
            selection,
            // R0073-0004: thread the per-entry ceiling so libarchive's
            // disk-write copy_data loop honours `ExtractionLimits::max_file_size`
            // for unknown-size or misdeclared entries.
            max_file_size: options.limits.max_file_size().to_option(),
            // R0075-0010: thread the cumulative cap so the libarchive
            // disk-write loop aborts once actual decoded bytes across
            // every entry exceed the archive-level budget. Closes the
            // gap where unknown-size entries bypass the upfront
            // `check_extraction_safe` cumulative gate (which only sums
            // declared sizes).
            max_total_size: options.limits.max_total_size().to_option(),
        };
        backend_view.extract_all(&mut plan)
    })
}

impl Archive {
    /// Extract all files from archive (FR-019: Multi-part, FR-022: Symlink warnings)
    ///
    /// Extracts all files to the destination directory, preserving directory structure.
    ///
    /// **Symlinks and Hard Links (FR-022)**: Symbolic links and hard links are **skipped**
    /// during extraction with warnings on all backends. Libarchive reads link types from
    /// archive metadata; ZipReader uses
    /// `is_symlink()`; SevenZ inspects `windows_attributes`; UnRAR uses `redir_type`.
    /// Use [`Archive::check_symlinks()`] to scan for links before extraction.
    ///
    /// # Multi-Part Archives
    ///
    /// End-to-end split-volume extraction is currently supported for RAR/RAR5
    /// archives only. ZIP split volumes (`.z01`, `.z02`, ...) and 7z numeric
    /// split volumes (`.001`, `.002`, ...) are not supported.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    ///
    /// // Check for symlinks before extraction
    /// let archive = Archive::open("data.tar.gz")?;
    /// let warnings = archive.check_symlinks()?;
    /// if !warnings.is_empty() {
    ///     eprintln!("Note: {} symlinks will be skipped", warnings.len());
    /// }
    ///
    /// // Extract (symlinks automatically skipped)
    /// let options = ExtractionOptions::new("output");
    /// let result = archive.extract_all(options)?;
    /// for warning in &result.warnings {
    ///     eprintln!("warning: {warning}");
    /// }
    ///
    /// // Multi-part archive (RAR/RAR5 only; open the first part)
    /// let archive = Archive::open("backup.part1.rar")?;
    /// let (is_multipart, parts) = archive.detect_multipart()?;
    /// if is_multipart {
    ///     println!("Extracting {} parts...", parts.len());
    /// }
    /// let options = ExtractionOptions::new("output");
    /// let _ = archive.extract_all(options)?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_all(&self, mut options: ExtractionOptions) -> Result<ResultWithWarnings<()>> {
        // When the caller supplied `options.filter`, `extract_all` becomes a
        // selective extraction driven by that predicate — matching the public
        // docs for `ExtractionOptions::filter`.
        if let Some(filter) = options.filter.take() {
            return self.extract_some(filter, options);
        }

        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_ALL)?;

        let extraction_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // Security: Check extraction safety (zip bomb protection)
        // Use metadata-only listing to avoid decompressing data before limit checks
        let entries = archive
            .list_files_for_limits_budgeted(Some(options.limits.max_entry_count().as_usize()))?;
        check_extraction_safe_with_archive(
            &entries,
            archive.payload_size_for_ratio(ops::EXTRACT_ALL)?,
            &options.limits,
        )?;

        // R0070-0028 caveat: destination is created here *before* the
        // overwrite/sanitize-path conflict check because
        // `sanitize_entry_path` canonicalises `dest` to detect symlink
        // escapes — the canonicalize itself requires the directory
        // to already exist. Splitting the safety check into a
        // path-shape pass that doesn't need an existing destination
        // would let us defer the mkdir; that refactor is tracked under
        // R0070-0028 follow-up but isn't worth the canonicalize-vs-
        // existence churn for this review pass. The
        // `extract_files`/`extract_by_ids`/`extract_some`/`extract_file`
        // paths *do* defer their mkdir because their own validation is
        // in-memory.
        //
        // This caveat is about mkdir *ordering* only. The separate
        // R0076-0005 gap — that per-entry containment was decided before
        // `create_dir_all` and never rechecked afterwards — is closed:
        // every backend now creates entry parents through
        // `security::create_parent_dirs_verified`, which re-canonicalises
        // and re-checks containment after the directories exist.
        ensure_destination(&options.destination)?;
        let mut warnings = check_overwrite_conflicts(
            &entries,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_ALL,
        )?;

        // Multi-part note: the UnRAR backend handles RAR/RAR5 multi-part
        // archives when the caller opens the first volume. Other backends
        // currently expect single-file inputs.
        warnings.extend(dispatch_extract_core(archive, &mut options, None)?);
        Ok(ResultWithWarnings::with_warnings((), warnings))
    }

    /// Extract a single file from archive
    ///
    /// Rejects directory and link entries — `extract_file` only
    /// materialises regular file payloads (R0070-0027). Use
    /// [`Archive::extract_some`] / [`Archive::extract_files`] /
    /// [`Archive::extract_all`] for archives whose selection includes
    /// directories.
    ///
    /// # Progress and cancellation
    ///
    /// [`ExtractionOptions::progress`] is honoured, at **entry
    /// granularity**: the callback is invoked once at `0` before the
    /// payload is written and once at the entry's declared size after
    /// it is, both against a total of that same declared size (`0` when
    /// the format declares none — the same denominator convention the
    /// bulk paths use, where an undeclared size contributes nothing).
    /// A [`ControlFlow::Break`](std::ops::ControlFlow::Break) vote
    /// surfaces [`ArchiveError::Cancelled`] with
    /// `operation == "extract_file"`; from the first sample that aborts
    /// before anything is written, from the second it reports a
    /// completed write the same way `extract_all`'s 100% notification
    /// does.
    ///
    /// Mid-payload samples are what this path does *not* give you: the
    /// per-backend single-entry writers take no chunk hook, so a
    /// multi-GiB single entry reports twice and not per chunk. Use
    /// [`Archive::extract_by_ids`] with the entry's id (or
    /// [`Archive::extract_files`] with its path) when a byte-granular
    /// bar over one entry matters — those route through the bulk core,
    /// which polls inside the copy loop.
    pub fn extract_file(&self, file_path: &str, mut options: ExtractionOptions) -> Result<()> {
        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_FILE)?;
        let extraction_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // Use metadata-only listing to avoid decompressing data before limit checks
        let entries = archive
            .list_files_for_limits_budgeted(Some(options.limits.max_entry_count().as_usize()))?;
        // R0073-0003: count matches so duplicate-path archives surface a
        // clear `OperationBlocked` instead of silently extracting whichever
        // entry appears first. Mirrors the policy `extract_files` already
        // enforces (R0070-0033) and points callers at the identity-preserving
        // `extract_by_ids` API.
        let mut matches = entries.iter().filter(|entry| entry.path == file_path);
        let entry = matches.next().ok_or_else(|| {
            ArchiveError::format(
                Some(archive.format()),
                format!("File '{}' not found in archive", file_path),
            )
        })?;
        if matches.next().is_some() {
            return Err(ArchiveError::OperationBlocked {
                operation: ops::EXTRACT_FILE.to_string(),
                reason: format!(
                    "Archive contains multiple entries with path '{}'; extract_file cannot disambiguate. \
                     Use Archive::extract_by_ids() with the desired entry ID from list_files() instead.",
                    file_path
                ),
            });
        }

        // Reject non-file entries before creating the destination
        // (R0070-0027 / R0070-0029). Without this, a `extract_file`
        // call on a directory entry left the destination directory on
        // disk and dispatched to a backend that handled directories
        // inconsistently (some materialised them; the memory/stream
        // path rejected them). Aligning behaviour at the facade keeps
        // the contract tight.
        use crate::entry::EntryType;
        match entry.entry_type {
            EntryType::File => {}
            EntryType::Symlink => {
                return Err(crate::error::link_extract_blocked(
                    &entry.path,
                    crate::error::LinkKind::Symbolic,
                    ops::EXTRACT_FILE,
                ));
            }
            EntryType::HardLink => {
                return Err(crate::error::link_extract_blocked(
                    &entry.path,
                    crate::error::LinkKind::Hard,
                    ops::EXTRACT_FILE,
                ));
            }
            EntryType::Directory | EntryType::Other => {
                return Err(ArchiveError::OperationBlocked {
                    operation: ops::EXTRACT_FILE.to_string(),
                    reason: format!(
                        "Entry '{}' is not a regular file ({:?}); use extract_some/extract_files/extract_all for directories",
                        entry.path, entry.entry_type
                    ),
                });
            }
        }

        let to_extract = [entry];
        // Single-file extraction must run the archive-level ratio guard too:
        // CRC-less compressed formats (TAR.GZ, TAR.BZ2, TAR.XZ) lack per-entry
        // compressed sizes, so the entry-scoped `check_extraction_safe` would
        // silently let a bomb through. `check_extraction_safe_with_archive`
        // uses the archive's on-disk size as the denominator for the ratio
        // check against the target entry's uncompressed size.
        check_extraction_safe_with_archive(
            &to_extract,
            archive.payload_size_for_ratio(ops::EXTRACT_FILE)?,
            &options.limits,
        )?;

        // R0070-0029: entry-existence and entry-kind validation already
        // ran above (path lookup, EntryType match) — those are the
        // checks that benefit from being mkdir-free. The
        // `check_overwrite_conflicts` call below uses
        // `sanitize_entry_path`, which canonicalises `dest` and
        // therefore needs the directory to exist. Create it first.
        ensure_destination(&options.destination)?;
        // A single-entry batch cannot case-collide with itself, and
        // `extract_file` has no warnings channel — discard the Vec.
        check_overwrite_conflicts(
            &to_extract,
            &options.destination,
            options.overwrite,
            ops::EXTRACT_FILE,
        )?;

        // Honour `ExtractionOptions::progress`: the field used to be
        // consumed only by `dispatch_extract_core`, so this — the one
        // disk-writing entry point that does not route through it —
        // silently dropped the callback. The pre-write sample runs
        // after every gate has passed and before the backend writes a
        // byte, so a `Break` here cancels a write that has not started;
        // the denominator is the listing's declared size, which is the
        // same authority the bulk paths sum for theirs.
        let declared_size = entry.size.unwrap_or(0);
        notify_extract_file_progress(&mut options.progress, 0, declared_size)?;

        // Use the current archive handle for extraction
        let extracted = match &archive.backend {
            // R0081-0067: forward the metadata-preservation opt-outs to the
            // single-file path too. UnRAR stamps the archive's mode/times on
            // the file it writes, so honouring `preserve_* == false` means
            // overriding those after the staged write (mirroring the bulk
            // path, R0080-0023).
            #[cfg(feature = "rar-support")]
            ArchiveBackend::Unrar(unrar) => unrar
                .extract_file_with_options(
                    file_path,
                    &options.destination,
                    options.overwrite,
                    options.preserve_permissions,
                    options.preserve_times,
                )
                .map(|_| ()),
            ArchiveBackend::SevenZ(sevenz) => sevenz.extract_file_with_options_preserve(
                file_path,
                &options.destination,
                options.overwrite,
                options.preserve_permissions,
                options.preserve_times,
                options.verify_crc32,
            ),
            ArchiveBackend::ZipWriter(_) => Err(ArchiveError::write_mode_only(ops::EXTRACT_FILE)),
            ArchiveBackend::ZipReader(zip) => zip.extract_file_with_options_preserve(
                file_path,
                &options.destination,
                options.overwrite,
                options.preserve_permissions,
                options.preserve_times,
                options.verify_crc32,
            ),
            ArchiveBackend::Libarchive(libarchive) => libarchive.extract_file_with_options(
                file_path,
                &options.destination,
                options.overwrite,
                options.preserve_permissions,
                options.preserve_times,
                // R0073-0004: per-entry ceiling for the libarchive
                // disk-write loop — see the multi-entry call above.
                // R0001-0010: that loop takes a single ceiling, and this
                // dispatch writes exactly one entry, so hand it the tighter
                // of the per-file and total budgets — otherwise a caller
                // with a small `max_total_size` still gets one entry decoded
                // up to the looser `max_file_size`.
                effective_entry_cap(&options.limits),
            ),
        };
        extracted?;

        // Completion sample, mirroring the bulk paths' 100%
        // notification (R0070-0035): a `Break` here still surfaces
        // `Cancelled` even though the payload is already on disk, so a
        // callback cannot silently swallow its own last vote.
        notify_extract_file_progress(&mut options.progress, declared_size, declared_size)?;
        Ok(())
    }

    /// Extract a single file to memory.
    ///
    /// Returns the file contents as a `Vec<u8>` without writing to disk.
    ///
    /// # Limits and DOS surface
    ///
    /// This function applies [`ExtractionLimits::default`] before dispatching
    /// to the backend: the entry-count budget bounds the preflight listing,
    /// the metadata gate rejects an over-limit declared size, and the
    /// backend receives `min(max_file_size, max_total_size)` as a decoded-byte
    /// ceiling so an over-producing decoder is stopped too (R0001-0004 /
    /// R0001-0005). For caller-supplied limits, use
    /// [`Archive::extract_to_memory_with_options`].
    ///
    /// The compression-ratio guard is applied **per entry**, so an archive with
    /// many moderately-bloating entries can still exhaust caller memory if the
    /// entries are fetched serially without a running total on the caller side.
    ///
    /// # Ambiguous names
    ///
    /// `file_path` is a `&str`, so entries sharing a name — including two
    /// non-UTF-8 names that lossy-decode to the same string — cannot be told
    /// apart here. This refuses rather than picking one, and directs the
    /// caller to [`Archive::extract_by_ids`]. There is no by-ID form of this
    /// method today: resolving such a collision means extracting to disk.
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        let limits = ExtractionLimits::default();
        // R0070-0001 precedent: gate on the handle's *mode*, not on a
        // specific writer backend variant — a Write-mode libarchive
        // handle must skip the read-side preflight the same way and be
        // rejected by the dispatch-layer mode gate below.
        if self.mode != crate::archive::ArchiveMode::Write {
            // R0001-0004: budget the preflight listing by the same
            // `max_entry_count` the limits declare, exactly as
            // `extract_to_memory_with_options` does. The unbudgeted
            // `list_files()` allocated the whole table of contents before
            // the count limit was ever evaluated (OI-0080-003).
            let entries =
                self.list_files_for_limits_budgeted(Some(limits.max_entry_count().as_usize()))?;
            check_single_entry_safe_with_archive(
                &entries,
                file_path,
                self.payload_size_for_ratio(ops::EXTRACT_TO_MEMORY)?,
                &limits,
                ops::EXTRACT_TO_MEMORY,
            )?;
        }
        // R0001-0005: the metadata gate above is an early rejection only —
        // it trusts the declared size. Carry the effective byte ceiling into
        // the backend as well so a decoder that over-produces past its
        // declaration (or an unknown-size entry) is rejected before the
        // buffer is allocated, mirroring
        // `extract_to_memory_with_options`.
        self.validated_source()
            .extract_to_memory_with_limit(file_path, effective_entry_cap(&limits))
    }

    /// Extract a single file to memory with caller-supplied options.
    ///
    /// Same as [`Archive::extract_to_memory`] but honors:
    /// - `options.limits` — applied to the per-entry safety pre-check.
    /// - `options.password` — when `Some`, the archive is reopened via
    ///   [`Archive::open_encrypted`] before the read, mirroring the
    ///   password-aware behavior of [`Archive::extract_all`] /
    ///   [`Archive::extract_file`]. Formats that do not support encryption
    ///   return the same `Unsupported` error as [`Archive::open_encrypted`]
    ///   (R0072-0002); use [`Archive::open`] when extracting from
    ///   non-encryptable formats.
    /// - `options.verify_crc32` — gated up front. Setting `true` for a
    ///   backend that cannot verify CRC32 in-memory (libarchive-backed
    ///   TAR family, ISO, standalone Gzip/Bzip2/Xz/Zst/Lz4/Lzma) returns
    ///   [`ArchiveError::Unsupported`] before any I/O runs.
    ///
    /// Other fields (`destination`, `overwrite`, `preserve_*`, `filter`,
    /// `progress`) describe disk-write semantics or selective traversal
    /// and have no effect on a single in-memory read; they are ignored by
    /// this entry point.
    pub fn extract_to_memory_with_options(
        &self,
        file_path: &str,
        options: &ExtractionOptions,
    ) -> Result<Vec<u8>> {
        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_TO_MEMORY)?;
        let extraction_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = extraction_archive.as_ref().unwrap_or(self);

        if archive.mode != crate::archive::ArchiveMode::Write {
            let entries = archive.list_files_for_limits_budgeted(Some(
                options.limits.max_entry_count().as_usize(),
            ))?;
            check_single_entry_safe_with_archive(
                &entries,
                file_path,
                archive.payload_size_for_ratio(ops::EXTRACT_TO_MEMORY)?,
                &options.limits,
                ops::EXTRACT_TO_MEMORY,
            )?;
        }
        // R0072-0006: pass the per-entry byte cap so RAR's temp-staged
        // payload is rejected pre-allocation when the decoded size
        // exceeds `max_file_size`. Other backends enforce the byte cap
        // via their bounded copy helpers.
        // R0001-0006: the cap is the tighter of `max_file_size` and
        // `max_total_size` — a single read materializes the whole extracted
        // total, so the total budget bounds it too.
        archive
            .validated_source()
            .extract_to_memory_with_limit(file_path, effective_entry_cap(&options.limits))
    }

    /// Same as `extract_to_memory` but skips the per-call entry-list rebuild and
    /// safety pre-check. Internal callers (e.g. `commit_changes`) use this when
    /// the entry has already been validated against a fresh listing.
    ///
    /// **Prefer [`ValidatedSource::extract_to_memory`]** for new internal
    /// callers — the token type makes the "we already validated this
    /// entry's source" precondition a fact at the type level instead of a
    /// `_unchecked`-named comment (D3 / R0068-0039).
    pub(crate) fn extract_to_memory_unchecked(&self, file_path: &str) -> Result<Vec<u8>> {
        dispatch_read_archive(self, ops::EXTRACT_TO_MEMORY, |b| {
            b.extract_to_memory(file_path)
        })
    }

    /// Construct a [`ValidatedSource`] view for this archive (D3).
    ///
    /// The token's existence is the safety invariant: callers obtain it
    /// only after they have a validated listing of this archive in
    /// hand, so the token's `extract_*` methods can skip the per-call
    /// safety pre-check that the public `extract_to_memory` /
    /// `extract_to_stream` paths perform.
    pub(crate) fn validated_source(&self) -> ValidatedSource<'_> {
        ValidatedSource { archive: self }
    }

    /// Extract a single file to a bounded (or explicitly unbounded)
    /// stream.
    ///
    /// Returns a [`StreamingExtractor`](crate::streaming::StreamingExtractor) that
    /// implements [`Read`](std::io::Read). Only libarchive-backed formats (TAR,
    /// TAR.GZ, TAR.BZ2, TAR.XZ) truly stream — ZipReader, SevenZ,
    /// and UnRAR backends currently materialize the entry into memory before
    /// wrapping it in a [`Cursor`](std::io::Cursor) (tracked by DEF-004 and
    /// OI-0057-007). For bounded memory use on those backends, check entry size
    /// via [`find_entry`](Self::find_entry) before extraction.
    ///
    /// # Limits and DOS surface
    ///
    /// Applies [`ExtractionLimits::default`] to the entry metadata in the
    /// safety pre-check, then shapes the output cap on the returned
    /// reader according to `bound` (I2 / AD 0062 A.2):
    ///
    /// - [`StreamBound::DeclaredSize`](crate::streaming::StreamBound::DeclaredSize)
    ///   — hold the stream to **exactly** the entry's declared
    ///   uncompressed size, degrading to a ceiling-only cap at the
    ///   effective entry limit when the entry declares no size. This is
    ///   the safe default.
    /// - [`StreamBound::Cap`](crate::streaming::StreamBound::Cap)`(n)` —
    ///   ceiling-only cap at `n` bytes: a budget, not an assertion about
    ///   the entry's size.
    /// - [`StreamBound::Unbounded`](crate::streaming::StreamBound::Unbounded)
    ///   — no caller-chosen output cap (see the variant's trust caveat).
    ///
    /// R0001-0007 / R0001-0011: under every bound the backend's
    /// materialization step receives
    /// `min(bound-derived cap, max_file_size, max_total_size)`, so a
    /// staging backend (ZIP/7z/RAR buffer or stage the entry before
    /// returning the reader) cannot decode an entry larger than the
    /// caller actually asked for. With `Cap(n)` on a bigger entry that
    /// means the extract call itself fails on those backends, where the
    /// incremental libarchive path instead serves a prefix up to `n`.
    /// R0001-0008: the `DeclaredSize` size comes from the entry listing,
    /// not from the materialized buffer length.
    ///
    /// Under a hard cap, an archive that emits more bytes than its header
    /// promised does **not** reach a silent EOF at the cap:
    /// [`Read::read`](std::io::Read::read) surfaces an
    /// [`InvalidData`](std::io::ErrorKind::InvalidData) error so callers
    /// can tell a complete entry from one cut off at the limit
    /// (AD 0062 A.2 / R0080-0007 / DCR-006). Under `DeclaredSize` with a
    /// known declared size the converse also holds: a stream that ends
    /// *before* the declaration surfaces an
    /// [`UnexpectedEof`](std::io::ErrorKind::UnexpectedEof) error rather
    /// than a short read (R0001-0011 / OI-0001-001). Both read verdicts
    /// are sticky — a retry replays the error rather than falling through
    /// to `Ok(0)` (R3 / ti-4ba1ff1a).
    ///
    /// # Where a violation surfaces (DCR-006 Amendments 4 and 5)
    ///
    /// **Under [`StreamBound::DeclaredSize`](crate::StreamBound::DeclaredSize)**
    /// every bound violation surfaces **no later than the read that
    /// observes it, and never as silent success**. A backend that observes
    /// it while materializing reports it earlier, as a typed
    /// [`ArchiveError`] from this call — there, an earlier delivery of the
    /// same verdict. Budget-exceeded pairs
    /// [`ArchiveError::OperationBlocked`] with
    /// [`InvalidData`](std::io::ErrorKind::InvalidData); truncation and
    /// over-production under the declaration pair
    /// [`ArchiveError::Corruption`] with
    /// [`UnexpectedEof`](std::io::ErrorKind::UnexpectedEof) and
    /// [`InvalidData`](std::io::ErrorKind::InvalidData) respectively. The
    /// full table is on [`StreamBound`](crate::StreamBound).
    ///
    /// **Under `Cap(n)` and `Unbounded` only the budget row is uniform.**
    /// Those bounds do not ask about the declaration, so declaration
    /// integrity is enforced incidentally by the staging backends (ZIP, 7z
    /// and RAR) and not at all by libarchive, where a truncated entry
    /// reads to a clean short EOF. That is a different verdict, not an
    /// earlier one — choose `DeclaredSize` when you need the check on
    /// every backend, and `DeclaredSize` + [`Read::take`](std::io::Read::take)
    /// when you need a window as well.
    ///
    /// Under `DeclaredSize`, one handler covers both arrival points:
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ArchiveError, StreamBound};
    /// use std::io::{ErrorKind, Read};
    ///
    /// # fn damaged(_: &str) {}
    /// let archive = Archive::open("input.tar")?;
    /// let mut stream = match archive.extract_to_stream("payload.bin", StreamBound::DeclaredSize) {
    ///     Ok(s) => s,
    ///     // Arrival point 1: a staging backend saw it while materializing.
    ///     Err(e @ (ArchiveError::Corruption { .. } | ArchiveError::OperationBlocked { .. })) => {
    ///         damaged(&e.to_string());
    ///         return Ok(());
    ///     }
    ///     Err(e) => return Err(e.into()),
    /// };
    ///
    /// let mut buf = [0u8; 8192];
    /// loop {
    ///     match stream.read(&mut buf) {
    ///         Ok(0) => break,
    ///         Ok(_n) => { /* consume the chunk */ }
    ///         // Arrival point 2: the incremental path saw it on a read.
    ///         Err(e) if matches!(e.kind(), ErrorKind::UnexpectedEof | ErrorKind::InvalidData) => {
    ///             damaged(&e.to_string());
    ///             break;
    ///         }
    ///         Err(e) => return Err(e.into()),
    ///     }
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// For caller-supplied limits, password, or CRC verification, use
    /// [`Archive::extract_to_stream_with_options`].
    ///
    /// # Example
    /// ```no_run
    /// use unified_archive::{Archive, StreamBound};
    /// use std::io::Read;
    ///
    /// let archive = Archive::open("big.tar")?;
    /// let mut stream = archive.extract_to_stream("big_file.bin", StreamBound::DeclaredSize)?;
    ///
    /// let mut buffer = [0u8; 8192];
    /// loop {
    ///     // Surface read errors (cap overrun, corruption, I/O) as
    ///     // errors instead of mistaking them for EOF; a hostile
    ///     // archive can over-produce past its declared size
    ///     // (AD 0062 A.2 / R0080-0007).
    ///     let n = stream.read(&mut buffer)?;
    ///     if n == 0 { break; }
    ///     // Chunked read. NOTE: only libarchive-backed formats
    ///     // (TAR, ISO, …) stream without first buffering the entry;
    ///     // ZIP/7z/RAR materialize the entry before this loop starts.
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn extract_to_stream(
        &self,
        file_path: &str,
        bound: crate::streaming::StreamBound,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.extract_to_stream_impl(file_path, &ExtractionOptions::default(), bound)
    }

    /// Extract a single file to a stream with caller-supplied options and
    /// an explicit [`StreamBound`](crate::streaming::StreamBound).
    ///
    /// Same as [`Archive::extract_to_stream`] but honors:
    /// - `options.limits` — applied to the per-entry safety pre-check, as
    ///   the hard ceiling on the backend materialization budget
    ///   (`min(max_file_size, max_total_size)`, R0001-0011), and (for
    ///   [`StreamBound::DeclaredSize`](crate::streaming::StreamBound::DeclaredSize))
    ///   as the unknown-size hard-cap fallback.
    /// - `options.password` — when `Some`, the archive is reopened via
    ///   [`Archive::open_encrypted`] before the read, mirroring the
    ///   password-aware behavior of the disk extraction paths.
    /// - `options.verify_crc32` — gated up front. Backends that cannot
    ///   verify CRC32 on a streaming read return
    ///   [`ArchiveError::Unsupported`] before any I/O runs.
    ///
    /// Other fields (`destination`, `overwrite`, `preserve_*`, `filter`,
    /// `progress`) describe disk-write or selective traversal semantics
    /// and have no effect on a single stream read.
    ///
    /// `bound` shapes the backend budget and the output cap exactly as
    /// documented on [`Archive::extract_to_stream`];
    /// [`StreamBound::DeclaredSize`](crate::streaming::StreamBound::DeclaredSize)
    /// uses the effective entry ceiling —
    /// `min(options.limits.max_file_size, options.limits.max_total_size)`
    /// — as the unknown-size fallback (R0001-0011).
    ///
    /// Violation classification is identical to
    /// [`Archive::extract_to_stream`]'s: a bound violation arrives either
    /// as a typed [`ArchiveError`] from this call (staging backends) or as
    /// an [`io::Error`](std::io::Error) from a later read (the incremental
    /// libarchive path). Under
    /// [`StreamBound::DeclaredSize`](crate::StreamBound::DeclaredSize) the
    /// two pair; under `Cap(n)` and `Unbounded` only the budget row is
    /// uniform across backends — see the table on
    /// [`StreamBound`](crate::StreamBound) (DCR-006 Amendments 4 and 5).
    pub fn extract_to_stream_with_options(
        &self,
        file_path: &str,
        options: &ExtractionOptions,
        bound: crate::streaming::StreamBound,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.extract_to_stream_impl(file_path, options, bound)
    }

    /// Single implementation behind [`Archive::extract_to_stream`] and
    /// [`Archive::extract_to_stream_with_options`] (I2).
    ///
    /// The `bound` is interpreted in exactly one place (the trailing
    /// `match`), so the bounded and unbounded stream surfaces cannot
    /// drift apart — this structurally retires R0081-0027, where the two
    /// former `_unbounded` methods disagreed on whether the mmap cap of
    /// the day was propagated (no backend memory-maps any more). The
    /// safety pre-check and password reopen are shared unconditionally;
    /// only the output cap depends on `bound`.
    fn extract_to_stream_impl(
        &self,
        file_path: &str,
        options: &ExtractionOptions,
        bound: crate::streaming::StreamBound,
    ) -> Result<crate::streaming::StreamingExtractor> {
        use crate::streaming::StreamBound;

        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_TO_STREAM)?;
        let extraction_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // R0001-0008: the authoritative declared size comes from the
        // listing, not from the returned stream. `StreamingExtractor::from_bytes`
        // reports the *materialized* buffer length, so on the staging
        // backends (ZIP/7z/RAR) an over-producing decoder would otherwise
        // define its own declaration and `DeclaredSize` would cap at the
        // oversized payload. `check_single_entry_safe_with_archive` has
        // already rejected a missing or duplicated path here, so the
        // `find` below is unambiguous.
        let declared_size = if archive.mode != crate::archive::ArchiveMode::Write {
            let entries = archive.list_files_for_limits_budgeted(Some(
                options.limits.max_entry_count().as_usize(),
            ))?;
            check_single_entry_safe_with_archive(
                &entries,
                file_path,
                archive.payload_size_for_ratio(ops::EXTRACT_TO_STREAM)?,
                &options.limits,
                ops::EXTRACT_TO_STREAM,
            )?;
            entries
                .iter()
                .find(|e| e.path == file_path)
                .and_then(|e| e.size)
        } else {
            None
        };

        // R0001-0007: thread the caller's per-entry ceiling into the
        // backend materialization step. The `bound` output cap below only
        // wraps the returned reader, so a staging backend (RAR overrides
        // `extract_to_stream_with_limit` to enforce the cap before the
        // buffer is built) could otherwise decode an oversized entry to
        // memory or a temp file before the first `read` ever ran.
        //
        // R0001-0011 / DEF-004: the budget is now derived from `bound`
        // itself — `min(bound tightening, max_file_size, max_total_size)`
        // — so `Cap(n)` binds pre-materialization instead of only on the
        // returned reader, and the stream path stops ignoring a tighter
        // `max_total_size`. The caller's limits stay a hard ceiling that a
        // `StreamBound` can tighten but never loosen.
        let limits_cap = effective_entry_cap(&options.limits);
        let backend_cap = stream_backend_cap(bound, declared_size, &options.limits);
        let stream = archive
            .validated_source()
            .extract_to_stream_with_limit(file_path, backend_cap)?;

        // I2: the output-size bound is interpreted in exactly this one
        // place. `with_hard_cap` makes over-production a read error rather
        // than a silent EOF at the cap (R0080-0007 / DCR-006);
        // `with_exact_size` additionally makes under-production an
        // `UnexpectedEof` read error (R0001-0011 / OI-0001-001).
        Ok(match bound {
            StreamBound::DeclaredSize => match declared_size {
                // R0001-0008 made the listing size authoritative, which is
                // what makes an exactness check meaningful rather than a
                // comparison of the decoder's output against itself.
                Some(declared) => stream.with_exact_size(declared),
                // No declaration exists (raw gzip/bzip2/xz single-file
                // readers, libarchive entries whose size field is unset),
                // so none is invented: ceiling-only at the effective entry
                // limit, and an early EOF is an ordinary EOF.
                None => match limits_cap {
                    Some(cap) => stream.with_hard_cap(cap),
                    None => stream,
                },
            },
            StreamBound::Cap(n) => stream.with_hard_cap(n),
            StreamBound::Unbounded => stream,
        })
    }

    /// Same as `extract_to_stream` but skips the per-call entry-list rebuild and
    /// safety pre-check. Used by `commit_changes` to copy retained entries.
    ///
    /// **Prefer [`ValidatedSource::extract_to_stream`]** for new
    /// internal callers (D3 / R0068-0040).
    pub(crate) fn extract_to_stream_unchecked(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        dispatch_read_archive(self, ops::EXTRACT_TO_STREAM, |b| {
            b.extract_to_stream(file_path)
        })
    }

    /// Core selective extraction primitive (AD 0029).
    ///
    /// Traverses the archive once with a shared handle, materializing only
    /// entries for which `predicate` returns true. Link entries in the
    /// selection are surfaced as `SkippedSymlink`/`SkippedHardLink` warnings
    /// per FR-022 — they do not produce errors.
    ///
    /// # Bomb-detection caveat for selective extraction
    ///
    /// The archive-level compression-ratio guard
    /// (`ExtractionLimits::max_compression_ratio`) uses the archive's full
    /// on-disk size as the denominator and the **selected** uncompressed
    /// size as the numerator. For very small selections from CRC-less
    /// compressed formats (TAR.GZ, TAR.BZ2, TAR.XZ — which lack per-entry
    /// compressed sizes) this is more permissive than a true per-entry
    /// ratio would be: a hostile archive could pack a single huge entry
    /// and rely on the small selection to pull the computed ratio below
    /// the threshold. When extracting subsets from untrusted archives in
    /// those formats, tighten `ExtractionLimits::max_total_size` and
    /// `max_file_size` rather than relying on the ratio alone.
    pub fn extract_some<F>(
        &self,
        mut predicate: F,
        mut options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>>
    where
        F: FnMut(&ArchiveEntry) -> bool,
    {
        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_SOME)?;
        let extraction_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = extraction_archive.as_ref().unwrap_or(self);

        // Metadata-only listing for the safety preflight, mirroring the
        // memory/stream paths.
        let entries = archive
            .list_files_for_limits_budgeted(Some(options.limits.max_entry_count().as_usize()))?;
        let selected: Vec<&ArchiveEntry> = entries.iter().filter(|e| predicate(e)).collect();

        if selected.is_empty() {
            return Ok(ResultWithWarnings::ok(()));
        }

        // Destination materialisation is performed inside
        // `extract_selected` after both safety and overwrite gates
        // pass — empty selections short-circuit before any filesystem
        // mutation, and a blocked safety gate no longer leaves a
        // fresh empty directory behind (R0070-0030 / R0071-0005).
        extract_selected(
            archive,
            selected,
            entries.len(),
            &mut options,
            ops::EXTRACT_SOME,
        )
    }

    /// Extract files matching a predicate.
    ///
    /// Thin wrapper over [`Archive::extract_some`] — single-pass through the
    /// archive with warning-aware return (FR-022).
    pub fn extract_filtered<F>(
        &self,
        predicate: F,
        options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>>
    where
        F: FnMut(&ArchiveEntry) -> bool,
    {
        self.extract_some(predicate, options)
    }

    /// Extract multiple files by their paths.
    ///
    /// Validates every path exists in the archive, then delegates to
    /// [`Archive::extract_some`] — the archive is traversed once per
    /// AD 0029 regardless of how many paths are selected.
    ///
    /// # Arguments
    ///
    /// * `paths` - Array of file paths within the archive to extract
    /// * `options` - Extraction options including destination directory
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    ///
    /// let archive = Archive::open("backup.zip")?;
    /// archive.extract_files(
    ///     &["readme.txt", "src/main.rs", "config.json"],
    ///     ExtractionOptions::new("./output"),
    /// )?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_files(
        &self,
        paths: &[&str],
        mut options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>> {
        // No-op short-circuit precedes any filesystem mutation
        // (R0069-0011 / R0070-0031).
        if paths.is_empty() {
            return Ok(ResultWithWarnings::ok(()));
        }

        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_FILES)?;

        // Deduplicate requested paths
        let mut seen = HashSet::new();
        let paths: Vec<&str> = paths.iter().copied().filter(|p| seen.insert(*p)).collect();

        let listing_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = listing_archive.as_ref().unwrap_or(self);

        // Metadata-only listing for the safety preflight, mirroring the
        // memory/stream paths.
        let entries = archive
            .list_files_for_limits_budgeted(Some(options.limits.max_entry_count().as_usize()))?;
        // Preserve entry identity across duplicate paths: a single requested
        // path matches every entry that carries it, so selective extraction
        // honors all colliding ZIP/7z central-directory records instead of
        // collapsing them to one representative.
        let paths_set: HashSet<&str> = paths.iter().copied().collect();
        let mut selected: Vec<&ArchiveEntry> = entries
            .iter()
            .filter(|e| paths_set.contains(e.path.as_str()))
            .collect();

        // Fail fast if any requested path has no matching entry —
        // before destination creation so a typo doesn't leave a fresh
        // mkdir behind (R0070-0031).
        let found_paths: HashSet<&str> = selected.iter().map(|e| e.path.as_str()).collect();
        for &path in &paths {
            if !found_paths.contains(path) {
                return Err(ArchiveError::format(
                    Some(self.format()),
                    format!("File '{}' not found in archive", path),
                ));
            }
        }

        // R0070-0033 — collision diagnostics for archives with duplicate
        // entry paths. ZIP and 7z permit multiple central-directory
        // records carrying the same logical path; one requested path
        // would silently expand to N selected entries that collide on
        // disk. Surface the duplication with a clear, actionable error
        // pointing the caller at `extract_by_ids` (which preserves
        // identity) before any safety/overwrite gate runs.
        let mut path_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for entry in &selected {
            *path_counts.entry(entry.path.as_str()).or_insert(0) += 1;
        }
        if let Some((dup_path, count)) = path_counts.iter().find(|(_, n)| **n > 1) {
            return Err(ArchiveError::OperationBlocked {
                operation: ops::EXTRACT_FILES.to_string(),
                reason: format!(
                    "Archive contains {} entries with path '{}'; extract_files cannot disambiguate. \
                     Use Archive::extract_by_ids() with the desired entry IDs from list_files() instead.",
                    count, dup_path
                ),
            });
        }

        // Keep selection order aligned with list_files() so downstream
        // ID-based selection and progress accounting stay coherent.
        selected.sort_by_key(|e| e.id);

        // Destination materialisation is performed inside
        // `extract_selected` after the safety/overwrite gates run, so
        // a blocked extraction never leaves an empty output dir on
        // disk (R0070-0031 / R0070-0033 / R0071-0005).
        extract_selected(
            archive,
            selected,
            entries.len(),
            &mut options,
            ops::EXTRACT_FILES,
        )
    }

    /// Extract files by their IDs.
    ///
    /// Extracts files using their sequential indices (0-based) from the
    /// archive's file list. The ID is consistent across all archive formats
    /// and represents the position returned by `list_files()`.
    ///
    /// Validates every ID, then delegates to [`Archive::extract_some`] — the
    /// archive is traversed once per AD 0029 regardless of how many IDs are
    /// selected.
    ///
    /// # Reaching an entry whose name is not valid UTF-8
    ///
    /// This is the supported route, and it is why no by-raw-bytes selector
    /// exists (AD 0064, amended 2026-09-03). Selection here is positional:
    /// an ID indexes the listing and no name string participates, so an
    /// entry is reachable even when its archived name cannot be spelled as
    /// a `&str`. Pair it with [`ArchiveEntry::raw_path`](crate::ArchiveEntry::raw_path),
    /// which carries the exact stored bytes — find by bytes, act by ID:
    ///
    /// ```no_run
    /// # use unified_archive::{Archive, ExtractionOptions};
    /// # let archive = Archive::open("names.tar")?;
    /// let entries = archive.list_files()?;
    /// let wanted = entries
    ///     .iter()
    ///     .find(|e| e.raw_path() == Some(&b"caf\xFF.txt"[..]))
    ///     .expect("the entry is listed");
    /// archive.extract_by_ids(&[wanted.id], ExtractionOptions::new("./out"))?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    ///
    /// The `&str`-keyed routes cannot always do this: two distinct raw
    /// names can lossy-decode to the same string, and those routes refuse
    /// the ambiguity rather than guess — pointing here. An ID is unique by
    /// construction, so a raw-bytes key would be no less ambiguous than the
    /// lossy one it replaced, only less often.
    ///
    /// **What this does not promise: a byte-faithful destination name.**
    /// The right entry is selected and its exact bytes are written, but the
    /// file lands under the lossy name, because the destination hand-off is
    /// still lossy (OI-0076-001 Required Actions 2 and 4). Two entries whose
    /// names collide therefore also collide on disk.
    ///
    /// # Arguments
    ///
    /// * `ids` - Array of file IDs (indices) to extract
    /// * `options` - Extraction options including destination directory
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::{Archive, ExtractionOptions};
    ///
    /// let archive = Archive::open("backup.rar")?;
    /// let entries = archive.list_files()?;
    ///
    /// // Print files with IDs
    /// for entry in entries.iter() {
    ///     println!("[{}] {}", entry.id, entry.path);
    /// }
    ///
    /// // Extract files with IDs 0 and 2
    /// archive.extract_by_ids(
    ///     &[0, 2],
    ///     ExtractionOptions::new("./output"),
    /// )?;
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn extract_by_ids(
        &self,
        ids: &[usize],
        mut options: ExtractionOptions,
    ) -> Result<ResultWithWarnings<()>> {
        // No-op short-circuit precedes any filesystem mutation
        // (R0069-0012 / R0070-0032).
        if ids.is_empty() {
            return Ok(ResultWithWarnings::ok(()));
        }

        assert_verify_crc32_supported(self, options.verify_crc32, ops::EXTRACT_BY_IDS)?;

        // Deduplicate requested IDs
        let mut seen = HashSet::new();
        let ids: Vec<usize> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();

        let listing_archive =
            reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)?;
        let archive = listing_archive.as_ref().unwrap_or(self);

        // R0073-0001: mirror the cap honouring used by extract_some /
        // extract_files / extract_to_memory_with_options.
        let entries = archive
            .list_files_for_limits_budgeted(Some(options.limits.max_entry_count().as_usize()))?;

        let mut selected = Vec::with_capacity(ids.len());
        for &id in &ids {
            let entry = entries
                .get(id)
                .ok_or_else(|| ArchiveError::OperationBlocked {
                    operation: ops::EXTRACT_BY_IDS.to_string(),
                    reason: crate::error::invalid_id_reason(id, entries.len()),
                })?;
            selected.push(entry);
        }

        // Destination materialisation is performed inside
        // `extract_selected` after the safety/overwrite gates run, so
        // a blocked extraction never leaves an empty output dir on
        // disk (R0070-0032 / R0071-0005).
        extract_selected(
            archive,
            selected,
            entries.len(),
            &mut options,
            ops::EXTRACT_BY_IDS,
        )
    }
}

/// Validated extraction source for internal callers (D3 / R0068-0039,
/// R0068-0040).
///
/// The previous `extract_to_*_unchecked` `pub(crate)` methods carried
/// their safety invariant ("the caller has already validated this
/// entry's source listing") in a doc comment. `ValidatedSource` makes
/// the invariant a type fact: the only way to obtain one is
/// [`Archive::validated_source`], which is also `pub(crate)`. Once
/// internal call sites have migrated, the legacy `_unchecked` methods
/// can be removed without breaking the safety story.
///
/// This wraps `&Archive` today; once D2 (Archive god-object split)
/// lands, the wrap becomes `&ReadArchive` so the token cannot
/// accidentally be obtained from a write/modify-mode handle.
pub(crate) struct ValidatedSource<'a> {
    archive: &'a Archive,
}

impl ValidatedSource<'_> {
    /// Decode `file_path` to an in-memory `Vec<u8>` against the
    /// already-validated archive. Skips the per-call entry-list rebuild
    /// and safety pre-check that public extraction APIs perform.
    pub(crate) fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.archive.extract_to_memory_unchecked(file_path)
    }

    /// Streaming variant of [`Self::extract_to_memory`].
    pub(crate) fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.archive.extract_to_stream_unchecked(file_path)
    }

    /// Stream a single already-listed entry by its stable listing id,
    /// applying the per-entry resource-limit checks against the resolved
    /// entry (ti-2a6e3153 / R0079-0028).
    ///
    /// The content-multiset digest walk reaches CRC-less entries by id
    /// rather than path so a duplicate-path tar digests each occurrence's
    /// real payload; `validate_single_entry`'s duplicate-path rejection is
    /// exactly what this bypasses. Because the caller already holds the
    /// resolved `entry`, we replicate [`check_single_entry_safe`]'s
    /// size/ratio arms directly against it (skipping that gate's existence
    /// and duplicate-path checks) before streaming — the entry is a
    /// regular file the caller pulled from a fresh listing, so the only
    /// residual policy is the `ExtractionLimits` budget.
    ///
    /// Backends without an id-based stream — every non-libarchive backend,
    /// none of which surface CRC-less file entries in practice — return
    /// [`ArchiveError::NotImplemented`] from the trait default; the caller
    /// falls back to the path-based stream so their behavior is unchanged.
    ///
    /// [`check_single_entry_safe`]: crate::security::check_single_entry_safe
    pub(crate) fn extract_to_stream_by_id(
        &self,
        entry: &ArchiveEntry,
        limits: &ExtractionLimits,
        op: &'static str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        // Replicate check_single_entry_safe's size/ratio arms for the
        // already-resolved entry. We must NOT call check_single_entry_safe
        // itself: its validate_single_entry step rejects duplicate paths,
        // which is the single-entry-gate behavior this id-based path exists
        // to bypass (OI-0076-002 / R0075-0011).
        if let Some(size) = entry.size {
            if limits.max_file_size().exceeded_by(size) {
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "File '{}' too large: {} bytes exceeds limit of {} bytes",
                        entry.path,
                        size,
                        limits.max_file_size().get()
                    ),
                ));
            }
            // R0081-0025: a single-entry read materializes exactly one file,
            // so its extracted total equals this entry's size; apply the same
            // min(max_file_size, max_total_size) ceiling the single-entry gate
            // does.
            if limits.max_total_size().exceeded_by(size) {
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "Total uncompressed size {} bytes exceeds limit of {} bytes",
                        size,
                        limits.max_total_size().get()
                    ),
                ));
            }
            if let Some(compressed) = entry.compressed_size {
                limits.check_ratio(size, compressed, &format!("File '{}'", entry.path), op)?;
            }
        }
        dispatch_read_archive(self.archive, op, |b| {
            b.extract_to_stream_by_listing_id(entry.id, &entry.path)
        })
    }

    /// Cap-aware extract-to-memory carrying the decoded-payload ceiling
    /// (`max_bytes`): an entry whose declared size exceeds the cap is
    /// rejected before buffering. (Before the AD 0007 collapse this also
    /// threaded a Piz-only mmap cap; no backend memory-maps any more.)
    pub(crate) fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        dispatch_read_archive(self.archive, ops::EXTRACT_TO_MEMORY, |b| {
            b.extract_to_memory_with_limit(file_path, max_bytes)
        })
    }

    /// Cap-aware extract-to-stream — see
    /// [`Self::extract_to_memory_with_limit`] for the `max_bytes` contract.
    pub(crate) fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<crate::streaming::StreamingExtractor> {
        dispatch_read_archive(self.archive, ops::EXTRACT_TO_STREAM, |b| {
            b.extract_to_stream_with_limit(file_path, max_bytes)
        })
    }
}

#[cfg(test)]
mod tests;

/// `Archive::extract_file` honours `ExtractionOptions::progress`.
///
/// The field used to be read only by `dispatch_extract_core`, which
/// `extract_file` does not go through, so a callback handed to the one
/// disk-writing single-entry API was silently discarded. These tests
/// pin the sample shape (entry granularity, declared-size denominator)
/// and both cancellation points.
#[cfg(test)]
mod extract_file_progress_tests {
    use super::{Archive, ArchiveError, ExtractionOptions};
    use crate::error::ops;
    use crate::test_utils::fixture;
    use std::ops::ControlFlow;
    use std::sync::{Arc, Mutex};

    type Samples = Arc<Mutex<Vec<(u64, Option<u64>)>>>;

    /// Open `test.zip`, pick its first regular file, and extract it into
    /// a fresh directory with a callback that records every sample and
    /// votes `Break` on the 1-based call numbers in `break_on`.
    fn extract_first_file_with_progress(
        break_on: &'static [usize],
    ) -> (
        crate::error::Result<()>,
        Samples,
        tempfile::TempDir,
        std::path::PathBuf,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let archive = Archive::open(fixture("test.zip")).unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.is_file() && e.size.unwrap_or(0) > 0)
            .expect("test.zip must carry a non-empty regular file")
            .clone();

        let samples: Samples = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&samples);
        let mut calls = 0usize;
        let options = ExtractionOptions {
            destination: temp.path().to_path_buf(),
            overwrite: true,
            progress: Some(Box::new(move |processed: u64, total: Option<u64>| {
                calls += 1;
                recorder.lock().unwrap().push((processed, total));
                if break_on.contains(&calls) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })),
            ..Default::default()
        };

        let result = archive.extract_file(&entry.path, options);
        let written = temp.path().join(&entry.path);
        (result, samples, temp, written)
    }

    #[test]
    fn extract_file_reports_declared_size_samples() {
        let (result, samples, _temp, written) = extract_first_file_with_progress(&[]);
        result.expect("extraction must succeed");
        assert!(written.is_file(), "the payload must be on disk");

        let samples = samples.lock().unwrap().clone();
        assert_eq!(
            samples.len(),
            2,
            "entry granularity: one pre-write sample and one completion sample, got {samples:?}",
        );
        let total = samples[0].1.expect("the denominator is always reported");
        assert_eq!(
            samples[0],
            (0, Some(total)),
            "the first sample reports nothing processed yet, got {samples:?}",
        );
        assert_eq!(
            samples[1],
            (total, Some(total)),
            "the last sample reports 100%, got {samples:?}",
        );
        assert_eq!(
            total,
            written.metadata().unwrap().len(),
            "the denominator is the entry's declared size, which for this \
             fixture is what actually landed on disk",
        );
    }

    #[test]
    fn break_before_the_write_cancels_without_writing() {
        let (result, samples, _temp, written) = extract_first_file_with_progress(&[1]);
        match result {
            Err(ArchiveError::Cancelled { operation }) => {
                assert_eq!(operation, ops::EXTRACT_FILE);
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        assert_eq!(
            samples.lock().unwrap().len(),
            1,
            "the cancellation must abort before the completion sample",
        );
        assert!(
            !written.exists(),
            "a pre-write cancellation must leave no payload behind",
        );
    }

    #[test]
    fn break_at_completion_still_cancels() {
        // R0070-0035 parity: the bulk paths surface a `Break` from the
        // 100% notification too, even though the bytes are already
        // written. `extract_file` reports it the same way rather than
        // swallowing the vote.
        let (result, samples, _temp, written) = extract_first_file_with_progress(&[2]);
        match result {
            Err(ArchiveError::Cancelled { operation }) => {
                assert_eq!(operation, ops::EXTRACT_FILE);
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
        assert_eq!(samples.lock().unwrap().len(), 2);
        assert!(
            written.is_file(),
            "the completion vote arrives after the write, which the rustdoc says",
        );
    }
}
