//! Safe wrapper for libarchive operations

use super::common::path_to_cstring_checked;
use super::libarchive::*;
use crate::entry::{ArchiveEntry, EntryType};
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::options::{ProgressCallback, RateLimiter};
use crate::security::sanitize_entry_path;
use once_cell::sync::OnceCell;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::{c_int, c_longlong, c_uint};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// Every libarchive symbol and ABI constant this module uses — including
// `archive_entry_size_is_set`, which used to be declared here — is bound
// in `super::libarchive` and reaches this module through the glob import
// above. See that module's declaration-site rule before adding an
// accessor; declaring one locally is a test failure, not a style nit.

/// Safe wrapper for libarchive
///
/// **Path storage (AD 0064).** The archive path is stored as
/// `PathBuf`. CString conversion at the FFI boundary uses
/// `path_to_cstring` (Unix bytes via `OsStrExt::as_bytes`) so
/// non-UTF-8 paths round-trip losslessly on Unix. Windows wide-char
/// support (`archive_read_open_filename_w`) is deferred to the
/// OI-0065-001 follow-up; the current Windows path falls back to
/// UTF-8 lossy conversion at the FFI boundary.
pub struct LibarchiveArchive {
    path: PathBuf,
    write_handle: Option<*mut Archive>, // For creation mode
    /// Output file owned by the writer when libarchive was opened via
    /// `archive_write_open_fd`. Holding the `File` here keeps the
    /// underlying descriptor valid for libarchive's lifetime so an
    /// outside process cannot swap the inode after our exclusive
    /// `O_CREAT|O_EXCL` check (R0072-0004). `close_write` takes it
    /// after `archive_write_close` returns and `sync_data`s it so the
    /// finished archive is crash-durable (R0079-0002); the kernel-level
    /// fd close runs after libarchive has flushed its buffers.
    write_output: Option<std::fs::File>,
    progress: Option<Box<dyn crate::options::ProgressCallback>>,
    bytes_written: u64,
    entries_written: usize,
    /// Reusable scratch buffer for streaming writes; allocated lazily on first
    /// streaming `add_file_*` call to avoid per-entry allocation in `commit_changes`.
    stream_buffer: Vec<u8>,
    /// Mirror of `ZipWriter::poisoned` (R0072-0008), and *only* that:
    /// set after any mid-entry write failure so subsequent `add_*` calls
    /// short-circuit instead of appending on top of an uncertain partial
    /// entry. The libarchive write handle is left intact so `finish` /
    /// `Drop` can still drain whatever entries succeeded before the
    /// failure — an entry-write poison therefore does not block, and is
    /// never consulted by, finalization. Terminal finalization state
    /// lives in `finish_failure`; the two used to share this flag, which
    /// is what discarded libarchive's own close-failure text.
    write_poisoned: bool,
    /// R0001-0018: rendered reason of a finalize that already failed.
    /// `ArchiveError` is not `Clone`, so the message is kept and replayed
    /// as a typed `OperationBlocked` from every later `close_write`.
    /// Named to match `ZipWriter::finish_failure` so both writer backends
    /// spell the same state the same way.
    finish_failure: Option<String>,
    /// Memoised entry listing (AD 0065 / OI-0065-003). The first
    /// `list_files_metadata_only` call populates this snapshot;
    /// subsequent calls share it via `Arc::clone` instead of reopening
    /// the archive. Closes the libarchive half of OI-0075-002 — every
    /// read backend now agrees that the listing is frozen at first
    /// observation. Write-mode handles never populate this cell.
    cached_listing: OnceCell<std::sync::Arc<Vec<ArchiveEntry>>>,
    /// Free-form `ARCHIVE_WARN` text libarchive produced while reading
    /// headers, in production order (OI-0080-006 / ticgit 12d431).
    ///
    /// Every read walk accepts `ARCHIVE_WARN`, because libarchive returns
    /// it when it recovered. What used to happen next was that the text
    /// went in the bin, so a caller could not learn what had been
    /// objected to. Most of those walks return through a signature with
    /// no warning channel — several are `ArchiveBackend` trait methods
    /// shared with the ZIP, 7z and UnRAR backends, which never produce
    /// this condition — so the text lands here instead of forcing a
    /// channel onto three unrelated backends.
    ///
    /// `extract_all` drains this into the `Vec<ArchiveWarning>` it already
    /// returns; everything else is reached with
    /// [`LibarchiveArchive::take_backend_warnings`].
    ///
    /// `RefCell`, not `Mutex`: this type is deliberately `!Sync`
    /// (R0079-0015), so `&self` is single-threaded by construction.
    backend_warnings: std::cell::RefCell<Vec<crate::error::ArchiveWarning>>,
}

// SAFETY: `write_handle` is a raw libarchive writer handle owned
// exclusively by this struct — created in `create`, used only inside
// `&mut self` method scopes (never stored elsewhere), and released via
// `close_write` (`archive_write_close` + `archive_write_free`).
// libarchive handles carry no thread-affine state, so moving the owner
// to another thread is sound. All other fields are `Send`
// (`ProgressCallback` has a `Send` supertrait, and
// `RefCell<Vec<ArchiveWarning>>` is `Send` because `ArchiveWarning` is),
// so this impl only restores what the raw pointer suppressed; the
// auto-derived `!Sync` is kept, so `&LibarchiveArchive` still cannot be
// shared across threads (R0079-0015) — which is also what makes the
// `RefCell` warning sink sound.
unsafe impl Send for LibarchiveArchive {}

/// RAII owner for a libarchive *read* handle: frees via
/// `archive_read_free` on drop so every early return in the extract
/// loops is leak-proof by construction. This class of hand-maintained
/// `archive_write_free` / `archive_read_free` ladders produced real
/// leaks before (R0070-0017 / R0070-0018).
struct ReadHandleGuard(*mut Archive);

impl Drop for ReadHandleGuard {
    fn drop(&mut self) {
        unsafe { archive_read_free(self.0) };
    }
}

/// Convert a `SystemTime` into libarchive's signed `(seconds, nanoseconds)`
/// timestamp representation.
///
/// libarchive stores timestamps as signed seconds, so pre-epoch instants map
/// to negative seconds instead of being backfilled with `now()` (R0080-0067)
/// or silently dropped (R0080-0068). For the pre-epoch side we negate
/// `UNIX_EPOCH - t` and borrow a whole second when there is a fractional part
/// so `nanoseconds` stays in `[0, 1_000_000_000)`.
fn systemtime_to_signed(t: std::time::SystemTime) -> (i64, u32) {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_nanos()),
        Err(e) => {
            // `e.duration()` is `UNIX_EPOCH - t`: how far `t` precedes the
            // epoch.
            let d = e.duration();
            let subsec = d.subsec_nanos();
            if subsec == 0 {
                (-(d.as_secs() as i64), 0)
            } else {
                (-(d.as_secs() as i64 + 1), 1_000_000_000 - subsec)
            }
        }
    }
}

/// Accepted checksum-failure vocabulary for libarchive data-read errors
/// (R0076-0046). libarchive has no dedicated errno for payload
/// corruption — the read-data error strings vary by format (`"ZIP bad
/// CRC ..."`, `"... checksum error"`, ...), so classification is
/// substring-based. Case-sensitive on purpose: these are the exact
/// spellings the previous inline matches accepted, and widening the set
/// risks reclassifying unrelated read errors as corruption.
const CHECKSUM_FAILURE_MARKERS: &[&str] = &["checksum", "Checksum", "CRC"];

/// True when a libarchive data-read error message describes a
/// checksum/CRC verification failure (payload corruption) rather than
/// some other read failure (R0076-0046). Every extract read path that
/// wants to distinguish corruption from generic format errors must go
/// through this helper instead of substring-matching locally.
fn is_libarchive_checksum_failure(message: &str) -> bool {
    CHECKSUM_FAILURE_MARKERS.iter().any(|m| message.contains(m))
}

/// Substrings that mark a libarchive message as an *operational* failure —
/// the storage or the OS refused, and the archive itself may be perfectly
/// intact (ticgit cf1474 / R0001-0056).
///
/// Every entry names an I/O verb, because that is the only thing that
/// separates the two classes in libarchive's message text. libarchive
/// raises these through `archive_set_error(a, errno, "Error reading ...")`
/// when a `read`/`open`/`seek`/`write` syscall failed; archive damage gets
/// format-specific wording instead — "Truncated input file", "Damaged tar
/// archive", "Invalid central directory signature".
///
/// The set is deliberately narrow, and both directions of widening it are
/// harmful:
///
/// * a damage message misclassified as operational turns a listed failed
///   file into a hard error, which is exactly the corruption-detection
///   regression cf1474 was filed to avoid;
/// * an operational message misclassified as damage tells the caller their
///   archive is corrupt when their disk is failing.
///
/// Case-sensitive, matching [`CHECKSUM_FAILURE_MARKERS`]. libarchive is a
/// linked system library here, not vendored, so this is version-dependent
/// by nature — it is a best-effort classifier over text the library does
/// not promise to keep stable, and it fails toward "not operational",
/// which preserves today's behaviour.
const OPERATIONAL_FAILURE_MARKERS: &[&str] = &[
    "Error reading",
    "Error opening",
    "Error seeking",
    "Error writing",
    "Can't open",
    "Couldn't open",
    "Failed to open",
];

/// True when a libarchive message describes a storage/OS failure rather
/// than archive damage (ticgit cf1474).
///
/// Callers that need the corruption side use
/// [`is_libarchive_checksum_failure`]; the two are the crate's only
/// message-level discriminators and every path that must tell an
/// operational fault from damage goes through one of them rather than
/// substring-matching locally.
pub(crate) fn is_libarchive_operational_failure(message: &str) -> bool {
    OPERATIONAL_FAILURE_MARKERS
        .iter()
        .any(|marker| message.contains(marker))
}

/// Get error message from archive
unsafe fn get_archive_error(archive: *mut Archive) -> String {
    if archive.is_null() {
        return "Archive pointer is null".to_string();
    }

    let err_ptr = unsafe { archive_error_string(archive) };
    if err_ptr.is_null() {
        return "Unknown error".to_string();
    }

    unsafe { CStr::from_ptr(err_ptr) }
        .to_string_lossy()
        .into_owned()
}

mod reader;
mod writer;

#[cfg(test)]
mod tests {
    use super::*;

    /// R0079-0015: the narrow `unsafe impl Send` on the FFI-handle owner
    /// must keep compiling — `Archive`'s `Send` is derived from it.
    #[test]
    fn test_libarchive_archive_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<LibarchiveArchive>();
    }

    /// R0076-0046: the data-read error classifier must recognize the
    /// checksum-failure vocabulary libarchive emits across formats, and
    /// must not classify unrelated read errors as corruption.
    #[test]
    fn checksum_failure_classifier_matches_known_messages() {
        // Known libarchive checksum/CRC failure message shapes.
        for msg in [
            "ZIP bad CRC: 0x12345678 should be 0x9abcdef0",
            "Checksum failure",
            "LZMA checksum error",
            "sha1 checksum error",
            "CRC error",
        ] {
            assert!(
                is_libarchive_checksum_failure(msg),
                "expected checksum-failure classification for: {msg}"
            );
        }
        // Counter-examples: real read failures that are not corruption
        // of a verified payload — these must stay generic format errors.
        for msg in [
            "Truncated input file (needed 512 bytes, only 0 available)",
            "Unrecognized archive format",
            "Damaged tar archive",
            "crc mismatch", // lowercase 'crc' was never matched
        ] {
            assert!(
                !is_libarchive_checksum_failure(msg),
                "must not classify as checksum failure: {msg}"
            );
        }
    }

    /// ticgit cf1474: the operational discriminator must separate a
    /// storage/OS failure from archive damage.
    ///
    /// Both directions matter and neither is the safe one. A damage
    /// message read as operational turns a listed failed file into a hard
    /// error, losing the rest of the integrity walk — the regression
    /// cf1474 was filed to avoid. An operational message read as damage
    /// tells the caller their archive is corrupt when their disk is
    /// failing.
    #[test]
    fn operational_failure_classifier_separates_storage_faults_from_damage() {
        // libarchive raises these through `archive_set_error(a, errno, ...)`
        // after a failed syscall: the archive may be perfectly intact.
        for msg in [
            "Error reading 'backup.tar.gz'",
            "Error opening archive",
            "Error seeking in archive",
            "Error writing to disk",
            "Can't open file",
            "Couldn't open archive",
            "Failed to open '/mnt/dead/archive.zip'",
        ] {
            assert!(
                is_libarchive_operational_failure(msg),
                "expected operational classification for: {msg}"
            );
        }

        // Archive damage. Every one of these must stay out, because each
        // is a `failed_files` entry rather than a walk-ending error.
        for msg in [
            "Truncated input file (needed 512 bytes, only 0 available)",
            "Damaged tar archive",
            "Unrecognized archive format",
            "Invalid central directory signature",
            "ZIP bad CRC: 0x12345678 should be 0x9abcdef0",
            "Checksum failure",
            // Case matters, matching CHECKSUM_FAILURE_MARKERS: widening to
            // case-insensitive would start matching prose.
            "error reading",
        ] {
            assert!(
                !is_libarchive_operational_failure(msg),
                "must not classify as operational: {msg}"
            );
        }
    }

    /// The two message-level discriminators must not both claim the same
    /// message: a message is either a checksum failure or a storage fault,
    /// never both, or the classification order would decide the answer.
    #[test]
    fn the_two_message_discriminators_do_not_overlap() {
        for msg in CHECKSUM_FAILURE_MARKERS {
            assert!(
                !is_libarchive_operational_failure(msg),
                "checksum marker {msg:?} also reads as operational"
            );
        }
        for msg in OPERATIONAL_FAILURE_MARKERS {
            assert!(
                !is_libarchive_checksum_failure(msg),
                "operational marker {msg:?} also reads as a checksum failure"
            );
        }
    }

    /// Build a minimal old-GNU sparse tar: one entry whose archived
    /// data is a single 512-byte segment at logical offset 4096, with
    /// a declared (real) size of 4608 — the hole spans `[0, 4096)`.
    fn build_gnu_sparse_tar() -> Vec<u8> {
        let mut header = [0u8; 512];
        header[..10].copy_from_slice(b"sparse.bin");
        header[100..108].copy_from_slice(b"0000644\0"); // mode
        header[108..116].copy_from_slice(b"0000000\0"); // uid
        header[116..124].copy_from_slice(b"0000000\0"); // gid
        // size: bytes physically stored in the archive (one block)
        header[124..136].copy_from_slice(b"00000001000\0");
        header[136..148].copy_from_slice(b"00000000000\0"); // mtime
        header[156] = b'S'; // old-GNU sparse entry
        header[257..265].copy_from_slice(b"ustar  \0"); // old-GNU magic
        // sparse[0]: data segment at offset 4096 (0o10000), length 512 (0o1000)
        header[386..398].copy_from_slice(b"00000010000\0");
        header[398..410].copy_from_slice(b"00000001000\0");
        // realsize: logical file size, hole + data = 4608 (0o11000)
        header[483..495].copy_from_slice(b"00000011000\0");
        // checksum (field counts as spaces while summing)
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        header[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());

        let mut tar = Vec::with_capacity(512 * 4);
        tar.extend_from_slice(&header);
        tar.extend_from_slice(&[0xABu8; 512]); // the data segment
        tar.extend_from_slice(&[0u8; 1024]); // end-of-archive blocks
        tar
    }

    /// R0079-0023: `extract_to_memory` must zero-fill sparse holes the
    /// same way disk extraction does, instead of mis-assembling the
    /// data segments and tripping the declared-size corruption check.
    #[test]
    fn extract_to_memory_zero_fills_sparse_holes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("sparse.tar");
        std::fs::write(&tar_path, build_gnu_sparse_tar()).expect("write fixture");

        let archive = LibarchiveArchive::open(&tar_path).expect("open sparse tar");
        let data = archive
            .extract_to_memory("sparse.bin")
            .expect("sparse entry extracts to memory");
        assert_eq!(data.len(), 4608);
        assert!(
            data[..4096].iter().all(|&b| b == 0),
            "hole must be zero-filled"
        );
        assert!(
            data[4096..].iter().all(|&b| b == 0xAB),
            "data segment bytes must land after the hole"
        );
    }
    /// R0081-0057 / R0081-0058 (memory analogue): a caller cap tighter
    /// than the declared size is the binding limit. Crossing it — here
    /// via the sparse hole's logical extent, not the physical bytes —
    /// must be an `operation_blocked` policy refusal naming the caller
    /// cap, not an archive-corruption error mislabeling the cap as the
    /// "declared" size.
    #[test]
    fn extract_to_memory_sparse_cap_attributes_caller_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("sparse.tar");
        std::fs::write(&tar_path, build_gnu_sparse_tar()).expect("write fixture");

        let archive = LibarchiveArchive::open(&tar_path).expect("open sparse tar");
        // The fixture's logical size is 4608 (a 4096-byte hole plus a
        // 512-byte segment) but only 512 physical bytes are stored. A
        // 1000-byte caller cap is tighter than the declared 4608, so the
        // logical extent crosses it and the refusal is a caller-policy
        // block — not corruption, and it must name 1000, not the
        // declared size.
        let err = archive
            .extract_to_memory_with_max_bytes("sparse.bin", Some(1000))
            .expect_err("caller cap below the logical extent must block");
        let msg = err.to_string();
        assert!(
            msg.contains("caller-configured") && msg.contains("1000"),
            "expected a caller-cap policy block, got: {msg}"
        );
        assert!(
            !msg.contains("declared"),
            "caller cap must not be mislabeled as the declared size: {msg}"
        );
    }

    /// OI-0076-002: single-entry extraction validates against the AD
    /// 0065 memoised listing first (gate shapes), and the index walk
    /// must refuse a stale snapshot — name drift at the target index,
    /// or EOF before the target index — when the archive file is
    /// rewritten on disk after listing.
    #[test]
    fn stale_cached_listing_surfaces_drift_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("drift.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer.add_file_from_data("a.txt", b"alpha").expect("add a");
        writer.add_file_from_data("b.txt", b"bravo").expect("add b");
        writer.close_write().expect("finish");

        let archive = LibarchiveArchive::open(&tar_path).expect("open");
        // Gate shape (OI-0076-002): unknown names are rejected from the
        // listing snapshot, not by walking the payload. This also
        // freezes the AD 0065 snapshot for the drift checks below.
        let err = archive
            .extract_to_memory("missing.txt")
            .expect_err("unknown entry must be gate-rejected");
        assert!(
            err.to_string().contains("not found in archive metadata"),
            "unexpected error: {err}"
        );

        // Rewrite the archive on disk behind the memoised listing:
        // one entry, different name.
        let swapped = dir.path().join("swapped.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&swapped, crate::ArchiveFormat::Tar, &mut options)
                .expect("create replacement tar");
        writer.add_file_from_data("x.txt", b"xxxxx").expect("add x");
        writer.close_write().expect("finish replacement");
        std::fs::rename(&swapped, &tar_path).expect("swap archive on disk");

        // Target index still exists but carries a different name.
        let err = archive
            .extract_to_memory("a.txt")
            .expect_err("stale name at target index must be refused");
        assert!(
            err.to_string()
                .contains("single-entry listing drift at index 0"),
            "unexpected error: {err}"
        );
        let err = archive
            .extract_to_stream("a.txt")
            .map(|_| ())
            .expect_err("stream must apply the same drift guard");
        assert!(
            err.to_string()
                .contains("single-entry listing drift at index 0"),
            "unexpected error: {err}"
        );

        // Target index is beyond the rewritten archive's end.
        let err = archive
            .extract_to_memory("b.txt")
            .expect_err("EOF before target index must be refused");
        assert!(
            err.to_string()
                .contains("entry index 1 not reached before end of archive"),
            "unexpected error: {err}"
        );
    }

    /// R0079-0003: `archive_entry_mode` / `archive_entry_filetype` /
    /// `archive_entry_set_perm` are bound with libarchive's real
    /// `__LA_MODE_T` width (16-bit on macOS). Round-trip permissions
    /// and file types through the writer and the listing to lock the
    /// bindings down.
    #[test]
    fn entry_mode_round_trips_through_la_mode_t_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("modes.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer
            .add_file_from_data("a.txt", b"hello")
            .expect("add file entry");
        writer.add_directory_entry("d").expect("add dir entry");
        writer.close_write().expect("finish");

        let archive = LibarchiveArchive::open(&tar_path).expect("open");
        let entries = archive.list_files_metadata_only().expect("list");
        let file = entries
            .iter()
            .find(|e| e.path == "a.txt")
            .expect("file entry listed");
        assert_eq!(file.entry_type, EntryType::File);
        assert_eq!(file.permissions, Some(0o644));
        let dir_entry = entries
            .iter()
            .find(|e| e.path == "d/")
            .expect("dir entry listed");
        assert_eq!(dir_entry.entry_type, EntryType::Directory);
        assert_eq!(dir_entry.permissions, Some(0o755));
    }

    /// R0080-0009 / R0080-0016: bulk extraction pins the AD 0065 listing
    /// the safety gate validated and cross-checks the fresh walk against
    /// it. A post-listing on-disk swap — name drift at a consumed index,
    /// extra live entries, or EOF before the listing length — must be
    /// refused instead of extracting unchecked content.
    #[test]
    fn stale_cached_listing_surfaces_bulk_drift_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("bulk-drift.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer.add_file_from_data("a.txt", b"alpha").expect("add a");
        writer.add_file_from_data("b.txt", b"bravo").expect("add b");
        writer.close_write().expect("finish");

        let archive = LibarchiveArchive::open(&tar_path).expect("open");
        // Freeze the AD 0065 snapshot at [a.txt, b.txt].
        let listed = archive.list_files_metadata_only().expect("list");
        assert_eq!(listed.len(), 2);

        // Rewrite the archive file on disk with the given entry names,
        // behind the memoised listing.
        let rewrite = |names: &[&str]| {
            let staged = dir.path().join("staged.tar");
            let mut options = crate::options::CompressionOptions::default();
            let mut writer =
                LibarchiveArchive::create(&staged, crate::ArchiveFormat::Tar, &mut options)
                    .expect("create replacement tar");
            for name in names {
                writer
                    .add_file_from_data(name, b"xxxxx")
                    .expect("add replacement entry");
            }
            writer.close_write().expect("finish replacement");
            std::fs::rename(&staged, &tar_path).expect("swap archive on disk");
        };

        // Name drift at a consumed index.
        rewrite(&["a.txt", "z.txt"]);
        let out = dir.path().join("out1");
        std::fs::create_dir_all(&out).expect("mkdir out1");
        let err = archive
            .extract_all(&out, None)
            .expect_err("name drift at index 1 must be refused");
        assert!(
            err.to_string()
                .contains("single-entry listing drift at index 1"),
            "unexpected error: {err}"
        );

        // More live entries than the validated listing.
        rewrite(&["a.txt", "b.txt", "c.txt"]);
        let out = dir.path().join("out2");
        std::fs::create_dir_all(&out).expect("mkdir out2");
        let err = archive
            .extract_all(&out, None)
            .expect_err("extra live entry must be refused");
        assert!(
            err.to_string()
                .contains("exceeds the validated listing length 2"),
            "unexpected error: {err}"
        );

        // EOF before the validated listing length.
        rewrite(&["a.txt"]);
        let out = dir.path().join("out3");
        std::fs::create_dir_all(&out).expect("mkdir out3");
        let err = archive
            .extract_all(&out, None)
            .expect_err("short archive must be refused");
        assert!(
            err.to_string()
                .contains("entry index 1 not reached before end of archive"),
            "unexpected error: {err}"
        );
    }

    /// R0080-0067 / R0080-0068: `systemtime_to_signed` encodes pre-epoch
    /// instants as negative seconds (borrowing a whole second when there is a
    /// fractional part) instead of dropping them or backfilling `now()`.
    #[test]
    fn systemtime_to_signed_round_trips_pre_and_post_epoch() {
        use std::time::{Duration, UNIX_EPOCH};
        // Post-epoch, whole second.
        assert_eq!(
            systemtime_to_signed(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            (1_700_000_000, 0)
        );
        // Epoch itself.
        assert_eq!(systemtime_to_signed(UNIX_EPOCH), (0, 0));
        // Pre-epoch, whole second: -86400 with no borrow.
        assert_eq!(
            systemtime_to_signed(UNIX_EPOCH - Duration::from_secs(86_400)),
            (-86_400, 0)
        );
        // Pre-epoch with a fractional part: 0.25 s before the epoch is
        // second -1 plus 750_000_000 ns.
        assert_eq!(
            systemtime_to_signed(UNIX_EPOCH - Duration::from_nanos(250_000_000)),
            (-1, 750_000_000)
        );
        // Post-epoch with a fractional part stays a straight split.
        assert_eq!(
            systemtime_to_signed(UNIX_EPOCH + Duration::from_nanos(1_250_000_000)),
            (1, 250_000_000)
        );
    }

    /// R0080-0067: a pre-epoch mtime survives a tar create round trip as its
    /// negative-epoch value instead of being replaced with `now()`. The public
    /// listing helper drops pre-epoch reads by crate policy, so the raw entry
    /// mtime is read back at the FFI layer to observe the stored value.
    #[test]
    fn pre_epoch_mtime_survives_tar_round_trip() {
        use std::time::{Duration, UNIX_EPOCH};

        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("pre-epoch.tar");

        // 1969-12-31T00:00:00Z — 86400 s before the epoch.
        let pre_epoch = UNIX_EPOCH - Duration::from_secs(86_400);
        let mut meta = ArchiveEntry::file("old.txt", 0).build();
        meta.modified = Some(pre_epoch);

        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");
        writer
            .add_file_from_data_with_metadata("old.txt", b"hello", &meta)
            .expect("add file with pre-epoch mtime");
        writer.close_write().expect("finish");

        // Read the first entry's raw signed mtime back through libarchive.
        let raw_secs = {
            let c_path = path_to_cstring_checked(&tar_path).expect("cstring");
            unsafe {
                let archive = LibarchiveArchive::open_read_handle(&c_path).expect("open read");
                let _guard = ReadHandleGuard(archive);
                let mut entry_ptr: *mut LibarchiveEntry = std::ptr::null_mut();
                let r = archive_read_next_header(archive, &mut entry_ptr);
                assert_eq!(r, ARCHIVE_OK, "expected the first header");
                archive_entry_mtime(entry_ptr)
            }
        };
        assert!(
            raw_secs < 0,
            "pre-epoch mtime must stay negative, got {raw_secs}"
        );

        let recovered = UNIX_EPOCH - Duration::from_secs((-raw_secs) as u64);
        let delta = pre_epoch
            .duration_since(recovered)
            .or_else(|_| recovered.duration_since(pre_epoch))
            .expect("time difference");
        assert!(
            delta <= Duration::from_secs(1),
            "mtime drifted by {delta:?}"
        );
    }

    /// R0080-0037: a stream reader that produces more than its declared size
    /// is rejected as soon as the surplus byte appears (not after reading to
    /// EOF), and the write failure poisons the writer so no further entry can
    /// be appended.
    #[test]
    fn overproducing_stream_reader_is_rejected_and_poisons_writer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tar_path = dir.path().join("overproduce.tar");
        let mut options = crate::options::CompressionOptions::default();
        let mut writer =
            LibarchiveArchive::create(&tar_path, crate::ArchiveFormat::Tar, &mut options)
                .expect("create tar");

        // Declare 4 bytes but feed the writer far more real data than one
        // 64 KiB chunk so a length-unaware loop would drain the whole reader.
        let mut reader = std::io::Cursor::new(vec![b'x'; 70_000]);
        let err = writer
            .add_file_from_reader("big.txt", &mut reader, 4)
            .expect_err("overproduction must be rejected");
        // DCR-011 / ticgit 9bdf2ca1 item 3: a declared-versus-actual length
        // mismatch is a declared-size violation, so every commit route now
        // classifies it as `Corruption` through
        // `ArchiveError::declared_length_mismatch` rather than as the `Io`
        // "Stream length mismatch" this test used to match. Assert the
        // variant and both numbers rather than the prose, so a future
        // rewording cannot silently make this vacuous.
        match &err {
            ArchiveError::Corruption { details, .. } => {
                assert!(
                    details.contains("over-produced"),
                    "expected an over-production report, got: {details}"
                );
                assert!(
                    details.contains('4'),
                    "the declared size must appear in the message: {details}"
                );
            }
            other => panic!("expected Corruption for a length mismatch, got: {other}"),
        }
        // The surplus is caught at the first extra byte, so the reader still
        // holds almost all of its bytes.
        assert!(
            reader.position() <= 64 * 1024,
            "reader was drained past the declared size: {}",
            reader.position()
        );

        // The failed write poisoned the writer; later writes are blocked.
        let blocked = writer
            .add_file_from_data("after.txt", b"nope")
            .expect_err("poisoned writer must reject further writes");
        assert!(
            blocked.to_string().contains("poisoned"),
            "unexpected error: {blocked}"
        );
    }
}
