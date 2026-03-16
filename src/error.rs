//! Unified error handling for archive operations

use std::path::PathBuf;

/// Unified error type for all archive operations
#[derive(Debug)]
pub enum ArchiveError {
    /// I/O error during file operations
    Io {
        operation: String,
        path: PathBuf,
        source: std::io::Error,
    },

    /// Archive format error
    Format {
        format: Option<crate::ArchiveFormat>,
        message: String,
    },

    /// Archive corruption detected
    Corruption { path: String, details: String },

    /// Password authentication error
    Password { message: String },

    /// Operation not supported
    Unsupported {
        operation: String,
        format: crate::ArchiveFormat,
        details: Option<String>,
    },

    /// Codec not available (requires installation)
    CodecUnavailable {
        codec: String,
        format: crate::ArchiveFormat,
        install_instructions: String,
    },

    /// Operation not yet implemented (format-agnostic)
    UnsupportedOperation { operation: String, reason: String },

    /// Invalid path
    InvalidPath { path: String, reason: String },
}

/// String constants for operation names used in error construction.
/// Prevents typos and enables refactoring of operation names.
pub(crate) mod ops {
    pub const EXTRACT_ALL: &str = "extract_all";
    pub const EXTRACT_FILE: &str = "extract_file";
    pub const EXTRACT_TO_MEMORY: &str = "extract_to_memory";
    pub const EXTRACT_TO_STREAM: &str = "extract_to_stream";
    pub const EXTRACT_FILTERED: &str = "extract_filtered";
    pub const EXTRACT_FILES: &str = "extract_files";
    pub const EXTRACT_BY_IDS: &str = "extract_by_ids";
    pub const LIST_FILES: &str = "list_files";
    pub const LIST_FILES_FOR_LIMITS: &str = "list_files_for_limits";
    pub const VALIDATE_INTEGRITY: &str = "validate_integrity";
    pub const MODIFY: &str = "modify";
    pub const ADD_ENTRY: &str = "add_entry";
    pub const REMOVE_ENTRY: &str = "remove_entry";
    pub const COMMIT_CHANGES: &str = "commit_changes";
    pub const ADD_DIRECTORY_ENTRY: &str = "add_directory_entry";
    pub const CLEAR_ENTRIES: &str = "clear_entries";
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ArchiveError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveError::Io {
                operation,
                path,
                source,
            } => {
                write!(
                    f,
                    "I/O error during {}: {} ({})",
                    operation,
                    path.display(),
                    source
                )
            }
            ArchiveError::Format { format, message } => {
                if let Some(fmt) = format {
                    write!(f, "{:?} format error: {}", fmt, message)
                } else {
                    write!(f, "Archive format error: {}", message)
                }
            }
            ArchiveError::Corruption { path, details } => {
                write!(f, "Corrupted entry '{}': {}", path, details)
            }
            ArchiveError::Password { message } => {
                write!(f, "Password error: {}", message)
            }
            ArchiveError::Unsupported {
                operation,
                format,
                details,
            } => {
                if let Some(d) = details {
                    write!(
                        f,
                        "Operation '{}' not supported for {:?} format: {}",
                        operation, format, d
                    )
                } else {
                    write!(
                        f,
                        "Operation '{}' not supported for {:?} format",
                        operation, format
                    )
                }
            }
            ArchiveError::CodecUnavailable {
                codec,
                format,
                install_instructions,
            } => {
                write!(
                    f,
                    "{} codec not available for {:?} format. {}",
                    codec, format, install_instructions
                )
            }
            ArchiveError::UnsupportedOperation { operation, reason } => {
                write!(f, "Operation '{}' not implemented: {}", operation, reason)
            }
            ArchiveError::InvalidPath { path, reason } => {
                write!(f, "Invalid path '{}': {}", path, reason)
            }
        }
    }
}

impl ArchiveError {
    /// Create a format error
    pub fn format(format: Option<crate::ArchiveFormat>, message: impl Into<String>) -> Self {
        Self::Format {
            format,
            message: message.into(),
        }
    }

    /// Create an I/O error
    pub fn io(
        operation: impl Into<String>,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            operation: operation.into(),
            path: path.into(),
            source,
        }
    }

    /// Create a corruption error
    pub fn corruption(path: impl Into<String>, details: impl Into<String>) -> Self {
        Self::Corruption {
            path: path.into(),
            details: details.into(),
        }
    }

    /// Create a password error
    pub fn password(message: impl Into<String>) -> Self {
        Self::Password {
            message: message.into(),
        }
    }

    /// Create an invalid path error
    pub fn invalid_path(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create an unsupported operation error
    pub fn unsupported(
        operation: impl Into<String>,
        format: crate::ArchiveFormat,
        details: Option<impl Into<String>>,
    ) -> Self {
        Self::Unsupported {
            operation: operation.into(),
            format,
            details: details.map(|d| d.into()),
        }
    }

    /// Create a codec unavailable error with platform-specific installation instructions
    pub fn codec_unavailable(codec: impl Into<String>, format: crate::ArchiveFormat) -> Self {
        let codec_str = codec.into();
        let install_instructions = Self::get_codec_install_instructions(&codec_str);

        Self::CodecUnavailable {
            codec: codec_str,
            format,
            install_instructions,
        }
    }

    /// Get platform-specific installation instructions for a codec
    fn get_codec_install_instructions(codec: &str) -> String {
        // Detect platform
        #[cfg(target_os = "macos")]
        let platform = "macos";
        #[cfg(target_os = "linux")]
        let platform = "linux";
        #[cfg(target_os = "windows")]
        let platform = "windows";
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let platform = "unknown";

        match (codec, platform) {
            // LZMA/LZMA2
            ("LZMA" | "LZMA2", "macos") => {
                "Install p7zip: brew install p7zip".to_string()
            }
            ("LZMA" | "LZMA2", "linux") => {
                "Install p7zip: sudo apt-get install p7zip-full (Debian/Ubuntu) or sudo yum install p7zip (RHEL/CentOS)".to_string()
            }
            ("LZMA" | "LZMA2", "windows") => {
                "Install 7-Zip from https://www.7-zip.org/".to_string()
            }

            // BZIP2
            ("BZIP2", "macos") => {
                "Install bzip2: brew install bzip2".to_string()
            }
            ("BZIP2", "linux") => {
                "Install bzip2: sudo apt-get install bzip2 (Debian/Ubuntu) or sudo yum install bzip2 (RHEL/CentOS)".to_string()
            }
            ("BZIP2", "windows") => {
                "Install bzip2 from http://gnuwin32.sourceforge.net/packages/bzip2.htm".to_string()
            }

            // XZ
            ("XZ", "macos") => {
                "Install xz: brew install xz".to_string()
            }
            ("XZ", "linux") => {
                "Install xz: sudo apt-get install xz-utils (Debian/Ubuntu) or sudo yum install xz (RHEL/CentOS)".to_string()
            }
            ("XZ", "windows") => {
                "Install XZ Utils from https://tukaani.org/xz/".to_string()
            }

            // RAR/RAR5 (read-only, uses UnRAR SDK via FFI)
            ("RAR" | "RAR5", _) => {
                "RAR/RAR5 support requires UnRAR library (already linked via FFI). If extraction fails, ensure UnRAR SDK is properly compiled.".to_string()
            }

            // Generic fallback
            _ => {
                format!(
                    "Install {} codec for your platform. See libarchive documentation: https://libarchive.org/",
                    codec
                )
            }
        }
    }

    /// Create error for attempting to read from a write-only archive
    pub fn write_mode_only(operation: impl Into<String>) -> Self {
        Self::UnsupportedOperation {
            operation: operation.into(),
            reason: "Cannot extract from an archive in Write mode".to_string(),
        }
    }

    /// Create error for read-only backends that don't support creation
    pub fn read_only_backend(operation: impl Into<String>) -> Self {
        Self::UnsupportedOperation {
            operation: operation.into(),
            reason: "This backend does not support archive creation".to_string(),
        }
    }
}

/// Warning emitted during archive operations (FR-022)
///
/// Warnings indicate non-fatal conditions that may require user attention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveWarning {
    /// Symbolic link skipped during operation
    ///
    /// Cross-platform symlink handling is not reliably supported across all
    /// archive formats and operating systems (FR-022). Symlinks are skipped
    /// with this warning to ensure consistent behavior.
    SkippedSymlink {
        /// Path of the skipped symlink
        path: String,
        /// Target of the symlink (if available)
        target: Option<String>,
    },

    /// Hard link skipped during operation
    ///
    /// Hard links have limited cross-platform support (FR-022).
    SkippedHardLink {
        /// Path of the skipped hard link
        path: String,
    },
}

impl std::fmt::Display for ArchiveWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveWarning::SkippedSymlink { path, target } => {
                if let Some(t) = target {
                    write!(
                        f,
                        "Skipped symbolic link '{}' -> '{}' (FR-022: cross-platform symlink support not reliable)",
                        path, t
                    )
                } else {
                    write!(
                        f,
                        "Skipped symbolic link '{}' (FR-022: cross-platform symlink support not reliable)",
                        path
                    )
                }
            }
            ArchiveWarning::SkippedHardLink { path } => {
                write!(
                    f,
                    "Skipped hard link '{}' (FR-022: limited cross-platform support)",
                    path
                )
            }
        }
    }
}

/// Result type that includes warnings
pub struct ResultWithWarnings<T> {
    /// Operation result
    pub value: T,
    /// Warnings emitted during operation
    pub warnings: Vec<ArchiveWarning>,
}

impl<T> ResultWithWarnings<T> {
    /// Create a result with no warnings
    pub fn ok(value: T) -> Self {
        Self {
            value,
            warnings: Vec::new(),
        }
    }

    /// Create a result with warnings
    pub fn with_warnings(value: T, warnings: Vec<ArchiveWarning>) -> Self {
        Self { value, warnings }
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: ArchiveWarning) {
        self.warnings.push(warning);
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchiveFormat;
    use std::io;

    // ── ArchiveError variant constructors ──

    #[test]
    fn test_io_error_construction() {
        let source = io::Error::new(io::ErrorKind::NotFound, "file gone");
        let err = ArchiveError::io("open", "/tmp/test.zip", source);
        match &err {
            ArchiveError::Io {
                operation,
                path,
                source,
            } => {
                assert_eq!(operation, "open");
                assert_eq!(path, &PathBuf::from("/tmp/test.zip"));
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("Expected Io variant, got {:?}", other),
        }
    }

    #[test]
    fn test_format_error_with_format() {
        let err = ArchiveError::format(Some(ArchiveFormat::Zip), "bad header");
        match &err {
            ArchiveError::Format { format, message } => {
                assert_eq!(*format, Some(ArchiveFormat::Zip));
                assert_eq!(message, "bad header");
            }
            other => panic!("Expected Format variant, got {:?}", other),
        }
    }

    #[test]
    fn test_format_error_without_format() {
        let err = ArchiveError::format(None, "unknown");
        match &err {
            ArchiveError::Format { format, message } => {
                assert!(format.is_none());
                assert_eq!(message, "unknown");
            }
            other => panic!("Expected Format variant, got {:?}", other),
        }
    }

    #[test]
    fn test_corruption_error_construction() {
        let err = ArchiveError::corruption("data/file.txt", "CRC mismatch");
        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "data/file.txt");
                assert_eq!(details, "CRC mismatch");
            }
            other => panic!("Expected Corruption variant, got {:?}", other),
        }
    }

    #[test]
    fn test_password_error_construction() {
        let err = ArchiveError::password("wrong password");
        match &err {
            ArchiveError::Password { message } => {
                assert_eq!(message, "wrong password");
            }
            other => panic!("Expected Password variant, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_path_error_construction() {
        let err = ArchiveError::invalid_path("../../etc/passwd", "path traversal");
        match &err {
            ArchiveError::InvalidPath { path, reason } => {
                assert_eq!(path, "../../etc/passwd");
                assert_eq!(reason, "path traversal");
            }
            other => panic!("Expected InvalidPath variant, got {:?}", other),
        }
    }

    #[test]
    fn test_unsupported_error_with_details() {
        let err = ArchiveError::unsupported("create", ArchiveFormat::Rar, Some("read-only format"));
        match &err {
            ArchiveError::Unsupported {
                operation,
                format,
                details,
            } => {
                assert_eq!(operation, "create");
                assert_eq!(*format, ArchiveFormat::Rar);
                assert_eq!(details.as_deref(), Some("read-only format"));
            }
            other => panic!("Expected Unsupported variant, got {:?}", other),
        }
    }

    #[test]
    fn test_unsupported_error_without_details() {
        let err = ArchiveError::unsupported("modify", ArchiveFormat::Tar, None::<&str>);
        match &err {
            ArchiveError::Unsupported { details, .. } => {
                assert!(details.is_none());
            }
            other => panic!("Expected Unsupported variant, got {:?}", other),
        }
    }

    #[test]
    fn test_codec_unavailable_construction() {
        let err = ArchiveError::codec_unavailable("LZMA", ArchiveFormat::SevenZip);
        match &err {
            ArchiveError::CodecUnavailable {
                codec,
                format,
                install_instructions,
            } => {
                assert_eq!(codec, "LZMA");
                assert_eq!(*format, ArchiveFormat::SevenZip);
                assert!(!install_instructions.is_empty());
            }
            other => panic!("Expected CodecUnavailable variant, got {:?}", other),
        }
    }

    #[test]
    fn test_write_mode_only_error() {
        let err = ArchiveError::write_mode_only("extract_all");
        match &err {
            ArchiveError::UnsupportedOperation { operation, reason } => {
                assert_eq!(operation, "extract_all");
                assert!(reason.contains("Write mode"));
            }
            other => panic!("Expected UnsupportedOperation variant, got {:?}", other),
        }
    }

    #[test]
    fn test_read_only_backend_error() {
        let err = ArchiveError::read_only_backend("create");
        match &err {
            ArchiveError::UnsupportedOperation { operation, reason } => {
                assert_eq!(operation, "create");
                assert!(reason.contains("creation"));
            }
            other => panic!("Expected UnsupportedOperation variant, got {:?}", other),
        }
    }

    // ── Display trait ──

    #[test]
    fn test_display_io_error() {
        let source = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let err = ArchiveError::io("read", "/secret/file.zip", source);
        let msg = err.to_string();
        assert!(msg.contains("I/O error during read"));
        assert!(msg.contains("/secret/file.zip"));
        assert!(msg.contains("access denied"));
    }

    #[test]
    fn test_display_format_error_with_format() {
        let err = ArchiveError::format(Some(ArchiveFormat::Rar5), "truncated header");
        let msg = err.to_string();
        assert!(msg.contains("Rar5"));
        assert!(msg.contains("format error"));
        assert!(msg.contains("truncated header"));
    }

    #[test]
    fn test_display_format_error_without_format() {
        let err = ArchiveError::format(None, "unrecognized magic bytes");
        let msg = err.to_string();
        assert!(msg.contains("Archive format error"));
        assert!(msg.contains("unrecognized magic bytes"));
    }

    #[test]
    fn test_display_corruption_error() {
        let err = ArchiveError::corruption("inner/data.bin", "expected CRC 0xDEAD got 0xBEEF");
        let msg = err.to_string();
        assert!(msg.contains("Corrupted entry 'inner/data.bin'"));
        assert!(msg.contains("0xDEAD"));
    }

    #[test]
    fn test_display_password_error() {
        let err = ArchiveError::password("incorrect password or encrypted headers");
        let msg = err.to_string();
        assert!(msg.contains("Password error"));
        assert!(msg.contains("incorrect password"));
    }

    #[test]
    fn test_display_unsupported_with_details() {
        let err = ArchiveError::unsupported("create", ArchiveFormat::Rar, Some("read-only"));
        let msg = err.to_string();
        assert!(msg.contains("'create'"));
        assert!(msg.contains("Rar"));
        assert!(msg.contains("read-only"));
    }

    #[test]
    fn test_display_unsupported_without_details() {
        let err = ArchiveError::unsupported("split", ArchiveFormat::Tar, None::<&str>);
        let msg = err.to_string();
        assert!(msg.contains("'split'"));
        assert!(msg.contains("Tar"));
        // Should NOT contain a trailing colon or extra details
        assert!(
            !msg.contains(": "),
            "Should not have trailing details separator when details is None (got: {})",
            msg
        );
    }

    #[test]
    fn test_display_codec_unavailable() {
        let err = ArchiveError::codec_unavailable("XZ", ArchiveFormat::TarXz);
        let msg = err.to_string();
        assert!(msg.contains("XZ codec not available"));
        assert!(msg.contains("TarXz"));
    }

    #[test]
    fn test_display_unsupported_operation() {
        let err = ArchiveError::write_mode_only("list_files");
        let msg = err.to_string();
        assert!(msg.contains("'list_files'"));
        assert!(msg.contains("not implemented"));
    }

    #[test]
    fn test_display_invalid_path() {
        let err = ArchiveError::invalid_path("/etc/shadow", "absolute path disallowed");
        let msg = err.to_string();
        assert!(msg.contains("Invalid path '/etc/shadow'"));
        assert!(msg.contains("absolute path disallowed"));
    }

    // ── std::error::Error trait ──

    #[test]
    fn test_error_source_io_variant() {
        let source = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke");
        let err = ArchiveError::io("write", "/dev/null", source);
        let error: &dyn std::error::Error = &err;
        assert!(error.source().is_some(), "Io variant should have a source");
        let inner = error.source().unwrap();
        assert!(inner.to_string().contains("pipe broke"));
    }

    #[test]
    fn test_error_source_non_io_variants_are_none() {
        let variants: Vec<ArchiveError> = vec![
            ArchiveError::format(None, "test"),
            ArchiveError::corruption("path", "details"),
            ArchiveError::password("wrong"),
            ArchiveError::invalid_path("p", "r"),
            ArchiveError::unsupported("op", ArchiveFormat::Zip, None::<&str>),
            ArchiveError::write_mode_only("test"),
            ArchiveError::read_only_backend("test"),
            ArchiveError::codec_unavailable("LZMA", ArchiveFormat::SevenZip),
        ];
        for err in &variants {
            let error: &dyn std::error::Error = err;
            assert!(
                error.source().is_none(),
                "Non-Io variant {:?} should have source() == None",
                err
            );
        }
    }

    #[test]
    fn test_debug_format_all_variants() {
        // Verify Debug is implemented and doesn't panic for all variants
        let source = io::Error::new(io::ErrorKind::Other, "test");
        let variants: Vec<ArchiveError> = vec![
            ArchiveError::io("op", "/path", source),
            ArchiveError::format(Some(ArchiveFormat::Zip), "msg"),
            ArchiveError::corruption("p", "d"),
            ArchiveError::password("pw"),
            ArchiveError::invalid_path("p", "r"),
            ArchiveError::unsupported("op", ArchiveFormat::Tar, Some("detail")),
            ArchiveError::codec_unavailable("XZ", ArchiveFormat::Xz),
            ArchiveError::write_mode_only("op"),
            ArchiveError::read_only_backend("op"),
        ];
        for err in &variants {
            let debug_str = format!("{:?}", err);
            assert!(!debug_str.is_empty());
        }
    }

    // ── Codec install instructions ──

    #[test]
    fn test_codec_install_instructions_rar() {
        let err = ArchiveError::codec_unavailable("RAR", ArchiveFormat::Rar);
        match &err {
            ArchiveError::CodecUnavailable {
                install_instructions,
                ..
            } => {
                assert!(install_instructions.contains("UnRAR"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_codec_install_instructions_unknown_codec() {
        let err = ArchiveError::codec_unavailable("ZSTD", ArchiveFormat::Zip);
        match &err {
            ArchiveError::CodecUnavailable {
                install_instructions,
                ..
            } => {
                assert!(
                    install_instructions.contains("ZSTD"),
                    "Should mention the codec name in generic fallback"
                );
            }
            _ => unreachable!(),
        }
    }

    // ── ArchiveWarning ──

    #[test]
    fn test_warning_skipped_symlink_with_target() {
        let warn = ArchiveWarning::SkippedSymlink {
            path: "link.txt".into(),
            target: Some("/etc/passwd".into()),
        };
        let msg = warn.to_string();
        assert!(msg.contains("link.txt"));
        assert!(msg.contains("/etc/passwd"));
        assert!(msg.contains("FR-022"));
    }

    #[test]
    fn test_warning_skipped_symlink_without_target() {
        let warn = ArchiveWarning::SkippedSymlink {
            path: "orphan_link".into(),
            target: None,
        };
        let msg = warn.to_string();
        assert!(msg.contains("orphan_link"));
        assert!(msg.contains("symbolic link"));
    }

    #[test]
    fn test_warning_skipped_hardlink() {
        let warn = ArchiveWarning::SkippedHardLink {
            path: "hardlink.dat".into(),
        };
        let msg = warn.to_string();
        assert!(msg.contains("hardlink.dat"));
        assert!(msg.contains("hard link"));
        assert!(msg.contains("FR-022"));
    }

    #[test]
    fn test_warning_equality() {
        let w1 = ArchiveWarning::SkippedHardLink {
            path: "a.txt".into(),
        };
        let w2 = ArchiveWarning::SkippedHardLink {
            path: "a.txt".into(),
        };
        let w3 = ArchiveWarning::SkippedHardLink {
            path: "b.txt".into(),
        };
        assert_eq!(w1, w2);
        assert_ne!(w1, w3);
    }

    #[test]
    fn test_warning_clone() {
        let warn = ArchiveWarning::SkippedSymlink {
            path: "link".into(),
            target: Some("target".into()),
        };
        let cloned = warn.clone();
        assert_eq!(warn, cloned);
    }

    // ── ResultWithWarnings ──

    #[test]
    fn test_result_with_warnings_ok() {
        let r = ResultWithWarnings::ok(42);
        assert_eq!(r.value, 42);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn test_result_with_warnings_with_warnings() {
        let warns = vec![ArchiveWarning::SkippedHardLink { path: "hl".into() }];
        let r = ResultWithWarnings::with_warnings("value", warns);
        assert_eq!(r.value, "value");
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn test_result_with_warnings_add_warning() {
        let mut r = ResultWithWarnings::ok(());
        assert!(r.warnings.is_empty());
        r.add_warning(ArchiveWarning::SkippedHardLink { path: "a".into() });
        r.add_warning(ArchiveWarning::SkippedSymlink {
            path: "b".into(),
            target: None,
        });
        assert_eq!(r.warnings.len(), 2);
    }
}
