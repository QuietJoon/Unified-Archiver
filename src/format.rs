//! Archive format detection and capabilities

use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Level of support for a format capability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Fully supported and tested
    Full,
    /// Partially supported (e.g., read works but write does not)
    Partial,
    /// Not supported
    None,
}

/// Per-operation capabilities for an archive format
///
/// Encodes read vs. write support separately, so callers can distinguish
/// "this format can decrypt but not encrypt" from "encryption is fully supported."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCapabilities {
    /// Encryption support for reading/decryption
    pub encryption_read: Support,
    /// Encryption support for writing/creation
    pub encryption_write: Support,
    /// Multipart archive reading
    pub multipart_read: Support,
    /// Multipart archive creation
    pub multipart_write: Support,
    /// Archive modification (add/remove/replace entries)
    pub modification: Support,
    /// Compression support
    pub compression: Support,
}

/// Supported archive formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    SevenZip,
    Zip,
    Rar,
    Rar5,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    Gzip,
    Bzip2,
    Xz,
    Iso,
}

impl ArchiveFormat {
    /// Detect archive format from file magic bytes
    pub fn detect(path: &Path) -> Result<Self> {
        let mut file =
            File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;

        // Read first 512 bytes for thorough magic byte detection (includes TAR header)
        let mut magic = [0u8; 512];
        let bytes_read = file
            .read(&mut magic)
            .map_err(|e| ArchiveError::io("read", path.to_path_buf(), e))?;

        if bytes_read < 4 {
            return Err(ArchiveError::format(
                None,
                "File too small to detect format",
            ));
        }

        // Try detection from bytes first
        if let Ok(format) = Self::detect_from_bytes(&magic[..bytes_read]) {
            // For compressed formats, check if it's a compressed TAR
            match format {
                ArchiveFormat::Gzip => {
                    // Check extension to distinguish .tar.gz from .gz
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_lowercase();
                        if name_str.ends_with(".tar.gz") || name_str.ends_with(".tgz") {
                            return Ok(ArchiveFormat::TarGzip);
                        }
                    }
                    return Ok(ArchiveFormat::Gzip);
                }
                ArchiveFormat::Bzip2 => {
                    // Check extension to distinguish .tar.bz2 from .bz2
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_lowercase();
                        if name_str.ends_with(".tar.bz2")
                            || name_str.ends_with(".tbz2")
                            || name_str.ends_with(".tb2")
                        {
                            return Ok(ArchiveFormat::TarBzip2);
                        }
                    }
                    return Ok(ArchiveFormat::Bzip2);
                }
                ArchiveFormat::Xz => {
                    // Check extension to distinguish .tar.xz from .xz
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_lowercase();
                        if name_str.ends_with(".tar.xz") || name_str.ends_with(".txz") {
                            return Ok(ArchiveFormat::TarXz);
                        }
                    }
                    return Ok(ArchiveFormat::Xz);
                }
                _ => return Ok(format),
            }
        }

        // Fallback to extension-based detection for formats without magic bytes
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            return match ext_str.as_str() {
                "tar" => Ok(ArchiveFormat::Tar),
                "iso" => Ok(ArchiveFormat::Iso),
                _ => Err(ArchiveError::format(None, "Unknown archive format")),
            };
        }

        Err(ArchiveError::format(None, "Unknown archive format"))
    }

    /// Detect archive format from raw bytes (magic bytes)
    ///
    /// Phase 7: Used for SFX detection to identify embedded archives at arbitrary offsets.
    /// Does not use file extension hints - only magic bytes.
    pub fn detect_from_bytes(magic: &[u8]) -> Result<Self> {
        if magic.len() < 4 {
            return Err(ArchiveError::format(
                None,
                "Buffer too small to detect format",
            ));
        }

        // RAR 5.0: "Rar!\x1A\x07\x01\x00"
        if magic.starts_with(b"Rar!\x1A\x07\x01\x00") {
            return Ok(ArchiveFormat::Rar5);
        }

        // RAR 4.x: "Rar!\x1A\x07\x00"
        if magic.starts_with(b"Rar!\x1A\x07\x00") {
            return Ok(ArchiveFormat::Rar);
        }

        // ZIP: "PK\x03\x04" or "PK\x05\x06" or "PK\x07\x08"
        if magic.starts_with(b"PK\x03\x04")
            || magic.starts_with(b"PK\x05\x06")
            || magic.starts_with(b"PK\x07\x08")
        {
            return Ok(ArchiveFormat::Zip);
        }

        // 7z: "7z\xBC\xAF\x27\x1C"
        if magic.starts_with(b"7z\xBC\xAF\x27\x1C") {
            return Ok(ArchiveFormat::SevenZip);
        }

        // GZIP: 0x1F 0x8B
        if magic.starts_with(&[0x1F, 0x8B]) {
            return Ok(ArchiveFormat::Gzip);
        }

        // BZIP2: "BZ"
        if magic.starts_with(b"BZh") {
            return Ok(ArchiveFormat::Bzip2);
        }

        // XZ: 0xFD 0x37 0x7A 0x58 0x5A 0x00
        if magic.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
            return Ok(ArchiveFormat::Xz);
        }

        // TAR detection from bytes requires checking offset 257 for "ustar"
        if magic.len() >= 262 && &magic[257..262] == b"ustar" {
            return Ok(ArchiveFormat::Tar);
        }

        Err(ArchiveError::format(
            None,
            "Unknown archive format from magic bytes",
        ))
    }

    /// Per-operation capability report for this format
    pub fn capabilities(&self) -> FormatCapabilities {
        match self {
            ArchiveFormat::Zip => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                multipart_read: Support::Partial, // libarchive; needs first volume
                multipart_write: Support::None,
                modification: Support::Full,
                compression: Support::Full,
            },
            ArchiveFormat::SevenZip => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::Full,
                compression: Support::Full,
            },
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                multipart_read: Support::Full,
                multipart_write: Support::None,
                modification: Support::None,
                compression: Support::Full,
            },
            ArchiveFormat::Tar => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression: Support::None,
            },
            ArchiveFormat::TarGzip
            | ArchiveFormat::TarBzip2
            | ArchiveFormat::TarXz
            | ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression: Support::Full,
            },
            ArchiveFormat::Iso => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression: Support::None,
            },
        }
    }

    /// Check if format supports compression
    pub fn supports_compression(&self) -> bool {
        self.capabilities().compression != Support::None
    }

    /// Check if format supports encryption (read or write)
    pub fn supports_encryption(&self) -> bool {
        let caps = self.capabilities();
        caps.encryption_read != Support::None || caps.encryption_write != Support::None
    }

    /// Check if format supports multi-part archives (read or write)
    pub fn supports_multipart(&self) -> bool {
        let caps = self.capabilities();
        caps.multipart_read != Support::None || caps.multipart_write != Support::None
    }

    /// Check if format supports modification
    pub fn can_modify(&self) -> bool {
        self.capabilities().modification != Support::None
    }

    /// Get file extensions for this format
    pub fn extensions(&self) -> &[&str] {
        match self {
            ArchiveFormat::SevenZip => &["7z"],
            ArchiveFormat::Zip => &["zip"],
            ArchiveFormat::Rar => &["rar"],
            ArchiveFormat::Rar5 => &["rar"],
            ArchiveFormat::Tar => &["tar"],
            ArchiveFormat::TarGzip => &["tar.gz", "tgz"],
            ArchiveFormat::TarBzip2 => &["tar.bz2", "tbz2"],
            ArchiveFormat::TarXz => &["tar.xz", "txz"],
            ArchiveFormat::Gzip => &["gz"],
            ArchiveFormat::Bzip2 => &["bz2"],
            ArchiveFormat::Xz => &["xz"],
            ArchiveFormat::Iso => &["iso"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// Helper: write bytes to a temp file and return the path
    fn temp_file_with_bytes(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    // ── detect_from_bytes: magic byte detection for each format ──

    #[test]
    fn test_detect_rar5_magic() {
        let magic = b"Rar!\x1A\x07\x01\x00extra_data_here";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Rar5
        );
    }

    #[test]
    fn test_detect_rar4_magic() {
        let magic = b"Rar!\x1A\x07\x00extra_data";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Rar
        );
    }

    #[test]
    fn test_detect_zip_local_file_header() {
        let magic = b"PK\x03\x04rest_of_header";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_zip_end_of_central_dir() {
        let magic = b"PK\x05\x06rest_of_data";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_zip_data_descriptor() {
        let magic = b"PK\x07\x08rest_of_data";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_7z_magic() {
        let magic = b"7z\xBC\xAF\x27\x1Cmore_data";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::SevenZip
        );
    }

    #[test]
    fn test_detect_gzip_magic() {
        let magic: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Gzip
        );
    }

    #[test]
    fn test_detect_bzip2_magic() {
        let magic = b"BZh91AY&SY";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Bzip2
        );
    }

    #[test]
    fn test_detect_xz_magic() {
        let magic: &[u8] = &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Xz
        );
    }

    #[test]
    fn test_detect_tar_magic() {
        // TAR magic "ustar" at offset 257
        let mut buf = vec![0u8; 512];
        buf[257..262].copy_from_slice(b"ustar");
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&buf).unwrap(),
            ArchiveFormat::Tar
        );
    }

    // ── detect_from_bytes: error cases ──

    #[test]
    fn test_detect_from_bytes_too_small() {
        let magic: &[u8] = &[0x1F, 0x8B, 0x08]; // Only 3 bytes
        assert!(ArchiveFormat::detect_from_bytes(magic).is_err());
    }

    #[test]
    fn test_detect_from_bytes_unknown_magic() {
        let magic = b"NOT_AN_ARCHIVE_FORMAT";
        assert!(ArchiveFormat::detect_from_bytes(magic).is_err());
    }

    #[test]
    fn test_detect_from_bytes_all_zeros() {
        let magic = &[0u8; 512];
        assert!(ArchiveFormat::detect_from_bytes(magic).is_err());
    }

    // ── detect_from_bytes: priority / ordering ──

    #[test]
    fn test_rar5_detected_before_rar4() {
        // RAR5 magic is a superset prefix of RAR4 — must check RAR5 first
        let rar5_magic = b"Rar!\x1A\x07\x01\x00";
        assert_eq!(
            ArchiveFormat::detect_from_bytes(rar5_magic).unwrap(),
            ArchiveFormat::Rar5
        );
    }

    // ── detect (file-based): compressed TAR disambiguation ──

    #[test]
    fn test_detect_tar_gz_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar.gz", gzip_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarGzip
        );
    }

    #[test]
    fn test_detect_tgz_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tgz", gzip_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarGzip
        );
    }

    #[test]
    fn test_detect_plain_gzip_not_tar() {
        let tmp = tempfile::tempdir().unwrap();
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.gz", gzip_bytes);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Gzip);
    }

    #[test]
    fn test_detect_tar_bz2_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let bz2_bytes = b"BZh91AY&SYextra";
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar.bz2", bz2_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarBzip2
        );
    }

    #[test]
    fn test_detect_tbz2_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let bz2_bytes = b"BZh91AY&SYextra";
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tbz2", bz2_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarBzip2
        );
    }

    #[test]
    fn test_detect_tar_xz_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let xz_bytes: &[u8] = &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar.xz", xz_bytes);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::TarXz);
    }

    #[test]
    fn test_detect_txz_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let xz_bytes: &[u8] = &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.txz", xz_bytes);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::TarXz);
    }

    // ── detect: extension-only fallback ──

    #[test]
    fn test_detect_tar_by_extension_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        // A TAR file without ustar magic (e.g., old-style TAR or tiny file)
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar", &[0u8; 512]);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Tar);
    }

    #[test]
    fn test_detect_iso_by_extension_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_file_with_bytes(tmp.path(), "test_detect.iso", &[0u8; 512]);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Iso);
    }

    // ── detect: error cases ──

    #[test]
    fn test_detect_nonexistent_file() {
        let result = ArchiveFormat::detect(Path::new("/nonexistent/archive.zip"));
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_file_too_small() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_file_with_bytes(tmp.path(), "tiny.zip", &[0x50]); // only 1 byte
        let result = ArchiveFormat::detect(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_unknown_format_unknown_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_file_with_bytes(tmp.path(), "test_detect.xyz", &[0u8; 512]);
        let result = ArchiveFormat::detect(&path);
        assert!(result.is_err());
    }

    // ── Format capabilities ──

    #[test]
    fn test_supports_compression() {
        assert!(ArchiveFormat::Zip.supports_compression());
        assert!(ArchiveFormat::SevenZip.supports_compression());
        assert!(ArchiveFormat::Rar.supports_compression());
        assert!(ArchiveFormat::Rar5.supports_compression());
        assert!(ArchiveFormat::Gzip.supports_compression());
        assert!(ArchiveFormat::Bzip2.supports_compression());
        assert!(ArchiveFormat::Xz.supports_compression());
        assert!(ArchiveFormat::TarGzip.supports_compression());

        // TAR and ISO do NOT support compression natively
        assert!(!ArchiveFormat::Tar.supports_compression());
        assert!(!ArchiveFormat::Iso.supports_compression());
    }

    #[test]
    fn test_supports_encryption() {
        assert!(ArchiveFormat::Zip.supports_encryption());
        assert!(ArchiveFormat::SevenZip.supports_encryption());
        assert!(ArchiveFormat::Rar.supports_encryption());
        assert!(ArchiveFormat::Rar5.supports_encryption());

        assert!(!ArchiveFormat::Tar.supports_encryption());
        assert!(!ArchiveFormat::TarGzip.supports_encryption());
        assert!(!ArchiveFormat::Gzip.supports_encryption());
        assert!(!ArchiveFormat::Iso.supports_encryption());
    }

    #[test]
    fn test_supports_multipart() {
        assert!(ArchiveFormat::Zip.supports_multipart());
        assert!(ArchiveFormat::Rar.supports_multipart());
        assert!(ArchiveFormat::Rar5.supports_multipart());

        // 7z multipart reading is not implemented in the sevenz-rust2 backend
        assert!(!ArchiveFormat::SevenZip.supports_multipart());
        assert!(!ArchiveFormat::Tar.supports_multipart());
        assert!(!ArchiveFormat::TarGzip.supports_multipart());
        assert!(!ArchiveFormat::Iso.supports_multipart());
    }

    #[test]
    fn test_can_modify() {
        assert!(ArchiveFormat::Zip.can_modify());
        assert!(ArchiveFormat::SevenZip.can_modify());

        assert!(!ArchiveFormat::Rar.can_modify());
        assert!(!ArchiveFormat::Rar5.can_modify());
        assert!(!ArchiveFormat::Tar.can_modify());
        assert!(!ArchiveFormat::Iso.can_modify());
    }

    // ── Extensions ──

    #[test]
    fn test_extensions_not_empty() {
        let all_formats = [
            ArchiveFormat::SevenZip,
            ArchiveFormat::Zip,
            ArchiveFormat::Rar,
            ArchiveFormat::Rar5,
            ArchiveFormat::Tar,
            ArchiveFormat::TarGzip,
            ArchiveFormat::TarBzip2,
            ArchiveFormat::TarXz,
            ArchiveFormat::Gzip,
            ArchiveFormat::Bzip2,
            ArchiveFormat::Xz,
            ArchiveFormat::Iso,
        ];
        for fmt in &all_formats {
            assert!(
                !fmt.extensions().is_empty(),
                "{:?} should have at least one extension",
                fmt
            );
        }
    }

    #[test]
    fn test_rar_and_rar5_share_extension() {
        assert_eq!(ArchiveFormat::Rar.extensions(), &["rar"]);
        assert_eq!(ArchiveFormat::Rar5.extensions(), &["rar"]);
    }

    // ── Trait impls ──

    #[test]
    fn test_format_clone_and_copy() {
        let fmt = ArchiveFormat::Zip;
        let copied = fmt; // Copy
        assert_eq!(fmt, copied);
    }

    #[test]
    fn test_format_debug() {
        let debug_str = format!("{:?}", ArchiveFormat::SevenZip);
        assert_eq!(debug_str, "SevenZip");
    }

    #[test]
    fn test_format_equality() {
        assert_eq!(ArchiveFormat::Zip, ArchiveFormat::Zip);
        assert_ne!(ArchiveFormat::Zip, ArchiveFormat::Rar);
        assert_ne!(ArchiveFormat::Rar, ArchiveFormat::Rar5);
    }

    // ── FormatCapabilities ──

    #[test]
    fn test_zip_capabilities() {
        let caps = ArchiveFormat::Zip.capabilities();
        assert_eq!(caps.encryption_read, Support::Full);
        // Per AD 0027: encrypted archive creation is rejected; read-only support.
        assert_eq!(caps.encryption_write, Support::None);
        assert_eq!(caps.multipart_read, Support::Partial);
        assert_eq!(caps.multipart_write, Support::None);
        assert_eq!(caps.modification, Support::Full);
        assert_eq!(caps.compression, Support::Full);
    }

    #[test]
    fn test_create_with_password_rejected_for_zip() {
        use crate::options::CompressionOptions;
        use secstr::SecStr;
        let tmp = std::env::temp_dir().join("ua_encrypted_create_rejected.zip");
        let _ = std::fs::remove_file(&tmp);
        let opts = CompressionOptions {
            format: ArchiveFormat::Zip,
            password: Some(SecStr::from("secret")),
            ..Default::default()
        };
        let result = crate::Archive::create(&tmp, opts);
        assert!(result.is_err(), "expected an error, got Ok");
        let err = result.err().unwrap();
        assert!(
            matches!(err, crate::ArchiveError::OperationBlocked { .. }),
            "expected OperationBlocked, got a different error variant"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_sevenz_capabilities() {
        let caps = ArchiveFormat::SevenZip.capabilities();
        assert_eq!(caps.encryption_read, Support::Full);
        assert_eq!(caps.encryption_write, Support::None);
        assert_eq!(caps.modification, Support::Full);
    }

    #[test]
    fn test_rar_read_only_capabilities() {
        for fmt in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
            let caps = fmt.capabilities();
            assert_eq!(caps.encryption_read, Support::Full);
            assert_eq!(caps.encryption_write, Support::None);
            assert_eq!(caps.multipart_read, Support::Full);
            assert_eq!(caps.modification, Support::None);
        }
    }

    #[test]
    fn test_tar_no_capabilities() {
        let caps = ArchiveFormat::Tar.capabilities();
        assert_eq!(caps.encryption_read, Support::None);
        assert_eq!(caps.compression, Support::None);
        assert_eq!(caps.modification, Support::None);
    }

    #[test]
    fn test_compressed_tar_only_compression() {
        for fmt in [
            ArchiveFormat::TarGzip,
            ArchiveFormat::TarBzip2,
            ArchiveFormat::TarXz,
        ] {
            let caps = fmt.capabilities();
            assert_eq!(caps.compression, Support::Full);
            assert_eq!(caps.encryption_read, Support::None);
            assert_eq!(caps.modification, Support::None);
        }
    }

    #[test]
    fn test_boolean_methods_delegate_to_capabilities() {
        // Verify boolean methods stay in sync with capabilities()
        let all_formats = [
            ArchiveFormat::SevenZip,
            ArchiveFormat::Zip,
            ArchiveFormat::Rar,
            ArchiveFormat::Rar5,
            ArchiveFormat::Tar,
            ArchiveFormat::TarGzip,
            ArchiveFormat::TarBzip2,
            ArchiveFormat::TarXz,
            ArchiveFormat::Gzip,
            ArchiveFormat::Bzip2,
            ArchiveFormat::Xz,
            ArchiveFormat::Iso,
        ];
        for fmt in &all_formats {
            let caps = fmt.capabilities();
            assert_eq!(
                fmt.supports_compression(),
                caps.compression != Support::None,
                "{:?} compression mismatch",
                fmt
            );
            assert_eq!(
                fmt.supports_encryption(),
                caps.encryption_read != Support::None || caps.encryption_write != Support::None,
                "{:?} encryption mismatch",
                fmt
            );
            assert_eq!(
                fmt.can_modify(),
                caps.modification != Support::None,
                "{:?} modification mismatch",
                fmt
            );
        }
    }

    #[test]
    fn test_support_enum_traits() {
        let s = Support::Full;
        let copied = s; // Copy
        assert_eq!(s, copied);
        assert_ne!(Support::Full, Support::None);
        assert!(!format!("{:?}", Support::Partial).is_empty());
    }
}
