//! FFI bindings for libarchive
//!
//! Manual bindings to libarchive for ZIP, 7z, TAR, and other formats

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_longlong};

// Opaque types
pub type Archive = c_void;
pub type LibarchiveEntry = c_void;

// Archive open modes
pub const ARCHIVE_EOF: c_int = 1;
pub const ARCHIVE_OK: c_int = 0;
pub const ARCHIVE_RETRY: c_int = -10;
pub const ARCHIVE_WARN: c_int = -20;
pub const ARCHIVE_FAILED: c_int = -25;
pub const ARCHIVE_FATAL: c_int = -30;

// Extract flags
pub const ARCHIVE_EXTRACT_TIME: c_int = 0x0004;
pub const ARCHIVE_EXTRACT_PERM: c_int = 0x0002;
pub const ARCHIVE_EXTRACT_ACL: c_int = 0x0020;
pub const ARCHIVE_EXTRACT_FFLAGS: c_int = 0x0040;
pub const ARCHIVE_EXTRACT_OWNER: c_int = 0x0001;
pub const ARCHIVE_EXTRACT_NO_OVERWRITE: c_int = 0x0008;
pub const ARCHIVE_EXTRACT_SECURE_SYMLINKS: c_int = 0x4000;
pub const ARCHIVE_EXTRACT_SECURE_NODOTDOT: c_int = 0x8000;

// File type constants
pub const AE_IFMT: c_int = 0o170000;
pub const AE_IFREG: c_int = 0o100000;
pub const AE_IFDIR: c_int = 0o040000;
pub const AE_IFLNK: c_int = 0o120000;

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
    pub fn archive_entry_mode(entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_entry_filetype(entry: *mut LibarchiveEntry) -> c_int;
    pub fn archive_entry_hardlink(entry: *mut LibarchiveEntry) -> *const c_char;

    // Phase 1: Additional metadata functions
    pub fn archive_entry_birthtime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_atime(entry: *mut LibarchiveEntry) -> i64;
    pub fn archive_entry_is_encrypted(entry: *mut LibarchiveEntry) -> c_int;

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
    pub fn archive_write_data_block(
        archive: *mut Archive,
        buff: *const c_void,
        size: usize,
        offset: c_longlong,
    ) -> c_int;

    // Set extraction path
    pub fn archive_entry_set_pathname(entry: *mut LibarchiveEntry, pathname: *const c_char);
    pub fn archive_entry_update_pathname_utf8(entry: *mut LibarchiveEntry, pathname: *const c_char);

    // Archive writing (creation) functions
    pub fn archive_write_new() -> *mut Archive;
    pub fn archive_write_set_format_zip(archive: *mut Archive) -> c_int;
    pub fn archive_write_set_format_7zip(archive: *mut Archive) -> c_int;
    pub fn archive_write_set_format_pax_restricted(archive: *mut Archive) -> c_int; // Modern TAR
    pub fn archive_write_set_format_ustar(archive: *mut Archive) -> c_int; // Classic TAR
    pub fn archive_write_add_filter_gzip(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_bzip2(archive: *mut Archive) -> c_int;
    pub fn archive_write_add_filter_xz(archive: *mut Archive) -> c_int;
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
    pub fn archive_entry_set_filetype(entry: *mut LibarchiveEntry, filetype: c_int);
    pub fn archive_entry_set_perm(entry: *mut LibarchiveEntry, perm: c_int);
    pub fn archive_entry_set_mtime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_entry_set_atime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
    pub fn archive_entry_set_ctime(entry: *mut LibarchiveEntry, sec: i64, nsec: c_longlong);
}
