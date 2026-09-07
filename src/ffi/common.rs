//! Common utilities shared across FFI wrappers

use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::error::{ArchiveError, Result};
use crate::options::ProgressCallback;
use crate::security::{verify_crc32, verify_crc32_value};

/// Build a CString from a filesystem path while preserving raw bytes
/// on Unix (per AD 0064 — non-UTF-8 path policy Option A).
///
/// On Unix the path is taken as the raw `OsStr` bytes via
/// `OsStrExt::as_bytes`, so non-UTF-8 sequences round-trip into the
/// FFI layer. On Windows the lossy UTF-8 form is used; full Windows
/// wide-char support (`archive_read_open_filename_w` /
/// `RAROpenArchiveExW`) is deferred to the OI-0065-001 follow-up.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
pub(crate) fn path_to_cstring(path: &Path) -> std::result::Result<CString, std::ffi::NulError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        CString::new(path.as_os_str().as_bytes())
    }
    #[cfg(not(unix))]
    {
        CString::new(path.to_string_lossy().into_owned())
    }
}

/// [`path_to_cstring`] with the standard NUL-byte rejection mapping —
/// every FFI call site paired the conversion with the same
/// `invalid_path(..., "Contains null byte")` error by hand.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
pub(crate) fn path_to_cstring_checked(path: &Path) -> Result<CString> {
    path_to_cstring(path)
        .map_err(|_| ArchiveError::invalid_path(path.display().to_string(), "Contains null byte"))
}

// `TempDirGuard` removed in favour of `tempfile::TempDir`, which gives
// the same RAII drop semantics with a collision-free name and without
// risking unlink of pre-existing content on a name clash (R0069-0028 /
// R0069-0029).

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Reject symlinks at the single-file `add_*_from_path` write boundary.
///
/// `op` labels the originating writer method so the error message names
/// the actual caller (`"add_file_from_path"`, etc.). Uses
/// `symlink_metadata` so the symlink itself is detected — `metadata`
/// would follow the link and silently archive the target's bytes under
/// the link's name (R0069-0054 / R0069-0055).
///
/// **TOCTOU note (R0070-0021).** This helper performs only the
/// metadata check; if the writer subsequently opens `path` with
/// regular `File::open`, an attacker can swap the path to a symlink
/// between the check and the open. Callers that need a single
/// atomic "open and verify not a symlink" should use
/// [`open_file_no_follow_symlinks`] instead.
pub(crate) fn reject_symlink_path(path: &Path, op: &'static str) -> Result<()> {
    let symlink_meta = std::fs::symlink_metadata(path)
        .map_err(|e| ArchiveError::io("metadata", path.to_path_buf(), e))?;
    if symlink_meta.file_type().is_symlink() {
        return Err(ArchiveError::operation_blocked(
            op,
            format!(
                "Refusing to archive symlink '{}': symlinks are not supported for archive creation",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Open a file and re-verify after the open that the resolved path
/// is not a symlink (post-open defence in depth, R0070-0021).
///
/// This narrows — but does not fully eliminate — the TOCTOU window
/// between [`reject_symlink_path`] and the actual `File::open`. After
/// opening, we re-`symlink_metadata` the path and refuse to archive if
/// the entry has become a symlink. A determined attacker swapping the
/// path twice (regular → symlink → regular) between the open and the
/// stat can still slip through. A future revision (tracked under
/// OI-0070-002 / R0070-0021) should add a true atomic no-follow open
/// (`O_NOFOLLOW` on Unix, `FILE_FLAG_OPEN_REPARSE_POINT` on Windows).
/// Adding `libc` for that primitive sat outside the inline-fix scope
/// of this review pass.
pub(crate) fn open_file_no_follow_symlinks(
    path: &Path,
    op: &'static str,
) -> Result<(File, std::fs::Metadata)> {
    reject_symlink_path(path, op)?;
    let file = File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;
    let post_meta = std::fs::symlink_metadata(path)
        .map_err(|e| ArchiveError::io("metadata", path.to_path_buf(), e))?;
    if post_meta.file_type().is_symlink() {
        return Err(ArchiveError::operation_blocked(
            op,
            format!(
                "Refusing to archive symlink '{}': symlinks are not supported for archive creation",
                path.display()
            ),
        ));
    }
    let metadata = file
        .metadata()
        .map_err(|e| ArchiveError::io("metadata", path.to_path_buf(), e))?;
    Ok((file, metadata))
}

/// Normalize archive entry path by converting backslashes to forward slashes
#[inline]
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Unix file-type probe on a raw mode word: true when the S_IFMT bits
/// carry S_IFLNK. Shared by the ZIP and 7z backends, whose link
/// policy must classify identically at listing time and in every
/// extract path (R0069-0018 / R0075-0049).
// Only the 7z backend consults this today; the other backends get the
// entry kind from their own metadata. Not dead, just uncalled when 7z is
// compiled out.
#[cfg_attr(not(feature = "sevenzip"), allow(dead_code))]
#[inline]
pub(crate) fn unix_mode_is_symlink(mode: u32) -> bool {
    (mode & 0xF000) == 0xA000
}

/// Atomic output file backed by `tempfile::NamedTempFile`.
///
/// Writes go to a randomly-named sibling tempfile in the destination's
/// parent directory. [`commit`](Self::commit) takes the value by move and
/// atomically renames it into place; if the value is dropped before commit,
/// `NamedTempFile`'s own `Drop` removes the temp file automatically. The
/// `inner` field is therefore plain `NamedTempFile` (no `Option`) — the
/// "still has a tempfile" invariant is encoded by the type system rather
/// than checked with `expect`.
pub(crate) struct AtomicOutputFile {
    inner: NamedTempFile,
    final_path: PathBuf,
    overwrite: bool,
}

impl AtomicOutputFile {
    /// Open a tempfile in `path`'s parent directory; commit will move it to `path`.
    ///
    /// `op` labels the originating operation so error messages (noclobber
    /// blocks, parent-dir I/O failures) reference the caller's intent
    /// (e.g. `"extract"`, `"commit_changes"`). Hardcoding `"extract"` at
    /// this layer mis-labeled every non-extraction caller.
    ///
    /// When `overwrite` is false the existence check is racy by design: it
    /// fails fast for the common case, and `commit()`'s `persist_noclobber`
    /// closes the actual race window via `link()`+`unlink()` (Unix) /
    /// `CreateFileW` exclusive flags (Windows).
    pub fn create(path: &Path, overwrite: bool, op: &'static str) -> Result<Self> {
        if !overwrite && path.exists() {
            return Err(ArchiveError::operation_blocked(
                op,
                format!("Destination file already exists: {}", path.display()),
            ));
        }

        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        // Honor the supplied `op` for the I/O error too — every
        // tempfile-creation failure used to surface as the literal
        // operation `"create"` regardless of the caller's intent
        // (R0070-0074).
        let inner = NamedTempFile::new_in(parent)
            .map_err(|e| ArchiveError::io(op, parent.to_path_buf(), e))?;

        Ok(Self {
            inner,
            final_path: path.to_path_buf(),
            overwrite,
        })
    }

    /// Mutable access to the underlying file for writing.
    pub fn file_mut(&mut self) -> &mut File {
        self.inner.as_file_mut()
    }

    /// Flush and atomically install the tempfile at the destination path.
    ///
    /// Consumes `self` — the `NamedTempFile`'s Drop runs only on the
    /// success branch (where `mem::forget` neutralises the
    /// already-renamed temp path) or implicitly on the failure branches.
    pub fn commit(self) -> Result<()> {
        let Self {
            inner,
            final_path,
            overwrite,
        } = self;

        // R0080-0071: `sync_all` (fsync), not `sync_data` (fdatasync).
        // `apply_preserved_metadata` stamps permissions/mtime onto this
        // handle before commit, and fdatasync does not guarantee inode
        // metadata is durably flushed; the extra metadata sync is marginal
        // against the payload flush.
        inner
            .as_file()
            .sync_all()
            .map_err(|e| ArchiveError::io("flush", final_path.clone(), e))?;

        if overwrite {
            // Atomic replace: `std::fs::rename` on Unix already overwrites
            // atomically; on Windows we route through `MoveFileExW` with
            // `MOVEFILE_REPLACE_EXISTING` so the destination is replaced in
            // place rather than deleted-then-recreated. The previous
            // delete-then-persist sequence left a window where a `persist`
            // failure would lose the original.
            let temp_path = inner.into_temp_path();
            rename_with_overwrite(&temp_path, &final_path)?;
            // The tempfile is now at `final_path`; suppress TempPath's
            // unlink-on-drop (which would fail with NotFound anyway, but
            // doing so explicitly keeps the intent clear).
            std::mem::forget(temp_path);
        } else {
            inner
                .persist_noclobber(&final_path)
                .map_err(|e| ArchiveError::io("rename", final_path.clone(), e.error))?;
        }
        // R0075-0037: best-effort parent-directory sync so the new
        // directory entry is durably observable after a crash. Errors
        // are non-fatal — the file is already in place; the sync is a
        // durability hint, not a correctness gate — but they surface as
        // a warning instead of being silently swallowed (R0076-0013).
        if let Err(e) = sync_parent_dir(&final_path) {
            eprintln!(
                "unified-archive: parent-directory fsync after installing `{}` failed: {e}. \
                 The file is in place but its directory entry may not be durable across a crash.",
                final_path.display()
            );
        }
        Ok(())
    }
}

/// Apply preserved entry metadata (Unix permission bits, modification
/// time) to a staged output file (R0079-0019).
///
/// Call after the payload is fully written and before
/// [`AtomicOutputFile::commit`]: the rename that installs the staged
/// file carries both attributes to the final path, while writing
/// payload bytes *after* a timestamp set would refresh the mtime.
/// Operating on the open handle (`fchmod` / `futimens` semantics)
/// rather than the destination path means a symlink can never be
/// followed.
///
/// `unix_mode` is masked to the permission bits (lower 12, matching
/// the `ArchiveEntry::permissions` contract) and is a no-op on
/// non-Unix platforms. Pass `None` for whichever field the caller's
/// `preserve_*` flag disables or the archive does not carry.
pub(crate) fn apply_preserved_metadata(
    file: &File,
    unix_mode: Option<u32>,
    modified: Option<std::time::SystemTime>,
    error_path: &Path,
) -> Result<()> {
    #[cfg(unix)]
    if let Some(mode) = unix_mode {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(mode & 0o7777))
            .map_err(|e| ArchiveError::io("set_permissions", error_path.to_path_buf(), e))?;
    }
    #[cfg(not(unix))]
    let _ = unix_mode;
    if let Some(mtime) = modified {
        file.set_modified(mtime)
            .map_err(|e| ArchiveError::io("set_times", error_path.to_path_buf(), e))?;
    }
    Ok(())
}

/// Best-effort `fsync` of the parent directory of `path` so a freshly
/// renamed entry is durably observable across crash recovery.
///
/// On Unix, opening the parent directory and calling `sync_all` is the
/// standard durability pattern. On Windows there is no direct equivalent; `FlushFileBuffers`
/// on a directory handle returns `ERROR_INVALID_HANDLE`, so the call is
/// a no-op.
pub(crate) fn sync_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    #[cfg(unix)]
    {
        let dir = std::fs::File::open(parent)
            .map_err(|e| ArchiveError::io("open_parent", parent.to_path_buf(), e))?;
        dir.sync_all()
            .map_err(|e| ArchiveError::io("fsync_parent", parent.to_path_buf(), e))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

/// Encode `path` as a NUL-terminated UTF-16 buffer for the wide Win32
/// APIs, rejecting a path that already carries a zero code unit.
///
/// R0001-0073: `MoveFileExW` and every other wide API stop at the first
/// NUL, so an interior zero makes the call operate on a *prefix* of the
/// path — a different file than the Rust `Path` denotes — instead of
/// failing. Mirrors the NUL rejection `path_to_cstring_checked` already
/// performs at the Unix FFI boundary.
#[cfg(windows)]
fn encode_wide_nul_terminated(path: &Path) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains an interior NUL",
        ));
    }
    wide.push(0);
    Ok(wide)
}

/// Cross-platform atomic rename that replaces an existing destination.
///
/// On Unix `std::fs::rename` already does this atomically (`renameat`).
/// On Windows the standard library's rename refuses to overwrite, so this
/// shells out to `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`. Used by
/// `AtomicOutputFile::commit` and modify-mode `commit_changes` so both
/// install paths use the same swap primitive.
///
/// The Windows arm rejects a path carrying an interior NUL code unit
/// with `InvalidPath` rather than letting the wide API truncate it
/// (R0001-0073); see `encode_wide_nul_terminated`.
#[cfg(not(windows))]
pub(crate) fn rename_with_overwrite(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| {
        let combined = std::io::Error::new(
            e.kind(),
            format!("rename {} -> {}: {}", from.display(), to.display(), e),
        );
        ArchiveError::io("rename", from, combined)
    })
}

#[cfg(windows)]
pub(crate) fn rename_with_overwrite(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    // Convert paths to wide strings (null-terminated UTF-16). A path
    // carrying an interior NUL is rejected up front (R0001-0073) with
    // the same wording the Unix CString boundary uses, rather than
    // silently truncating at the zero code unit.
    let encode = |p: &std::path::Path| -> Result<Vec<u16>> {
        encode_wide_nul_terminated(p)
            .map_err(|_| ArchiveError::invalid_path(p.display().to_string(), "Contains null byte"))
    };
    let from_wide = encode(from)?;
    let to_wide = encode(to)?;

    // MOVEFILE_REPLACE_EXISTING = 0x1, MOVEFILE_WRITE_THROUGH = 0x8.
    // WRITE_THROUGH is only meaningful across volumes, but is harmless here
    // and matches the durability guarantees Unix `rename` provides.
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    // Link to kernel32.dll MoveFileExW
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;

        fn GetLastError() -> u32;
    }

    let result = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        let error_code = unsafe { GetLastError() };
        return Err(ArchiveError::io(
            "rename",
            from,
            std::io::Error::from_raw_os_error(error_code as i32),
        ));
    }

    Ok(())
}

/// Atomic noclobber install: move `from` to `to`, failing with
/// `AlreadyExists` when `to` exists — including a destination that
/// appeared *after* a caller's racy pre-check, which
/// [`rename_with_overwrite`] would silently replace (R0079-0016).
///
/// Unix uses `link(2)` + best-effort unlink of the source — the same
/// primitive `tempfile`'s `persist_noclobber` relies on; a failed
/// unlink only leaves a stray staging file behind, never a wrong
/// destination. Windows routes through `MoveFileExW` *without*
/// `MOVEFILE_REPLACE_EXISTING`, so a pre-existing destination fails the
/// move with `ERROR_ALREADY_EXISTS` (surfaced as `AlreadyExists`) rather
/// than being clobbered — the previous placeholder-then-rename fallback
/// created an empty destination and then failed to rename over it
/// (R0080-0004). Any other platform falls back to the Unix `link`+unlink
/// pair. A Windows path carrying an interior NUL code unit fails with
/// `InvalidInput` instead of being truncated by the wide API
/// (R0001-0073).
///
/// Returns `std::io::Result` so callers can map `AlreadyExists` to
/// their operation-specific structured error.
#[cfg(unix)]
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
pub(crate) fn rename_noclobber(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::hard_link(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

#[cfg(windows)]
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
pub(crate) fn rename_noclobber(from: &Path, to: &Path) -> std::io::Result<()> {
    // R0001-0073: an interior NUL would shorten the path handed to
    // `MoveFileExW`, so it surfaces as `InvalidInput` instead of moving
    // some other file. Callers only special-case `AlreadyExists`, so the
    // new kind flows through their generic mapping.
    let from_wide = encode_wide_nul_terminated(from)?;
    let to_wide = encode_wide_nul_terminated(to)?;

    // Deliberately no `MOVEFILE_REPLACE_EXISTING`: an existing destination
    // must fail the move (surfacing `AlreadyExists`) rather than being
    // clobbered. `MOVEFILE_WRITE_THROUGH` matches `rename_with_overwrite`.
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    // Link to kernel32.dll MoveFileExW
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;

        fn GetLastError() -> u32;
    }

    let result =
        unsafe { MoveFileExW(from_wide.as_ptr(), to_wide.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if result == 0 {
        let code = unsafe { GetLastError() };
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
pub(crate) fn rename_noclobber(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::hard_link(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

#[cfg_attr(not(feature = "read"), allow(dead_code))]
/// Convert year/month/day/hour/minute/second to SystemTime
///
/// Returns `None` for pre-1970 dates, for years past a sane four-digit
/// Gregorian ceiling (9999), and for any intermediate arithmetic that
/// would overflow (R0081-0040).
pub(crate) fn ymd_hms_to_system_time(
    year: u64,
    month: u64,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};

    // Bound the year at both ends: pre-epoch is unrepresentable here and
    // a garbage far-future year must not be allowed to drive the
    // day/second arithmetic below into overflow (R0081-0040).
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;

    // Reject impossible calendar dates (e.g., April 31, Feb 30, non-leap Feb 29).
    const DAYS_IN_MONTH: [u64; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut max_day = DAYS_IN_MONTH[month as usize];
    if month == 2 && is_leap {
        max_day = 29;
    }
    if day > max_day {
        return None;
    }

    // Cumulative days before each month (non-leap year)
    const DAYS_BEFORE_MONTH: [u64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    // Days from 1970 to start of year, using the correct Gregorian leap-year formula
    fn leap_days_before(year: u64) -> u64 {
        let y = year - 1;
        let from = 1970u64 - 1;
        (y / 4 - y / 100 + y / 400) - (from / 4 - from / 100 + from / 400)
    }
    // Checked arithmetic throughout. The 9999 ceiling keeps realistic
    // inputs far clear of these bounds, so the guards never fire in
    // practice; they exist only so a pathological input yields `None`
    // rather than a panic (R0081-0040).
    let mut days = (year - 1970)
        .checked_mul(365)?
        .checked_add(leap_days_before(year))?
        .checked_add(DAYS_BEFORE_MONTH[month as usize])?
        .checked_add(day - 1)?;

    if is_leap && month > 2 {
        days = days.checked_add(1)?;
    }

    let total_seconds = days
        .checked_mul(86400)?
        .checked_add(hour.checked_mul(3600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)?;
    UNIX_EPOCH.checked_add(Duration::from_secs(total_seconds))
}

/// Convert Unix seconds (e.g. from ZIP 0x5455 ExtendedTimestamp or
/// libarchive timestamps) into `SystemTime`. Negative (pre-epoch)
/// values produce `None`: `SystemTime` itself *can* represent instants
/// before 1970, but this crate drops pre-epoch timestamps on read as a
/// matter of policy (R0076-0043), so they never reach entry metadata.
pub(crate) fn unix_seconds_to_system_time(seconds: i64) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};
    if seconds < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(seconds as u64))
}

/// Convert SystemTime to zip::DateTime for ZIP archive entries.
///
/// Returns None if the time cannot be represented in ZIP's DOS date format
/// (valid range: 1980-01-01 through 2107-12-31).
#[cfg_attr(
    not(all(any(feature = "create", feature = "modify"), feature = "zip-write")),
    allow(dead_code)
)]
pub(crate) fn system_time_to_zip_datetime(time: std::time::SystemTime) -> Option<zip::DateTime> {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    let secs = duration.as_secs();

    // Break epoch seconds into calendar components
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let hour = (day_secs / 3600) as u8;
    let minute = ((day_secs % 3600) / 60) as u8;
    let second = (day_secs % 60) as u8;

    // Convert days since epoch to year/month/day
    // Algorithm: civil_from_days (Howard Hinnant)
    let z = days as i64 + 719468; // shift epoch from 1970-01-01 to 0000-03-01
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    // Range-check the wide i64 year *before* narrowing to u16.
    // `from_date_and_time` enforces 1980..=2107, but casting a
    // far-future or far-past year through `as u16` first would wrap it
    // into the valid window and serialize a wrong date (R0081-0042).
    let year_wide = if month <= 2 { y + 1 } else { y };
    if !(1980..=2107).contains(&year_wide) {
        return None;
    }
    let year = year_wide as u16;

    zip::DateTime::from_date_and_time(year, month, day, hour, minute, second).ok()
}

/// Accepted CRC-mismatch vocabulary for decoder-side entry read errors
/// (R0081-0043). zip 2.4 validates entry CRCs inside its
/// reader and reports a mismatch as a bare `io::Error` with no typed
/// variant, so classification is substring-based. The crate emits the
/// exact spelling `"Invalid checksum"` today; the regression test pins
/// that wording so an upstream change trips a test instead of silently
/// downgrading corruption to a generic I/O error. Mirrors the
/// libarchive R0076-0046 approach.
const CHECKSUM_FAILURE_MARKERS: &[&str] = &["Invalid checksum"];

/// True when a decoder-side read error message describes a CRC/checksum
/// verification failure (payload corruption) rather than some other
/// read failure. Every read path that wants to distinguish corruption
/// from a generic I/O error routes through here instead of matching the
/// string locally (R0081-0043).
fn is_checksum_failure_message(message: &str) -> bool {
    CHECKSUM_FAILURE_MARKERS.iter().any(|m| message.contains(m))
}

/// Map a decoder-side entry read failure to the library's typed error.
///
/// A CRC mismatch surfaces as the same `Corruption` the explicit
/// `security::verify_crc32` check produces, instead of a generic `Io`
/// (R0079-0046); classification goes through
/// [`is_checksum_failure_message`] so the upstream marker set stays
/// centralized and tested (R0081-0043).
pub(crate) fn map_entry_read_error(e: std::io::Error, entry_path: &Path) -> ArchiveError {
    if is_checksum_failure_message(&e.to_string()) {
        ArchiveError::Corruption {
            path: entry_path.display().to_string(),
            details: "CRC32 mismatch reported by the entry decoder".to_string(),
        }
    } else {
        ArchiveError::io("read", entry_path.to_path_buf(), e)
    }
}

/// Read a declared-size entry payload into memory under the shared
/// bounds contract the ZIP/7z memory paths previously re-implemented
/// per backend (R0070-0015 / R0075-0051):
///
/// - sizes that do not fit in `usize`, and allocation failures, surface
///   as `OperationBlocked` (via `try_reserve`) instead of aborting;
/// - at most `declared + 1` bytes are read, so an over-producing
///   decoder is detected as `Corruption` rather than silently filling
///   RAM;
/// - `require_exact` callers (7z, whose TOC size is authoritative and
///   whose decoder can EOF short) additionally treat a short read as
///   `Corruption`;
/// - `expected_crc` is verified through the canonical
///   `security::verify_crc32`, so mismatch diagnostics keep one shape
///   across backends.
///
/// Read errors route through [`map_entry_read_error`] so decoder-side
/// CRC failures keep the R0079-0046 `Corruption` mapping.
pub(crate) fn read_entry_to_memory_bounded<R: Read + ?Sized>(
    reader: &mut R,
    declared_size: u64,
    entry_path: &str,
    op: &'static str,
    expected_crc: Option<u32>,
    require_exact: bool,
) -> Result<Vec<u8>> {
    let size = usize::try_from(declared_size).map_err(|_| ArchiveError::OperationBlocked {
        operation: op.to_string(),
        reason: format!(
            "Entry '{}' is too large to buffer in memory: {} bytes",
            entry_path, declared_size
        ),
    })?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve(size)
        .map_err(|_| ArchiveError::OperationBlocked {
            operation: op.to_string(),
            reason: format!(
                "Unable to allocate {} bytes for entry '{}'",
                declared_size, entry_path
            ),
        })?;

    let cap = declared_size.saturating_add(1);
    (&mut *reader)
        .take(cap)
        .read_to_end(&mut buffer)
        .map_err(|e| map_entry_read_error(e, Path::new(entry_path)))?;
    if buffer.len() as u64 > declared_size {
        return Err(ArchiveError::corruption(
            entry_path,
            format!(
                "{}: backend produced more than the declared {} bytes",
                op, declared_size
            ),
        ));
    }
    // Skip the short-read check when the declared size is 0 — both
    // valid empty entries and zero-declared streaming entries
    // terminate at EOF with an empty buffer.
    if require_exact && declared_size > 0 && (buffer.len() as u64) < declared_size {
        return Err(ArchiveError::corruption(
            entry_path,
            format!(
                "{}: backend returned {} bytes, expected {}",
                op,
                buffer.len(),
                declared_size
            ),
        ));
    }

    verify_crc32(&buffer, expected_crc, entry_path)?;
    Ok(buffer)
}

/// [`read_entry_to_memory_bounded`] with an additional caller-supplied
/// per-entry byte cap, honoured *before* any buffering: an entry whose
/// declared size already exceeds `max_bytes` is rejected up front, so
/// the reserve never over-allocates against the cap and no payload is
/// decoded just to be thrown away by a post-hoc check
/// (R0076-0009 / R0076-0062). `None` behaves exactly like the plain
/// bounded read. Because the bounded read rejects decoders that
/// over-produce past `declared_size`, `declared_size <= cap` implies
/// the returned buffer also satisfies the cap.
pub(crate) fn read_entry_to_memory_capped<R: Read + ?Sized>(
    reader: &mut R,
    declared_size: u64,
    max_bytes: Option<u64>,
    entry_path: &str,
    op: &'static str,
    expected_crc: Option<u32>,
    require_exact: bool,
) -> Result<Vec<u8>> {
    if let Some(cap) = max_bytes {
        if declared_size > cap {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "Entry '{}' declares {} bytes; exceeds the configured per-entry limit of {} bytes",
                    entry_path, declared_size, cap
                ),
            });
        }
    }
    read_entry_to_memory_bounded(
        reader,
        declared_size,
        entry_path,
        op,
        expected_crc,
        require_exact,
    )
}

/// Loop-invariant spec for [`write_entry_atomically`].
pub(crate) struct StagedEntryWrite<'a> {
    /// Final destination path (staging sibling + atomic install).
    pub output_path: &'a Path,
    /// Archive-internal path used in error diagnostics.
    pub entry_path: &'a str,
    /// Public operation label (`extract_all`, `extract_file`, …).
    pub op: &'static str,
    pub overwrite: bool,
    /// Optional expected CRC32, verified by the bounded copy.
    pub expected_crc: Option<u32>,
    /// Declared payload size. Authoritative for ZIP central directories
    /// and 7z TOCs, so the copy runs under `CopyByteBudget::Exact` —
    /// over- and under-decode both surface before commit (R0072-0005 /
    /// R0073-0005).
    pub declared_size: u64,
    /// Unix permission bits to preserve (`None` = leave staging default).
    pub unix_mode: Option<u32>,
    /// Modification time to preserve.
    pub modified: Option<std::time::SystemTime>,
}

/// The staged-write pipeline every ZIP/7z extract path previously
/// re-strung by hand: stage via [`AtomicOutputFile`], copy with the
/// exact-size budget + optional CRC, apply preserved metadata to the
/// staged handle (R0079-0019), then commit atomically. `on_chunk`
/// threads the per-entry cancellation hook (R0073-0006). Returns the
/// decoded byte count.
pub(crate) fn write_entry_atomically<R: Read + ?Sized>(
    reader: &mut R,
    spec: StagedEntryWrite<'_>,
    on_chunk: Option<&mut dyn FnMut(u64) -> ControlFlow<()>>,
) -> Result<u64> {
    let mut output_file = AtomicOutputFile::create(spec.output_path, spec.overwrite, spec.op)?;
    let bytes_written = copy_with_optional_crc_bounded(
        reader,
        output_file.file_mut(),
        on_chunk,
        CopyContext {
            entry_path: spec.entry_path,
            write_error_path: spec.output_path,
            op: spec.op,
            expected_crc: spec.expected_crc,
            budget: CopyByteBudget::Exact(spec.declared_size),
        },
    )?;
    apply_preserved_metadata(
        output_file.file_mut(),
        spec.unix_mode,
        spec.modified,
        spec.output_path,
    )?;
    output_file.commit()?;
    Ok(bytes_written)
}

/// Poll the extraction progress callback between entries (and at the
/// final 100% notification), mapping a `ControlFlow::Break` vote to
/// the typed [`ArchiveError::Cancelled`] every backend loop previously
/// hand-rolled as a `Format` error (R0070-0035..0038,
/// R0076-0012 / R0076-0064 / R0076-0065). `operation` labels the
/// public API the cancellation aborted (e.g. `ops::EXTRACT_ALL`).
pub(crate) fn check_extraction_cancelled(
    progress: &mut Option<&mut Box<dyn ProgressCallback>>,
    bytes_processed: u64,
    total_bytes: u64,
    operation: &'static str,
) -> Result<()> {
    if let Some(cb) = progress.as_mut() {
        if let ControlFlow::Break(()) = cb.on_progress(bytes_processed, Some(total_bytes)) {
            return Err(ArchiveError::Cancelled { operation });
        }
    }
    Ok(())
}

/// Build the per-entry cancellation hook handed to
/// [`copy_with_optional_crc_bounded`]: reports `bytes_before +
/// bytes_in_entry` against the loop's precomputed total so a `Break`
/// from the user's callback aborts mid-entry instead of waiting for
/// the next entry boundary (R0073-0006).
pub(crate) fn entry_cancel_hook<'a>(
    progress: &'a mut Option<&mut Box<dyn ProgressCallback>>,
    bytes_before: u64,
    total_bytes: u64,
) -> impl FnMut(u64) -> ControlFlow<()> + 'a {
    move |bytes_in_entry| match progress.as_mut() {
        Some(cb) => cb.on_progress(
            bytes_before.saturating_add(bytes_in_entry),
            Some(total_bytes),
        ),
        None => ControlFlow::Continue(()),
    }
}

#[cfg_attr(not(feature = "integrity"), allow(dead_code))]
/// Compute CRC32 by streaming through a reader (no full buffering)
pub(crate) fn compute_crc32_reader<R: Read>(reader: &mut R, error_path: &Path) -> Result<u32> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| map_entry_read_error(e, error_path))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize())
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Map a `walkdir::Error` from a recursive-add traversal onto the
/// crate's `Io` error, preferring the walker's own path and I/O error
/// when present. Shared by the facade namespace pre-walk and both
/// creation backends' emission walks.
pub(crate) fn walkdir_io_error(
    e: &walkdir::Error,
    fallback_path: &Path,
    op: &'static str,
) -> ArchiveError {
    let path = e
        .path()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| fallback_path.to_path_buf());
    // Preserve the OS error code when the walker carried one: rebuilding
    // via `io::Error::new(kind, text)` drops `raw_os_error` and the
    // source chain, weakening diagnostics and any errno-based policy
    // decision downstream. Fall back to a kind+message clone only when
    // there is no raw code to carry (R0081-0044). Path context is kept
    // separately via `ArchiveError::io`.
    let io_error = match e.io_error() {
        Some(err) => err
            .raw_os_error()
            .map(std::io::Error::from_raw_os_error)
            .unwrap_or_else(|| std::io::Error::new(err.kind(), err.to_string())),
        None => std::io::Error::other(e.to_string()),
    };
    ArchiveError::io(op, path, io_error)
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Entry kind surfaced by [`walk_directory_tree`]. `Special` covers
/// everything that is neither a regular file nor a directory — the
/// walker never follows links, so symlinks land here and each caller
/// applies its own link policy.
pub(crate) enum DirWalkKind {
    File,
    Dir,
    Special { is_symlink: bool },
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// One filesystem entry surfaced by [`walk_directory_tree`], with its
/// parent-rooted relative archive path (raw, un-normalized form —
/// callers apply their own separator-normalization policy). The walk
/// refuses a non-UTF-8 source component rather than substituting
/// U+FFFD, so this is the source's own name and not a rendering of it.
pub(crate) struct DirWalkEntry<'a> {
    pub fs_path: &'a Path,
    pub archive_path: String,
    pub kind: DirWalkKind,
    /// True for the walked root directory itself.
    pub is_root: bool,
    /// For directories: true when the pre-order stream shows no
    /// children (leaf directory).
    pub is_leaf_dir: bool,
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Shared recursive-add traversal, previously re-implemented by both
/// creation backends and the facade namespace pre-walk (R0075-0006):
/// rejects a symlinked root (R0070-0048), walks with
/// `follow_links(false)`, strips the parent-rooted prefix so the source
/// directory's own name survives into archive paths (matching `tar`,
/// `zip -r`, `7z a -r`), and detects leaf directories from the
/// pre-order stream so no extra `read_dir` runs per directory. Policy
/// decisions — separator normalization, link/special handling,
/// leaf-only vs all-dir emission — stay with the caller's `emit`
/// closure. `op` labels walk I/O errors.
pub(crate) fn walk_directory_tree(
    dir_path: &Path,
    op: &'static str,
    mut emit: impl FnMut(DirWalkEntry<'_>) -> Result<()>,
) -> Result<()> {
    // `is_dir()` would follow a symlinked root silently, giving the
    // root a laxer policy than its children; `symlink_metadata` keeps
    // the link itself in scope (R0070-0048).
    let root_meta = std::fs::symlink_metadata(dir_path)
        .map_err(|e| ArchiveError::io("metadata", dir_path.to_path_buf(), e))?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err(ArchiveError::invalid_path(
            dir_path.to_string_lossy().as_ref(),
            "add_directory_recursive: root must be a real directory \
             (not a symlink to a directory)",
        ));
    }

    let (base_path, synthetic_root) = archive_base_for_root(dir_path);

    // Peekable pre-order stream: a directory's children immediately
    // follow it at greater depth, so leaf detection needs no extra
    // syscall per directory. Sort each directory's children by file
    // name so identical trees produce identical archive order
    // regardless of the filesystem's native readdir order (R0081-0045);
    // the sort is per-directory and preserves that pre-order invariant,
    // so the peek-based leaf detection stays correct and siblings stay
    // grouped under their parent.
    let mut entries = walkdir::WalkDir::new(dir_path)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .peekable();
    while let Some(entry_result) = entries.next() {
        let entry = entry_result.map_err(|e| walkdir_io_error(&e, dir_path, op))?;
        let fs_path = entry.path();
        let relative = fs_path.strip_prefix(base_path).unwrap_or(fs_path);
        reject_non_utf8_relative(fs_path, relative)?;
        let archive_path = compose_archive_path(relative, synthetic_root.as_deref());
        // Defensive: with a parent-rooted base every entry has a
        // non-empty relative form, and a synthesised root supplies one
        // where the parent-rooted rule cannot. Keep the guard so a
        // degenerate walker setup can't slip an empty path through.
        if archive_path.is_empty() {
            continue;
        }
        let file_type = entry.file_type();
        let kind = if file_type.is_dir() {
            DirWalkKind::Dir
        } else if file_type.is_file() {
            DirWalkKind::File
        } else {
            DirWalkKind::Special {
                is_symlink: file_type.is_symlink(),
            }
        };
        let is_leaf_dir = file_type.is_dir()
            && !entries
                .peek()
                .and_then(|r| r.as_ref().ok())
                .map(|next| next.depth() > entry.depth())
                .unwrap_or(false);
        emit(DirWalkEntry {
            fs_path,
            archive_path,
            kind,
            is_root: fs_path == dir_path,
            is_leaf_dir,
        })?;
    }
    Ok(())
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Refuse a walked entry whose archive-relative path is not valid
/// UTF-8, rather than rendering it lossily (AD 0064 write-side ruling).
///
/// The single-file add has rejected a non-UTF-8 source name since
/// AD 0064; the recursive add reached [`compose_archive_path`] and
/// substituted `U+FFFD`, storing the entry under a name that is not the
/// source's. This is the one point where the two disagreed. A recursive
/// add has no per-entry rename hook, so the whole call fails — the
/// accepted cost of not growing a byte-keyed public write surface.
///
/// `fs_path` is only used to spell the diagnostic; `relative` is what
/// decides. Checking the relative path covers the source root's own
/// name too, since that name becomes the top-level archive prefix.
fn reject_non_utf8_relative(fs_path: &Path, relative: &Path) -> Result<()> {
    if relative.to_str().is_some() {
        return Ok(());
    }
    Err(ArchiveError::invalid_path(
        fs_path.to_string_lossy().as_ref(),
        "source path component is not valid UTF-8; rename or exclude it, \
         or add the file individually with add_file_from_path_as (AD 0064)",
    ))
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Choose the base every archive path is made relative to, plus a
/// synthesised name for the source root when it needs one (OI-0080-005
/// residual / ticgit 330f38).
///
/// The ordinary rule is "relative to the source's parent", which is what
/// makes `add_directory_recursive("/home/u/project")` archive
/// `project/`, `project/src/…` — the source directory's own name becomes
/// the top-level prefix.
///
/// A filesystem root has no parent, and that used to break the rule
/// silently. `Path::parent()` returns `None` for `/`, so the base fell
/// back to the root itself: `"/".strip_prefix("/")` is `""`, the empty
/// archive path hit the guard above, and **the root entry was dropped
/// without a word** — while its children were archived unprefixed, as
/// `etc/`, `usr/`, … So the source root's own mtime and mode were lost,
/// and the one case where the top-level prefix disappears was the case
/// nobody could see.
///
/// A root therefore gets a synthesised name, and that name behaves
/// exactly like any other source directory's: it is the prefix for the
/// whole tree. Archiving `/` yields `rootfs/`, `rootfs/etc/`, … This is
/// what makes the result *coherent* — naming the root entry without
/// prefixing its children would emit an empty `rootfs/` sitting beside
/// `etc/`, which is worse than dropping it. Nesting also makes a
/// collision impossible by construction: every entry is under the
/// synthesised name, so it cannot clash with a real child such as
/// `/rootfs`.
///
/// The name is derived from the root's own spelling, so it is stable
/// across runs and distinguishes volumes: `C:\` gives `C`, `\\srv\share\`
/// gives `srv_share`. Unix `/` has no alphanumerics to draw on and falls
/// back to `rootfs`.
fn archive_base_for_root(dir_path: &Path) -> (&Path, Option<String>) {
    match dir_path.parent() {
        Some(parent) => (parent, None),
        None => (dir_path, Some(synthesise_root_name(dir_path))),
    }
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Fallback name for a filesystem root whose spelling carries no
/// alphanumeric character to name it by — Unix `/`.
const DEFAULT_ROOT_ARCHIVE_NAME: &str = "rootfs";

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Derive a stable, filesystem-safe archive name from a root path's own
/// textual form.
///
/// Alphanumeric runs are kept and joined with `_`; everything else
/// (separators, colons, spaces) is dropped. Deterministic, so the same
/// root always produces the same name — an archive of `C:\` made today
/// and one made next year agree on their top-level entry.
fn synthesise_root_name(root: &Path) -> String {
    let spelling = root.to_string_lossy();
    let name = spelling
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if name.is_empty() {
        DEFAULT_ROOT_ARCHIVE_NAME.to_string()
    } else {
        name
    }
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Join a walked entry's parent-relative path to the source root's
/// synthesised name, if it has one.
///
/// With no synthesised name this is the historical behaviour: the
/// relative path *is* the archive path. With one — a filesystem root —
/// the root itself becomes that name (its relative form is empty) and
/// every descendant is nested beneath it.
fn compose_archive_path(relative: &Path, synthetic_root: Option<&str>) -> String {
    match synthetic_root {
        None => relative.to_string_lossy().into_owned(),
        Some(name) if relative.as_os_str().is_empty() => name.to_string(),
        Some(name) => format!("{name}/{}", relative.to_string_lossy()),
    }
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Ensure a directory path has a trailing slash
pub(crate) fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{}/", path)
    }
}

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// Notify a creation progress callback after writing `additional` bytes.
///
/// Shared by ZipWriter and LibarchiveArchive creation backends.
/// Returns [`ArchiveError::Cancelled`] (operation `ops::CREATE`) when
/// the callback signals cancellation via `ControlFlow::Break`
/// (R0076-0012).
///
/// **Cancellation is best-effort post-write (R0072-0007).** The
/// callback fires *after* the chunk reaches the writer, so a `Break`
/// returned here cannot un-write data already handed off. Specifically:
///
/// - Per-chunk streaming sources see Break after each 64 KiB chunk —
///   the in-flight entry is left partial and the writer is poisoned
///   (ZIP) or rejected (libarchive, R0072-0008). `entries_written` is
///   not incremented.
/// - Buffered byte sources (`add_file_from_data`) write the whole
///   payload in one call before the callback can vote — those entries
///   are fully present in the archive when Break fires.
///
/// `finish()` / `Drop` still drain the writer to a structurally-valid
/// archive containing every entry that was successfully written before
/// the Break. Treat the cancellation surface as "stop *future* work,"
/// not "roll back already-written work" — callers that need rollback
/// should write to a temp path and `rename` only on success.
pub(crate) fn notify_creation_progress(
    progress: &mut Option<Box<dyn ProgressCallback>>,
    bytes_written: &mut u64,
    additional: u64,
) -> Result<()> {
    *bytes_written = bytes_written.saturating_add(additional);
    if let Some(cb) = progress.as_mut() {
        if let ControlFlow::Break(()) = cb.on_progress(*bytes_written, None) {
            return Err(ArchiveError::Cancelled {
                operation: crate::error::ops::CREATE,
            });
        }
    }
    Ok(())
}

/// Copy data from `reader` to `writer` and call `notify` after each
/// chunk so creation progress and `ControlFlow::Break` cancellation
/// fire during the entry, not only after the whole entry is on disk
/// (R0071-0007). The 64 KiB chunk size matches the libarchive write-side
/// buffer so cross-backend progress cadence is consistent.
#[cfg_attr(
    not(all(any(feature = "create", feature = "modify"), feature = "zip-write")),
    allow(dead_code)
)]
pub(crate) fn copy_with_progress<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    write_error_path: &Path,
    notify: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<u64> {
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| ArchiveError::io("read", write_error_path.to_path_buf(), e))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buffer[..n])
            .map_err(|e| ArchiveError::io("write", write_error_path.to_path_buf(), e))?;
        // R0076-0010: saturating_add — progress accounting must not wrap on
        // pathologically long readers; the surrounding progress callback
        // already uses saturating arithmetic for entry totals.
        total = total.saturating_add(n as u64);
        notify(n as u64)?;
    }
    Ok(total)
}

/// Byte-size contract for [`copy_with_optional_crc_bounded`].
///
/// Lets call sites distinguish "this is an upper bound; short reads are
/// fine" (`Cap`) from "the archive metadata declares this exact size; a
/// short or long decoded payload is corruption" (`Exact`). The previous
/// `Option<u64>` parameter was always treated as `Cap`, so a backend
/// reader that EOF'd early committed truncated output silently when
/// `verify_crc32 = false` (R0073-0005).
///
/// `Unbounded` and `Cap` are documented API surface for backends that
/// genuinely cannot declare an exact size (libarchive disk extraction
/// for unknown-size entries, future inspection-time CRC walks); they
/// are kept on the enum even when no current call site selects them
/// so the contract is closed at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyByteBudget {
    /// No declared size — the helper does not bound the running total.
    #[expect(
        dead_code,
        reason = "kept for type-level contract completeness; no current call site constructs this variant"
    )]
    Unbounded,
    /// Upper bound only. Decoder may legitimately EOF before reaching
    /// `n` (e.g. inspection-time CRC walks where the archive declares
    /// no size).
    #[expect(
        dead_code,
        reason = "kept for type-level contract completeness; no current call site constructs this variant"
    )]
    Cap(u64),
    /// Exact byte count. The decoded payload must equal `n` on EOF —
    /// short reads are reported as corruption, overflow is rejected on
    /// the next chunk just like `Cap` (R0073-0005).
    Exact(u64),
}

/// Loop-invariant context for [`copy_with_optional_crc_bounded`].
///
/// Bundles the labels and policy fields each call needs (entry diagnostics
/// path, on-disk path for write errors, public-API op label, optional CRC
/// expectation, and byte budget) so the helper takes 4 args instead of 8.
pub(crate) struct CopyContext<'a> {
    /// Archive-internal path used in error diagnostics (read-error path,
    /// CRC mismatch, ratio-budget exceeded, cancellation).
    pub entry_path: &'a str,
    /// On-disk path used when reporting write errors.
    pub write_error_path: &'a Path,
    /// Public operation label (`extract_all`, `extract_file`, …) so
    /// `OperationBlocked` errors surface the caller-side method name.
    pub op: &'static str,
    /// Optional expected CRC32. When `Some`, the helper hashes the
    /// stream and verifies on success.
    pub expected_crc: Option<u32>,
    /// Byte budget — `Exact` enforces both the upper bound and detects
    /// short-decoder corruption; `Cap` is upper-bound-only; `Unbounded`
    /// disables the limit.
    pub budget: CopyByteBudget,
}

/// Copy data with optional CRC32 verification, a byte-size contract,
/// and an optional chunk-level cancellation hook.
///
/// Always streams in 64 KiB chunks regardless of CRC mode so the copy
/// can be interrupted from outside (drop the writer) and so the
/// caller-supplied budget can stop a runaway decoder mid-stream
/// (R0072-0005, R0073-0005). Previously the no-CRC path delegated to
/// `std::io::copy`, which produced a single blocking I/O call without
/// any size enforcement and made cancellation observable only between
/// entries.
///
/// `ctx.budget` (R0073-0005):
/// - [`CopyByteBudget::Unbounded`] — no size enforcement.
/// - [`CopyByteBudget::Cap(n)`](CopyByteBudget::Cap) — abort with
///   `OperationBlocked` once the running total would exceed `n`. Short
///   reads (decoder EOFs before `n`) are accepted.
/// - [`CopyByteBudget::Exact(n)`](CopyByteBudget::Exact) — same overflow
///   guard as `Cap`, plus on-EOF check that `total == n`. A
///   decoder-side short read surfaces as `Corruption`. Used by ZIP /
///   7z call sites where the central-directory / TOC size is
///   authoritative.
///
/// `on_chunk` (R0073-0006): optional `FnMut(bytes_so_far_in_entry) ->
/// ControlFlow<()>`. Invoked after each successful chunk write. A
/// `ControlFlow::Break(())` return aborts the copy with the typed
/// [`ArchiveError::Cancelled`] (R0076-0012) before
/// [`AtomicOutputFile::commit`] is reached, so the partial output is
/// dropped on `Drop`. Pass `None` when the surrounding extraction has
/// no progress/cancel surface.
///
/// The output file is left in whatever state the partial write
/// produced; `AtomicOutputFile::commit()` is the caller's
/// responsibility, so an aborted copy never commits.
pub(crate) fn copy_with_optional_crc_bounded<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    mut on_chunk: Option<&mut dyn FnMut(u64) -> std::ops::ControlFlow<()>>,
    ctx: CopyContext<'_>,
) -> Result<u64> {
    let CopyContext {
        entry_path,
        write_error_path,
        op,
        expected_crc,
        budget,
    } = ctx;
    let mut hasher = expected_crc.map(|_| crc32fast::Hasher::new());
    let mut total: u64 = 0;
    let mut buffer = [0u8; 65536];

    let cap = match budget {
        CopyByteBudget::Unbounded => None,
        CopyByteBudget::Cap(c) | CopyByteBudget::Exact(c) => Some(c),
    };

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| map_entry_read_error(e, Path::new(entry_path)))?;
        if n == 0 {
            break;
        }
        if let Some(c) = cap {
            if total.saturating_add(n as u64) > c {
                // Surface the originating public operation
                // (`extract_all`, `extract_file`, …) instead of the
                // generic `extract` so downstream callers can match on
                // the caller-side method name.
                return Err(ArchiveError::operation_blocked(
                    op,
                    format!(
                        "Decoded payload for '{entry_path}' exceeds the configured per-entry limit of {c} bytes",
                    ),
                ));
            }
        }
        writer
            .write_all(&buffer[..n])
            .map_err(|e| ArchiveError::io("write", write_error_path.to_path_buf(), e))?;
        if let Some(h) = hasher.as_mut() {
            h.update(&buffer[..n]);
        }
        // R0076-0011: saturating_add — the cap check above already
        // guards its own addition, but the unbounded (`cap == None`)
        // branch reaches this site without one.
        total = total.saturating_add(n as u64);
        if let Some(cb) = on_chunk.as_mut() {
            if let std::ops::ControlFlow::Break(()) = cb(total) {
                return Err(ArchiveError::Cancelled { operation: op });
            }
        }
    }

    // R0073-0005: short-decode is corruption when the archive metadata
    // declared an authoritative size. The CRC check (when enabled) would
    // also catch this, but call sites that opt out of CRC verification
    // would otherwise commit truncated output silently. Kept as a
    // nested `if` rather than a let-chain to stay within the crate's
    // 1.85 MSRV.
    if let CopyByteBudget::Exact(expected) = budget {
        if total != expected {
            return Err(ArchiveError::corruption(
                entry_path.to_string(),
                format!(
                    "Decoder produced {} bytes for '{}'; archive metadata declared {} bytes",
                    total, entry_path, expected
                ),
            ));
        }
    }

    if let Some(h) = hasher {
        verify_crc32_value(h.finalize(), expected_crc, entry_path)?;
    }
    Ok(total)
}

#[cfg(test)]
mod root_naming_tests {
    use super::*;

    /// ticgit 330f38: a filesystem root has no parent, so the
    /// parent-relative rule produced an empty archive path for the root
    /// itself, which the walker's guard then dropped — silently. Its
    /// children were still archived, unprefixed, so the loss was invisible
    /// unless you went looking for the root's own mtime and mode.
    #[test]
    fn a_filesystem_root_gets_a_synthesised_name_and_an_ordinary_directory_does_not() {
        let (base, synthetic) = archive_base_for_root(Path::new("/"));
        assert_eq!(base, Path::new("/"), "a root is its own base");
        assert_eq!(
            synthetic.as_deref(),
            Some("rootfs"),
            "the root must be named, or its entry is dropped"
        );

        let ordinary = Path::new("/home/u/project");
        let (base, synthetic) = archive_base_for_root(ordinary);
        assert_eq!(
            base,
            Path::new("/home/u"),
            "an ordinary source stays relative to its parent"
        );
        assert_eq!(
            synthetic, None,
            "an ordinary source already has a name of its own"
        );
    }

    /// The synthesised name behaves exactly like an ordinary source
    /// directory's name: it is the top-level prefix for the whole tree.
    ///
    /// This is what makes the result coherent. Naming the root entry
    /// without nesting its children would emit an empty `rootfs/` sitting
    /// beside `etc/` — worse than the drop it replaces. Nesting also makes
    /// a collision impossible: a real `/rootfs` becomes `rootfs/rootfs`,
    /// not a second `rootfs`.
    #[test]
    fn the_synthesised_root_prefixes_the_whole_tree() {
        assert_eq!(
            compose_archive_path(Path::new(""), Some("rootfs")),
            "rootfs"
        );
        assert_eq!(
            compose_archive_path(Path::new("etc"), Some("rootfs")),
            "rootfs/etc"
        );
        assert_eq!(
            compose_archive_path(Path::new("etc/hosts"), Some("rootfs")),
            "rootfs/etc/hosts"
        );
        // The real child that would otherwise have collided.
        assert_eq!(
            compose_archive_path(Path::new("rootfs"), Some("rootfs")),
            "rootfs/rootfs"
        );
    }

    /// Without a synthesised root nothing changes — the parent-relative
    /// path is the archive path, exactly as before.
    #[test]
    fn an_ordinary_source_composes_exactly_as_it_always_did() {
        assert_eq!(compose_archive_path(Path::new("project"), None), "project");
        assert_eq!(
            compose_archive_path(Path::new("project/src/main.rs"), None),
            "project/src/main.rs"
        );
        assert_eq!(compose_archive_path(Path::new(""), None), "");
    }

    /// AD 0064 write-side ruling, tested on in-memory paths.
    ///
    /// It has to be in-memory. Both macOS filesystems in play here
    /// refuse to store the bytes: `/Volumes/Temp` is HFS+, which
    /// transliterates an invalid byte into the literal ASCII text
    /// `%FF`, so a file created as `bad\xFF.txt` comes back out of
    /// `read_dir` as `bad%FF.txt` — valid UTF-8, and correctly not
    /// rejected. A filesystem-level test of a non-UTF-8 *child* name
    /// therefore cannot be written on this host at all; it would pass
    /// only by never constructing the case it claims to cover.
    #[cfg(unix)]
    #[test]
    fn non_utf8_relative_paths_are_refused() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let fs_path = Path::new("/src/tree/bad.txt");

        // A child component carrying a byte that is never valid UTF-8.
        let bad = Path::new(OsStr::from_bytes(b"tree/bad\xFF.txt"));
        let err = reject_non_utf8_relative(fs_path, bad)
            .expect_err("an invalid byte in a child component must be refused");
        assert!(
            err.to_string().contains("not valid UTF-8"),
            "the refusal must name the encoding: {err}"
        );

        // The source root's own name is a component like any other —
        // and the one that prefixes every entry in the archive.
        let bad_root = Path::new(OsStr::from_bytes(b"tree\xFF/fine.txt"));
        reject_non_utf8_relative(fs_path, bad_root)
            .expect_err("an invalid byte in the root component must be refused");

        // A lone truncated multi-byte sequence, which `to_string_lossy`
        // would render as a single U+FFFD.
        let truncated = Path::new(OsStr::from_bytes(b"tree/\xE6\x97.txt"));
        reject_non_utf8_relative(fs_path, truncated)
            .expect_err("a truncated multi-byte sequence must be refused");
    }

    /// The guard must key on UTF-8 validity, not on being ASCII. A
    /// "printable ASCII" check would pass the test above and silently
    /// break every non-English filename, so pin the difference.
    #[test]
    fn valid_utf8_relative_paths_are_accepted() {
        reject_non_utf8_relative(Path::new("/x"), Path::new("tree/plain.txt"))
            .expect("ASCII must be accepted");
        reject_non_utf8_relative(Path::new("/x"), Path::new("tree/日本語.txt"))
            .expect("non-ASCII UTF-8 must be accepted");
        reject_non_utf8_relative(Path::new("/x"), Path::new("트리/파일.txt"))
            .expect("non-ASCII UTF-8 must be accepted in every component");
        reject_non_utf8_relative(Path::new("/x"), Path::new(""))
            .expect("the empty relative path of a synthesised root must be accepted");
    }

    /// The name is derived from the root's own spelling, so it is stable
    /// across runs and distinguishes volumes rather than calling every
    /// root the same thing.
    #[test]
    fn synthesised_root_names_are_stable_and_volume_specific() {
        for (root, expected) in [
            ("/", "rootfs"),
            ("C:\\", "C"),
            ("D:\\", "D"),
            ("\\\\srv\\share\\", "srv_share"),
        ] {
            let name = synthesise_root_name(Path::new(root));
            assert_eq!(name, expected, "root {root:?} named {name:?}");
            assert_eq!(
                name,
                synthesise_root_name(Path::new(root)),
                "the same root must always produce the same name"
            );
        }

        assert_ne!(
            synthesise_root_name(Path::new("C:\\")),
            synthesise_root_name(Path::new("D:\\")),
            "different volumes must not collapse to one archive name"
        );
    }

    /// Whatever the spelling, the result has to be usable as an archive
    /// path component.
    #[test]
    fn a_synthesised_name_is_always_a_usable_path_component() {
        for root in ["/", "C:\\", "\\\\srv\\share\\", "//", ":::"] {
            let name = synthesise_root_name(Path::new(root));
            assert!(!name.is_empty(), "{root:?} produced an empty name");
            assert!(
                !name.contains('/') && !name.contains('\\'),
                "{root:?} produced {name:?}, which is a path, not a component"
            );
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{root:?} produced {name:?}, which is not portable"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R0079-0016: the `overwrite = false` staging install must be
    /// atomic-noclobber — a destination that exists (including one that
    /// appeared after a caller's racy pre-check) surfaces as
    /// `AlreadyExists` and is left untouched.
    #[test]
    fn rename_noclobber_rejects_existing_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stage = dir.path().join("stage");
        let dest = dir.path().join("dest");
        std::fs::write(&stage, b"staged").expect("write stage");
        std::fs::write(&dest, b"original").expect("write dest");

        let err = rename_noclobber(&stage, &dest).expect_err("existing dest must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dest).expect("read dest"), b"original");
        assert!(stage.exists(), "stage must survive a rejected install");
    }

    #[test]
    fn rename_noclobber_installs_when_destination_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stage = dir.path().join("stage");
        let dest = dir.path().join("dest");
        std::fs::write(&stage, b"staged").expect("write stage");

        rename_noclobber(&stage, &dest).expect("install succeeds");
        assert_eq!(std::fs::read(&dest).expect("read dest"), b"staged");
        assert!(!stage.exists(), "stage is consumed by the install");
    }

    /// R0001-0073: a path carrying an interior NUL code unit must be
    /// rejected before it reaches `MoveFileExW`, which would otherwise
    /// stop at the zero and operate on the truncated prefix. Windows-only
    /// (the helper is `cfg(windows)`); verified by the Windows CI job.
    #[cfg(windows)]
    #[test]
    fn encode_wide_rejects_interior_nul() {
        use std::os::windows::ffi::OsStringExt;

        let clean = encode_wide_nul_terminated(Path::new("C:\\dir\\file.zip"))
            .expect("NUL-free path encodes");
        assert_eq!(clean.last(), Some(&0u16));
        assert!(!clean[..clean.len() - 1].contains(&0u16));

        let nul_bearing = PathBuf::from(std::ffi::OsString::from_wide(&[
            0x61, 0x00, 0x62, 0x2E, 0x7A,
        ]));
        let err =
            encode_wide_nul_terminated(&nul_bearing).expect_err("interior NUL must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    /// R0081-0043: the entry-read classifier recognises the CRC-mismatch
    /// marker set and leaves unrelated read failures as generic I/O.
    #[test]
    fn checksum_failure_classifier_matches_known_messages() {
        for msg in CHECKSUM_FAILURE_MARKERS {
            assert!(
                is_checksum_failure_message(msg),
                "expected checksum-failure classification for: {msg}"
            );
        }
        for msg in [
            "failed to fill whole buffer",
            "unexpected end of file",
            "No such file or directory (os error 2)",
        ] {
            assert!(
                !is_checksum_failure_message(msg),
                "must not classify as checksum failure: {msg}"
            );
        }
    }

    /// R0081-0043: pin the *actual* string the zip 2.4 reader raises on a
    /// CRC mismatch, so an upstream wording change trips this test instead
    /// of silently reclassifying corruption. Writes one Stored entry,
    /// corrupts a payload byte in the serialized archive, reads it back,
    /// and asserts the surfaced error is classified as a checksum failure
    /// and mapped to structured `Corruption`.
    #[test]
    fn zip_reader_crc_mismatch_is_classified_as_checksum_failure() {
        use std::io::Cursor;
        use zip::write::{SimpleFileOptions, ZipWriter};

        const PAYLOAD: &[u8] = b"unified-archive-crc-probe";

        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("probe.bin", opts).expect("start_file");
            writer.write_all(PAYLOAD).expect("write payload");
            writer.finish().expect("finish");
        }

        // The Stored payload appears verbatim exactly once (the central
        // directory does not duplicate file data); flipping a byte breaks
        // the CRC the header still describes.
        let offset = buf
            .windows(PAYLOAD.len())
            .position(|w| w == PAYLOAD)
            .expect("payload present in archive");
        buf[offset] ^= 0xFF;

        let mut archive = zip::ZipArchive::new(Cursor::new(buf)).expect("open corrupt archive");
        let mut entry = archive.by_index(0).expect("open entry");
        let read_err = entry
            .read_to_end(&mut Vec::new())
            .expect_err("CRC mismatch must surface as an error");

        assert!(
            is_checksum_failure_message(&read_err.to_string()),
            "upstream CRC-mismatch wording changed: {read_err}"
        );
        assert!(matches!(
            map_entry_read_error(read_err, Path::new("probe.bin")),
            ArchiveError::Corruption { .. }
        ));
    }

    /// R0076-0014: the helper both writers now reach the filesystem
    /// through must itself refuse a symlink.
    ///
    /// This is a unit test rather than an integration one on purpose. The
    /// public `add_file_from_path` never reaches this code for a plain
    /// symlink — `creation::validate_file_path` refuses at the facade
    /// first — so an integration test cannot tell whether this layer works
    /// at all. It is defence in depth, and defence in depth that nothing
    /// exercises is indistinguishable from defence that was deleted.
    #[cfg(unix)]
    #[test]
    fn open_file_no_follow_symlinks_refuses_a_symlink() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("real.txt");
        std::fs::write(&target, b"target bytes").expect("write target");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let err = super::open_file_no_follow_symlinks(&link, "add_file_from_path")
            .expect_err("a symlink must be refused at this layer too");
        assert!(
            err.to_string().contains("symlink"),
            "the refusal must name the reason; got: {err}"
        );
    }

    /// The control: a regular file opens, and reports its real length.
    #[cfg(unix)]
    #[test]
    fn open_file_no_follow_symlinks_accepts_a_regular_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("real.txt");
        std::fs::write(&target, b"target bytes").expect("write target");

        let (_file, metadata) = super::open_file_no_follow_symlinks(&target, "add_file_from_path")
            .expect("a regular file must open");
        assert_eq!(metadata.len(), b"target bytes".len() as u64);
    }
}
