//! Native Rust 7z backend using the `sevenz-rust2` crate
//!
//! This backend provides native 7z operations with CRC32 support.
//! sevenz-rust2 is a maintained fork with Rust 2024 edition support.

use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::ffi::common::{
    StagedEntryWrite, check_extraction_cancelled, entry_cancel_hook, normalize_path,
    read_entry_to_memory_capped, unix_mode_is_symlink, write_entry_atomically,
};
use crate::format::ArchiveFormat;
use crate::fs_identity::{FileIdentity, identity_drift};
use crate::options::ProgressCallback;
use crate::payload_window::PayloadWindow;
use crate::security::{canonicalize_dest_base, sanitize_entry_path, sanitize_entry_path_with_base};
use once_cell::sync::OnceCell;
use sevenz_rust2::{ArchiveReader, Password};
use std::path::{Path, PathBuf};

/// Native Rust 7z archive wrapper
///
/// Provides 7z operations with CRC32 from metadata when available.
///
/// **Caching (AD 0065):** the parsed listing is memoised in `listing`
/// so the header/TOC parse is paid once per handle — the first
/// `list_files` call walks a fresh reader; later calls clone the
/// snapshot. Extraction paths still open their own reader because
/// sevenz-rust2's walk API consumes it.
///
/// **Identity binding (OI-0001-002):** because every later operation
/// re-opens the archive *by pathname*, the memoised listing can end up
/// describing a file that is no longer the one being read. `identity`
/// pins the archive file the handle is bound to — the internal
/// `open_reader` binds it on the first re-open and compares on every one
/// after it, so a same-name replacement is refused instead of read.
///
/// **In-place offset reads (DEF-001):** `path` need not name a bare
/// `.7z`. It may name a self-extracting executable whose real archive
/// starts at `payload_offset`, in which case every reader is built over
/// a `PayloadWindow` rather than over the raw `File` — see the
/// crate-internal `open_at_offset`.
pub struct SevenZArchive {
    path: PathBuf,
    password: Option<crate::Password>,
    /// Byte offset of the 7z signature within `path`. `0` for an
    /// ordinary `.7z`; non-zero only for an SFX payload opened where it
    /// lies. See [`Self::open_at_offset`].
    payload_offset: u64,
    listing: OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// The archive file this handle is bound to, captured lazily at the
    /// first `open_reader` — normally the listing walk, i.e. exactly
    /// when the AD 0065 snapshot is taken.
    ///
    /// `OnceCell` rather than a plain field because the constructor is
    /// file-untouching (AD 0052): a `stat` in `open()` would move the
    /// missing-file error from the first operation to `open()` and break
    /// the documented first-operation-validation contract.
    identity: OnceCell<FileIdentity>,
}

impl SevenZArchive {
    /// Open 7z archive for reading.
    ///
    /// **Validation timing (AD 0052):** first-operation validation — the
    /// crate-wide contract, not a per-backend quirk. Only the path is
    /// stored; the 7z header signature, table of contents, and any
    /// LZMA/LZMA2 codec requirements are not parsed until the caller
    /// invokes `list_files`, `extract_*`, or `test_integrity`, so corrupt
    /// or non-7z inputs surface their error from that first operation
    /// rather than from `open()`. The TOC parse is memoised in `listing`
    /// (AD 0065), so it is paid once per handle. Callers who want the
    /// check *now* call [`crate::Archive::validate`] (or
    /// [`crate::backend::ReadBackend::validate`]), which forces exactly
    /// this parse and leaves it cached — it is not `validate_integrity`,
    /// which decodes every payload.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::at_offset(path.as_ref().to_path_buf(), 0)
    }

    /// Open the 7z archive that starts at `offset` bytes into `path`,
    /// reading it where it lies (DEF-001).
    ///
    /// This is the in-place answer to a self-extracting 7z: instead of
    /// copying `[offset, EOF)` out to a tempfile and opening that (AD
    /// 0040, and the payload ceiling that copy forces), every reader is
    /// built over a [`PayloadWindow`] that makes byte `offset` look like
    /// byte 0. No temp space is consumed and no payload-sized copy is
    /// made before the first entry can be listed.
    ///
    /// **No offset agreement check is needed, or possible.**
    /// `sevenz_rust2::Archive::read` requires the six-byte
    /// `7z\xBC\xAF\x27\x1C` signature at *stream position 0* and
    /// searches for nothing, so the window's base is definitionally the
    /// archive start: a wrong `offset` fails the signature check at the
    /// first operation rather than opening something bogus at a
    /// plausible-looking position. Callers that want a cheap pre-flight
    /// (`Archive::try_open_in_place` does) probe the magic themselves
    /// before calling this.
    ///
    /// **Validation timing (AD 0052) is unchanged.** Like
    /// [`SevenZArchive::open`], this parses nothing — it only records
    /// the path and the offset. A payload that carries 7z magic but is
    /// corrupt fails at the *first operation*, exactly as an ordinary
    /// corrupt `.7z` does.
    ///
    /// **Modify mode cannot reach this.** `Archive::is_solid` has a
    /// Modify-mode 7z probe that reopens the source by pathname through
    /// `SevenZArchive::open` — i.e. at offset 0, which would answer for
    /// the SFX stub rather than the payload. It is unreachable for an
    /// offset handle: the probe is gated on `ArchiveMode::Modify`, and
    /// offset opens produce Read-mode handles only
    /// (`Archive::try_open_in_place` builds them via `new_read`).
    /// Stated rather than defended against, so that a future
    /// Modify-mode offset open trips this note instead of silently
    /// probing the wrong bytes.
    pub(crate) fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self> {
        Self::at_offset(path.as_ref().to_path_buf(), offset)
    }

    /// The one constructor, so a new field cannot be initialised two
    /// different ways in two places.
    fn at_offset(path: PathBuf, payload_offset: u64) -> Result<Self> {
        Ok(Self {
            path,
            password: None,
            payload_offset,
            listing: OnceCell::new(),
            identity: OnceCell::new(),
        })
    }

    /// Open encrypted 7z archive with password.
    ///
    /// Same first-operation validation as [`SevenZArchive::open`].
    /// Password correctness is verified at extraction time; a wrong
    /// password surfaces as `ArchiveError::Password` from the `extract_*`
    /// paths. [`crate::Archive::validate`] parses metadata only, so it
    /// does **not** report a bad password.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let mut archive = Self::open(path)?;
        archive.password = Some(crate::Password::new(password));
        Ok(archive)
    }

    /// Get archive path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The archive file this handle is bound to, or `None` if no
    /// operation has opened a reader yet.
    ///
    /// Exists so [`crate::Archive::payload_size_for_ratio`] can
    /// revalidate before the `stat` that produces the compression-ratio
    /// denominator: that stat is a by-path re-resolution the facade does
    /// on its own, outside every backend's bracket, while the numerator
    /// comes from the cached listing.
    pub(crate) fn bound_identity(&self) -> Option<FileIdentity> {
        self.identity.get().copied()
    }

    /// Bind — or re-check — the archive file this handle reads from.
    ///
    /// `found` is a *best-effort* capture: `None` means the `stat`
    /// failed. The first successful capture becomes the binding; every
    /// later one must match it. A capture that fails **after** a binding
    /// exists is a refusal, not a skip — a file that vanished under a
    /// live handle is exactly as disqualifying as one that was swapped,
    /// which is the same fail-closed rule
    /// [`FileIdentity::revalidate`] applies.
    /// Compare against an existing binding — never create one.
    ///
    /// The *pre*-open half of [`Self::open_reader`]'s bracket. It cannot
    /// bind, because at that point nothing has proved the file is an
    /// archive at all: AD 0052 makes the first operation the validator,
    /// and a first operation against a 7z a producer is still writing
    /// fails (the 7z directory lives at the end). Binding there recorded
    /// the half-written `len` and then refused the retry against the
    /// finished file as a swap — a permanent refusal on a healthy
    /// archive nobody touched. Binding happens in the post-open half
    /// ([`Self::bind_or_check_identity`]) instead, once the native open
    /// has succeeded.
    ///
    /// No swap coverage is lost: once a binding exists this compares on
    /// every call, before the native open reads a byte.
    fn check_identity_if_bound(&self, found: Option<FileIdentity>, op: &'static str) -> Result<()> {
        let Some(&expected) = self.identity.get() else {
            return Ok(());
        };
        match found {
            Some(found) if found == expected => Ok(()),
            found => Err(identity_drift(expected, found, &self.path, op)),
        }
    }

    fn bind_or_check_identity(&self, found: Option<FileIdentity>, op: &'static str) -> Result<()> {
        let expected = match found {
            // The first successful capture wins and becomes the binding.
            // `get_or_init` returns the *winner's* value, so a racing
            // loser is compared against it instead of silently
            // overwriting it.
            Some(found) => *self.identity.get_or_init(|| found),
            None => match self.identity.get() {
                // Never bound and still unable to stat: capture stays
                // best-effort, and the native open below remains the
                // authority on whether the file is usable at all.
                None => return Ok(()),
                Some(&expected) => expected,
            },
        };
        match found {
            Some(found) if found == expected => Ok(()),
            found => Err(identity_drift(expected, found, &self.path, op)),
        }
    }

    /// Open a 7z ArchiveReader with the stored path and password.
    ///
    /// Maps wrong-password / encryption-required failures to
    /// [`ArchiveError::Password`] (R0070-0049). The previous mapping
    /// collapsed every sevenz-rust2 error onto `ArchiveError::Format`,
    /// breaking the API contract that says wrong passwords surface as
    /// `Password { .. }`.
    ///
    /// # Identity binding (OI-0001-002)
    ///
    /// Every 7z operation routes through here, so this is the one place
    /// the by-pathname re-open happens — and therefore the one place the
    /// handle can be bound to the archive *file*. The native open is
    /// bracketed: [`Self::check_identity_if_bound`] before, so the reader
    /// is built from the file this handle is bound to, and
    /// [`Self::bind_or_check_identity`] after, so a swap *during* the
    /// open cannot leave a foreign reader in the caller's hands. The
    /// first *successful* open binds; on the facade path that is the
    /// listing walk, i.e. exactly when the AD 0065 snapshot is taken, so
    /// the snapshot and every later read provably describe one file.
    /// A failed open binds nothing, which is what keeps the AD 0052
    /// retry-a-still-being-written-archive shape working — see
    /// [`Self::check_identity_if_bound`].
    ///
    /// A drift here is [`ArchiveError::OperationBlocked`] carrying
    /// "identity changed" — deliberately a different variant *and* a
    /// disjoint vocabulary from the per-entry name/cardinality guards,
    /// which stay `ArchiveError::Format` with "listing drift" and are
    /// **not** superseded: they still catch a same-inode same-length
    /// in-place rewrite, any same-length replacement off Unix, the
    /// accepted stat-to-open window, and plain index-bookkeeping bugs.
    ///
    /// `op` is the caller's operation label, so the refusal names the
    /// public call the user actually made rather than a fixed
    /// open-family placeholder.
    ///
    /// Identity comes from a path `stat` rather than an `fstat` of the
    /// descriptor being read. Since DEF-001 this method *does* own the
    /// `File` — the in-place window has to be built over it — so an
    /// `fstat` binding is now mechanically possible and would close the
    /// residual stat-to-open window that DCR-007 / R0081 I6 accept.
    /// It is deliberately not done here: it would change what the
    /// binding *is* for every 7z handle, which is a change to the
    /// identity contract rather than to offset reads, and belongs to
    /// its own ticket alongside the same move for the other backends.
    /// The residual window is unchanged by this ticket, not widened.
    fn open_reader(&self, op: &'static str) -> Result<ArchiveReader<PayloadWindow<std::fs::File>>> {
        self.check_identity_if_bound(FileIdentity::capture(&self.path), op)?;
        let password = self
            .password
            .as_ref()
            .map(crate::Password::as_str)
            .map_or_else(Password::empty, Password::from);
        // DEF-001: the source is a window, not the raw file, so an SFX
        // payload at `payload_offset` is read where it lies. At offset 0
        // the window is the whole file and this is what
        // `ArchiveReader::open` did anyway — that call is just
        // `File::open` + `ArchiveReader::new` — modulo the one `fstat`
        // `PayloadWindow::from_file` needs to freeze the window length.
        //
        // Opening the `File` here also means a missing or unreadable
        // archive now surfaces as a typed `ArchiveError::Io` instead of
        // being funnelled through the sevenz error-*text* classifier
        // below as an "Invalid 7z" format error. That is a deliberate
        // improvement in precision, not an accident: a file that cannot
        // be opened is not a format failure.
        let file = std::fs::File::open(&self.path)
            .map_err(|e| ArchiveError::io("open", self.path.clone(), e))?;
        let window = PayloadWindow::from_file(file, self.payload_offset)
            .map_err(|e| ArchiveError::io("open payload window", self.path.clone(), e))?;
        let reader = ArchiveReader::new(window, password).map_err(|e| {
            let msg = e.to_string();
            // R0075-0027: phrase-based classifier so an archive whose
            // generic error text incidentally mentions "password" /
            // "encrypted" doesn't get reclassified as a password
            // failure. The phrases below are the ones sevenz-rust2
            // surfaces for genuine encryption failures
            // (wrong password, missing password, AES-specific guard).
            if is_sevenz_encryption_message(&msg) {
                ArchiveError::password(format!("7z: {}", msg))
            } else {
                ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    format!("Invalid 7z: {}", msg),
                )
            }
        })?;
        // Close the bracket: a replacement that landed between the stat
        // above and the open just now is caught here, before the caller
        // ever sees the reader.
        self.bind_or_check_identity(FileIdentity::capture(&self.path), op)?;
        Ok(reader)
    }

    /// Check if archive uses solid compression
    ///
    /// 7z solid compression compresses multiple files together in a single stream,
    /// providing better compression ratios but requiring sequential decompression.
    ///
    /// # Returns
    /// * `Ok(true)` - Archive uses solid compression
    /// * `Ok(false)` - Archive uses non-solid (independent file) compression
    /// * `Err(...)` - I/O error or invalid archive
    pub fn is_solid(&self) -> Result<bool> {
        // `ops` has no `is_solid` label and adding one is a change to
        // `crate::error`, not to this backend; `LIST_FILES` is the honest
        // stand-in — the probe parses exactly the TOC the listing walk
        // parses, and reports nothing else.
        let reader = self.open_reader(crate::error::ops::LIST_FILES)?;

        let archive = reader.archive();

        // sevenz-rust2 provides direct access to solid flag
        Ok(archive.is_solid)
    }

    /// Modification time from 7z metadata (NtTime → SystemTime).
    ///
    /// R0075-0048: NtTime is `u64` 100-ns intervals since 1601-01-01;
    /// hostile or corrupt archives can carry values that overflow when
    /// subtracted from the Unix-epoch baseline or multiplied to
    /// nanoseconds. Use checked arithmetic and drop the timestamp when
    /// it cannot be represented as a valid `SystemTime` rather than
    /// panicking in debug or wrapping in release. Shared by listing and
    /// the extraction-time metadata application (R0079-0019).
    fn entry_modified_time(entry: &sevenz_rust2::ArchiveEntry) -> Option<std::time::SystemTime> {
        if !entry.has_last_modified_date {
            return None;
        }
        let nt_time = u64::from(entry.last_modified_date);
        let unix_epoch_nt: u64 = 116_444_736_000_000_000; // NT time at Unix epoch
        // R0001-0059: NT time starts at 1601-01-01, so every timestamp
        // between 1601 and the Unix epoch is a *valid* instant that
        // `checked_sub(unix_epoch_nt)` turned into `None`, silently
        // dropping the archive's modification time. `SystemTime`
        // represents pre-epoch instants fine — subtract the gap FROM
        // `UNIX_EPOCH` instead. The R0075-0048 contract is unchanged:
        // every step stays checked, so a hostile value still yields
        // `None` rather than panicking in debug or wrapping in release.
        let (delta_nt, is_after_epoch) = match nt_time.checked_sub(unix_epoch_nt) {
            Some(delta_nt) => (delta_nt, true),
            // `checked_sub` only returns `None` when `nt_time <
            // unix_epoch_nt`, so this subtraction cannot underflow.
            None => (unix_epoch_nt - nt_time, false),
        };
        let delta = std::time::Duration::from_nanos(delta_nt.checked_mul(100)?);
        if is_after_epoch {
            std::time::UNIX_EPOCH.checked_add(delta)
        } else {
            std::time::UNIX_EPOCH.checked_sub(delta)
        }
    }

    /// Unix permission bits from the 7z attribute field's upper half
    /// (the p7zip convention used by `parse_entry` for listing).
    /// `None` when the archive carries no attributes or no Unix mode
    /// bits — Windows-created 7z archives are not given a synthetic
    /// mode (R0079-0019).
    fn entry_unix_mode(entry: &sevenz_rust2::ArchiveEntry) -> Option<u32> {
        if !entry.has_windows_attributes {
            return None;
        }
        let mode = (entry.windows_attributes >> 16) & 0o7777;
        (mode != 0).then_some(mode)
    }

    /// Link probe shared by listing and every extract/integrity path
    /// (R0075-0049 / R0075-0053): Unix S_IFLNK in the attribute word's
    /// upper half (p7zip convention), or the Windows reparse-point bit
    /// (FILE_ATTRIBUTE_REPARSE_POINT, 0x0400) in the lower half.
    fn entry_is_link(entry: &sevenz_rust2::ArchiveEntry) -> bool {
        entry.has_windows_attributes
            && (unix_mode_is_symlink(entry.windows_attributes >> 16)
                || (entry.windows_attributes & 0x0400) != 0)
    }

    /// Classify a 7z entry into an [`EntryType`] — the single source of
    /// truth for this backend, so listing, extraction, the progress
    /// denominator, and the integrity walk agree on every entry's kind.
    ///
    /// R0001-0022: the attribute word's upper half carries the Unix mode
    /// on p7zip-created archives (the same half [`Self::entry_unix_mode`]
    /// reads), so FIFO, socket, and character/block-device modes are
    /// reachable here. Decoding only `S_IFLNK` and falling back to the
    /// TOC directory flag classified all of them as regular files and
    /// decoded them under file semantics. Decode `S_IFMT` in full and map
    /// every unsupported kind to [`EntryType::Other`], which the
    /// extraction paths skip and `validate_single_entry` already refuses.
    /// Mirrors `classify_zip_entry_type` (R0081-0076 / R0081-0077),
    /// including the zero-nibble fallthrough: Windows-created archives
    /// carry no Unix mode, so their upper half is `0` and the TOC flag
    /// decides. Advances OI-0080-007.
    fn classify_entry_type(entry: &sevenz_rust2::ArchiveEntry) -> EntryType {
        const S_IFMT: u32 = 0o170000;
        const S_IFLNK: u32 = 0o120000;
        const S_IFDIR: u32 = 0o040000;
        const S_IFREG: u32 = 0o100000;

        // R0075-0049: classify links before directories so a
        // reparse-point entry whose `is_directory` flag is set does not
        // bypass the link policy. Matches the ordering used by the ZIP
        // backend.
        if Self::entry_is_link(entry) {
            return EntryType::Symlink;
        }

        if entry.has_windows_attributes {
            match (entry.windows_attributes >> 16) & S_IFMT {
                S_IFLNK => return EntryType::Symlink,
                S_IFDIR => return EntryType::Directory,
                S_IFREG => return EntryType::File,
                0 => {} // no type nibble recorded — fall through to the TOC flag
                _ => return EntryType::Other, // FIFO / char / block device / socket / ...
            }
        }

        if entry.is_directory {
            EntryType::Directory
        } else {
            EntryType::File
        }
    }

    /// CRC32 from 7z metadata, gated on the optional kCRC digest being
    /// present (R0079-0005) and fitting in `u32` (R0070-0050).
    ///
    /// `sevenz_rust2::ArchiveEntry::crc` defaults to 0 when the archive
    /// omits the digest, so the value is meaningless unless `has_crc`
    /// is set — reading it unconditionally surfaced `Some(0)` for
    /// CRC-less entries and made `verify_crc32` extraction fail
    /// spuriously on valid data. Returns `None` in both failure modes
    /// so callers elide verification rather than comparing against a
    /// bogus value.
    fn entry_crc32(entry: &sevenz_rust2::ArchiveEntry) -> Option<u32> {
        (entry.has_crc && entry.crc <= u32::MAX as u64).then_some(entry.crc as u32)
    }

    /// Compute the order in which `ArchiveReader::for_each_entries`
    /// visits `archive.files` indices (R0079-0004).
    ///
    /// sevenz-rust2 0.19 walks block by block — per block the contiguous
    /// run `files[block_first_file_index[b]..][..num_unpack_sub_streams]`
    /// — then makes a second pass over block-less files
    /// (`file_block_index == None`) in `files` order.
    /// 7-Zip writes no-stream entries (directories, empty files) ahead
    /// of stream entries in the TOC, so callback order differs from
    /// `list_files()` order for essentially every real-world archive.
    /// The per-block sub-stream count is crate-private upstream;
    /// `BlockDecoder::entry_count` is its public accessor and reads
    /// only archive metadata (the dummy source is never touched).
    ///
    /// The returned order is validated to be an exact permutation of
    /// `0..files_len` (R0081-0073 / R0081-0074): a corrupt stream map
    /// is rejected as [`ArchiveError::corruption`] rather than clamped
    /// into a truncated or non-permutation walk that would map callback
    /// positions onto the wrong listing entries.
    fn visit_order(&self, archive: &sevenz_rust2::Archive) -> Result<Vec<usize>> {
        let files_len = archive.files.len();
        let mut order = Vec::with_capacity(files_len);
        let password = Password::empty();
        let mut dummy = std::io::Cursor::new([0u8; 0]);
        for block_index in 0..archive.blocks.len() {
            let count =
                sevenz_rust2::BlockDecoder::new(1, block_index, archive, &password, &mut dummy)
                    .entry_count();
            // R0081-0073: a missing first-file index or a sub-stream run
            // that overruns `files` is corrupt stream metadata, not a
            // range to clamp — the previous `unwrap_or(files_len)` /
            // `.min(files_len)` silently truncated the block (or emptied
            // it), dropping entries from the walk. Reject it instead.
            let start = archive
                .stream_map
                .block_first_file_index
                .get(block_index)
                .copied()
                .ok_or_else(|| {
                    ArchiveError::corruption(
                        self.path.display().to_string(),
                        format!(
                            "7z stream map: block {} has no first-file index",
                            block_index
                        ),
                    )
                })?;
            let end = start
                .checked_add(count)
                .filter(|&end| end <= files_len)
                .ok_or_else(|| {
                    ArchiveError::corruption(
                        self.path.display().to_string(),
                        format!(
                            "7z stream map: block {} sub-stream run {}..{}+{} overruns {} files",
                            block_index, start, start, count, files_len
                        ),
                    )
                })?;
            order.extend(start..end);
        }
        for (file_index, block_idx) in archive.stream_map.file_block_index.iter().enumerate() {
            if block_idx.is_none() {
                order.push(file_index);
            }
        }
        // R0081-0074: the extract walk's length-only guard (R0080-0027)
        // cannot see overlapping block runs that duplicate some indices
        // while omitting others — the vector length can still match yet
        // callback positions map to the wrong listing entries. Require
        // `order` to be an exact permutation of `0..files_len`: every
        // file index present exactly once.
        let mut seen = vec![false; files_len];
        for &index in &order {
            let slot = seen.get_mut(index).ok_or_else(|| {
                ArchiveError::corruption(
                    self.path.display().to_string(),
                    format!(
                        "7z visit order: index {} out of range for {} files",
                        index, files_len
                    ),
                )
            })?;
            if *slot {
                return Err(ArchiveError::corruption(
                    self.path.display().to_string(),
                    format!(
                        "7z visit order: file index {} visited more than once",
                        index
                    ),
                ));
            }
            *slot = true;
        }
        if let Some(missing) = seen.iter().position(|visited| !visited) {
            return Err(ArchiveError::corruption(
                self.path.display().to_string(),
                format!("7z visit order: file index {} is never visited", missing),
            ));
        }
        Ok(order)
    }

    /// Per-file encryption flags derived from block coders, indexed by
    /// `archive.files` position. Shared by listing (encryption status)
    /// and the extraction paths (password-suspect error
    /// classification, R0079-0006).
    fn encrypted_file_flags(&self, archive: &sevenz_rust2::Archive) -> Result<Vec<bool>> {
        // R0001-0021: the returned vector is built by iterating
        // `file_block_index`, so it is sized by the stream map — but every
        // consumer indexes it by `archive.files` position and defaults a
        // missing slot with `.unwrap_or(false)`. A stream map shorter than
        // the file table therefore reported trailing files as
        // *unencrypted*, suppressing listing's encryption status and
        // disarming the password-suspect classification for exactly those
        // entries. Require the mapping to cover every file and fail closed
        // on a mismatch, matching the R0081-0075 block-index check below.
        let files_len = archive.files.len();
        let mapping_len = archive.stream_map.file_block_index.len();
        if mapping_len != files_len {
            return Err(ArchiveError::corruption(
                self.path.display().to_string(),
                format!(
                    "7z stream map: file-block index covers {} entries but the table of contents declares {} files",
                    mapping_len, files_len
                ),
            ));
        }

        let aes_id = sevenz_rust2::EncoderMethod::ID_AES256_SHA256;
        let encrypted_blocks: Vec<bool> = archive
            .blocks
            .iter()
            .map(|block| {
                block
                    .coders
                    .iter()
                    .any(|coder| coder.encoder_method_id() == aes_id)
            })
            .collect();
        // R0081-0075: a file that references a block index past the end
        // of `blocks` is corrupt metadata — the previous
        // `.unwrap_or(false)` reported it as unencrypted, masking the
        // corruption and potentially misclassifying a later password
        // failure. Surface it as corruption instead. The per-entry
        // lookup stays O(1), so the AD 0065 cached listing path is not
        // slowed.
        archive
            .stream_map
            .file_block_index
            .iter()
            .enumerate()
            .map(|(file_index, block_idx)| match block_idx {
                Some(idx) => encrypted_blocks.get(*idx).copied().ok_or_else(|| {
                    ArchiveError::corruption(
                        self.path.display().to_string(),
                        format!(
                            "7z stream map: file {} references block {} but only {} blocks exist",
                            file_index,
                            idx,
                            encrypted_blocks.len()
                        ),
                    )
                }),
                None => Ok(false),
            })
            .collect()
    }

    /// Map an error returned by `ArchiveReader::for_each_entries` onto
    /// the crate error contract (R0079-0006).
    ///
    /// The default `7z a -p` shape encrypts content but not the
    /// header, so `open()` succeeds with any password and the typed
    /// `PasswordRequired` / `MaybeBadPassword` failures only surface
    /// during block decode — they must classify as
    /// [`ArchiveError::Password`], not `Format`.
    ///
    /// R0001-0061: everything else used to flatten onto
    /// [`ArchiveError::format`], so a failure to *read the archive file*
    /// was indistinguishable from a damaged archive. Reuse the crate's
    /// existing corruption-vs-operational judgement
    /// ([`sevenz_read_error_is_corruption`]) on the wrapped source and
    /// keep an operational failure's `Io` shape — the same distinction
    /// `test_integrity` already draws inside the callback (R0080-0030).
    /// Advances OI-0080-007.
    fn map_walk_error(&self, error: sevenz_rust2::Error, context: &str) -> ArchiveError {
        if matches!(
            error,
            sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_)
        ) {
            return ArchiveError::password(format!("7z: {}", error));
        }
        let described = format!("{}: {}", context, error);
        match error {
            sevenz_rust2::Error::Io(source, _) | sevenz_rust2::Error::FileOpen(source, _)
                if !sevenz_read_error_is_corruption(&source) =>
            {
                // The archive path is the resource that failed: upstream's
                // `Io` / `FileOpen` wrappers only carry source-side reads
                // and opens of the archive itself.
                ArchiveError::io(
                    "read",
                    self.path.clone(),
                    std::io::Error::new(source.kind(), described),
                )
            }
            _ => ArchiveError::format(Some(ArchiveFormat::SevenZip), described),
        }
    }

    /// Reclassify a decode-side failure as a password error when it
    /// came from an encrypted block and a password was supplied
    /// (R0079-0006).
    ///
    /// A wrong password does not surface as a typed upstream error
    /// from inside the callbacks: AES decodes garbage, which fails as
    /// a decoder `Io` read error or as a CRC/size mismatch
    /// (`Corruption`). Upstream's `MaybeBadPassword` wrapping only
    /// applies to errors *returned* from the callback, which the
    /// stash-and-`Ok(false)` pattern never does. Write-side `Io`
    /// errors and all errors on unencrypted entries keep their
    /// original classification.
    ///
    /// **This mapping is lossy, and deliberately so (OI-0080-007 item
    /// 1).** A genuinely damaged payload decoded with the *correct*
    /// password fails identically and is reported as `Password` too, so
    /// the typed error cannot tell a wrong passphrase from damaged
    /// media. That is a property of 7z AES-256, which carries no
    /// authentication tag and no password-verification value — unlike
    /// RAR5's per-file check value or ZIP's AES/ZipCrypto verification
    /// bytes, both of which let their backends keep the two apart. There
    /// is nothing better to compute here: at this point the information
    /// does not exist.
    ///
    /// The ruling was to document the ambiguity rather than invent a
    /// typed shape for it (no `PasswordOrCorruption`, no `ambiguous`
    /// flag) — a typed marker would advertise a distinction the format
    /// cannot supply. The caller-facing statement, including what a
    /// caller can do instead, lives on [`ArchiveError::Password`];
    /// `ArchiveError` is `#[non_exhaustive]`, so adding a shape later
    /// stays non-breaking.
    fn classify_decode_error(&self, error: ArchiveError, entry_encrypted: bool) -> ArchiveError {
        if !entry_encrypted || self.password.is_none() {
            return error;
        }
        let password_suspect = match &error {
            ArchiveError::Io { operation, .. } => operation == "read",
            ArchiveError::Corruption { .. } => true,
            _ => false,
        };
        if password_suspect {
            ArchiveError::password(format!(
                "7z: decoding an encrypted entry failed with the supplied password (likely wrong password): {}",
                error
            ))
        } else {
            error
        }
    }

    /// List all files in 7z archive with CRC32 from metadata
    ///
    /// CRC32 is read from 7z headers when available.
    /// Encryption status is determined from block coders (not password presence).
    ///
    /// **Caching (AD 0065 / OI-0065-003).** The first call parses the
    /// TOC and stores the result; subsequent calls share the snapshot
    /// via `Arc::clone`.
    pub fn list_files(&self) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.list_files_budgeted(None)
    }

    /// Budgeted listing (OI-0080-003). `budget = Some(n)` aborts before
    /// building our `Vec<ArchiveEntry>` when the TOC declares more than `n`
    /// entries. Residual: sevenz-rust2 materialized `archive.files` when the
    /// reader was constructed, so the budget bounds only OUR allocation, not
    /// the library-internal TOC. The budget applies only to the first
    /// materialization; a cache hit ignores it, and an aborted parse does not
    /// populate `listing`.
    pub fn list_files_budgeted(
        &self,
        budget: Option<usize>,
    ) -> Result<std::sync::Arc<Vec<ArchiveEntry>>> {
        self.listing
            .get_or_try_init(|| self.walk_listing(budget).map(std::sync::Arc::new))
            .map(std::sync::Arc::clone)
    }

    fn walk_listing(&self, budget: Option<usize>) -> Result<Vec<ArchiveEntry>> {
        // First call on the facade path: this is where the handle binds
        // to the archive file, so the AD 0065 snapshot below and every
        // later re-open are provably reading one file (OI-0001-002).
        let reader = self.open_reader(crate::error::ops::LIST_FILES)?;
        let archive = reader.archive();

        // OI-0080-003: `archive.files` is the TOC sevenz-rust2 already parsed
        // on reader construction; reject before allocating our `Vec` past the
        // budget (see trait rustdoc for the residual).
        if let Some(budget) = budget {
            if archive.files.len() > budget {
                return Err(crate::security::too_many_entries_parsed(budget));
            }
        }

        let encrypted_files = self.encrypted_file_flags(archive)?;

        let mut entries = Vec::new();

        for (index, entry) in archive.files.iter().enumerate() {
            let is_encrypted = encrypted_files.get(index).copied().unwrap_or(false);

            let mut parsed_entry = self.parse_entry(entry, is_encrypted)?;

            // CRC32 from 7z metadata (R0079-0005): only file entries
            // that actually carry the optional kCRC digest get a
            // value — directories and CRC-less entries report `None`
            // per the `ArchiveEntry` contract, matching the ZIP/RAR
            // backends.
            if parsed_entry.entry_type == EntryType::File {
                parsed_entry.crc32 = Self::entry_crc32(entry);
            }

            parsed_entry.id = index;
            entries.push(parsed_entry);
        }

        Ok(entries)
    }

    /// Parse 7z entry into ArchiveEntry
    fn parse_entry(
        &self,
        entry: &sevenz_rust2::ArchiveEntry,
        is_encrypted: bool,
    ) -> Result<ArchiveEntry> {
        // Normalize path separators to forward slashes
        let path = normalize_path(&entry.name);

        // R0001-0022: classify through the shared 7z decoder. It keeps
        // the symlink-before-directory ordering (R0075-0049) and maps
        // special Unix modes (FIFO/char/block/socket) to
        // `EntryType::Other` instead of silently reporting them as
        // regular files.
        let entry_type = Self::classify_entry_type(entry);
        let is_dir = entry_type == EntryType::Directory;

        let size = if is_dir { None } else { Some(entry.size) };

        // R0072-0010: directory entries carry no payload, so the
        // `ArchiveEntry` contract requires `compressed_size = None` for
        // them. Previously 7z surfaced `Some(entry.compressed_size)`
        // unconditionally, which broke cross-format consumers comparing
        // directory invariants.
        let compressed_size = if is_dir {
            None
        } else {
            Some(entry.compressed_size)
        };

        let modified = Self::entry_modified_time(entry);

        let mut entry_parsed = ArchiveEntry::file(path, 0).build();
        entry_parsed.entry_type = entry_type;
        entry_parsed.size = size;
        entry_parsed.compressed_size = compressed_size;
        entry_parsed.modified = modified;
        entry_parsed.is_encrypted = is_encrypted;
        // R0075-0050: surface 7z attributes on the entry when the
        // archive carries them. The lower 16 bits store Windows
        // file attributes; the upper 16 bits store Unix mode (when
        // produced by p7zip on Unix). Map the Windows half into
        // `attributes.windows`; the Unix permissions half is masked
        // into `permissions` if non-zero. Other 7z fields (e.g.
        // creation time / archive-specific flags) are not exposed
        // by sevenz-rust2's `ArchiveEntry` yet — surface what we
        // have and leave the rest as `None`.
        if entry.has_windows_attributes {
            let win_attrs = entry.windows_attributes;
            entry_parsed.permissions = Self::entry_unix_mode(entry);
            // Lower 16 bits are the canonical Windows file-attribute
            // bitmap.
            entry_parsed.attributes = Some(crate::entry::FileAttributes {
                windows: Some(win_attrs & 0xFFFF),
                unix_xattr: None,
                archive_specific: None,
            });
        }

        Ok(entry_parsed)
    }

    /// Extract all files to destination directory
    pub fn extract_all(
        &self,
        dest_path: &Path,
        progress: Option<&mut Box<dyn ProgressCallback>>,
    ) -> Result<Vec<ArchiveWarning>> {
        self.extract_all_with_options(dest_path, progress, true, true, true, false, None)
    }

    /// Extract all files with options. When `selection` is `Some`, only entries
    /// whose positional index (matching `list_files()` order) is in the set are
    /// materialized; the archive is traversed once per AD 0029.
    ///
    /// `preserve_permissions` / `preserve_times` (R0079-0019) apply the
    /// TOC's Unix mode bits (p7zip attribute convention) / NT-time
    /// modification timestamp to each staged file before the atomic
    /// install.
    #[allow(clippy::too_many_arguments)]
    pub fn extract_all_with_options(
        &self,
        dest_path: &Path,
        mut progress: Option<&mut Box<dyn ProgressCallback>>,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        verify_crc32: bool,
        selection: Option<&std::collections::HashSet<usize>>,
    ) -> Result<Vec<ArchiveWarning>> {
        let mut warnings: Vec<ArchiveWarning> = Vec::new();
        let mut reader = self.open_reader(crate::error::ops::EXTRACT)?;

        // R0079-0004: for_each_entries visits entries block-major (stream
        // files per block, then no-stream files), not in TOC order.
        // Precompute the visit order so `selection` indices always mean
        // `list_files()` order.
        let visit_order = self.visit_order(reader.archive())?;
        let encrypted_files = self.encrypted_file_flags(reader.archive())?;

        // R0080-0027: pin the validated listing so the extraction walk can
        // be cross-checked against it per visited position (extends the
        // R0080-0009 listing-drift philosophy to 7z). Cached after the
        // first call (AD 0065), so this is a metadata-only parse.
        let listing = self.list_files()?;

        // Calculate total size for progress (selected entries only when
        // filtering). R0080-0064: exclude links as well as directories so
        // the denominator matches what the extraction loop actually writes
        // — the loop skips both, and `test_integrity` in this file uses the
        // same predicate. Counting link sizes here left progress unable to
        // reach 100% naturally. R0001-0022: the shared classifier now also
        // excludes `EntryType::Other` (FIFO/device/socket), which the loop
        // skips too.
        let total_bytes: u64 = if progress.is_some() {
            reader
                .archive()
                .files
                .iter()
                .enumerate()
                .filter(|(_, e)| Self::classify_entry_type(e) == EntryType::File)
                .filter(|(idx, _)| match selection {
                    Some(sel) => sel.contains(idx),
                    None => true,
                })
                .map(|(_, e)| e.size)
                .fold(0u64, u64::saturating_add)
        } else {
            0
        };

        let mut bytes_processed = 0u64;
        let mut extraction_error: Option<ArchiveError> = None;
        let mut callback_pos: usize = 0;
        // R0001-0060: directory metadata is applied in a post-walk pass —
        // `(output path, unix mode, mtime)` per extracted directory entry.
        let mut pending_dir_metadata: Vec<(PathBuf, Option<u32>, Option<std::time::SystemTime>)> =
            Vec::new();
        // Canonicalize the destination once — every entry's sanitize
        // pass compares against the same stable base.
        let canonical_dest = canonicalize_dest_base(dest_path)?;

        // Extract using for_each_entries
        let result = reader.for_each_entries(|entry, entry_reader| {
            // R0079-0024: upstream discards the per-block stop flag, so
            // `Ok(false)` only terminates the current block — later
            // blocks and the no-stream pass keep invoking the callback.
            // Stop doing work once a fatal error is recorded.
            if extraction_error.is_some() {
                return Ok(false);
            }

            // Check cancellation
            if let Err(e) = check_extraction_cancelled(
                &mut progress,
                bytes_processed,
                total_bytes,
                crate::error::ops::EXTRACT_ALL,
            ) {
                extraction_error = Some(e);
                return Ok(false);
            }

            // R0079-0004: translate the callback position back to the
            // `files` index so selection matches `list_files()` order.
            let Some(current_idx) = visit_order.get(callback_pos).copied() else {
                extraction_error = Some(ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    "Extract: archive walk visited more entries than the table of contents declares",
                ));
                return Ok(false);
            };
            callback_pos += 1;

            // R0080-0027: cross-check the walked entry's name against the
            // cached listing at this position. `entry.name` is available in
            // the callback, so a walk that yields a different entry than the
            // validated listing at `current_idx` — a stale AD 0065 cache or
            // a rewritten archive — is caught before any bytes are written.
            let normalized_path = normalize_path(&entry.name);
            match listing.get(current_idx) {
                Some(listed) if listed.path == normalized_path => {}
                Some(listed) => {
                    extraction_error = Some(ArchiveError::corruption(
                        normalized_path.clone(),
                        format!(
                            "Extract: listing drift at index {} — expected '{}', walked '{}'",
                            current_idx, listed.path, normalized_path
                        ),
                    ));
                    return Ok(false);
                }
                None => {
                    extraction_error = Some(ArchiveError::corruption(
                        normalized_path.clone(),
                        format!(
                            "Extract: walk visited index {} beyond the {} listed entries",
                            current_idx,
                            listing.len()
                        ),
                    ));
                    return Ok(false);
                }
            }

            if let Some(sel) = selection {
                if !sel.contains(&current_idx) {
                    return Ok(true);
                }
            }

            // R0075-0053 / R0075-0052: classify and skip link entries
            // *before* sanitising the destination path so a symlink
            // whose archive name carries a traversal pattern surfaces
            // as a `SkippedSymlink` warning rather than as an
            // `InvalidPath` error. Warning paths use the same
            // normalised string that listing/filtering uses so UI
            // consumers can correlate by string match.
            let entry_type = Self::classify_entry_type(entry);
            if entry_type == EntryType::Symlink {
                warnings.push(ArchiveWarning::SkippedSymlink {
                    path: normalized_path.clone(),
                    target: None,
                });
                return Ok(true); // skip, continue to next entry
            }
            // R0001-0022: special Unix modes (FIFO/char/block/socket) are
            // neither regular files nor directories; skip them rather than
            // materializing a plain file, exactly as the ZIP backend does
            // for `EntryType::Other` (R0081-0077). No `ArchiveWarning`
            // variant describes a skipped special entry yet, so the skip is
            // silent — the listing already reports the entry as `Other`.
            if entry_type == EntryType::Other {
                return Ok(true); // skip, continue to next entry
            }

            // Sanitize entry path to prevent path traversal (link
            // entries already handled above).
            let entry_path =
                match sanitize_entry_path_with_base(&normalized_path, dest_path, &canonical_dest) {
                    Ok(path) => path,
                    Err(e) => {
                        extraction_error = Some(e);
                        return Ok(false);
                    }
                };

            // Create parent directories
            if let Some(parent) = entry_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    extraction_error =
                        Some(ArchiveError::io("create_dir", parent.to_path_buf(), e));
                    return Ok(false);
                }
            }

            if entry_type == EntryType::Directory {
                // Create directory
                if let Err(e) = std::fs::create_dir_all(&entry_path) {
                    extraction_error = Some(ArchiveError::io("create_dir", entry_path.clone(), e));
                    return Ok(false);
                }
                // R0001-0060: `preserve_permissions` / `preserve_times`
                // reached only the file writer, so extracted directories
                // kept the umask mode and the creation-time mtime even when
                // preservation was requested. Record the metadata and apply
                // it after the walk (deepest-first): installing a child
                // re-stamps its parent's mtime, and a restrictive archived
                // mode applied up front could block that install outright.
                let dir_mode = preserve_permissions
                    .then(|| Self::entry_unix_mode(entry))
                    .flatten();
                let dir_modified = preserve_times
                    .then(|| Self::entry_modified_time(entry))
                    .flatten();
                if dir_mode.is_some() || dir_modified.is_some() {
                    pending_dir_metadata.push((entry_path.clone(), dir_mode, dir_modified));
                }
            } else {
                // Extract file through the shared staged-write pipeline
                // (Exact budget, optional CRC, metadata preservation,
                // atomic commit). Errors that came off the decoder get
                // the password-suspect reclassification (R0079-0006);
                // stage/metadata/commit errors pass through it unchanged
                // (the classifier only rewrites read-Io and Corruption).
                let mut cancel_check =
                    entry_cancel_hook(&mut progress, bytes_processed, total_bytes);
                match write_entry_atomically(
                    entry_reader,
                    StagedEntryWrite {
                        output_path: &entry_path,
                        entry_path: &normalized_path,
                        op: crate::error::ops::EXTRACT_ALL,
                        overwrite,
                        expected_crc: if verify_crc32 {
                            Self::entry_crc32(entry)
                        } else {
                            None
                        },
                        declared_size: entry.size,
                        unix_mode: preserve_permissions
                            .then(|| Self::entry_unix_mode(entry))
                            .flatten(),
                        modified: preserve_times
                            .then(|| Self::entry_modified_time(entry))
                            .flatten(),
                    },
                    Some(&mut cancel_check),
                ) {
                    Ok(bytes_written) => bytes_processed += bytes_written,
                    Err(e) => {
                        let entry_encrypted =
                            encrypted_files.get(current_idx).copied().unwrap_or(false);
                        extraction_error = Some(self.classify_decode_error(e, entry_encrypted));
                        return Ok(false);
                    }
                }
            }

            Ok(true) // Continue extraction
        });

        // R0001-0023: the stashed callback error is the *real* cause — a
        // cancellation, a path refusal, a cap breach, a listing-drift
        // corruption. Returning `Ok(false)` to abort the walk can itself
        // make upstream surface an error, so mapping `result` first
        // overwrote the typed cause with a generic `Format`. Prefer the
        // stash; only map the upstream result when nothing was recorded.
        // Advances OI-0080-007.
        if let Some(err) = extraction_error {
            return Err(err);
        }

        result.map_err(|e| self.map_walk_error(e, "Extract"))?;

        // R0080-0027: guard against a truncated upstream walk. Out-of-range
        // callback positions already error above, but a walk that invokes
        // the callback for *fewer* entries than the TOC declares would
        // otherwise leave selected entries silently unextracted and still
        // report a forced 100% progress. Require every declared position to
        // have been visited.
        if callback_pos != visit_order.len() {
            return Err(ArchiveError::corruption(
                self.path.display().to_string(),
                format!(
                    "Extract: archive walk visited {} of {} declared entries",
                    callback_pos,
                    visit_order.len()
                ),
            ));
        }

        // R0001-0060: apply the deferred directory metadata now that every
        // payload is installed. Deepest-first (descending component count)
        // so stamping a parent cannot be undone by a child install, and so
        // a read-only archived mode on a parent is applied only after its
        // children are already in place.
        pending_dir_metadata
            .sort_by_key(|(path, _, _)| std::cmp::Reverse(path.components().count()));
        for (path, unix_mode, modified) in &pending_dir_metadata {
            apply_directory_metadata(path, *unix_mode, *modified)?;
        }

        // R0070-0037: honor cancellation in the final callback the
        // same way the per-entry path does.
        check_extraction_cancelled(
            &mut progress,
            total_bytes,
            total_bytes,
            crate::error::ops::EXTRACT_ALL,
        )?;

        Ok(warnings)
    }

    /// Extract a single file by path
    pub fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        self.extract_file_with_options(file_path, dest_path, true, false)
    }

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
    /// TOC's Unix mode bits / NT-time modification timestamp to the
    /// staged file before the atomic install.
    pub fn extract_file_with_options_preserve(
        &self,
        file_path: &str,
        dest_path: &Path,
        overwrite: bool,
        preserve_permissions: bool,
        preserve_times: bool,
        verify_crc32: bool,
    ) -> Result<()> {
        // OI-0076-002: resolve through the shared single-entry gate
        // before opening the extraction reader — existence, uniqueness,
        // and link/directory policy live in validate_single_entry
        // (directory entries now error at the gate instead of
        // materializing, R0076-0060).
        let listing = self.list_files()?;
        let validated = crate::security::validate_single_entry(
            &listing,
            file_path,
            crate::error::ops::EXTRACT_FILE,
        )?;
        let target_id = validated.id();
        let validated_path = validated.path().to_string();

        let mut reader = self.open_reader(crate::error::ops::EXTRACT_FILE)?;

        // R0079-0006: per-entry encryption flags for password-suspect
        // error classification; callback positions translate through
        // the R0079-0004 visit order.
        let visit_order = self.visit_order(reader.archive())?;
        let encrypted_files = self.encrypted_file_flags(reader.archive())?;
        let entry_encrypted = encrypted_files.get(target_id).copied().unwrap_or(false);

        let mut found = false;
        // Sanitize the validated listing path (R0076-0050) to prevent
        // path traversal.
        let output_path = sanitize_entry_path(&validated_path, dest_path)?;
        let mut extraction_error: Option<ArchiveError> = None;
        let mut callback_pos: usize = 0;

        let result = reader.for_each_entries(|entry, entry_reader| {
            // R0079-0024: upstream discards the per-block stop flag, so
            // `Ok(false)` only terminates the current block. Stop
            // decoding later blocks once the target has been handled or
            // a fatal error is recorded.
            if found || extraction_error.is_some() {
                return Ok(false);
            }
            // R0076-0059: seek by stable listing id, never by name scan;
            // the callback position translates through the R0079-0004
            // visit order.
            let Some(current_idx) = visit_order.get(callback_pos).copied() else {
                extraction_error = Some(ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    "Extract: archive walk visited more entries than the table of contents declares",
                ));
                return Ok(false);
            };
            callback_pos += 1;

            if current_idx != target_id {
                return Ok(true); // Continue
            }
            found = true;

            // OI-0076-002 drift guard: the AD 0065 listing snapshot can
            // go stale if the archive is rewritten on disk between
            // listing and extraction — never extract a mismatched entry.
            let walked_path = normalize_path(&entry.name);
            if walked_path != validated_path {
                extraction_error = Some(ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    format!(
                        "single-entry listing drift at index {}: expected '{}', found '{}'",
                        current_idx, validated_path, walked_path
                    ),
                ));
                return Ok(false);
            }

            // Create parent directories
            if let Some(parent) = output_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    extraction_error = Some(ArchiveError::io("create_dir", parent.to_path_buf(), e));
                    return Ok(false);
                }
            }

            // Extract file through the shared staged-write pipeline
            // (Exact budget, optional CRC, metadata preservation,
            // atomic commit). Decoder errors get the
            // password-suspect reclassification (R0079-0006);
            // stage/metadata/commit errors pass through it unchanged
            // (the classifier only rewrites read-Io and Corruption).
            if let Err(e) = write_entry_atomically(
                entry_reader,
                StagedEntryWrite {
                    output_path: &output_path,
                    entry_path: &validated_path,
                    op: crate::error::ops::EXTRACT_FILE,
                    overwrite,
                    expected_crc: if verify_crc32 {
                        Self::entry_crc32(entry)
                    } else {
                        None
                    },
                    declared_size: entry.size,
                    unix_mode: preserve_permissions
                        .then(|| Self::entry_unix_mode(entry))
                        .flatten(),
                    modified: preserve_times
                        .then(|| Self::entry_modified_time(entry))
                        .flatten(),
                },
                None,
            ) {
                extraction_error = Some(self.classify_decode_error(e, entry_encrypted));
                return Ok(false);
            }

            Ok(false) // Stop extraction
        });

        // R0001-0023: prefer the typed error the callback stashed over the
        // upstream walk error the abort may have produced (see the
        // `extract_all_with_options` note).
        if let Some(err) = extraction_error {
            return Err(err);
        }

        result.map_err(|e| self.map_walk_error(e, "Extract"))?;

        if !found {
            return Err(ArchiveError::format(
                Some(ArchiveFormat::SevenZip),
                format!("File '{}' not found in archive", file_path),
            ));
        }

        Ok(())
    }

    /// Extract a single file to memory
    pub fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        self.extract_to_memory_capped(file_path, None, crate::error::ops::EXTRACT_TO_MEMORY)
    }

    /// Cap-aware extract-to-memory: `max_bytes` is honoured *before*
    /// buffering — an entry whose TOC-declared size exceeds the cap is
    /// rejected up front instead of being fully decoded and post-checked
    /// (R0076-0062). Backend `extract_to_memory_with_limit` routes here.
    ///
    /// R5 (ti-581bcda4): `op` labels every error this path can raise, so a
    /// refusal raised on behalf of `Archive::extract_to_stream` is not
    /// mislabelled `extract_to_memory` (R0071-0010).
    pub(crate) fn extract_to_memory_capped(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
        op: &'static str,
    ) -> Result<Vec<u8>> {
        // OI-0076-002: resolve through the shared single-entry gate
        // before opening the extraction reader — existence, uniqueness,
        // and link/directory policy live in validate_single_entry
        // (R0076-0060).
        let listing = self.list_files()?;
        // R5 (ti-581bcda4): `op`, not a hard-coded `extract_to_memory`.
        let validated = crate::security::validate_single_entry(&listing, file_path, op)?;
        let target_id = validated.id();
        let validated_path = validated.path().to_string();

        self.extract_to_memory_by_listing_id(target_id, &validated_path, max_bytes, op)
    }

    /// Id-addressed core of [`Self::extract_to_memory_capped`]: everything
    /// after the by-path single-entry gate.
    ///
    /// `target_id` is a stable listing id — an index into `archive.files`,
    /// which [`Self::list_files_budgeted`] assigns positionally — and
    /// `validated_path` is the normalized listing name the caller already
    /// resolved for it. The walk still translates callback positions
    /// through the R0079-0004 visit order and still applies the
    /// OI-0076-002 drift guard; what it deliberately does *not* do is
    /// re-run `validate_single_entry`, whose uniqueness half is precisely
    /// what an id-addressed seek exists to bypass.
    fn extract_to_memory_by_listing_id(
        &self,
        target_id: usize,
        validated_path: &str,
        max_bytes: Option<u64>,
        op: &'static str,
    ) -> Result<Vec<u8>> {
        // `op` already distinguishes the memory path from the by-id
        // stream path this method also serves, so an identity refusal
        // names the public call the caller actually made (R5 /
        // ti-581bcda4, same reason the gate errors are labelled).
        let mut reader = self.open_reader(op)?;

        // R0079-0006: per-entry encryption flags for password-suspect
        // error classification; callback positions translate through
        // the R0079-0004 visit order.
        let visit_order = self.visit_order(reader.archive())?;
        let encrypted_files = self.encrypted_file_flags(reader.archive())?;
        let entry_encrypted = encrypted_files.get(target_id).copied().unwrap_or(false);

        let mut result: Option<Vec<u8>> = None;
        let mut extraction_error: Option<ArchiveError> = None;
        let mut callback_pos: usize = 0;

        let extract_result = reader.for_each_entries(|entry, entry_reader| {
            // R0079-0024: upstream discards the per-block stop flag, so
            // `Ok(false)` only terminates the current block. Stop
            // decoding later blocks once the target has been handled or
            // a fatal error is recorded.
            if result.is_some() || extraction_error.is_some() {
                return Ok(false);
            }
            // R0076-0059: seek by stable listing id, never by name scan;
            // the callback position translates through the R0079-0004
            // visit order.
            let Some(current_idx) = visit_order.get(callback_pos).copied() else {
                extraction_error = Some(ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    "Extract: archive walk visited more entries than the table of contents declares",
                ));
                return Ok(false);
            };
            callback_pos += 1;

            if current_idx != target_id {
                return Ok(true); // Continue
            }

            // OI-0076-002 drift guard: the AD 0065 listing snapshot can
            // go stale if the archive is rewritten on disk between
            // listing and extraction — never extract a mismatched entry.
            let walked_path = normalize_path(&entry.name);
            if walked_path != validated_path {
                extraction_error = Some(ArchiveError::format(
                    Some(ArchiveFormat::SevenZip),
                    format!(
                        "single-entry listing drift at index {}: expected '{}', found '{}'",
                        current_idx, validated_path, walked_path
                    ),
                ));
                return Ok(false);
            }

            // Shared bounds contract (R0070-0015): over-produce is
            // corruption; `require_exact` because the 7z TOC size is
            // authoritative, so a short read surfaces as corruption
            // rather than silent success (R0075-0051). The caller
            // cap is enforced before buffering (R0076-0062).
            match read_entry_to_memory_capped(
                entry_reader,
                entry.size,
                max_bytes,
                validated_path,
                // R5 (ti-581bcda4): carry the caller's operation label.
                op,
                None,
                true,
            ) {
                Ok(buffer) => {
                    result = Some(buffer);
                    Ok(false) // Stop extraction
                }
                Err(e) => {
                    extraction_error = Some(self.classify_decode_error(e, entry_encrypted));
                    Ok(false)
                }
            }
        });

        // R0001-0023: prefer the typed error the callback stashed over the
        // upstream walk error the abort may have produced (see the
        // `extract_all_with_options` note).
        if let Some(err) = extraction_error {
            return Err(err);
        }

        extract_result.map_err(|e| self.map_walk_error(e, "Extract"))?;

        result.ok_or_else(|| {
            ArchiveError::format(
                Some(ArchiveFormat::SevenZip),
                format!("File '{}' not found in archive", validated_path),
            )
        })
    }

    /// Stream one entry addressed by its stable listing id rather than by
    /// path (ti-2a6e3153 for tar; DCR-012 for AE-2 ZIP; this method closes
    /// the same hole for 7z).
    ///
    /// **Why 7z needs this at all.** A 7z file entry lists `crc32: None`
    /// whenever its record carries no optional kCRC digest — see
    /// [`Self::entry_crc32`], which gates on `entry.has_crc`. 7-Zip itself
    /// writes no-stream entries (empty files) that way, and nothing in the
    /// format obliges a writer to store the digest for a streamed member
    /// either. Such an entry routes through the content-multiset digest
    /// walk's streaming arm. Before this method existed the walk hit the
    /// `ReadBackend::extract_to_stream_by_listing_id` trait default, fell
    /// back to the by-*path* stream, and a 7z holding the same path twice
    /// with a CRC-less occurrence stopped digesting at
    /// `security::validate_single_entry` with `OperationBlocked` — the
    /// OI-0076-002 defect, reached through 7z instead of ZIP.
    ///
    /// **What still guards this path.** The id-addressed walk keeps the
    /// OI-0076-002 drift guard (a header whose normalized name disagrees
    /// with `validated_path` is refused, so a stale AD 0065 listing cannot
    /// redirect the read) and the R0070-0015 exact-size bound. Only the
    /// uniqueness half of the single-entry gate is skipped.
    ///
    /// Same buffered-adapter caveat as [`Self::extract_to_stream`]:
    /// sevenz-rust2 exposes no owned entry-level `Read` (AD 0035 /
    /// DEF-004), so the entry is materialized in full before the cursor is
    /// handed back.
    ///
    /// The forward in `crate::backend` is what makes any of this reachable;
    /// calling this inherent method directly cannot detect a missing one.
    /// Prove it from the facade.
    pub(crate) fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        let data = self.extract_to_memory_by_listing_id(
            id,
            validated_path,
            None,
            crate::error::ops::EXTRACT_TO_STREAM,
        )?;
        Ok(crate::streaming::StreamingExtractor::from_bytes(data))
    }

    /// Extract a single file to a stream
    ///
    /// Note: Currently loads the entire file into memory before wrapping in a cursor.
    /// The sevenz-rust2 API uses a callback-based extraction model that doesn't
    /// support streaming reads — see AD 0035 for the investigation and deferral
    /// of true entry-level streaming.
    pub fn extract_to_stream(
        &self,
        file_path: &str,
    ) -> Result<crate::streaming::StreamingExtractor> {
        self.extract_to_stream_with_limit(file_path, None)
    }

    /// Streaming variant of [`Self::extract_to_memory_capped`].
    ///
    /// R0001-0011: the 7z stream path used to inherit the
    /// `ReadBackend::extract_to_stream_with_limit` trait default, which
    /// materializes the whole entry first and caps only the returned
    /// reader. Routing through the capped memory path rejects an entry
    /// whose TOC-declared size exceeds `max_bytes` before buffering
    /// (R0076-0062), so a caller-chosen budget binds pre-materialization.
    ///
    /// Buffered adapter either way: sevenz-rust2 0.19.4 exposes no owned
    /// entry-level `Read` (all public APIs are either callback-scoped
    /// `&mut dyn Read` or return `Vec<u8>`), deferred per AD 0035 until
    /// upstream exposes an owned reader. For a solid block the decoder
    /// still works through preceding members to reach the target — the cap
    /// bounds our buffer, not that decode work.
    pub(crate) fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<crate::streaming::StreamingExtractor> {
        // R5 (ti-581bcda4): label the refusal with the public operation the
        // caller actually invoked.
        let data = self.extract_to_memory_capped(
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_STREAM,
        )?;
        Ok(crate::streaming::StreamingExtractor::from_bytes(data))
    }

    /// Test archive integrity by verifying CRC32 for all files
    ///
    /// Streams each entry through an 8KB buffer; sevenz-rust2 verifies
    /// CRC32 during decompression. Per-entry payload corruption — a CRC
    /// mismatch or a decode-class read error — is recorded as a failed
    /// path; a genuine archive-file I/O error (open/read on the archive
    /// itself) propagates as `Err` (R0080-0030). Returns a list of paths
    /// that failed.
    pub fn test_integrity(&self) -> Result<Vec<String>> {
        let mut reader = self.open_reader(crate::error::ops::VALIDATE_INTEGRITY)?;

        // R0001-0025: the validated visit-order mapping (R0079-0004 /
        // R0081-0073 / R0081-0074) is the declared-entry census. Building
        // it here also rejects a corrupt stream map before any decode.
        let visit_order = self.visit_order(reader.archive())?;

        let mut failed_files = Vec::new();
        // R0080-0030: a genuine archive-file I/O error is operational, not
        // an integrity failure. The `for_each_entries` callback can only
        // return `sevenz_rust2::Error`, so stash the typed error here and
        // surface it after the walk to keep its true `Io` shape instead of
        // flattening it to a `Format` error.
        let mut operational_error: Option<ArchiveError> = None;
        let mut callback_pos: usize = 0;

        // Single-pass: sevenz-rust2 verifies CRC32 during decompression.
        // R0070-0051: skip non-regular entries (directories, symlinks,
        // reparse points). Extraction policy already drops these, so
        // including them in the integrity walk yielded inconsistent
        // failure counts vs the file-only `validated` accounting.
        // R0001-0022: the shared classifier also skips `EntryType::Other`
        // (FIFO/device/socket), which extraction skips too.
        let walk = reader.for_each_entries(|entry, entry_reader| {
            if operational_error.is_some() {
                return Ok(false);
            }
            // R0001-0025: count every callback so the post-walk
            // completeness check can prove the whole file table was
            // visited, and reject a walk that overruns the declared
            // census — the same guard the extraction paths apply.
            if callback_pos >= visit_order.len() {
                operational_error = Some(ArchiveError::corruption(
                    self.path.display().to_string(),
                    "test_integrity: archive walk visited more entries than the table of contents declares",
                ));
                return Ok(false);
            }
            callback_pos += 1;

            if Self::classify_entry_type(entry) != EntryType::File {
                return Ok(true);
            }

            let path = normalize_path(&entry.name);
            let mut buf = [0u8; 8192];
            // R0001-0024: count the decoded bytes. The drain loop only
            // checked for decoder errors, so an entry that ends early and
            // carries no kCRC digest (`has_crc == false` — upstream then
            // interposes no `Crc32VerifyingReader`) drained cleanly and was
            // reported as valid. The 7z TOC size is authoritative (the same
            // premise `extract_to_memory` encodes as `require_exact`,
            // R0075-0051), so require exact equality.
            let mut bytes_read: u64 = 0;
            loop {
                match entry_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes_read = bytes_read.saturating_add(n as u64);
                        continue;
                    }
                    Err(e) => {
                        // R0080-0030: a decoder-reported CRC mismatch or
                        // a corrupt/truncated compressed stream is an
                        // integrity failure recorded against the entry;
                        // a genuine archive-file I/O error is operational
                        // and propagates as the true error.
                        if !sevenz_read_error_is_corruption(&e) {
                            operational_error =
                                Some(ArchiveError::io("read", self.path.clone(), e));
                            return Ok(false);
                        }
                        // R0075-0054: drain remaining data after a
                        // CRC failure so the solid-stream cursor
                        // stays aligned for the next entry.
                        // Subsequent drain errors are intentionally
                        // ignored — the entry is already recorded
                        // as failed, and any persistent stream
                        // damage will surface again on the next
                        // entry's own read loop. Surfacing them
                        // separately here would require a parallel
                        // failed-list that the caller currently
                        // has no way to interpret.
                        while entry_reader.read(&mut buf).unwrap_or(0) > 0 {}
                        failed_files.push(path);
                        return Ok(true);
                    }
                }
            }
            // R0001-0024: a clean early EOF is an integrity failure, not a
            // pass — record the short entry instead of reporting success.
            if bytes_read != entry.size {
                failed_files.push(path);
            }
            Ok(true)
        });

        // R0001-0023: prefer the typed error the callback stashed over the
        // upstream walk error the abort may have produced (see the
        // `extract_all_with_options` note).
        if let Some(err) = operational_error {
            return Err(err);
        }

        walk.map_err(|e| self.map_walk_error(e, "test_integrity"))?;

        // R0001-0025: prove the whole file table was tested. The walk's
        // per-callback bound above rejects an overrun; this rejects a
        // truncated walk that would otherwise return an incomplete
        // "everything passed" report. `visit_order` is an exact
        // permutation of `0..files.len()` (R0081-0074), so equality here
        // means every declared entry position was visited exactly once.
        if callback_pos != visit_order.len() {
            return Err(ArchiveError::corruption(
                self.path.display().to_string(),
                format!(
                    "test_integrity: archive walk visited {} of {} declared entries",
                    callback_pos,
                    visit_order.len()
                ),
            ));
        }

        Ok(failed_files)
    }
}

/// Apply preserved directory metadata after extraction (R0001-0060).
///
/// The file path applies metadata to the *staged* handle before the
/// atomic install (`common::apply_preserved_metadata`);
/// directories have no staging step, so they are stamped in place once
/// every child is written. `unix_mode` is masked to the permission bits
/// (matching the `ArchiveEntry::permissions` contract) and is a no-op on
/// non-Unix platforms, exactly as the file path does.
///
/// The timestamp is applied before the mode so a restrictive archived
/// mode cannot block the handle the timestamp needs; `chmod` touches
/// `ctime`, never `mtime`, so the order is otherwise immaterial.
fn apply_directory_metadata(
    path: &Path,
    unix_mode: Option<u32>,
    modified: Option<std::time::SystemTime>,
) -> Result<()> {
    if let Some(mtime) = modified {
        let dir = open_directory_for_metadata(path)
            .map_err(|e| ArchiveError::io("open_dir", path.to_path_buf(), e))?;
        dir.set_modified(mtime)
            .map_err(|e| ArchiveError::io("set_times", path.to_path_buf(), e))?;
    }
    #[cfg(unix)]
    if let Some(mode) = unix_mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))
            .map_err(|e| ArchiveError::io("set_permissions", path.to_path_buf(), e))?;
    }
    #[cfg(not(unix))]
    let _ = unix_mode;
    Ok(())
}

/// Open a directory handle that `SetFileTime` / `futimens` accept
/// (R0001-0060).
///
/// Unix opens the directory read-only — `futimens` needs no write
/// access. Windows refuses `File::open` on a directory outright: the
/// handle must carry `FILE_FLAG_BACKUP_SEMANTICS`, and setting
/// timestamps needs `FILE_WRITE_ATTRIBUTES` rather than `GENERIC_WRITE`.
fn open_directory_for_metadata(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0000_0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open(path)
    }
}

/// Does a `for_each_entries` read error mark a corrupt entry payload
/// (record it as an integrity failure, R0080-0030) rather than a genuine
/// archive-file I/O error (propagate it)?
///
/// sevenz-rust2 surfaces a decoder-detected CRC mismatch as
/// `io::Error::other(sevenz_rust2::Error::ChecksumVerificationFailed)` and
/// other decode/format faults as `io::Error::other(Error::…)`; the inner
/// value downcasts to the crate's error type. A real read failure on the
/// underlying archive file propagates as a plain OS `io::Error` whose inner
/// value is not a `sevenz_rust2::Error` (or is its `Io` / `FileOpen`
/// wrapper), while a corrupt/truncated codec stream shows up as a plain
/// `InvalidData` / `UnexpectedEof`.
fn sevenz_read_error_is_corruption(e: &std::io::Error) -> bool {
    if let Some(inner) = e
        .get_ref()
        .and_then(|i| i.downcast_ref::<sevenz_rust2::Error>())
    {
        return !matches!(
            inner,
            sevenz_rust2::Error::Io(..) | sevenz_rust2::Error::FileOpen(..)
        );
    }
    matches!(
        e.kind(),
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
    )
}

/// Phrase-based encryption-error classifier for sevenz-rust2 errors.
///
/// R0075-0027: previously the classifier accepted any message
/// containing `password` / `encrypted` / `aes` and treated it as a
/// password failure. That misfires on archives whose error text
/// mentions any of those words for an unrelated reason. Restrict to
/// canonical phrases the upstream crate emits for genuine encryption
/// failures.
fn is_sevenz_encryption_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    const ENCRYPTION_PHRASES: &[&str] = &[
        "password required",
        "password is required",
        "wrong password",
        "bad password",
        "invalid password",
        "incorrect password",
        "password mismatch",
        "passphrase required",
        "passphrase incorrect",
        "wrong passphrase",
        "incorrect passphrase",
        "bad passphrase",
        "decryption failed",
        "decryption error",
        "encrypted file",
        "encrypted archive requires",
        "aes-256 encryption",
        "aes encryption requires",
    ];
    ENCRYPTION_PHRASES.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests;
