//! FFI bindings for libarchive
//!
//! Manual bindings to libarchive for ZIP, 7z, TAR, and other formats
//!
//! # Declaration-site rule (read this before adding an accessor)
//!
//! **Every libarchive symbol and every libarchive ABI constant this
//! crate binds is declared in this file, inside the single
//! `#[link(name = "archive")] unsafe extern "C"` block below. No other
//! file in the crate may open a foreign-function block for libarchive.**
//! Locality is expressed by *visibility and comment grouping*, never by
//! file: a symbol only one module needs is declared `pub(crate)` and
//! grouped under a comment naming its consumer and the record that
//! motivated it. Generalized: one declaration module per C library —
//! this file for libarchive, `super::unrar` for unrar.
//!
//! Why the rule is absolute rather than a preference:
//!
//! * rustc does **not** cross-check two `extern` blocks that declare the
//!   same symbol. Re-declaring, say, `archive_entry_mtime_nsec` as
//!   `-> i64` in another module compiles clean and is silently ABI-wrong
//!   on Windows, where LLP64 makes `long` 32 bits. One home makes that
//!   divergence impossible to even express.
//! * ABI review (the `la_mode_t` / `la_ssize_t` / `c_long` width work of
//!   R0079-0003 and R0001-0053) needs exactly one place to audit.
//! * The "declare it beside its consumer" rule this replaces had already
//!   drifted off its own exemplar: `archive_entry_size_is_set` lived in
//!   `libarchive_wrapper.rs` long after its last consumer there was
//!   routed through `entry_declared_size` in `libarchive_wrapper/reader.rs`
//!   (R0001-0014), and the `_nsec` getters cited that stale rule as their
//!   justification for a third home.
//!
//! Rejected alternative, named so it is not re-proposed: "consolidate the
//! ABI-critical getters but keep single-consumer probes local." It has no
//! crisp boundary, and following it is exactly what produced the three
//! declaration homes this rule collapsed.
//!
//! `declaration_site_tests` below enforces the rule mechanically, over
//! the same set this prose claims: it walks **every** `.rs` file under
//! `src/` and fails any that opens a foreign-function block, exempting
//! only the two declaration modules (this file, and `src/ffi/unrar.rs`
//! for unrar). It previously checked three hard-coded files, so a fourth
//! file reintroducing a block passed silently — the rule said "no other
//! file" while the test said "not these three".

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_long, c_longlong, c_uint};

// Opaque types
pub type Archive = c_void;
pub type LibarchiveEntry = c_void;

// FFI-faithful scalar aliases (R0079-0003). Rust requires `extern`
// declarations to match the foreign function's true signature, so these
// mirror libarchive's own typedefs instead of approximating with `c_int`.

/// `la_ssize_t` — libarchive's signed byte-count type (`ssize_t`; 64-bit
/// on every supported platform, matching the existing `archive_read_data`
/// / `archive_write_data` bindings).
#[allow(non_camel_case_types)]
pub type la_ssize_t = c_longlong;

/// `__LA_MODE_T` — `mode_t` until libarchive 4.0 (archive_entry.h).
/// 16 bits on Apple platforms, FreeBSD, DragonFly, and Windows
/// (`unsigned short`); 32 bits elsewhere (Linux glibc/musl, NetBSD,
/// OpenBSD, illumos).
#[allow(non_camel_case_types)]
#[cfg(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "dragonfly",
    windows
))]
pub type la_mode_t = u16;
#[allow(non_camel_case_types)]
#[cfg(not(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "dragonfly",
    windows
)))]
pub type la_mode_t = u32;

// Archive open modes
pub const ARCHIVE_EOF: c_int = 1;
pub const ARCHIVE_OK: c_int = 0;
pub const ARCHIVE_RETRY: c_int = -10;
pub const ARCHIVE_WARN: c_int = -20;
pub const ARCHIVE_FAILED: c_int = -25;
pub const ARCHIVE_FATAL: c_int = -30;

// Extract flags.
//
// These are a hand transcription of libarchive's own `#define`s in
// `archive.h`, and they are load-bearing: `archive_write_disk_set_options`
// takes an opaque `int`, so a wrong value is not a type error, not a link
// error, and not a runtime error. It silently turns a different feature on.
//
// Two of them WERE wrong. `SECURE_SYMLINKS` was `0x4000` and
// `SECURE_NODOTDOT` was `0x8000`; the real values are `0x0100` and `0x0200`,
// while `0x4000` and `0x8000` are `NO_HFS_COMPRESSION` and
// `HFS_COMPRESSION_FORCED` — two contradictory macOS HFS+ compression flags.
// Since `extract_flags` ORs both security flags unconditionally, every
// libarchive extraction this crate has ever performed ran with libarchive's
// symlink-redirect guard and `..` rejection OFF, and with two conflicting
// compression hints ON. Corrected 2026-09-05 against libarchive 3.8.9's
// `archive.h`; see `extract_flag_values_match_libarchive_header`.
pub const ARCHIVE_EXTRACT_OWNER: c_int = 0x0001;
pub const ARCHIVE_EXTRACT_PERM: c_int = 0x0002;
pub const ARCHIVE_EXTRACT_TIME: c_int = 0x0004;
pub const ARCHIVE_EXTRACT_NO_OVERWRITE: c_int = 0x0008;
pub const ARCHIVE_EXTRACT_ACL: c_int = 0x0020;
pub const ARCHIVE_EXTRACT_FFLAGS: c_int = 0x0040;
pub const ARCHIVE_EXTRACT_SECURE_SYMLINKS: c_int = 0x0100;
pub const ARCHIVE_EXTRACT_SECURE_NODOTDOT: c_int = 0x0200;

// File type constants (`__LA_MODE_T` values in archive_entry.h)
pub const AE_IFMT: la_mode_t = 0o170000;
pub const AE_IFREG: la_mode_t = 0o100000;
pub const AE_IFDIR: la_mode_t = 0o040000;
pub const AE_IFLNK: la_mode_t = 0o120000;

#[link(name = "archive")]
unsafe extern "C" {
    // Archive creation/destruction
    pub fn archive_read_new() -> *mut Archive;
    pub fn archive_read_free(archive: *mut Archive) -> c_int;
    pub fn archive_write_disk_new() -> *mut Archive;
    pub fn archive_write_disk_free(archive: *mut Archive) -> c_int;
    pub fn archive_write_free(archive: *mut Archive) -> c_int;

    // Format support
    pub fn archive_read_support_format_all(archive: *mut Archive) -> c_int;
    pub fn archive_read_support_format_raw(archive: *mut Archive) -> c_int;
    pub fn archive_read_support_format_zip(archive: *mut Archive) -> c_int;
    pub fn archive_read_support_format_7zip(archive: *mut Archive) -> c_int;
    pub fn archive_read_support_format_tar(archive: *mut Archive) -> c_int;
    pub fn archive_read_support_filter_all(archive: *mut Archive) -> c_int;

    // Opening archives
    pub fn archive_read_open_filename(
        archive: *mut Archive,
        filename: *const c_char,
        block_size: usize,
    ) -> c_int;

    // Reading entries
    pub fn archive_read_next_header(
        archive: *mut Archive,
        entry: *mut *mut LibarchiveEntry,
    ) -> c_int;

    pub fn archive_read_data_skip(archive: *mut Archive) -> c_int;

    pub fn archive_read_data(archive: *mut Archive, buffer: *mut c_void, size: usize)
    -> c_longlong;

    // Entry metadata
    pub fn archive_entry_pathname(entry: *mut LibarchiveEntry) -> *const c_char;
    pub fn archive_entry_size(entry: *mut LibarchiveEntry) -> c_longlong;
    pub fn archive_entry_mtime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_mode(entry: *mut LibarchiveEntry) -> la_mode_t;
    pub fn archive_entry_filetype(entry: *mut LibarchiveEntry) -> la_mode_t;
    pub fn archive_entry_hardlink(entry: *mut LibarchiveEntry) -> *const c_char;
    pub fn archive_entry_symlink(entry: *mut LibarchiveEntry) -> *const c_char;

    // Phase 1: Additional metadata functions
    pub fn archive_entry_birthtime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_atime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_ctime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_is_encrypted(entry: *mut LibarchiveEntry) -> c_int;

    // R0076-0043: presence probes for the timestamp fields. libarchive
    // returns `0` from the value getters both when "not set" and when
    // "explicitly set to UNIX_EPOCH" — the `_is_set` companions
    // disambiguate, so a legitimate 1970-01-01T00:00:00Z timestamp is
    // not silently dropped to `None`.
    pub fn archive_entry_mtime_is_set(entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_entry_atime_is_set(entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_entry_ctime_is_set(entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_entry_birthtime_is_set(entry: *mut LibarchiveEntry) -> c_int;

    // Note: CRC32 is format-specific and not available through generic API

    // Error handling
    pub fn archive_error_string(archive: *mut Archive) -> *const c_char;
    pub fn archive_errno(archive: *mut Archive) -> c_int;

    // Extraction to disk
    pub fn archive_write_disk_set_options(archive: *mut Archive, flags: c_int) -> c_int;
    pub fn archive_write_header(archive: *mut Archive, entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_write_finish_entry(archive: *mut Archive) -> c_int;
    pub fn archive_read_data_block(
        archive: *mut Archive,
        buff: *mut *const c_void,
        size: *mut usize,
        offset: *mut c_longlong,
    ) -> c_int;
    // Returns `la_ssize_t`: `ARCHIVE_OK` (0) through libarchive 3.x,
    // the written byte count from 4.0 onward — any value >= ARCHIVE_OK
    // is success; negatives are the ARCHIVE_WARN/FAILED/FATAL family
    // (R0079-0003, mirroring the `archive_write_data` contract).
    pub fn archive_write_data_block(
        archive: *mut Archive,
        buff: *const c_void,
        size: usize,
        offset: c_longlong,
    ) -> la_ssize_t;

    // Set extraction path
    pub fn archive_entry_set_pathname(entry: *mut LibarchiveEntry, pathname: *const c_char);
    pub fn archive_entry_update_pathname_utf8(
        entry: *mut LibarchiveEntry,
        pathname: *const c_char,
    ) -> c_int;

    // Archive writing (creation) functions
    pub fn archive_write_new() -> *mut Archive;
    pub fn archive_write_set_format_zip(archive: *mut Archive) -> c_int;
    pub fn archive_write_set_format_7zip(archive: *mut Archive) -> c_int;
    pub fn archive_write_set_format_pax_restricted(archive: *mut Archive) -> c_int; // Modern TAR
    pub fn archive_write_set_format_ustar(archive: *mut Archive) -> c_int; // Classic TAR
    pub fn archive_write_add_filter_gzip(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_bzip2(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_xz(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_zstd(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_lz4(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_lzma(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_none(archive: *mut Archive) -> c_int;
    pub fn archive_write_set_filter_option(
        archive: *mut Archive,
        module: *const c_char,
        option: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn archive_write_set_format_option(
        archive: *mut Archive,
        module: *const c_char,
        option: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn archive_write_open_filename(archive: *mut Archive, filename: *const c_char) -> c_int;
    pub fn archive_write_open_fd(archive: *mut Archive, fd: c_int) -> c_int;
    pub fn archive_write_set_passphrase(archive: *mut Archive, passphrase: *const c_char) -> c_int;
    pub fn archive_write_data(
        archive: *mut Archive,
        buff: *const c_void,
        size: usize,
    ) -> c_longlong;
    pub fn archive_write_close(archive: *mut Archive) -> c_int;

    // Entry creation and manipulation
    pub fn archive_entry_new() -> *mut LibarchiveEntry;
    pub fn archive_entry_free(entry: *mut LibarchiveEntry);
    pub fn archive_entry_clear(entry: *mut LibarchiveEntry) -> *mut LibarchiveEntry;
    pub fn archive_entry_set_size(entry: *mut LibarchiveEntry, size: c_longlong);
    pub fn archive_entry_set_filetype(entry: *mut LibarchiveEntry, filetype: c_uint);
    pub fn archive_entry_set_perm(entry: *mut LibarchiveEntry, perm: la_mode_t);
    pub fn archive_entry_set_mtime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_entry_set_atime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_entry_set_ctime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_entry_set_birthtime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_format(archive: *mut Archive) -> c_int;

    // R0081-0052 / R0081-0062: `archive_entry_size_is_set` is the only
    // libarchive probe that distinguishes a declared zero size from an
    // unset one — the value getter (`archive_entry_size`) returns 0 for
    // both. Consumed by `entry_declared_size` in
    // `libarchive_wrapper/reader.rs` (R0001-0014), which every size read
    // in the read paths routes through. Signature per libarchive's
    // `archive_entry.h`:
    //   int archive_entry_size_is_set(struct archive_entry *);
    pub(crate) fn archive_entry_size_is_set(entry: *mut LibarchiveEntry) -> c_int;

    // R0001-0053: the whole-second timestamp getters above truncate every
    // archive timestamp to one-second resolution. libarchive exposes the
    // subsecond half through this parallel `_nsec` family, consumed by
    // `parse_entry` in `libarchive_wrapper/reader.rs`. The return type is
    // `c_long`, not `i64`: `long` is 32-bit on Windows LLP64, and
    // widening it here would silently corrupt every subsecond timestamp
    // on that target. Signatures per libarchive's `archive_entry.h`:
    //   long archive_entry_mtime_nsec(struct archive_entry *);
    //   long archive_entry_atime_nsec(struct archive_entry *);
    //   long archive_entry_ctime_nsec(struct archive_entry *);
    //   long archive_entry_birthtime_nsec(struct archive_entry *);
    pub(crate) fn archive_entry_mtime_nsec(entry: *mut LibarchiveEntry) -> c_long;
    pub(crate) fn archive_entry_atime_nsec(entry: *mut LibarchiveEntry) -> c_long;
    pub(crate) fn archive_entry_ctime_nsec(entry: *mut LibarchiveEntry) -> c_long;
    pub(crate) fn archive_entry_birthtime_nsec(entry: *mut LibarchiveEntry) -> c_long;
}

/// libarchive format-code base mask. The low 16 bits identify the
/// concrete sub-variant (e.g. `TAR_USTAR` vs `TAR_GNUTAR`); the high
/// bits identify the format family. AD 0062 A.6's TAR-confirmation
/// path uses the masked value to distinguish a real tar stream from
/// the raw-pseudo-format libarchive returns when the underlying
/// payload after decompression isn't actually tar.
pub const ARCHIVE_FORMAT_BASE_MASK: c_int = 0xff_0000;
pub const ARCHIVE_FORMAT_TAR_BASE: c_int = 0x30_000;
pub const ARCHIVE_FORMAT_RAW_BASE: c_int = 0x90_000;
/// libarchive's `ARCHIVE_FORMAT_ZIP` base code (`archive.h`), consumed
/// by the R0001-0052 creation-time gate in `libarchive_wrapper/reader.rs`.
pub(crate) const ARCHIVE_FORMAT_ZIP_BASE: c_int = 0x5_0000;

/// Enforcement of the declaration-site rule stated in this module's
/// documentation: libarchive is bound in exactly one place, so no two
/// `extern` blocks can drift into incompatible signatures for the same
/// symbol without rustc noticing (it never would).
#[cfg(test)]
mod declaration_site_tests {
    use std::path::{Path, PathBuf};

    /// Block-opening needle. Deliberately *not* `extern "C"` alone:
    /// `reader.rs` spells a function-pointer *type* with the same ABI
    /// string in its setup-call table and `wrapper.rs` defines an
    /// `extern "C" fn` callback for UnRAR; neither declares a foreign
    /// symbol, and both must keep compiling. `unsafe extern "C" {`
    /// contains this needle, so the Rust 2024 form is covered.
    const NEEDLE: &str = "extern \"C\" {";

    /// The one declaration module per C library (this file for libarchive,
    /// `super::unrar` for unrar). Every other `.rs` file under `src/` is
    /// forbidden from opening a foreign-function block.
    const DECLARATION_HOMES: [&str; 2] = ["src/ffi/libarchive.rs", "src/ffi/unrar.rs"];

    /// Repo-relative, forward-slash path for a file under the manifest dir.
    fn relative_slash_path(root: &Path, file: &Path) -> String {
        file.strip_prefix(root)
            .unwrap_or(file)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Mechanical enforcement of the declaration-site rule stated in this
    /// module's documentation.
    ///
    /// This used to `include_str!` three hard-coded files, so a *fourth*
    /// file reintroducing an `extern` block passed silently — which is
    /// exactly how the three declaration homes the rule collapsed arose in
    /// the first place. The check now walks every `.rs` file under `src/`,
    /// so the enforcement covers the same set the prose claims: "no other
    /// file in the crate".
    ///
    /// The walk is rooted at `CARGO_MANIFEST_DIR` rather than the process
    /// cwd, and asserts both a floor on the number of files scanned and
    /// that the historically-drifting files were among them, so a walk
    /// that silently found nothing fails instead of passing vacuously.
    #[test]
    fn libarchive_symbols_have_one_declaration_site() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let src = root.join("src");
        assert!(
            src.is_dir(),
            "source tree not found at {}; the declaration-site rule cannot be enforced",
            src.display()
        );

        let mut scanned: Vec<String> = Vec::new();
        for entry in walkdir::WalkDir::new(&src)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let relative = relative_slash_path(&root, path);
            scanned.push(relative.clone());
            if DECLARATION_HOMES.contains(&relative.as_str()) {
                continue;
            }
            let source = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            assert!(
                !source.contains(NEEDLE),
                "{relative} opens a foreign-function block; every declaration belongs in its \
                 library's one declaration module ({}) — see this module's declaration-site rule",
                DECLARATION_HOMES.join(" or ")
            );
        }

        // Guard against a walk that found nothing and "passed".
        assert!(
            scanned.len() >= 20,
            "declaration-site walk scanned only {} files; the walk is broken, not the tree",
            scanned.len()
        );
        for anchor in [
            "src/ffi/libarchive.rs",
            "src/ffi/libarchive_wrapper.rs",
            "src/ffi/libarchive_wrapper/reader.rs",
            "src/ffi/libarchive_wrapper/writer.rs",
        ] {
            assert!(
                scanned.iter().any(|s| s == anchor),
                "declaration-site walk missed {anchor}"
            );
        }
    }
}

/// The extract-flag constants are a hand transcription of C `#define`s, and
/// `archive_write_disk_set_options` takes an opaque `int` — so a wrong value
/// cannot fail to compile, fail to link, or fail at run time. It just turns a
/// different feature on, permanently and silently.
///
/// That is not hypothetical here. `SECURE_SYMLINKS` and `SECURE_NODOTDOT`
/// carried `0x4000` / `0x8000` until 2026-09-05 — the values of
/// `NO_HFS_COMPRESSION` and `HFS_COMPRESSION_FORCED` — so libarchive's two
/// extraction guards were off for the whole life of the crate while two
/// contradictory macOS compression hints were on. Nothing caught it because
/// there was nothing to catch it with. This module is that something.
#[cfg(test)]
mod extract_flag_tests {
    use super::*;

    /// Read libarchive's own header if we can find it, and compare every flag
    /// this crate declares against the `#define` it claims to mirror.
    ///
    /// Header discovery is best-effort — it is not present on every machine —
    /// so a miss skips the comparison rather than failing. The pinned-value
    /// test below is what guarantees this module always asserts *something*.
    #[test]
    fn extract_flag_values_match_libarchive_header() {
        const CANDIDATES: [&str; 4] = [
            "/opt/homebrew/opt/libarchive/include/archive.h",
            "/usr/local/opt/libarchive/include/archive.h",
            "/usr/include/archive.h",
            "/usr/local/include/archive.h",
        ];
        let Some(header) = CANDIDATES
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
        else {
            eprintln!("libarchive archive.h not found; comparison skipped");
            return;
        };

        // `#define\tARCHIVE_EXTRACT_PERM\t\t\t(0x0002)` — the value is always
        // parenthesised hex in this header.
        let defined = |name: &str| -> Option<i32> {
            header.lines().find_map(|line| {
                let rest = line.strip_prefix("#define")?.trim_start();
                let rest = rest.strip_prefix(name)?;
                // Guard against `ARCHIVE_EXTRACT_TIME` matching
                // `ARCHIVE_EXTRACT_TIME_SOMETHING`.
                if !rest.starts_with(char::is_whitespace) {
                    return None;
                }
                let value = rest.trim().trim_start_matches('(').trim_end_matches(')');
                let digits = value
                    .strip_prefix("0x")
                    .or_else(|| value.strip_prefix("0X"))?;
                i32::from_str_radix(digits, 16).ok()
            })
        };

        for (name, ours) in [
            ("ARCHIVE_EXTRACT_OWNER", ARCHIVE_EXTRACT_OWNER),
            ("ARCHIVE_EXTRACT_PERM", ARCHIVE_EXTRACT_PERM),
            ("ARCHIVE_EXTRACT_TIME", ARCHIVE_EXTRACT_TIME),
            ("ARCHIVE_EXTRACT_NO_OVERWRITE", ARCHIVE_EXTRACT_NO_OVERWRITE),
            ("ARCHIVE_EXTRACT_ACL", ARCHIVE_EXTRACT_ACL),
            ("ARCHIVE_EXTRACT_FFLAGS", ARCHIVE_EXTRACT_FFLAGS),
            (
                "ARCHIVE_EXTRACT_SECURE_SYMLINKS",
                ARCHIVE_EXTRACT_SECURE_SYMLINKS,
            ),
            (
                "ARCHIVE_EXTRACT_SECURE_NODOTDOT",
                ARCHIVE_EXTRACT_SECURE_NODOTDOT,
            ),
        ] {
            let theirs = defined(name)
                .unwrap_or_else(|| panic!("{name} not found in the located archive.h"));
            assert_eq!(
                ours, theirs,
                "{name}: this crate declares {ours:#06x}, libarchive defines {theirs:#06x}. \
                 A mismatch here silently enables a DIFFERENT libarchive feature — that is \
                 exactly how the two SECURE_* guards spent the crate's whole life switched off."
            );
        }
    }

    /// The values, pinned literally, so this module asserts something even on
    /// a host with no libarchive headers installed. Sourced from libarchive
    /// 3.8.9 `archive.h`; they have been stable across libarchive 3.x.
    #[test]
    fn extract_flag_values_are_pinned() {
        assert_eq!(ARCHIVE_EXTRACT_OWNER, 0x0001);
        assert_eq!(ARCHIVE_EXTRACT_PERM, 0x0002);
        assert_eq!(ARCHIVE_EXTRACT_TIME, 0x0004);
        assert_eq!(ARCHIVE_EXTRACT_NO_OVERWRITE, 0x0008);
        assert_eq!(ARCHIVE_EXTRACT_ACL, 0x0020);
        assert_eq!(ARCHIVE_EXTRACT_FFLAGS, 0x0040);
        assert_eq!(ARCHIVE_EXTRACT_SECURE_SYMLINKS, 0x0100);
        assert_eq!(ARCHIVE_EXTRACT_SECURE_NODOTDOT, 0x0200);
    }

    /// The two guards are not optional and must not become caller-tunable:
    /// every extraction gets them. This pins the intent so a future options
    /// refactor cannot quietly drop them behind a flag.
    #[test]
    fn both_security_guards_are_distinct_bits_and_not_compression_flags() {
        assert_ne!(
            ARCHIVE_EXTRACT_SECURE_SYMLINKS,
            ARCHIVE_EXTRACT_SECURE_NODOTDOT
        );
        // 0x4000 / 0x8000 are NO_HFS_COMPRESSION / HFS_COMPRESSION_FORCED.
        for flag in [
            ARCHIVE_EXTRACT_SECURE_SYMLINKS,
            ARCHIVE_EXTRACT_SECURE_NODOTDOT,
        ] {
            assert_eq!(
                flag & (0x4000 | 0x8000),
                0,
                "a security guard must not collide with the HFS+ compression flags"
            );
        }
    }
}
