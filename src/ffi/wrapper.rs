//! Safe Rust wrappers around FFI bindings
//!
//! Provides memory-safe and ergonomic interfaces with RAII patterns

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::ffi::unrar::*;
use crate::format::ArchiveFormat;
use crate::options::{ProgressCallback, RateLimiter};
use crate::password::Password;
use crate::payload_window::PayloadWindow;
use crate::security::sanitize_entry_path;
use once_cell::sync::OnceCell;
use std::cell::Cell;
use std::ffi::CString;
use std::os::raw::{c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use super::common::normalize_path;
use tempfile::NamedTempFile;

/// Seconds between Windows FILETIME epoch (1601-01-01) and Unix epoch (1970-01-01)
const FILETIME_UNIX_EPOCH_DIFF_SECS: u64 = 11_644_473_600;

/// Convert a Windows FILETIME (high+low 32-bit halves, 100-ns ticks since
/// 1601-01-01 UTC) into a `SystemTime`, or `None` for pre-1970 timestamps
/// rather than collapsing them to UNIX_EPOCH (R0069-0030). Returns `None`
/// when both halves are zero so callers don't have to pre-check for
/// "missing timestamp". Sub-second ticks are preserved as nanoseconds
/// instead of being truncated away (R0076-0069).
fn filetime_to_system_time(low: u32, high: u32) -> Option<SystemTime> {
    if low == 0 && high == 0 {
        return None;
    }
    let ticks = (high as u64) << 32 | (low as u64);
    let secs = ticks / 10_000_000;
    if secs < FILETIME_UNIX_EPOCH_DIFF_SECS {
        // Pre-epoch — surface as "unknown" instead of clamping to 1970-01-01.
        return None;
    }
    // Each tick is 100 ns; the remainder is < 10^7, so `* 100` stays below
    // one second in nanoseconds and cannot overflow u32 (R0076-0069).
    let subsec_nanos = (ticks % 10_000_000) as u32 * 100;
    Some(UNIX_EPOCH + std::time::Duration::new(secs - FILETIME_UNIX_EPOCH_DIFF_SECS, subsec_nanos))
}

/// Process-wide serialization lock for UnRAR FFI calls.
///
/// The UnRAR C library maintains mutable global state (parser buffers,
/// encryption tables, error codes) that is not safe to touch from multiple
/// threads even when the threads operate on *different* archive handles.
/// Guard every FFI entry point with this mutex to prevent the cross-thread
/// CRC / header corruption observed in concurrent-use testing.
///
/// The lock is acquired at small scopes (one `unsafe { ffi_call() }` each),
/// so throughput is only affected when multiple threads actually contend.
/// Poisoning is recovered by reading past the poison — each FFI call is
/// stateless with respect to the poisoned thread's archive handle, so there
/// is no cross-contamination risk.
static UNRAR_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// Set while this thread holds [`UNRAR_LOCK`]. The R0080-0022
    /// `UCM_PROCESSDATA` trampoline runs the caller's progress callback
    /// *while the lock is held*; because the mutex is non-reentrant, a
    /// callback that starts another UnRAR operation on the same thread would
    /// re-acquire the lock and deadlock the whole process permanently. This
    /// sentinel lets [`unrar_lock`] detect same-thread re-entry and return a
    /// typed error instead (R0081-0063).
    static IN_UNRAR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// RAII guard returned by [`unrar_lock`]. Owns the process-wide
/// [`UNRAR_LOCK`] mutex guard and clears the thread-local re-entrancy
/// sentinel when dropped (R0081-0063).
struct UnrarLockGuard {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for UnrarLockGuard {
    fn drop(&mut self) {
        IN_UNRAR.set(false);
    }
}

/// Acquire the UnRAR FFI lock, recovering from poison.
///
/// Rejects same-thread re-entry with a typed error rather than
/// self-deadlocking (R0081-0063): the `UCM_PROCESSDATA` trampoline invokes
/// caller code (the progress callback) while this lock is held, so a callback
/// that attempts another UnRAR operation on the same thread would otherwise
/// block forever on the non-reentrant mutex. `try_lock` is deliberately *not*
/// used — legitimate cross-thread contention must still block, because
/// UnRAR's global state is not thread-safe and the mutex exists to serialize
/// it; turning that contention into a spurious error would be wrong. A
/// thread-local sentinel distinguishes the two cases.
#[inline]
fn unrar_lock() -> Result<UnrarLockGuard> {
    if IN_UNRAR.get() {
        return Err(ArchiveError::operation_blocked(
            crate::error::ops::EXTRACT,
            "re-entrant UnRAR access detected: a progress callback (or other code running during extraction) attempted another archive operation on the same thread; this is unsupported and would deadlock",
        ));
    }
    // Mark the thread as holding the lock before we block on acquisition;
    // the sentinel is thread-local, so it only ever gates *this* thread's
    // re-entry and never interferes with legitimate cross-thread blocking.
    IN_UNRAR.set(true);
    let guard = UNRAR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(UnrarLockGuard { _guard: guard })
}

/// How far into a file UnRAR itself will look for the RAR signature of a
/// self-extracting archive's payload.
///
/// `MAXSFXSIZE` is `0x400000` (4 MiB) in the vendored
/// `src/ffi/native/unrar/rardefs.hpp`, and `Archive::IsArchive`
/// (`src/ffi/native/unrar/archive.cpp`) reads that many bytes less the 16 it
/// has already buffered, then scans them for the first marker. Bumping the
/// vendored sources means checking this number against that header again.
///
/// Deliberately expressed as `MAXSFXSIZE - 16` so it is a few bytes
/// *stricter* than the C++ loop rather than a few bytes looser. The
/// asymmetry is free: an offset this bound declines is staged the way it
/// always was, while an offset it wrongly admitted would hand the SDK an
/// outer file whose payload it cannot find and fail the open outright.
///
/// Crate-visible because the gate that consults it lives in
/// [`crate::Archive`], not here — this backend never refuses on its own;
/// see [`UnrarArchive::open_at_offset`].
pub(crate) const UNRAR_MAX_SFX_SCAN: u64 = 0x400000 - 16;

/// Safe wrapper around UnRAR archive handle
///
/// Automatically closes archive on drop (RAII pattern)
pub struct UnrarArchive {
    handle: RARHandle,
    path: PathBuf,
    /// Where the archive payload begins inside [`Self::path`]: `0` for an
    /// ordinary open, non-zero only for a handle opened in place on a
    /// self-extracting archive ([`Self::open_at_offset`]).
    ///
    /// The SDK handle needs nothing from this, which is why in-place RAR
    /// support cost no FFI. `RAROpenArchiveEx` is handed the *outer*
    /// pathname and re-derives the payload start itself: `Archive::IsArchive`
    /// reads 7 bytes at position 0 and, finding no RAR marker, scans up to
    /// `MAXSFXSIZE` bytes for the first one and sets its internal `SFXSize`
    /// to that offset. Listing, extraction and integrity testing all go
    /// through that handle, so they already read an SFX payload in place and
    /// always did.
    ///
    /// This field exists for the one read that does *not* go through the
    /// handle: [`Self::parse_recovery_percentage`] opens `self.path` itself
    /// and walks raw block bytes, which on an in-place handle would
    /// otherwise start on stub bytes and report nonsense.
    payload_offset: u64,
    password: Option<Password>,
    /// Archive header flags (includes solid, volume, locked flags)
    flags: c_uint,
    /// Memoised full listing (R0079-0001 / OI-0065-003). UnRAR handles
    /// are exhausted after one header walk, so `list_files` populates
    /// this cell via a dedicated fresh handle; every later call shares
    /// the cached view via `Arc::clone` regardless of how `self.handle`
    /// has been positioned in between.
    listing: OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// Identity of the archive file this handle is bound to, captured at
    /// open inside the R0081-0064 lock bracket.
    ///
    /// Two guards read it, at two altitudes:
    ///
    /// - [`Self::revalidate_identity`] — the recovery-metadata reader
    ///   re-parses `self.path` through a plain `File`, so it re-`stat`s
    ///   and compares before trusting those bytes (R0080-0061).
    /// - [`Self::fresh_handle`] — every later operation re-opens the
    ///   archive *by pathname* while the AD 0065 listing snapshot, and
    ///   every safety-gate decision taken from it, still describes the
    ///   file seen at open (OI-0001-002).
    ///
    /// `None` when the `stat` failed: capture is deliberately best-effort,
    /// because the native open — not this — is the authority on whether
    /// the file is usable, so a handle that could not be bound still
    /// works, it simply carries no binding. The *comparisons* are not
    /// best-effort; see [`crate::fs_identity::FileIdentity::revalidate`].
    ///
    /// UnRAR is a path-only backend: `RAROpenArchiveEx` takes a pathname
    /// and cannot accept a descriptor, so the owned-fd hand-off that would
    /// close the residual stat-to-open window is impossible here on every
    /// platform — see [`crate::fs_identity`] for why that hand-off was
    /// rejected even for the fd-capable libarchive backend.
    identity: Option<crate::fs_identity::FileIdentity>,
    /// Callback state registered with this handle for its whole life.
    ///
    /// Boxed so its address is stable: the SDK holds a raw pointer to it
    /// from `RAROpenArchiveEx` until `RARCloseArchive`, and moving this
    /// `UnrarArchive` moves the `Box`, not the allocation it points at.
    /// See [`UnrarHandleContext`] for why it has to outlive individual
    /// operations.
    callback_context: Box<UnrarHandleContext>,
}

// SAFETY: `handle` is a raw UnRAR SDK handle owned exclusively by this
// struct — created in `open_with_mode`, used only inside `&self`/`&mut
// self` method scopes (never stored elsewhere), and closed exactly once
// in `Drop`. The handle carries no thread-affine state; the SDK's
// mutable *global* state is serialised behind `UNRAR_LOCK` around every
// FFI call, so moving the owner to another thread is sound. All other
// fields are `Send`, so this impl only restores what the raw pointer
// suppressed; the auto-derived `!Sync` is kept, so `&UnrarArchive`
// still cannot be shared across threads (R0079-0015).
unsafe impl Send for UnrarArchive {}

impl UnrarArchive {
    /// Open RAR/RAR5 archive with specific mode
    fn open_with_mode(path: impl AsRef<Path>, mode: c_uint) -> Result<Self> {
        Self::open_with_mode_and_password(path, mode, None)
    }

    /// [`Self::open_with_mode`], with the password available *during* the
    /// open rather than set afterwards.
    ///
    /// That ordering is the whole point (ticgit 3f8790). A `-hp` archive
    /// keeps its main header inside a HEAD_CRYPT block, and the SDK reads
    /// that header inside `RAROpenArchiveEx` — before any `RARSetPassword`
    /// could run. Without a password there it cannot decrypt, so
    /// `open_data.flags` comes back missing `ROADF_RECOVERY` and every
    /// other main-header flag, and nothing revisits that word later. The
    /// callback registered here answers the SDK's `UCM_NEEDPASSWORD`
    /// request while the header is being read.
    fn open_with_mode_and_password(
        path: impl AsRef<Path>,
        mode: c_uint,
        password: Option<&str>,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let callback_context = UnrarHandleContext::new(password)?;

        // CString preserves raw bytes on Unix (AD 0064). Windows wide-char
        // path support deferred to OI-0065-001.
        let c_path = crate::ffi::common::path_to_cstring_checked(&path_buf)?;

        unsafe {
            // Register at open ONLY when there is a password to answer
            // with. `dll.cpp` copies these into its `Cmd` before
            // `Archive::IsArchive` reads the main header, which is what
            // lets a `-hp` archive decrypt in time for the flags word
            // (ticgit 3f8790).
            //
            // Registering unconditionally is wrong, and measurably so: a
            // header-encrypted archive opened *without* a password used to
            // succeed — `Archive::open` on such a file reports
            // `is_encrypted()` without ever seeing the contents — and a
            // callback that declines the password request makes the SDK
            // proceed as though an empty password had been supplied, which
            // fails the open with `ERAR_MISSING_PASSWORD`. Declining with
            // `-1` does not help: `RequestArcPassword` reaches the same
            // `Cmd->Password.Set()` either way. The only faithful "no
            // password" behaviour is the one the SDK takes when
            // `Cmd->Callback` is NULL, so that is preserved verbatim.
            //
            // The callback is installed immediately after a successful
            // open instead, which is all ticgit 03ddc6 needs — volume
            // requests happen during walks, not during the main-header
            // read.
            let (open_callback, open_user_data) = if callback_context.password.is_some() {
                (
                    unrar_handle_callback as *const () as *mut c_void,
                    &*callback_context as *const UnrarHandleContext as isize,
                )
            } else {
                (std::ptr::null_mut(), 0)
            };

            let mut open_data = RAROpenArchiveDataEx {
                arc_name: c_path.as_ptr(),
                open_mode: mode,
                callback: open_callback,
                user_data: open_user_data,
                ..Default::default()
            };

            let _guard = unrar_lock()?;

            // R0081-0064: capture the file identity *before* the native open
            // and re-verify it *after*, both under the lock, so a swap between
            // capturing identity and binding the handle is detected. Capturing
            // after the open (as before) would bind the handle to the bytes
            // seen at open while recording identity for whatever the file was
            // afterwards, defeating the recovery-metadata revalidation that
            // relies on this identity (R0080-0061) and the fresh-handle
            // binding OI-0001-002 layers on top of it.
            let identity = crate::fs_identity::FileIdentity::capture(&path_buf);
            let handle = RAROpenArchiveEx(&mut open_data);

            if handle.is_null() || open_data.open_result != ERAR_SUCCESS as u32 {
                // A volume request during the open — the SDK follows the
                // volume chain while reading the main header of a
                // continuation volume — aborts through the callback and
                // lands here as `ERAR_EOPEN`. Prefer the specific
                // diagnostic when the callback left one.
                if let Some(err) = callback_context.take_abort_error(&path_buf) {
                    return Err(err);
                }
                return Err(map_unrar_error(open_data.open_result as c_int, &path_buf));
            }

            // Re-stat and compare. A `None` capture (a `stat` failure) skips
            // the check, mirroring `revalidate_identity`.
            //
            // Deliberately the `(dev, ino)` half only, and deliberately
            // Unix-only — which is *not* what `fresh_handle` does a few
            // methods down, and the asymmetry is the point rather than an
            // oversight. This bracket is R0081-0064's, and it was reviewed
            // as an inode comparison that is a static no-op off Unix;
            // moving `UnrarFileIdentity` onto the shared `FileIdentity`
            // must not quietly widen it. A length comparison here has a
            // false positive the inode comparison does not: an external
            // `rar a` append or an in-progress download is still growing
            // the archive, and for a multi-volume set `RAROpenArchiveEx`
            // walks the whole volume chain inside this call, so the window
            // is milliseconds rather than microseconds — `open()` would
            // start failing where it used to succeed. `fresh_handle` keeps
            // the *full* comparison because it answers a different
            // question: whether the file still matches the AD 0065 listing
            // snapshot a safety gate already ruled on, and there the
            // appended archive is exactly what must be refused.
            #[cfg(unix)]
            if let Some(expected) = identity {
                if crate::fs_identity::FileIdentity::capture(&path_buf).map(|found| found.inode)
                    != Some(expected.inode)
                {
                    // The handle was bound to whatever the open saw; close it
                    // before surfacing the swap. We already hold the lock, so
                    // the raw close is correctly serialized (re-acquiring would
                    // trip the re-entrancy sentinel).
                    RARCloseArchive(handle);
                    return Err(ArchiveError::operation_blocked(
                        "open",
                        format!(
                            "archive path {} identity changed during open; refusing to bind a handle whose recorded identity may not match the opened bytes",
                            path_buf.display()
                        ),
                    ));
                }
            }

            // Every walk on this handle from here on can answer a volume
            // request (ticgit 03ddc6), whether or not the open itself had a
            // callback. `RARSetCallback` is idempotent, so re-registering
            // the same pointer the open already carried is harmless.
            RARSetCallback(
                handle,
                Some(unrar_handle_callback),
                &*callback_context as *const UnrarHandleContext as isize,
            );

            Ok(Self {
                handle,
                path: path_buf,
                // Offset-zero is the ordinary case; `open_at_offset` is the
                // only thing that moves it.
                payload_offset: 0,
                password: password.map(Password::new),
                flags: open_data.flags,
                listing: OnceCell::new(),
                identity,
                callback_context,
            })
        }
    }

    /// Prefer the callback's own reason over the SDK's return code.
    ///
    /// A missing volume met *outside* an extraction — a listing walk's
    /// `RAR_SKIP`, a header read — aborts through the handle callback, and
    /// `DllVolChange` reports that abort as a bare `ERAR_EOPEN`
    /// indistinguishable from "could not open the archive at all". Asking
    /// the context first turns it back into the diagnostic that says what
    /// to do about it (ticgit 03ddc6).
    fn unrar_error(&self, result: c_int) -> ArchiveError {
        self.callback_context
            .take_abort_error(&self.path)
            .unwrap_or_else(|| map_unrar_error(result, &self.path))
    }

    /// The raw pointer the SDK holds for this handle's callback, for
    /// restoring it after an operation installs its own.
    fn callback_context_ptr(&self) -> isize {
        &*self.callback_context as *const UnrarHandleContext as isize
    }

    /// Open RAR/RAR5 archive for reading and extraction
    ///
    /// Uses RAR_OM_EXTRACT mode to support both listing headers and extracting files.
    /// This mode allows iteration through entries and extraction operations.
    ///
    /// **Validation timing (AD 0052):** `RAROpenArchiveEx` reads and
    /// validates the archive's *main* header here, so a non-RAR or
    /// header-damaged input fails at `open`. Per-entry headers are not
    /// read until the first walk, so the crate-wide first-operation
    /// contract still applies to everything past the main header —
    /// [`crate::Archive::validate`] forces that walk (memoised per
    /// AD 0065) and is strictly stronger than this open-time check,
    /// while still decoding no payload (unlike `validate_integrity`).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_mode(path, RAR_OM_EXTRACT)
    }

    /// Open encrypted RAR/RAR5 archive with password
    ///
    /// The password reaches `RAROpenArchiveEx` itself, so a
    /// header-encrypted (`-hp`) archive's main header is decrypted while
    /// the SDK is deriving the archive flags rather than after
    /// (ticgit 3f8790). `RARSetPassword` still runs afterwards: for a
    /// data-only-encrypted (`-p`) archive the header never prompts, so the
    /// callback is never asked and the payload path needs the password set
    /// the ordinary way.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let archive = Self::open_with_mode_and_password(path, RAR_OM_EXTRACT, Some(password))?;

        let c_password = CString::new(password)
            .map_err(|_| ArchiveError::password("Password contains null byte"))?;

        unsafe {
            let _guard = unrar_lock()?;
            RARSetPassword(archive.handle, c_password.as_ptr());
        }

        Ok(archive)
    }

    /// Open a RAR payload that begins at `offset` bytes into `path`, in
    /// place — without first copying the payload out to a tempfile.
    ///
    /// The native open here is the *ordinary* one, on the outer pathname,
    /// and that is the whole of the trick: UnRAR resolves the SFX itself
    /// (see [`Self::payload_offset`]), so the handle this returns is
    /// indistinguishable from one opened on a plain `.rar`. No offset
    /// arithmetic reaches listing, extraction or integrity testing, and no
    /// new FFI was needed to get here — the crate simply never routed
    /// offset opens at this backend before.
    ///
    /// # What the caller has to have established first
    ///
    /// This constructor does not re-derive `offset` and cannot check it:
    /// the SDK does not report its `SFXSize` back through the DLL API, so
    /// there is no way to ask afterwards where the library actually landed.
    /// The caller therefore owns three facts, and [`crate::Archive`]'s
    /// in-place gate establishes all three before calling:
    ///
    /// 1. bytes at `offset` are a RAR marker;
    /// 2. no *earlier* RAR marker exists in `[0, offset)` — UnRAR binds to
    ///    the **first** one it finds, so an earlier byte-run in the stub
    ///    would silently open a different archive than the caller addressed;
    /// 3. `offset + 8` is within [`UNRAR_MAX_SFX_SCAN`] — past that the
    ///    library cannot see the marker at all.
    ///
    /// # Identity
    ///
    /// The open-time binding is captured exactly as for every other
    /// constructor, by name on `path`, and stays correct unchanged: for an
    /// in-place handle the outer file *is* the archive, so binding the outer
    /// pathname binds the bytes being read. The same holds for
    /// [`Self::revalidate_identity`] and [`Self::fresh_handle`].
    pub(crate) fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> {
        let mut archive = Self::open_with_mode(path, RAR_OM_EXTRACT)?;
        archive.payload_offset = offset;
        Ok(archive)
    }

    /// Create a fresh handle for extraction operations, bound to the same
    /// file this handle was opened on.
    ///
    /// UnRAR handles get exhausted after list_files(), so extraction operations
    /// need a fresh handle. This creates a new handle with the same path/password.
    ///
    /// # Why the re-open is guarded (OI-0001-002)
    ///
    /// The re-open is **by pathname**, but the AD 0065 listing snapshot —
    /// and every safety-gate decision already taken from it: the
    /// extraction-ratio verdict, the total- and per-entry-size ceilings,
    /// the entry-kind policy — describes the file that was open when the
    /// snapshot was taken. An archive rewritten on disk in between can
    /// keep every entry *name* and still carry different payloads, sizes,
    /// CRCs, entry types or encryption, so the per-entry name comparisons
    /// downstream cannot answer the question they are being asked.
    /// Comparing the file identity can: the fresh open re-captured its own
    /// identity inside the same R0081-0064 lock bracket, so when that
    /// disagrees with the identity this handle is bound to, `op` is
    /// refused rather than run against bytes the safety gate never saw.
    ///
    /// The comparison is bracketed the way the other three read backends
    /// bracket theirs — *before* the native open as well as after. The
    /// pre-open half is what makes the refusal reachable at all when the
    /// replacement is not a plainly openable RAR, and what keeps the
    /// caller's passphrase away from attacker-chosen bytes; see the
    /// comment on it.
    ///
    /// Note the asymmetry with the R0081-0064 open-time bracket inside
    /// [`Self::open_with_mode_and_password`], which compares the `(dev, ino)` half
    /// only. This one compares the *full* identity, length included,
    /// on purpose: an archive that grew under a live handle no longer
    /// matches the listing snapshot, and refusing that is the guard, not
    /// a false positive.
    ///
    /// Both disagreements fail closed — a *different* identity, and an
    /// identity that can no longer be captured at all (a `stat` that now
    /// fails on a file the SDK just opened). A handle that never captured
    /// an identity carries no binding and is not second-guessed here:
    /// capture is best-effort, comparison is not.
    ///
    /// This is **additive**. None of the name or cardinality guards
    /// downstream ([`listing_drift_mismatch`], [`listing_drift_extra`],
    /// [`listing_drift_eof`]) is superseded by it, because they catch what
    /// a `stat` cannot: a same-inode, same-length in-place rewrite; any
    /// same-length replacement off Unix; the accepted stat-to-native-open
    /// window; and plain index-bookkeeping bugs, where no file was swapped
    /// at all.
    ///
    /// # Residual: continuation volumes are not bound
    ///
    /// Only the first volume's path is identity-bound. The continuation
    /// volumes of a multi-volume set are opened *inside* the SDK by the
    /// volume-change callback and never pass through this method — and
    /// there is no per-continuation-volume listing snapshot for them to be
    /// bound to in the first place, since the listing is one walk across
    /// the whole set. That makes this out of scope here rather than an
    /// oversight: closing it would mean checking identity inside the
    /// volume-change callback against a snapshot that does not exist yet.
    fn fresh_handle(&self, op: &'static str) -> Result<Self> {
        // Pre-open half. libarchive, ZIP and 7z all revalidate *before*
        // handing the pathname to their native opener; UnRAR used to be
        // the one backend that opened first and compared afterwards, and
        // that ordering cost two things.
        //
        // It masked the drift whenever the replacement was not a plainly
        // openable RAR: a header-encrypted file answers
        // `ERAR_MISSING_PASSWORD`, `map_unrar_error` turns that into
        // `ArchiveError::password("Password required")`, and `?` carried
        // it out before the comparison below was ever reached — so the
        // caller was told their unencrypted archive now needs a password
        // instead of being told the file changed.
        //
        // And in the password-carrying branch it handed the caller's real
        // passphrase to the SDK against attacker-chosen bytes before any
        // identity check had run.
        if let Some(expected) = self.identity {
            crate::fs_identity::FileIdentity::revalidate(expected, &self.path, op)?;
        }

        let mut fresh = if let Some(pw) = self.password.as_ref() {
            Self::open_with_password(&self.path, pw.as_str())?
        } else {
            Self::open(&self.path)?
        };
        // A re-open of an in-place SFX handle is still an in-place handle.
        // The native side does not care — it re-derives the payload start
        // from the outer pathname either way — but a fresh handle that
        // silently reset this to 0 would be a trap for the next raw-byte
        // reader added to this type.
        fresh.payload_offset = self.payload_offset;

        if let Some(expected) = self.identity {
            if fresh.identity != Some(expected) {
                // `fresh` is dropped on the way out, which closes its
                // native handle (see the `Drop` impl).
                return Err(crate::fs_identity::identity_drift(
                    expected,
                    fresh.identity,
                    &self.path,
                    op,
                ));
            }
        }

        Ok(fresh)
    }

    /// Read next entry header with CRC32 and metadata
    pub fn read_header(&self) -> Result<Option<ArchiveEntry>> {
        Ok(self.read_header_with_flags()?.map(|(entry, _)| entry))
    }

    /// [`Self::read_header`] plus the raw `RARHeaderDataEx::flags` word.
    ///
    /// The listing walk needs `RHDF_SPLITBEFORE` / `RHDF_SPLITAFTER`, which
    /// [`parse_header`] deliberately does not carry onto `ArchiveEntry`:
    /// they describe the *header's* relationship to its neighbours in the
    /// volume set, not a property of the file the entry names.
    fn read_header_with_flags(&self) -> Result<Option<(ArchiveEntry, c_uint)>> {
        unsafe {
            let mut header = RARHeaderDataEx::default();
            let _guard = unrar_lock()?;
            let result = RARReadHeaderEx(self.handle, &mut header);
            drop(_guard);

            match result {
                ERAR_SUCCESS => Ok(Some((parse_header(&header)?, header.flags))),
                ERAR_END_ARCHIVE => Ok(None),
                ERAR_BAD_PASSWORD => Err(ArchiveError::password("Wrong password")),
                ERAR_MISSING_PASSWORD => Err(ArchiveError::password("Password required")),
                _ => Err(self.unrar_error(result)),
            }
        }
    }

    /// Skip current entry (move to next)
    pub fn skip_entry(&self) -> Result<()> {
        unsafe {
            let _guard = unrar_lock()?;
            let result = RARProcessFile(self.handle, RAR_SKIP, std::ptr::null(), std::ptr::null());

            if result == ERAR_SUCCESS {
                Ok(())
            } else {
                Err(self.unrar_error(result))
            }
        }
    }

    /// List all files in archive with CRC32.
    ///
    /// Repeat-safe (R0079-0001): the walk runs on a dedicated fresh
    /// handle and the result is memoised, so every call returns the
    /// full listing regardless of call order. Walking `self.handle`
    /// directly would exhaust it after one pass, making any second
    /// listing silently empty.
    pub fn list_files(&self) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.list_files_budgeted(None)
    }

    /// Budgeted listing (OI-0080-003). `budget = Some(n)` aborts the header
    /// walk once more than `n` entries are seen. UnRAR reads headers one at a
    /// time (`read_header` + `skip_entry`), so this is a true streaming early
    /// abort — it stops reading further headers rather than only bounding our
    /// `Vec`. The budget applies only to the first materialization; a cache
    /// hit ignores it, and an aborted parse does not populate `listing`.
    pub fn list_files_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.listing
            .get_or_try_init(|| {
                self.fresh_handle(crate::error::ops::LIST_FILES)?
                    .walk_entries(budget)
                    .map(std::sync::Arc::new)
            })
            .map(std::sync::Arc::clone)
    }

    /// Walk this handle's headers from its current position into a
    /// listing. Exhausts the handle — callers must hold a fresh one
    /// (see [`Self::fresh_handle`]).
    fn walk_entries(&self, budget: Option<usize>) -> Result<Vec<ArchiveEntry>> {
        let mut entries: Vec<ArchiveEntry> = Vec::new();
        let mut index = 0;
        let mut headers_seen = 0usize;

        while let Some((mut entry, flags)) = self.read_header_with_flags()? {
            // OI-0080-003: true streaming early abort — once we already hold
            // `budget` entries and another header is present, stop before
            // pushing/skipping the rest so UnRAR never reads past the
            // budget-th record.
            //
            // The budget is checked against *headers read*, not only against
            // entries kept. Coalescing (below) folds continuation headers into
            // their predecessor without growing `entries`, so bounding entries
            // alone would let an archive of a million `SPLITBEFORE` headers
            // walk forever under any budget.
            headers_seen += 1;
            if let Some(budget) = budget {
                if entries.len() >= budget || headers_seen > budget.saturating_add(1) {
                    return Err(crate::security::too_many_entries_parsed(budget));
                }
            }

            // 3b4d15: a split file is stored once per volume, each part
            // carrying the same name and the *whole* file's unpacked size.
            // Left as separate entries they read as N distinct files sharing
            // one path, which triples a content total and makes every
            // extraction guard reject the archive. UnRAR already says which
            // headers are continuations; the listing just has to honour it.
            if flags & RHDF_SPLITBEFORE != 0 {
                if let Some(prev) = entries.last_mut() {
                    // Packed bytes accumulate across parts; unpacked size is
                    // the same full-file value in every part, so it is kept
                    // from the first rather than summed.
                    prev.compressed_size = match (prev.compressed_size, entry.compressed_size) {
                        (Some(a), Some(b)) => Some(a.saturating_add(b)),
                        (a, b) => a.or(b),
                    };
                    // Each part carries a CRC over its own chunk, not over the
                    // file. Surfacing one of them as the entry's CRC would be a
                    // checksum that verifies nothing, so the coalesced entry
                    // reports none — `None` already means "not available here".
                    prev.crc32 = None;
                    self.skip_entry()?;
                    continue;
                }
                // A continuation with nothing before it: the caller opened a
                // middle volume directly. There is no predecessor to fold
                // into, so it stands alone.
            }

            entry.id = index;
            entries.push(entry);
            self.skip_entry()?;
            index += 1;
        }

        Ok(entries)
    }

    /// Get archive path (preserves raw bytes per AD 0064)
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The archive file this handle is bound to (OI-0001-002 /
    /// R0081-0064), or `None` if the open-time `stat` failed.
    ///
    /// Exists so [`crate::Archive::payload_size_for_ratio`] can
    /// revalidate before the `stat` that produces the compression-ratio
    /// denominator: that stat is a by-path re-resolution the facade does
    /// on its own, outside [`Self::fresh_handle`]'s bracket, while the
    /// numerator comes from the cached listing.
    pub(crate) fn bound_identity(&self) -> Option<crate::fs_identity::FileIdentity> {
        self.identity
    }

    /// Check if archive uses solid compression
    ///
    /// Solid compression compresses all files together as a single stream,
    /// providing better compression ratios but slower random access.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive uses solid compression
    /// * `Ok(false)` - Archive uses non-solid compression
    pub fn is_solid(&self) -> Result<bool> {
        Ok((self.flags & ROADF_SOLID) != 0)
    }

    /// Check if archive has recovery records
    ///
    /// Recovery records allow repairing corrupted archives.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive has recovery records
    /// * `Ok(false)` - Archive has no recovery records
    pub fn has_recovery_record(&self) -> Result<bool> {
        Ok((self.flags & ROADF_RECOVERY) != 0)
    }

    /// Get recovery record percentage
    ///
    /// Extracts the recovery percentage from RAR recovery record blocks.
    /// Common values are 2%, 3%, 5%, 10%, etc.
    ///
    /// # Range
    /// `u16`, not `u8` (ticgit 7ca208). RAR 6.10 stores the RAR5 recovery
    /// percentage as a vint instead of a single byte and raised its ceiling
    /// from 99% to 1000%, so `rar -rr1000p` produces a perfectly readable
    /// archive whose percentage `u8` cannot carry. The RAR4 walk is a
    /// different story: it *computes* the percentage from block counts and
    /// caps it at 100, so on that path the wider type buys representation
    /// only — a RAR4 archive can never report more than 100 here.
    ///
    /// # Returns
    /// * `Ok(Some(percentage))` - Recovery records present with known percentage
    /// * `Ok(None)` - No recovery records or percentage cannot be determined
    /// * `Err(...)` - I/O error during parsing
    ///
    /// `Ok(None)` also covers the header-encrypted archive: the walk below
    /// reads raw bytes with no password, so for a `-hp` archive the record
    /// is known to exist ([`Self::has_recovery_record`] reads the decrypted
    /// main header) while its size is not knowable here (ticgit 3f8790).
    ///
    /// # Implementation
    /// Parses the RAR file directly to locate and extract recovery record block headers,
    /// which contain the recovery percentage value.
    pub fn recovery_percentage(&self) -> Result<Option<u16>> {
        // Quick check: if no recovery flag, return None immediately
        if (self.flags & ROADF_RECOVERY) == 0 {
            return Ok(None);
        }

        // Recovery records exist, try to extract percentage
        // This requires parsing the RAR file to find recovery record blocks
        self.parse_recovery_percentage()
    }

    /// Revalidate that `self.path` still names the file opened at
    /// construction, comparing a fresh `stat` against the captured
    /// `(dev, ino)` identity (R0080-0061). Returns `OperationBlocked` on
    /// drift so recovery metadata is never parsed from a swapped file. A
    /// `None` captured identity (capture failed) skips the check.
    ///
    /// Deliberately compares only the `(dev, ino)` half of the shared
    /// [`crate::fs_identity::FileIdentity`], so this guard keeps exactly
    /// the reach it was reviewed with under R0080-0061; the length half
    /// belongs to the handle binding of [`Self::fresh_handle`], which
    /// answers a different question about a different window.
    #[cfg(unix)]
    fn revalidate_identity(&self) -> Result<()> {
        let Some(expected) = self.identity else {
            return Ok(());
        };
        let meta = std::fs::metadata(&self.path)
            .map_err(|e| ArchiveError::io("recovery-identity-revalidate", self.path.clone(), e))?;
        let found = crate::fs_identity::FileIdentity::from_metadata(&meta);
        if found.inode != expected.inode {
            return Err(ArchiveError::operation_blocked(
                "recovery_percentage",
                format!(
                    "archive path {} identity changed since open (expected dev/ino {}/{}, found {}/{}); refusing to parse recovery metadata from a swapped file",
                    self.path.display(),
                    expected.inode.dev,
                    expected.inode.ino,
                    found.inode.dev,
                    found.inode.ino
                ),
            ));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn revalidate_identity(&self) -> Result<()> {
        Ok(())
    }

    /// Parse recovery percentage from RAR file structure
    ///
    /// RAR recovery records are stored as special blocks: type 0x78 in RAR4,
    /// service header (type 3, name "RR") in RAR5.
    fn parse_recovery_percentage(&self) -> Result<Option<u16>> {
        use std::fs::File;
        use std::io::Read;

        // ticgit 3f8790 follow-on: this walk reads raw bytes with no
        // password, so it cannot read an encrypted header. Until 3f8790
        // the question never arose — `ROADF_RECOVERY` was lost for `-hp`
        // archives, so `recovery_percentage` short-circuited before ever
        // reaching here. With the flag now correct the walk *is* reached,
        // and it read the HEAD_CRYPT block's ciphertext as though it were
        // a header: bogus vints, and a `Corruption` error reporting
        // "RAR5 header extends past end of archive" for an archive
        // `unrar t` calls perfectly sound.
        //
        // A header this parser is not permitted to read is not damage.
        // Report the documented "percentage cannot be determined" instead.
        // The flag still says a record exists — `has_recovery_record()`
        // answers that from the decrypted main header — so the caller
        // learns the record is there and its size is not knowable without
        // implementing RAR5 header decryption here, which is not this
        // function's job.
        if (self.flags & ROADF_ENCHEADERS) != 0 {
            return Ok(None);
        }

        // R0080-0061: the percentage is parsed from a fresh open of
        // `self.path`, which a non-cooperating writer could have swapped
        // since the UnRAR handle was opened. Revalidate the identity
        // captured at construction so the returned percentage cannot
        // describe a different file than the flags read from the live
        // handle. Non-Unix: skipped (no portable identity here).
        self.revalidate_identity()?;

        // The payload does not always start at byte 0. An in-place SFX
        // handle ([`Self::open_at_offset`]) is bound to the outer
        // executable, and this walk is the one read in this type that does
        // *not* go through the SDK handle that resolves the offset for
        // every other operation — left unwrapped it would read the stub as
        // though it were a RAR block chain and report nonsense.
        //
        // A window makes byte `payload_offset` look like byte 0, which is
        // what both parsers already assume: their `Start(8)` / `Start(7)`
        // signature skips and their `End(0)` archive-length probe become
        // payload-relative for free, because both are generic over
        // `Read + Seek`. For the ordinary offset-0 handle the window spans
        // the whole file and every position is its own translation.
        let outer = File::open(&self.path)
            .map_err(|e| ArchiveError::io("open", std::path::Path::new(&self.path), e))?;
        let mut file = PayloadWindow::from_file(outer, self.payload_offset)
            .map_err(|e| ArchiveError::io("open", std::path::Path::new(&self.path), e))?;

        // Read first 16 bytes to check RAR signature and version
        let mut header = [0u8; 16];
        file.read_exact(&mut header)
            .map_err(|e| ArchiveError::io("read", std::path::Path::new(&self.path), e))?;

        // Check if RAR5 format (signature: "Rar!\x1A\x07\x01\x00")
        let is_rar5 = header.starts_with(b"Rar!\x1A\x07\x01\x00");

        if is_rar5 {
            // RAR5 format: parse modern block structure
            parse_rar5_recovery(&self.path, &mut file)
        } else {
            // RAR4 format: parse legacy block structure
            parse_rar4_recovery(&self.path, &mut file)
        }
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &std::path::Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, true, true, None, None, None)
    }

    /// Extract all files with options. When `selection` is `Some`, only entries
    /// whose positional index (matching `list_files()` order) is in the set
    /// are materialized; RAR's sequential format means non-selected entries
    /// are still traversed but their payload is skipped.
    ///
    /// `preserve_permissions` / `preserve_times == false` are honoured by
    /// normalising the staged file after UnRAR writes it (R0080-0023), and
    /// `max_file_size` / `max_total_size` bound the actual decoded bytes
    /// per entry and archive-wide from inside the UnRAR data callback
    /// (R0080-0008 / R0080-0022).
    ///
    /// The walk is bound to the AD 0065 cached listing: every consumed
    /// positional index must still name the entry that listing (and the
    /// safety gate above it) validated, and the visited count must match
    /// the listing length at EOF. An archive rewritten on disk between
    /// listing and extraction fails closed with a `Format` error instead
    /// of materializing attacker-chosen entries (R0001-0019).
    #[allow(clippy::too_many_arguments)]
    pub fn extract_all_with_options(
        &self,
        dest_path: &std::path::Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        selection: Option<&std::collections::HashSet<usize>>,
        max_file_size: Option<u64>,
        max_total_size: Option<u64>,
    ) -> Result<Vec<ArchiveWarning>> {
        let mut warnings: Vec<ArchiveWarning> = Vec::new();
        // `list_files()` walks a dedicated fresh handle and memoises
        // (R0079-0001), so the original `self.handle`'s position
        // (possibly EOF after a prior walk) cannot zero out the
        // progress total here.
        //
        // R0001-0019: pin the AD 0065 cached listing — the exact
        // snapshot the safety gate validated — unconditionally (it used
        // to be fetched only when a progress callback was supplied). The
        // walk below re-opens `self.path` fresh, so without this the
        // preflight and the extraction could inspect different archive
        // contents after an on-disk swap. Every consumed positional
        // index's path is cross-checked against this listing and the
        // visited count is reconciled at EOF, mirroring the libarchive
        // bulk guard (R0080-0009 / R0080-0016).
        let listing = self.list_files()?;
        let total_bytes: u64 = if progress.is_some() {
            listing
                .iter()
                .enumerate()
                .filter(|(idx, _)| match selection {
                    Some(sel) => sel.contains(idx),
                    None => true,
                })
                .filter_map(|(_, e)| e.size)
                .fold(0u64, u64::saturating_add)
        } else {
            0
        };

        // Fresh handle for extraction (`self.handle` may be EOF-positioned).
        let fresh = self.fresh_handle(crate::error::ops::EXTRACT_ALL)?;

        let abs_dest = resolve_dest_path(dest_path)?;
        // Canonicalize the destination once — every entry's sanitize
        // pass compares against the same stable base.
        let canonical_dest = crate::security::canonicalize_dest_base(&abs_dest)?;

        // One mutable extraction context owns the progress callback for the
        // whole walk so cancellation polling *and* the decoded-byte caps can
        // fire from inside `RARProcessFile`'s C decode loop, not just between
        // entries (R0080-0022 / R0080-0008). AD 0019: the callback runs on
        // this same lock-holding thread — no Send/Sync gymnastics needed.
        let mut ctx = UnrarExtractContext::new(
            crate::error::ops::EXTRACT_ALL,
            progress,
            total_bytes,
            max_total_size,
        );
        let mut entry_idx: usize = 0;

        while let Some((entry, flags)) = fresh.read_header_with_flags()? {
            // Rate-limited cancellation check before extracting.
            ctx.poll_between_entries()?;

            // 3b4d15: a continuation header is the same file resumed in the
            // next volume, not another entry. `walk_entries` folds these into
            // their predecessor, so the cached listing counts one entry where
            // UnRAR reports one header per volume — and this walk has to fold
            // them the same way or its index runs ahead of that listing.
            //
            // They only reach this walk when an entry is SKIPPED rather than
            // extracted: `ExtractCurrentFile` merges the volumes itself and
            // consumes every part, while `ProcessFile`'s `RAR_SKIP` branch on
            // a non-solid archive calls `MergeArchive(..,'L')` and returns
            // positioned on the next volume's header. Counting that header
            // would make the drift guard below report a rewrite that never
            // happened, for an archive `unrar t` calls healthy.
            //
            // `entry_idx > 0` mirrors `walk_entries`' "nothing to fold into"
            // case: a continuation as the very first header means the caller
            // opened a middle volume directly, and there it stands alone.
            if flags & RHDF_SPLITBEFORE != 0 && entry_idx > 0 {
                fresh.skip_entry()?;
                continue;
            }

            // Selection filter: skip entries whose positional index is not in
            // the set (RAR requires sequential traversal; non-selected entries
            // are skipped with RAR_SKIP rather than reopening the archive per
            // entry).
            let current_idx = entry_idx;
            entry_idx += 1;

            // R0001-0019 drift guard: every consumed positional index
            // must still name the entry the cached listing (and
            // therefore the safety gate) validated. Runs before the
            // selection and link filters so a swapped or grown archive
            // is refused even for indices this call would otherwise
            // skip. Extends the OI-0076-002 single-entry guard to the
            // bulk walk.
            match listing.get(current_idx) {
                Some(expected) if expected.path == entry.path => {}
                Some(expected) => {
                    return Err(listing_drift_mismatch(
                        current_idx,
                        &expected.path,
                        &entry.path,
                    ));
                }
                // More live entries than the validated listing — the
                // archive grew on disk after listing.
                None => return Err(listing_drift_extra(current_idx, listing.len())),
            }

            if let Some(sel) = selection {
                if !sel.contains(&current_idx) {
                    fresh.skip_entry()?;
                    continue;
                }
            }

            // Skip symlinks and hardlinks for security
            if entry.entry_type == EntryType::Symlink {
                warnings.push(ArchiveWarning::SkippedSymlink {
                    path: entry.path.clone(),
                    target: None,
                });
                fresh.skip_entry()?;
                continue;
            }
            if entry.entry_type == EntryType::HardLink {
                warnings.push(ArchiveWarning::SkippedHardLink {
                    path: entry.path.clone(),
                });
                fresh.skip_entry()?;
                continue;
            }

            // Sanitize path to prevent traversal attacks
            let safe_path = crate::security::sanitize_entry_path_with_base(
                &entry.path,
                &abs_dest,
                &canonical_dest,
            )?;

            // R0076-0005: create *and* re-verify containment. The
            // `!parent.exists()` guard this replaced also skipped the
            // verification whenever the directory was already there, which
            // is precisely the case a swap produces.
            if let Some(parent) = safe_path.parent() {
                crate::security::create_parent_dirs_verified(parent, &canonical_dest, &entry.path)?;
            }

            if entry.is_file() {
                ctx.begin_file(max_file_size);
                unrar_extract_atomic(
                    fresh.handle,
                    &safe_path,
                    overwrite,
                    &self.path,
                    crate::error::ops::EXTRACT_ALL,
                    preserve_permissions,
                    preserve_times,
                    Some(&mut ctx),
                    fresh.callback_context_ptr(),
                )?;
                ctx.finish_file(entry.size.unwrap_or(0));
            } else {
                // Directories — let UnRAR create them (via RAR_EXTRACT to the
                // path). No payload, so no callback / cap accounting is needed.
                // AD 0064 Option A: this is a real filesystem destination, so
                // it must carry the destination's raw bytes (Unix) / UTF-16
                // code units (Windows). `to_string_lossy` had UnRAR create a
                // *different* directory than the file branch above extracts
                // that directory's entries into.
                let dest_name = UnrarDestName::new(&safe_path)?;
                unsafe {
                    let _guard = unrar_lock()?;
                    let result = dest_name.process(fresh.handle, RAR_EXTRACT);
                    if result != ERAR_SUCCESS {
                        return Err(self.unrar_error(result));
                    }
                }
                ctx.advance_declared(entry.size.unwrap_or(0));
            }
        }

        // R0001-0019 cardinality guard: the fresh walk must have
        // consumed exactly as many positional indices as the validated
        // listing holds. Fewer live entries means the archive was
        // truncated or rewritten after listing — refuse rather than
        // report a short extraction as success. (The "more entries"
        // case is caught eagerly inside the loop.)
        if entry_idx != listing.len() {
            return Err(listing_drift_eof(entry_idx, listing.len()));
        }

        // R0070-0038: honor cancellation in the final callback the
        // same way the per-entry path does.
        ctx.poll_final()?;

        Ok(warnings)
    }

    /// Extract a single file by path
    ///
    /// Creates a fresh handle to avoid state exhaustion issues.
    ///
    /// Returns the absolute path of the file that was actually written so
    /// callers do not have to re-derive it from `file_path` (OI-0069-003).
    /// UnRAR's header walk can hand back a normalised path that does not
    /// match the caller's spelling byte-for-byte; the returned `PathBuf`
    /// is the canonical sink so a follow-up open cannot drift.
    pub fn extract_file(
        &self,
        file_path: &str,
        dest_path: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
        // Single-file default: preserve both permissions and times, matching
        // the historical behaviour before the flags were threaded through
        // (R0081-0067).
        self.extract_file_with_options(file_path, dest_path, true, true, true)
    }

    /// Same contract as [`Self::extract_file`] plus an explicit `overwrite`
    /// flag and the `preserve_permissions` / `preserve_times` opt-outs. The
    /// returned `PathBuf` is the absolute path the archive entry was written
    /// to (OI-0069-003). The preservation flags are honoured by overriding the
    /// mode/mtime UnRAR stamps on the staged file, matching the bulk path
    /// (R0080-0023 / R0081-0067).
    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &std::path::Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
    ) -> Result<std::path::PathBuf> {
        // R1: the declared size the core now reports is only of interest to
        // the staging-to-memory path; the disk path keeps its `PathBuf`
        // contract unchanged.
        self.extract_file_core(
            file_path,
            dest_path,
            overwrite,
            preserve_permissions,
            preserve_times,
            crate::error::ops::EXTRACT_FILE,
            None,
        )
        .map(|(path, _declared)| path)
    }

    /// Shared single-file extraction core behind
    /// [`Self::extract_file_with_options`] and
    /// [`Self::extract_to_memory_with_limit`]. `op` labels errors for the
    /// calling operation (R0081-0068); `ctx`, when `Some`, installs the UnRAR
    /// data callback so a per-entry byte cap (and/or cancellation) can abort
    /// the decode mid-stream instead of only after the whole entry has been
    /// written to disk (R0081-0065).
    ///
    /// R1: returns the written path **and** the header's declared unpacked
    /// size (`None` when the header carries none), so the staging-to-memory
    /// caller can hold the staged payload to the archive's own declaration
    /// instead of only to the staged file's length.
    #[allow(clippy::too_many_arguments)]
    fn extract_file_core(
        &self,
        file_path: &str,
        dest_path: &std::path::Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        op: &'static str,
        ctx: Option<&mut UnrarExtractContext<'_>>,
    ) -> Result<(std::path::PathBuf, Option<u64>)> {
        // OI-0076-002: resolve through the shared single-entry gate
        // first — existence, uniqueness, and link/directory policy live
        // in validate_single_entry (the former in-method
        // Symlink/HardLink/Directory rejections are deleted; directory
        // entries now error at the gate, R0076-0060).
        let listing = self.list_files()?;
        let validated = crate::security::validate_single_entry(&listing, file_path, op)?;
        let target_id = validated.id();
        let validated_path = validated.path().to_string();

        // Create fresh handle for extraction (avoid state exhaustion).
        // `op` is the caller's own label, so a refusal on this path names
        // extract_file / extract_to_memory / extract_to_stream, not a
        // generic one (R5, ti-581bcda4).
        let fresh = self.fresh_handle(op)?;

        let abs_dest = resolve_dest_path(dest_path)?;

        // R0076-0059: seek by stable listing position, never by name scan.
        let mut entry_idx: usize = 0;
        loop {
            match fresh.read_header_with_flags()? {
                Some((entry, flags)) => {
                    // 3b4d15: a continuation header is the same file resumed in the
                    // next volume, not another entry. `walk_entries` folds these into
                    // their predecessor, so the cached listing counts one entry where
                    // UnRAR reports one header per volume — and this walk has to fold
                    // them the same way or its index runs ahead of that listing.
                    //
                    // They only reach this walk when an entry is SKIPPED rather than
                    // extracted: `ExtractCurrentFile` merges the volumes itself and
                    // consumes every part, while `ProcessFile`'s `RAR_SKIP` branch on
                    // a non-solid archive calls `MergeArchive(..,'L')` and returns
                    // positioned on the next volume's header. Counting that header
                    // would make the drift guard below report a rewrite that never
                    // happened, for an archive `unrar t` calls healthy.
                    //
                    // `entry_idx > 0` mirrors `walk_entries`' "nothing to fold into"
                    // case: a continuation as the very first header means the caller
                    // opened a middle volume directly, and there it stands alone.
                    if flags & RHDF_SPLITBEFORE != 0 && entry_idx > 0 {
                        fresh.skip_entry()?;
                        continue;
                    }

                    let current_idx = entry_idx;
                    entry_idx += 1;
                    if current_idx != target_id {
                        // Skip this file
                        fresh.skip_entry()?;
                        continue;
                    }

                    // OI-0076-002 drift guard: the AD 0065 listing
                    // snapshot can go stale if the archive is rewritten
                    // on disk between listing and extraction — never
                    // extract a mismatched entry.
                    if entry.path != validated_path {
                        return Err(listing_drift_mismatch(
                            current_idx,
                            &validated_path,
                            &entry.path,
                        ));
                    }

                    // R2 (DCR-006 Amendment 4): pre-decode refusal on the
                    // *declared* size, unifying UnRAR's `Cap(n)` trigger
                    // with ZIP's and 7z's. Those two reject when the
                    // central-directory / TOC size exceeds the budget,
                    // before any decode; UnRAR previously only aborted once
                    // *decoded* bytes crossed the budget inside
                    // `UCM_PROCESSDATA`, so an over-declaring entry that
                    // decoded small still succeeded and an honest oversized
                    // entry paid the decode work up to the cap first. The
                    // message shape is `read_entry_to_memory_capped`'s
                    // verbatim so the diagnostic reads the same on all three
                    // staging backends.
                    //
                    // `RARProcessFile` is never invoked for a refused entry.
                    // Entries whose header declares no size skip the check
                    // (no declaration is invented — hard constraint 3) and
                    // stay covered by the mid-decode abort, which also
                    // remains the backstop for headers that under-declare.
                    //
                    // The cap read here is `ctx.entry_cap`, i.e.
                    // `min(per-entry budget, remaining archive budget)` as
                    // `begin_file` computed it. For the single-entry paths
                    // that is exactly the caller's stream/memory budget; a
                    // future caller that constructs a context carrying a
                    // `total_remaining` would have the pre-check judge
                    // against the combined budget, which is the correct
                    // conservative reading.
                    if let Some(c) = ctx.as_deref() {
                        if let (Some(cap), Some(declared)) = (c.entry_cap, entry.size) {
                            if declared > cap {
                                return Err(ArchiveError::OperationBlocked {
                                    operation: op.to_string(),
                                    reason: format!(
                                        "Entry '{}' declares {} bytes; exceeds the configured per-entry limit of {} bytes",
                                        validated_path, declared, cap
                                    ),
                                });
                            }
                        }
                    }

                    // Sanitize the validated listing path to prevent
                    // traversal attacks; the returned PathBuf is the
                    // path actually written (OI-0069-003).
                    let safe_path = sanitize_entry_path(&validated_path, &abs_dest)?;

                    // R0076-0005: create *and* re-verify containment.
                    if let Some(parent) = safe_path.parent() {
                        let canonical_dest = crate::security::canonicalize_dest_base(&abs_dest)?;
                        crate::security::create_parent_dirs_verified(
                            parent,
                            &canonical_dest,
                            &validated_path,
                        )?;
                    }

                    unrar_extract_atomic(
                        fresh.handle,
                        &safe_path,
                        overwrite,
                        &self.path,
                        op,
                        preserve_permissions,
                        preserve_times,
                        ctx,
                        fresh.callback_context_ptr(),
                    )?;
                    // R1: hand the header's declaration back so the caller
                    // can hold the staged payload to it.
                    return Ok((safe_path, entry.size));
                }
                None => {
                    return Err(ArchiveError::format(
                        Some(ArchiveFormat::Rar),
                        format!("File '{}' not found in archive", file_path),
                    ));
                }
            }
        }
    }

    /// Extract a single file to memory
    ///
    /// Note: this extracts to a temporary directory and reads the result
    /// back into memory.
    ///
    /// That is **not** an UnRAR limitation, though this comment used to say so.
    /// The vendored SDK hands each decoded block to the `UCM_PROCESSDATA`
    /// callback for any operation other than `RAR_SKIP`, and this crate already
    /// relies on that callback elsewhere to abort mid-decode on a size cap. The
    /// real obstacle is adapting a *push* callback into a *pull* `Read`, which
    /// is ordinary work rather than an upstream wall (DEF-004).
    ///
    /// Concurrency: the staging directory is owned by `tempfile::TempDir`,
    /// which generates a collision-free name and removes the entire tree
    /// on drop (R0069-0028 / R0069-0029). The previous timestamp+pid
    /// scheme could collide between parallel calls in the same process
    /// and, on collision, the legacy `TempDirGuard` would unlink content
    /// it did not create.
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.extract_to_memory_with_limit(file_path, None)
    }

    /// Extract a single file to memory with an optional caller-provided
    /// per-entry size cap.
    ///
    /// `max_bytes`:
    /// - `None` — keep the legacy "anything that fits in `usize`"
    ///   behavior. Inspection-time CRC walks pass `None` because the
    ///   archive-level zip-bomb gate has already accepted the entry.
    /// - `Some(cap)` — abort with `OperationBlocked` if the staged
    ///   extracted file's actual length exceeds `cap`. This catches the
    ///   case where a malformed RAR underreports the entry size to the
    ///   facade's metadata gate but the decoder produces a much larger
    ///   payload (R0072-0006). Callers in possession of an
    ///   `ExtractionLimits::max_file_size` budget should pass it.
    pub fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        self.extract_to_memory_with_limit_op(
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_MEMORY,
        )
    }

    /// R5 (ti-581bcda4): [`Self::extract_to_memory_with_limit`] with an
    /// explicit operation label.
    ///
    /// The RAR staging path raises `OperationBlocked` / `Corruption` errors
    /// that used to be hard-labelled `extract_to_memory` even when the
    /// caller had invoked `Archive::extract_to_stream`, contradicting the
    /// R0071-0010 convention (the label names the public operation).
    /// [`Self::extract_to_stream_with_limit`] passes
    /// [`ops::EXTRACT_TO_STREAM`](crate::error::ops::EXTRACT_TO_STREAM);
    /// every memory entry point keeps
    /// [`ops::EXTRACT_TO_MEMORY`](crate::error::ops::EXTRACT_TO_MEMORY).
    pub(crate) fn extract_to_memory_with_limit_op(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
        op: &'static str,
    ) -> Result<Vec<u8>> {
        // Owned, collision-free staging tree. Drops on success and error
        // alike via `TempDir`'s Drop impl.
        let temp_dir = tempfile::Builder::new()
            .prefix("unrar_mem_")
            .tempdir()
            .map_err(|e| ArchiveError::io("create_temp_dir", std::env::temp_dir(), e))?;

        // R0081-0065: wire a cap-only extract context so the per-entry byte
        // budget aborts the decode mid-stream (via the `UCM_PROCESSDATA`
        // callback) instead of only rejecting the finished tempfile after it
        // has been fully written to disk. No progress callback and no
        // archive-wide budget here — just the per-entry cap. `None` keeps the
        // legacy uncapped inspection-walk behaviour.
        // R5 (ti-581bcda4): `op`, so a cap abort raised while serving
        // `extract_to_stream` is labelled with that operation.
        let mut cap_ctx = max_bytes.map(|cap| {
            let mut c = UnrarExtractContext::new(op, None, 0, None);
            c.begin_file(Some(cap));
            c
        });

        // Extract and trust the path the core returns — UnRAR may normalise
        // the entry path differently from the caller's spelling, so
        // re-deriving it via `sanitize_entry_path(file_path, ...)` can drift
        // to a sibling that was never written (OI-0069-003).
        //
        // R0001-0057: call the core on `self`. `extract_file_core` opens its
        // own fresh handle for the walk and only uses the receiver for the
        // memoised `list_files()` snapshot and `self.path`, so pre-opening a
        // handle here bought nothing but a wasted native open and an extra
        // trip through the process-wide UnRAR lock.
        // R5 (ti-581bcda4): `op`, not the hard-coded memory label.
        // R1: `declared_from_listing` is the header's unpacked size (`None`
        // when the header carries none).
        let (extracted_path, declared_from_listing) = self.extract_file_core(
            file_path,
            temp_dir.path(),
            true,
            true,
            true,
            op,
            cap_ctx.as_mut(),
        )?;
        let mut file = std::fs::File::open(&extracted_path)
            .map_err(|e| ArchiveError::io("open_extracted", extracted_path.clone(), e))?;

        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("stat_extracted", extracted_path.clone(), e))?;
        let len = metadata.len();

        // R0072-0006: enforce the caller-supplied per-entry cap before
        // allocating. The facade precheck uses metadata; if the decoder
        // wrote more bytes than promised, we stop here instead of
        // committing the over-sized payload.
        if let Some(cap) = max_bytes {
            if len > cap {
                return Err(ArchiveError::operation_blocked(
                    // R5 (ti-581bcda4): caller's operation label.
                    op,
                    format!(
                        "File '{}' decoded to {} bytes; exceeds the configured per-entry limit of {} bytes",
                        file_path, len, cap
                    ),
                ));
            }
        }

        // R1 (DCR-006 Amendment 4): hold the staged payload to the
        // *listing's* declaration, not only to its own length. The bounded
        // read below compares the staged file against `len` — the staged
        // file's own metadata — so a RAR entry that decoded short (or long)
        // relative to its header sailed through here and only surfaced at
        // read time, via the facade's `with_exact_size` wrapper, and only
        // under `DeclaredSize`. Checking against the header makes RAR
        // truncation and over-production call-time `Corruption`, exactly
        // like ZIP's and 7z's `require_exact` staging, and makes the
        // DCR-006 Amendment 4 classification table honest for all three
        // staging backends.
        //
        // Applied only when the header actually declares a size; for
        // `None`-size entries the facade's read-time exactness check stays
        // the sole authority (hard constraint 3 — no invented declaration).
        check_staged_length(file_path, op, len, declared_from_listing)?;

        // Shared bounds contract (R0072-0006 / R0075-0059): the staged
        // file must deliver exactly the bytes its metadata declared — a
        // racing append or truncation against the process-private
        // staging tree surfaces as corruption instead of silent
        // over/under-read. `temp_dir` drops at return, removing the
        // entire staging tree.
        //
        // R5 (ti-581bcda4): `op` is the caller's operation label.
        crate::ffi::common::read_entry_to_memory_bounded(&mut file, len, file_path, op, None, true)
    }

    /// Extract a single file to a stream
    ///
    /// Note: currently stages the entry to a temporary file on disk, then
    /// loads it into memory before wrapping it in a cursor. Of the three
    /// non-libarchive backends this is the only one that touches disk, so it is
    /// the one that fails the crate's definition of streaming outright rather
    /// than only on the memory axis.
    ///
    /// The previous wording — "the UnRAR API requires extraction to disk first"
    /// — was wrong, and wrong in the direction that stops someone fixing it: it
    /// names an upstream wall where there is none. The SDK emits decoded blocks
    /// through `UCM_PROCESSDATA`; turning that push callback into a pull `Read`
    /// is the actual work (DEF-004).
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.extract_to_stream_with_limit(file_path, None)
    }

    /// Streaming variant of [`Self::extract_to_memory_with_limit`].
    ///
    /// Same RAR-staging caveat: the payload is materialized to disk
    /// before the cursor wraps it, so `max_bytes` is applied to the
    /// actual decoded length, not just the metadata-declared one
    /// (R0072-0006).
    pub fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<crate::streaming::StreamingExtractor> {
        // R5 (ti-581bcda4): every refusal on this path names
        // `extract_to_stream`, the operation the caller invoked.
        let data = self.extract_to_memory_with_limit_op(
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_STREAM,
        )?;
        Ok(crate::streaming::StreamingExtractor::from_bytes(data))
    }

    /// Test archive integrity without extracting to disk
    ///
    /// Uses RAR's test mode to verify CRC32 checksums for all files.
    /// Returns a list of file paths that failed verification.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut failed_files = Vec::new();

        // Open a fresh handle for testing (preserves password for encrypted archives)
        let fresh = self.fresh_handle(crate::error::ops::VALIDATE_INTEGRITY)?;

        // Count of entries actually submitted to RAR_TEST, for the
        // partial-progress error if cursor recovery later fails (R0080-0020).
        let mut tested: usize = 0;

        while let Some(entry) = fresh.read_header()? {
            // R0070-0053: directories and link entries are not part of
            // the regular-file payload, and extraction policy skips
            // them. Including them in the integrity walk would
            // exercise RAR's link-target metadata path instead of the
            // payload CRC32.
            if entry.is_directory()
                || matches!(entry.entry_type, EntryType::Symlink | EntryType::HardLink)
            {
                fresh.skip_entry()?;
                continue;
            }
            tested += 1;

            // Test the file using RAR_TEST mode
            unsafe {
                // Serialize UnRAR FFI calls (global state is not thread-safe).
                let _guard = unrar_lock()?;
                let result = RARProcessFile(
                    fresh.handle,
                    RAR_TEST, // Test mode - verifies CRC32 without extracting
                    std::ptr::null(),
                    std::ptr::null(),
                );

                if result != ERAR_SUCCESS {
                    // R0080-0021: classify the RAR_TEST failure through the
                    // shared error map instead of treating every non-success
                    // as a damaged file. A wrong/missing password or an
                    // I/O-shaped SDK fault is a whole-operation error, not
                    // per-entry CRC corruption, so surface it immediately and
                    // let the caller retry/diagnose. Only a data- or
                    // archive-integrity failure counts as a damaged entry we
                    // record and keep walking past.
                    match result {
                        ERAR_BAD_DATA | ERAR_BAD_ARCHIVE => {
                            failed_files.push(entry.path.clone());
                        }
                        _ => return Err(self.unrar_error(result)),
                    }

                    // R0075-0060: force an explicit RAR_SKIP so the next
                    // `read_header` lands on the following entry rather than
                    // re-reading or stalling on this one.
                    let skip_result =
                        RARProcessFile(fresh.handle, RAR_SKIP, std::ptr::null(), std::ptr::null());
                    if skip_result != ERAR_SUCCESS {
                        // R0080-0020: the recovery skip failed, so the handle
                        // can no longer be advanced and the remaining entries
                        // cannot be tested. Return a typed error carrying how
                        // many entries were tested and the damaged files found
                        // so far — never an `Ok` list the caller would read as
                        // a complete integrity result.
                        return Err(ArchiveError::operation_blocked(
                            crate::error::ops::VALIDATE_INTEGRITY,
                            format!(
                                "RAR integrity walk could not advance past a damaged entry in {} after testing {} entr{} (recovery skip failed: {}); damaged files found so far: [{}]",
                                self.path.display(),
                                tested,
                                if tested == 1 { "y" } else { "ies" },
                                map_unrar_error(skip_result, &self.path),
                                failed_files.join(", ")
                            ),
                        ));
                    }
                    continue;
                }
            }
        }

        Ok(failed_files)
    }
}

impl Drop for UnrarArchive {
    fn drop(&mut self) {
        unsafe {
            // Drop still runs on panic paths — recovering from poison is
            // important. `unrar_lock()` only reports re-entrancy when this same
            // thread already holds `UNRAR_LOCK` (in which case the close is
            // still correctly serialized); either way the handle must be
            // closed, so ignore the guard result rather than leak it or skip
            // the close (R0081-0063).
            let _lock = unrar_lock();
            RARCloseArchive(self.handle);
        }
    }
}

/// Lowest `unp_ver` value UnRAR reports for a RAR5-family file header.
///
/// `ReadHeader50` (arcread.cpp) never forwards the archived algorithm
/// byte: it maps the header's 6-bit algorithm field onto one of three
/// sentinels — `VER_PACK5` (50), `VER_PACK7` (70) or `VER_UNKNOWN`
/// (9999), all defined in headers.hpp. `ReadHeader15` instead stores the
/// raw archived byte (`hd->UnpVer=Raw.Get1()`), which every real RAR4-era
/// writer emits as 10/13/15/20/26/29/36 — all below 50. `>= 50` is
/// therefore the format discriminator, and it keeps working for a future
/// `VER_PACK8`.
const RAR5_MIN_UNP_VER: u32 = 50;

/// Decode the Unix permission bits out of a RAR file header's attribute
/// field, choosing the unpacking by archive format.
///
/// RAR5 stores `st_mode` unshifted in the file header's Attributes vint
/// (`arcread.cpp` `ReadHeader50`: `hd->FileAttr=(uint)Raw.GetV()`), while
/// RAR4 packs it into the upper 16 bits of a 32-bit word (`ReadHeader15`:
/// `hd->FileAttr=Raw.Get4()`). `dll.cpp` forwards both verbatim
/// (`D->FileAttr=hd->FileAttr`), so the caller must pick the shift.
///
/// A genuine Unix `st_mode` always carries file-type bits (`S_IFREG`
/// `0o100000`, `S_IFDIR` `0o040000`, `S_IFLNK` `0o120000`). Their absence
/// means the field holds no Unix mode at all, so report `None` rather
/// than a positive claim of `0o000` — `Some(0)` asserts "this file is
/// readable by nobody", which is strictly worse than admitting the mode
/// is unknown (ticgit b75cafb4, 2026-08-16).
fn unix_mode_from_file_attr(file_attr: u32, unp_ver: u32) -> Option<u32> {
    let candidate = if unp_ver >= RAR5_MIN_UNP_VER {
        file_attr
    } else {
        file_attr >> 16
    };
    if candidate & 0o170000 == 0 {
        None
    } else {
        // Mask to permission bits; the file-type bits are already
        // represented by `entry_type`.
        Some(candidate & 0o7777)
    }
}

/// Parse UnRAR header into ArchiveEntry with CRC32
fn parse_header(header: &RARHeaderDataEx) -> Result<ArchiveEntry> {
    // `RARHeaderDataEx` is `#[repr(C, packed)]` (R0079-0008): fields used
    // through references (slicing, method calls, `format!` captures) must
    // be copied to aligned locals first; plain by-value reads are fine.
    let file_name_w = header.file_name_w;

    // Decode the wide filename per the UnRAR ABI: UTF-16 on Windows,
    // UTF-32 (4-byte `wchar_t`) on macOS, Linux, and BSD. The predicate
    // keys on `windows` vs. not — matching `RarWchar` — so Linux/BSD no
    // longer misdecode a 4-byte array as UTF-16 (R0080-0001).
    let file_name = {
        #[cfg(not(windows))]
        {
            // Off Windows, wchar_t is UTF-32 (4 bytes per character).
            let mut len = 0;
            while len < file_name_w.len() && file_name_w[len] != 0 {
                len += 1;
            }
            let chars: Vec<char> = file_name_w[..len]
                .iter()
                .filter_map(|&code| std::char::from_u32(code))
                .collect();
            chars.iter().collect::<String>()
        }
        #[cfg(windows)]
        {
            // On Windows, wchar_t is UTF-16 (2 bytes per character).
            let mut len = 0;
            while len < file_name_w.len() && file_name_w[len] != 0 {
                len += 1;
            }
            String::from_utf16_lossy(&file_name_w[..len])
        }
    };

    // Normalize path (convert backslashes to forward slashes)
    let normalized_path = normalize_path(&file_name);

    // Determine entry type. R0075-0057: classify redirection /
    // link entries before directories so a junction-style entry
    // whose `RHDF_DIRECTORY` flag is set is reported as a link
    // (matching ZIP/7z ordering and the facade's link policy).
    let is_directory_flag = (header.flags & RHDF_DIRECTORY) != 0;
    // RAR5 redir_type: FSREDIR_UNIXSYMLINK=1, FSREDIR_WINSYMLINK=2,
    //   FSREDIR_JUNCTION=3, FSREDIR_HARDLINK=4, FSREDIR_FILECOPY=5
    let entry_type = if header.redir_type == 1 || header.redir_type == 2 || header.redir_type == 3 {
        EntryType::Symlink
    } else if header.redir_type == 4 || header.redir_type == 5 {
        EntryType::HardLink
    } else if is_directory_flag {
        EntryType::Directory
    } else {
        EntryType::File
    };
    let is_directory = entry_type == EntryType::Directory;

    // Combine 64-bit sizes
    let unp_size = ((header.unp_size_high as u64) << 32) | (header.unp_size as u64);
    let pack_size = ((header.pack_size_high as u64) << 32) | (header.pack_size as u64);

    // Convert DOS timestamp to SystemTime
    let modified = dos_time_to_system_time(header.file_time);

    // Use high-resolution mtime if available, otherwise fall back to the
    // DOS-format file_time. `filetime_to_system_time` returns None for
    // pre-1970 FILETIMEs instead of collapsing them to UNIX_EPOCH.
    let modified = filetime_to_system_time(header.mtime_low, header.mtime_high).or(modified);

    let mut entry = ArchiveEntry::file(normalized_path, 0).build();
    entry.entry_type = entry_type;
    entry.size = if is_directory { None } else { Some(unp_size) };
    entry.compressed_size = if is_directory { None } else { Some(pack_size) };
    entry.modified = modified;
    // CRC32 value 0 is valid (e.g. empty files) — only skip for directories
    entry.crc32 = if is_directory {
        None
    } else {
        Some(header.file_crc)
    };
    // R0075-0056: `header.file_attr` is host-OS-dependent. On Windows
    // hosts (host_os == 0 || host_os == 2 — the same predicate used for
    // the `attributes.windows` field below) it carries Windows file
    // attributes; on Unix hosts it carries a Unix `st_mode`. Only
    // surface the Unix permission bits in `permissions` when the entry
    // came from a Unix-style host; Windows attribute bits land in
    // `attributes.windows` instead.
    //
    // Corrected 2026-08-16 (ticgit b75cafb4): the `st_mode` *packing* is
    // format-dependent, not unconditional. RAR5 stores the mode
    // unshifted, RAR4 in the upper 16 bits, so the old unconditional
    // `>> 16` decoded every RAR5 entry to `Some(0)` — e.g.
    // `tests/fixtures/test.rar` records the Attributes vint
    // `a4 83 02` = 33188 = 0o100644, which `>> 16` flattens to zero.
    // `unp_ver` is the discriminator because the UnRAR DLL API exposes
    // no archive-format field at all: `RARHeaderDataEx` has none,
    // `RAROpenArchiveDataEx.Flags` carries no RARFMT15/RARFMT50 bit,
    // and `dll.cpp` normalises both formats' host OS through one line
    // (`D->HostOS = hd->HSType==HSYS_WINDOWS ? HOST_WIN32 : HOST_UNIX`)
    // so `host_os` cannot discriminate either. See
    // `unix_mode_from_file_attr` for the sentinel rationale.
    //
    // Alternatives deliberately rejected, so the next reader need not
    // re-litigate them:
    //   * Sniffing the `Rar!\x1A\x07\x01\x00` signature at open and
    //     caching an `is_rar5` flag. Workable (SFX payloads are staged
    //     from the payload offset, so the signature sits at offset 0 of
    //     whatever `UnrarArchive` opens), but it inserts a second read
    //     of `self.path` into `open_with_mode`'s identity window — the
    //     window R0081-0064 reasons about byte by byte. Too much risk
    //     for a metadata field.
    //   * Patching the vendored unrar to export `Arc.Format`. Forks
    //     upstream and creates rebase debt for one metadata bit.
    //   * A "retry the other shift if this one looks wrong" heuristic.
    //     Silent auto-correction makes the field unexplainable.
    let is_windows_host = header.host_os == 0 || header.host_os == 2;
    // Copies — the struct is `#[repr(C, packed)]`, so taking references
    // into its fields is E0793.
    let file_attr = header.file_attr;
    let unp_ver = header.unp_ver;
    entry.permissions = if is_windows_host {
        None
    } else {
        unix_mode_from_file_attr(file_attr, unp_ver)
    };

    // Phase 1: Enhanced metadata

    // Creation time (ctime) — pre-1970 returns None instead of UNIX_EPOCH.
    entry.created = filetime_to_system_time(header.ctime_low, header.ctime_high);

    // Access time (atime) — same pre-epoch policy.
    entry.accessed = filetime_to_system_time(header.atime_low, header.atime_high);

    // Encryption status
    entry.is_encrypted = (header.flags & RHDF_ENCRYPTED) != 0;

    // Comment is not available via RARHeaderDataEx (cmt_buf/cmt_size are not populated by UnRAR)
    entry.comment = None;

    // Platform-specific attributes
    use crate::entry::FileAttributes;
    entry.attributes = Some(FileAttributes {
        windows: if is_windows_host {
            Some(header.file_attr)
        } else {
            None
        },
        unix_xattr: None, // UnRAR doesn't expose xattrs directly
        archive_specific: Some(format!(
            "host_os={} method={} unp_ver={}",
            // Copies — `format!` captures by reference, which is not
            // allowed into packed fields.
            { header.host_os },
            { header.method },
            { header.unp_ver }
        )),
    });

    Ok(entry)
}

/// Convert DOS timestamp to SystemTime
fn dos_time_to_system_time(dos_time: u32) -> Option<SystemTime> {
    if dos_time == 0 {
        return None;
    }

    // DOS time format: bits 0-4=seconds/2, 5-10=minutes, 11-15=hours
    // DOS date format: bits 16-20=day, 21-24=month, 25-31=year-1980
    let seconds = ((dos_time & 0x1F) * 2) as u64;
    let minutes = ((dos_time >> 5) & 0x3F) as u64;
    let hours = ((dos_time >> 11) & 0x1F) as u64;
    let day = ((dos_time >> 16) & 0x1F) as u64;
    let month = ((dos_time >> 21) & 0x0F) as u64;
    let year = (((dos_time >> 25) & 0x7F) + 1980) as u64;

    super::common::ymd_hms_to_system_time(year, month, day, hours, minutes, seconds)
}

/// Convert dest_path to an absolute path (thread-safe, avoids changing cwd)
fn resolve_dest_path(dest_path: &Path) -> Result<std::path::PathBuf> {
    dest_path.canonicalize().or_else(|_| {
        if dest_path.is_absolute() {
            Ok(dest_path.to_path_buf())
        } else {
            Ok(std::env::current_dir()
                .map_err(|e| ArchiveError::io("getcwd", dest_path.to_path_buf(), e))?
                .join(dest_path))
        }
    })
}

/// Walked entry name does not match the gate-validated listing name at
/// the consumed index (OI-0076-002 drift guard) — never touch a
/// mismatched entry's payload.
fn listing_drift_mismatch(index: usize, expected: &str, found: &str) -> ArchiveError {
    ArchiveError::format(
        Some(ArchiveFormat::Rar),
        format!(
            "single-entry listing drift at index {}: expected '{}', found '{}'",
            index, expected, found
        ),
    )
}

/// R0001-0019: the fresh bulk walk produced more live entries than the
/// gate-validated listing holds — the archive grew on disk after
/// listing. Shares the "listing drift" vocabulary of the libarchive
/// bulk guard (R0080-0009 / R0080-0016).
fn listing_drift_extra(index: usize, listing_len: usize) -> ArchiveError {
    ArchiveError::format(
        Some(ArchiveFormat::Rar),
        format!(
            "listing drift: archive entry index {} exceeds the validated listing length {}",
            index, listing_len
        ),
    )
}

/// R0001-0019: the fresh bulk walk ended before consuming every index
/// the gate-validated listing holds — the archive was truncated or
/// rewritten after listing. Cardinality half of the drift guard.
fn listing_drift_eof(visited: usize, listing_len: usize) -> ArchiveError {
    ArchiveError::format(
        Some(ArchiveFormat::Rar),
        format!(
            "listing drift: archive ended after {} entries but the validated listing holds {}",
            visited, listing_len
        ),
    )
}

/// Reason the UnRAR data callback aborted an in-flight `RARProcessFile`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UnrarAbort {
    /// The caller's progress callback voted to cancel (R0080-0022).
    Cancelled,
    /// A per-entry or archive-wide decoded-byte cap was exceeded
    /// (R0080-0008).
    CapExceeded,
    /// The UnRAR data callback reported a negative processed-length, which
    /// signals ABI/state corruption rather than a real data block. We abort
    /// and surface corruption instead of silently coercing the length to zero
    /// and continuing to decode on corrupt state (R0081-0069).
    CallbackError,
    /// UnRAR asked for the next volume of a multi-volume set and it is not
    /// available (`UCM_CHANGEVOLUME*` with `RAR_VOL_ASK`). Answering
    /// anything but `-1` there makes the SDK retry the same name
    /// indefinitely, under the process-wide lock (AD 0019), so the
    /// trampoline aborts and this records why.
    MissingVolume,
}

/// Callback state that lives as long as an UnRAR handle.
///
/// Boxed and owned by [`UnrarArchive`], and registered before
/// `RAROpenArchiveEx` reads the main header — the SDK copies
/// `RAROpenArchiveDataEx::callback` / `user_data` into its `Cmd` there
/// (`dll.cpp`), so this is installed for the whole life of the handle and
/// every SDK call on it has a callback. Two defects needed exactly that:
///
/// * ticgit 3f8790 — a header-encrypted (`-hp`) archive keeps its main
///   header inside a HEAD_CRYPT block. Without a password *during the
///   open*, `Archive::IsArchive` cannot decrypt it, so `Arc.Protected`
///   stays false and `open_data.flags` comes back without
///   `ROADF_RECOVERY` — and without every other main-header flag.
///   `RARSetPassword` afterwards does not revisit that flags word. The
///   password now arrives through `UCM_NEEDPASSWORD` while the header is
///   being read.
/// * ticgit 03ddc6 — the extract path installs its own callback for the
///   duration of one `RARProcessFile` and clears it after, so a missing
///   volume met during a *listing* walk's `RAR_SKIP` found no callback at
///   all. `DllVolChange` then took its own no-callback branch, which sets
///   `ERAR_EOPEN` without asking, and the typed `MissingVolume`
///   diagnostic could not reach the caller.
struct UnrarHandleContext {
    /// Password to answer `UCM_NEEDPASSWORD` with, already NUL-terminated
    /// for the SDK's `char[]` buffer. `None` for an archive opened
    /// without one, in which case the SDK's own missing-password error
    /// stands.
    password: Option<CString>,
    /// Why the callback aborted, if it did.
    ///
    /// `Cell` because the trampoline reaches this through a raw pointer
    /// while the owner is only borrowed immutably. Sound for the same
    /// reason the extract context is: AD 0019 serialises every UnRAR call
    /// — including the library's synchronous call back into us — onto the
    /// one thread holding `UNRAR_LOCK`, and `UnrarArchive` is `!Sync`.
    abort: Cell<Option<UnrarAbort>>,
}

impl UnrarHandleContext {
    fn new(password: Option<&str>) -> Result<Box<Self>> {
        let password = match password {
            None => None,
            Some(pw) => Some(
                CString::new(pw)
                    .map_err(|_| ArchiveError::password("Password contains null byte"))?,
            ),
        };
        Ok(Box::new(Self {
            password,
            abort: Cell::new(None),
        }))
    }

    /// Take the recorded abort reason, if any, as a typed error.
    ///
    /// Mirrors [`UnrarExtractContext::take_abort_error`] so a caller sees
    /// the same diagnostic whether the volume ran out during an extract
    /// (extract context installed) or during a listing walk (this one).
    fn take_abort_error(&self, archive_path: &Path) -> Option<ArchiveError> {
        self.abort
            .take()
            .and_then(|abort| shared_abort_error(abort, archive_path))
    }
}

/// Registered through `RAROpenArchiveDataEx` for a handle's whole life.
///
/// Deliberately narrow: it answers the password request and applies the
/// volume-change policy, and returns the non-abort default for everything
/// else. Payload accounting stays in [`unrar_process_callback`], which the
/// extract path installs over this one for the duration of a single
/// `RARProcessFile` and then restores this one after.
///
/// # Safety
/// `user_data` must be the `*const UnrarHandleContext` registered with the
/// handle, valid for as long as the handle is open.
unsafe extern "C" fn unrar_handle_callback(
    msg: c_uint,
    user_data: isize,
    p1: isize,
    p2: isize,
) -> c_int {
    let outcome = std::panic::catch_unwind(|| {
        if user_data == 0 {
            // No context to consult. Abort a volume request anyway — the
            // SDK's own no-callback branch would do the same thing with a
            // less specific error, and a retry loop under the
            // process-wide lock is worse than an unlabelled failure.
            if msg == UCM_CHANGEVOLUME || msg == UCM_CHANGEVOLUMEW {
                return if p2 == RAR_VOL_NOTIFY { 1 } else { -1 };
            }
            return 1;
        }
        // SAFETY: the pointer registered at open, per the contract above.
        // Shared reference only — the context's one mutable field is a
        // `Cell`.
        let ctx = unsafe { &*(user_data as *const UnrarHandleContext) };

        if msg == UCM_CHANGEVOLUME || msg == UCM_CHANGEVOLUMEW {
            if p2 == RAR_VOL_NOTIFY {
                return 1;
            }
            ctx.abort.set(Some(UnrarAbort::MissingVolume));
            return -1;
        }

        if msg == UCM_NEEDPASSWORD {
            let Some(password) = ctx.password.as_ref() else {
                // Let the SDK raise its own `ERAR_MISSING_PASSWORD`
                // rather than inventing an answer.
                return 1;
            };
            let bytes = password.as_bytes_with_nul();
            // `p2` is the buffer's capacity in `char`s, including room for
            // the terminator. Refuse rather than truncate: a silently
            // shortened password is a wrong password, and the SDK would
            // report it as one.
            if p1 == 0 || p2 <= 0 || bytes.len() > p2 as usize {
                return 1;
            }
            // SAFETY: `p1` is the SDK's `char PasswordA[MAXPASSWORD]`
            // stack buffer and `p2` its element count; the length check
            // above keeps the copy inside it.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p1 as *mut u8, bytes.len());
            }
            return 1;
        }

        // `UCM_NEEDPASSWORDW` is deliberately not answered. The SDK asks
        // wide-first and falls through to the ANSI request when the wide
        // buffer comes back empty (`Archive::RequestArcPassword`), and it
        // zero-initialises that buffer before asking — so declining here
        // routes the request to the arm above and keeps this trampoline
        // free of `wchar_t`-width handling, which differs between
        // platforms.
        1
    });
    outcome.unwrap_or(-1)
}

/// Per-extraction context handed to the UnRAR C library through
/// `RARSetCallback`'s `LPARAM` user-data slot. The trampoline
/// ([`unrar_process_callback`]) casts the pointer back to `&mut Self` on
/// every `UCM_PROCESSDATA` block.
///
/// AD 0019: every UnRAR FFI call — including the library's synchronous
/// invocation of our trampoline from inside `RARProcessFile` — runs on the
/// one thread holding `UNRAR_LOCK`. The context is therefore only ever
/// touched by a single thread at a time, so no `Send`/`Sync` bound or
/// interior-mutability guard is required; a plain `&mut` reached through the
/// raw pointer is sound.
struct UnrarExtractContext<'a> {
    /// Public operation label used when mapping an abort to an error.
    op: &'static str,
    /// Caller progress callback for rate-limited cancellation polling
    /// (`None` when the caller supplied none).
    progress: Option<&'a mut Box<dyn ProgressCallback>>,
    /// Shared throttle for both the between-entry poll and the in-callback
    /// poll, so a tight data stream cannot call back per block.
    rate_limiter: RateLimiter,
    /// Bytes accounted before the current entry (progress numerator base).
    bytes_before: u64,
    /// Progress denominator (sum of the selected entries' declared sizes).
    total_bytes: u64,
    /// Decoded bytes for the current entry so far (reset per entry).
    entry_decoded: u64,
    /// Effective decoded-byte ceiling for the current entry —
    /// `min(max_file_size, remaining archive budget)` (`None` = unbounded).
    entry_cap: Option<u64>,
    /// Remaining archive-wide decoded-byte budget (`None` = unbounded).
    total_remaining: Option<u64>,
    /// Set by the trampoline when it returns `-1` to abort the current
    /// `RARProcessFile`.
    abort: Option<UnrarAbort>,
}

impl<'a> UnrarExtractContext<'a> {
    fn new(
        op: &'static str,
        progress: Option<&'a mut Box<dyn ProgressCallback>>,
        total_bytes: u64,
        max_total_size: Option<u64>,
    ) -> Self {
        Self {
            op,
            progress,
            rate_limiter: RateLimiter::new(),
            bytes_before: 0,
            total_bytes,
            entry_decoded: 0,
            entry_cap: None,
            total_remaining: max_total_size,
            abort: None,
        }
    }

    /// Prepare per-entry state before extracting a file payload. The
    /// effective per-entry ceiling is the smaller of the configured
    /// per-file cap and whatever archive-wide budget remains, mirroring
    /// libarchive's `min(entry_cap, remaining_total)` (R0080-0013).
    fn begin_file(&mut self, max_file_size: Option<u64>) {
        self.entry_decoded = 0;
        self.abort = None;
        self.entry_cap = min_opt(max_file_size, self.total_remaining);
    }

    /// Fold the just-extracted entry's actual decoded bytes into the
    /// archive-wide budget, then advance the progress numerator by the
    /// entry's declared size (keeping it on the same declared-size basis
    /// as `total_bytes`).
    fn finish_file(&mut self, declared_size: u64) {
        if let Some(rem) = self.total_remaining.as_mut() {
            *rem = rem.saturating_sub(self.entry_decoded);
        }
        self.advance_declared(declared_size);
    }

    /// Advance the progress numerator by an entry's declared size (used for
    /// directories and other entries that occupy a slot in the denominator
    /// without a decoded payload).
    fn advance_declared(&mut self, declared_size: u64) {
        self.bytes_before = self.bytes_before.saturating_add(declared_size);
    }

    /// Rate-limited between-entry cancellation poll. Mirrors
    /// [`crate::ffi::common::check_extraction_cancelled`] but shares this
    /// context's rate limiter with the in-callback poll.
    fn poll_between_entries(&mut self) -> Result<()> {
        if let Some(cb) = self.progress.as_mut() {
            if self.rate_limiter.should_call() {
                if let std::ops::ControlFlow::Break(()) =
                    cb.on_progress(self.bytes_before, Some(self.total_bytes))
                {
                    return Err(ArchiveError::Cancelled { operation: self.op });
                }
            }
        }
        Ok(())
    }

    /// Unconditional final 100% callback (R0070-0038 parity).
    fn poll_final(&mut self) -> Result<()> {
        if let Some(cb) = self.progress.as_mut() {
            if let std::ops::ControlFlow::Break(()) =
                cb.on_progress(self.total_bytes, Some(self.total_bytes))
            {
                return Err(ArchiveError::Cancelled { operation: self.op });
            }
        }
        Ok(())
    }

    /// Handle one `UCM_PROCESSDATA` block: account decoded bytes, enforce
    /// the byte cap, then run the rate-limited cancellation poll. Returns
    /// the C callback code (`1` continue, `-1` abort). Only ever called on
    /// the lock-holding thread (AD 0019).
    fn on_process_data(&mut self, block: u64) -> c_int {
        self.entry_decoded = self.entry_decoded.saturating_add(block);

        if let Some(cap) = self.entry_cap {
            if self.entry_decoded > cap {
                self.abort = Some(UnrarAbort::CapExceeded);
                return -1;
            }
        }

        if let Some(cb) = self.progress.as_mut() {
            if self.rate_limiter.should_call() {
                let processed = self
                    .bytes_before
                    .saturating_add(self.entry_decoded)
                    .min(self.total_bytes);
                if let std::ops::ControlFlow::Break(()) =
                    cb.on_progress(processed, Some(self.total_bytes))
                {
                    self.abort = Some(UnrarAbort::Cancelled);
                    return -1;
                }
            }
        }
        1
    }

    /// Translate a recorded abort into the typed error the facade expects.
    /// Cancellation becomes [`ArchiveError::Cancelled`]; a cap violation
    /// becomes the same `OperationBlocked` shape every other backend uses
    /// for byte-cap breaches (R0080-0008). Returns `None` when the callback
    /// did not abort (an ordinary decode failure maps via
    /// [`map_unrar_error`] instead).
    fn take_abort_error(&mut self, archive_path: &Path) -> Option<ArchiveError> {
        match self.abort.take() {
            Some(UnrarAbort::Cancelled) => Some(ArchiveError::Cancelled { operation: self.op }),
            Some(UnrarAbort::CapExceeded) => Some(ArchiveError::operation_blocked(
                self.op,
                format!(
                    "RAR entry in {} decoded {} bytes, exceeding the configured extraction limit of {} bytes",
                    archive_path.display(),
                    self.entry_decoded,
                    self.entry_cap.unwrap_or(0)
                ),
            )),
            Some(other) => shared_abort_error(other, archive_path),
            None => None,
        }
    }
}

/// The abort reasons whose diagnostic needs nothing but the archive path,
/// so both callback contexts report them identically.
///
/// A missing volume can be met by either trampoline — the extract one when
/// a payload spans volumes, the handle one when a listing walk skips past a
/// split entry (ticgit 03ddc6) — and the caller must not be able to tell
/// which was installed. Keeping the wording in one place is also why the
/// recovery advice below can be trusted: it named a method that did not
/// exist once already.
///
/// Returns `None` for the reasons that need the extract context's own
/// counters; those are reported by
/// [`UnrarExtractContext::take_abort_error`].
fn shared_abort_error(abort: UnrarAbort, archive_path: &Path) -> Option<ArchiveError> {
    match abort {
        // R0081-0069: a negative processed-length surfaced by the
        // trampoline is neither a byte-cap breach nor a cancellation — it
        // is ABI/state corruption, so map it to a corruption error.
        UnrarAbort::CallbackError => Some(ArchiveError::corruption(
            archive_path.display().to_string(),
            "UnRAR data callback reported a negative processed-length; aborting to avoid masking ABI/state corruption",
        )),
        // A volume the set references is absent, so the entry's data is
        // truncated at the set boundary — the same class as a central
        // directory that ends mid-record, hence `Corruption` rather than
        // `Io`. The message points at the typed parser instead of guessing
        // a name here: the callback receives the wanted volume in `p1`, but
        // as a platform-width `wchar` buffer for the `W` message that
        // arrives first, and `VolumeSetReport::defects()` already answers
        // "which volume" precisely and portably.
        UnrarAbort::MissingVolume => Some(ArchiveError::corruption(
            archive_path.display().to_string(),
            "UnRAR asked for the next volume of this multi-volume set and it is not available; \
             aborted rather than retrying the same volume name under the process-wide UnRAR lock. \
             Call unified_archive::format::multipart::parse_volume_set on the sibling paths and \
             read VolumeSetReport::defects() to learn which volume is missing",
        )),
        UnrarAbort::Cancelled | UnrarAbort::CapExceeded => None,
    }
}

/// Hold a staged RAR payload to the size its file header declared.
///
/// R1 (DCR-006 Amendments 4 and 5): the bounded read that follows compares the
/// staged file against its own metadata, so a payload that decoded short or
/// long *relative to its header* used to sail through and surface only at read
/// time, and only under `StreamBound::DeclaredSize`. Checking it here makes RAR
/// truncation and over-production a call-time [`ArchiveError::Corruption`],
/// matching ZIP's and 7z's `require_exact` staging.
///
/// `declared == None` (a header that states no size) returns `Ok(())`: no
/// declaration is invented, and the facade's read-time exactness check remains
/// the sole authority for those entries.
///
/// Extracted from the call site so the decision is unit-testable — tripping it
/// end to end would need a RAR whose header lies about its unpacked size, which
/// cannot be produced without byte-patching a fixture and recomputing its
/// header CRC.
fn check_staged_length(
    file_path: &str,
    op: &'static str,
    staged_len: u64,
    declared: Option<u64>,
) -> Result<()> {
    if let Some(declared) = declared {
        if staged_len != declared {
            return Err(ArchiveError::corruption(
                file_path,
                format!(
                    "{}: staged payload is {} bytes, listing declares {}",
                    op, staged_len, declared
                ),
            ));
        }
    }
    Ok(())
}

/// `min` over two optional ceilings: `None` means "unbounded", so the result
/// is bounded whenever either input is.
fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// Panic-safe UnRAR data-callback trampoline (R0080-0022).
///
/// Registered via [`RARSetCallback`] for the duration of a single
/// `RARProcessFile`; `user_data` is the `*mut UnrarExtractContext` we
/// registered alongside it. The Rust body runs inside
/// [`std::panic::catch_unwind`] and returns `-1` on panic so an unwind can
/// never cross the C frame (which would be undefined behaviour).
///
/// AD 0019: UnRAR invokes this synchronously from `RARProcessFile` on the
/// lock-holding thread, so the context is single-threaded despite being
/// reached through a raw pointer.
unsafe extern "C" fn unrar_process_callback(
    msg: c_uint,
    user_data: isize,
    _p1: isize,
    p2: isize,
) -> c_int {
    let outcome = std::panic::catch_unwind(|| {
        // Volume change is handled explicitly, because "keep the default
        // behaviour" is not what a non-abort answer means here. See
        // [`RAR_VOL_ASK`] and `volume.cpp`'s `DllVolChange`: returning
        // `>= 0` without rewriting the name buffer makes the SDK retry the
        // same volume name indefinitely, and that spin holds AD 0019's
        // process-wide lock the whole time, stalling every RAR operation in
        // the process rather than just this one.
        if msg == UCM_CHANGEVOLUME || msg == UCM_CHANGEVOLUMEW {
            // `RAR_VOL_NOTIFY` says the next volume *was* opened. `-1` here
            // makes `DllVolNotify` return false and aborts a valid
            // multi-volume read, so this arm must stay non-negative.
            if p2 == RAR_VOL_NOTIFY {
                return 1;
            }
            // `RAR_VOL_ASK`: the volume is missing. Abort, and record why so
            // `take_abort_error` can name it. Supplying the next path
            // instead is the multi-volume continuation feature, tracked
            // separately (ticgit 61660f / OI-0001-006); it is not something
            // this trampoline can invent.
            if user_data != 0 {
                // SAFETY: as below — the registered context pointer,
                // single-threaded under AD 0019.
                let ctx = unsafe { &mut *(user_data as *mut UnrarExtractContext<'_>) };
                ctx.abort = Some(UnrarAbort::MissingVolume);
            }
            // Abort even with no context to record into: a stalled lock is
            // worse than an unlabelled error.
            return -1;
        }
        if msg != UCM_PROCESSDATA || user_data == 0 {
            // Only data blocks are handled; the remaining messages
            // (password prompt, large-dict notice) keep UnRAR's default
            // behaviour by returning a non-abort code.
            return 1;
        }
        // SAFETY: `user_data` is the context pointer registered just before
        // this `RARProcessFile` call; it outlives the call and is only
        // touched on this thread (AD 0019).
        let ctx = unsafe { &mut *(user_data as *mut UnrarExtractContext<'_>) };
        // R0081-0069: a negative processed-length is not a real data block —
        // it signals ABI/state corruption. Record a callback abort and return
        // -1 so the operation fails with a typed corruption error via
        // `take_abort_error`, instead of coercing the length to zero and
        // continuing to decode on corrupt state.
        if p2 < 0 {
            ctx.abort = Some(UnrarAbort::CallbackError);
            return -1;
        }
        ctx.on_process_data(p2 as u64)
    });
    outcome.unwrap_or(-1)
}

/// Apply the `preserve_*` opt-outs (R0080-0023) to a staged RAR output and
/// flush it durably before install (R0080-0024).
///
/// UnRAR always stamps the archive's mode and mtime onto the file it writes,
/// so honouring `preserve_permissions == false` / `preserve_times == false`
/// means *overriding* what UnRAR applied:
///
/// * permissions off → reset to the staging default the shared atomic writer
///   produces when it preserves nothing (`NamedTempFile`'s `0o600`, matching
///   `write_entry_atomically` with `unix_mode == None`).
/// * times off → stamp the current time, since the payload is already on disk
///   carrying the archive mtime UnRAR set.
///
/// On Unix the archive mode may be read-only, which would block the
/// read/write reopen needed to set the mtime, so the mode is captured, forced
/// reopen-friendly, then reapplied through the handle. Finally `sync_all`
/// flushes the payload plus any metadata just changed.
fn finalize_staged_file(
    temp_path: &Path,
    preserve_permissions: bool,
    preserve_times: bool,
) -> Result<()> {
    // Capture the mode UnRAR applied and force a mode we can definitely
    // reopen read/write. We own this just-created O_EXCL temp, so a
    // path-based chmod is both permitted (owner) and safe from symlink swaps
    // (no other name resolves to this inode).
    #[cfg(unix)]
    let final_mode: u32 = {
        use std::os::unix::fs::PermissionsExt;
        let applied = std::fs::metadata(temp_path)
            .map_err(|e| ArchiveError::io("stat_staged", temp_path.to_path_buf(), e))?
            .permissions()
            .mode()
            & 0o7777;
        std::fs::set_permissions(temp_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| ArchiveError::io("set_permissions", temp_path.to_path_buf(), e))?;
        // preserve → restore UnRAR's mode; opt-out → keep the NamedTempFile
        // staging default (0o600), mirroring the shared atomic writer when it
        // preserves no `unix_mode` (R0080-0023).
        if preserve_permissions { applied } else { 0o600 }
    };
    #[cfg(not(unix))]
    let _ = preserve_permissions;

    let staged = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(temp_path)
        .map_err(|e| ArchiveError::io("open_staged", temp_path.to_path_buf(), e))?;

    // R0080-0023: opt out of time preservation by overriding UnRAR's archive
    // mtime with the current time. A later `chmod` only touches ctime, so it
    // does not disturb this mtime.
    if !preserve_times {
        staged
            .set_modified(SystemTime::now())
            .map_err(|e| ArchiveError::io("set_times", temp_path.to_path_buf(), e))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .set_permissions(std::fs::Permissions::from_mode(final_mode))
            .map_err(|e| ArchiveError::io("set_permissions", temp_path.to_path_buf(), e))?;
    }

    // R0080-0024: flush data and the metadata we may have just changed before
    // the caller renames the sibling into place.
    staged
        .sync_all()
        .map_err(|e| ArchiveError::io("fsync_staged", temp_path.to_path_buf(), e))?;
    Ok(())
}

/// Destination path handed to the UnRAR SDK for a `RAR_EXTRACT`, held in
/// the platform's own path encoding (AD 0064 Option A).
///
/// Unix keeps the raw `OsStr` bytes in a `CString` and drives the narrow
/// `RARProcessFile`: `dll.cpp`'s `ProcessFile` runs `DestName` through
/// `CharToWide`, which maps bytes it cannot decode into the private-use
/// area, and `WideToCharMap` restores them verbatim when the file is
/// created, so the bytes survive the round trip.
///
/// Windows keeps the UTF-16 code units — including the unpaired
/// surrogates a Win32 path may legally carry — and drives
/// `RARProcessFileW`. The narrow entry point is lossy on Windows even for
/// paths Rust can render: the same `ProcessFile` pushes `DestName`
/// through `OemToExt` and then `MultiByteToWideChar(CP_ACP, ...)`, so
/// anything outside the active ANSI code page is destroyed before the SDK
/// ever opens the file. The wide destination is stored verbatim into
/// `Cmd.DllDestName`, so the two entry points are otherwise identical.
struct UnrarDestName {
    #[cfg(not(windows))]
    narrow: CString,
    #[cfg(windows)]
    wide: Vec<RarWchar>,
}

impl UnrarDestName {
    /// Encode `path` for the SDK, rejecting an interior NUL: the C side
    /// stops at the first zero code unit, so it would silently operate on
    /// a *prefix* of the path — a different file — rather than fail.
    fn new(path: &Path) -> Result<Self> {
        #[cfg(not(windows))]
        {
            Ok(Self {
                narrow: crate::ffi::common::path_to_cstring_checked(path)?,
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;

            let mut wide: Vec<RarWchar> = path.as_os_str().encode_wide().collect();
            if wide.contains(&0) {
                return Err(ArchiveError::invalid_path(
                    path.display().to_string(),
                    "Contains null byte",
                ));
            }
            wide.push(0);
            Ok(Self { wide })
        }
    }

    /// Run `operation` on `handle`'s current entry with this destination.
    ///
    /// # Safety
    ///
    /// `handle` must be a live UnRAR handle positioned on an entry header
    /// and the caller must hold [`UNRAR_LOCK`] (see [`unrar_lock`]).
    unsafe fn process(&self, handle: RARHandle, operation: c_int) -> c_int {
        #[cfg(not(windows))]
        {
            unsafe { RARProcessFile(handle, operation, std::ptr::null(), self.narrow.as_ptr()) }
        }
        #[cfg(windows)]
        {
            unsafe { RARProcessFileW(handle, operation, std::ptr::null(), self.wide.as_ptr()) }
        }
    }
}

/// Atomically extract a single RAR file entry to `safe_path`.
///
/// UnRAR writes the file directly via `RARProcessFile`, so we cannot share
/// `AtomicOutputFile` (which owns the file handle). We mirror its semantics
/// by giving UnRAR a sibling tempfile path; on success the staged file is
/// finalised ([`finalize_staged_file`]) and renamed into place via
/// `rename_with_overwrite` / `persist_noclobber`. On any failure the
/// `TempPath` drops and removes the partial file, so the destination is
/// never left half-extracted.
///
/// When `ctx` is `Some`, an UnRAR data callback is installed for the
/// duration of the `RARProcessFile` call so caller cancellation and the
/// decoded-byte caps can abort mid-entry (R0080-0022 / R0080-0008); it is
/// cleared before the handle is used again. AD 0019: the callback fires on
/// this same lock-holding thread, so the raw context pointer is never shared
/// across threads.
///
/// Metadata: UnRAR stamps the entry's modification time (`SetCloseFileTime`)
/// and attributes (`SetFileAttr` → `chmod` on Unix) onto the staged file.
/// `preserve_permissions` / `preserve_times == false` are honoured by
/// [`finalize_staged_file`], which overrides those stamps (R0080-0023). The
/// staged file is then `fsync`ed and, after the rename, the parent directory
/// is synced (R0080-0024).
#[allow(clippy::too_many_arguments)]
fn unrar_extract_atomic(
    handle: RARHandle,
    safe_path: &Path,
    overwrite: bool,
    archive_path: &Path,
    op: &'static str,
    preserve_permissions: bool,
    preserve_times: bool,
    mut ctx: Option<&mut UnrarExtractContext<'_>>,
    handle_context: isize,
) -> Result<()> {
    if !overwrite && safe_path.exists() {
        return Err(ArchiveError::OperationBlocked {
            // R0081-0068: use the caller's operation label instead of a
            // hard-coded "extract" so single-file vs. bulk callers keep
            // their own context.
            operation: op.to_string(),
            reason: format!("Destination file already exists: {}", safe_path.display()),
        });
    }

    let parent = safe_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let temp = NamedTempFile::new_in(parent)
        .map_err(|e| ArchiveError::io("create_temp", parent.to_path_buf(), e))?;
    let temp_path = temp.into_temp_path();

    // AD 0064 Option A: hand the SDK the staging path in the platform's own
    // encoding. `to_string_lossy` replaced every non-UTF-8 byte of the
    // destination's parent directory with U+FFFD, so UnRAR was pointed at a
    // path that is not this tempfile — usually `ERAR_ECREATE`, and where the
    // mangled directory happens to exist, a silent zero-byte extraction
    // (nothing between here and `persist`/`rename` checks the staged length).
    let dest_name = UnrarDestName::new(&temp_path)?;

    let result = unsafe {
        let _guard = unrar_lock()?;
        // Install the progress/cancellation callback for the duration of this
        // single `RARProcessFile`. AD 0019: the trampoline runs on this same
        // lock-holding thread, so the raw context pointer is never shared
        // across threads.
        if let Some(ctx) = ctx.as_deref_mut() {
            RARSetCallback(
                handle,
                Some(unrar_process_callback),
                ctx as *mut UnrarExtractContext<'_> as isize,
            );
        }
        let r = dest_name.process(handle, RAR_EXTRACT);
        // Restore the handle-lifetime callback rather than clearing to
        // `None`. Clearing used to be right when nothing else needed one;
        // it is not now, because the walks *between* extractions have to
        // keep answering volume requests (ticgit 03ddc6) — a later
        // `RAR_SKIP` past a split entry would otherwise meet the SDK's
        // no-callback branch again. The extract context's pointer is a
        // local of this function, so it must not stay registered past this
        // point either way.
        RARSetCallback(handle, Some(unrar_handle_callback), handle_context);
        r
    };

    if result != ERAR_SUCCESS {
        // If the callback aborted for a known reason, surface that
        // (cancellation / cap violation) instead of UnRAR's generic
        // user-break code.
        if let Some(ctx) = ctx {
            if let Some(err) = ctx.take_abort_error(archive_path) {
                return Err(err);
            }
        }
        return Err(map_unrar_error(result, archive_path));
    }

    // R0080-0023 / R0080-0024: apply the `preserve_*` opt-outs and flush the
    // staged payload durably before install. The temp is a just-created
    // O_EXCL sibling (`NamedTempFile`), so reopening it by path cannot follow
    // a symlink an attacker planted — no other name resolves to this inode.
    finalize_staged_file(&temp_path, preserve_permissions, preserve_times)?;

    if overwrite {
        // R0070-0019: route the cross-platform replace through the
        // shared `rename_with_overwrite` helper. Previously the
        // Windows fork did `remove_file` + `persist`, which leaves
        // the destination missing if `persist` fails. The shared
        // helper uses `MoveFileExW` on Windows so the swap is
        // atomic and the original survives a persist failure.
        let temp_pathbuf_ref = temp_path.to_path_buf();
        let owned = temp_path.keep().map_err(|e| {
            ArchiveError::io(
                "persist_temp",
                temp_pathbuf_ref.clone(),
                std::io::Error::other(e.to_string()),
            )
        })?;
        if let Err(e) = crate::ffi::common::rename_with_overwrite(&owned, safe_path) {
            let _ = std::fs::remove_file(&owned);
            return Err(e);
        }
    } else {
        temp_path
            .persist_noclobber(safe_path)
            .map_err(|e| ArchiveError::io("rename", safe_path.to_path_buf(), e.error))?;
    }

    // R0080-0024: fsync the parent directory so the freshly renamed entry is
    // durably observable across crash recovery (both install branches).
    crate::ffi::common::sync_parent_dir(safe_path)?;

    Ok(())
}

/// Map UnRAR error code to ArchiveError
fn map_unrar_error(code: c_int, path: &Path) -> ArchiveError {
    let path_display = path.display().to_string();
    match code {
        ERAR_NO_MEMORY => ArchiveError::format(Some(ArchiveFormat::Rar), "Out of memory"),
        ERAR_BAD_DATA => ArchiveError::corruption(
            path_display,
            "CRC32 checksum verification failed - archive is corrupted",
        ),
        ERAR_BAD_ARCHIVE => {
            ArchiveError::format(Some(ArchiveFormat::Rar), "Not a valid RAR archive")
        }
        ERAR_UNKNOWN_FORMAT => ArchiveError::format(None, "Unknown archive format"),
        // `ERAR_EOPEN` covers any open failure — missing file, permission
        // denied, path encoding issue. Surface it as a generic I/O open
        // error (`Other`) so callers don't see misleading `NotFound`
        // for permission failures (R0069-0031). If the path actually
        // doesn't exist, the OS still gets a chance to surface that via
        // a separate `stat` round-trip on the caller side.
        ERAR_EOPEN => ArchiveError::io(
            "open",
            path.to_path_buf(),
            std::io::Error::other(format!(
                "UnRAR ERAR_EOPEN: cannot open archive at {}",
                path_display
            )),
        ),
        // I/O-shaped SDK failures map to `ArchiveError::Io` with the
        // failing operation named, preserving retryable I/O classification
        // instead of collapsing to a generic format error (R0080-0051).
        ERAR_EREAD => ArchiveError::io(
            "read",
            path.to_path_buf(),
            std::io::Error::other(format!(
                "UnRAR ERAR_EREAD: read failed for {}",
                path_display
            )),
        ),
        ERAR_EWRITE => ArchiveError::io(
            "write",
            path.to_path_buf(),
            std::io::Error::other(format!(
                "UnRAR ERAR_EWRITE: write failed for {}",
                path_display
            )),
        ),
        ERAR_ECREATE => ArchiveError::io(
            "create",
            path.to_path_buf(),
            std::io::Error::other(format!(
                "UnRAR ERAR_ECREATE: cannot create output for {}",
                path_display
            )),
        ),
        ERAR_ECLOSE => ArchiveError::io(
            "close",
            path.to_path_buf(),
            std::io::Error::other(format!(
                "UnRAR ERAR_ECLOSE: close failed for {}",
                path_display
            )),
        ),
        ERAR_MISSING_PASSWORD => ArchiveError::password("Password required"),
        ERAR_BAD_PASSWORD => ArchiveError::password("Wrong password"),
        _ => ArchiveError::format(
            Some(ArchiveFormat::Rar),
            format!("UnRAR error code: {}", code),
        ),
    }
}

/// Outcome of trying to completely fill a buffer from a reader.
enum FillOutcome {
    /// The reader was at EOF before any byte was read (a clean block
    /// boundary).
    Eof,
    /// The buffer was filled completely.
    Full,
    /// EOF arrived after some — but not all — bytes were read.
    Partial,
}

/// Read up to `buf.len()` bytes, distinguishing a clean EOF at the start
/// (zero bytes available — a valid end of the block chain) from a
/// truncated read (some bytes available, then EOF — corruption). Unlike
/// `read_exact`, a boundary EOF is reported as [`FillOutcome::Eof`]
/// rather than an error, so block walkers can stop without treating
/// truncation as "nothing here" (R0080-0052 / R0080-0059).
fn fill_or_eof<R: std::io::Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<FillOutcome> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    if filled == 0 {
        Ok(FillOutcome::Eof)
    } else if filled == buf.len() {
        Ok(FillOutcome::Full)
    } else {
        Ok(FillOutcome::Partial)
    }
}

/// Parse RAR5 recovery record blocks.
///
/// Walks the RAR5 block chain via streaming vint reads
/// ([`read_rar5_vint`]) so blocks with arbitrary header sizes are
/// handled correctly (R0075-0061 — the previous fixed 50-byte prefix
/// returned `None` for blocks whose vint fields ran past that window).
/// Recovery records are service blocks (type 3) with name `"RR"`;
/// end-of-archive is type 5. Generic over `Read + Seek` so unit tests
/// can drive the walk with synthetic block sequences; `archive_path`
/// is only used for error context.
///
/// The percentage value itself is extracted heuristically — RAR5 pins
/// the recovery percentage to a specific extra-area record, but the
/// public spec doesn't fix that record's offset within the service
/// header's content. We tighten the previous "scan first 1024 bytes for
/// any value in 1..=15" pattern (R0075-0062) to scan only the bytes
/// *after* the verified `"RR"` name, which rejects accidental matches in
/// unrelated fields.
///
/// The value is a vint, not a byte: RAR 6.10 changed the encoding when it
/// raised the maximum recovery record from 99% to 1000% (see
/// [`rar5_service_recovery_percent`], which quotes the vendored UnRAR
/// comment). The range this can legitimately return is therefore `1..=1000`,
/// which is why the signature is `Option<u16>` (ticgit 7ca208) and not the
/// `Option<u8>` it carried until 0.5.0.
fn parse_rar5_recovery<R: std::io::Read + std::io::Seek>(
    archive_path: &Path,
    file: &mut R,
) -> Result<Option<u16>> {
    use std::io::SeekFrom;

    // Archive length, used to reject block offsets that run past the end
    // of the file before seeking to them (R0080-0055).
    let file_len = file
        .seek(SeekFrom::End(0))
        .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

    // Skip RAR5 signature (8 bytes: "Rar!\x1A\x07\x01\x00")
    file.seek(SeekFrom::Start(8))
        .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

    loop {
        let block_pos = file
            .stream_position()
            .map_err(|e| ArchiveError::io("tell", archive_path, e))?;

        // CRC32(4) | HeaderSize(vint) | header_body.
        // A clean zero-byte read at a block boundary means the walk ran
        // off the end without finding a recovery record; a partial read
        // (or any other I/O error) is truncation/corruption and must not
        // masquerade as "no recovery record" (R0080-0052).
        let mut crc_buf = [0u8; 4];
        match fill_or_eof(file, &mut crc_buf)
            .map_err(|e| ArchiveError::io("read", archive_path, e))?
        {
            FillOutcome::Eof => return Ok(None),
            FillOutcome::Full => {}
            FillOutcome::Partial => {
                return Err(ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR5 block header truncated inside CRC field",
                ));
            }
        }

        // The CRC bytes were consumed, so a malformed or truncated
        // header-size vint here is corruption, not "no recovery record"
        // (R0080-0053).
        let header_size = read_rar5_vint(file)?;
        // header_body_start: position right after the HeaderSize vint —
        // i.e. the byte where HeaderType begins. Used both to compute
        // next_block (block_pos + crc(4) + vint_len(HeaderSize) +
        // header_size + data) and to bound how much of this header we may
        // inspect.
        let header_body_start = file
            .stream_position()
            .map_err(|e| ArchiveError::io("tell", archive_path, e))?;
        let header_size_vint_len = header_body_start - (block_pos + 4);

        // R0081-0071: verify the RAR5 header checksum before treating any
        // field as authoritative. The vendored UnRAR's `GetCRC50` computes the
        // standard CRC32 over everything after the 4-byte CRC field — the
        // HeaderSize vint bytes plus the `header_size`-byte header body — and
        // compares it to the leading `crc_buf`. Bound the region by the
        // archive length first so a bogus HeaderSize cannot drive a huge read.
        let crc_region_len = header_size_vint_len
            .checked_add(header_size)
            .ok_or_else(|| {
                ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR5 header size overflowed",
                )
            })?;
        let crc_region_end = (block_pos + 4).checked_add(crc_region_len).ok_or_else(|| {
            ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR5 header extends past the addressable range",
            )
        })?;
        if crc_region_end > file_len {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR5 header extends past end of archive",
            ));
        }
        file.seek(SeekFrom::Start(block_pos + 4))
            .map_err(|e| ArchiveError::io("seek", archive_path, e))?;
        let mut crc_region = vec![0u8; crc_region_len as usize];
        file.read_exact(&mut crc_region)
            .map_err(|e| ArchiveError::io("read", archive_path, e))?;
        if crc32fast::hash(&crc_region) != u32::from_le_bytes(crc_buf) {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR5 block header CRC mismatch",
            ));
        }
        // Resume parsing the header body from just after the HeaderSize vint.
        file.seek(SeekFrom::Start(header_body_start))
            .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

        let header_type = read_rar5_vint(file)?;
        let header_flags = read_rar5_vint(file)?;

        // Optional ExtraSize / DataSize vints in the standard header tail.
        // The extra area is where the recovery percentage lives, so unlike
        // the previous version of this walk it must not be discarded.
        let extra_area_size = if header_flags & 0x0001 != 0 {
            read_rar5_vint(file)?
        } else {
            0
        };
        let data_area_size = if header_flags & 0x0002 != 0 {
            read_rar5_vint(file)?
        } else {
            0
        };

        // End-of-archive marker
        if header_type == 5 {
            return Ok(None);
        }

        if header_type == 3 {
            // Service header. Read the remaining header bytes (up to
            // 4 KiB) into memory and look for the "RR" name.
            let consumed_in_header = file
                .stream_position()
                .map_err(|e| ArchiveError::io("tell", archive_path, e))?
                .saturating_sub(header_body_start);
            // A header declaring fewer bytes than we have already consumed
            // is corrupt; reject it rather than saturating to an empty
            // tail and parsing from a bogus boundary (R0080-0054).
            if consumed_in_header > header_size {
                return Err(ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR5 service header declares fewer bytes than already consumed",
                ));
            }
            let remaining = header_size - consumed_in_header;
            // `header_size` was CRC-verified above, so `remaining` is the
            // archive's own declaration rather than an attacker's free
            // choice. Cap the allocation anyway: a service header this
            // large is not an RR header, and refusing to allocate for it
            // is cheaper than reading it to find that out.
            if remaining > RAR5_SERVICE_HEADER_READ_CAP {
                return Ok(None);
            }
            let mut tail = vec![0u8; remaining as usize];
            file.read_exact(&mut tail)
                .map_err(|e| ArchiveError::io("read", archive_path, e))?;

            if let Some(raw) = rar5_service_recovery_percent(&tail, extra_area_size) {
                // ticgit 7ca208: this used to be a `u8::try_from`, so every
                // archive built with `rar -rr256p` or above — legal since
                // RAR 6.10 raised the ceiling from 99% to 1000% — fell into
                // the documented "percentage cannot be determined" branch
                // and reported `None`. It never truncated; it discarded.
                // `u16` carries the whole 1..=1000 range the format defines.
                //
                // The `try_from` stays because the source is a `u64` vint
                // with no upper bound of its own: a value past `u16::MAX` is
                // an order of magnitude beyond anything RAR can produce, so
                // it is a malformed extra-area record rather than a number
                // to report, and `None` remains the honest answer for it.
                return Ok(u16::try_from(raw).ok().filter(|pct| *pct > 0));
            }
            return Ok(None);
        }

        // Compute the next block's starting offset:
        //   block_pos + crc32(4) + vint_len(header_size) + header_size + data.
        // Every component is attacker-controlled, so add with overflow
        // checks, demand strict forward progress, and reject offsets past
        // the archive end before seeking (R0080-0055 / R0080-0056).
        let next_block = block_pos
            .checked_add(4)
            .and_then(|v| v.checked_add(header_size_vint_len))
            .and_then(|v| v.checked_add(header_size))
            .and_then(|v| v.checked_add(data_area_size))
            .ok_or_else(|| {
                ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR5 next-block offset overflowed",
                )
            })?;
        if next_block <= block_pos {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR5 block did not advance (non-monotonic next-block offset)",
            ));
        }
        if next_block > file_len {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR5 block extends past end of archive",
            ));
        }
        file.seek(SeekFrom::Start(next_block))
            .map_err(|e| ArchiveError::io("seek", archive_path, e))?;
    }
}

/// Parse RAR4 recovery record blocks.
///
/// RAR4 uses a legacy block format with fixed-size headers.
/// Recovery records have block type 0x78. Generic over `Read + Seek`
/// so unit tests can drive the walk with synthetic block sequences;
/// `archive_path` is only used for error context.
///
/// Returns `Option<u16>` to match [`parse_rar5_recovery`] and the public
/// signature above it (ticgit 7ca208), but the widening buys nothing on
/// this path: RAR4 stores no percentage field at all — the number is
/// derived from the recovery/total block counts below and capped at 100,
/// which fit a `u8` and always did. Only the RAR 6.10 vint in the RAR5
/// walk needs the extra range.
fn parse_rar4_recovery<R: std::io::Read + std::io::Seek>(
    archive_path: &Path,
    file: &mut R,
) -> Result<Option<u16>> {
    use std::io::SeekFrom;

    // RAR4 signature is 7 bytes: "Rar!\x1A\x07\x00"
    // After signature comes the main archive header, then file/service blocks

    // R0081-0072: capture the archive length once so the ADD_SIZE data-area
    // skip can be bounded (mirroring the RAR5 walk after R0080-0055) rather
    // than seeking blindly past EOF.
    let file_len = file
        .seek(SeekFrom::End(0))
        .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

    // Seek past signature (already read 16 bytes, go back to start of blocks at offset 7)
    file.seek(SeekFrom::Start(7))
        .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

    // Parse blocks until we find recovery record (0x78) or EOF
    loop {
        // Read RAR4 block header (7 bytes minimum)
        // Format: HEAD_CRC(2) | HEAD_TYPE(1) | HEAD_FLAGS(2) | HEAD_SIZE(2)
        let mut block_header = [0u8; 7];
        match fill_or_eof(file, &mut block_header)
            .map_err(|e| ArchiveError::io("read", archive_path, e))?
        {
            // A clean boundary EOF ends the walk; a partial header is
            // truncation/corruption, not a silent stop (R0080-0059).
            FillOutcome::Eof => return Ok(None),
            FillOutcome::Full => {}
            FillOutcome::Partial => {
                return Err(ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR4 block header truncated",
                ));
            }
        }

        let head_crc = u16::from_le_bytes([block_header[0], block_header[1]]);
        let head_type = block_header[2];
        let head_flags = u16::from_le_bytes([block_header[3], block_header[4]]);
        let head_size = u16::from_le_bytes([block_header[5], block_header[6]]);

        // A RAR4 block header is at minimum the 7-byte basic header; a
        // smaller declared size is corruption and would make the skip
        // arithmetic below underflow into a bogus or backward seek
        // (R0080-0060).
        if head_size < 7 {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR4 block header smaller than 7-byte minimum",
            ));
        }

        // R0081-0070: buffer the rest of the header (everything after the
        // 7-byte basic header, up to `head_size`) and verify HEAD_CRC before
        // trusting any field or seek decision. The vendored UnRAR's `GetCRC15`
        // is the low 16 bits of the standard CRC32 over the header from
        // HEAD_TYPE to the end of the `head_size`-byte header (the data area
        // that follows a LONG_BLOCK is not covered). A truncated header here
        // is corruption, not a clean stop.
        let mut header_rest = vec![0u8; (head_size - 7) as usize];
        file.read_exact(&mut header_rest)
            .map_err(|e| ArchiveError::io("read", archive_path, e))?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&block_header[2..7]);
        hasher.update(&header_rest);
        if (hasher.finalize() & 0xFFFF) as u16 != head_crc {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR4 block header CRC mismatch",
            ));
        }

        // Recovery record block (type 0x78). Its recovery counts live in the
        // header bytes just read.
        if head_type == 0x78 {
            // RAR4 recovery record structure:
            // Bytes 0-3: Total blocks
            // Bytes 4-7: Recovery blocks
            // Percentage = (recovery_blocks / total_blocks) * 100
            if header_rest.len() >= 8 {
                let total_blocks = u32::from_le_bytes([
                    header_rest[0],
                    header_rest[1],
                    header_rest[2],
                    header_rest[3],
                ]);
                let recovery_blocks = u32::from_le_bytes([
                    header_rest[4],
                    header_rest[5],
                    header_rest[6],
                    header_rest[7],
                ]);

                if total_blocks > 0 {
                    // Prevent integer overflow - cap at 100%. The cap, not
                    // the return type, is what bounds this path: it stayed
                    // at 100 when the type widened to `u16` (ticgit 7ca208)
                    // because RAR4 predates the RAR 6.10 extended range and
                    // a ratio of blocks cannot mean 1000% in any case.
                    let percentage =
                        ((recovery_blocks as u64 * 100) / total_blocks as u64).min(100) as u16;
                    return Ok(Some(percentage));
                }
            }

            // Could not determine exact percentage
            return Ok(None);
        }

        // A LONG_BLOCK (0x8000) carries a 4-byte ADD_SIZE data area following
        // the header; every other block has no data area. The header bytes
        // (including the ADD_SIZE field) were already consumed above, so only
        // the data area still needs skipping.
        let data_area: u64 = if head_flags & 0x8000 != 0 {
            // LONG_BLOCK: the 4-byte ADD_SIZE field lives right after the
            // basic header, so the header must be at least 11 bytes. A smaller
            // size with the flag set is corruption (R0080-0060).
            if head_size < 11 {
                return Err(ArchiveError::corruption(
                    archive_path.display().to_string(),
                    "RAR4 long block header smaller than 11-byte minimum",
                ));
            }
            u32::from_le_bytes([
                header_rest[0],
                header_rest[1],
                header_rest[2],
                header_rest[3],
            ]) as u64
        } else {
            0
        };

        // R0081-0072: bound the data-area seek by the archive length before
        // seeking (like the RAR5 walk after R0080-0055). Forward progress is
        // already guaranteed by the `head_size >= 7` / `>= 11` guards, which
        // advance the cursor by at least the header size each iteration.
        let after_header = file
            .stream_position()
            .map_err(|e| ArchiveError::io("tell", archive_path, e))?;
        let next_offset = after_header.checked_add(data_area).ok_or_else(|| {
            ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR4 next-block offset overflowed",
            )
        })?;
        if next_offset > file_len {
            return Err(ArchiveError::corruption(
                archive_path.display().to_string(),
                "RAR4 block extends past end of archive",
            ));
        }
        file.seek(SeekFrom::Start(next_offset))
            .map_err(|e| ArchiveError::io("seek", archive_path, e))?;

        // Check for end of archive marker (type 0x7B)
        if head_type == 0x7B {
            return Ok(None);
        }

        // NOTE: HEAD_FLAGS bit 0x8000 is LONG_BLOCK ("ADD_SIZE present"),
        // not a volume-end marker — every RAR4 file header sets it.
        // Treating it as a terminator killed the walk at the first file
        // block, before the trailing 0x78 recovery record (R0079-0025).
        // Termination is the 0x7B end-of-archive block above or EOF.
    }
}

/// Streaming variant of [`decode_vint`] that pulls one byte at a time
/// from a `Read`. Used by RAR5 block-walking code that no longer
/// pre-loads a fixed-size header prefix (R0075-0061): the vint
/// continues until the high bit clears or 10 bytes have been read,
/// whichever comes first.
///
/// On EOF mid-vint the function surfaces the underlying I/O error
/// rather than returning a partial value.
/// Largest RAR5 service header this walk will read into memory while
/// looking for a recovery record. The RR header is a few dozen bytes; the
/// cap only exists so a legitimately huge service header (a large archive
/// comment, say) is skipped instead of allocated.
const RAR5_SERVICE_HEADER_READ_CAP: u64 = 64 * 1024;

/// RAR5 extra-area record type carrying a service header's subdata array
/// (`FHEXTRA_SUBDATA` in the vendored UnRAR `headers5.hpp`).
const RAR5_FHEXTRA_SUBDATA: u64 = 0x07;

/// Read a RAR5 vint out of a byte slice, returning the value and the number
/// of bytes consumed. `None` if the slice ends mid-vint or the value does
/// not fit a `u64`.
fn slice_rar5_vint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (index, &byte) in bytes.iter().take(10).enumerate() {
        if shift >= 64 {
            return None;
        }
        value |= u64::from(byte & 0x7F).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
        shift += 7;
    }
    None
}

/// Decode the recovery-record percentage from a RAR5 service header.
///
/// `tail` is the header body from just after the optional ExtraSize /
/// DataSize vints to the end of the header; `extra_area_size` is that
/// header's declared extra-area length, which occupies `tail`'s last
/// `extra_area_size` bytes.
///
/// The percentage is **not** a byte to be scanned for. The vendored UnRAR
/// reads it in `arcread.cpp`'s `HEAD_SERVICE` arm, from the `FHEXTRA_SUBDATA`
/// (0x07) extra-area record of the service header whose name is `"RR"`:
///
/// ```text
/// RawPercent.Read(hd->SubData.data(), hd->SubData.size());
/// RecoveryPercent = (int)RawPercent.GetV();
/// ```
///
/// with the upstream comment "It is stored as a single byte up to RAR 6.02
/// and as vint since 6.10, where we extended the maximum RR size from 99% to
/// 1000%". Returns the raw vint, so the caller decides what to do with a
/// value that does not fit its return type.
///
/// This replaces a scan that returned the first byte in `1..=15` found after
/// the literal `"RR"`. That byte was the extra record's own *size* field, so
/// every recovery-bearing archive reported the same percentage regardless of
/// the `-rr` value it was built with, and any record above 15% could not be
/// represented at all.
///
/// Returns `None` — never an error — when the header is not an `"RR"` service
/// header or the extra area does not parse. The header bytes are already
/// CRC-verified by the caller, so a parse that does not fit this shape means
/// this is some other service header, not a damaged archive; reporting
/// corruption here would turn unfamiliar-but-valid headers into false damage
/// reports.
fn rar5_service_recovery_percent(tail: &[u8], extra_area_size: u64) -> Option<u64> {
    let mut pos = 0usize;

    // FileFlags, UnpackedSize, Attributes.
    let (file_flags, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;
    let (_unpacked_size, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;
    let (_attributes, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;

    // Optional fixed-width mtime (0x0002) and data CRC32 (0x0004).
    if file_flags & 0x0002 != 0 {
        pos = pos.checked_add(4)?;
    }
    if file_flags & 0x0004 != 0 {
        pos = pos.checked_add(4)?;
    }

    // CompressionInfo, HostOS, then the length-prefixed name.
    let (_compression_info, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;
    let (_host_os, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;
    let (name_size, used) = slice_rar5_vint(tail.get(pos..)?)?;
    pos += used;

    let name_end = pos.checked_add(usize::try_from(name_size).ok()?)?;
    let name = tail.get(pos..name_end)?;
    if name != b"RR" {
        return None;
    }

    // The extra area is the tail's suffix. Deriving its start from the
    // declared size rather than from `name_end` keeps this correct even
    // when a header carries fields this parser does not know about.
    let extra_len = usize::try_from(extra_area_size).ok()?;
    let extra_start = tail.len().checked_sub(extra_len)?;
    if extra_start < name_end {
        return None;
    }
    let mut extra = tail.get(extra_start..)?;

    while !extra.is_empty() {
        let (record_size, size_len) = slice_rar5_vint(extra)?;
        let record_end = size_len.checked_add(usize::try_from(record_size).ok()?)?;
        let record = extra.get(size_len..record_end)?;
        let (record_type, type_len) = slice_rar5_vint(record)?;
        if record_type == RAR5_FHEXTRA_SUBDATA {
            let subdata = record.get(type_len..)?;
            return slice_rar5_vint(subdata).map(|(percent, _)| percent);
        }
        extra = extra.get(record_end..)?;
    }

    None
}

pub(crate) fn read_rar5_vint<R: std::io::Read>(reader: &mut R) -> Result<u64> {
    let mut value = 0u64;
    let mut shift: u32 = 0;
    for _ in 0..10 {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Rar5), format!("RAR5 vint read: {}", e))
        })?;
        let byte = buf[0];
        let data_bits = u64::from(byte & 0x7F);

        if shift >= 64 {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::Rar5),
                "Invalid vint encoding: shift would exceed u64 width",
            ));
        }
        match data_bits.checked_shl(shift) {
            Some(v) => value |= v,
            None => {
                return Err(ArchiveError::format(
                    Some(ArchiveFormat::Rar5),
                    "Invalid vint encoding: shift overflow on data bits",
                ));
            }
        }

        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }

    Err(ArchiveError::format(
        Some(ArchiveFormat::Rar5),
        "Invalid vint encoding: exceeds 10-byte cap",
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod staged_length_tests;
