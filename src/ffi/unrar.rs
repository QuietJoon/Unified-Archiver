//! UnRAR FFI bindings for RAR/RAR5 support
//!
//! Manual bindings to UnRAR library (v7.13.0)
//! Provides CRC32 and complete metadata extraction

use std::os::raw::{c_char, c_int, c_uint, c_void};

// Error codes
pub const ERAR_SUCCESS: c_int = 0;
pub const ERAR_END_ARCHIVE: c_int = 10;
pub const ERAR_NO_MEMORY: c_int = 11;
pub const ERAR_BAD_DATA: c_int = 12;
pub const ERAR_BAD_ARCHIVE: c_int = 13;
pub const ERAR_UNKNOWN_FORMAT: c_int = 14;
pub const ERAR_EOPEN: c_int = 15;
pub const ERAR_MISSING_PASSWORD: c_int = 22;
pub const ERAR_BAD_PASSWORD: c_int = 24;

// Open modes
pub const RAR_OM_LIST: c_uint = 0;
pub const RAR_OM_EXTRACT: c_uint = 1;

// Process operations
pub const RAR_SKIP: c_int = 0;
pub const RAR_TEST: c_int = 1;
pub const RAR_EXTRACT: c_int = 2;

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

/// Extended header data with CRC32 and full metadata
/// NOTE: wchar_t is 4 bytes on macOS/Darwin, 2 bytes on Windows
#[repr(C)]
#[derive(Debug)]
pub struct RARHeaderDataEx {
    pub arc_name: [c_char; 1024],
    #[cfg(target_os = "macos")]
    pub arc_name_w: [c_uint; 1024], // wchar_t is 4 bytes on macOS
    #[cfg(not(target_os = "macos"))]
    pub arc_name_w: [u16; 1024], // wchar_t is 2 bytes on other platforms

    pub file_name: [c_char; 1024],
    #[cfg(target_os = "macos")]
    pub file_name_w: [c_uint; 1024], // wchar_t is 4 bytes on macOS
    #[cfg(not(target_os = "macos"))]
    pub file_name_w: [u16; 1024], // wchar_t is 2 bytes on other platforms

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
    #[cfg(target_os = "macos")]
    pub redir_name: *mut c_uint, // wchar_t pointer is 8 bytes on macOS (but points to 4-byte wchar_t)
    #[cfg(not(target_os = "macos"))]
    pub redir_name: *mut u16, // wchar_t pointer on other platforms

    pub redir_name_size: c_uint,
    pub dir_target: c_uint,
    pub mtime_low: c_uint,
    pub mtime_high: c_uint,
    pub ctime_low: c_uint,
    pub ctime_high: c_uint,
    pub atime_low: c_uint,
    pub atime_high: c_uint,
    #[cfg(target_os = "macos")]
    pub arc_name_ex: *mut c_uint, // wchar_t pointer on macOS
    #[cfg(not(target_os = "macos"))]
    pub arc_name_ex: *mut u16, // wchar_t pointer on other platforms

    pub arc_name_ex_size: c_uint,
    #[cfg(target_os = "macos")]
    pub file_name_ex: *mut c_uint, // wchar_t pointer on macOS
    #[cfg(not(target_os = "macos"))]
    pub file_name_ex: *mut u16, // wchar_t pointer on other platforms

    pub file_name_ex_size: c_uint,
    pub reserved: [c_uint; 982],
}

/// Open archive data
/// NOTE: wchar_t is 4 bytes on macOS/Darwin, 2 bytes on Windows
#[repr(C)]
pub struct RAROpenArchiveDataEx {
    pub arc_name: *const c_char,
    #[cfg(target_os = "macos")]
    pub arc_name_w: *const c_uint, // wchar_t pointer on macOS
    #[cfg(not(target_os = "macos"))]
    pub arc_name_w: *const u16, // wchar_t pointer on other platforms

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
    #[cfg(target_os = "macos")]
    pub cmt_buf_w: *mut c_uint, // wchar_t pointer on macOS
    #[cfg(not(target_os = "macos"))]
    pub cmt_buf_w: *mut u16, // wchar_t pointer on other platforms

    #[cfg(target_os = "macos")]
    pub mark_of_the_web: *mut c_uint, // wchar_t pointer on macOS
    #[cfg(not(target_os = "macos"))]
    pub mark_of_the_web: *mut u16, // wchar_t pointer on other platforms

    pub reserved: [c_uint; 23],
}

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

    /// Process with Unicode paths
    pub fn RARProcessFileW(
        handle: RARHandle,
        operation: c_int,
        dest_path: *const u16,
        dest_name: *const u16,
    ) -> c_int;

    /// Close archive
    pub fn RARCloseArchive(handle: RARHandle) -> c_int;

    /// Set password for encrypted archives
    pub fn RARSetPassword(handle: RARHandle, password: *const c_char);

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
