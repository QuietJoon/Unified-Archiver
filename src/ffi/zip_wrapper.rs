//! Native Rust ZIP backend using the `zip` crate
//!
//! This backend provides fast ZIP operations with direct CRC32 access from metadata.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::format::ArchiveFormat;
use crate::options::ProgressCallback;
use crate::password::Password;
use crate::security::{
    canonicalize_dest_base, multiple_entries_reason, sanitize_entry_path,
    sanitize_entry_path_with_base,
};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use zip::ZipArchive as RawZipArchive;

mod aes;
mod raw_directory;

use aes::{crc32_check_exempt, drain_entry_crc32_counted};
use raw_directory::{RawCentralDirectory, RawRecord, read_central_directory_exact};

use super::common::{
    StagedEntryWrite, check_extraction_cancelled, entry_cancel_hook, normalize_path,
    read_entry_to_memory_capped, write_entry_atomically,
};

/// Classify a ZIP entry into an [`EntryType`] from its Unix mode bits and
/// a directory-name hint — the single source of truth for the `zip`-crate
/// ZIP backend (this module) so every entry's type stays consistent
/// across listing and extraction (R0081-0076 / R0081-0077).
///
/// When the entry carries Unix mode bits, the `S_IFMT` type nibble is
/// authoritative: symlink (`S_IFLNK`) is tested before directory
/// (`S_IFDIR`) to preserve the backends' symlink-before-directory
/// ordering (R0075-0043); regular files (`S_IFREG`) map to
/// [`EntryType::File`]; and every remaining Unix type — FIFO, character
/// or block device, socket — maps to [`EntryType::Other`] so it is never
/// silently reinterpreted as a directory or a plain file. A mode with no
/// type nibble (`0`) falls through to the name hint.
///
/// Without Unix mode information (e.g. a DOS-hosted entry with empty
/// external attributes), a trailing `/` marks a directory and every other
/// entry is a regular file, matching the ZIP directory convention.
pub(crate) fn classify_zip_entry_type(unix_mode: Option<u32>, name_is_dir: bool) -> EntryType {
    const S_IFMT: u32 = 0o170000;
    const S_IFLNK: u32 = 0o120000;
    const S_IFDIR: u32 = 0o040000;
    const S_IFREG: u32 = 0o100000;

    if let Some(mode) = unix_mode {
        match mode & S_IFMT {
            S_IFLNK => return EntryType::Symlink,
            S_IFDIR => return EntryType::Directory,
            S_IFREG => return EntryType::File,
            0 => {} // no type nibble recorded — fall through to the name hint
            _ => return EntryType::Other, // FIFO / char / block device / socket / ...
        }
    }

    if name_is_dir {
        EntryType::Directory
    } else {
        EntryType::File
    }
}

/// Is this `zip`-crate error the "the entry is encrypted and you supplied
/// no password" condition?
///
/// The crate reports it as `UnsupportedArchive(ZipError::PASSWORD_REQUIRED)`
/// — the same variant it uses for genuinely unsupported archive shapes — so
/// the discriminator is the message constant the crate publishes for exactly
/// this case (`zip::result::ZipError::PASSWORD_REQUIRED`), compared by value
/// rather than by pointer identity.
fn is_password_required(e: &zip::result::ZipError) -> bool {
    matches!(
        e,
        zip::result::ZipError::UnsupportedArchive(message)
            if *message == zip::result::ZipError::PASSWORD_REQUIRED
    )
}

/// Is this the `zip`-crate refusal that means "this entry is WinZip-AES
/// encrypted and the crate was built without its `aes-crypto` feature"?
///
/// Reported as `UnsupportedArchive` with a message, like several other
/// conditions, so the message is the only discriminator the crate offers
/// — there is no published constant for this one the way there is for
/// [`ZipError::PASSWORD_REQUIRED`](zip::result::ZipError::PASSWORD_REQUIRED).
/// Matched on the stable half of the sentence ("AES" plus the feature
/// name) rather than the whole string, so upstream rewording around it
/// does not silently turn this back into a generic format error.
///
/// Always compiled: with `zip-crypto` on, the condition cannot arise and
/// this simply never matches. Keeping it unconditional means the arm is
/// covered by the ordinary build too, rather than existing only in the
/// configuration nobody runs by default.
fn is_aes_feature_missing(e: &zip::result::ZipError) -> bool {
    match e {
        zip::result::ZipError::UnsupportedArchive(message) => {
            message.contains("AES") && message.contains("aes-crypto")
        }
        _ => false,
    }
}

/// Open a ZIP entry by index, using password decryption if provided.
///
/// **Missing-credential classification (ticgit `9bdf2c`).** A read of an
/// encrypted entry through a handle that carries no password is a *missing
/// credential*, not a malformed archive, so it surfaces as
/// [`ArchiveError::Password`] — matching the RAR backend
/// (`ERAR_MISSING_PASSWORD`) and the 7z backend (`Error::PasswordRequired`),
/// and matching what `docs/API_REFERENCE.md` promises callers. It used to
/// surface as [`ArchiveError::Format`] carrying the crate's "Password
/// required to decrypt file" text, so ZIP was the one backend where a
/// caller had to string-match to tell a missing password from corruption.
/// DCR-012 made that user-visible on a second path: the content-multiset
/// digest streams AE-2 entries, so digesting a password-protected ZIP
/// reaches exactly here.
///
/// Only the missing-credential condition moved. A wrong password stays
/// [`ArchiveError::Password`] (the crate's `InvalidPassword`), and every
/// other failure — a truncated local header, an unsupported compression
/// method, a decoder setup failure — stays [`ArchiveError::Format`].
fn open_entry_by_index<'a>(
    zip: &'a mut RawZipArchive<File>,
    index: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a>> {
    let classify = |e: zip::result::ZipError| -> ArchiveError {
        if matches!(e, zip::result::ZipError::InvalidPassword) {
            ArchiveError::password(format!("Invalid password for ZIP entry {}", index))
        } else if is_password_required(&e) {
            ArchiveError::password(format!(
                "Password required to decrypt ZIP entry {}: reopen the archive with its password",
                index
            ))
        } else if is_aes_feature_missing(&e) {
            // The `zip` crate's own message names *its* feature
            // (`aes-crypto`), which is not a feature of this crate — a
            // caller reading it would search our `Cargo.toml` for
            // `aes-crypto` and find nothing. Name ours instead, so the
            // diagnostic is actionable rather than merely accurate
            // (AD 0058 format features).
            ArchiveError::unsupported(
                "extract",
                ArchiveFormat::Zip,
                Some(format!(
                    "ZIP entry {index} is WinZip-AES encrypted and this build cannot decrypt it \
                     (enable the `zip-crypto` Cargo feature)"
                )),
            )
        } else {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read entry {}: {}", index, e),
            )
        }
    };

    if let Some(pw) = password {
        zip.by_index_decrypt(index, pw.as_bytes()).map_err(classify)
    } else {
        zip.by_index(index).map_err(classify)
    }
}

/// R0080-0030 integrity error contract, shared in intent with the
/// 7z `test_integrity` path: a decoder-reported CRC mismatch
/// (`Corruption`) or a corrupt/truncated compressed stream (an `Io` error
/// whose source `ErrorKind` is `InvalidData` / `UnexpectedEof`) is a
/// per-entry integrity failure recorded against the entry path; every
/// other error — a real archive-file read/open failure — propagates.
fn is_integrity_payload_failure(e: &ArchiveError) -> bool {
    match e {
        ArchiveError::Corruption { .. } => true,
        ArchiveError::Io { source, .. } => matches!(
            source.kind(),
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
        ),
        _ => false,
    }
}

/// Drift guard for `ValidatedEntry` id-based seeks (OI-0076-002): the
/// entry at the validated listing index must still carry the validated
/// normalized name — the AD 0065 cached listing can go stale if the
/// file is rewritten on disk. Resolution-by-listing also covers names
/// stored with backslash separators (R0076-0048): the listing is
/// normalized, so the validated name maps to the stored entry's index
/// without a raw-name byte match.
fn check_listing_drift(
    zip: &RawZipArchive<File>,
    target_id: usize,
    validated_path: &str,
) -> Result<()> {
    let found = zip
        .name_for_index(target_id)
        .map(normalize_path)
        .unwrap_or_else(|| "<missing>".to_string());
    if found != validated_path {
        return Err(ArchiveError::format(
            Some(ArchiveFormat::Zip),
            format!(
                "single-entry listing drift at index {}: expected '{}', found '{}'",
                target_id, validated_path, found
            ),
        ));
    }
    Ok(())
}

/// Modification time from the entry's DOS-precision timestamp,
/// converted via the shared calendar helper (the zip crate's
/// `to_time()` needs the "time" feature, which is not enabled).
/// The 2-second-precision baseline behind [`zip_entry_times`].
fn zip_entry_mtime(zip_file: &zip::read::ZipFile) -> Option<std::time::SystemTime> {
    zip_file.last_modified().and_then(|dt| {
        super::common::ymd_hms_to_system_time(
            dt.year() as u64,
            dt.month() as u64,
            dt.day() as u64,
            dt.hour() as u64,
            dt.minute() as u64,
            dt.second() as u64,
        )
    })
}

/// One entry's resolved timestamps, read once from its central-directory
/// record (R0001-0062).
struct ZipEntryTimes {
    modified: Option<std::time::SystemTime>,
    accessed: Option<std::time::SystemTime>,
    created: Option<std::time::SystemTime>,
}

/// Resolve an entry's timestamps from the central-directory record.
///
/// OI-0065-002 (re-homed to the sole ZIP backend at the AD 0007
/// collapse): the DOS timestamp is 2-second precision and carries no
/// access/creation time. When the record carries a `0x5455`
/// extended-timestamp extra field, its 1-second values override
/// `modified` and surface `accessed` / `created` — the behaviour the
/// removed piz reader provided. The `0x5455` values are signed 32-bit
/// Unix seconds (valid below 2038-01-19), so decode through `i32` before
/// widening.
///
/// R0001-0062: listing *and* the extraction-time `preserve_times` capture
/// both go through this helper. Extraction previously took the coarse DOS
/// value straight from [`zip_entry_mtime`], so a restored file's mtime
/// disagreed with the one the listing had just reported. `by_index` and
/// `by_index_raw` expose the same central-directory extra fields, so both
/// call sites see identical input. (This is read-side only — the
/// write-side central-record convention stays as tracked by OI-0081-004.)
fn zip_entry_times(zip_file: &zip::read::ZipFile) -> ZipEntryTimes {
    let mut times = ZipEntryTimes {
        modified: zip_entry_mtime(zip_file),
        accessed: None,
        created: None,
    };
    for field in zip_file.extra_data_fields() {
        if let zip::extra_fields::ExtraField::ExtendedTimestamp(ts) = field {
            if let Some(secs) = ts.mod_time() {
                times.modified = super::common::unix_seconds_to_system_time(secs as i32 as i64);
            }
            if let Some(secs) = ts.ac_time() {
                times.accessed = super::common::unix_seconds_to_system_time(secs as i32 as i64);
            }
            if let Some(secs) = ts.cr_time() {
                times.created = super::common::unix_seconds_to_system_time(secs as i32 as i64);
            }
        }
    }
    times
}

/// Native Rust ZIP archive wrapper
///
/// Provides fast ZIP operations with CRC32 from metadata (no decompression needed).
///
/// **Caching (D4 / R0068-0033):** the open `RawZipArchive<File>` is
/// memoised across calls under a [`std::sync::Mutex`]. The first listing /
/// extraction call pays the file open + central-directory parse; later
/// callers reuse the same handle. The Archive layer is `!Sync`, so the
/// mutex is uncontended in normal use — it just enforces interior
/// mutability against the zip crate's `&mut self` requirements.
/// The parsed listing is additionally memoised in `listing`
/// (AD 0065 / OI-0065-003) and shared out via `Arc::clone`.
///
/// **Duplicate-collapse rejection, and the one raw index behind it
/// (R0079-0026 / DCR-009 — AD 0007 collapse; widened by OI-0001-003).**
/// The `zip` crate keys its central-directory map by exact name, so two
/// records sharing a byte-identical name collapse to a single listing
/// entry (the last record wins). The removed piz reader kept every record,
/// so its listing carried both and the single-entry gate refused the
/// ambiguity. `raw_directory` restores that by memoising ONE
/// `RawCentralDirectory` index (crate-internal, hence not a link) — read
/// through the same open descriptor
/// every extraction reads through — and every operation consults it:
/// the by-name single-entry paths refuse an ambiguous name, and the paths
/// that consume the whole collapsed view (bulk extraction, the integrity
/// walk, an id-addressed stream) refuse the archive instead of silently
/// omitting a shadowed record.
///
/// **File-identity binding (OI-0001-002).** The cached listing describes
/// one particular file, so the handle is bound to that file's identity —
/// captured by `fstat` of the cached descriptor itself — and the binding
/// is re-checked at the single place this backend can ever re-resolve the
/// pathname, `open_zip_bound`. See that method for why ZIP's window is the
/// narrowest of the four read backends.
pub struct ZipArchive {
    path: PathBuf,
    password: Option<Password>,
    cached_zip: std::sync::Mutex<Option<RawZipArchive<File>>>,
    listing: once_cell::sync::OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// Memoised raw-central-directory index (R0079-0026 / DCR-009 /
    /// OI-0001-003). Populated lazily on the first operation that consults
    /// it, which since OI-0001-003 is any listing, extraction or integrity
    /// call rather than only a by-name single-entry extraction.
    raw_directory: once_cell::sync::OnceCell<RawCentralDirectory>,
    /// Identity of the file this handle is bound to (OI-0001-002), taken
    /// from the descriptor the cached `RawZipArchive` reads through. Bound
    /// on the first cache population and compared on every later one; see
    /// [`ZipArchive::open_zip_bound`].
    ///
    /// A `OnceCell` to match the two memo cells beside it, not because a
    /// race needs guarding: both writers run under the `cached_zip` mutex,
    /// so the cell is only ever touched by one thread at a time.
    identity: once_cell::sync::OnceCell<crate::fs_identity::FileIdentity>,
}

impl ZipArchive {
    /// Open ZIP archive for reading.
    ///
    /// **Validation timing (AD 0052):** first-operation validation — the
    /// crate-wide contract, not a per-backend quirk. The constructor only
    /// stores the path; the file is not even opened. Central-directory
    /// parsing and every other format-level check happens on the first
    /// call that needs the contents (`list_files`, `extract_*`,
    /// `test_integrity`), so a corrupt, truncated, or non-ZIP file
    /// surfaces its error there rather than from `open()`. That first
    /// parse is memoised in `listing` (AD 0065), so it is paid once per
    /// handle. Callers who want the check *now* call
    /// [`crate::Archive::validate`] (or
    /// [`crate::backend::ReadBackend::validate`]), which forces exactly
    /// this parse and leaves it cached — it is not
    /// `validate_integrity`, which decodes every payload.
    ///
    /// That same contract is why the OI-0001-002 file-identity binding is
    /// *not* taken here: a constructor `stat` would report a missing or
    /// unreadable file from `open()` instead of from the first operation,
    /// changing a documented contract. The binding is taken at the first
    /// cache population instead (`open_zip_bound`), from the very
    /// descriptor that operation will read through.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        Ok(Self {
            path: path_buf,
            password: None,
            cached_zip: std::sync::Mutex::new(None),
            listing: once_cell::sync::OnceCell::new(),
            raw_directory: once_cell::sync::OnceCell::new(),
            identity: once_cell::sync::OnceCell::new(),
        })
    }

    /// The archive file this handle is bound to, or `None` if no
    /// operation has populated the cache yet.
    ///
    /// Exists so the facade can revalidate at the one by-path
    /// re-resolution that does not go through this backend at all:
    /// [`crate::Archive::payload_size_for_ratio`] stats the pathname to
    /// produce the compression-ratio *denominator* while the numerator
    /// comes from the cached listing. ZIP is the worst case for that
    /// straddle — after the descriptor is cached there is no second
    /// by-path open here, so no identity refusal from this backend is
    /// even possible.
    pub(crate) fn bound_identity(&self) -> Option<crate::fs_identity::FileIdentity> {
        self.identity.get().copied()
    }

    /// Op label for the identity guard in [`Self::open_zip_bound`].
    ///
    /// The ZIP cache layer sits below every public entry point and has no
    /// caller op at that altitude: `with_zip` is reached from listing,
    /// extraction, streaming and integrity alike, and `with_cached_file`
    /// from the raw-directory scan those all consult. Threading an op
    /// through the cache would mean widening two internal helpers and
    /// every one of their call sites purely to decorate one message. A
    /// fixed `"open"`-family label is used instead, matching the
    /// read-side open-time guard in [`crate::Archive`] (OI-0081-001),
    /// which reports the same condition under the same word: the failing
    /// operation really is *opening the archive file*, not whatever the
    /// caller was going to do with it afterwards.
    const IDENTITY_OP: &str = "open";

    /// Open `self.path` and bind — or re-check — this handle's file
    /// identity, then build the raw archive.
    ///
    /// This is ZIP's *only* by-path re-open. Every listing, extraction,
    /// streaming and integrity path reads through the cached
    /// `RawZipArchive` ([`Self::with_zip`]) or the very descriptor it owns
    /// ([`Self::with_cached_file`], OI-0001-003), so the window this guard
    /// closes is exactly "cache empty, or dropped after a failed rebuild".
    ///
    /// **No stat-to-open window at all (OI-0001-002).** The identity comes
    /// from [`std::fs::File::metadata`] — an `fstat` of the descriptor
    /// *every subsequent read goes through* — not from a path `stat` that
    /// something could swap out before the open lands. The other three
    /// read backends hand a pathname to a native opener and can only
    /// narrow that window; here it is genuinely closed, because the
    /// inode measured is by construction the inode read.
    ///
    /// First *successful* open binds; later calls compare and fail closed
    /// on a mismatch with the shared "identity changed" refusal
    /// ([`crate::fs_identity::identity_drift`]), which stays disjoint from
    /// the `Format` / "listing drift" vocabulary the name and cardinality
    /// guards use — those are additive and all still stand, because a
    /// same-inode same-length rewrite is invisible to any stat identity.
    ///
    /// **Why the bind is after the parse, not before it.** AD 0052 says
    /// `open()` touches no file and the *first operation* validates, so a
    /// first operation run against an archive a producer is still writing
    /// is a supported, recoverable shape: the central directory lives at
    /// the end of a ZIP, `RawZipArchive::new` fails until the writer is
    /// done, and every entry point takes `&self`, so the caller simply
    /// retries. Binding before the parse turned that transient `Format`
    /// error into a permanent one — the failed attempt recorded `len` at
    /// its half-written value, and the retry against the finished,
    /// healthy file was refused forever as "identity changed" with nobody
    /// having swapped anything. Recording the binding only once a parse
    /// has succeeded keeps the retry working while losing no swap
    /// coverage: the comparison above still runs on every call that finds
    /// a binding already in place.
    ///
    /// Both call sites hold the `cached_zip` mutex, so the cell is never
    /// written concurrently.
    fn open_zip_bound(&self) -> Result<RawZipArchive<File>> {
        let file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;
        let meta = file
            .metadata()
            .map_err(|e| ArchiveError::io("stat", self.path.clone(), e))?;
        let found = crate::fs_identity::FileIdentity::from_metadata(&meta);
        // Compare-only: an existing binding is enforced before a single
        // byte of the replacement is parsed.
        if let Some(&expected) = self.identity.get() {
            if expected != found {
                return Err(crate::fs_identity::identity_drift(
                    expected,
                    Some(found),
                    &self.path,
                    Self::IDENTITY_OP,
                ));
            }
        }
        let zip = RawZipArchive::new(file).map_err(|e| {
            ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e))
        })?;
        // Bind only now. `found` is the `fstat` of the very descriptor
        // this archive reads through, so the binding still names the
        // inode that was parsed, not whatever the path resolves to next.
        let _ = self.identity.set(found);
        Ok(zip)
    }

    /// Test-only: empty the memoised `RawZipArchive` so the next operation
    /// has to re-open `self.path` through [`Self::open_zip_bound`].
    ///
    /// In production that second population happens when a
    /// [`Self::with_cached_file`] rebuild fails and leaves the slot empty;
    /// staging a rebuild failure *and* a file swap in one test would prove
    /// less about the guard than reaching the same state directly does.
    #[cfg(test)]
    fn drop_cached_zip(&self) {
        *self
            .cached_zip
            .lock()
            .expect("test helper: the cache mutex must not be poisoned") = None;
    }

    /// Run `f` against a `RawZipArchive<File>`, populating the cache on
    /// first call and reusing it thereafter. The mutex is uncontended in
    /// normal use because `Archive` is `!Sync`; it exists to satisfy
    /// `RawZipArchive`'s `&mut self` requirement under the wrapper's
    /// `&self` API.
    fn with_zip<R>(&self, f: impl FnOnce(&mut RawZipArchive<File>) -> Result<R>) -> Result<R> {
        // R0075-0042: surface mutex poisoning as an error rather than
        // silently recovering with `into_inner`. A panic while the raw
        // ZIP cursor was borrowed leaves it in an unknown state; later
        // operations should not pretend the archive is healthy.
        let mut guard = self.cached_zip.lock().map_err(|_| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                "ZIP archive cache mutex poisoned by an earlier panic; reopen the archive",
            )
        })?;
        let zip = match &mut *guard {
            Some(zip) => zip,
            slot => slot.insert(self.open_zip_bound()?),
        };
        f(zip)
    }

    /// Run `f` against the very `File` the cached `RawZipArchive` reads
    /// through — the atomicity half of OI-0001-003 (R0001-0026).
    ///
    /// The raw central-directory scan used to do its own
    /// `File::open(&self.path)`, so the scan and every subsequent extraction
    /// could read two different files: replace the path between the two
    /// opens and the guard blesses one archive while the extractor reads
    /// another. Rather than open twice and reconcile, this hands out the
    /// descriptor that is already open, so there is nothing to reconcile:
    /// one file, one answer. (Since OI-0001-002 the backend *does* record
    /// an open-time identity, but that guard covers the other direction —
    /// a re-open when the cache is empty — and is no substitute for
    /// reading one source here.) The `zip` crate exposes its
    /// reader only by consuming the archive (`into_inner`), so the cached
    /// handle is taken apart under the same mutex acquisition and rebuilt
    /// from the same descriptor before the lock is released; the one extra
    /// central-directory parse is paid once per handle, when the index is
    /// built.
    fn with_cached_file<T>(&self, f: impl FnOnce(&mut File) -> Result<T>) -> Result<T> {
        let mut guard = self.cached_zip.lock().map_err(|_| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                "ZIP archive cache mutex poisoned by an earlier panic; reopen the archive",
            )
        })?;
        // Materialise the cache first: `f` must receive the descriptor every
        // other operation reads through, whether or not one was open yet.
        if guard.is_none() {
            *guard = Some(self.open_zip_bound()?);
        }
        let zip = guard
            .take()
            .expect("cache was just materialised, so the slot is occupied");
        let mut file = zip.into_inner();

        let out = f(&mut file);

        // Rebuild from the SAME descriptor. A rebuild failure leaves the
        // cache empty (a later call reopens) and is reported only when `f`
        // itself succeeded, so the caller's own error is never masked.
        match RawZipArchive::new(file) {
            Ok(zip) => {
                *guard = Some(zip);
                out
            }
            Err(e) => out.and(Err(ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Invalid ZIP: {}", e),
            ))),
        }
    }

    /// Open encrypted ZIP archive with password.
    ///
    /// Same first-operation validation as [`ZipArchive::open`] — the
    /// password is stored alongside the path, but neither the archive
    /// shape nor the password's correctness is checked until the caller
    /// requests entry data. A wrong password surfaces from the
    /// `extract_*` paths via `ArchiveError::Password`; note that
    /// [`crate::Archive::validate`] parses metadata only, so it does
    /// **not** report a bad password.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let mut archive = Self::open(path)?;
        archive.password = Some(Password::new(password));
        Ok(archive)
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// List all files in ZIP archive with CRC32 from central directory metadata
    ///
    /// CRC32 is read directly from the ZIP central directory (no decompression needed).
    /// A stored CRC32 *value* of 0 is preserved as valid — `CRC32(b"") == 0`, so an
    /// empty file lists `Some(0)` and is never mistaken for a CRC-less entry
    /// (AD 0012). The only entry kind that lists `None` is the AE-2 AES
    /// placeholder: AE-2 stores 0 in the CRC32 field by specification, so the
    /// value is not a checksum at all. The gate is [`crc32_check_exempt`],
    /// which requires the `0x9901` extra field's vendor version to *be*
    /// AE-2 — never the value alone, and never `encrypted() && crc == 0`,
    /// which would also sweep in encrypted empty files whose zero CRC is a
    /// real checksum (see that function for why the distinction matters).
    /// Listing `None` lets the content-multiset digest walk stream the
    /// entry's decrypted payload instead of folding a constant placeholder
    /// (DCR-012 / R0079-0007). Listing itself still needs no password;
    /// digesting an AE-2 entry — like any other read of its data — does.
    ///
    /// Encrypted ZIPs are listed via `by_index_raw`, which surfaces central
    /// directory metadata without the decryption gate that `by_index`
    /// imposes. Listing therefore works regardless of whether the handle
    /// holds a password — encryption only matters when the caller tries to
    /// read entry data.
    ///
    /// **Caching (AD 0065 / OI-0065-003).** The first call walks the
    /// central directory and memoises the result; subsequent calls
    /// share the snapshot via `Arc::clone`.
    ///
    /// **Collapsed records (OI-0001-003).** The listing is the `zip` crate's
    /// deduped view, so an archive whose raw central directory carries
    /// byte-identical duplicate names lists fewer entries than it holds.
    /// Listing therefore consults the raw index and fails closed
    /// ([`ArchiveError::OperationBlocked`]) when the collapse cannot be
    /// attributed to specific names — see
    /// [`Self::reject_if_unlocalizable`] for why an *attributable*
    /// duplicate still lists.
    pub fn list_files(&self) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.list_files_budgeted(None)
    }

    /// Budgeted listing (OI-0080-003). `budget = Some(n)` aborts before
    /// building our `Vec<ArchiveEntry>` when the archive declares more than
    /// `n` entries. Residual: the `zip` crate parsed the whole central
    /// directory when the `ZipArchive` was constructed, so the budget bounds
    /// only OUR allocation, not the library-internal directory. The budget
    /// applies only to the first materialization; a cache hit ignores it, and
    /// an aborted parse does not populate `listing`.
    pub fn list_files_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        let entries = self
            .listing
            .get_or_try_init(|| {
                self.with_zip(|zip| {
                    // OI-0080-003: `zip.len()` is the count the `zip` crate
                    // already parsed from the central directory; reject before
                    // allocating our `Vec` past the budget.
                    if let Some(budget) = budget {
                        if zip.len() > budget {
                            return Err(crate::security::too_many_entries_parsed(budget));
                        }
                    }

                    let mut entries = Vec::new();

                    for i in 0..zip.len() {
                        let zip_file = zip.by_index_raw(i).map_err(|e| {
                            ArchiveError::format(
                                Some(ArchiveFormat::Zip),
                                format!("Read entry {}: {}", i, e),
                            )
                        })?;

                        // Parse entry metadata
                        let mut entry = self.parse_entry(&zip_file)?;

                        // CRC32 from central directory metadata. A stored 0
                        // stays a valid checksum (AD 0012) — CRC32(b"") == 0 —
                        // so an empty file still lists Some(0), encrypted or
                        // not. What is not a checksum is the AE-2 AES
                        // placeholder, and `crc32_check_exempt` identifies it
                        // by the 0x9901 vendor version rather than by the
                        // value (or by `encrypted() && crc == 0`, which is a
                        // superset that also catches encrypted empty files),
                        // so only genuine AE-2 entries list `None` and get
                        // streamed by the digest walk (DCR-012 / R0079-0007).
                        // `ArchiveEntry` initialises `crc32: None`, so no else
                        // arm is needed.
                        if entry.entry_type == EntryType::File && !crc32_check_exempt(&zip_file) {
                            entry.crc32 = Some(zip_file.crc32());
                        }

                        entry.id = i;
                        entries.push(entry);
                    }

                    Ok(std::sync::Arc::new(entries))
                })
            })
            .map(std::sync::Arc::clone)?;
        // OI-0001-003: the count this hands back (and every `entry_count`
        // derived from it) is the deduped one. Consult the raw index before
        // returning it. Ordered after the budget check on purpose: an
        // over-budget archive must be refused without walking its directory
        // a second time.
        self.reject_if_unlocalizable(crate::error::ops::LIST_FILES)?;
        Ok(entries)
    }

    /// The one raw central-directory index, built on first use and shared
    /// by every operation that consults it (OI-0001-003).
    fn raw_central_directory(&self) -> Result<&RawCentralDirectory> {
        self.raw_directory
            .get_or_try_init(|| self.scan_raw_central_directory())
    }

    /// Refuse a by-name single-entry extraction when the requested
    /// normalized path is ambiguous in the RAW central directory
    /// (R0079-0026 / DCR-009). Runs immediately after
    /// [`crate::security::validate_single_entry`] returns a unique listing
    /// match, so the two guards compose: `validate_single_entry` catches
    /// distinct raw names that normalize to one path (they survive as
    /// separate listing entries), and this catches byte-identical names the
    /// `zip` crate collapsed into a single deduped record.
    ///
    /// The refusal stays **per name** (plus the unattributable-collapse
    /// case): an archive that carries one ambiguous name must keep serving
    /// its unambiguous entries, which is the contract the cross-backend
    /// single-entry parity suite pins.
    fn reject_if_duplicate(&self, validated_path: &str, op: &'static str) -> Result<()> {
        let raw = self.raw_central_directory()?;
        if raw.any_undetected || raw.duplicate_names.contains(validated_path) {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: multiple_entries_reason(validated_path),
            });
        }
        Ok(())
    }

    /// Refuse an operation that consumes the archive's WHOLE collapsed view
    /// when the raw directory carried records the `zip` crate collapsed
    /// (OI-0001-003 / R0001-0030).
    ///
    /// Bulk extraction, the integrity walk and an id-addressed stream all
    /// iterate `zip.len()` — the deduped view — so on an ambiguous archive
    /// they used to report success over a *subset* of the records that exist:
    /// a complete-looking extraction that quietly dropped a shadowed payload,
    /// an integrity pass that never verified it, a content digest that never
    /// folded it in. A shadowed record has no listing id, so there is nothing
    /// to extract it *with*; the honest answer is to refuse the whole-view
    /// operation and let the caller address the unambiguous entries by name.
    fn reject_if_collapsed(&self, op: &'static str) -> Result<()> {
        let raw = self.raw_central_directory()?;
        if raw.is_collapsed() {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: raw.collapsed_reason(),
            });
        }
        Ok(())
    }

    /// Refuse a listing (and therefore every entry count derived from it)
    /// when the collapse cannot be localized to specific names
    /// (OI-0001-003).
    ///
    /// Listing is deliberately the *narrow* gate. When attribution succeeds,
    /// the ambiguity is confined to named entries and every read of those
    /// names is refused, so a caller cannot be silently handed the wrong
    /// payload — and the refusal text's own advice ("address the entry by
    /// id from `list_files()`") stays reachable, as does the cross-backend
    /// parity contract that an archive carrying one ambiguous name still
    /// lists. When attribution fails there is no name to attach the refusal
    /// to, so no downstream guard can protect the caller and the listing
    /// itself must fail closed.
    fn reject_if_unlocalizable(&self, op: &'static str) -> Result<()> {
        let raw = self.raw_central_directory()?;
        if raw.any_undetected {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: raw.collapsed_reason(),
            });
        }
        Ok(())
    }

    /// Walk the RAW central directory once and build the index every
    /// operation consults (R0079-0026 / DCR-009 / OI-0001-003).
    ///
    /// The `zip` crate exposes only its name-deduped central-directory map,
    /// so the records it collapsed have to be counted from the file itself,
    /// starting at `central_directory_start()` (already absolute — it folds
    /// in any SFX/prepended-data offset).
    ///
    /// **One source (R0001-0026).** The walk reads through
    /// [`Self::with_cached_file`], i.e. the descriptor the cached
    /// `RawZipArchive` already owns, so the index and every later extraction
    /// see the same bytes. It performs no `File::open` of its own; replacing
    /// the file at `self.path` mid-flight can no longer make the guard bless
    /// one archive while the extractor reads another.
    ///
    /// **Tallying (R0001-0029).** Records are tallied by their RAW name
    /// bytes and then joined against the crate's own listing through
    /// `ZipFile::name_raw()`. Decoding the bytes here ourselves would be
    /// wrong twice over: the crate decodes CP437 (not UTF-8) whenever the
    /// general-purpose UTF-8 flag is clear, and `reject_if_duplicate` is
    /// consulted with the *listed* path — so a `from_utf8_lossy` key full
    /// of replacement characters could never equal it, and a non-UTF-8
    /// duplicate went unrejected even though it had been detected. The raw
    /// bytes are also exactly what the crate collapses on: two records with
    /// byte-identical names always decode to the same string.
    ///
    /// **Accounting (R0001-0028).** `raw_records - deduped_len` is how many
    /// records the crate collapsed away; `attributed` is how many of those
    /// collapses this scan pinned to listed names. Any shortfall means a
    /// collapse escaped attribution, and `any_undetected` then refuses even
    /// the archive's by-name single-entry surface.
    fn scan_raw_central_directory(&self) -> Result<RawCentralDirectory> {
        const CD_HEADER_SIGNATURE: u32 = 0x0201_4b50;
        const EOCD_SIGNATURE: u32 = 0x0605_4b50;
        const ZIP64_EOCD_SIGNATURE: u32 = 0x0606_4b50;
        const ZIP64_EOCD_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
        const DIGITAL_SIGNATURE: u32 = 0x0505_4b50;
        const CD_SIGNATURE_LEN: usize = 4;
        const CD_FIXED_HEADER_LEN: usize = 46;

        // R0001-0029: raw name bytes -> the listing index/normalized path
        // pairs the public listing exposes for them. `by_index_raw` reads
        // only the already parsed central-directory metadata, so this is an
        // in-memory walk. One raw spelling can back several listed names
        // when two records share their bytes but disagree on the UTF-8 flag;
        // keeping all of them means every ambiguous listed path is refused.
        type ListedByRawName = HashMap<Vec<u8>, Vec<(usize, String)>>;
        let (cd_start, deduped_len, listed_by_raw_name) = self.with_zip(|zip| {
            let mut listed_by_raw_name: ListedByRawName = HashMap::new();
            for i in 0..zip.len() {
                let raw = zip.by_index_raw(i).map_err(|e| {
                    ArchiveError::format(
                        Some(ArchiveFormat::Zip),
                        format!("Read entry {}: {}", i, e),
                    )
                })?;
                let normalized = normalize_path(raw.name());
                let slot = listed_by_raw_name
                    .entry(raw.name_raw().to_vec())
                    .or_default();
                if !slot.iter().any(|(_, name)| name == &normalized) {
                    slot.push((i, normalized));
                }
            }
            Ok((zip.central_directory_start(), zip.len(), listed_by_raw_name))
        })?;

        // R0001-0026: the same descriptor the cached handle reads through —
        // never a second open of `self.path`.
        let raw_names = self.with_cached_file(|file| {
            file.seek(SeekFrom::Start(cd_start))
                .map_err(|e| ArchiveError::io("seek", self.path.clone(), e))?;

            let mut raw_names: Vec<Vec<u8>> = Vec::new();

            loop {
                // R0001-0027: dispatch on the record signature, read on its
                // own first. Only a recognised terminator ends the walk; a
                // truncated record or an OS read error is a typed failure,
                // never a quiet `break` that would leave duplicate detection
                // half-done.
                let mut signature = [0u8; CD_SIGNATURE_LEN];
                read_central_directory_exact(
                    file,
                    &mut signature,
                    &self.path,
                    "record signature",
                )?;
                match u32::from_le_bytes(signature) {
                    CD_HEADER_SIGNATURE => {}
                    // The directory is terminated by the EOCD, optionally
                    // preceded by an archive digital-signature record and/or
                    // the ZIP64 EOCD record + its locator. All are
                    // legitimate ends.
                    EOCD_SIGNATURE
                    | ZIP64_EOCD_SIGNATURE
                    | ZIP64_EOCD_LOCATOR_SIGNATURE
                    | DIGITAL_SIGNATURE => break,
                    other => {
                        return Err(ArchiveError::corruption(
                            self.path.display().to_string(),
                            format!(
                                "central directory: unexpected record signature {:#010x} after {} record(s)",
                                other,
                                raw_names.len()
                            ),
                        ));
                    }
                }

                // Offsets are the fixed header's, less the 4 signature bytes
                // already consumed: name/extra/comment lengths live at
                // 28/30/32.
                let mut rest = [0u8; CD_FIXED_HEADER_LEN - CD_SIGNATURE_LEN];
                read_central_directory_exact(file, &mut rest, &self.path, "record header")?;
                let name_len = u16::from_le_bytes([rest[24], rest[25]]) as usize;
                let extra_len = u16::from_le_bytes([rest[26], rest[27]]) as usize;
                let comment_len = u16::from_le_bytes([rest[28], rest[29]]) as usize;

                let mut name_bytes = vec![0u8; name_len];
                read_central_directory_exact(file, &mut name_bytes, &self.path, "entry name")?;
                file.seek(SeekFrom::Current((extra_len + comment_len) as i64))
                    .map_err(|e| ArchiveError::io("seek", self.path.clone(), e))?;

                raw_names.push(name_bytes);
            }

            Ok(raw_names)
        })?;

        let mut counts: HashMap<&[u8], u32> = HashMap::new();
        for name in &raw_names {
            *counts.entry(name.as_slice()).or_insert(0) += 1;
        }

        let mut duplicate_names: HashSet<String> = HashSet::new();
        let mut attributed: u64 = 0;
        for (raw_name, count) in &counts {
            if *count <= 1 {
                continue;
            }
            // Only a duplicate whose raw bytes map onto listed paths is
            // rejectable by name; anything else falls through to the global
            // refusal below (R0001-0028 / R0001-0029).
            if let Some(listed) = listed_by_raw_name.get(*raw_name) {
                attributed += u64::from(*count).saturating_sub(listed.len() as u64);
                duplicate_names.extend(listed.iter().map(|(_, name)| name.clone()));
            }
        }

        // R0001-0028: compare the collapse counts instead of asking whether
        // any name was attributed at all — one ordinary duplicate must not
        // disarm the guard for a second, unattributable collision. A raw
        // count *below* the crate's own is equally unexplained (the EOCD
        // undercounts the directory, or the two views disagree about where
        // it starts), so treat that as ambiguous too.
        let raw_records = raw_names.len() as u64;
        let collapsed = raw_records.saturating_sub(deduped_len as u64);
        let any_undetected = attributed < collapsed || raw_records < deduped_len as u64;

        // OI-0001-003 asks the index to carry the per-record name bytes and
        // the crate index each maps to, so the whole-view guards can explain
        // *which* records are unaddressable rather than only that some are.
        let records = raw_names
            .into_iter()
            .map(|name| {
                let listed_indices = listed_by_raw_name
                    .get(&name)
                    .map(|listed| listed.iter().map(|(index, _)| *index).collect())
                    .unwrap_or_default();
                RawRecord {
                    name,
                    listed_indices,
                }
            })
            .collect();

        Ok(RawCentralDirectory {
            records,
            deduped_len,
            duplicate_names,
            any_undetected,
        })
    }

    /// Parse ZIP entry into ArchiveEntry
    fn parse_entry(&self, zip_file: &zip::read::ZipFile) -> Result<ArchiveEntry> {
        // Normalize path separators for cross-backend consistency.
        let path = normalize_path(zip_file.name());

        // R0075-0043 / R0081-0077: classify through the shared ZIP mode
        // decoder. It keeps the symlink-before-directory ordering and
        // maps special Unix modes (FIFO/char/block/socket) to
        // `EntryType::Other` instead of silently reporting them as regular
        // files.
        let entry_type = classify_zip_entry_type(zip_file.unix_mode(), path.ends_with('/'));

        let is_dir = entry_type == EntryType::Directory;
        let size = if is_dir { None } else { Some(zip_file.size()) };

        let compressed_size = if is_dir {
            None
        } else {
            Some(zip_file.compressed_size())
        };

        // R0001-0062 / OI-0065-002: resolve through the shared helper the
        // extraction paths also use, so a listed timestamp and the one
        // restored on disk can never disagree.
        let times = zip_entry_times(zip_file);

        let mut entry = ArchiveEntry::file(path, 0).build();
        entry.entry_type = entry_type;
        entry.size = size;
        entry.compressed_size = compressed_size;
        entry.modified = times.modified;
        entry.accessed = times.accessed;
        entry.created = times.created;
        entry.is_encrypted = zip_file.encrypted();
        // R0075-0044: surface Unix permissions when present. ZIP stores
        // the OS host in central-directory metadata; entries written on
        // a Unix-style host carry mode bits in `unix_mode`. Mask down to
        // permission bits (lower 12) per `ArchiveEntry::permissions`'s
        // contract — the high bits encode file type and are irrelevant
        // here.
        entry.permissions = zip_file.unix_mode().map(|m| m & 0o7777);

        Ok(entry)
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, true, true, false, None)
    }

    /// Extract a single file by path
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, false)
    }

    /// Extract a single file to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.extract_to_memory_capped(file_path, None, crate::error::ops::EXTRACT_TO_MEMORY)
    }

    /// Cap-aware extract-to-memory: `max_bytes` is honoured *before*
    /// buffering — an entry whose central-directory declared size exceeds
    /// the cap is rejected up front instead of being fully decoded and
    /// post-checked (R0081-0030). Backend `extract_to_memory_with_limit`
    /// routes here so the ZIP path no longer inherits the trait default
    /// that buffers the whole payload before comparing against the cap.
    ///
    /// R5 (ti-581bcda4): `op` labels every error this path can raise. The
    /// constant used to be hard-coded to `extract_to_memory`, which
    /// mislabelled a refusal raised on behalf of
    /// `Archive::extract_to_stream` — contradicting the R0071-0010
    /// convention that the label names the *public* operation the caller
    /// invoked. The stream entry point passes
    /// [`ops::EXTRACT_TO_STREAM`](crate::error::ops::EXTRACT_TO_STREAM);
    /// the memory entry points keep
    /// [`ops::EXTRACT_TO_MEMORY`](crate::error::ops::EXTRACT_TO_MEMORY).
    pub(crate) fn extract_to_memory_capped(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
        op: &'static str,
    ) -> Result<Vec<u8>> {
        let password = self.password.as_ref().map(Password::as_str);
        // OI-0076-002: resolve through the shared single-entry gate first —
        // existence, uniqueness, and entry-kind policy (links, non-files)
        // live in `validate_single_entry`; the code below only seeks by
        // the validated listing index.
        let entries = self.list_files()?;
        // R5 (ti-581bcda4): `op`, not a hard-coded `extract_to_memory`.
        let target = crate::security::validate_single_entry(&entries, file_path, op)?;
        // R0079-0026 / DCR-009: the `zip` crate collapsed any byte-identical
        // duplicate names, so `validate_single_entry` saw a unique match;
        // refuse here if the raw central directory carried duplicates.
        self.reject_if_duplicate(target.path(), op)?;
        let target_id = target.id();
        let validated_path = target.path();
        self.with_zip(|zip| {
            check_listing_drift(zip, target_id, validated_path)?;
            let mut zip_file = open_entry_by_index(zip, target_id, password)?;

            // CRC verification catches corrupt/tampered payloads. AE-2
            // entries are exempt — their stored CRC is a placeholder
            // (R0079-0007).
            let expected_crc = (!crc32_check_exempt(&zip_file)).then(|| zip_file.crc32());
            let size = zip_file.size();
            // R0001-0031: `require_exact = true`. The ZIP central
            // directory's uncompressed size is authoritative (the same
            // reason the staged-write path runs under
            // `CopyByteBudget::Exact`), so a short decode is corruption,
            // not a valid buffer — otherwise an under-produced payload
            // could escape as `Ok(Vec<u8>)` whenever the CRC compare is
            // exempt (AE-2) or attacker-controlled. Zero-size entries are
            // unaffected: the shared helper gates on
            // `require_exact && declared_size > 0`.
            read_entry_to_memory_capped(
                &mut zip_file,
                size,
                max_bytes,
                validated_path,
                // R5 (ti-581bcda4): carry the caller's operation label.
                op,
                expected_crc,
                true,
            )
        })
    }

    /// Extract all files with options (overwrite, metadata preservation,
    /// verify_crc32).
    ///
    /// `preserve_permissions` / `preserve_times` (R0079-0019) apply the
    /// central directory's Unix mode bits / DOS-precision modification
    /// time to each staged file before the atomic install.
    ///
    /// When `selection` is `Some`, only entries whose positional index
    /// (matching `list_files()` order) is in the set are materialized — other
    /// entries are skipped without warnings. The archive is traversed once
    /// per AD 0029.
    ///
    /// **Collapsed records (OI-0001-003).** The walk iterates the `zip`
    /// crate's deduped view, so on an archive whose raw central directory
    /// carries duplicate names it would report a complete extraction while
    /// silently dropping every shadowed payload. It therefore consults the
    /// raw index first and refuses the archive
    /// ([`ArchiveError::OperationBlocked`]) instead. Refusal rather than a
    /// warning because no `ArchiveWarning` variant describes an
    /// unaddressable central-directory record, and a shadowed record has no
    /// listing id, so there is no selection the caller could pass to reach
    /// it.
    #[allow(clippy::too_many_arguments)]
    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        verify_crc32: bool,
        selection: Option<&std::collections::HashSet<usize>>,
    ) -> Result<Vec<ArchiveWarning>> {
        // OI-0001-003: before touching the filesystem — a refused archive
        // must leave the destination untouched.
        self.reject_if_collapsed(crate::error::ops::EXTRACT_ALL)?;

        std::fs::create_dir_all(dest_path)
            .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

        let password = self.password.as_ref().map(Password::as_str);
        let mut progress = progress;
        self.with_zip(|zip| {
            let mut warnings: Vec<ArchiveWarning> = Vec::new();

            // Calculate total size for progress (selected entries only when
            // filtering). `by_index_raw` is the right primitive here — only
            // central-directory metadata is needed, and the raw form skips
            // the decryption/decompression setup that `by_index` would
            // otherwise pay per entry. R0075-0067: exclude directory and
            // symlink entries from the total — extraction skips them so
            // counting their declared sizes overstates the work the bar
            // is tracking.
            let total_bytes: u64 = if progress.is_some() {
                let mut total = 0u64;
                for i in 0..zip.len() {
                    if let Some(sel) = selection {
                        if !sel.contains(&i) {
                            continue;
                        }
                    }
                    // R0080-0062: propagate central-directory read errors
                    // instead of dropping them. A damaged entry that
                    // silently vanished from the denominator left progress
                    // inconsistent with the extraction walk, which fails on
                    // that same entry later.
                    let f = zip.by_index_raw(i).map_err(|e| {
                        ArchiveError::format(
                            Some(ArchiveFormat::Zip),
                            format!("Read entry {}: {}", i, e),
                        )
                    })?;
                    // R0075-0067 / R0081-0077: count only regular files.
                    // Directories, symlinks, and special (FIFO/device/socket)
                    // entries are never materialized, so their declared sizes
                    // must stay out of the progress denominator.
                    if classify_zip_entry_type(
                        f.unix_mode(),
                        normalize_path(f.name()).ends_with('/'),
                    ) != EntryType::File
                    {
                        continue;
                    }
                    // R0076-0052: a crafted central directory with huge
                    // declared sizes can wrap the progress denominator.
                    total = total.saturating_add(f.size());
                }
                total
            } else {
                0
            };

            let mut bytes_processed = 0u64;
            let num_entries = zip.len();
            // Canonicalize the destination once — every entry's sanitize
            // pass compares against the same stable base.
            let canonical_dest = canonicalize_dest_base(dest_path)?;

            for i in 0..num_entries {
                check_extraction_cancelled(
                    &mut progress,
                    bytes_processed,
                    total_bytes,
                    crate::error::ops::EXTRACT_ALL,
                )?;

                if let Some(sel) = selection {
                    if !sel.contains(&i) {
                        continue;
                    }
                }

                // R0080-0028: classify the entry from raw central-directory
                // metadata *before* opening a decrypting reader.
                // `open_entry_by_index` decrypts, so classifying afterwards
                // demanded a password (and ran decoder setup) for encrypted
                // symlink/directory entries that extraction policy skips
                // outright. `by_index_raw` exposes name/type without the
                // decryption gate.
                //
                // Skipping blocked kinds before touching the filesystem also
                // keeps the previous order's stub parent directories from
                // being left behind (R0069-0024); the warning is reported
                // under the same normalized path listing/filtering uses so
                // UI consumers can correlate by string match (R0075-0046).
                let (normalized_name, entry_type) = {
                    let raw = zip.by_index_raw(i).map_err(|e| {
                        ArchiveError::format(
                            Some(ArchiveFormat::Zip),
                            format!("Read entry {}: {}", i, e),
                        )
                    })?;
                    let normalized_name = normalize_path(raw.name());
                    let entry_type =
                        classify_zip_entry_type(raw.unix_mode(), normalized_name.ends_with('/'));
                    (normalized_name, entry_type)
                };
                match entry_type {
                    EntryType::Symlink => {
                        warnings.push(ArchiveWarning::SkippedSymlink {
                            path: normalized_name.clone(),
                            target: None,
                        });
                        continue;
                    }
                    // R0081-0077: special Unix modes (FIFO/char/block/socket)
                    // are neither regular files nor directories; skip them
                    // rather than materializing a plain file, so a device /
                    // FIFO entry is treated the same way by both ZIP backends.
                    EntryType::Other => continue,
                    _ => {}
                }

                // R0076-0049: sanitize the *normalized* name so warning,
                // selection, and output paths agree for entries containing
                // backslashes. Previously sanitize ran on `zip_file.name()`
                // (raw stored name) while warnings ran on `normalized_name`.
                let entry_path =
                    sanitize_entry_path_with_base(&normalized_name, dest_path, &canonical_dest)?;

                if let Some(parent) = entry_path.parent() {
                    // R0076-0005: create *and* re-verify — the containment
                    // answer computed above is stale the moment these
                    // directories exist.
                    crate::security::create_parent_dirs_verified(
                        parent,
                        &canonical_dest,
                        &normalized_name,
                    )?;
                }

                if entry_type == EntryType::Directory {
                    std::fs::create_dir_all(&entry_path)
                        .map_err(|e| ArchiveError::io("create_dir", entry_path.clone(), e))?;
                } else {
                    // Only regular files reach the decrypting reader
                    // (R0080-0028): links and directories were already
                    // classified and handled above without a password.
                    let mut zip_file = open_entry_by_index(zip, i, password)?;
                    let entry_size = zip_file.size();
                    // Capture metadata up front — the staged write below
                    // borrows `zip_file` mutably (R0079-0019). AE-2
                    // entries are exempt from the CRC compare — their
                    // stored CRC is a placeholder (R0079-0007).
                    let expected_crc =
                        (verify_crc32 && !crc32_check_exempt(&zip_file)).then(|| zip_file.crc32());
                    let entry_mode = preserve_permissions.then(|| zip_file.unix_mode()).flatten();
                    // R0001-0062: same timestamp resolution as listing —
                    // the 0x5455 extended timestamp wins over the coarse
                    // DOS value when the record carries one.
                    let entry_mtime = preserve_times
                        .then(|| zip_entry_times(&zip_file).modified)
                        .flatten();

                    let mut cancel_check =
                        entry_cancel_hook(&mut progress, bytes_processed, total_bytes);
                    write_entry_atomically(
                        &mut zip_file,
                        StagedEntryWrite {
                            output_path: &entry_path,
                            entry_path: &normalized_name,
                            op: crate::error::ops::EXTRACT_ALL,
                            overwrite,
                            expected_crc,
                            declared_size: entry_size,
                            unix_mode: entry_mode,
                            modified: entry_mtime,
                        },
                        Some(&mut cancel_check),
                    )?;

                    bytes_processed += entry_size;
                }
            }

            // R0070-0035: honor cancellation at 100% so a Break in the
            // final callback surfaces consistently with the per-entry
            // path. Previously the result was swallowed via `let _`.
            check_extraction_cancelled(
                &mut progress,
                total_bytes,
                total_bytes,
                crate::error::ops::EXTRACT_ALL,
            )?;

            Ok(warnings)
        })
    }

    /// Extract a single file with options (overwrite, verify_crc32).
    ///
    /// Legacy single-file surface; metadata preservation defaults to on,
    /// matching `ExtractionOptions::default()` (R0079-0019).
    pub fn extract_file_with_options(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        self.extract_file_with_options_preserve(
            file_path,
            dest_path,
            overwrite,
            true,
            true,
            verify_crc32,
        )
    }

    /// Metadata-aware variant of [`Self::extract_file_with_options`]
    /// (R0079-0019): `preserve_permissions` / `preserve_times` apply the
    /// central directory's Unix mode bits / DOS-precision modification
    /// time to the staged file before the atomic install.
    pub fn extract_file_with_options_preserve(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        let password = self.password.as_ref().map(Password::as_str);
        // OI-0076-002: resolve through the shared single-entry gate first —
        // link and non-file rejection policy lives in
        // `validate_single_entry`, and it runs before any filesystem
        // mutation so a rejected entry leaves the destination untouched
        // (R0075-0066). Directory entries requested through this API now
        // error at the gate instead of materializing as mkdir (R0076-0054);
        // use extract_all / extract_files for directory entries.
        let entries = self.list_files()?;
        let target = crate::security::validate_single_entry(
            &entries,
            file_path,
            crate::error::ops::EXTRACT_FILE,
        )?;
        // R0079-0026 / DCR-009: refuse duplicated raw central-directory
        // names that the `zip` crate silently deduped (see
        // `Self::reject_if_duplicate`).
        self.reject_if_duplicate(target.path(), crate::error::ops::EXTRACT_FILE)?;
        let target_id = target.id();
        let validated_path = target.path();
        self.with_zip(|zip| {
            check_listing_drift(zip, target_id, validated_path)?;
            let mut zip_file = open_entry_by_index(zip, target_id, password)?;

            std::fs::create_dir_all(dest_path)
                .map_err(|e| ArchiveError::io("create_dir", dest_path.to_path_buf(), e))?;

            // R0076-0050: sanitize the validated normalized listing name,
            // never the raw stored name.
            let output_path = sanitize_entry_path(validated_path, dest_path)?;

            if let Some(parent) = output_path.parent() {
                // R0076-0005: single-entry path, so canonicalising the
                // destination here costs one stat rather than one per entry.
                let canonical_dest = crate::security::canonicalize_dest_base(dest_path)?;
                crate::security::create_parent_dirs_verified(
                    parent,
                    &canonical_dest,
                    validated_path,
                )?;
            }

            // Capture metadata up front — the staged write below borrows
            // `zip_file` mutably (R0079-0019). AE-2 entries are exempt
            // from the CRC compare (R0079-0007).
            let expected_crc =
                (verify_crc32 && !crc32_check_exempt(&zip_file)).then(|| zip_file.crc32());
            let entry_mode = preserve_permissions.then(|| zip_file.unix_mode()).flatten();
            // R0001-0062: resolve the timestamp exactly as listing does
            // (0x5455 extended timestamp over the coarse DOS value).
            let entry_mtime = preserve_times
                .then(|| zip_entry_times(&zip_file).modified)
                .flatten();
            let entry_size = zip_file.size();

            write_entry_atomically(
                &mut zip_file,
                StagedEntryWrite {
                    output_path: &output_path,
                    entry_path: validated_path,
                    op: crate::error::ops::EXTRACT_FILE,
                    overwrite,
                    expected_crc,
                    declared_size: entry_size,
                    unix_mode: entry_mode,
                    modified: entry_mtime,
                },
                None,
            )?;

            Ok(())
        })
    }

    /// Test integrity of all entries by verifying CRC32
    ///
    /// Opens the ZIP once and stream-verifies each file entry in a single
    /// pass. Per-entry payload corruption — a CRC mismatch or a
    /// decode-class read error — is recorded as a failed path; a genuine
    /// archive-file I/O error (open/read on the archive itself) propagates
    /// as `Err` (R0080-0030).
    ///
    /// **Collapsed records (OI-0001-003).** The walk iterates the `zip`
    /// crate's deduped view, so a shadowed duplicate record's payload is
    /// never verified at all — an "all entries pass" answer over a subset of
    /// the records the file carries. An ambiguous central directory is an
    /// archive-level defect, not a per-entry one, so it propagates as `Err`
    /// ([`ArchiveError::OperationBlocked`]) rather than being reported as a
    /// failed entry path.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        self.reject_if_collapsed(crate::error::ops::VALIDATE_INTEGRITY)?;

        let password = self.password.as_ref().map(Password::as_str);
        self.with_zip(|zip| {
            let mut failed = Vec::new();

            for i in 0..zip.len() {
                // R0080-0029: classify the entry from raw central-directory
                // metadata before opening a decrypting reader. Directories,
                // symlinks, and special Unix modes (R0081-0077) are not part
                // of the regular-file payload and extraction skips them
                // (R0070-0052), so an encrypted non-file must not demand a
                // password just to be skipped — running the CRC32 walk on
                // them would also charge a mismatch against the link-target
                // string. R0080-0063: record failures under the shared
                // normalized path so they correlate with listing IDs.
                let (skip, normalized_name) = {
                    let raw = zip.by_index_raw(i).map_err(|e| {
                        ArchiveError::format(
                            Some(ArchiveFormat::Zip),
                            format!("Read entry {}: {}", i, e),
                        )
                    })?;
                    let normalized_name = normalize_path(raw.name());
                    let entry_type =
                        classify_zip_entry_type(raw.unix_mode(), normalized_name.ends_with('/'));
                    (entry_type != EntryType::File, normalized_name)
                };
                if skip {
                    continue;
                }

                let mut zip_file = open_entry_by_index(zip, i, password)?;
                let expected_crc = zip_file.crc32();
                // R0001-0032: the central directory's uncompressed size is
                // authoritative, so capture it and compare it against what
                // the decoder actually produced.
                let declared_size = zip_file.size();
                // AE-2 entries carry no real stored CRC; still stream the
                // payload so the zip crate's AES authentication check runs
                // (auth failures surface as read errors).
                let crc_exempt = crc32_check_exempt(&zip_file);

                // R0080-0030: a corrupt/short compressed payload
                // (decode-class read error) or a decoder-reported CRC
                // failure is recorded against the entry; a genuine
                // archive-file I/O error propagates.
                match drain_entry_crc32_counted(&mut zip_file, &self.path) {
                    Ok((actual_crc, actual_bytes)) => {
                        // R0001-0032: a matching CRC alone is not a
                        // structural check. An entry that decodes to fewer
                        // (or more) bytes than the central directory
                        // declares is damaged even when the stored CRC
                        // matches the short data, and it is the *only*
                        // signal for an AE-2 entry whose CRC compare is
                        // exempt.
                        if actual_bytes != declared_size
                            || (!crc_exempt && actual_crc != expected_crc)
                        {
                            failed.push(normalized_name);
                        }
                    }
                    Err(e) if is_integrity_payload_failure(&e) => {
                        failed.push(normalized_name);
                    }
                    Err(e) => return Err(e),
                }
            }

            Ok(failed)
        })
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// True streaming would require holding a borrow on the ZipArchive reader,
    /// which conflicts with the ownership model.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.extract_to_stream_with_limit(file_path, None)
    }

    /// Streaming variant of [`Self::extract_to_memory_capped`].
    ///
    /// R0001-0011: the ZIP stream path used to inherit the
    /// `ReadBackend::extract_to_stream_with_limit` trait default, which
    /// materializes the *whole* entry through `extract_to_stream` and only
    /// then wraps the buffer in a cap — so a caller-chosen budget bounded
    /// what could be read while the entry had already been decoded in
    /// full. Routing through the capped memory path instead rejects an
    /// entry whose central-directory declared size exceeds `max_bytes`
    /// before any buffer is reserved (R0081-0030), which is what makes the
    /// budget bind pre-materialization. Same buffered-adapter caveat as
    /// [`Self::extract_to_stream`]: true streaming would need to hold a
    /// borrow on the `ZipArchive` reader (DEF-004 / OI-0057-007).
    pub(crate) fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<crate::streaming::StreamingExtractor> {
        // R5 (ti-581bcda4): a refusal raised while serving
        // `Archive::extract_to_stream` is labelled `extract_to_stream`, not
        // `extract_to_memory` — the capped memory helper is an
        // implementation detail of this path, not the caller's operation.
        let data = self.extract_to_memory_capped(
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_STREAM,
        )?;
        Ok(crate::streaming::StreamingExtractor::from_bytes(data))
    }

    /// Stream one entry addressed by its stable listing id rather than by
    /// path (ti-2a6e3153 for tar; DCR-012 brings ZIP under the same rule).
    ///
    /// The content-multiset digest walk reaches this method for every ZIP
    /// entry whose listing carries no CRC32 — since DCR-012 that is exactly
    /// the AE-2 AES entries, whose stored CRC is a placeholder rather than a
    /// checksum. Routing them through the by-*path* stream instead would run
    /// [`crate::security::validate_single_entry`] and
    /// [`Self::reject_if_duplicate`], so an AE-2 archive carrying duplicate
    /// (or duplicate-after-normalization) names — or one whose duplicate
    /// accounting is incomplete (`any_undetected`) — would stop digesting at
    /// all with `OperationBlocked`. That is the OI-0076-002 defect, and it
    /// would be *introduced* by making AE-2 entries stream. Seeking by id
    /// avoids it.
    ///
    /// **What still guards this path.** [`check_listing_drift`] refuses an id
    /// whose entry no longer carries the validated normalized name, so a
    /// stale AD 0065 listing cannot silently redirect the read; the sole
    /// caller (`ValidatedSource::extract_to_stream_by_id`, reached only from
    /// the digest walk) filters to `EntryType::File`, re-applies the
    /// size/ratio arms itself, and never writes to disk. What is deliberately
    /// skipped is only the *uniqueness* half of the single-entry gate — which
    /// is the entire reason an id-addressed seek exists.
    ///
    /// **Collapsed records (OI-0001-003).** Skipping uniqueness is not the
    /// same as ignoring the raw directory. A byte-identical duplicate name is
    /// a *collapse*: the shadowed record has no id, so a digest built from
    /// this path would silently omit a payload and two archives with
    /// different contents could agree. That case is refused here through
    /// [`Self::reject_if_collapsed`]. Duplicate-*after-normalization* names
    /// — distinct raw spellings such as `a/b` and `a\b` — are not a collapse:
    /// both records keep their own id, both stream, and this path stays the
    /// reason they can (the OI-0076-002 defect the by-name route would
    /// reintroduce).
    ///
    /// AE-2 entries stay exempt from the CRC compare here for the same
    /// R0079-0007 reason the other read paths do: comparing decrypted bytes
    /// against the placeholder 0 would flag every non-empty entry corrupt.
    /// AE-1 and plaintext entries keep their compare.
    ///
    /// Same buffered-adapter caveat as [`Self::extract_to_stream`]: the whole
    /// entry is materialized before the cursor is handed back (DEF-004 /
    /// OI-0057-007), so digesting a large AE-2 member costs its full size in
    /// memory plus full decryption.
    /// Push the entry's decoded payload into `sink` without buffering it
    /// (DEF-004).
    ///
    /// The `zip` crate's entry reader borrows from the archive, which is why
    /// this backend cannot *return* a reader and has always buffered instead.
    /// Nothing stops it being read from here, inside the borrow — the sink
    /// shape is what makes the borrow a non-issue rather than a wall.
    ///
    /// Bounding is the caller's, per the trait contract, so this deliberately
    /// does not enforce the declared size or verify the CRC: the digest
    /// surface needs the entry's own declared bound, which the caller holds.
    pub(crate) fn stream_payload_to_sink_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
        sink: &mut crate::backend::PayloadChunkSink<'_>,
    ) -> Result<u64> {
        self.reject_if_collapsed(crate::error::ops::EXTRACT_TO_STREAM)?;
        let password = self.password.as_ref().map(Password::as_str);
        self.with_zip(|zip| {
            check_listing_drift(zip, id, validated_path)?;
            let mut zip_file = open_entry_by_index(zip, id, password)?;
            let mut buf = [0u8; 64 * 1024];
            let mut total = 0u64;
            loop {
                let read = std::io::Read::read(&mut zip_file, &mut buf).map_err(|e| {
                    ArchiveError::io(
                        "read zip entry",
                        std::path::PathBuf::from(validated_path),
                        e,
                    )
                })?;
                if read == 0 {
                    break;
                }
                sink(&buf[..read])?;
                total += read as u64;
            }
            Ok(total)
        })
    }

    pub(crate) fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.reject_if_collapsed(crate::error::ops::EXTRACT_TO_STREAM)?;

        let password = self.password.as_ref().map(Password::as_str);
        let data = self.with_zip(|zip| {
            check_listing_drift(zip, id, validated_path)?;
            let mut zip_file = open_entry_by_index(zip, id, password)?;
            let expected_crc = (!crc32_check_exempt(&zip_file)).then(|| zip_file.crc32());
            let size = zip_file.size();
            // R0001-0031: `require_exact = true` — the central directory's
            // uncompressed size is authoritative, so a short decode is
            // corruption rather than a valid buffer. This matters most here,
            // where the CRC compare is exempt.
            read_entry_to_memory_capped(
                &mut zip_file,
                size,
                None,
                validated_path,
                crate::error::ops::EXTRACT_TO_STREAM,
                expected_crc,
                true,
            )
        })?;
        Ok(crate::streaming::StreamingExtractor::from_bytes(data))
    }
}

#[cfg(test)]
mod tests;
