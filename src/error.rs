//! Unified error handling for archive operations

use std::path::PathBuf;

/// Unified error type for all archive operations
///
/// R0076-0076: marked `#[non_exhaustive]` so adding a new variant is not a
/// breaking change for downstream code that exhaustively matches.
#[derive(Debug)]
#[non_exhaustive]
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

    /// Corruption detected
    ///
    /// `path` names the corrupt subject, which is an *entry* path for
    /// payload/CRC failures and an *archive* path for container-level
    /// failures (header walks, recovery records, backend-state
    /// inconsistencies). `Display` therefore stays neutral about which
    /// of the two it is (R0001-0069).
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

    /// Operation not available in the current archive mode (e.g. extracting from Write-mode)
    WriteModeOnly { operation: String },

    /// Backend does not support this operation (e.g. creation on read-only backend)
    ReadOnlyBackend { operation: String },

    /// Feature not yet implemented (deferred to future phase).
    ///
    /// No public call path returns this today: its only production
    /// producer is the `ReadBackend::extract_to_stream_by_listing_id`
    /// trait default, and the one caller
    /// (`Archive::calculate_content_multiset_digest_and_size`) catches
    /// it and falls back to the by-path stream. It stays in the enum as
    /// the typed home for future deferrals.
    NotImplemented { operation: String, reason: String },

    /// Operation blocked by resource limits, conflicts, or format constraints
    OperationBlocked { operation: String, reason: String },

    /// Invalid path
    InvalidPath { path: String, reason: String },

    /// Operation cancelled by a caller-supplied callback (R0075-0003).
    ///
    /// Returned when a long-running step is aborted via a progress hook
    /// that signalled cancellation. Exactly three labels are emitted
    /// today: SFX staging (`"sfx_staging"`), the backend extract-all
    /// loops (`"extract_all"`), and creation writes (`"create"`).
    /// `extract_files` / `extract_by_ids` / `extract_some` cancel
    /// through those shared loops, so they also report `"extract_all"`;
    /// single-entry `extract_file` passes no cancellation hook to
    /// `write_entry_atomically` and therefore never yields this
    /// variant. One typed variant covers every
    /// cancellation surface so callers match on it instead of sniffing
    /// `Format` / `OperationBlocked` message text
    /// (R0076-0012 / R0076-0064 / R0076-0065).
    Cancelled { operation: &'static str },
}

/// Operation identifier carried by `ArchiveError::Unsupported`,
/// `OperationBlocked`, `WriteModeOnly`, and friends.
///
/// Migration target for the previously stringly-typed `error::ops::*`
/// constants (Review 0068 D7 / R0068-0045). Using the enum on new
/// construction sites is preferred — it eliminates typos and lets the
/// type system enforce that every variant has a known kebab/snake-case
/// label. The public `ArchiveError` constructors still accept anything
/// `Into<String>`, so call sites can pass either `Operation::ExtractAll`
/// (auto-stringified via `Display`) or one of the legacy
/// `error::ops::EXTRACT_ALL` constants without churn during the
/// migration.
///
/// R0076-0077: `#[non_exhaustive]` so new operation labels can be added
/// without breaking downstream exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operation {
    /// Generic "extract" label for shared per-entry safety checks that
    /// run before the caller's specific extract_* path is identified.
    Extract,
    /// `Archive::extract_all`
    ExtractAll,
    /// `Archive::extract_file`
    ExtractFile,
    /// `Archive::extract_to_memory` / `extract_to_memory_with_options`
    ExtractToMemory,
    /// `Archive::extract_to_stream` / `extract_to_stream_with_options`
    ExtractToStream,
    /// `Archive::extract_files`
    ExtractFiles,
    /// `Archive::extract_by_ids`
    ExtractByIds,
    /// `Archive::extract_some` / `extract_filtered`
    ExtractSome,
    /// `Archive::list_files`
    ListFiles,
    /// `Archive::list_files_for_limits`
    ListFilesForLimits,
    /// `Archive::validate_integrity`
    ValidateIntegrity,
    /// `Archive::modify` / `modify_with_options`
    Modify,
    /// `Archive::add_entry`
    AddEntry,
    /// `Archive::remove_entry` / `remove_entry_by_id` / `replace_entry`
    RemoveEntry,
    /// `Archive::commit_changes`
    CommitChanges,
    /// `Archive::pending_operations`
    PendingOperations,
    /// `Archive::clear_operations`
    ClearOperations,
    /// `Archive::add_directory_entry`
    AddDirectoryEntry,
    /// `Archive::add_file_from_data`
    AddFileFromData,
    /// `Archive::add_file_from_path_as`
    AddFileFromPathAs,
    /// `Archive::add_directory`
    AddDirectory,
    /// `Archive::add_directory_recursive`
    AddDirectoryRecursive,
    /// `Archive::create`
    Create,
    /// `Archive::finish` / `close`
    Finish,
}

impl Operation {
    /// Stable kebab/snake-case label used in error messages and assertion
    /// strings. `const`-eligible so the legacy `error::ops::*` constants
    /// can be derived from it without runtime cost.
    ///
    /// Crate-internal: external callers should rely on the `Display` impl
    /// (or `to_string()`) rather than the raw `&'static str`. Keeping the
    /// raw accessor crate-private leaves room to change the kebab/snake
    /// labels later without breaking downstream `match` statements.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Extract => "extract",
            Self::ExtractAll => "extract_all",
            Self::ExtractFile => "extract_file",
            Self::ExtractToMemory => "extract_to_memory",
            Self::ExtractToStream => "extract_to_stream",
            Self::ExtractFiles => "extract_files",
            Self::ExtractByIds => "extract_by_ids",
            Self::ExtractSome => "extract_some",
            Self::ListFiles => "list_files",
            Self::ListFilesForLimits => "list_files_for_limits",
            Self::ValidateIntegrity => "validate_integrity",
            Self::Modify => "modify",
            Self::AddEntry => "add_entry",
            Self::RemoveEntry => "remove_entry",
            Self::CommitChanges => "commit_changes",
            Self::PendingOperations => "pending_operations",
            Self::ClearOperations => "clear_operations",
            Self::AddDirectoryEntry => "add_directory_entry",
            Self::AddFileFromData => "add_file_from_data",
            Self::AddFileFromPathAs => "add_file_from_path_as",
            Self::AddDirectory => "add_directory",
            Self::AddDirectoryRecursive => "add_directory_recursive",
            Self::Create => "create",
            Self::Finish => "finish",
        }
    }
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Operation> for String {
    fn from(op: Operation) -> Self {
        op.as_str().to_string()
    }
}

/// Legacy string constants used in error construction. New code should
/// prefer the [`Operation`] enum directly; these constants are kept as
/// thin views over it for the migration window (R0068-0045 / AD 0051).
pub(crate) mod ops {
    use super::Operation;
    pub const EXTRACT: &str = Operation::Extract.as_str();
    pub const EXTRACT_ALL: &str = Operation::ExtractAll.as_str();
    pub const EXTRACT_FILE: &str = Operation::ExtractFile.as_str();
    pub const EXTRACT_TO_MEMORY: &str = Operation::ExtractToMemory.as_str();
    pub const EXTRACT_TO_STREAM: &str = Operation::ExtractToStream.as_str();
    pub const EXTRACT_FILES: &str = Operation::ExtractFiles.as_str();
    pub const EXTRACT_BY_IDS: &str = Operation::ExtractByIds.as_str();
    pub const EXTRACT_SOME: &str = Operation::ExtractSome.as_str();
    pub const LIST_FILES: &str = Operation::ListFiles.as_str();
    pub const LIST_FILES_FOR_LIMITS: &str = Operation::ListFilesForLimits.as_str();
    pub const VALIDATE_INTEGRITY: &str = Operation::ValidateIntegrity.as_str();
    pub const MODIFY: &str = Operation::Modify.as_str();
    pub const ADD_ENTRY: &str = Operation::AddEntry.as_str();
    pub const REMOVE_ENTRY: &str = Operation::RemoveEntry.as_str();
    pub const COMMIT_CHANGES: &str = Operation::CommitChanges.as_str();
    pub const PENDING_OPERATIONS: &str = Operation::PendingOperations.as_str();
    pub const CLEAR_OPERATIONS: &str = Operation::ClearOperations.as_str();
    pub const ADD_DIRECTORY_ENTRY: &str = Operation::AddDirectoryEntry.as_str();
    pub const ADD_FILE_FROM_DATA: &str = Operation::AddFileFromData.as_str();
    pub const ADD_FILE_FROM_PATH_AS: &str = Operation::AddFileFromPathAs.as_str();
    pub const ADD_DIRECTORY: &str = Operation::AddDirectory.as_str();
    pub const ADD_DIRECTORY_RECURSIVE: &str = Operation::AddDirectoryRecursive.as_str();
    pub const CREATE: &str = Operation::Create.as_str();
    pub const FINISH: &str = Operation::Finish.as_str();
}

/// Format the reason text for an out-of-range entry-ID lookup.
///
/// Special-cases empty archives so the message reads "archive has 0
/// entries; no valid IDs" instead of the prior "valid IDs: 0-0" which
/// implied ID 0 was acceptable (R0069-0013). Lives in `error.rs` so
/// every module that surfaces ID-range errors (extraction,
/// modification) can import it without a peer-module dependency.
pub(crate) fn invalid_id_reason(id: usize, entry_count: usize) -> String {
    if entry_count == 0 {
        format!("Invalid ID {}: archive has 0 entries; no valid IDs", id)
    } else {
        format!(
            "Invalid ID {}: archive has {} entries (valid IDs: 0-{})",
            id,
            entry_count,
            entry_count - 1,
        )
    }
}

/// Kinds of link entries rejected by single-file extraction (FR-022).
#[derive(Debug, Clone, Copy)]
pub(crate) enum LinkKind {
    Symbolic,
    Hard,
}

/// Construct the `OperationBlocked` error returned by each backend's
/// single-file extraction path when it encounters a link entry.
/// Centralising the phrasing keeps the FR-022 reason string consistent
/// across backends (tests match on "FR-022") and avoids copy-paste
/// drift.
///
/// `op` lets callers attribute the rejection to the public API the user
/// invoked — `extract_to_memory`, `extract_to_stream`, `extract_file`,
/// or `extract_some` (R0070-0054). Past versions hard-coded
/// `extract_file`, so memory/stream callers saw a misleading op label
/// in the resulting error.
pub(crate) fn link_extract_blocked(
    file_path: &str,
    kind: LinkKind,
    op: &'static str,
) -> ArchiveError {
    let label = match kind {
        LinkKind::Symbolic => "symbolic link",
        LinkKind::Hard => "hard link",
    };
    ArchiveError::OperationBlocked {
        operation: op.to_string(),
        reason: format!(
            "Entry '{}' is a {}; refusing to materialize per FR-022 link-skip policy",
            file_path, label
        ),
    }
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
                // R0001-0069: `path` names an archive at many construction
                // sites and an entry at others, so the message stays neutral
                // about which of the two the subject is instead of asserting
                // "entry" and mislabelling whole-archive corruption.
                write!(f, "Corruption detected in '{}': {}", path, details)
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
            ArchiveError::WriteModeOnly { operation } => {
                write!(
                    f,
                    "Operation '{}' cannot be performed: archive is in write mode",
                    operation
                )
            }
            ArchiveError::ReadOnlyBackend { operation } => {
                write!(
                    f,
                    "Operation '{}' cannot be performed: backend is read-only",
                    operation
                )
            }
            ArchiveError::NotImplemented { operation, reason } => {
                write!(
                    f,
                    "Operation '{}' is not yet implemented: {}",
                    operation, reason
                )
            }
            ArchiveError::OperationBlocked { operation, reason } => {
                write!(
                    f,
                    "Operation '{}' cannot be performed: {}",
                    operation, reason
                )
            }
            ArchiveError::InvalidPath { path, reason } => {
                write!(f, "Invalid path '{}': {}", path, reason)
            }
            ArchiveError::Cancelled { operation } => {
                write!(f, "Operation '{}' was cancelled by caller", operation)
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

    /// Create the corruption error for a declared-vs-actual payload
    /// length mismatch.
    ///
    /// One constructor for every commit route, because a source that
    /// hands the writer a byte count other than the length it declared
    /// is a declared-size violation, and DCR-011 already classifies
    /// those as `Corruption` on the digest routes. Before this existed
    /// the same condition surfaced three ways — `Format`
    /// "over-produced"/"under-produced" from the ZIP writer, `Io`
    /// "Stream length mismatch" from the libarchive writer, and a third
    /// spelling from the buffered fallback — so a caller could not
    /// match on it.
    ///
    /// `actual` for an over-producing source is normally a *lower
    /// bound*: the write paths read through `Read::take(declared + 1)`
    /// and fail on the probe byte rather than draining the source, so
    /// the message says "at least" in that direction. The message
    /// always states both numbers, so the caller can see which way the
    /// mismatch went.
    pub fn declared_length_mismatch(path: impl Into<String>, declared: u64, actual: u64) -> Self {
        let details = if actual > declared {
            format!(
                "payload over-produced: declared {} bytes, observed at least {} bytes",
                declared, actual
            )
        } else if actual < declared {
            format!(
                "payload under-produced: declared {} bytes, observed {} bytes",
                declared, actual
            )
        } else {
            // Not a mismatch. Reachable only from a mis-wired caller;
            // stay truthful rather than asserting a direction.
            format!(
                "payload length reported as mismatched: declared {} bytes, observed {} bytes",
                declared, actual
            )
        };
        Self::Corruption {
            path: path.into(),
            details,
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

    /// Platform-specific installation instructions for a codec.
    ///
    /// Public so a caller that wants the hint without building an error
    /// — a write-filter registration site probing codec availability,
    /// for instance — can reach it from outside this module.
    /// `ArchiveError::codec_unavailable` embeds the same text.
    pub fn codec_install_instructions(codec: &str) -> String {
        Self::get_codec_install_instructions(codec)
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

        // Codec names arrive in whatever spelling the backend uses —
        // libarchive filter names are lower-case ("xz", "lzma"), the 7z
        // and RAR paths use upper-case — so match on a folded key and
        // keep the caller's spelling for the message.
        let key = codec.to_ascii_uppercase();
        match (key.as_str(), platform) {
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
        Self::WriteModeOnly {
            operation: operation.into(),
        }
    }

    /// Create error for read-only backends that don't support creation
    pub fn read_only_backend(operation: impl Into<String>) -> Self {
        Self::ReadOnlyBackend {
            operation: operation.into(),
        }
    }

    /// Create error for features not yet implemented
    pub fn not_implemented(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::NotImplemented {
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create error for operations blocked by resource limits, conflicts, or format constraints
    pub fn operation_blocked(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::OperationBlocked {
            operation: operation.into(),
            reason: reason.into(),
        }
    }
}

/// Entry kind that no backend can materialize on extraction and that no
/// writer can re-emit on modify.
///
/// DCR-010 turned the FR-022 skip policy from a *link denylist* into a
/// *kind allowlist*: only regular files and directories may reach the
/// disk writer, so FIFOs, sockets and character/block device nodes are
/// skipped. That skip class had no `ArchiveWarning` of its own, which
/// made the rejections in the libarchive reader (R0001-0001) and the 7z
/// reader (R0001-0022) silent. This enum names the kind precisely so a
/// security-auditing caller is never handed a link warning for a device
/// node.
///
/// `#[non_exhaustive]` so further kinds can be named without breaking
/// downstream exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnsupportedEntryKind {
    /// Symbolic link. Only produced by the modify path, which drops link
    /// entries it cannot re-emit; extraction reports symlinks through
    /// the dedicated `ArchiveWarning::SkippedSymlink` (which also
    /// carries the link target).
    Symlink,
    /// Hard link. Same split as `Symlink`: extraction uses
    /// `ArchiveWarning::SkippedHardLink`.
    HardLink,
    /// Named pipe (`S_IFIFO`).
    Fifo,
    /// Unix domain socket (`S_IFSOCK`).
    Socket,
    /// Character device node (`S_IFCHR`).
    CharacterDevice,
    /// Block device node (`S_IFBLK`).
    BlockDevice,
    /// A kind the backend recognised as neither a regular file nor a
    /// directory but could not classify further — for example a 7z
    /// entry whose `S_IFMT` bits are absent or unknown, decoded as
    /// `EntryType::Other`.
    Other,
}

impl UnsupportedEntryKind {
    /// Classify a raw Unix mode word.
    ///
    /// The argument may be a full `st_mode` / `AE_IFMT` word: the format
    /// bits are masked out here, so callers do not have to. Returns
    /// `None` for regular files and directories (the two allowlisted
    /// kinds) and for a mode word with no format bits set at all, which
    /// carries no kind information and must not be reported as a
    /// skipped special entry.
    pub fn from_unix_mode(mode: u32) -> Option<Self> {
        // S_IFMT and friends, spelled out so this compiles identically
        // on every target without pulling in `libc`. These values are
        // fixed by POSIX and match libarchive's `AE_IF*` constants.
        const S_IFMT: u32 = 0o170_000;
        const S_IFIFO: u32 = 0o010_000;
        const S_IFCHR: u32 = 0o020_000;
        const S_IFDIR: u32 = 0o040_000;
        const S_IFBLK: u32 = 0o060_000;
        const S_IFREG: u32 = 0o100_000;
        const S_IFLNK: u32 = 0o120_000;
        const S_IFSOCK: u32 = 0o140_000;

        match mode & S_IFMT {
            S_IFREG | S_IFDIR => None,
            0 => None,
            S_IFIFO => Some(Self::Fifo),
            S_IFCHR => Some(Self::CharacterDevice),
            S_IFBLK => Some(Self::BlockDevice),
            S_IFLNK => Some(Self::Symlink),
            S_IFSOCK => Some(Self::Socket),
            _ => Some(Self::Other),
        }
    }
}

impl std::fmt::Display for UnsupportedEntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            UnsupportedEntryKind::Symlink => "symbolic link",
            UnsupportedEntryKind::HardLink => "hard link",
            UnsupportedEntryKind::Fifo => "FIFO",
            UnsupportedEntryKind::Socket => "socket",
            UnsupportedEntryKind::CharacterDevice => "character device",
            UnsupportedEntryKind::BlockDevice => "block device",
            UnsupportedEntryKind::Other => "unsupported-kind",
        };
        f.write_str(name)
    }
}

/// Why an entry of an unsupported kind never reached its destination.
///
/// Both arms describe the *same* class of entry — one during read-out,
/// one during rewrite — so they share
/// `ArchiveWarning::SkippedUnsupportedEntry` rather than splitting into
/// two variants a caller would have to match twice.
///
/// `#[non_exhaustive]` so further stages can be named without breaking
/// downstream exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntrySkipReason {
    /// Skipped during extraction: the entry kind is not materializable,
    /// so it was passed over instead of being written to the
    /// destination (DCR-010).
    UnsupportedKindOnExtract,
    /// Dropped during a modify-mode rewrite: the entry exists in the
    /// source archive but the writer cannot re-emit its kind, so the
    /// rewritten archive does not contain it.
    DroppedDuringModify,
}

impl std::fmt::Display for EntrySkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            EntrySkipReason::UnsupportedKindOnExtract => "skipped during extraction",
            EntrySkipReason::DroppedDuringModify => "dropped during modify",
        };
        f.write_str(name)
    }
}

/// Warning emitted during archive operations (FR-022).
///
/// Warnings indicate non-fatal conditions that may require user attention.
/// Currently emitted only by the extraction path. The enum is
/// `#[non_exhaustive]` so future variants can be added without
/// breaking existing callers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

    /// Two entries' output paths collide on case-insensitive filesystems
    ///
    /// The extraction preflight compares output paths byte-exactly, but
    /// default macOS (APFS) and Windows volumes fold case, so entries such
    /// as `README` and `readme` silently merge — the later entry replaces
    /// the earlier one (R0079-0036). The destination's case sensitivity
    /// cannot be known portably, so this is a warning rather than an
    /// error; callers extracting to case-insensitive volumes should treat
    /// it as a conflict. The comparison uses Unicode lowercase folding;
    /// normalization-form collisions (NFC vs NFD) are not detected.
    OutputPathCaseCollision {
        /// Archive path of the entry that first claimed the output path
        first: String,
        /// Archive path of the later entry whose output path collides
        second: String,
    },

    /// Entry passed over because its *kind* is not supported
    ///
    /// DCR-010 widened the FR-022 skip class from links to every entry
    /// kind outside the regular-file/directory allowlist, but left the
    /// caller-visible half unbuilt: the libarchive reader (R0001-0001)
    /// and the 7z reader (R0001-0022) skipped FIFOs, sockets and device
    /// nodes silently, which contradicts MADR-0010's "silent data loss
    /// is unacceptable". This variant closes that gap, and the
    /// modify-mode rewrite reuses it for link and special entries it
    /// cannot re-emit — `reason` says which of the two happened, and
    /// `kind` says exactly what was lost, so a security-auditing caller
    /// is never told "symlink" about a device node.
    SkippedUnsupportedEntry {
        /// Archive path of the entry, in the same normalised spelling
        /// that listing and filtering report, so callers can correlate
        /// by string match
        path: String,
        /// Kind of the entry that was skipped or dropped
        kind: UnsupportedEntryKind,
        /// Whether the entry was skipped on extraction or dropped by a
        /// modify-mode rewrite
        reason: EntrySkipReason,
    },

    /// A backend recovered from a condition but had something to say
    ///
    /// Unlike every variant above, this one carries **unstructured
    /// third-party text**. It exists because libarchive's `ARCHIVE_WARN`
    /// is a recoverable status whose only detail is a free-form English
    /// string from the linked library — not a vendored one, so its exact
    /// wording is version- and format-dependent and cannot be parsed into
    /// fields here without inventing a taxonomy the library does not have.
    ///
    /// Before OI-0080-006 that text was discarded at every site that
    /// accepted the status, so a caller was told an archive was readable
    /// with no way to learn what the library had objected to. Keeping it
    /// verbatim is the point: do not match on `message`, show it.
    ///
    /// `ARCHIVE_WARN` means libarchive continued. A condition it could not
    /// recover from is an [`ArchiveError`], never this.
    BackendAdvisory {
        /// Backend that produced the message, e.g. `"libarchive"`
        backend: &'static str,
        /// Operation during which it was produced, spelled the same way
        /// `ArchiveError` spells its `operation` fields, so an advisory
        /// correlates with the errors from the same call
        operation: String,
        /// The backend's own text, verbatim and unparsed
        message: String,
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
            ArchiveWarning::BackendAdvisory {
                backend,
                operation,
                message,
            } => {
                write!(
                    f,
                    "{} reported a recoverable condition during {}: {}",
                    backend, operation, message
                )
            }
            ArchiveWarning::OutputPathCaseCollision { first, second } => {
                write!(
                    f,
                    "Entries '{}' and '{}' differ only by case and merge on case-insensitive destination filesystems (R0079-0036)",
                    first, second
                )
            }
            ArchiveWarning::SkippedUnsupportedEntry { path, kind, reason } => match reason {
                EntrySkipReason::UnsupportedKindOnExtract => write!(
                    f,
                    "Skipped {} entry '{}': only regular files and directories are materialized (DCR-010)",
                    kind, path
                ),
                EntrySkipReason::DroppedDuringModify => write!(
                    f,
                    "Dropped {} entry '{}' while rewriting the archive: the writer cannot re-emit this entry kind",
                    kind, path
                ),
            },
        }
    }
}

impl ArchiveWarning {
    /// Report an entry skipped during extraction because its kind is
    /// outside the regular-file/directory allowlist (DCR-010).
    pub fn skipped_unsupported_entry(path: impl Into<String>, kind: UnsupportedEntryKind) -> Self {
        Self::SkippedUnsupportedEntry {
            path: path.into(),
            kind,
            reason: EntrySkipReason::UnsupportedKindOnExtract,
        }
    }

    /// Report an entry dropped by a modify-mode rewrite because the
    /// writer cannot re-emit its kind.
    pub fn dropped_unsupported_entry(path: impl Into<String>, kind: UnsupportedEntryKind) -> Self {
        Self::SkippedUnsupportedEntry {
            path: path.into(),
            kind,
            reason: EntrySkipReason::DroppedDuringModify,
        }
    }
}

/// Result type that includes warnings
#[derive(Debug)]
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
            ArchiveError::WriteModeOnly { operation } => {
                assert_eq!(operation, "extract_all");
            }
            other => panic!("Expected WriteModeOnly variant, got {:?}", other),
        }
    }

    #[test]
    fn test_read_only_backend_error() {
        let err = ArchiveError::read_only_backend("create");
        match &err {
            ArchiveError::ReadOnlyBackend { operation } => {
                assert_eq!(operation, "create");
            }
            other => panic!("Expected ReadOnlyBackend variant, got {:?}", other),
        }
    }

    #[test]
    fn test_not_implemented_error() {
        let err = ArchiveError::not_implemented("create_split", "split-archive creation deferred");
        match &err {
            ArchiveError::NotImplemented { operation, reason } => {
                assert_eq!(operation, "create_split");
                assert!(reason.contains("deferred"));
            }
            other => panic!("Expected NotImplemented variant, got {:?}", other),
        }
    }

    #[test]
    fn test_operation_blocked_error() {
        let err = ArchiveError::operation_blocked("extract", "file too large");
        match &err {
            ArchiveError::OperationBlocked { operation, reason } => {
                assert_eq!(operation, "extract");
                assert!(reason.contains("too large"));
            }
            other => panic!("Expected OperationBlocked variant, got {:?}", other),
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
        assert!(msg.contains("Corruption detected in 'inner/data.bin'"));
        assert!(msg.contains("0xDEAD"));
    }

    /// R0001-0069: the same `Display` arm renders archive-subject
    /// corruption, so it must not claim the path is an entry.
    #[test]
    fn test_display_corruption_error_archive_subject() {
        let err =
            ArchiveError::corruption("/vol/backup.rar", "recovery record header CRC mismatch");
        let msg = err.to_string();
        assert!(msg.contains("Corruption detected in '/vol/backup.rar'"));
        assert!(!msg.contains("entry"));
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
    fn test_display_write_mode_only() {
        let err = ArchiveError::write_mode_only("list_files");
        let msg = err.to_string();
        assert!(msg.contains("'list_files'"));
        assert!(msg.contains("cannot be performed"));
        assert!(msg.contains("write mode"));
    }

    #[test]
    fn test_display_read_only_backend() {
        let err = ArchiveError::read_only_backend("create");
        let msg = err.to_string();
        assert!(msg.contains("'create'"));
        assert!(msg.contains("cannot be performed"));
        assert!(msg.contains("read-only"));
    }

    #[test]
    fn test_display_not_implemented() {
        let err = ArchiveError::not_implemented("create_split", "split-archive creation deferred");
        let msg = err.to_string();
        assert!(msg.contains("'create_split'"));
        assert!(msg.contains("not yet implemented"));
        assert!(msg.contains("split-archive creation deferred"));
    }

    #[test]
    fn test_display_operation_blocked() {
        let err = ArchiveError::operation_blocked("extract", "file too large");
        let msg = err.to_string();
        assert!(msg.contains("'extract'"));
        assert!(msg.contains("cannot be performed"));
        assert!(msg.contains("file too large"));
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
            ArchiveError::not_implemented("op", "reason"),
            ArchiveError::operation_blocked("op", "reason"),
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
        let source = io::Error::other("test");
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
            ArchiveError::not_implemented("op", "reason"),
            ArchiveError::operation_blocked("op", "reason"),
        ];
        for err in &variants {
            let debug_str = format!("{:?}", err);
            assert!(!debug_str.is_empty());
        }
    }

    // ── Declared-vs-actual length mismatch ──

    #[test]
    fn declared_length_mismatch_is_corruption_naming_both_lengths() {
        let err = ArchiveError::declared_length_mismatch("dir/data.bin", 100, 42);
        match &err {
            ArchiveError::Corruption { path, details } => {
                assert_eq!(path, "dir/data.bin");
                assert!(
                    details.contains("100"),
                    "declared length missing: {details}"
                );
                assert!(details.contains("42"), "actual length missing: {details}");
            }
            other => panic!("Expected Corruption variant, got {other:?}"),
        }
    }

    #[test]
    fn declared_length_mismatch_says_which_direction_it_went() {
        let under = ArchiveError::declared_length_mismatch("e", 100, 42).to_string();
        assert!(under.contains("under-produced"), "{under}");
        assert!(!under.contains("over-produced"), "{under}");

        let over = ArchiveError::declared_length_mismatch("e", 100, 101).to_string();
        assert!(over.contains("over-produced"), "{over}");
        // The write paths stop on the `take(declared + 1)` probe byte, so
        // the over-production count is a lower bound, not a total.
        assert!(over.contains("at least"), "{over}");
    }

    #[test]
    fn declared_length_mismatch_display_names_the_entry() {
        let msg = ArchiveError::declared_length_mismatch("dir/data.bin", 7, 0).to_string();
        assert!(msg.contains("dir/data.bin"), "{msg}");
        assert!(msg.starts_with("Corruption detected"), "{msg}");
    }

    #[test]
    fn declared_length_mismatch_equal_lengths_stay_truthful() {
        // A mis-wired caller must not get a false direction claim.
        let msg = ArchiveError::declared_length_mismatch("e", 5, 5).to_string();
        assert!(msg.contains('5'), "{msg}");
        assert!(!msg.contains("over-produced"), "{msg}");
        assert!(!msg.contains("under-produced"), "{msg}");
    }

    // ── SkippedUnsupportedEntry ──

    #[test]
    fn skipped_unsupported_entry_carries_path_kind_and_reason() {
        let warn = ArchiveWarning::skipped_unsupported_entry(
            "dev/null",
            UnsupportedEntryKind::CharacterDevice,
        );
        match &warn {
            ArchiveWarning::SkippedUnsupportedEntry { path, kind, reason } => {
                assert_eq!(path, "dev/null");
                assert_eq!(*kind, UnsupportedEntryKind::CharacterDevice);
                assert_eq!(*reason, EntrySkipReason::UnsupportedKindOnExtract);
            }
            other => panic!("Expected SkippedUnsupportedEntry, got {other:?}"),
        }
    }

    #[test]
    fn dropped_unsupported_entry_marks_the_modify_reason() {
        let warn = ArchiveWarning::dropped_unsupported_entry("link", UnsupportedEntryKind::Symlink);
        match &warn {
            ArchiveWarning::SkippedUnsupportedEntry { reason, kind, .. } => {
                assert_eq!(*reason, EntrySkipReason::DroppedDuringModify);
                assert_eq!(*kind, UnsupportedEntryKind::Symlink);
            }
            other => panic!("Expected SkippedUnsupportedEntry, got {other:?}"),
        }
    }

    #[test]
    fn skipped_unsupported_entry_display_names_kind_and_stage() {
        let extract =
            ArchiveWarning::skipped_unsupported_entry("p/fifo", UnsupportedEntryKind::Fifo)
                .to_string();
        assert!(extract.contains("p/fifo"), "{extract}");
        assert!(extract.contains("FIFO"), "{extract}");
        assert!(extract.starts_with("Skipped"), "{extract}");

        let modify =
            ArchiveWarning::dropped_unsupported_entry("p/sock", UnsupportedEntryKind::Socket)
                .to_string();
        assert!(modify.contains("p/sock"), "{modify}");
        assert!(modify.contains("socket"), "{modify}");
        assert!(modify.starts_with("Dropped"), "{modify}");
    }

    #[test]
    fn skipped_unsupported_entry_never_claims_a_link_for_a_device_node() {
        // The whole point of the variant (DCR-010): a security-auditing
        // caller must not read "symbolic link" for a block device.
        let msg = ArchiveWarning::skipped_unsupported_entry("d", UnsupportedEntryKind::BlockDevice)
            .to_string();
        assert!(msg.contains("block device"), "{msg}");
        assert!(!msg.contains("link"), "{msg}");
    }

    #[test]
    fn skipped_unsupported_entry_equality_distinguishes_kind_and_reason() {
        let a = ArchiveWarning::skipped_unsupported_entry("p", UnsupportedEntryKind::Fifo);
        let b = ArchiveWarning::skipped_unsupported_entry("p", UnsupportedEntryKind::Fifo);
        let other_kind =
            ArchiveWarning::skipped_unsupported_entry("p", UnsupportedEntryKind::Socket);
        let other_reason =
            ArchiveWarning::dropped_unsupported_entry("p", UnsupportedEntryKind::Fifo);
        assert_eq!(a, b);
        assert_ne!(a, other_kind);
        assert_ne!(a, other_reason);
    }

    #[test]
    fn unsupported_entry_kind_from_unix_mode_classifies_special_kinds() {
        // Full mode words, format bits plus permission bits.
        assert_eq!(
            UnsupportedEntryKind::from_unix_mode(0o010_644),
            Some(UnsupportedEntryKind::Fifo)
        );
        assert_eq!(
            UnsupportedEntryKind::from_unix_mode(0o020_666),
            Some(UnsupportedEntryKind::CharacterDevice)
        );
        assert_eq!(
            UnsupportedEntryKind::from_unix_mode(0o060_660),
            Some(UnsupportedEntryKind::BlockDevice)
        );
        assert_eq!(
            UnsupportedEntryKind::from_unix_mode(0o120_777),
            Some(UnsupportedEntryKind::Symlink)
        );
        assert_eq!(
            UnsupportedEntryKind::from_unix_mode(0o140_755),
            Some(UnsupportedEntryKind::Socket)
        );
    }

    #[test]
    fn unsupported_entry_kind_from_unix_mode_allows_files_dirs_and_bare_modes() {
        assert_eq!(UnsupportedEntryKind::from_unix_mode(0o100_644), None);
        assert_eq!(UnsupportedEntryKind::from_unix_mode(0o040_755), None);
        // No format bits at all carries no kind information, so it must
        // not be reported as a skipped special entry.
        assert_eq!(UnsupportedEntryKind::from_unix_mode(0o644), None);
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
    fn codec_install_instructions_are_case_insensitive() {
        // libarchive spells its filters in lower case; the instruction
        // table is keyed in upper case. Both must reach the same arm so
        // a write-filter registration site gets the real hint instead of
        // the generic fallback.
        let upper = ArchiveError::codec_install_instructions("XZ");
        let lower = ArchiveError::codec_install_instructions("xz");
        assert_eq!(upper, lower);
        // The instruction table only has XZ arms for the three
        // first-class platforms; elsewhere both spellings share the
        // generic fallback, which the equality assertion above already
        // covers.
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        assert!(!lower.contains("libarchive documentation"), "{lower}");
    }

    #[test]
    fn codec_unavailable_is_constructible_and_displayable_from_a_filter_name() {
        // Shape check for the libarchive write-filter call site: a
        // lower-case `&str` codec name plus a format is all it needs.
        let err = ArchiveError::codec_unavailable("zstd", ArchiveFormat::Tar);
        let msg = err.to_string();
        match &err {
            ArchiveError::CodecUnavailable {
                codec,
                format,
                install_instructions,
            } => {
                assert_eq!(codec, "zstd");
                assert_eq!(*format, ArchiveFormat::Tar);
                assert!(!install_instructions.is_empty());
            }
            other => panic!("Expected CodecUnavailable variant, got {other:?}"),
        }
        assert!(msg.contains("zstd"), "{msg}");
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
