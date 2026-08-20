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
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use zip::ZipArchive as RawZipArchive;

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

/// Open a ZIP entry by index, using password decryption if provided
fn open_entry_by_index<'a>(
    zip: &'a mut RawZipArchive<File>,
    index: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a>> {
    if let Some(pw) = password {
        zip.by_index_decrypt(index, pw.as_bytes()).map_err(|e| {
            if matches!(e, zip::result::ZipError::InvalidPassword) {
                ArchiveError::password(format!("Invalid password for ZIP entry {}", index))
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::Zip),
                    format!("Read entry {}: {}", index, e),
                )
            }
        })
    } else {
        zip.by_index(index).map_err(|e| {
            ArchiveError::format(
                Some(ArchiveFormat::Zip),
                format!("Read entry {}: {}", index, e),
            )
        })
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

/// WinZip AES extra-field header id (`0x9901`) and the vendor version
/// that marks an entry as AE-2 — the variant that stores no CRC32 at all.
/// AE-1 (`0x0001`) stores the real CRC32 and keeps every checksum
/// comparison in this file.
const AES_EXTRA_FIELD_ID: u16 = 0x9901;
const AES_VENDOR_VERSION_AE2: u16 = 0x0002;

/// Read the AES vendor version out of an entry's `0x9901` extra field.
///
/// The `zip` crate parses this field into a private `aes_mode` tuple and
/// exposes no accessor for it, but it leaves the raw field in
/// `ZipFile::extra_data()` (only the ZIP64 field is stripped), so the two
/// bytes are readable here. Layout per the WinZip AES specification: a
/// 7-byte body of `version:u16 | vendor_id:"AE" | strength:u8 |
/// method:u16`, little-endian.
///
/// Returns `None` when the entry carries no AES field — a plaintext entry
/// or a legacy ZipCrypto one — and `None` for a malformed or truncated
/// extra-field chain rather than guessing.
fn aes_vendor_version(zip_file: &zip::read::ZipFile) -> Option<u16> {
    let extra = zip_file.extra_data()?;
    let mut cursor = 0usize;
    // Extra fields are a chain of `id:u16 | len:u16 | body[len]`.
    while cursor + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[cursor], extra[cursor + 1]]);
        let len = u16::from_le_bytes([extra[cursor + 2], extra[cursor + 3]]) as usize;
        let body = cursor + 4;
        let end = body.checked_add(len)?;
        if end > extra.len() {
            // Truncated chain: stop rather than read past the field.
            return None;
        }
        if id == AES_EXTRA_FIELD_ID {
            if len < 2 {
                return None;
            }
            return Some(u16::from_le_bytes([extra[body], extra[body + 1]]));
        }
        cursor = end;
    }
    None
}

/// AE-2 AES entries store 0 in the central-directory CRC32 field by
/// specification, so comparing decrypted bytes against that placeholder
/// would flag every non-empty entry as corrupt. Mirror the zip crate's
/// own gate (it disables its internal `Crc32Reader` for AE-2 and relies
/// on the AES authentication tag for integrity instead) and exempt those
/// entries from every CRC comparison in this file (R0079-0007) — and,
/// since DCR-012, from the listing's `crc32` field too.
///
/// **The gate is `encrypted() && crc == 0 && AE-2`, and the third
/// conjunct is load-bearing.** `encrypted() && crc == 0` alone is a
/// *superset* of AE-2: it also sweeps in any encrypted entry whose
/// payload is genuinely empty — a legacy ZipCrypto empty file, or an AE-1
/// empty file from a writer that does not use the `zip` crate's
/// "under 20 bytes ⇒ AE-2" rule. Those carry a *real* stored CRC32,
/// `CRC32(b"") == 0`, which AD 0012 says is a valid checksum and not an
/// absent one. Exempting them would drop a checksum the archive actually
/// carried, and (post-DCR-012) would list `crc32: None` and force a
/// needless decrypt-and-stream during the digest walk. Reading the
/// `0x9901` vendor version distinguishes the placeholder from the real
/// zero, so the gate now means what its name says.
///
/// Residual, stated rather than hidden: an AE-2 entry whose central
/// record omits the `0x9901` field is not recognised here. Such an entry
/// is unreadable anyway — the `zip` crate rejects
/// "AES encryption without AES extra data field" when parsing the central
/// directory — so it cannot reach a CRC comparison in the first place.
fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted()
        && zip_file.crc32() == 0
        && aes_vendor_version(zip_file) == Some(AES_VENDOR_VERSION_AE2)
}

/// Drain an entry's decoded payload, returning both its CRC32 and the
/// number of bytes produced.
///
/// R0001-0032: the shared `common::compute_crc32_reader` returns only a
/// checksum, so the integrity walk could not tell an entry that decoded
/// short from an intact one — a truncated payload whose stored CRC
/// matches the shortened bytes, or an AE-2 entry exempt from the CRC
/// compare, was reported clean. Counting here lets the caller require the
/// byte count to equal the authoritative central-directory size. Read
/// errors route through the same `map_entry_read_error` the shared helper
/// uses, so decoder-side CRC failures keep the R0079-0046 `Corruption`
/// mapping the caller classifies on.
fn drain_entry_crc32_counted<R: Read + ?Sized>(
    reader: &mut R,
    error_path: &Path,
) -> Result<(u32, u64)> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = [0u8; 8192];
    let mut bytes_read: u64 = 0;

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| super::common::map_entry_read_error(e, error_path))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        bytes_read += n as u64;
    }

    Ok((hasher.finalize(), bytes_read))
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

/// Read exactly `buf.len()` bytes of the raw central directory, keeping
/// the three outcomes the duplicate scan must distinguish apart.
///
/// R0001-0027: the scan previously ran `read_exact(..).is_err() { break }`
/// over a whole 46-byte header, which collapsed "reached the EOCD"
/// (expected), "the record is cut short" (truncation), and "the OS read
/// failed" (I/O fault) into one silent loop exit — and a scan that ends
/// early leaves `counts` incomplete, i.e. it *disables* duplicate
/// detection. The EOCD the `zip` crate already parsed always follows the
/// last central-directory record, so a short read here is a truncated
/// directory (`Corruption`); anything else is a real archive-file read
/// failure (`Io`). Normal termination is decided by the record signature,
/// never by a read error.
fn read_central_directory_exact(
    file: &mut File,
    buf: &mut [u8],
    path: &Path,
    what: &str,
) -> Result<()> {
    file.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ArchiveError::corruption(
                path.display().to_string(),
                format!("central directory ends mid-record: incomplete {}", what),
            )
        } else {
            ArchiveError::io("read", path.to_path_buf(), e)
        }
    })
}

/// Open a ZIP file and create a RawZipArchive
fn open_zip(path: &Path) -> Result<RawZipArchive<File>> {
    let file = File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;
    RawZipArchive::new(file)
        .map_err(|e| ArchiveError::format(Some(ArchiveFormat::Zip), format!("Invalid ZIP: {}", e)))
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
/// **Duplicate-name rejection (R0079-0026 / DCR-009 — AD 0007 collapse).**
/// The `zip` crate keys its central-directory map by exact name, so two
/// records sharing a byte-identical name collapse to a single listing
/// entry (the last record wins). The removed piz reader kept every record,
/// so its listing carried both and the single-entry gate refused the
/// ambiguity. To preserve that rejection now that `zip` is the sole ZIP
/// backend, `duplicate_guard` memoises a raw central-directory scan
/// (`scan_duplicate_names`) and the by-name single-entry paths refuse any
/// normalized path that appeared more than once in the raw directory.
pub struct ZipArchive {
    path: PathBuf,
    password: Option<Password>,
    cached_zip: std::sync::Mutex<Option<RawZipArchive<File>>>,
    listing: once_cell::sync::OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// Memoised raw-central-directory duplicate-name detection
    /// (R0079-0026 / DCR-009). Populated lazily on the first single-entry
    /// extraction.
    duplicate_guard: once_cell::sync::OnceCell<DuplicateGuard>,
}

/// Result of scanning the RAW central directory for duplicate normalized
/// entry names (R0079-0026 / DCR-009).
struct DuplicateGuard {
    /// Normalized paths that appear more than once across the raw
    /// central-directory records. The by-name single-entry paths refuse
    /// these because the `zip` crate's deduped listing would otherwise
    /// silently hand back the surviving (last) record's payload.
    names: HashSet<String>,
    /// `true` when at least one record the `zip` crate collapsed could not
    /// be attributed to a specific listed name (e.g. an exotic
    /// mixed-encoding collision whose two raw byte strings decode to one
    /// name, or a record the crate never parsed because the EOCD undercounts
    /// the directory). In that case every by-name single-entry extraction is
    /// refused rather than risk silently returning the wrong payload — a
    /// safe over-rejection for a genuinely ambiguous archive.
    ///
    /// R0001-0028: this is decided by counting, not by
    /// `names.is_empty()`. The old heuristic disarmed itself the moment a
    /// single ordinary duplicate was attributed, so a crafted archive could
    /// hide an unattributable collision behind an ordinary one.
    any_undetected: bool,
}

impl ZipArchive {
    /// Open ZIP archive for reading.
    ///
    /// **Validation timing (AD 0052):** the constructor only stores the
    /// path; the file is not even opened. Central-directory parsing and
    /// any other format-level validation happens lazily on the first call
    /// to `list_files`, `extract_*`, or `test_integrity`. A corrupt,
    /// truncated, or non-ZIP file therefore surfaces its error at first
    /// use, not at `open()` time.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        Ok(Self {
            path: path_buf,
            password: None,
            cached_zip: std::sync::Mutex::new(None),
            listing: once_cell::sync::OnceCell::new(),
            duplicate_guard: once_cell::sync::OnceCell::new(),
        })
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
            slot => slot.insert(open_zip(&self.path)?),
        };
        f(zip)
    }

    /// Open encrypted ZIP archive with password.
    ///
    /// Same lazy-validation semantics as [`ZipArchive::open`] — the
    /// password is stored alongside the path, but neither the archive
    /// shape nor the password's correctness is checked until the caller
    /// requests entry data. A wrong password surfaces from the
    /// `extract_*` paths via `ArchiveError::Password`.
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
        self.listing
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
            .map(std::sync::Arc::clone)
    }

    /// Refuse a by-name single-entry extraction when the requested
    /// normalized path is ambiguous in the RAW central directory
    /// (R0079-0026 / DCR-009). Runs immediately after
    /// [`crate::security::validate_single_entry`] returns a unique listing
    /// match, so the two guards compose: `validate_single_entry` catches
    /// distinct raw names that normalize to one path (they survive as
    /// separate listing entries), and this catches byte-identical names the
    /// `zip` crate collapsed into a single deduped record.
    fn reject_if_duplicate(&self, validated_path: &str, op: &'static str) -> Result<()> {
        let guard = self
            .duplicate_guard
            .get_or_try_init(|| self.scan_duplicate_names())?;
        if guard.any_undetected || guard.names.contains(validated_path) {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: multiple_entries_reason(validated_path),
            });
        }
        Ok(())
    }

    /// Walk the RAW central directory and collect the listed paths that are
    /// backed by more than one record (R0079-0026 / DCR-009).
    ///
    /// The `zip` crate exposes only its name-deduped central-directory map,
    /// so this re-reads the directory from `central_directory_start()`
    /// (already absolute — it folds in any SFX/prepended-data offset).
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
    /// collapse escaped attribution, and `any_undetected` then refuses the
    /// archive's whole by-name single-entry surface.
    fn scan_duplicate_names(&self) -> Result<DuplicateGuard> {
        const CD_HEADER_SIGNATURE: u32 = 0x0201_4b50;
        const EOCD_SIGNATURE: u32 = 0x0605_4b50;
        const ZIP64_EOCD_SIGNATURE: u32 = 0x0606_4b50;
        const ZIP64_EOCD_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
        const DIGITAL_SIGNATURE: u32 = 0x0505_4b50;
        const CD_SIGNATURE_LEN: usize = 4;
        const CD_FIXED_HEADER_LEN: usize = 46;

        // R0001-0029: raw name bytes -> the normalized path(s) the public
        // listing exposes for them. `by_index_raw` reads only the already
        // parsed central-directory metadata, so this is an in-memory walk.
        // One raw spelling can back several listed names when two records
        // share their bytes but disagree on the UTF-8 flag; keeping all of
        // them means every ambiguous listed path is refused.
        let (cd_start, deduped_len, listed_by_raw_name) = self.with_zip(|zip| {
            let mut listed_by_raw_name: HashMap<Vec<u8>, HashSet<String>> = HashMap::new();
            for i in 0..zip.len() {
                let raw = zip.by_index_raw(i).map_err(|e| {
                    ArchiveError::format(
                        Some(ArchiveFormat::Zip),
                        format!("Read entry {}: {}", i, e),
                    )
                })?;
                listed_by_raw_name
                    .entry(raw.name_raw().to_vec())
                    .or_default()
                    .insert(normalize_path(raw.name()));
            }
            Ok((zip.central_directory_start(), zip.len(), listed_by_raw_name))
        })?;

        let mut file =
            File::open(&self.path).map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;
        file.seek(SeekFrom::Start(cd_start))
            .map_err(|e| ArchiveError::io("seek", self.path.clone(), e))?;

        let mut counts: HashMap<Vec<u8>, u32> = HashMap::new();
        let mut raw_records: u64 = 0;

        loop {
            // R0001-0027: dispatch on the record signature, read on its own
            // first. Only a recognised terminator ends the walk; a truncated
            // record or an OS read error is a typed failure, never a quiet
            // `break` that would leave duplicate detection half-done.
            let mut signature = [0u8; CD_SIGNATURE_LEN];
            read_central_directory_exact(
                &mut file,
                &mut signature,
                &self.path,
                "record signature",
            )?;
            match u32::from_le_bytes(signature) {
                CD_HEADER_SIGNATURE => {}
                // The directory is terminated by the EOCD, optionally
                // preceded by an archive digital-signature record and/or the
                // ZIP64 EOCD record + its locator. All are legitimate ends.
                EOCD_SIGNATURE
                | ZIP64_EOCD_SIGNATURE
                | ZIP64_EOCD_LOCATOR_SIGNATURE
                | DIGITAL_SIGNATURE => break,
                other => {
                    return Err(ArchiveError::corruption(
                        self.path.display().to_string(),
                        format!(
                            "central directory: unexpected record signature {:#010x} after {} record(s)",
                            other, raw_records
                        ),
                    ));
                }
            }

            // Offsets are the fixed header's, less the 4 signature bytes
            // already consumed: name/extra/comment lengths live at 28/30/32.
            let mut rest = [0u8; CD_FIXED_HEADER_LEN - CD_SIGNATURE_LEN];
            read_central_directory_exact(&mut file, &mut rest, &self.path, "record header")?;
            let name_len = u16::from_le_bytes([rest[24], rest[25]]) as usize;
            let extra_len = u16::from_le_bytes([rest[26], rest[27]]) as usize;
            let comment_len = u16::from_le_bytes([rest[28], rest[29]]) as usize;

            let mut name_bytes = vec![0u8; name_len];
            read_central_directory_exact(&mut file, &mut name_bytes, &self.path, "entry name")?;
            file.seek(SeekFrom::Current((extra_len + comment_len) as i64))
                .map_err(|e| ArchiveError::io("seek", self.path.clone(), e))?;

            *counts.entry(name_bytes).or_insert(0) += 1;
            raw_records += 1;
        }

        let mut names: HashSet<String> = HashSet::new();
        let mut attributed: u64 = 0;
        for (raw_name, count) in counts {
            if count <= 1 {
                continue;
            }
            // Only a duplicate whose raw bytes map onto listed paths is
            // rejectable by name; anything else falls through to the global
            // refusal below (R0001-0028 / R0001-0029).
            if let Some(listed) = listed_by_raw_name.get(&raw_name) {
                attributed += u64::from(count).saturating_sub(listed.len() as u64);
                names.extend(listed.iter().cloned());
            }
        }

        // R0001-0028: compare the collapse counts instead of asking whether
        // any name was attributed at all — one ordinary duplicate must not
        // disarm the guard for a second, unattributable collision. A raw
        // count *below* the crate's own is equally unexplained (the EOCD
        // undercounts the directory, or the two views disagree about where
        // it starts), so treat that as ambiguous too.
        let collapsed = raw_records.saturating_sub(deduped_len as u64);
        let any_undetected = attributed < collapsed || raw_records < deduped_len as u64;

        Ok(DuplicateGuard {
            names,
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

        let mut entry = ArchiveEntry::new(path, 0);
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
                    std::fs::create_dir_all(parent)
                        .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
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
                std::fs::create_dir_all(parent)
                    .map_err(|e| ArchiveError::io("create_dir", parent.to_path_buf(), e))?;
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
    pub fn test_integrity(&self) -> Result<Vec<String>> {
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
    /// AE-2 entries stay exempt from the CRC compare here for the same
    /// R0079-0007 reason the other read paths do: comparing decrypted bytes
    /// against the placeholder 0 would flag every non-empty entry corrupt.
    /// AE-1 and plaintext entries keep their compare.
    ///
    /// Same buffered-adapter caveat as [`Self::extract_to_stream`]: the whole
    /// entry is materialized before the cursor is handed back (DEF-004 /
    /// OI-0057-007), so digesting a large AE-2 member costs its full size in
    /// memory plus full decryption.
    pub(crate) fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
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
mod tests {
    use super::*;
    use crate::test_utils::fixture;

    #[test]
    fn test_zip_wrapper_open_valid() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path);
        assert!(archive.is_ok());
        assert_eq!(archive.unwrap().path(), path);
    }

    #[test]
    fn test_zip_wrapper_open_nonexistent_list_fails() {
        // open() succeeds (lazy open), but list_files() fails when file doesn't exist
        let archive = ZipArchive::open("/nonexistent/archive.zip").unwrap();
        let result = archive.list_files();
        assert!(result.is_err());
    }

    /// R0001-0084: exercise decryption, not just password storage. This
    /// used to open the UNENCRYPTED `test.zip` with an arbitrary password
    /// and assert only the stored path, so an encrypted-open regression
    /// could not fail it. `test_encrypted.zip` is a ZipCrypto archive with
    /// readable headers, password `test123` (see `tests/fixtures/README.md`
    /// and `tests/review_0068_test.rs`), holding a copy of `test_file.txt`.
    #[test]
    fn test_zip_wrapper_open_with_password() {
        let path = fixture("test_encrypted.zip");
        let expected = std::fs::read(fixture("test_file.txt")).unwrap();

        let archive = ZipArchive::open_with_password(&path, "test123").unwrap();
        assert_eq!(archive.path(), path);
        assert_eq!(
            archive.extract_to_memory("test_file.txt").unwrap(),
            expected,
            "the correct password must yield the entry plaintext"
        );

        // Headers are not encrypted: listing works without a password and
        // reports the entry as encrypted (AD 0014 defers the password check
        // to extraction).
        let entries = archive.list_files().unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.path == "test_file.txt" && e.is_encrypted),
            "the entry must be listed and flagged encrypted"
        );

        // No password at all: the zip crate refuses to build a reader.
        let no_password = ZipArchive::open(&path).unwrap();
        match no_password.extract_to_memory("test_file.txt") {
            Err(ArchiveError::Format { .. }) | Err(ArchiveError::Password { .. }) => {}
            other => panic!("missing password must not yield plaintext, got {other:?}"),
        }

        // Wrong password: ZipCrypto's check byte rejects nearly every wrong
        // key (`Password`); on a check-byte collision the decrypted bytes
        // still fail the central-directory CRC32 (`Corruption`). Neither
        // path may return data.
        let wrong = ZipArchive::open_with_password(&path, "not-the-password").unwrap();
        match wrong.extract_to_memory("test_file.txt") {
            Err(ArchiveError::Password { .. }) | Err(ArchiveError::Corruption { .. }) => {}
            other => panic!("wrong password must not yield plaintext, got {other:?}"),
        }
    }

    #[test]
    fn test_zip_wrapper_list_files() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files();
        assert!(entries.is_ok());
        let entries = entries.unwrap();
        assert!(!entries.is_empty());

        for entry in entries.iter() {
            assert!(!entry.path.is_empty());
            // zip crate provides CRC32 from metadata
            if entry.entry_type == EntryType::File {
                assert!(
                    entry.crc32.is_some(),
                    "File entry '{}' should have CRC32 from zip metadata",
                    entry.path
                );
            }
        }
    }

    #[test]
    fn test_zip_wrapper_extract_to_memory() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let data = archive.extract_to_memory("test_file.txt");
        assert!(data.is_ok());
        assert!(!data.unwrap().is_empty());
    }

    #[test]
    fn test_zip_wrapper_extract_to_memory_nonexistent() {
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let result = archive.extract_to_memory("does_not_exist.txt");
        assert!(result.is_err());
    }

    /// R0076-0048: an entry stored with backslash separators is listed
    /// under the normalized (`/`) name; by-name opening must find it by
    /// that listed name instead of requiring the raw stored spelling.
    #[test]
    fn test_zip_wrapper_backslash_entry_opens_by_listed_name() {
        use std::io::Write as _;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("backslash.zip");
        let payload = b"backslash payload";

        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("a\\b.txt", options).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a/b.txt", "listing must normalize");

        let data = archive
            .extract_to_memory(&entries[0].path)
            .expect("listed (normalized) name must resolve the stored entry");
        assert_eq!(data, payload);
    }

    /// OI-0080-003: build a ZIP with many tiny stored entries for the
    /// parse-time entry-count budget tests.
    fn build_many_entry_zip(path: &Path, count: usize) {
        use std::io::Write as _;

        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for i in 0..count {
            writer
                .start_file(format!("entry_{i}.txt"), options)
                .unwrap();
            writer.write_all(b"x").unwrap();
        }
        writer.finish().unwrap();
    }

    /// OI-0080-003: an over-budget listing aborts with the parse-time
    /// `OperationBlocked` shape, and the aborted parse does not poison the
    /// backend cache — a later unbudgeted call still materializes.
    #[test]
    fn test_zip_list_files_budget_aborts_over_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("many.zip");
        build_many_entry_zip(&path, 50);

        let archive = ZipArchive::open(&path).unwrap();
        let err = archive.list_files_budgeted(Some(10)).unwrap_err();
        match err {
            ArchiveError::OperationBlocked { reason, .. } => assert!(
                reason.contains("parsed more than 10 entries"),
                "unexpected reason: {reason}"
            ),
            other => panic!("expected OperationBlocked, got {other:?}"),
        }

        // No cache poisoning: the aborted budgeted parse left `listing` empty,
        // so a later unbudgeted parse on the same handle still succeeds.
        let entries = archive.list_files_budgeted(None).unwrap();
        assert_eq!(entries.len(), 50);
    }

    /// OI-0080-003: an unbudgeted listing (`budget = None`) succeeds.
    #[test]
    fn test_zip_list_files_budget_none_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("many.zip");
        build_many_entry_zip(&path, 50);

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files_budgeted(None).unwrap();
        assert_eq!(entries.len(), 50);
    }

    /// OI-0080-003 / AD 0065 (documented behavior): once the listing is
    /// materialized, a budgeted call hits the cache and returns the cached
    /// `Arc` unchanged — the budget applies only to the first parse.
    #[test]
    fn test_zip_list_files_budget_ignored_on_cache_hit() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("many.zip");
        build_many_entry_zip(&path, 50);

        let archive = ZipArchive::open(&path).unwrap();
        // Populate the cache unbudgeted.
        assert_eq!(archive.list_files_budgeted(None).unwrap().len(), 50);
        // Cache hit: the smaller budget is ignored, cached 50 entries returned.
        assert_eq!(archive.list_files_budgeted(Some(10)).unwrap().len(), 50);
    }

    #[test]
    fn test_zip_wrapper_extract_to_stream() {
        use std::io::Read;
        let path = fixture("test.zip");
        let archive = ZipArchive::open(&path).unwrap();
        let mut stream = archive.extract_to_stream("test_file.txt").unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    /// Build an AES-encrypted single-entry ZIP. The zip crate writes the
    /// AE-2 variant (stored CRC32 = 0) for payloads under 20 bytes and
    /// AE-1 (real stored CRC32) otherwise.
    fn build_aes_zip(path: &Path, payload: &[u8], password: &str) {
        use std::io::Write as _;

        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .with_aes_encryption(zip::AesMode::Aes256, password);
        writer.start_file("secret.txt", options).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
    }

    /// R0079-0007: AE-2 entries store CRC32 = 0 in the central directory,
    /// so the wrapper-level CRC comparison must be skipped — integrity is
    /// covered by the AES authentication tag instead.
    #[test]
    fn test_zip_wrapper_ae2_encrypted_not_flagged_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae2.zip");
        let payload = b"tiny"; // < 20 bytes => AE-2 (placeholder CRC 0)
        build_aes_zip(&path, payload, "pw");

        let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
        let data = archive.extract_to_memory("secret.txt").unwrap();
        assert_eq!(data, payload);

        let failed = archive.test_integrity().unwrap();
        assert!(
            failed.is_empty(),
            "AE-2 entry falsely reported corrupt: {:?}",
            failed
        );

        let dest = tmp.path().join("out");
        archive
            .extract_all_with_options(&dest, None, true, true, true, true, None)
            .unwrap();
        assert_eq!(std::fs::read(dest.join("secret.txt")).unwrap(), payload);
    }

    /// AE-1 entries (>= 20 bytes) keep a real stored CRC32, so the
    /// wrapper-level verification must still run for them.
    #[test]
    fn test_zip_wrapper_ae1_encrypted_crc_still_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae1.zip");
        let payload = b"payload long enough for AE-1";
        build_aes_zip(&path, payload, "pw");

        let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
        let data = archive.extract_to_memory("secret.txt").unwrap();
        assert_eq!(data, payload);
        assert!(archive.test_integrity().unwrap().is_empty());
    }

    /// Build a single-entry, unencrypted, stored ZIP.
    fn build_plain_zip(path: &Path, name: &str, payload: &[u8]) {
        use std::io::Write as _;

        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file(name, options).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
    }

    /// DCR-012: an AE-2 entry's central-directory CRC32 is the
    /// specification's placeholder 0, not a checksum, so the listing must
    /// report `None`. Reporting `Some(0)` made every AE-2 entry fold the
    /// same constant into the content-multiset digest, so two AES ZIPs with
    /// different contents collided.
    #[test]
    fn test_zip_wrapper_ae2_entry_lists_crc32_none() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae2-listing.zip");
        let payload = b"tiny"; // < 20 bytes => AE-2 (placeholder CRC 0)
        build_aes_zip(&path, payload, "pw");

        let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "secret.txt")
            .expect("entry listed");
        assert_eq!(entry.entry_type, EntryType::File);
        assert!(entry.is_encrypted, "AE-2 entry must list as encrypted");
        assert_eq!(
            entry.crc32, None,
            "AE-2 placeholder CRC must not be listed as a checksum"
        );
    }

    /// The DCR-012 gate keys on the AE-2 *placeholder*, not on encryption:
    /// an AE-1 entry (>= 20 bytes) carries a real stored CRC32, so it must
    /// still be listed.
    #[test]
    fn test_zip_wrapper_ae1_entry_still_lists_stored_crc32() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae1-listing.zip");
        let payload = b"payload long enough for AE-1";
        build_aes_zip(&path, payload, "pw");

        let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "secret.txt")
            .expect("entry listed");
        assert!(entry.is_encrypted);
        assert_eq!(entry.crc32, Some(crc32fast::hash(payload)));
    }

    /// AD 0012 anti-regression: a CRC32 *value* of 0 is a valid checksum —
    /// `CRC32(b"") == 0` — so a plaintext empty file must keep listing
    /// `Some(0)`. This test fails if anyone re-widens the DCR-012 gate from
    /// `encrypted() && crc == 0` to `crc == 0`.
    #[test]
    fn test_zip_wrapper_plaintext_empty_file_still_lists_crc32_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.zip");
        build_plain_zip(&path, "empty.txt", b"");

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "empty.txt")
            .expect("entry listed");
        assert!(!entry.is_encrypted);
        assert_eq!(
            entry.crc32,
            Some(0),
            "AD 0012: CRC32 value 0 is a valid checksum, not an absent one"
        );
    }

    /// Build a ZIP holding one Stored, zero-byte entry that is flagged
    /// **encrypted** but carries **no** AES extra field — the shape a
    /// legacy ZipCrypto empty file has, and the exact case the
    /// pre-narrowing gate `encrypted() && crc == 0` swept in:
    /// `CRC32(b"") == 0` is a *real* stored checksum here, not the AE-2
    /// placeholder.
    ///
    /// The `zip` crate cannot write one directly
    /// (`with_deprecated_encryption` is crate-private), so the fixture is
    /// a crate-written plaintext empty file with general-purpose flag bit
    /// 0 set in both the local and the central header. Building it by
    /// mutating a well-formed archive rather than hand-assembling one
    /// keeps every other field exactly as the writer emitted it, so a
    /// parse failure here would be about the flag, not about the fixture.
    ///
    /// The payload bytes a real ZipCrypto entry carries (a 12-byte
    /// encryption header) are absent; nothing in this test reads entry
    /// data, and `list_files` reads the central directory only.
    fn build_encrypted_empty_zip_without_aes_field(path: &Path, name: &str) {
        const FLAG_OFFSET_IN_LOCAL_HEADER: usize = 6;
        const FLAG_OFFSET_IN_CENTRAL_HEADER: usize = 8;
        const LOCAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
        const CENTRAL_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];

        build_plain_zip(path, name, b"");
        let mut bytes = std::fs::read(path).unwrap();

        // Single-entry archive with an empty payload, so the first
        // occurrence of each signature is the header we want.
        let set_encrypted_flag = |bytes: &mut Vec<u8>, signature: [u8; 4], flag_offset: usize| {
            let at = bytes
                .windows(4)
                .position(|w| w == signature)
                .expect("signature present in a crate-written archive");
            let flag_at = at + flag_offset;
            bytes[flag_at] |= 0x01;
        };
        set_encrypted_flag(&mut bytes, LOCAL_SIGNATURE, FLAG_OFFSET_IN_LOCAL_HEADER);
        set_encrypted_flag(&mut bytes, CENTRAL_SIGNATURE, FLAG_OFFSET_IN_CENTRAL_HEADER);

        std::fs::write(path, &bytes).unwrap();
    }

    /// AD-0012 anti-regression for the *narrowed* gate. The pre-narrowing
    /// predicate `encrypted() && crc == 0` was a superset of AE-2: an
    /// encrypted entry whose payload is genuinely empty carries a real
    /// stored `CRC32(b"") == 0`, and exempting it discarded a checksum the
    /// archive did carry (and, post-DCR-012, forced a pointless
    /// decrypt-and-stream during the digest walk). Requiring the `0x9901`
    /// vendor version to be AE-2 keeps it listed as `Some(0)`.
    #[test]
    fn test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("zipcrypto-empty.zip");
        build_encrypted_empty_zip_without_aes_field(&path, "empty.txt");

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "empty.txt")
            .expect("entry listed");
        assert!(
            entry.is_encrypted,
            "the fixture must actually carry the encrypted flag, \
             otherwise it does not exercise the gate"
        );
        assert_eq!(
            entry.crc32,
            Some(0),
            "an encrypted empty file's zero CRC is a real checksum (AD 0012), \
             not the AE-2 placeholder — the gate must not sweep it in"
        );
    }

    /// The `0x9901` parser reads the vendor version out of an extra-field
    /// chain, tolerates a preceding field, and refuses malformed input
    /// rather than guessing. Exercised through a real AE-1 / AE-2 archive
    /// below; this pins the byte-level behaviour the gate depends on.
    #[test]
    fn test_zip_wrapper_aes_vendor_version_reads_ae1_and_ae2() {
        let tmp = tempfile::tempdir().unwrap();

        // AE-2: the zip writer picks it for payloads under 20 bytes.
        let ae2 = tmp.path().join("ae2-vendor.zip");
        build_aes_zip(&ae2, b"tiny", "pw");
        let ae2_archive = ZipArchive::open(&ae2).unwrap();
        let ae2_entry = ae2_archive
            .list_files()
            .unwrap()
            .iter()
            .find(|e| e.path == "secret.txt")
            .cloned()
            .expect("entry listed");
        assert_eq!(
            ae2_entry.crc32, None,
            "AE-2 placeholder CRC must not be listed as a checksum"
        );

        // AE-1: 20 bytes or more, so the writer keeps the real CRC32.
        let ae1 = tmp.path().join("ae1-vendor.zip");
        let payload = b"payload long enough for AE-1";
        build_aes_zip(&ae1, payload, "pw");
        let ae1_archive = ZipArchive::open(&ae1).unwrap();
        let ae1_entry = ae1_archive
            .list_files()
            .unwrap()
            .iter()
            .find(|e| e.path == "secret.txt")
            .cloned()
            .expect("entry listed");
        assert!(ae1_entry.is_encrypted);
        assert_eq!(
            ae1_entry.crc32,
            Some(crc32fast::hash(payload)),
            "AE-1 stores a real CRC32 and must keep it"
        );
    }

    /// DCR-012: with the listing reporting `None`, the digest walk streams
    /// AE-2 entries by listing id. The id-addressed path must return the
    /// decrypted payload (and must not trip the AE-2 CRC compare), because
    /// it is what keeps a duplicate-name AES ZIP digestible at all
    /// (OI-0076-002).
    #[test]
    fn test_zip_wrapper_extract_to_stream_by_listing_id_ae2() {
        use std::io::Read as _;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae2-by-id.zip");
        let payload = b"tiny"; // AE-2
        build_aes_zip(&path, payload, "pw");

        let archive = ZipArchive::open_with_password(&path, "pw").unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "secret.txt")
            .expect("entry listed");

        let mut stream = archive
            .extract_to_stream_by_listing_id(entry.id, &entry.path)
            .unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, payload);

        // The drift guard still binds: a mismatched validated name is refused.
        assert!(
            archive
                .extract_to_stream_by_listing_id(entry.id, "other.txt")
                .is_err()
        );
    }

    /// Without a password the AE-2 payload cannot be read, so the
    /// id-addressed stream the digest walk uses must fail rather than
    /// substitute a value. Listing itself keeps working (AD 0014).
    ///
    /// Only `is_err()` is asserted: TicGit `9bdf2c` owns tightening the ZIP
    /// no-password read path from `ArchiveError::Format` to
    /// `ArchiveError::Password`, and that ticket should be free to sharpen
    /// this assertion without reopening DCR-012.
    #[test]
    fn test_zip_wrapper_ae2_stream_by_id_without_password_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ae2-nopw.zip");
        build_aes_zip(&path, b"tiny", "pw");

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "secret.txt")
            .expect("listing an encrypted ZIP needs no password (AD 0014)");
        assert_eq!(entry.crc32, None);

        assert!(
            archive
                .extract_to_stream_by_listing_id(entry.id, &entry.path)
                .is_err(),
            "an AE-2 entry cannot be digested without the password"
        );
    }

    /// R0079-0019: `preserve_permissions` / `preserve_times` must be
    /// honoured by the zip-crate extract paths — a 0o755 entry keeps
    /// its exec bit and archive mtime when the flags are set, and gets
    /// neither when they are cleared.
    #[test]
    #[cfg(unix)]
    fn test_zip_wrapper_extract_preserves_mode_and_mtime_per_flags() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mode.zip");
        crate::test_utils::build_zip_with_mode_and_mtime(&path);
        let expected_mtime =
            crate::ffi::common::ymd_hms_to_system_time(2010, 1, 2, 3, 4, 6).unwrap();

        let archive = ZipArchive::open(&path).unwrap();

        let preserved = tmp.path().join("preserved");
        archive
            .extract_all_with_options(&preserved, None, true, true, true, false, None)
            .unwrap();
        let meta = std::fs::metadata(preserved.join("tool.sh")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
        assert_eq!(meta.modified().unwrap(), expected_mtime);

        let plain = tmp.path().join("plain");
        archive
            .extract_all_with_options(&plain, None, true, false, false, false, None)
            .unwrap();
        let meta = std::fs::metadata(plain.join("tool.sh")).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o111,
            0,
            "exec bits must not be applied when preserve_permissions = false"
        );
        assert_ne!(meta.modified().unwrap(), expected_mtime);

        // Single-file path honours the same flags.
        let single = tmp.path().join("single");
        archive
            .extract_file_with_options_preserve("tool.sh", &single, true, true, true, false)
            .unwrap();
        let meta = std::fs::metadata(single.join("tool.sh")).unwrap();
        assert_eq!(meta.permissions().mode() & 0o7777, 0o755);
        assert_eq!(meta.modified().unwrap(), expected_mtime);
    }

    #[test]
    fn test_zip_wrapper_memory_and_stream_same_data() {
        use std::io::Read;
        let path = fixture("test.zip");

        let archive1 = ZipArchive::open(&path).unwrap();
        let mem_data = archive1.extract_to_memory("test_file.txt").unwrap();

        let archive2 = ZipArchive::open(&path).unwrap();
        let mut stream = archive2.extract_to_stream("test_file.txt").unwrap();
        let mut stream_data = Vec::new();
        stream.read_to_end(&mut stream_data).unwrap();

        assert_eq!(mem_data, stream_data);
    }

    /// R0080-0030: a corrupt regular-file payload must be recorded in the
    /// failed list, not abort `test_integrity` with an error. Only genuine
    /// archive-file I/O errors propagate.
    #[test]
    fn test_zip_wrapper_integrity_records_corrupt_payload() {
        use std::io::Write as _;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("corrupt.zip");
        let payload = b"unified-archive integrity payload marker bytes";

        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("data.txt", options).unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();

        // Flip one stored-payload byte on disk so its CRC no longer matches
        // the central-directory value, leaving all framing intact.
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes
            .windows(payload.len())
            .position(|w| w == &payload[..])
            .expect("stored payload present in archive");
        bytes[at] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();

        let archive = ZipArchive::open(&path).unwrap();
        let failed = archive
            .test_integrity()
            .expect("corrupt payload must be recorded, not abort the walk");
        assert_eq!(failed, vec!["data.txt".to_string()]);
    }

    /// R0081-0076 / R0081-0077: the shared classifier maps every Unix
    /// `S_IFMT` type to a single, stable `EntryType`, so both ZIP backends
    /// agree. Verified directly over raw mode bits (no archive needed).
    #[test]
    fn test_classify_zip_entry_type_over_mode_bits() {
        // With a Unix mode present, the type nibble is authoritative.
        assert_eq!(
            classify_zip_entry_type(Some(0o100644), false),
            EntryType::File
        );
        assert_eq!(
            classify_zip_entry_type(Some(0o040755), true),
            EntryType::Directory
        );
        assert_eq!(
            classify_zip_entry_type(Some(0o120777), false),
            EntryType::Symlink
        );
        // Symlink is decided before the directory-style name (ordering kept).
        assert_eq!(
            classify_zip_entry_type(Some(0o120777), true),
            EntryType::Symlink
        );
        // Every remaining Unix type -> Other (FIFO, char/block device,
        // socket, and any other non-standard nibble).
        for special in [0o010644u32, 0o020644, 0o060644, 0o140644, 0o160644] {
            assert_eq!(
                classify_zip_entry_type(Some(special), false),
                EntryType::Other,
                "mode {:o} must classify as Other",
                special
            );
        }
        // A mode with no type nibble falls back to the directory-name hint.
        assert_eq!(
            classify_zip_entry_type(Some(0o000644), false),
            EntryType::File
        );
        assert_eq!(
            classify_zip_entry_type(Some(0o000644), true),
            EntryType::Directory
        );
        // No Unix mode at all: name hint only.
        assert_eq!(classify_zip_entry_type(None, false), EntryType::File);
        assert_eq!(classify_zip_entry_type(None, true), EntryType::Directory);
    }

    /// Build a single-entry ZIP ("special") whose central-directory Unix
    /// mode carries the `S_IFIFO` type nibble — a special (non-regular,
    /// non-symlink, non-directory) entry both ZIP backends must classify as
    /// `EntryType::Other`. `unix_permissions` masks to the low 12 bits, so
    /// the type nibble is injected by patching the external-attributes field
    /// of the central-directory header directly (no header is checksummed).
    fn build_zip_with_special_mode_entry(path: &Path) {
        use std::io::Write as _;

        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);
        writer.start_file("special", options).unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();

        // Locate the central-directory header (signature "PK\x01\x02"). Its
        // 4-byte external-file-attributes field is at CDH offset 38, and the
        // Unix mode lives in the high 16 bits, so the byte carrying the
        // `S_IFMT` type nibble sits at CDH offset 41. Turning its high nibble
        // from 0 (mode 0o644 has no type bits) into 1 yields S_IFIFO.
        let mut bytes = std::fs::read(path).unwrap();
        let cdh = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .expect("central-directory header present");
        bytes[cdh + 41] |= 0x10;
        std::fs::write(path, &bytes).unwrap();
    }

    /// R0081-0076 / R0081-0077: a special-mode entry must be classified
    /// `EntryType::Other` and materialized by neither listing nor
    /// extraction. (Pre-AD-0007 this asserted parity across the two ZIP
    /// backends; the `zip` crate is now the sole ZIP backend.)
    #[test]
    fn test_zip_special_mode_entry_classified_other() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("special.zip");
        build_zip_with_special_mode_entry(&path);

        let zip = ZipArchive::open(&path).unwrap();
        let zip_entries = zip.list_files().unwrap();
        assert_eq!(zip_entries.len(), 1);
        assert_eq!(
            zip_entries[0].entry_type,
            EntryType::Other,
            "zip backend must classify a special-mode entry as Other"
        );

        // The special entry must not be materialized on extract_all.
        let zip_dest = tmp.path().join("zip_out");
        zip.extract_all(&zip_dest, None).unwrap();
        assert!(
            !zip_dest.join("special").exists(),
            "zip backend must not materialize a special-mode entry"
        );
    }

    // ── raw-central-directory fixtures (R0001-0027 / R0001-0028 /
    //    R0001-0029 / R0001-0031 / R0001-0032) ──
    //
    // Entry names and the EOCD counters carry no checksum, so a fixture
    // can be patched in place after `ZipWriter::finish` — the R0079-0026
    // technique the integration parity suite already uses.

    /// Build a stored-only ZIP with one entry per name. Payloads are
    /// name-independent so byte-patching a name never disturbs a CRC.
    fn build_stored_zip(path: &Path, names: &[&str]) {
        use std::io::Write as _;

        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (i, name) in names.iter().enumerate() {
            writer.start_file(*name, options).unwrap();
            writer.write_all(format!("payload-{i}").as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    fn le_u16(bytes: &[u8], at: usize) -> usize {
        u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize
    }

    fn le_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    /// Offset of the EOCD record (these fixtures carry no archive comment).
    fn eocd_offset(bytes: &[u8]) -> usize {
        bytes
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
            .expect("EOCD present")
    }

    /// Byte offsets of every central-directory record, in stored order.
    fn central_record_offsets(bytes: &[u8]) -> Vec<usize> {
        let mut at = le_u32(bytes, eocd_offset(bytes) + 16) as usize;
        let mut offsets = Vec::new();
        while at + 46 <= bytes.len() && bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
            offsets.push(at);
            at += 46 + le_u16(bytes, at + 28) + le_u16(bytes, at + 30) + le_u16(bytes, at + 32);
        }
        offsets
    }

    /// Rewrite both EOCD entry counters so the `zip` crate parses only the
    /// first `count` central-directory records while the file still
    /// physically carries more.
    fn set_eocd_entry_count(bytes: &mut [u8], count: u16) {
        let eocd = eocd_offset(bytes);
        bytes[eocd + 8..eocd + 10].copy_from_slice(&count.to_le_bytes());
        bytes[eocd + 10..eocd + 12].copy_from_slice(&count.to_le_bytes());
    }

    /// Rename every occurrence of `from` to `to` — both the local and the
    /// central header. Lengths must match so no offset moves.
    fn rename_entry_bytes(bytes: &mut [u8], from: &[u8], to: &[u8]) {
        assert_eq!(from.len(), to.len(), "in-place rename needs equal lengths");
        let mut i = 0;
        while i + from.len() <= bytes.len() {
            if &bytes[i..i + from.len()] == from {
                bytes[i..i + from.len()].copy_from_slice(to);
                i += from.len();
            } else {
                i += 1;
            }
        }
    }

    /// R0001-0029: a duplicated non-UTF-8 name must be refused under the
    /// path the *listing* exposes. The scan used to tally raw names through
    /// `String::from_utf8_lossy`, recording a replacement-character key that
    /// the crate's CP437-decoded listing name can never equal — so the
    /// duplicate was detected and then never matched by `reject_if_duplicate`.
    #[test]
    fn test_zip_duplicate_non_utf8_name_refused_under_listed_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("dup_cp437.zip");
        build_stored_zip(&path, &["dup0.txt", "dup1.txt"]);

        // The entries are written with ASCII names, so the general-purpose
        // UTF-8 flag stays clear and the crate decodes the patched bytes as
        // CP437. 0xE9 is not valid UTF-8.
        let mut bytes = std::fs::read(&path).unwrap();
        rename_entry_bytes(&mut bytes, b"dup0.txt", b"d\xE9p0.txt");
        rename_entry_bytes(&mut bytes, b"dup1.txt", b"d\xE9p0.txt");
        std::fs::write(&path, &bytes).unwrap();

        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(entries.len(), 1, "the zip crate collapses the duplicate");

        let listed = entries[0].path.clone();
        assert!(
            !listed.contains('\u{FFFD}'),
            "listed path is the crate's CP437 decode, not a lossy one: {listed}"
        );
        match archive.extract_to_memory(&listed) {
            Err(ArchiveError::OperationBlocked { .. }) => {}
            other => panic!("duplicated non-UTF-8 name must be refused, got {other:?}"),
        }
    }

    /// R0001-0028: an attributable duplicate must not disarm the
    /// unexplained-surplus refusal. The guard used to require
    /// `names.is_empty()`, so one attributed duplicate re-enabled every
    /// other by-name extraction even when the raw directory carried records
    /// the `zip` crate never accounted for.
    #[test]
    fn test_zip_duplicate_surplus_not_masked_by_attributed_duplicate() {
        let tmp = tempfile::tempdir().unwrap();

        // Control: the surplus is fully attributed to `dup0.txt`, so only
        // that name is refused and unrelated entries stay extractable.
        let plain = tmp.path().join("dup_only.zip");
        build_stored_zip(&plain, &["a.txt", "dup0.txt", "dup1.txt", "z.txt"]);
        let mut bytes = std::fs::read(&plain).unwrap();
        rename_entry_bytes(&mut bytes, b"dup1.txt", b"dup0.txt");
        std::fs::write(&plain, &bytes).unwrap();

        let archive = ZipArchive::open(&plain).unwrap();
        archive
            .extract_to_memory("a.txt")
            .expect("a fully attributed surplus must not refuse unrelated entries");
        match archive.extract_to_memory("dup0.txt") {
            Err(ArchiveError::OperationBlocked { .. }) => {}
            other => panic!("duplicated name must be refused, got {other:?}"),
        }

        // Same bytes, but the EOCD undercounts the directory by one: a
        // fourth raw record exists that the crate never parsed. `dup0.txt`
        // is still attributable, yet one collapse is not — the whole
        // by-name single-entry surface must now be refused.
        let masked = tmp.path().join("dup_plus_surplus.zip");
        let mut bytes = std::fs::read(&plain).unwrap();
        set_eocd_entry_count(&mut bytes, 3);
        std::fs::write(&masked, &bytes).unwrap();

        let archive = ZipArchive::open(&masked).unwrap();
        match archive.extract_to_memory("a.txt") {
            Err(ArchiveError::OperationBlocked { .. }) => {}
            other => {
                panic!("an unattributed collapse must refuse every by-name op, got {other:?}")
            }
        }
    }

    /// R0001-0027: a malformed or truncated central directory must fail
    /// closed. The scan used to `break` on any read error or unknown
    /// signature, which silently truncated the tally — and a tally that
    /// stops early *disables* duplicate detection for everything after it.
    #[test]
    fn test_zip_duplicate_scan_rejects_malformed_central_directory() {
        let tmp = tempfile::tempdir().unwrap();

        // (a) An unknown signature where the walk expects another header or
        //     a directory terminator.
        let bogus = tmp.path().join("bogus_signature.zip");
        build_stored_zip(&bogus, &["a.txt", "b.txt"]);
        let mut bytes = std::fs::read(&bogus).unwrap();
        let second = central_record_offsets(&bytes)[1];
        set_eocd_entry_count(&mut bytes, 1); // keep the crate's own parse valid
        bytes[second..second + 4].copy_from_slice(&[0x50, 0x4b, 0x77, 0x77]);
        std::fs::write(&bogus, &bytes).unwrap();

        let archive = ZipArchive::open(&bogus).unwrap();
        match archive.extract_to_memory("a.txt") {
            Err(ArchiveError::Corruption { .. }) => {}
            other => panic!("unknown record signature must fail closed, got {other:?}"),
        }

        // (b) A record whose declared name length runs past the end of file.
        let short = tmp.path().join("short_record.zip");
        build_stored_zip(&short, &["a.txt", "b.txt"]);
        let mut bytes = std::fs::read(&short).unwrap();
        let second = central_record_offsets(&bytes)[1];
        set_eocd_entry_count(&mut bytes, 1);
        bytes[second + 28..second + 30].copy_from_slice(&u16::MAX.to_le_bytes());
        std::fs::write(&short, &bytes).unwrap();

        let archive = ZipArchive::open(&short).unwrap();
        match archive.extract_to_memory("a.txt") {
            Err(ArchiveError::Corruption { .. }) => {}
            other => panic!("a record running past EOF must fail closed, got {other:?}"),
        }
    }

    /// R0001-0031 / R0001-0032: the central directory's uncompressed size is
    /// authoritative. An entry that decodes short must not pass integrity
    /// validation just because its stored CRC32 still matches the shortened
    /// data, and it must not escape the memory path as a valid buffer.
    #[test]
    fn test_zip_short_decode_is_corruption_not_success() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("short_decode.zip");
        build_stored_zip(&path, &["a.txt"]);

        // Inflate only the central directory's declared uncompressed size.
        // A stored entry copies `compressed_size` bytes through untouched,
        // so the payload and its CRC32 stay self-consistent — exactly the
        // case a checksum-only comparison cannot see.
        let mut bytes = std::fs::read(&path).unwrap();
        let record = central_record_offsets(&bytes)[0];
        let declared = le_u32(&bytes, record + 24);
        bytes[record + 24..record + 28].copy_from_slice(&(declared + 5).to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();

        let archive = ZipArchive::open(&path).unwrap();
        assert_eq!(
            archive.test_integrity().unwrap(),
            vec!["a.txt".to_string()],
            "an entry that decodes fewer bytes than declared is not intact"
        );
        match archive.extract_to_memory("a.txt") {
            Err(ArchiveError::Corruption { .. }) => {}
            other => panic!("a short decode must not be returned as data, got {other:?}"),
        }
    }

    /// R0001-0062: extraction must restore the timestamp the listing
    /// reports. Both now resolve the 0x5455 extended timestamp; extraction
    /// used to fall back to the coarse 2-second DOS value, so the listed and
    /// the restored mtime could disagree.
    #[test]
    fn test_zip_extraction_restores_extended_timestamp() {
        use std::io::Write as _;
        use std::time::{Duration, UNIX_EPOCH};

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ts.zip");

        // 0x5455 payload: flags byte (bit 0 = mtime present) followed by a
        // signed 32-bit Unix second count. An odd value is deliberate — the
        // DOS timestamp cannot represent it.
        let secs: i32 = 1_700_000_001;
        let mut extra = Vec::with_capacity(5);
        extra.push(0b0000_0001u8);
        extra.extend_from_slice(&secs.to_le_bytes());

        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let mut options = zip::write::FullFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        options
            .add_extra_data(0x5455, extra.into_boxed_slice(), false)
            .unwrap();
        writer.start_file("ts.txt", options).unwrap();
        writer.write_all(b"payload").unwrap();
        writer.finish().unwrap();

        let expected = UNIX_EPOCH + Duration::from_secs(secs as u64);
        let archive = ZipArchive::open(&path).unwrap();
        let entries = archive.list_files().unwrap();
        assert_eq!(
            entries[0].modified,
            Some(expected),
            "listing must surface the 0x5455 timestamp"
        );

        let all_dest = tmp.path().join("all");
        archive
            .extract_all_with_options(&all_dest, None, true, true, true, false, None)
            .unwrap();
        assert_eq!(
            std::fs::metadata(all_dest.join("ts.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            expected,
            "extract_all must restore the listed (0x5455) mtime"
        );

        let single_dest = tmp.path().join("single");
        archive
            .extract_file_with_options_preserve("ts.txt", &single_dest, true, true, true, false)
            .unwrap();
        assert_eq!(
            std::fs::metadata(single_dest.join("ts.txt"))
                .unwrap()
                .modified()
                .unwrap(),
            expected,
            "extract_file must restore the listed (0x5455) mtime"
        );
    }
}
