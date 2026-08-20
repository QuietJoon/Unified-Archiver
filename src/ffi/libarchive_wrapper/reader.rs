//! Read-side libarchive operations: listing, extraction, integrity, and streaming.

use super::*;

// The R0001-0053 `_nsec` timestamp getters and the R0001-0052
// `ARCHIVE_FORMAT_ZIP_BASE` constant used to be declared in this file;
// both now live in `crate::ffi::libarchive` with every other libarchive
// binding and reach this module through `use super::*`. See that
// module's declaration-site rule before adding an accessor; declaring
// one locally is a test failure, not a style nit.

/// Combine a libarchive whole-second timestamp with its `_nsec`
/// companion into a `SystemTime`.
///
/// R0001-0053: the value getters only expose whole seconds, so every
/// populated timestamp silently truncated its subsecond part. The
/// crate-wide pre-epoch policy still applies — negative seconds yield
/// `None` through the shared conversion (R0076-0043) — and a
/// nanosecond field outside `[0, 1e9)` (a corrupt header can carry
/// anything) degrades to the whole second instead of rejecting an
/// otherwise valid timestamp.
fn libarchive_time(seconds: i64, nanos: std::os::raw::c_long) -> Option<std::time::SystemTime> {
    let base = crate::ffi::common::unix_seconds_to_system_time(seconds)?;
    match u32::try_from(nanos) {
        Ok(n) if n < 1_000_000_000 => base
            .checked_add(std::time::Duration::from_nanos(u64::from(n)))
            .or(Some(base)),
        _ => Some(base),
    }
}

/// Read an entry's declared payload size, distinguishing "declared
/// zero" from "never declared".
///
/// R0001-0014: `archive_entry_size()` returns 0 both for a genuinely
/// empty entry and for one whose format never declared a size, so a
/// bare `size >= 0` test reports every unknown-size entry as exactly
/// zero bytes — listings claim `Some(0)` and the extraction paths hand
/// `copy_data` a zero-byte ceiling. `archive_entry_size_is_set` is the
/// only probe that separates the two (the distinction R0081-0052 and
/// R0081-0062 already made locally in the memory and streaming paths);
/// every size read in this module routes through here so listing, bulk
/// extraction, single extraction, memory, streaming, and integrity all
/// agree on what "unknown" means.
///
/// # Safety
/// `entry` must be a live libarchive entry pointer (or NULL).
unsafe fn entry_declared_size(entry: *mut LibarchiveEntry) -> Option<u64> {
    if entry.is_null() {
        return None;
    }
    if unsafe { archive_entry_size_is_set(entry) } == 0 {
        return None;
    }
    u64::try_from(unsafe { archive_entry_size(entry) }).ok()
}

/// RAII owner for a libarchive *disk-writer* handle
/// (`archive_write_disk_new`): frees via `archive_write_free` on drop.
/// Declare it after the [`ReadHandleGuard`] so reverse drop order frees
/// the writer before the reader, matching the manual free order the
/// error ladders used to maintain.
struct WriteDiskGuard(*mut Archive);

impl Drop for WriteDiskGuard {
    fn drop(&mut self) {
        unsafe { archive_write_free(self.0) };
    }
}

/// True when `archive_path` looks like a raw single-file compressed
/// archive (`.gz`, `.bz2`, `.xz`, `.zst`, `.lz4`, `.lzma`, `.lzop`).
/// Used to gate the `data` → file-stem remap below so it only fires on
/// the formats libarchive actually exposes as raw single-entry archives
/// (R0070-0023). Any TAR/ZIP/ISO entry literally named `"data"` is left
/// alone.
fn is_raw_compressed_archive(archive_path: &Path) -> bool {
    archive_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "gz" | "tgz"
                    | "bz2"
                    | "tbz2"
                    | "tb2"
                    | "xz"
                    | "txz"
                    | "zst"
                    | "lz4"
                    | "lzma"
                    | "lzo"
                    | "lzop"
            )
        })
        .unwrap_or(false)
        && !is_compound_tar_extension(archive_path)
}

/// True when the archive looks like `.tar.gz` / `.tar.bz2` / `.tar.xz`,
/// in which case libarchive surfaces the actual TAR entries by name and
/// the `data` pseudo-entry never appears.
///
/// Includes the short tar aliases (`.tgz`, `.tbz2`, `.tb2`, `.txz`) so
/// the `data` → file-stem remap in `raw_format_name` does not rename a
/// legitimate tar entry literally named `data` into the archive's stem
/// (R0072-0001). Delegates to the canonical suffix list in
/// [`path_extension_claims_compressed_tar`] so the two probes cannot
/// drift.
fn is_compound_tar_extension(archive_path: &Path) -> bool {
    path_extension_claims_compressed_tar(&archive_path.to_string_lossy())
}

/// Classify a libarchive error string into a typed `ArchiveError`.
///
/// libarchive does not expose a dedicated encryption errno — failures
/// for header-encrypted ZIP/7z and password-required entries surface as
/// `ARCHIVE_FAILED` with an English message describing the cause. To
/// keep the substring-matching contained at the FFI boundary
/// (R0069-0057), all libarchive error strings flow through this helper
/// before they leave the wrapper. Encryption-shaped messages become
/// `ArchiveError::Password { .. }` so callers (notably
/// `Archive::modify`'s encrypted-archive probe) can pattern-match on a
/// typed variant instead of inspecting message text themselves.
fn classify_libarchive_error(message: impl Into<String>) -> ArchiveError {
    let message = message.into();
    if is_libarchive_encryption_message(&message) {
        return ArchiveError::password(message);
    }
    ArchiveError::format(None, message)
}

/// Advance past the current entry's data, classifying a failed skip as
/// a structured error instead of silently leaving the libarchive read
/// cursor in an undefined position (R0076-0029..0034 / AD 0059).
///
/// A failed `archive_read_data_skip` is dangerous precisely because it
/// is silent: the next `archive_read_next_header` can report a
/// misleading error or — worse — succeed on a torn payload stream, so
/// `list_files` / extract / integrity walks would report success on a
/// structurally broken archive. `ARCHIVE_OK` and `ARCHIVE_WARN` are
/// treated as success (matching the long-standing checked skip in
/// `list_files`); anything else is pulled through
/// [`classify_libarchive_error`].
///
/// The helper deliberately does **not** free any handles — the caller
/// holds the only references to its `ext` disk writer / `archive` read
/// handle and must free them on the returned `Err` before returning,
/// matching the surrounding cleanup style.
///
/// # Safety
/// `archive` must be a live libarchive read handle positioned on an
/// entry header (i.e. immediately after a successful
/// `archive_read_next_header`).
unsafe fn checked_data_skip(archive: *mut Archive) -> Result<()> {
    let skip_result = unsafe { archive_read_data_skip(archive) };
    if skip_result != ARCHIVE_OK && skip_result != ARCHIVE_WARN {
        let error_msg = unsafe { get_archive_error(archive) };
        return Err(classify_libarchive_error(error_msg));
    }
    Ok(())
}

/// Detect libarchive's actual encryption error vocabulary instead of
/// matching any mention of an encryption-shaped noun.
///
/// R0075-0027: the previous classifier accepted any message
/// containing `encrypt` / `password` / `passphrase` and reclassified
/// it as `ArchiveError::Password`. That misfires on corrupt archives
/// whose error text incidentally mentions one of those words —
/// e.g. a TAR with a member named `password.txt` could surface
/// `Failed to read 'password.txt'` and get reclassified as a
/// password error.
///
/// This implementation pattern-matches against the canonical phrases
/// libarchive itself emits across the formats we care about. The set
/// is conservative — additions belong here when a real libarchive
/// version is observed emitting a phrase we don't yet cover. A
/// missed encryption error degrades to `Format`, which keeps the
/// previous "we don't recognize this" behavior; a false-positive
/// classification masks a real corruption error and is the higher-
/// cost mistake.
fn is_libarchive_encryption_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    // Phrases that always indicate encryption / passphrase context.
    const ENCRYPTION_PHRASES: &[&str] = &[
        // libarchive-zip / zip2 / pkzip
        "encrypted file is unsupported",
        "encrypted file is required",
        "encryption is unsupported",
        "encryption error",
        "winzip aes encryption",
        "win-zip-style aes",
        "zip aes encryption",
        "decryption error",
        // Generic decoder paths
        "passphrase required",
        "passphrase is required",
        "passphrase incorrect",
        "incorrect passphrase",
        "bad passphrase",
        "wrong passphrase",
        // RAR / 7z paths surfaced through libarchive
        "password required",
        "password is required",
        "wrong password",
        "bad password",
        "incorrect password",
        "password mismatch",
    ];
    ENCRYPTION_PHRASES.iter().any(|p| lower.contains(p))
}

/// True if the path's filename extension claims a compressed-tar
/// container (AD 0062 A.6). Used by `LibarchiveArchive::open` to
/// gate the post-open tar-payload confirmation pass — only files
/// whose name leads us to *expect* a tar stream get the extra
/// header read; bare `.gz` / `.bz2` / `.xz` paths skip the check.
fn path_extension_claims_compressed_tar(archive_path: &str) -> bool {
    let lower = archive_path.to_ascii_lowercase();
    lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tar.lz4")
        || lower.ends_with(".tar.lzma")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tb2")
        || lower.ends_with(".txz")
}

/// Hint string for the diagnostic raised by AD 0062 A.6 when a
/// compressed-tar extension wraps a non-tar payload. Names the
/// bare compressed format the caller should open instead.
fn bare_compressed_hint(archive_path: &str) -> &'static str {
    let lower = archive_path.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        "Gzip"
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") || lower.ends_with(".tb2") {
        "Bzip2"
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        "Xz"
    } else {
        "the underlying compressed format"
    }
}

/// Build a unique sibling tempfile path for a destination path.
///
/// R0075-0019: libarchive disk extraction stages every regular file
/// to a sibling tempfile in the destination's parent directory and
/// renames into place after `archive_write_finish_entry` succeeds.
/// This helper produces a non-clashing tempfile path using
/// `tempfile::Builder` so we don't have to roll our own collision-
/// resistant suffix scheme.
///
/// Returns the staging tempfile's path (the tempfile itself is
/// closed before we return so libarchive can `open(O_CREAT|...)` the
/// path itself). The brief gap between the unlink and libarchive's
/// open is narrow — the path is in a fresh tempfile name space, so
/// winning it requires an external process with write access to the
/// destination directory guessing that exact filename — but it is a
/// real residual race, tracked as R0076-0045 (OI-0076-003 item 5).
/// Closing it needs an owned fd or a libarchive callback hand-off,
/// which is why it is deferred rather than fixed here.
fn build_staging_path(final_path: &Path) -> Result<std::path::PathBuf> {
    let parent = final_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("entry");
    let prefix = format!("{stem}.ua-stage.");
    let temp = tempfile::Builder::new()
        .prefix(&prefix)
        .tempfile_in(parent)
        .map_err(|e| ArchiveError::io("create_tempfile", parent.to_path_buf(), e))?;
    let temp_path = temp.into_temp_path();
    let path = temp_path.to_path_buf();
    // Detach from the `TempPath`'s drop-unlink — we manage the
    // placeholder explicitly below. `keep()` failing means the file
    // was already removed, which is the end state we want anyway, so
    // it is benign and intentionally ignored.
    let _ = temp_path.keep();
    // Remove the empty placeholder so libarchive's O_CREAT|O_EXCL-style
    // open recreates the file at this exact path. A `NotFound` here is
    // fine (the placeholder is already gone); any other failure means
    // the placeholder lingers and libarchive's create-by-name would
    // surface a confusing error later, so surface it now (R0076-0044).
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(ArchiveError::io(
                "remove_staging_placeholder",
                path.clone(),
                e,
            ));
        }
    }
    Ok(path)
}

/// Install a staged extraction file at its final destination.
///
/// `overwrite = true` replaces atomically via `rename_with_overwrite`.
/// `overwrite = false` (R0079-0016) installs through the atomic
/// noclobber primitive so a destination that came into existence after
/// the racy pre-check — e.g. a file created by a concurrent process
/// during a long entry decode — is rejected with the same structured
/// already-exists error the pre-check (and the ZIP/7z/UnRAR
/// `persist_noclobber` paths) emit, instead of being silently replaced.
///
/// On `Err` the staging file is left in place; the caller's cleanup
/// path removes it.
fn install_staged_file(
    stage: &Path,
    final_path: &Path,
    overwrite: bool,
    op: &'static str,
    entry_archive_path: &str,
) -> Result<()> {
    if overwrite {
        return crate::ffi::common::rename_with_overwrite(stage, final_path);
    }
    crate::ffi::common::rename_noclobber(stage, final_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            ArchiveError::operation_blocked(
                op,
                format!(
                    "Destination file already exists: '{}' (archive entry: '{}')",
                    final_path.display(),
                    entry_archive_path
                ),
            )
        } else {
            let combined = std::io::Error::new(
                e.kind(),
                format!(
                    "rename {} -> {}: {}",
                    stage.display(),
                    final_path.display(),
                    e
                ),
            );
            ArchiveError::io("rename", stage, combined)
        }
    })
}

/// Resolve the raw-format pseudo-name `"data"` to the archive's file
/// stem so standalone `.gz/.bz2/.xz` entries surface a meaningful path.
///
/// Restricted to raw single-file compressed archives via
/// [`is_raw_compressed_archive`] so a TAR/ZIP/ISO entry literally named
/// `data` is not rewritten (R0070-0023).
fn raw_format_name<'a>(name: &'a str, archive_path: &Path) -> std::borrow::Cow<'a, str> {
    if name == "data" && is_raw_compressed_archive(archive_path) {
        archive_path
            .file_stem()
            .map(|s| std::borrow::Cow::Owned(s.to_string_lossy().into_owned()))
            .unwrap_or(std::borrow::Cow::Borrowed(name))
    } else {
        std::borrow::Cow::Borrowed(name)
    }
}

/// EOF hit before the walk reached the gate-validated listing index
/// (OI-0076-002): the AD 0065 cached listing went stale — the archive
/// was rewritten on disk after listing.
fn listing_drift_eof(target_id: usize) -> ArchiveError {
    ArchiveError::format(
        None,
        format!(
            "listing drift: entry index {} not reached before end of archive",
            target_id
        ),
    )
}

/// Walked entry name does not match the gate-validated listing name at
/// the target index (OI-0076-002 drift guard) — never touch a
/// mismatched entry's payload.
fn listing_drift_mismatch(index: usize, expected: &str, found: &str) -> ArchiveError {
    ArchiveError::format(
        None,
        format!(
            "single-entry listing drift at index {}: expected '{}', found '{}'",
            index, expected, found
        ),
    )
}

/// The fresh bulk walk produced more live entries than the
/// gate-validated listing holds (R0080-0016 cardinality guard) — the
/// archive grew on disk after listing. Shares the "listing drift"
/// vocabulary of [`listing_drift_eof`] / [`listing_drift_mismatch`].
fn listing_drift_extra(index: usize, listing_len: usize) -> ArchiveError {
    ArchiveError::format(
        None,
        format!(
            "listing drift: archive entry index {} exceeds the validated listing length {}",
            index, listing_len
        ),
    )
}

/// Read an entry's pathname as an owned `String`, returning empty on NULL.
///
/// SAFETY: caller must hold a valid entry pointer whose lifetime spans the
/// call. The returned `String` copies the bytes, so the underlying C buffer
/// may be invalidated by the next libarchive call.
unsafe fn entry_pathname_string(entry: *mut LibarchiveEntry) -> String {
    let ptr = unsafe { archive_entry_pathname(entry) };
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

fn extract_flags(
    overwrite: bool,
    preserve_permissions: bool,
    preserve_times: bool,
) -> std::os::raw::c_int {
    let mut flags = ARCHIVE_EXTRACT_SECURE_SYMLINKS | ARCHIVE_EXTRACT_SECURE_NODOTDOT;
    if preserve_times {
        flags |= ARCHIVE_EXTRACT_TIME;
    }
    if preserve_permissions {
        flags |= ARCHIVE_EXTRACT_PERM;
    }
    if !overwrite {
        flags |= ARCHIVE_EXTRACT_NO_OVERWRITE;
    }
    flags
}

/// Copy data from read archive to write archive.
///
/// `entry_path` is interpolated into checksum error messages so the
/// caller can attribute corruption to the specific file rather than a
/// generic `"archive"` label (R0070-0045).
///
/// `max_bytes` is the per-entry decoded-payload ceiling (R0073-0004).
/// `None` means "no enforcement" (used by callers that have not been
/// migrated yet); `Some(n)` aborts the copy with `OperationBlocked`
/// before `archive_write_data_block` would push the running total past
/// `n`. The disk file written through `aw` is left in whatever state
/// libarchive's last successful block produced — the caller's
/// `archive_write_disk_*` handle is freed on the same error path. The
/// shared safety gate (`check_single_entry_safe_with_archive`) lets
/// entries without size metadata through with the contract that the
/// backend stream enforces limits as bytes arrive; this argument is
/// the libarchive disk path's half of that contract for both
/// declared-size (cap = declared) and unknown-size (cap =
/// `ExtractionLimits::max_file_size`) entries.
///
/// `op` labels the caller's operation in the per-entry-limit error so
/// `extract_all` / `extract_file` failures carry their own op instead
/// of a hard-coded generic label (R0076-0047).
///
/// `expected_size` (R0080-0014): the entry's authoritative declared
/// size as reported by [`entry_declared_size`], else `None`. When
/// `Some(n)`, a clean EOF whose sparse-aware logical extent is not `n`
/// is reported as corruption, so a truncated payload cannot install
/// silently. `None` skips the equality check (unknown-size streams).
///
/// `cancel` (R0080-0015): an optional callback invoked with the running
/// decoded-byte total after each committed block, rate-limited to the
/// extract loop's ~60 Hz cadence. A `ControlFlow::Break(())` return
/// aborts the copy with the typed `ArchiveError::Cancelled`, so
/// cancellation latency is bounded by block size, not entry size.
unsafe fn copy_data(
    ar: *mut Archive,
    aw: *mut Archive,
    entry_path: &str,
    max_bytes: Option<u64>,
    op: &'static str,
    expected_size: Option<u64>,
    mut cancel: Option<&mut dyn FnMut(u64) -> std::ops::ControlFlow<()>>,
) -> Result<u64> {
    let mut buff: *const c_void = std::ptr::null();
    let mut size: usize = 0;
    let mut offset: c_longlong = 0;
    let mut total: u64 = 0;
    // R0080-0014: highest logical extent (offset + size) any block
    // reached. For a sequential entry this equals `total`; for a sparse
    // entry libarchive reports data at its logical offset (and a
    // trailing hole as a zero-length block at the end offset), so this
    // still lands on the declared size when the entry is well formed.
    let mut logical_end: u64 = 0;
    // R0080-0015: throttle the cancellation poll to the same ~60 Hz
    // cadence as the between-header check so a Break aborts inside a
    // large entry without calling back on every decoded block.
    let mut rate_limiter = RateLimiter::new();

    loop {
        let r = unsafe { archive_read_data_block(ar, &mut buff, &mut size, &mut offset) };

        if r == ARCHIVE_EOF {
            break;
        }

        if r != ARCHIVE_OK {
            let error_msg = unsafe { get_archive_error(ar) };
            if is_libarchive_checksum_failure(&error_msg) {
                return Err(ArchiveError::corruption(
                    entry_path.to_string(),
                    format!("Checksum verification failed: {}", error_msg),
                ));
            }
            return Err(ArchiveError::format(None, error_msg));
        }

        // R0081-0059: a negative block offset is corrupt metadata.
        // Reject it before both the accounting below and the disk
        // writer — the old code coerced it to 0 for accounting yet still
        // handed the raw negative to `archive_write_data_block`.
        let block_offset = match u64::try_from(offset) {
            Ok(o) => o,
            Err(_) => {
                return Err(ArchiveError::corruption(
                    entry_path.to_string(),
                    format!(
                        "backend reported a negative block offset ({}) for '{}'",
                        offset, entry_path
                    ),
                ));
            }
        };

        // R0001-0015: a positive-sized block with a null buffer violates
        // libarchive's contract and would cross the unsafe boundary into
        // `archive_write_data_block` unchecked. The memory path already
        // refuses exactly this shape (R0081-0056); mirror it here.
        if size > 0 && buff.is_null() {
            return Err(ArchiveError::corruption(
                entry_path.to_string(),
                "backend reported a non-empty block with a null buffer".to_string(),
            ));
        }

        // R0001-0016: the supported sparse ordering is monotonic and
        // non-overlapping — every block starts at or after the extent
        // already committed. A block whose offset precedes `logical_end`
        // would silently overwrite bytes this copy already wrote while
        // still reporting success; the memory path rejects the same
        // shape (R0081-0055).
        if block_offset < logical_end {
            return Err(ArchiveError::corruption(
                entry_path.to_string(),
                format!(
                    "backend reported a backward/overlapping block (offset {} precedes {} bytes already written)",
                    block_offset, logical_end
                ),
            ));
        }

        // R0081-0058: the file's logical size is its highest extent
        // (offset + size), not the count of physical bytes decoded. A
        // sparse block writes `size` bytes at `block_offset`, so a tiny
        // block at a huge offset materialises a huge file on disk.
        // Enforce the cap against that logical extent — before writing —
        // so a sparse hole cannot slip past max_file_size /
        // max_total_size the way a physical-only count let it.
        //
        // R0001-0054: an extent that overflows `u64` is corrupt metadata,
        // not a `u64::MAX` extent — `saturating_add` hid the overflow and
        // the raw offset/size still reached the writer.
        let block_end = match block_offset.checked_add(size as u64) {
            Some(end) => end,
            None => {
                return Err(ArchiveError::corruption(
                    entry_path.to_string(),
                    format!(
                        "backend reported a block extent that overflows (offset {} + size {})",
                        block_offset, size
                    ),
                ));
            }
        };
        if let Some(cap) = max_bytes {
            if block_end > cap {
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "Decoded payload for '{}' exceeds the configured per-entry limit of {} bytes",
                        entry_path, cap
                    ),
                ));
            }
        }

        let r = unsafe { archive_write_data_block(aw, buff, size, offset) };

        // R0079-0003: the write-data family returns ARCHIVE_OK (0)
        // through libarchive 3.x and the written byte count from 4.0,
        // so any non-negative value is success — same check as the
        // `archive_write_data` call site in `write_entry_inner`.
        if r < 0 {
            let error_msg = unsafe { get_archive_error(aw) };
            return Err(ArchiveError::format(None, error_msg));
        }

        // R0081-0058: charge the running total by the logical extent
        // (`max(total, logical_end)`) so `max_total_size`, which the
        // caller accumulates from this return value, reflects the bytes
        // the extracted file actually occupies rather than just the
        // physical block bytes.
        logical_end = logical_end.max(block_end);
        total = total.max(logical_end);

        // R0080-0015: poll the (rate-limited) cancellation hook after
        // each committed block so a `Break` vote aborts the copy with
        // the typed Cancelled error instead of running to the entry end.
        if let Some(cb) = cancel.as_mut() {
            if rate_limiter.should_call() {
                if let std::ops::ControlFlow::Break(()) = cb(total) {
                    return Err(ArchiveError::Cancelled { operation: op });
                }
            }
        }
    }

    // R0080-0014: a decoder that reached a clean EOF before its
    // authoritative declared size produced a truncated file. Compare the
    // logical extent (sparse-aware) with the declared size and surface a
    // corruption error naming the entry, declared, and decoded counts.
    if let Some(expected) = expected_size {
        if logical_end != expected {
            return Err(ArchiveError::corruption(
                entry_path.to_string(),
                format!(
                    "Declared size {} bytes but decoded {} bytes",
                    expected, logical_end
                ),
            ));
        }
    }

    Ok(total)
}

/// True when the read handle's selected format maps its `ctime` field
/// to a *creation* timestamp instead of POSIX's inode metadata-change
/// time (R0001-0052). Only ZIP qualifies: libarchive lands the PKWARE
/// 0x5455 "creation" value in `archive_entry_ctime`, which is what
/// OI-0065-002's metadata round-trip reads back.
///
/// # Safety
/// `archive` must be a live libarchive read handle that has already bid
/// a format (i.e. positioned after a successful
/// `archive_read_next_header`).
unsafe fn format_ctime_is_creation(archive: *mut Archive) -> bool {
    let format = unsafe { archive_format(archive) };
    format & ARCHIVE_FORMAT_BASE_MASK == ARCHIVE_FORMAT_ZIP_BASE
}

/// Parse libarchive entry into ArchiveEntry
///
/// `ctime_is_creation` (R0001-0052) must be true only when the handle
/// the entry came from selected a format whose `ctime` field carries a
/// *creation* timestamp — in practice ZIP, whose PKWARE 0x5455 extra is
/// where libarchive lands it. Callers derive it from
/// [`format_ctime_is_creation`] against their own read handle; the
/// caller passes it in rather than the parser reaching for a global
/// because `parse_entry` is a free function shared by every walk.
unsafe fn parse_entry(
    entry: *mut LibarchiveEntry,
    ctime_is_creation: bool,
) -> Option<ArchiveEntry> {
    if entry.is_null() {
        return None;
    }

    let pathname_ptr = unsafe { archive_entry_pathname(entry) };
    if pathname_ptr.is_null() {
        return None;
    }

    // R0070-0025: when the entry name bytes round-trip cleanly through
    // UTF-8 we store only the `path` field (byte-for-byte equivalent).
    // When they don't (legacy non-UTF-8 archives), we keep the raw
    // bytes in `raw_path` and let `path` carry the lossy display form
    // so existing display/listing call sites still work.
    let pathname_bytes = unsafe { CStr::from_ptr(pathname_ptr) }.to_bytes();
    let (path, raw_path) = match std::str::from_utf8(pathname_bytes) {
        Ok(s) => (s.to_string(), None),
        Err(_) => (
            String::from_utf8_lossy(pathname_bytes).into_owned(),
            Some(pathname_bytes.to_vec()),
        ),
    };

    // R0001-0014: `_is_set`-aware read — an entry whose format never
    // declared a size stays `None` instead of masquerading as `Some(0)`.
    let size = unsafe { entry_declared_size(entry) };
    let mtime = unsafe { archive_entry_mtime(entry) };
    let mode = unsafe { archive_entry_mode(entry) };
    let filetype = unsafe { archive_entry_filetype(entry) };

    // Check for hard links first (archive_entry_hardlink returns non-NULL for hard links)
    let hardlink_ptr = unsafe { archive_entry_hardlink(entry) };
    let entry_type = if !hardlink_ptr.is_null() {
        EntryType::HardLink
    } else {
        match filetype & AE_IFMT {
            AE_IFREG => EntryType::File,
            AE_IFDIR => EntryType::Directory,
            AE_IFLNK => EntryType::Symlink,
            _ => EntryType::Other,
        }
    };

    // R0070-0059 / R0070-0060: surface link targets through
    // `ArchiveEntry.link_target` so callers (and `check_symlinks`) can
    // present the same data extraction warnings already carry.
    let link_target = match entry_type {
        EntryType::Symlink => {
            let symlink_ptr = unsafe { archive_entry_symlink(entry) };
            if symlink_ptr.is_null() {
                None
            } else {
                Some(
                    unsafe { CStr::from_ptr(symlink_ptr) }
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
        EntryType::HardLink => {
            if hardlink_ptr.is_null() {
                None
            } else {
                Some(
                    unsafe { CStr::from_ptr(hardlink_ptr) }
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
        _ => None,
    };

    // R0076-0043: use the `_is_set` probe rather than `mtime > 0` so a
    // legitimate UNIX_EPOCH timestamp survives. The shared conversion
    // returns `None` for (never-observed) negative values, matching the
    // crate-wide pre-epoch policy.
    // R0001-0053: fold in the `_nsec` companion so subsecond precision
    // survives the read side too.
    let modified = if unsafe { archive_entry_mtime_is_set(entry) } != 0 {
        libarchive_time(mtime, unsafe { archive_entry_mtime_nsec(entry) })
    } else {
        None
    };

    // R0075-0024: `archive_entry_mode` packs file-type bits in the
    // upper octet (S_IFMT) alongside permission bits. `ArchiveEntry`
    // documents `permissions` as Unix permission bits only (file type
    // is already represented by `entry_type`), so mask with 0o7777.
    let permissions = if mode > 0 {
        Some((mode as u32) & 0o7777)
    } else {
        None
    };

    // Phase 1: Enhanced metadata

    // Creation time. ZIP's PKWARE 0x5455 extra carries a "creation"
    // timestamp that libarchive surfaces via `archive_entry_ctime`
    // (where the bytes literally land), while POSIX-style
    // `archive_entry_birthtime` is populated from filesystems that
    // expose a true birth time (HFS/NTFS/ext4 inodes large enough).
    // Prefer birthtime when present, fall back to ctime — the fallback
    // is what lets OI-0065-002's round-trip read back what 0x5455 wrote.
    // R0076-0043: same `_is_set` treatment for created/accessed.
    // R0001-0052: the ctime fallback is gated on `ctime_is_creation`.
    // POSIX ctime is the inode *metadata-change* time, not a creation
    // time, so taking it unconditionally mislabelled every tar/cpio/ISO
    // entry's metadata-change stamp as `created`. Only the ZIP mapping
    // above genuinely carries a creation timestamp in that field;
    // everything else leaves `created` unset.
    let birthtime_set = unsafe { archive_entry_birthtime_is_set(entry) } != 0;
    let ctime_set = unsafe { archive_entry_ctime_is_set(entry) } != 0;
    let created = if birthtime_set {
        libarchive_time(unsafe { archive_entry_birthtime(entry) }, unsafe {
            archive_entry_birthtime_nsec(entry)
        })
    } else if ctime_set && ctime_is_creation {
        libarchive_time(unsafe { archive_entry_ctime(entry) }, unsafe {
            archive_entry_ctime_nsec(entry)
        })
    } else {
        None
    };

    // Access time (atime)
    let atime = unsafe { archive_entry_atime(entry) };
    let accessed = if unsafe { archive_entry_atime_is_set(entry) } != 0 {
        libarchive_time(atime, unsafe { archive_entry_atime_nsec(entry) })
    } else {
        None
    };

    // Encryption status
    let is_encrypted = unsafe { archive_entry_is_encrypted(entry) } != 0;

    // R0073-0010: CRC32 is not directly exposed through libarchive's generic
    // metadata API. Public listing stays metadata-only per MADR-0001; integrity
    // and manifest-digest paths walk content on demand
    // (`Archive::validate_integrity`, `calculate_manifest_digest`).

    let mut arch_entry = ArchiveEntry::new(path, 0);
    arch_entry.entry_type = entry_type;
    arch_entry.size = size;
    arch_entry.compressed_size = None; // libarchive doesn't expose this easily
    arch_entry.modified = modified;
    arch_entry.permissions = permissions;
    arch_entry.crc32 = None;

    // Phase 1: Enhanced metadata
    arch_entry.created = created;
    arch_entry.accessed = accessed;
    arch_entry.is_encrypted = is_encrypted;
    arch_entry.comment = None; // Not available in libarchive generic API
    arch_entry.attributes = None; // No platform-specific attributes available
    arch_entry.link_target = link_target;
    arch_entry.raw_path = raw_path;

    Some(arch_entry)
}

impl LibarchiveArchive {
    /// Open a libarchive read handle with all formats/filters enabled
    ///
    /// Encapsulates: archive_read_new, null check, support_format_all,
    /// support_filter_all, open_filename, error check with archive_read_free on failure.
    pub(super) unsafe fn open_read_handle(c_path: &std::ffi::CStr) -> Result<*mut Archive> {
        let archive = unsafe { archive_read_new() };
        if archive.is_null() {
            return Err(ArchiveError::format(
                None,
                "Failed to create libarchive instance",
            ));
        }

        // Format/filter setup — check every return code so a setup
        // failure surfaces immediately instead of as an opaque parse
        // error from the first read (R0069-0034).
        let setup_calls: &[(unsafe extern "C" fn(*mut Archive) -> c_int, &str)] = &[
            (
                archive_read_support_format_all,
                "archive_read_support_format_all",
            ),
            // "raw" pseudo-format as lowest-priority fallback: standalone
            // compressed files (.gz, .bz2, .xz) that aren't tar archives
            // are exposed as a single-entry archive.
            (
                archive_read_support_format_raw,
                "archive_read_support_format_raw",
            ),
            (
                archive_read_support_filter_all,
                "archive_read_support_filter_all",
            ),
        ];
        for (func, name) in setup_calls {
            let rc = unsafe { func(archive) };
            if rc != ARCHIVE_OK {
                let detail = unsafe { get_archive_error(archive) };
                unsafe { archive_read_free(archive) };
                return Err(classify_libarchive_error(format!("{}: {}", name, detail)));
            }
        }

        let result = unsafe { archive_read_open_filename(archive, c_path.as_ptr(), 10240) };
        if result != ARCHIVE_OK {
            let error_msg = unsafe { get_archive_error(archive) };
            unsafe { archive_read_free(archive) };
            return Err(classify_libarchive_error(error_msg));
        }

        Ok(archive)
    }

    /// Open an archive with libarchive.
    ///
    /// **Validation timing (AD 0052):** unlike the other backends,
    /// libarchive eagerly validates at open-time — `open_read_handle`
    /// opens the source and this constructor then probes the first
    /// entry header, which is what actually drives libarchive's format
    /// bidding (R0001-0012: `archive_read_open_filename` alone bids no
    /// format, so the documented eager validation used to happen only
    /// for compressed-tar filenames). The handle is freed afterwards. A
    /// corrupt or unsupported input therefore fails here rather than at
    /// first use. Each subsequent operation reopens the file because
    /// libarchive's read handle is iterator-shaped and cannot be
    /// rewound.
    ///
    /// Future work (Group D D4 / R0068-0035) will memoise the validated
    /// handle so the parse cost is paid once instead of twice; this
    /// constructor's public contract stays "validates eagerly" through
    /// that change.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let path_display = path_buf.display().to_string();

        // Verify file exists by attempting to open
        let c_path = path_to_cstring_checked(&path_buf)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // The validation handle closes on every exit — each
            // subsequent operation reopens the archive itself.
            let _archive_guard = ReadHandleGuard(archive);

            // R0001-0012: probe the first header for *every* format, not
            // just the compressed-tar names checked below. Opening the
            // filename only wires up the read callbacks — libarchive bids
            // a format and parses structure on the first
            // `archive_read_next_header`, so without this probe the AD
            // 0052 eager-validation contract documented above held for
            // compressed-tar filenames alone.
            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let next = archive_read_next_header(archive, &mut entry_ptr);
            if next != ARCHIVE_OK && next != ARCHIVE_WARN && next != ARCHIVE_EOF {
                // R0075-0021: a non-EOF, non-OK/WARN return from the
                // eager probe is a real read error — fail the open here
                // so the eager-validate contract documented above isn't
                // violated. The previous code silently fell through to
                // the "everything is fine" path and let downstream
                // operations stumble over the broken stream.
                return Err(classify_libarchive_error(get_archive_error(archive)));
            }
            // ARCHIVE_EOF: an archive with zero entries is a legal
            // shape, so it stays a success (subject to the A.6 format
            // assertion below).

            // AD 0062 A.6: when the filename extension claims a
            // compressed-tar (`.tar.gz` / `.tar.bz2` / `.tar.xz` and
            // their `.tgz` / `.tbz2` / `.tb2` / `.txz` aliases) but
            // the post-decompression payload is *not* a tar stream
            // (libarchive surfaces ARCHIVE_FORMAT_RAW for plain
            // gzip / bzip2 / xz inputs that lack the tar wrapper),
            // surface a precise `ArchiveError::Format` instead of
            // the opaque libarchive parse error a downstream
            // `list_files` would later raise. Layered on the shared
            // probe above rather than repeating the header read.
            if path_extension_claims_compressed_tar(&path_display) {
                let actual = archive_format(archive) & ARCHIVE_FORMAT_BASE_MASK;
                // R0001-0013: the EOF path used to fall through as
                // "legal empty tar" without asserting the format, so an
                // empty raw compressed stream passed the A.6 gate. Check
                // it there too — but tolerate a zero format code, which
                // is what libarchive reports when no format was bid at
                // all and is indistinguishable from a genuinely empty
                // container. A real empty `.tar.gz` (a gzip of 1024 NUL
                // bytes) does bid TAR, so it still opens.
                let format_mismatch = if next == ARCHIVE_EOF {
                    actual != 0 && actual != ARCHIVE_FORMAT_TAR_BASE
                } else {
                    actual != ARCHIVE_FORMAT_TAR_BASE
                };
                if format_mismatch {
                    return Err(ArchiveError::format(
                        None,
                        format!(
                            "File '{}' has a compressed-tar extension but its post-decompression \
                             payload is not a tar stream (libarchive reported format code 0x{:x}). \
                             Open it as the bare compressed format ({}) instead, or rename the file \
                             if the tar wrapper is genuinely missing.",
                            path_display,
                            actual,
                            bare_compressed_hint(&path_display),
                        ),
                    ));
                }
            }
        }

        Ok(Self {
            path: path_buf,
            write_handle: None,
            write_output: None,
            progress: None,
            bytes_written: 0,
            entries_written: 0,
            stream_buffer: Vec::new(),
            write_poisoned: false,
            finish_failure: None,
            cached_listing: OnceCell::new(),
        })
    }

    /// List all files in the archive (metadata only).
    ///
    /// Returns entry metadata without decompressing any payload. CRC32
    /// verification for libarchive-backed formats lives on
    /// [`Self::test_integrity`] and the `calculate_manifest_digest` pipeline
    /// in `src/inspection.rs`, which materialize CRCs only when callers
    /// explicitly ask. Per MADR-0001, listing never walks payload data.
    ///
    /// **Caching (AD 0065).** The first call reopens the archive, walks
    /// the entry headers, and stores the resulting `Vec<ArchiveEntry>`
    /// in `cached_listing`. Subsequent calls clone from the cached
    /// snapshot — every read backend agrees that the listing is frozen
    /// at first observation. Callers who need fresh-from-disk metadata
    /// drop and reopen the archive.
    pub fn list_files_metadata_only(&self) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.list_files_metadata_only_budgeted(None)
    }

    /// Budgeted listing (OI-0080-003). `budget = Some(n)` aborts the header
    /// walk once more than `n` entries are seen. libarchive walks headers
    /// one-by-one via `archive_read_next_header`, so this is a true streaming
    /// early abort — it stops reading further headers rather than only
    /// bounding our `Vec`. The budget applies only to the first
    /// materialization; a cache hit ignores it, and an aborted parse does not
    /// populate `cached_listing`.
    pub fn list_files_metadata_only_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.cached_listing
            .get_or_try_init(|| {
                self.list_files_metadata_only_uncached(budget)
                    .map(std::sync::Arc::new)
            })
            .map(std::sync::Arc::clone)
    }

    fn list_files_metadata_only_uncached(
        &self,
        budget: Option<usize>,
    ) -> Result<Vec<ArchiveEntry>> {
        let c_path = path_to_cstring_checked(&self.path)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guard (R0070-0017).
            let _archive_guard = ReadHandleGuard(archive);

            let mut entries = Vec::new();
            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut index = 0;

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK {
                    // R0069-0057: route through the FFI-side classifier so
                    // header-encrypted libarchive errors surface as
                    // `ArchiveError::Password { .. }` instead of a generic
                    // `Format` carrying an English-encrypted-file string.
                    return Err(classify_libarchive_error(get_archive_error(archive)));
                }

                // R0001-0052: the format is only bid once a header has
                // been read, so ask the handle here rather than before
                // the walk.
                let ctime_is_creation = format_ctime_is_creation(archive);
                if let Some(mut entry) = parse_entry(entry_ptr, ctime_is_creation) {
                    // OI-0080-003: true streaming early abort — once we already
                    // hold `budget` entries and another header has parsed, stop
                    // before reading/skipping the rest so libarchive never
                    // processes past the budget-th record.
                    if let Some(budget) = budget {
                        if entries.len() >= budget {
                            return Err(crate::security::too_many_entries_parsed(budget));
                        }
                    }

                    // Raw format (standalone .gz/.bz2/.xz) returns "data" as the
                    // entry name. Replace with the archive filename sans compression
                    // extension for a meaningful path.
                    let resolved = raw_format_name(&entry.path, &self.path);
                    if let std::borrow::Cow::Owned(s) = resolved {
                        entry.path = s;
                    }

                    // Metadata-only — skip payload unconditionally.
                    // R0075-0020: surface skip failures the same way
                    // as header-read failures. A torn payload stream
                    // discovered while skipping would otherwise let
                    // `list_files` succeed on a structurally broken
                    // archive.
                    checked_data_skip(archive)?;

                    entry.id = index;
                    entries.push(entry);
                    index += 1;
                } else {
                    checked_data_skip(archive)?;
                }
            }

            Ok(entries)
        }
    }

    /// Extract all files to destination
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, true, true, None, None, None)
    }

    /// Extract all files with options. When `selection` is `Some`, only entries
    /// whose positional index (matching `list_files()` order) is in the set are
    /// materialized; non-selected entries advance via `archive_read_data_skip`
    /// so the archive is traversed once per AD 0029.
    ///
    /// `max_file_size = Some(n)` (R0073-0004) is the absolute per-entry
    /// ceiling enforced inside the libarchive copy loop. The shared
    /// safety gate lets unknown-size entries through with the contract
    /// that the backend stream enforces limits as bytes arrive; this
    /// parameter is how the libarchive disk path honours that contract
    /// when [`entry_declared_size`] reports an undeclared size.
    ///
    /// Symlinks, hard links, and (R0001-0001) special filesystem nodes
    /// — FIFOs, sockets, character and block devices — are skipped
    /// rather than materialized; only regular files and directories are
    /// written to the destination. Every skip is reported in the
    /// returned warnings: links through
    /// [`ArchiveWarning::SkippedSymlink`] /
    /// [`ArchiveWarning::SkippedHardLink`], special nodes through
    /// [`ArchiveWarning::SkippedUnsupportedEntry`] carrying the precise
    /// [`crate::error::UnsupportedEntryKind`] (DCR-010).
    ///
    /// `max_total_size = Some(n)` (R0075-0010) caps the cumulative
    /// decoded byte count across every entry written to disk. The
    /// preflight gate sums declared sizes only, so unknown-size
    /// entries (TAR family, ISO, raw gz/bz2/xz) bypass the upfront
    /// cumulative check. This runtime counter closes that gap by
    /// aborting the extract loop once the actual decoded total
    /// exceeds the supplied cap.
    #[allow(clippy::too_many_arguments)] // R0075-0010 added max_total_size on top of the existing 8-arg API; collapsing into an options struct would just add ceremony
    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        selection: Option<&std::collections::HashSet<usize>>,
        max_file_size: Option<u64>,
        max_total_size: Option<u64>,
    ) -> Result<Vec<ArchiveWarning>> {
        let mut warnings: Vec<ArchiveWarning> = Vec::new();

        // R0080-0009 / R0080-0016: pin the AD 0065 cached listing — the
        // exact snapshot the safety gate validated — and thread it into
        // the bulk walk below. The walk re-opens `self.path` fresh, so
        // without this the preflight (path / conflict / budget checks)
        // and the extraction could inspect different archive contents
        // after an on-disk swap. Every consumed positional index's
        // pathname is cross-checked against this listing (extending the
        // OI-0076-002 single-entry drift guard) and the visited count is
        // reconciled with the listing length at EOF.
        let listing = self.list_files_metadata_only()?;

        // Calculate total size for progress tracking (selected entries
        // only when filtering). The denominator sums only declared
        // (`Some`) sizes, so the progress numerator below only counts
        // decoded bytes for entries that carried a declared size — the
        // two must stay consistent (R0080-0065).
        //
        // R0001-0055: restrict it further to `EntryType::File`. The walk
        // below skips symlinks, hard links, and (R0001-0001) special
        // nodes, so their declared sizes never produce matching decoded
        // bytes and a fully successful extraction finished short of the
        // denominator. `parse_entry` classifies exactly the entries this
        // loop materializes as `File` (hard links become `HardLink`),
        // which keeps the two sides on the same basis. Directory entries
        // declare zero bytes, so dropping them changes no total.
        let total_bytes: u64 = if progress.is_some() {
            listing
                .iter()
                .enumerate()
                .filter(|(idx, _)| match selection {
                    Some(sel) => sel.contains(idx),
                    None => true,
                })
                .filter(|(_, e)| e.entry_type == EntryType::File)
                .filter_map(|(_, e)| e.size)
                .fold(0u64, u64::saturating_add)
        } else {
            0
        };

        let c_path = path_to_cstring_checked(&self.path)?;

        // Canonicalize the destination once — every entry's sanitize
        // pass compares against the same stable base.
        let canonical_dest = crate::security::canonicalize_dest_base(dest_path)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guards (R0070-0017): reverse drop order
            // frees the disk writer before the read handle, matching the
            // manual free order the error ladders used to maintain.
            let _archive_guard = ReadHandleGuard(archive);

            // Create disk writer
            let ext = archive_write_disk_new();
            if ext.is_null() {
                return Err(ArchiveError::format(None, "Failed to create disk writer"));
            }
            let _ext_guard = WriteDiskGuard(ext);

            let flags = extract_flags(overwrite, preserve_permissions, preserve_times);

            // R0076-0027: surface disk-writer option-setup failures instead of
            // silently extracting with weaker behavior than the caller asked for.
            let option_result = archive_write_disk_set_options(ext, flags);
            if option_result != ARCHIVE_OK {
                return Err(ArchiveError::format(None, get_archive_error(ext)));
            }

            // Extract all entries
            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut bytes_processed = 0u64;
            // R0080-0065: progress numerator, advanced only for entries
            // whose declared size fed `total_bytes`, so it can never
            // outrun the denominator. Distinct from `bytes_processed`,
            // which counts every decoded byte for the `max_total_size`
            // budget (including unknown-size entries absent from the
            // denominator).
            let mut progress_bytes = 0u64;
            let mut rate_limiter = RateLimiter::new();
            let mut entry_idx: usize = 0;

            loop {
                // Check cancellation before processing next entry
                if progress.is_some() && rate_limiter.should_call() {
                    crate::ffi::common::check_extraction_cancelled(
                        &mut progress,
                        progress_bytes.min(total_bytes),
                        total_bytes,
                        crate::error::ops::EXTRACT_ALL,
                    )?;
                }

                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0079-0022: skip — without consuming a positional
                // index — exactly the entries the listing walk skips
                // (`parse_entry` returns `None` for a NULL entry
                // pointer or NULL pathname), so the positional indices
                // below stay bijective with `list_files()` ids. This
                // also stops a NULL-pathname entry from falling
                // through to `archive_write_header` unsanitized. Later
                // per-entry code relies on this guard for non-null
                // `entry_ptr` / pathname.
                if entry_ptr.is_null() || archive_entry_pathname(entry_ptr).is_null() {
                    checked_data_skip(archive)?;
                    continue;
                }

                // Selection filter: skip entries whose positional index is
                // not in the set. Applied before link checks so non-selected
                // links are silently advanced (no warning) — FR-022 warnings
                // are only for entries that were actually targeted.
                let current_idx = entry_idx;
                entry_idx += 1;

                // R0080-0009 / R0080-0016 drift guard: every consumed
                // positional index must still name the entry the cached
                // listing (and therefore the safety gate) validated. Runs
                // before the selection and link filters so a swapped or
                // grown archive is refused even for indices this call
                // would otherwise skip. Scoped so the borrowed name drops
                // before any `archive_entry_set_pathname` below. Mirrors
                // the OI-0076-002 single-entry guard.
                {
                    let expected = match listing.get(current_idx) {
                        Some(e) => e.path.as_str(),
                        // More live entries than the validated listing —
                        // the archive grew on disk after listing.
                        None => {
                            return Err(listing_drift_extra(current_idx, listing.len()));
                        }
                    };
                    let raw_name =
                        CStr::from_ptr(archive_entry_pathname(entry_ptr)).to_string_lossy();
                    let walked_name = raw_format_name(&raw_name, &self.path);
                    if walked_name.as_ref() != expected {
                        return Err(listing_drift_mismatch(current_idx, expected, &walked_name));
                    }
                }

                if let Some(sel) = selection {
                    if !sel.contains(&current_idx) {
                        checked_data_skip(archive)?;
                        continue;
                    }
                }

                // Skip symlinks and hard links for security (FR-022)
                // Symlinks and hard links can be used for path traversal attacks
                let entry_filetype = archive_entry_filetype(entry_ptr) & AE_IFMT;
                if entry_filetype == AE_IFLNK {
                    let path = entry_pathname_string(entry_ptr);
                    let symlink_ptr = archive_entry_symlink(entry_ptr);
                    let target = if symlink_ptr.is_null() {
                        None
                    } else {
                        Some(CStr::from_ptr(symlink_ptr).to_string_lossy().into_owned())
                    };
                    warnings.push(ArchiveWarning::SkippedSymlink { path, target });
                    checked_data_skip(archive)?;
                    continue;
                }
                // Hard links are detected by archive_entry_hardlink() returning non-NULL
                let hardlink_ptr = archive_entry_hardlink(entry_ptr);
                if !hardlink_ptr.is_null() {
                    let path = entry_pathname_string(entry_ptr);
                    warnings.push(ArchiveWarning::SkippedHardLink { path });
                    checked_data_skip(archive)?;
                    continue;
                }

                // R0001-0001: only regular files and directories may be
                // materialized. Everything else — FIFOs, sockets,
                // character and block devices — reached
                // `archive_write_header` / `archive_write_disk` before
                // this guard, so a crafted archive could ask the
                // extraction backend to create special filesystem nodes
                // inside the destination. Skip them here, before any
                // destination or staging setup, rather than aborting the
                // whole extraction, so one hostile entry cannot deny
                // extraction of the rest — the same treatment the ZIP
                // backend gives `EntryType::Other` (R0081-0077).
                if entry_filetype != AE_IFREG && entry_filetype != AE_IFDIR {
                    // DCR-010: the skip is reported, not silent. A
                    // caller auditing what never reached disk gets the
                    // precise kind (FIFO / socket / character or block
                    // device) instead of an unexplained absence, and
                    // never gets a *link* warning for a device node.
                    // Links are already handled above, so only
                    // special / unclassifiable kinds arrive here; a
                    // filetype with no format bits at all carries no
                    // kind information, so it is reported as `Other`
                    // rather than dropped.
                    let path = entry_pathname_string(entry_ptr);
                    let kind = crate::error::UnsupportedEntryKind::from_unix_mode(u32::from(
                        entry_filetype,
                    ))
                    .unwrap_or(crate::error::UnsupportedEntryKind::Other);
                    warnings.push(ArchiveWarning::skipped_unsupported_entry(path, kind));
                    checked_data_skip(archive)?;
                    continue;
                }

                // Resolve the final destination for this entry and,
                // for regular file entries, prepare an atomic staging
                // sibling (R0075-0019). Directory entries are
                // materialised in place because (a) libarchive needs
                // to see the directory at its real path so subsequent
                // entries within it land correctly, and (b) directory
                // creation has no partial-payload concern. Symlink /
                // hardlink entries were already skipped above.
                //
                // Staging strategy for files:
                //   1. Pre-check destination existence in `!overwrite`
                //      mode and fail loudly before any libarchive
                //      header allocation.
                //   2. Build a unique sibling tempfile path
                //      (e.g. `dest.bin.ua-stage.XXXXXX`).
                //   3. Hand the tempfile path to libarchive via
                //      `archive_entry_set_pathname` so write_header /
                //      copy_data / finish_entry land bytes and
                //      metadata at the staging path, not the final
                //      one.
                //   4. After `archive_write_finish_entry` succeeds,
                //      atomically rename staging → final via
                //      `rename_with_overwrite`.
                //   5. On any failure, remove the staging file so the
                //      destination tree never sees a half-written
                //      file.
                let stage_atomically = entry_filetype == AE_IFREG;
                // Pathname is non-null per the R0079-0022 guard above.
                let raw_name = CStr::from_ptr(archive_entry_pathname(entry_ptr)).to_string_lossy();
                let pathname = raw_format_name(&raw_name, &self.path);
                let entry_archive_path = pathname.as_ref().to_string();
                let resolved = crate::security::sanitize_entry_path_with_base(
                    pathname.as_ref(),
                    dest_path,
                    &canonical_dest,
                )?;

                let mut staged_path: Option<std::path::PathBuf> = None;
                // Keeps the pathname bytes alive for libarchive's
                // duration on this entry (dropped at iteration end).
                let _entry_path_cstring: CString;
                if stage_atomically {
                    // R0075-0019: refuse pre-existing files in
                    // `!overwrite` mode before staging — this
                    // keeps the contract identical to the
                    // ZIP/7z extract paths and prevents a
                    // doomed-to-rename-fail copy from running
                    // to completion before being rolled back.
                    if !overwrite && resolved.exists() {
                        return Err(ArchiveError::operation_blocked(
                            crate::error::ops::EXTRACT_ALL,
                            format!(
                                "Destination file already exists: '{}' (archive entry: '{}')",
                                resolved.display(),
                                entry_archive_path
                            ),
                        ));
                    }

                    // Ensure parent directory exists so the
                    // staging file's `open()` does not fail
                    // when an entry lives below directories
                    // that libarchive would normally create.
                    if let Some(parent) = resolved.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
                    }

                    let stage = build_staging_path(&resolved)?;
                    let stage_str = stage.to_string_lossy();
                    let stage_cstr = CString::new(stage_str.as_bytes()).map_err(|_| {
                        ArchiveError::invalid_path(stage_str.as_ref(), "Contains null byte")
                    })?;
                    archive_entry_set_pathname(entry_ptr, stage_cstr.as_ptr());
                    staged_path = Some(stage);
                    _entry_path_cstring = stage_cstr;
                } else {
                    let final_str = resolved.to_string_lossy();
                    let final_cstr = CString::new(final_str.as_bytes()).map_err(|_| {
                        ArchiveError::invalid_path(final_str.as_ref(), "Contains null byte")
                    })?;
                    archive_entry_set_pathname(entry_ptr, final_cstr.as_ptr());
                    _entry_path_cstring = final_cstr;
                }

                // Helper: remove a partial staging file. Used on every
                // error path below to keep the destination tree clean.
                let cleanup_staging = |staged: &Option<std::path::PathBuf>| {
                    if let Some(p) = staged {
                        let _ = std::fs::remove_file(p);
                    }
                };

                // Write header
                let r = archive_write_header(ext, entry_ptr);
                if r != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    cleanup_staging(&staged_path);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // R0073-0004: pick the per-entry ceiling — declared
                // size when libarchive surfaces one, otherwise the
                // caller's `max_file_size` fallback. A healthy archive
                // never trips this; unknown-size or misdeclared entries
                // are stopped before `archive_write_data_block` writes
                // past the ceiling.
                // R0001-0014: `entry_declared_size` uses the `_is_set`
                // probe, so a format that never declared a size is
                // `None` (caller fallback) instead of a zero-byte
                // ceiling that every payload byte would overrun.
                let declared = entry_declared_size(entry_ptr);
                let entry_byte_cap = declared.or(max_file_size);

                // R0080-0013: fold the remaining archive-wide budget into
                // the per-entry ceiling so an unknown-size or misdeclared
                // entry cannot consume up to the full per-file ceiling
                // when only a few bytes of the cumulative `max_total_size`
                // budget remain. copy_data then fails before writing the
                // first byte past the tighter of the two limits; the
                // post-entry accounting below still catches the exact-fit
                // boundary.
                let effective_cap = match (
                    entry_byte_cap,
                    max_total_size.map(|cap| cap.saturating_sub(bytes_processed)),
                ) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };

                // R0080-0014: hand copy_data the declared size so it can
                // reject a clean short decode (a decoder that ends before
                // its authoritative size installs a truncated file). The
                // check is sparse-aware — it compares the logical extent
                // (max offset+size across blocks), which equals the
                // declared size for well-formed sparse entries.
                let expected_size = declared;

                // R0080-0015: thread a rate-limited cancellation hook into
                // copy_data's block loop so a Break vote aborts inside a
                // large entry instead of waiting for the next header.
                // Known-size entries advance the progress numerator by the
                // in-entry byte count; unknown-size entries (absent from
                // the declared-size denominator, R0080-0065) report only
                // the stable prior total, so the numerator stays monotonic
                // and never exceeds `total_bytes`.
                let known_size = declared.is_some();
                let progress_before = progress_bytes;
                let mut cancel_hook = |bytes_in_entry: u64| -> std::ops::ControlFlow<()> {
                    match progress.as_mut() {
                        Some(cb) => {
                            let numerator = if known_size {
                                progress_before
                                    .saturating_add(bytes_in_entry)
                                    .min(total_bytes)
                            } else {
                                progress_before.min(total_bytes)
                            };
                            cb.on_progress(numerator, Some(total_bytes))
                        }
                        None => std::ops::ControlFlow::Continue(()),
                    }
                };

                // Copy data — the guards free both handles on any
                // failure (R0070-0017); only the staging file needs
                // explicit cleanup.
                let actual_bytes = match copy_data(
                    archive,
                    ext,
                    &entry_archive_path,
                    effective_cap,
                    crate::error::ops::EXTRACT_ALL,
                    expected_size,
                    Some(&mut cancel_hook),
                ) {
                    Ok(n) => n,
                    Err(e) => {
                        cleanup_staging(&staged_path);
                        return Err(e);
                    }
                };

                // R0075-0010: track actual decoded bytes (not the
                // declared size, which can be 0 for unknown-size
                // streams). This cumulative counter feeds the optional
                // `max_total_size` budget below.
                bytes_processed = bytes_processed.saturating_add(actual_bytes);
                if let Some(total_cap) = max_total_size {
                    if bytes_processed > total_cap {
                        cleanup_staging(&staged_path);
                        return Err(ArchiveError::operation_blocked(
                            crate::error::ops::EXTRACT_ALL,
                            format!(
                                "Cumulative decoded payload exceeded the configured archive total of {} bytes after entry '{}'",
                                total_cap, entry_archive_path
                            ),
                        ));
                    }
                }

                // R0080-0065: advance the progress numerator only for
                // entries whose declared size fed the denominator, so the
                // numerator and denominator stay consistent.
                if known_size {
                    progress_bytes = progress_bytes.saturating_add(actual_bytes);
                }

                // Finish entry
                let finish_result = archive_write_finish_entry(ext);
                if finish_result != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    cleanup_staging(&staged_path);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // R0075-0019: rename staging → final after all
                // libarchive bookkeeping has succeeded. The install
                // matches ZIP/7z semantics: `overwrite = true`
                // replaces atomically, `overwrite = false` commits
                // through the atomic noclobber primitive so the
                // pre-check race window cannot clobber a file that
                // appeared mid-extract (R0079-0016).
                if let Some(stage) = staged_path.as_ref() {
                    if let Err(e) = install_staged_file(
                        stage.as_path(),
                        resolved.as_path(),
                        overwrite,
                        crate::error::ops::EXTRACT_ALL,
                        &entry_archive_path,
                    ) {
                        cleanup_staging(&staged_path);
                        return Err(e);
                    }
                }
            }

            // R0080-0016 cardinality guard: the fresh walk must have
            // consumed exactly as many positional indices as the
            // validated listing holds. Fewer live entries means the
            // archive was truncated or rewritten after listing — refuse
            // rather than report a short extraction as success. (The
            // "more entries" case is caught eagerly inside the loop.)
            if entry_idx != listing.len() {
                return Err(listing_drift_eof(entry_idx));
            }

            // Final progress update. R0080-0066 clamped the real
            // processed total to the denominator so the last callback
            // could never move backward from an over-total numerator;
            // R0001-0055 completes the pair by reporting the full
            // denominator on the success path. Now that the denominator
            // holds only the extractable regular-file payloads this walk
            // actually decodes, reaching this point means every one of
            // them was decoded, so 100% is the honest terminal state —
            // previously a skipped link or special entry left the
            // callback stuck below completion forever. Honor
            // cancellation here too so a Break at the end surfaces
            // consistently with the pre-entry callback path (R0070-0034).
            crate::ffi::common::check_extraction_cancelled(
                &mut progress,
                total_bytes,
                total_bytes,
                crate::error::ops::EXTRACT_ALL,
            )?;

            Ok(warnings)
        }
    }

    /// Extract a single file
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, true, true, None)
    }

    /// `max_file_size = Some(n)` (R0073-0004) is the absolute per-entry
    /// ceiling enforced inside the libarchive copy loop — see
    /// [`Self::extract_all_with_options`] for the contract.
    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        max_file_size: Option<u64>,
    ) -> Result<()> {
        // OI-0076-002: resolve through the shared single-entry gate
        // first — existence, uniqueness, and link/directory policy live
        // in validate_single_entry (the former in-method AE_IFLNK /
        // hardlink rejections are deleted; directory/special entries
        // now error at the gate, closing R0076-0036's missing dir/Other
        // rejection).
        let entries = self.list_files_metadata_only()?;
        let target = crate::security::validate_single_entry(
            &entries,
            file_path,
            crate::error::ops::EXTRACT_FILE,
        )?;
        let target_id = target.id();
        let validated_path = target.path().to_string();

        let c_path = path_to_cstring_checked(&self.path)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guard (R0070-0018).
            let _archive_guard = ReadHandleGuard(archive);

            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut entry_idx: usize = 0;

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);

                if result == ARCHIVE_EOF {
                    return Err(listing_drift_eof(target_id));
                } else if result != ARCHIVE_OK {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0079-0022: skip — without consuming a positional
                // index — exactly the headers the listing walk skips
                // (`parse_entry` returns `None` for a NULL entry pointer
                // or NULL pathname), so `entry_idx` stays bijective with
                // `list_files_metadata_only` ids.
                if entry_ptr.is_null() || archive_entry_pathname(entry_ptr).is_null() {
                    checked_data_skip(archive)?;
                    continue;
                }

                let current_idx = entry_idx;
                entry_idx += 1;
                if current_idx != target_id {
                    checked_data_skip(archive)?;
                    continue;
                }

                // OI-0076-002 drift guard. Scoped so the borrowed name
                // drops before `archive_entry_set_pathname` repoints the
                // entry's pathname buffer below.
                {
                    let raw_name =
                        CStr::from_ptr(archive_entry_pathname(entry_ptr)).to_string_lossy();
                    let walked_name = raw_format_name(&raw_name, &self.path);
                    if walked_name.as_ref() != validated_path {
                        return Err(listing_drift_mismatch(
                            current_idx,
                            &validated_path,
                            &walked_name,
                        ));
                    }
                }

                // Create disk writer
                let ext = archive_write_disk_new();
                if ext.is_null() {
                    return Err(ArchiveError::format(None, "Failed to create disk writer"));
                }
                let ext_guard = WriteDiskGuard(ext);

                let flags = extract_flags(overwrite, preserve_permissions, preserve_times);
                // R0076-0028: same return-code check as extract-all.
                let option_result = archive_write_disk_set_options(ext, flags);
                if option_result != ARCHIVE_OK {
                    return Err(ArchiveError::format(None, get_archive_error(ext)));
                }

                // Resolve the destination (with path
                // traversal protection) from the validated
                // listing path (R0076-0050).
                let full_path = sanitize_entry_path(&validated_path, dest_path)?;

                // R0076-0035: stage to a sibling tempfile and
                // rename on success, matching the extract-all
                // path, so a copy_data / header / finish failure
                // can never leave a partial file at the caller's
                // destination. Previously the sanitized final
                // path was written in place, so a mid-copy error
                // left a truncated file behind.
                //
                // Honor `!overwrite` before staging (same as
                // extract-all) so a doomed-to-rename-fail copy
                // doesn't run to completion first.
                if !overwrite && full_path.exists() {
                    return Err(ArchiveError::operation_blocked(
                        crate::error::ops::EXTRACT_FILE,
                        format!(
                            "Destination file already exists: '{}' (archive entry: '{}')",
                            full_path.display(),
                            validated_path
                        ),
                    ));
                }

                // Ensure the parent directory exists so the
                // staging file's open() succeeds for entries
                // under directories libarchive would otherwise
                // create itself.
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
                }

                let stage = build_staging_path(&full_path)?;
                let stage_str = stage.to_string_lossy();
                let stage_cstr = match CString::new(stage_str.as_bytes()) {
                    Ok(cstr) => cstr,
                    Err(_) => {
                        let _ = std::fs::remove_file(&stage);
                        return Err(ArchiveError::invalid_path(
                            stage_str.as_ref(),
                            "Contains null byte",
                        ));
                    }
                };

                archive_entry_set_pathname(entry_ptr, stage_cstr.as_ptr());

                // Write header
                let header_result = archive_write_header(ext, entry_ptr);
                if header_result != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    let _ = std::fs::remove_file(&stage);
                    return Err(ArchiveError::format(None, error_msg));
                }

                // R0073-0004: same per-entry ceiling rule as
                // the multi-entry path — declared size when
                // libarchive surfaces one, caller's
                // `max_file_size` otherwise. R0001-0014: the shared
                // `_is_set`-aware read, so an undeclared size falls back
                // to the caller cap instead of a zero-byte ceiling.
                let declared = entry_declared_size(entry_ptr);
                let entry_byte_cap = declared.or(max_file_size);

                // R0080-0014: hand copy_data the declared size so a clean
                // short decode surfaces as corruption instead of
                // installing a truncated single file — mirrors the
                // extract-all guard. No archive-wide budget or
                // cancellation surface here, so `max_bytes` stays the
                // per-entry cap and the cancellation hook is `None`.
                let expected_size = declared;

                // Copy data — clean up the staging file on any
                // failure (the guards free both handles;
                // R0070-0018 + R0076-0035 partial-file cleanup).
                if let Err(e) = copy_data(
                    archive,
                    ext,
                    &validated_path,
                    entry_byte_cap,
                    crate::error::ops::EXTRACT_FILE,
                    expected_size,
                    None,
                ) {
                    let _ = std::fs::remove_file(&stage);
                    return Err(e);
                }

                // Finish
                let finish_result = archive_write_finish_entry(ext);
                if finish_result != ARCHIVE_OK {
                    let error_msg = get_archive_error(ext);
                    let _ = std::fs::remove_file(&stage);
                    return Err(ArchiveError::format(None, error_msg));
                }
                // Close the disk writer before installing, matching
                // the free order the manual ladder maintained.
                drop(ext_guard);

                // R0076-0035: install atomically now that every
                // libarchive bookkeeping step has succeeded,
                // matching ZIP/7z semantics: overwrite
                // replaces atomically; `!overwrite` commits
                // through the atomic noclobber primitive so the
                // pre-check race window cannot clobber a file
                // that appeared mid-extract (R0079-0016).
                if let Err(e) = install_staged_file(
                    stage.as_path(),
                    full_path.as_path(),
                    overwrite,
                    crate::error::ops::EXTRACT_FILE,
                    &validated_path,
                ) {
                    let _ = std::fs::remove_file(&stage);
                    return Err(e);
                }
                return Ok(());
            }
        }
    }

    /// Extract to memory using direct in-memory extraction (no temp files)
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.extract_to_memory_with_max_bytes(file_path, None)
    }

    /// Extract to memory with an explicit per-entry byte cap.
    ///
    /// `max_bytes`:
    /// - `None` — keep the legacy behavior: a known-size entry caps at
    ///   the declared size + 1 byte; an unknown-size entry caps at
    ///   the internal `MAX_UNKNOWN_MEMORY_EXTRACT` (4 GiB) safety
    ///   ceiling.
    /// - `Some(cap)` — apply `cap` as a hard ceiling regardless of the
    ///   declared size (R0075-0017). Catches the case where a caller
    ///   supplies a tighter `ExtractionLimits::max_file_size` than the
    ///   entry's declared size, AND the case where libarchive returns
    ///   an unknown declared size and the legacy 4 GiB fallback is
    ///   stricter than the caller wants.
    pub fn extract_to_memory_with_max_bytes(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        // OI-0076-002: resolve through the shared single-entry gate
        // first — existence, uniqueness, and link/directory policy live
        // in validate_single_entry (closes R0076-0037's missing kind
        // check for memory extraction).
        let entries = self.list_files_metadata_only()?;
        let target = crate::security::validate_single_entry(
            &entries,
            file_path,
            crate::error::ops::EXTRACT_TO_MEMORY,
        )?;
        let target_id = target.id();
        let validated_path = target.path().to_string();

        let c_path = path_to_cstring_checked(&self.path)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guard (R0070-0017).
            let _archive_guard = ReadHandleGuard(archive);

            let mut entry: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut entry_idx: usize = 0;

            // Seek to the validated listing index (R0076-0059: by
            // stable position, never by name).
            loop {
                let r = archive_read_next_header(archive, &mut entry);
                if r == ARCHIVE_EOF {
                    return Err(listing_drift_eof(target_id));
                }
                if r != ARCHIVE_OK && r != ARCHIVE_WARN {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0079-0022: skip — without consuming a positional
                // index — exactly the headers the listing walk skips
                // (`parse_entry` returns `None` for a NULL entry pointer
                // or NULL pathname), so `entry_idx` stays bijective with
                // `list_files_metadata_only` ids.
                if entry.is_null() || archive_entry_pathname(entry).is_null() {
                    checked_data_skip(archive)?;
                    continue;
                }

                let current_idx = entry_idx;
                entry_idx += 1;
                if current_idx != target_id {
                    checked_data_skip(archive)?;
                    continue;
                }

                // OI-0076-002 drift guard.
                {
                    let entry_name_raw =
                        CStr::from_ptr(archive_entry_pathname(entry)).to_string_lossy();
                    let entry_name = raw_format_name(&entry_name_raw, &self.path);
                    if entry_name.as_ref() != validated_path {
                        return Err(listing_drift_mismatch(
                            current_idx,
                            &validated_path,
                            &entry_name,
                        ));
                    }
                }
                // Read data blocks directly into memory.
                //
                // R0081-0052: `archive_entry_size()` returns 0 both for a
                // genuinely empty entry and for one whose size the format
                // never declared, so a known-zero entry must keep
                // `Some(0)` — the declared-size contract below then
                // rejects any byte an empty entry produces — and only a
                // truly unset size falls back to the safety cap.
                // R0001-0014: that `_is_set` probe now lives in the
                // shared `entry_declared_size` helper, which the listing
                // and disk paths route through too.
                let size_usize = match entry_declared_size(entry) {
                    Some(declared) => match usize::try_from(declared) {
                        Ok(s) => Some(s),
                        Err(_) => {
                            return Err(ArchiveError::operation_blocked(
                                crate::error::ops::EXTRACT_TO_MEMORY,
                                format!("Entry too large for memory: {} bytes", declared),
                            ));
                        }
                    },
                    None => None,
                };

                let mut buffer = Vec::new();
                if let Some(size) = size_usize {
                    if buffer.try_reserve(size).is_err() {
                        return Err(ArchiveError::operation_blocked(
                            crate::error::ops::EXTRACT_TO_MEMORY,
                            format!("Failed to allocate {} bytes", size),
                        ));
                    }
                }

                let mut buff: *const c_void = std::ptr::null();
                let mut size: usize = 0;
                let mut offset: c_longlong = 0;

                // Cap arrival bytes so an over-producing decoder
                // is detected rather than silently filling RAM
                // (R0070-0015). When the declared size is 0 /
                // unknown — typical for libarchive's raw
                // single-file streams (`.gz`, `.bz2`, `.xz`) and
                // for some TAR/CPIO entries — fall back to a
                // conservative default cap so a hostile archive
                // cannot grow the buffer until the process is
                // killed (R0071-0004). 4 GiB is generous enough
                // that legitimate single-entry memory extraction
                // succeeds and small enough to refuse the
                // open-ended decompression bomb.
                //
                // R0075-0017: if the caller supplied `max_bytes`,
                // honour the tighter of (caller cap, declared
                // size, fallback cap). This closes the gap where
                // an unknown-size entry would silently expand to
                // the 4 GiB fallback even when the caller's
                // `ExtractionLimits::max_file_size` was much
                // smaller.
                const MAX_UNKNOWN_MEMORY_EXTRACT: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
                // R0081-0057: keep the archive's own declared ceiling and
                // the caller's policy ceiling distinct so a cap breach is
                // attributed to the right party. `declared_ceiling` is the
                // format's contract (present only for a size-declared
                // entry); the effective `cap` folds in the tighter of the
                // caller's `max_bytes` and the unknown-size fallback.
                let declared_ceiling: Option<u64> = size_usize.map(|s| s as u64);
                let baseline_cap = declared_ceiling.unwrap_or(MAX_UNKNOWN_MEMORY_EXTRACT);
                let cap = match max_bytes {
                    Some(caller_cap) => baseline_cap.min(caller_cap),
                    None => baseline_cap,
                };

                loop {
                    let r = archive_read_data_block(archive, &mut buff, &mut size, &mut offset);
                    if r == ARCHIVE_EOF {
                        break;
                    }
                    if r != ARCHIVE_OK {
                        return Err(ArchiveError::format(None, get_archive_error(archive)));
                    }

                    // R0081-0053: a negative block offset is corrupt
                    // metadata, not a zero offset — reject it before any
                    // accounting rather than coercing it to 0.
                    let block_offset = match u64::try_from(offset) {
                        Ok(o) => o,
                        Err(_) => {
                            return Err(ArchiveError::corruption(
                                file_path.to_string(),
                                format!(
                                    "extract_to_memory: backend reported a negative block offset ({})",
                                    offset
                                ),
                            ));
                        }
                    };

                    // R0081-0055: the supported sparse ordering is monotonic
                    // and non-overlapping — every block starts at or after
                    // the bytes assembled so far. A block whose offset is
                    // behind the current logical position would overwrite
                    // already reconstructed data; the old `saturating_sub`
                    // silently appended it at the end, corrupting the file
                    // while returning success. Reject it as corruption.
                    let assembled = buffer.len() as u64;
                    if block_offset < assembled {
                        return Err(ArchiveError::corruption(
                            file_path.to_string(),
                            format!(
                                "extract_to_memory: backend reported a backward/overlapping block (offset {} precedes {} bytes already assembled)",
                                block_offset, assembled
                            ),
                        ));
                    }

                    // R0081-0056: a positive-sized block with a null buffer
                    // contributes no bytes; accepting it as success would
                    // silently truncate an unknown-size entry. Treat it as
                    // backend corruption.
                    if size > 0 && buff.is_null() {
                        return Err(ArchiveError::corruption(
                            file_path.to_string(),
                            "extract_to_memory: backend reported a non-empty block with a null buffer"
                                .to_string(),
                        ));
                    }

                    // R0079-0023 (kept — see
                    // extract_to_memory_zero_fills_sparse_holes):
                    // `archive_read_data_block` returns only data segments
                    // plus their offset — holes (sparse GNU/pax tar entries)
                    // are the consumer's responsibility, which the disk path
                    // delegates to `archive_write_data_block`. Zero-fill the
                    // gap between the bytes assembled so far and this block's
                    // offset (a trailing hole arrives as a zero-length block
                    // at the end offset). Gap bytes count against `cap`
                    // exactly like payload bytes, so a hostile offset cannot
                    // grow the buffer past the ceiling.
                    let gap = block_offset - assembled;
                    let needed = gap.saturating_add(size as u64);
                    if needed > 0 {
                        if assembled.saturating_add(needed) > cap {
                            // R0081-0057: attribute the breach to the binding
                            // ceiling. A size-declared entry that overruns its
                            // own declared size is archive corruption; a breach
                            // of the (tighter) caller cap or the unknown-size
                            // safety fallback is a policy refusal, not
                            // corruption — and it must name the real limit.
                            return Err(match declared_ceiling {
                                Some(declared) if cap >= declared => ArchiveError::corruption(
                                    file_path.to_string(),
                                    format!(
                                        "extract_to_memory: backend produced more than the declared {} bytes",
                                        declared
                                    ),
                                ),
                                Some(_) => ArchiveError::operation_blocked(
                                    crate::error::ops::EXTRACT_TO_MEMORY,
                                    format!(
                                        "extract_to_memory: output exceeds the caller-configured {} byte limit",
                                        cap
                                    ),
                                ),
                                None => ArchiveError::operation_blocked(
                                    crate::error::ops::EXTRACT_TO_MEMORY,
                                    format!(
                                        "extract_to_memory: unknown-size entry exceeds {} byte safety cap",
                                        cap
                                    ),
                                ),
                            });
                        }
                        if gap > 0 {
                            let Ok(gap_len) = usize::try_from(gap) else {
                                return Err(ArchiveError::operation_blocked(
                                    crate::error::ops::EXTRACT_TO_MEMORY,
                                    format!(
                                        "Entry too large for memory: sparse hole of {} bytes",
                                        gap
                                    ),
                                ));
                            };
                            buffer.resize(buffer.len() + gap_len, 0);
                        }
                        if size > 0 {
                            // `buff` is non-null here (checked above).
                            let slice = std::slice::from_raw_parts(buff as *const u8, size);
                            buffer.extend_from_slice(slice);
                        }
                    }
                }

                // R0075-0025: when the header declared a positive
                // size, refuse a short read so a truncated stream
                // surfaces as corruption rather than silent
                // success. Unknown-size streams (declared 0 / -1)
                // are accepted at whatever EOF the decoder
                // delivers — that is the contract for raw
                // gzip/bzip2/xz inputs.
                if let Some(declared) = size_usize {
                    if buffer.len() < declared {
                        return Err(ArchiveError::corruption(
                            file_path.to_string(),
                            format!(
                                "extract_to_memory: backend returned {} bytes, expected {}",
                                buffer.len(),
                                declared
                            ),
                        ));
                    }
                }

                return Ok(buffer);
            }
        }
    }

    /// Get archive path (preserves raw bytes per AD 0064)
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Extract a single file to a stream (Phase 2.4)
    ///
    /// Returns a StreamingExtractor that reads data directly from the archive
    /// using `archive_read_data`, without loading the entire file into memory.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        // OI-0076-002: resolve through the shared single-entry gate
        // first — existence, uniqueness, and link/directory policy live
        // in validate_single_entry (closes R0076-0038's missing kind
        // check for streaming). The reader then seeks by stable listing
        // id, never by name.
        let entries = self.list_files_metadata_only()?;
        let target = crate::security::validate_single_entry(
            &entries,
            file_path,
            crate::error::ops::EXTRACT_TO_STREAM,
        )?;
        let reader = LibarchiveStreamReader::open(&self.path, target.id(), target.path())?;
        let size = reader.entry_size;
        Ok(crate::streaming::StreamingExtractor::new(
            Box::new(reader),
            size,
        ))
    }

    /// Stream a single entry by its stable listing id (ti-2a6e3153).
    ///
    /// Reuses the OI-0076-002 seek machinery
    /// ([`LibarchiveStreamReader::open`]) but takes the id and normalized
    /// path the caller already resolved instead of routing through
    /// `validate_single_entry`, whose duplicate-path rejection is exactly
    /// what the digest walk must bypass to hash each occurrence of a
    /// duplicate-path tar. The reader still performs the OI-0076-002 drift
    /// guard (it refuses a header whose id maps to a mismatched name), and
    /// derives `entry_size` identically to `extract_to_stream`. The
    /// per-entry limit checks the path-based gate would apply are the
    /// caller's responsibility (see
    /// `ValidatedSource::extract_to_stream_by_id`).
    pub(crate) fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        let reader = LibarchiveStreamReader::open(&self.path, id, validated_path)?;
        let size = reader.entry_size;
        Ok(crate::streaming::StreamingExtractor::new(
            Box::new(reader),
            size,
        ))
    }

    /// Resolve every target's payload in ONE traversal of the archive
    /// (OI-0001-009 / ticgit `82bf8fd4`).
    ///
    /// [`Self::extract_to_stream_by_listing_id`] opens a fresh handle and
    /// re-walks from the first header for every entry, so the digest of a
    /// CRC-less archive re-decoded the whole stream once per member —
    /// quadratic, and on a compressed TAR it re-ran the decompressor from
    /// byte zero each time. This method opens one handle, walks the
    /// headers once in ascending id order, and hands each target a reader
    /// borrowed from that handle.
    ///
    /// **What is preserved verbatim from the per-entry path.** The
    /// positional index is advanced by exactly the same rule
    /// ([`parse_entry`]-invisible headers — NULL entry pointer or NULL
    /// pathname — are skipped *without* consuming an index, R0079-0022),
    /// so `id` keeps meaning the same thing it means in
    /// `list_files_metadata_only`. The OI-0076-002 drift guard still fires
    /// on a name mismatch. Resolution stays keyed on the id: the
    /// `validated_path` is compared, never searched for, so a
    /// duplicate-path tar's two occurrences remain two distinct targets.
    ///
    /// **What is deliberately NOT done here.** No size bound is applied to
    /// the borrowed reader. The DCR-011 exact/ceiling contract belongs to
    /// the caller, which is the only party that knows whether the listing
    /// declared a size at all — inventing one here for an unknown-size
    /// entry is the exact mistake hard constraint 3 forbids.
    ///
    /// Errors from `visit` propagate unchanged so the visitor's own
    /// entry-naming diagnostics survive; the read handle is freed on every
    /// exit by [`ReadHandleGuard`].
    pub(crate) fn visit_payloads_by_listing_id(
        &self,
        targets: &[crate::backend::PayloadTarget<'_>],
        visit: &mut crate::backend::PayloadVisitor<'_>,
    ) -> Result<()> {
        if targets.is_empty() {
            return Ok(());
        }
        // Ascending id order is what makes one forward walk sufficient.
        // The caller passes listing order, which for this backend already
        // is id order; sorting keeps the contract independent of that.
        let mut pending: Vec<&crate::backend::PayloadTarget<'_>> = targets.iter().collect();
        pending.sort_by_key(|t| t.id);

        let c_path = path_to_cstring_checked(&self.path)?;

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guard (R0070-0017) — including the early
            // returns below and any error `visit` propagates.
            let _archive_guard = ReadHandleGuard(archive);

            let mut entry: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut entry_idx: usize = 0;
            let mut next = 0usize;

            while next < pending.len() {
                let r = archive_read_next_header(archive, &mut entry);
                if r == ARCHIVE_EOF {
                    // A target the listing promised never arrived: the
                    // archive changed under the cached listing.
                    return Err(listing_drift_eof(pending[next].id));
                }
                if r != ARCHIVE_OK && r != ARCHIVE_WARN {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0079-0022: skip without consuming a positional index,
                // exactly as the listing walk and the single-entry seek do.
                if entry.is_null() || archive_entry_pathname(entry).is_null() {
                    checked_data_skip(archive)?;
                    continue;
                }

                let current_idx = entry_idx;
                entry_idx += 1;
                if current_idx != pending[next].id {
                    checked_data_skip(archive)?;
                    continue;
                }

                let target = pending[next];

                // OI-0076-002 drift guard — refuse a header whose name
                // disagrees with the listing name resolved for this id.
                let name_raw = CStr::from_ptr(archive_entry_pathname(entry)).to_string_lossy();
                let name = raw_format_name(&name_raw, &self.path);
                if name.as_ref() != target.validated_path {
                    return Err(listing_drift_mismatch(
                        current_idx,
                        target.validated_path,
                        &name,
                    ));
                }

                {
                    let mut reader = BorrowedEntryReader::new(archive);
                    visit(target, &mut reader)?;
                }
                // Drain whatever the visitor left unread so the cursor
                // lands on this entry's end before the next header.
                checked_data_skip(archive)?;
                next += 1;
            }
        }

        Ok(())
    }

    /// Test archive integrity by verifying checksums for all files
    ///
    /// Streams each file and verifies checksums (libarchive validates during read).
    /// Returns a list of file paths that failed verification.
    ///
    /// R0001-0017: a clean decode is not sufficient — formats without a
    /// backend checksum (plain TAR, CPIO, ISO) report success for a
    /// truncated payload, so each entry's sparse-aware decoded extent is
    /// also compared against its declared size, the same length
    /// invariant [`copy_data`] enforces on the extraction path.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let c_path = path_to_cstring_checked(&self.path)?;

        let mut failed_files = Vec::new();

        unsafe {
            let archive = Self::open_read_handle(&c_path)?;
            // Free-on-every-exit guard (R0070-0017).
            let _archive_guard = ReadHandleGuard(archive);

            let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();

            loop {
                let result = archive_read_next_header(archive, &mut entry_ptr);
                if result == ARCHIVE_EOF {
                    break;
                } else if result != ARCHIVE_OK && result != ARCHIVE_WARN {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0001-0052: the format is only bid once a header has
                // been read, so ask the handle per entry.
                let ctime_is_creation = format_ctime_is_creation(archive);
                let Some(entry) = parse_entry(entry_ptr, ctime_is_creation) else {
                    checked_data_skip(archive)?;
                    continue;
                };

                if entry.entry_type != EntryType::File {
                    checked_data_skip(archive)?;
                    continue;
                }

                // R0001-0014 / R0001-0017: `_is_set`-aware declared size —
                // `None` for a format that never declared one, which skips
                // the length equality check below.
                let declared_size = entry.size;
                let path = entry.path;

                // Stream through data — libarchive validates checksums during read
                let mut buf: *const c_void = std::ptr::null();
                let mut size: usize = 0;
                let mut offset: c_longlong = 0;
                let mut failed = false;
                // R0001-0017: highest logical extent (offset + size) any
                // block reached, the same sparse-aware accounting
                // `copy_data` performs, so a hole-bearing entry still
                // lands on its declared size.
                let mut logical_end: u64 = 0;
                loop {
                    let r = archive_read_data_block(archive, &mut buf, &mut size, &mut offset);
                    if r == ARCHIVE_EOF {
                        break;
                    }
                    if r != ARCHIVE_OK {
                        // R0001-0056: classify before recording. A
                        // password / encryption failure is an operational
                        // fault, not a payload integrity fault, so it
                        // propagates as the typed error the drain path
                        // below already produces instead of being reported
                        // as a damaged entry — `failed_files` stays
                        // reserved for payload faults.
                        let classified = classify_libarchive_error(get_archive_error(archive));
                        if matches!(classified, ArchiveError::Password { .. }) {
                            return Err(classified);
                        }
                        failed = true;
                        // R0080-0019: drain the remaining blocks so the
                        // libarchive cursor stays consistent for the next
                        // entry, but capture the terminal status. The old
                        // `while ... == ARCHIVE_OK {}` could not tell a
                        // clean drain (EOF) from a wedged cursor (error);
                        // a non-EOF terminus means subsequent entries would
                        // be missed or misattributed, so abort the whole
                        // integrity walk with a typed archive error rather
                        // than returning a normal failed-path list.
                        let drain_status = loop {
                            let dr =
                                archive_read_data_block(archive, &mut buf, &mut size, &mut offset);
                            if dr != ARCHIVE_OK {
                                break dr;
                            }
                        };
                        if drain_status != ARCHIVE_EOF {
                            return Err(classify_libarchive_error(get_archive_error(archive)));
                        }
                        break;
                    }

                    match u64::try_from(offset) {
                        Ok(block_offset) => {
                            logical_end = logical_end.max(block_offset.saturating_add(size as u64));
                        }
                        // R0001-0017: a negative block offset is corrupt
                        // metadata — the same judgement `copy_data` makes.
                        // Mark the entry damaged but keep reading so the
                        // cursor still lands on this entry's EOF.
                        Err(_) => failed = true,
                    }
                }

                // R0001-0017: a decoder that reached a clean EOF short of
                // (or past) the entry's authoritative declared size did
                // not reproduce the payload, even though no checksum
                // fired. Unknown-size entries skip the check.
                if !failed {
                    if let Some(declared) = declared_size {
                        if logical_end != declared {
                            failed = true;
                        }
                    }
                }

                if failed {
                    failed_files.push(path);
                }
            }
        }

        Ok(failed_files)
    }
}

/// Reader over the *current* entry of a libarchive handle the walk still
/// owns (OI-0001-009).
///
/// The mirror image of [`LibarchiveStreamReader`]: same
/// `archive_read_data` loop and the same "a negative return is a read
/// error, zero is this entry's EOF" contract, but it **borrows** the
/// handle instead of owning it, so it has no `Drop` — freeing the handle
/// here would pull it out from under
/// [`LibarchiveArchive::visit_payloads_by_listing_id`], which still has
/// later targets to reach. The lifetime ties the reader to the walk's
/// stack frame; it is handed to the visitor as `&mut dyn Read` and never
/// escapes it.
struct BorrowedEntryReader<'a> {
    archive: *mut Archive,
    eof: bool,
    _handle: std::marker::PhantomData<&'a mut Archive>,
}

impl BorrowedEntryReader<'_> {
    /// # Safety
    /// `archive` must be a live read handle positioned on the header of
    /// the entry whose payload is wanted, and must outlive the reader.
    unsafe fn new(archive: *mut Archive) -> Self {
        Self {
            archive,
            eof: false,
            _handle: std::marker::PhantomData,
        }
    }
}

impl std::io::Read for BorrowedEntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.eof || buf.is_empty() {
            return Ok(0);
        }

        let n =
            unsafe { archive_read_data(self.archive, buf.as_mut_ptr() as *mut c_void, buf.len()) };

        if n == 0 {
            self.eof = true;
            Ok(0)
        } else if n < 0 {
            self.eof = true;
            Err(std::io::Error::other(format!(
                "libarchive read error: {}",
                unsafe { get_archive_error(self.archive) }
            )))
        } else {
            Ok(n as usize)
        }
    }
}

/// Streaming reader that reads directly from a libarchive handle via `archive_read_data`.
///
/// Owns the archive handle and frees it on drop. Positioned at the target entry
/// after construction — subsequent `Read::read` calls pull decompressed data
/// without buffering the entire file in memory.
pub(crate) struct LibarchiveStreamReader {
    archive: *mut Archive,
    /// Entry size if known (from archive_entry_size)
    pub(crate) entry_size: Option<u64>,
    eof: bool,
}

// SAFETY: The archive handle is exclusively owned by this struct.
// No other code accesses it after construction until drop.
unsafe impl Send for LibarchiveStreamReader {}

impl LibarchiveStreamReader {
    /// Open the archive and position at the gate-validated target entry
    /// for streaming reads.
    ///
    /// OI-0076-002: `target_id` is the stable listing position resolved
    /// by `validate_single_entry`, and `validated_path` the matching
    /// normalized listing name — the walk seeks by index and refuses a
    /// name mismatch instead of re-matching by name.
    fn open(archive_path: &Path, target_id: usize, validated_path: &str) -> Result<Self> {
        let c_path = path_to_cstring_checked(archive_path)?;

        unsafe {
            let archive = LibarchiveArchive::open_read_handle(&c_path)?;
            // Free-on-every-exit guard; disarmed via `mem::forget` on
            // the success path, where ownership of the handle moves to
            // the returned reader (whose Drop frees it).
            let guard = ReadHandleGuard(archive);

            let mut entry: *mut LibarchiveEntry = std::ptr::null_mut();
            let mut entry_idx: usize = 0;
            loop {
                let r = archive_read_next_header(archive, &mut entry);
                if r == ARCHIVE_EOF {
                    return Err(listing_drift_eof(target_id));
                }
                if r != ARCHIVE_OK && r != ARCHIVE_WARN {
                    return Err(ArchiveError::format(None, get_archive_error(archive)));
                }

                // R0079-0022: skip — without consuming a positional
                // index — exactly the headers the listing walk skips
                // (`parse_entry` returns `None` for a NULL entry pointer
                // or NULL pathname), so `entry_idx` stays bijective with
                // `list_files_metadata_only` ids.
                if entry.is_null() || archive_entry_pathname(entry).is_null() {
                    checked_data_skip(archive)?;
                    continue;
                }

                let current_idx = entry_idx;
                entry_idx += 1;
                if current_idx != target_id {
                    checked_data_skip(archive)?;
                    continue;
                }

                // OI-0076-002 drift guard.
                let name_raw = CStr::from_ptr(archive_entry_pathname(entry)).to_string_lossy();
                let name = raw_format_name(&name_raw, archive_path);
                if name.as_ref() != validated_path {
                    return Err(listing_drift_mismatch(current_idx, validated_path, &name));
                }

                // R0081-0062: `archive_entry_size()` returns 0 for both a
                // known-empty entry and one whose size the format never
                // declared, so a known zero size must survive as
                // `Some(0)` — downstream stream enforcement can then
                // reject any byte an empty entry over-produces instead of
                // treating it as unknown length. R0001-0014: shared with
                // the listing, disk, and memory paths via
                // `entry_declared_size`.
                let entry_size = entry_declared_size(entry);
                std::mem::forget(guard);
                return Ok(Self {
                    archive,
                    entry_size,
                    eof: false,
                });
            }
        }
    }
}

impl std::io::Read for LibarchiveStreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.eof || buf.is_empty() {
            return Ok(0);
        }

        let n =
            unsafe { archive_read_data(self.archive, buf.as_mut_ptr() as *mut c_void, buf.len()) };

        if n == 0 {
            self.eof = true;
            Ok(0)
        } else if n < 0 {
            self.eof = true;
            Err(std::io::Error::other(format!(
                "libarchive read error: {}",
                unsafe { get_archive_error(self.archive) }
            )))
        } else {
            Ok(n as usize)
        }
    }
}

impl Drop for LibarchiveStreamReader {
    fn drop(&mut self) {
        if !self.archive.is_null() {
            unsafe {
                archive_read_free(self.archive);
            }
            self.archive = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R0001-0053: seconds and nanoseconds must combine instead of the
    /// subsecond half being dropped on the floor.
    #[test]
    fn libarchive_time_combines_seconds_and_nanoseconds() {
        let t = libarchive_time(10, 500_000_000).expect("epoch + 10.5s is representable");
        let d = t
            .duration_since(std::time::UNIX_EPOCH)
            .expect("post-epoch instant");
        assert_eq!(d.as_secs(), 10);
        assert_eq!(d.subsec_nanos(), 500_000_000);
    }

    /// The crate-wide pre-epoch policy (R0076-0043) must survive the
    /// R0001-0053 nanosecond plumbing.
    #[test]
    fn libarchive_time_drops_pre_epoch_seconds() {
        assert!(libarchive_time(-1, 0).is_none());
        assert!(libarchive_time(-1, 500_000_000).is_none());
    }

    /// A corrupt header can carry any `_nsec` value; an out-of-range one
    /// degrades to the whole second rather than discarding the
    /// timestamp.
    #[test]
    fn libarchive_time_ignores_out_of_range_nanoseconds() {
        for nanos in [-1, 1_000_000_000, 2_000_000_000] {
            let t = libarchive_time(42, nanos).expect("seconds still convert");
            let d = t
                .duration_since(std::time::UNIX_EPOCH)
                .expect("post-epoch instant");
            assert_eq!(d.as_secs(), 42, "nanos = {nanos}");
            assert_eq!(d.subsec_nanos(), 0, "nanos = {nanos}");
        }
    }

    /// R0001-0052: the ZIP base code must sit on the same grid as its
    /// sibling `super::libarchive` format constants, otherwise the
    /// creation-time gate silently never (or always) fires.
    #[test]
    fn zip_format_base_is_a_distinct_masked_base_code() {
        assert_eq!(
            ARCHIVE_FORMAT_ZIP_BASE & ARCHIVE_FORMAT_BASE_MASK,
            ARCHIVE_FORMAT_ZIP_BASE
        );
        assert_ne!(ARCHIVE_FORMAT_ZIP_BASE, ARCHIVE_FORMAT_TAR_BASE);
        assert_ne!(ARCHIVE_FORMAT_ZIP_BASE, ARCHIVE_FORMAT_RAW_BASE);
    }

    // ── DCR-010 / R0001-0001: the kind-allowlist skip is reported ──

    /// One 512-byte ustar header.
    ///
    /// Hand-rolled because neither of the fixture routes works here:
    /// this crate's writer only emits regular files, and a real FIFO or
    /// device node needs `mkfifo`/`mknod` (no libc dependency, and
    /// device nodes need root). libarchive reads the synthesised header
    /// exactly as it reads a GNU tar one, which is all the walk cares
    /// about.
    fn ustar_header(name: &str, typeflag: u8, size: usize) -> [u8; 512] {
        fn put(h: &mut [u8; 512], off: usize, text: &str) {
            h[off..off + text.len()].copy_from_slice(text.as_bytes());
        }

        let mut h = [0u8; 512];
        let name_bytes = name.as_bytes();
        assert!(
            name_bytes.len() < 100,
            "fixture names must fit the ustar name field"
        );
        h[..name_bytes.len()].copy_from_slice(name_bytes);
        // Directory entries need the traversal bit or the entries
        // beneath them cannot be created.
        let mode = if typeflag == b'5' {
            "0000755\0"
        } else {
            "0000644\0"
        };
        put(&mut h, 100, mode);
        put(&mut h, 108, "0000000\0"); // uid
        put(&mut h, 116, "0000000\0"); // gid
        put(&mut h, 124, &format!("{size:011o}\0")); // size
        put(&mut h, 136, "00000000000\0"); // mtime
        h[156] = typeflag;
        put(&mut h, 257, "ustar\0"); // magic
        put(&mut h, 263, "00"); // version
        put(&mut h, 329, "0000000\0"); // devmajor
        put(&mut h, 337, "0000000\0"); // devminor

        // Checksum: unsigned sum of every byte with the checksum field
        // itself read as spaces.
        for b in h[148..156].iter_mut() {
            *b = b' ';
        }
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        put(&mut h, 148, &format!("{sum:06o}\0 "));
        h
    }

    /// Write a tar holding zero-length `specials` (name, ustar typeflag)
    /// followed by one real regular file, so a test can assert both the
    /// skip warnings and that the allowlisted entry still lands.
    fn write_ustar(path: &Path, specials: &[(&str, u8)], regular: (&str, &[u8])) {
        let mut buf: Vec<u8> = Vec::new();
        for (name, typeflag) in specials {
            buf.extend_from_slice(&ustar_header(name, *typeflag, 0));
        }
        buf.extend_from_slice(&ustar_header(regular.0, b'0', regular.1.len()));
        buf.extend_from_slice(regular.1);
        let pad = (512 - regular.1.len() % 512) % 512;
        let padded = buf.len() + pad;
        buf.resize(padded, 0);
        // Two zero blocks mark end-of-archive.
        let with_eoa = buf.len() + 1024;
        buf.resize(with_eoa, 0);
        std::fs::write(path, &buf).expect("write fixture tar");
    }

    /// FIFOs, character devices and block devices are skipped by the
    /// R0001-0001 allowlist, and the skip must reach the caller as
    /// `SkippedUnsupportedEntry` naming the precise kind — never as a
    /// link warning, and never silently. The allowlisted entry in the
    /// same archive still extracts.
    #[test]
    fn special_nodes_are_reported_as_skipped_unsupported_entries() {
        use crate::error::{EntrySkipReason, UnsupportedEntryKind};

        let dir = tempfile::tempdir().expect("tempdir");
        let tar = dir.path().join("special.tar");
        write_ustar(
            &tar,
            &[("pipe", b'6'), ("chardev", b'3'), ("blockdev", b'4')],
            ("real.txt", b"payload"),
        );
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).expect("create dest");

        let archive = LibarchiveArchive::open(&tar).expect("open fixture tar");
        let warnings = archive.extract_all(&dest, None).expect("extract");

        assert_eq!(
            std::fs::read(dest.join("real.txt")).expect("the regular entry must extract"),
            b"payload",
            "one hostile entry must not deny extraction of the rest"
        );
        for skipped in ["pipe", "chardev", "blockdev"] {
            assert!(
                !dest.join(skipped).exists(),
                "{skipped} must never be materialized"
            );
        }

        let mut reported: Vec<(String, UnsupportedEntryKind)> = warnings
            .iter()
            .filter_map(|w| match w {
                ArchiveWarning::SkippedUnsupportedEntry { path, kind, reason } => {
                    assert_eq!(
                        *reason,
                        EntrySkipReason::UnsupportedKindOnExtract,
                        "extraction skips carry the extraction reason"
                    );
                    Some((path.clone(), *kind))
                }
                _ => None,
            })
            .collect();
        // `UnsupportedEntryKind` is deliberately not `Ord`; the path
        // alone gives a stable order for the comparison below.
        reported.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            reported,
            vec![
                ("blockdev".to_string(), UnsupportedEntryKind::BlockDevice),
                ("chardev".to_string(), UnsupportedEntryKind::CharacterDevice),
                ("pipe".to_string(), UnsupportedEntryKind::Fifo),
            ],
            "every skipped special node must be named with its kind; got {warnings:?}"
        );
        assert!(
            !warnings.iter().any(|w| matches!(
                w,
                ArchiveWarning::SkippedSymlink { .. } | ArchiveWarning::SkippedHardLink { .. }
            )),
            "a device node must never be reported as a link: {warnings:?}"
        );
    }

    /// The rendered warning has to be readable on its own — a caller
    /// logging it should see the kind, the path, and why nothing was
    /// written.
    #[test]
    fn skipped_special_node_warning_renders_kind_and_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar = dir.path().join("fifo.tar");
        write_ustar(&tar, &[("var/run/sock.fifo", b'6')], ("keep.txt", b"x"));
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).expect("create dest");

        let archive = LibarchiveArchive::open(&tar).expect("open fixture tar");
        let warnings = archive.extract_all(&dest, None).expect("extract");
        let rendered = warnings
            .iter()
            .map(|w| w.to_string())
            .find(|m| m.contains("var/run/sock.fifo"))
            .unwrap_or_else(|| panic!("no warning mentioned the skipped FIFO: {warnings:?}"));
        assert!(
            rendered.contains("FIFO"),
            "the warning must name the kind: {rendered}"
        );
    }

    /// An archive of only allowlisted kinds must stay warning-free — the
    /// new emission must not fire on regular files or directories.
    #[test]
    fn allowlisted_kinds_emit_no_unsupported_entry_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar = dir.path().join("plain.tar");
        write_ustar(&tar, &[("adir/", b'5')], ("adir/file.txt", b"data"));
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).expect("create dest");

        let archive = LibarchiveArchive::open(&tar).expect("open fixture tar");
        let warnings = archive.extract_all(&dest, None).expect("extract");
        assert!(
            warnings.is_empty(),
            "files and directories are materialized, not skipped: {warnings:?}"
        );
        assert!(dest.join("adir").is_dir(), "directory entry extracted");
        assert_eq!(
            std::fs::read(dest.join("adir/file.txt")).expect("file entry extracted"),
            b"data"
        );
    }
}
