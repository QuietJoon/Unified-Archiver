//! UnRAR FFI bindings for RAR/RAR5 support
//!
//! Manual bindings to UnRAR library (v7.13.0)
//! Provides CRC32 and complete metadata extraction
//!
//! Licence: the UnRAR sources these bindings link against are vendored under
//! `src/ffi/native/unrar/`; their full freeware licence is
//! `src/ffi/native/unrar/license.txt` and is also reproduced in the
//! repository `LICENSE` file. All copyrights to RAR and the utility UnRAR are
//! exclusively owned by the author, Alexander Roshal. The clause that licence
//! requires to be carried in source code comments follows verbatim.

// UnRAR source code may be used in any software to handle
// RAR archives without limitations free of charge, but cannot be
// used to develop RAR (WinRAR) compatible archiver and to
// re-create RAR compression algorithm, which is proprietary.
// Distribution of modified UnRAR source code in separate form
// or as a part of other software is permitted, provided that
// full text of this paragraph, starting from "UnRAR source code"
// words, is included in license, or in documentation if license
// is not available, and in source code comments of resulting package.

use std::os::raw::{c_char, c_int, c_uint, c_void};

// Error codes
pub const ERAR_SUCCESS: c_int = 0;
pub const ERAR_END_ARCHIVE: c_int = 10;
pub const ERAR_NO_MEMORY: c_int = 11;
pub const ERAR_BAD_DATA: c_int = 12;
pub const ERAR_BAD_ARCHIVE: c_int = 13;
pub const ERAR_UNKNOWN_FORMAT: c_int = 14;
pub const ERAR_EOPEN: c_int = 15;
pub const ERAR_ECREATE: c_int = 16;
pub const ERAR_ECLOSE: c_int = 17;
pub const ERAR_EREAD: c_int = 18;
pub const ERAR_EWRITE: c_int = 19;
pub const ERAR_SMALL_BUF: c_int = 20;
pub const ERAR_UNKNOWN: c_int = 21;
pub const ERAR_MISSING_PASSWORD: c_int = 22;
pub const ERAR_BAD_PASSWORD: c_int = 24;

// Open modes
pub const RAR_OM_LIST: c_uint = 0;
pub const RAR_OM_EXTRACT: c_uint = 1;

// Process operations
pub const RAR_SKIP: c_int = 0;
pub const RAR_TEST: c_int = 1;
pub const RAR_EXTRACT: c_int = 2;

// Callback messages (`UNRARCALLBACK_MESSAGES` in dll.hpp). The C enum has
// no explicit discriminants, so values follow declaration order starting
// at 0. `UCM_PROCESSDATA` delivers one decoded data block (P1 = buffer
// address, P2 = block byte count) — verified against the vendored
// `src/ffi/native/unrar/dll.hpp` (R0080-0022).
pub const UCM_CHANGEVOLUME: c_uint = 0;
pub const UCM_PROCESSDATA: c_uint = 1;
pub const UCM_NEEDPASSWORD: c_uint = 2;
pub const UCM_CHANGEVOLUMEW: c_uint = 3;
pub const UCM_NEEDPASSWORDW: c_uint = 4;
pub const UCM_LARGEDICT: c_uint = 5;

// Header flags (file-level)
pub const RHDF_SPLITBEFORE: c_uint = 0x01;
pub const RHDF_SPLITAFTER: c_uint = 0x02;
pub const RHDF_ENCRYPTED: c_uint = 0x04;
pub const RHDF_SOLID: c_uint = 0x10;
pub const RHDF_DIRECTORY: c_uint = 0x20;

// Archive flags (from RAROpenArchiveDataEx.flags)
pub const ROADF_VOLUME: c_uint = 0x0001;
pub const ROADF_COMMENT: c_uint = 0x0002;
pub const ROADF_LOCK: c_uint = 0x0004;
pub const ROADF_SOLID: c_uint = 0x0008;
pub const ROADF_NEWNUMBERING: c_uint = 0x0010;
pub const ROADF_SIGNED: c_uint = 0x0020;
pub const ROADF_RECOVERY: c_uint = 0x0040;
pub const ROADF_ENCHEADERS: c_uint = 0x0080;
pub const ROADF_FIRSTVOLUME: c_uint = 0x0100;

// Hash types
pub const RAR_HASH_NONE: c_uint = 0;
pub const RAR_HASH_CRC32: c_uint = 1;
pub const RAR_HASH_BLAKE2: c_uint = 2;

/// Width of the UnRAR ABI's `wchar_t`.
///
/// UnRAR's wide strings are 2-byte UTF-16 **only** on Windows; macOS,
/// Linux, and every BSD use a 4-byte `wchar_t` (UTF-32). The predicate
/// must key on `windows` vs. not — the earlier `target_os = "macos"`
/// split silently narrowed the wide arrays to 2 bytes on Linux/BSD,
/// shifting every following packed field and inviting out-of-bounds
/// writes (R0080-0001).
#[cfg(windows)]
pub(crate) type RarWchar = u16;
#[cfg(not(windows))]
pub(crate) type RarWchar = c_uint;

/// Extended header data with CRC32 and full metadata
/// NOTE: `wchar_t` is 2 bytes (UTF-16) on Windows, 4 bytes (UTF-32) on
/// macOS/Darwin, Linux, and BSD.
///
/// Layout: the vendored UnRAR sources compile `dll.hpp` under
/// `#pragma pack(push, 1)`, so this binding must be `#[repr(C, packed)]`
/// — a plain `#[repr(C)]` declaration pads `cmt_buf` to pointer
/// alignment and skews every later field (`redir_type`, the high-res
/// timestamps) by 4 bytes (R0079-0008). Read multi-byte fields by value
/// (copy); references into packed fields are rejected (E0793).
#[repr(C, packed)]
#[derive(Debug)]
pub struct RARHeaderDataEx {
    pub arc_name: [c_char; 1024],
    pub arc_name_w: [RarWchar; 1024],

    pub file_name: [c_char; 1024],
    pub file_name_w: [RarWchar; 1024],

    pub flags: c_uint,
    pub pack_size: c_uint,
    pub pack_size_high: c_uint,
    pub unp_size: c_uint,
    pub unp_size_high: c_uint,
    pub host_os: c_uint,
    pub file_crc: c_uint, // ✅ CRC32 checksum
    pub file_time: c_uint,
    pub unp_ver: c_uint,
    pub method: c_uint,
    pub file_attr: c_uint,
    pub cmt_buf: *mut c_char,
    pub cmt_buf_size: c_uint,
    pub cmt_size: c_uint,
    pub cmt_state: c_uint,
    pub dict_size: c_uint,
    pub hash_type: c_uint, // RAR_HASH_CRC32, etc.
    pub hash: [c_char; 32],
    pub redir_type: c_uint,
    pub redir_name: *mut RarWchar,

    pub redir_name_size: c_uint,
    pub dir_target: c_uint,
    pub mtime_low: c_uint,
    pub mtime_high: c_uint,
    pub ctime_low: c_uint,
    pub ctime_high: c_uint,
    pub atime_low: c_uint,
    pub atime_high: c_uint,
    pub arc_name_ex: *mut RarWchar,

    pub arc_name_ex_size: c_uint,
    pub file_name_ex: *mut RarWchar,

    pub file_name_ex_size: c_uint,
    pub reserved: [c_uint; 982],
}

/// Open archive data
/// NOTE: `wchar_t` is 2 bytes (UTF-16) on Windows, 4 bytes (UTF-32) on
/// macOS/Darwin, Linux, and BSD.
///
/// Same `#pragma pack(push, 1)` constraint as [`RARHeaderDataEx`]
/// (R0079-0008): without `packed`, `cmt_buf_w` and later fields sit 4
/// bytes past where the C side reads them.
#[repr(C, packed)]
pub struct RAROpenArchiveDataEx {
    pub arc_name: *const c_char,
    pub arc_name_w: *const RarWchar,

    pub open_mode: c_uint,
    pub open_result: c_uint,
    pub cmt_buf: *mut c_char,
    pub cmt_buf_size: c_uint,
    pub cmt_size: c_uint,
    pub cmt_state: c_uint,
    pub flags: c_uint,
    pub callback: *mut c_void,
    pub user_data: isize,
    pub op_flags: c_uint,
    pub cmt_buf_w: *mut RarWchar,

    pub mark_of_the_web: *mut RarWchar,

    pub reserved: [c_uint; 23],
}

/// `sizeof(wchar_t)` for the platforms this binding distinguishes: 2 on
/// Windows (UTF-16), 4 everywhere else (UTF-32). The layout locks below
/// cross-check this value against the actual `RarWchar` element size.
const WCHAR_SIZE: usize = if cfg!(windows) { 2 } else { 4 };
const PTR_SIZE: usize = size_of::<*mut c_void>();
const UINT_SIZE: usize = size_of::<c_uint>();

// Compile-time layout locks (R0079-0008): the C side is compiled with
// `#pragma pack(push, 1)` (src/ffi/native/unrar/dll.hpp), so the struct
// sizes must equal the exact field-size sums with zero padding.
const _: () = {
    let header = 1024 // arc_name
        + 1024 * WCHAR_SIZE // arc_name_w
        + 1024 // file_name
        + 1024 * WCHAR_SIZE // file_name_w
        + 11 * UINT_SIZE // flags .. file_attr
        + PTR_SIZE // cmt_buf
        + 5 * UINT_SIZE // cmt_buf_size .. hash_type
        + 32 // hash
        + UINT_SIZE // redir_type
        + PTR_SIZE // redir_name
        + 8 * UINT_SIZE // redir_name_size .. atime_high
        + PTR_SIZE + UINT_SIZE // arc_name_ex(_size)
        + PTR_SIZE + UINT_SIZE // file_name_ex(_size)
        + 982 * UINT_SIZE; // reserved
    assert!(
        size_of::<RARHeaderDataEx>() == header,
        "RARHeaderDataEx must match UnRAR's pack(1) layout"
    );

    let open = 2 * PTR_SIZE // arc_name, arc_name_w
        + 2 * UINT_SIZE // open_mode, open_result
        + PTR_SIZE // cmt_buf
        + 4 * UINT_SIZE // cmt_buf_size, cmt_size, cmt_state, flags
        + PTR_SIZE // callback
        + size_of::<isize>() // user_data (LPARAM)
        + UINT_SIZE // op_flags
        + 2 * PTR_SIZE // cmt_buf_w, mark_of_the_web
        + 23 * UINT_SIZE; // reserved
    assert!(
        size_of::<RAROpenArchiveDataEx>() == open,
        "RAROpenArchiveDataEx must match UnRAR's pack(1) layout"
    );
};

/// UnRAR C callback (`UNRARCALLBACK` in dll.hpp):
/// `int (UINT msg, LPARAM UserData, LPARAM P1, LPARAM P2)`. `LPARAM` is the
/// platform `long`, mapped to `isize` to match `RAROpenArchiveDataEx.user_data`.
/// For `UCM_PROCESSDATA`, `p1` is the data buffer address and `p2` the block
/// size in bytes; returning `-1` aborts the running `RARProcessFile`.
pub type UnrarCallback =
    unsafe extern "C" fn(msg: c_uint, user_data: isize, p1: isize, p2: isize) -> c_int;

pub type RARHandle = *mut c_void;

#[link(name = "unrar")]
unsafe extern "C" {
    /// Open RAR archive
    pub fn RAROpenArchiveEx(archive_data: *mut RAROpenArchiveDataEx) -> RARHandle;

    /// Read next entry header (with CRC32)
    pub fn RARReadHeaderEx(handle: RARHandle, header_data: *mut RARHeaderDataEx) -> c_int;

    /// Process current entry (skip/test/extract)
    pub fn RARProcessFile(
        handle: RARHandle,
        operation: c_int,
        dest_path: *const c_char,
        dest_name: *const c_char,
    ) -> c_int;

    /// Process with Unicode paths. Currently uncalled; retained for
    /// header fidelity. Wide params use the target-correct `RarWchar`
    /// (`wchar_t *` on the C side), not Windows-only UTF-16 (R0080-0002).
    pub fn RARProcessFileW(
        handle: RARHandle,
        operation: c_int,
        dest_path: *const RarWchar,
        dest_name: *const RarWchar,
    ) -> c_int;

    /// Close archive
    pub fn RARCloseArchive(handle: RARHandle) -> c_int;

    /// Set password for encrypted archives
    pub fn RARSetPassword(handle: RARHandle, password: *const c_char);

    /// Register a progress/volume callback and its `LPARAM` user-data
    /// (`RARSetCallback` in dll.hpp). Passing `None` clears any callback
    /// previously installed on the handle (R0080-0022).
    pub fn RARSetCallback(handle: RARHandle, callback: Option<UnrarCallback>, user_data: isize);

    /// Get DLL version
    pub fn RARGetDllVersion() -> c_int;
}

impl Default for RARHeaderDataEx {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl Default for RAROpenArchiveDataEx {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    /// Pin the offsets the issue's C probes reported for the pack(1)
    /// layout (R0079-0008): `redir_type` and the high-resolution
    /// timestamps are the fields whose misreads broke link
    /// classification and ctime/atime.
    #[test]
    fn header_field_offsets_match_packed_c_layout() {
        let names = 2 * 1024 + 2 * 1024 * WCHAR_SIZE;
        assert_eq!(offset_of!(RARHeaderDataEx, flags), names);
        assert_eq!(offset_of!(RARHeaderDataEx, cmt_buf), names + 11 * UINT_SIZE);

        let redir_type = names + 11 * UINT_SIZE + PTR_SIZE + 5 * UINT_SIZE + 32;
        assert_eq!(offset_of!(RARHeaderDataEx, redir_type), redir_type);
        assert_eq!(
            offset_of!(RARHeaderDataEx, mtime_low),
            redir_type + UINT_SIZE + PTR_SIZE + 2 * UINT_SIZE
        );
    }
}
