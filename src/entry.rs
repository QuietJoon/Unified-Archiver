//! Archive entry metadata

use std::time::SystemTime;

/// Type of archive entry
///
/// R0076-0079: marked `#[non_exhaustive]` so a richer entry category
/// (devices, FIFOs, sockets) can be added without breaking downstream
/// exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    HardLink,
    Other,
}

/// Platform-specific file attributes.
///
/// **Per-backend coverage (R0070-0061).** Most fields are populated
/// only by a subset of backends — exposing them as plain `Option<…>`
/// implies blanket support that no backend currently delivers. The
/// table below documents what each backend actually fills today;
/// callers that need a guaranteed signal should consult the format's
/// capability matrix or fall back to the typed top-level
/// [`ArchiveEntry`] fields (`permissions`, `link_target`, …).
///
/// R0001-0044: the `7z` and `RAR` columns are stated as the backends
/// actually behave — `archive_specific` is a RAR-only field (the 7z
/// reader leaves it `None`), and RAR fills `windows` from the entry's
/// recorded host OS, not from the archive generation.
///
/// | Field             | ZIP (zip) | 7z | RAR | libarchive (TAR/ISO) |
/// |-------------------|---------------|----|-----|----------------------|
/// | `windows`         | none today    | populated when entry has Windows attrs | populated when the entry's host OS is Windows | none today |
/// | `unix_xattr`      | none today    | none today | none today | future work — libarchive exposes via `archive_entry_xattr_*` but not wired |
/// | `archive_specific`| none today    | none today | populated with the entry's host OS / method / unpack version | none today |
///
/// Future versions may move backend-specific data behind a typed enum
/// so the type system can enforce coverage; until then, treat this
/// struct as a hint surface.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct FileAttributes {
    /// Windows file attributes (FILE_ATTRIBUTE_*).
    /// Currently populated by the 7z backend (when the entry carries
    /// Windows attributes) and the RAR backend (when the entry records a
    /// Windows host OS) only.
    pub windows: Option<u32>,

    /// Unix extended attributes (name-value pairs).
    /// **Currently never populated by any backend** — the field is
    /// reserved for the future libarchive `archive_entry_xattr_*`
    /// plumbing.
    pub unix_xattr: Option<Vec<(String, Vec<u8>)>>,

    /// Archive-specific attributes (format-dependent string).
    /// Currently populated by the RAR backend only, as
    /// `host_os=<n> method=<n> unp_ver=<n>` taken from the entry header
    /// (R0001-0044). The 7z backend leaves this `None` — it has no
    /// solid-block tag to surface, because `sevenz-rust2` does not
    /// expose one on its entry type.
    pub archive_specific: Option<String>,
}

/// Metadata for a single file or directory within an archive
///
/// This structure provides consistent metadata across all archive formats.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Full path within archive (UTF-8, forward slashes)
    pub path: String,

    /// File size in bytes (uncompressed), None for directories
    pub size: Option<u64>,

    /// Compressed size in bytes, None for uncompressed formats or directories
    pub compressed_size: Option<u64>,

    /// Last modification time (UTC)
    pub modified: Option<SystemTime>,

    /// CRC32 checksum, None if not available
    pub crc32: Option<u32>,

    /// Entry type
    pub entry_type: EntryType,

    /// Unix permission bits (`mode & 0o7777`) when known.
    ///
    /// Always represented as Unix permission bits regardless of the
    /// source archive's host platform — backends mask file-type bits
    /// (S_IFMT) and platform-specific attribute bits (Windows
    /// FILE_ATTRIBUTE_*) before populating this field, so callers can
    /// rely on `permissions & !0o7777 == 0` (R0075-0024 / 0044 / 0045
    /// / 0056 / 0079). Windows-only attributes live in
    /// [`Self::attributes`]`.windows`.
    ///
    /// `None` when the source archive doesn't carry permission
    /// information (raw gzip / bzip2 / xz / zst / lz4 / lzma streams,
    /// RAR archives stored on a Windows host that recorded no Unix
    /// mode, ZIP archives lacking the Unix-attribute extra field).
    pub permissions: Option<u32>,

    // Phase 1: Enhanced metadata fields
    /// Creation time (UTC)
    pub created: Option<SystemTime>,

    /// Last access time (UTC)
    pub accessed: Option<SystemTime>,

    /// Whether entry is encrypted/password-protected
    pub is_encrypted: bool,

    /// Entry comment (if format supports)
    pub comment: Option<String>,

    /// Platform-specific file attributes
    pub attributes: Option<FileAttributes>,

    /// Raw archive entry name as bytes (R0070-0025).
    ///
    /// When the backend's metadata API exposes the entry name as raw
    /// bytes that don't round-trip cleanly through UTF-8 (legacy
    /// archives created on systems with non-UTF-8 locales), the bytes
    /// are stored here verbatim. The display-friendly `path` field
    /// stays UTF-8 and may have used `to_string_lossy()` substitution
    /// (`U+FFFD`) on those entries.
    ///
    /// `None` means either the bytes were valid UTF-8 (in which case
    /// `path` is byte-for-byte exact) or the backend doesn't surface
    /// raw names. Callers that need to extract such entries by exact
    /// name must compare against this byte slice rather than `path`.
    pub raw_path: Option<Vec<u8>>,

    /// Symlink/hardlink target (R0070-0060).
    ///
    /// Populated for `EntryType::Symlink` / `EntryType::HardLink`
    /// entries when the backend surfaces the target during listing.
    /// Coverage is best-effort and varies by backend:
    ///
    /// - libarchive: populated from `archive_entry_symlink` /
    ///   `archive_entry_hardlink` when present.
    /// - ZIP / 7z / UnRAR: populated when the backend's
    ///   metadata API exposes the target; otherwise `None`.
    ///
    /// Always `None` for `EntryType::File` and `EntryType::Directory`.
    pub link_target: Option<String>,

    /// Sequential index/ID within archive (0-based)
    ///
    /// This ID is consistent across all archive formats and represents
    /// the position of this entry in the archive's file list.
    /// Use this ID with `extract_by_ids()` for efficient batch extraction.
    pub id: usize,
}

/// Builder for [`ArchiveEntry`] (R0075-0078).
///
/// Returned by [`ArchiveEntry::file`] / [`ArchiveEntry::try_file`] /
/// [`ArchiveEntry::dir_at`] / [`ArchiveEntry::try_dir_at`]. All
/// metadata setters consume `self` and
/// return the builder, so call sites read top-down:
///
/// ```
/// use unified_archive::ArchiveEntry;
/// let entry = ArchiveEntry::file("readme.md", 0)
///     .size(1024)
///     .crc32(0xDEADBEEF)
///     .permissions(0o644)
///     .build();
/// ```
///
/// The setter for `permissions` masks off non-Unix bits so the
/// `permissions & !0o7777 == 0` contract (R0075-0079) cannot be
/// violated through the builder.
pub struct ArchiveEntryBuilder {
    inner: ArchiveEntry,
}

impl ArchiveEntryBuilder {
    /// Set the uncompressed size in bytes.
    pub fn size(mut self, size: u64) -> Self {
        self.inner.size = Some(size);
        self
    }

    /// Set the compressed size in bytes.
    pub fn compressed_size(mut self, size: u64) -> Self {
        self.inner.compressed_size = Some(size);
        self
    }

    /// Set the last-modified timestamp.
    pub fn modified(mut self, t: SystemTime) -> Self {
        self.inner.modified = Some(t);
        self
    }

    /// Set the creation timestamp.
    pub fn created(mut self, t: SystemTime) -> Self {
        self.inner.created = Some(t);
        self
    }

    /// Set the access timestamp.
    pub fn accessed(mut self, t: SystemTime) -> Self {
        self.inner.accessed = Some(t);
        self
    }

    /// Set the CRC32 checksum.
    pub fn crc32(mut self, c: u32) -> Self {
        self.inner.crc32 = Some(c);
        self
    }

    /// Set Unix permission bits. Non-Unix bits (`!0o7777`) are masked
    /// off so the [`ArchiveEntry::permissions`] contract holds.
    pub fn permissions(mut self, p: u32) -> Self {
        self.inner.permissions = Some(p & 0o7777);
        self
    }

    /// Set the raw path bytes (preserves non-UTF-8 entry names).
    pub fn raw_path(mut self, bytes: Vec<u8>) -> Self {
        self.inner.raw_path = Some(bytes);
        self
    }

    /// Set the symlink/hardlink target.
    pub fn link_target(mut self, t: String) -> Self {
        self.inner.link_target = Some(t);
        self
    }

    /// Set the entry comment.
    pub fn comment(mut self, c: String) -> Self {
        self.inner.comment = Some(c);
        self
    }

    /// Mark the entry as encrypted/password-protected.
    pub fn encrypted(mut self, e: bool) -> Self {
        self.inner.is_encrypted = e;
        self
    }

    /// Set platform-specific file attributes.
    pub fn attributes(mut self, a: FileAttributes) -> Self {
        self.inner.attributes = Some(a);
        self
    }

    /// Finalize the builder into an [`ArchiveEntry`].
    pub fn build(self) -> ArchiveEntry {
        self.inner
    }
}

impl ArchiveEntry {
    /// Begin building a file-typed entry (R0075-0078).
    ///
    /// Path is not validated; pass through [`Self::try_file`] when
    /// you need the empty-path check.
    pub fn file(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder {
        ArchiveEntryBuilder {
            inner: Self::with_type(path.into(), id, EntryType::File),
        }
    }

    /// Begin building a file-typed entry, rejecting empty paths
    /// (R0075-0078).
    pub fn try_file(
        path: impl Into<String>,
        id: usize,
    ) -> std::result::Result<ArchiveEntryBuilder, crate::ArchiveError> {
        let path = path.into();
        if path.is_empty() {
            return Err(crate::ArchiveError::invalid_path(
                "",
                "ArchiveEntry path must not be empty",
            ));
        }
        Ok(Self::file(path, id))
    }

    /// Begin building a directory-typed entry (R0075-0078). Directories
    /// report `size = None` and `compressed_size = None` across every
    /// backend.
    ///
    /// Path is not validated; pass through [`Self::try_dir_at`] when
    /// you need the empty-path check. The infallible constructors
    /// ([`Self::file`], [`Self::dir_at`], [`Self::new`],
    /// [`Self::directory`]) share one policy: they assume the caller
    /// supplies a non-empty path, and the `try_*` variants are the
    /// single gate that enforces the empty-path invariant (R0081-0004).
    pub fn dir_at(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder {
        ArchiveEntryBuilder {
            inner: Self::with_type(path.into(), id, EntryType::Directory),
        }
    }

    /// Begin building a directory-typed entry, rejecting empty paths
    /// (R0075-0078). Fallible counterpart to [`Self::dir_at`],
    /// mirroring [`Self::try_file`] so directory and file builders
    /// enforce the same empty-path invariant (R0081-0004).
    pub fn try_dir_at(
        path: impl Into<String>,
        id: usize,
    ) -> std::result::Result<ArchiveEntryBuilder, crate::ArchiveError> {
        let path = path.into();
        if path.is_empty() {
            return Err(crate::ArchiveError::invalid_path(
                "",
                "ArchiveEntry path must not be empty",
            ));
        }
        Ok(Self::dir_at(path, id))
    }

    /// Create a new file-typed archive entry. Prefer
    /// [`ArchiveEntry::file`] for new code (R0075-0078) — the builder
    /// API allows fluent setter chains and rejects empty paths via
    /// [`ArchiveEntry::try_file`].
    ///
    /// **Marked for v0.4 deprecation.** Once the v0.4 API freeze
    /// lands this constructor will gain `#[deprecated]` and external
    /// callers will be expected to migrate to the builder. v0.3
    /// keeps it un-deprecated so the existing call surface (40+
    /// sites across the test suite + internal backends) doesn't
    /// generate noise during the migration window.
    pub fn new(path: String, id: usize) -> Self {
        Self::with_type(path, id, EntryType::File)
    }

    /// Create a new directory-typed archive entry. Prefer
    /// [`ArchiveEntry::dir_at`] for new code (R0075-0078). Same v0.4
    /// migration plan as [`Self::new`].
    pub fn directory(path: String, id: usize) -> Self {
        Self::with_type(path, id, EntryType::Directory)
    }

    /// Construct a symlink-typed entry with the supplied link target.
    /// Stamps `link_target` directly so callers don't have to set the
    /// field after the fact, which would otherwise leave a window
    /// where `entry_type == Symlink` but `link_target == None`.
    pub fn symlink(path: String, id: usize, target: String) -> Self {
        let mut entry = Self::with_type(path, id, EntryType::Symlink);
        entry.link_target = Some(target);
        entry
    }

    fn with_type(path: String, id: usize, entry_type: EntryType) -> Self {
        Self {
            path,
            size: None,
            compressed_size: None,
            modified: None,
            crc32: None,
            entry_type,
            permissions: None,
            created: None,
            accessed: None,
            is_encrypted: false,
            comment: None,
            attributes: None,
            link_target: None,
            raw_path: None,
            id,
        }
    }

    /// Observed stored-size fraction (`compressed_size / size`).
    ///
    /// Returns `None` if either size is unknown or the uncompressed size is
    /// zero. The value can exceed `1.0` when compression is ineffective or
    /// representation overhead makes the stored form larger. Note this is the
    /// **inverse** of the expansion-ratio convention used by
    /// [`crate::ExtractionLimits::max_compression_ratio`] (`uncompressed /
    /// compressed`) — use
    /// [`expansion_ratio`](Self::expansion_ratio) when you want a value
    /// directly comparable to the security limit.
    pub fn compression_fraction(&self) -> Option<f64> {
        match (self.size, self.compressed_size) {
            (Some(s), Some(cs)) if s > 0 => Some(cs as f64 / s as f64),
            _ => None,
        }
    }

    /// Expansion ratio (`size / compressed_size`), matching the convention
    /// used by [`ExtractionLimits::max_compression_ratio`](crate::ExtractionLimits::max_compression_ratio)
    /// and the zip-bomb gate.
    /// The value can be below `1.0` when the stored representation is larger
    /// than the original. Returns `None` when either size is unknown or
    /// `compressed_size` is zero.
    ///
    /// **`None` is not a safety verdict (R0001-0045).** It conflates two
    /// very different states:
    ///
    /// - *No information* — the backend did not report `size` and/or
    ///   `compressed_size` for this entry.
    /// - *Unsafe metadata* — a nonempty entry declaring **zero**
    ///   compressed bytes. The ratio is mathematically undefined and the
    ///   security layer refuses such an archive outright, because it is
    ///   indistinguishable from a crafted bomb header (R0076-0002).
    ///
    /// Do not use this helper as the bomb signal: an entry with
    /// `size > 0` and `compressed_size == Some(0)` returns the same
    /// `None` as an entry carrying no size metadata at all. The binding
    /// check is
    /// [`ExtractionLimits::max_compression_ratio`](crate::ExtractionLimits::max_compression_ratio),
    /// enforced during extraction; callers that want to pre-screen must
    /// inspect `size` / `compressed_size` themselves and treat the
    /// nonempty-with-zero-compressed case as suspicious.
    pub fn expansion_ratio(&self) -> Option<f64> {
        match (self.size, self.compressed_size) {
            (Some(s), Some(cs)) if cs > 0 => Some(s as f64 / cs as f64),
            _ => None,
        }
    }

    /// Deprecated alias for [`compression_fraction`](Self::compression_fraction).
    ///
    /// Named `compression_ratio` historically but semantically it returns the
    /// **inverse** of the security-facing ratio (`max_compression_ratio`),
    /// which misled callers comparing against that limit. Call
    /// `compression_fraction` (shrinkage) or `expansion_ratio` (zip-bomb
    /// gauge) explicitly. Scheduled for removal in a future release.
    ///
    /// Hidden from rustdoc so completion/discovery does not
    /// surface the misleading name.
    #[doc(hidden)]
    #[deprecated(since = "0.2.0", note = "use compression_fraction or expansion_ratio")]
    pub fn compression_ratio(&self) -> Option<f64> {
        self.compression_fraction()
    }

    /// Check if this entry is a directory
    pub fn is_directory(&self) -> bool {
        self.entry_type == EntryType::Directory
    }

    /// Check if this entry is a file
    pub fn is_file(&self) -> bool {
        self.entry_type == EntryType::File
    }

    /// Check if this entry is a symlink
    pub fn is_symlink(&self) -> bool {
        self.entry_type == EntryType::Symlink
    }

    /// Check if this entry is a hard link
    pub fn is_hardlink(&self) -> bool {
        self.entry_type == EntryType::HardLink
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ArchiveEntry::new defaults ──

    #[test]
    fn test_new_entry_defaults() {
        let entry = ArchiveEntry::new("docs/readme.md".into(), 0);
        assert_eq!(entry.path, "docs/readme.md");
        assert_eq!(entry.id, 0);
        assert_eq!(entry.entry_type, EntryType::File);
        assert!(entry.size.is_none());
        assert!(entry.compressed_size.is_none());
        assert!(entry.modified.is_none());
        assert!(entry.crc32.is_none());
        assert!(entry.permissions.is_none());
        assert!(entry.created.is_none());
        assert!(entry.accessed.is_none());
        assert!(entry.compression_fraction().is_none());
        assert!(!entry.is_encrypted);
        assert!(entry.comment.is_none());
        assert!(entry.attributes.is_none());
    }

    #[test]
    fn test_new_entry_with_id() {
        let entry = ArchiveEntry::new("file.txt".into(), 42);
        assert_eq!(entry.id, 42);
    }

    // ── Type checks ──

    #[test]
    fn test_is_file() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        assert!(entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());

        entry.entry_type = EntryType::Directory;
        assert!(!entry.is_file());
    }

    #[test]
    fn test_is_directory() {
        let mut entry = ArchiveEntry::new("folder/".into(), 0);
        entry.entry_type = EntryType::Directory;
        assert!(entry.is_directory());
        assert!(!entry.is_file());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());
    }

    #[test]
    fn test_is_symlink() {
        let mut entry = ArchiveEntry::new("link".into(), 0);
        entry.entry_type = EntryType::Symlink;
        assert!(entry.is_symlink());
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_hardlink());
    }

    #[test]
    fn test_is_hardlink() {
        let mut entry = ArchiveEntry::new("hlink".into(), 0);
        entry.entry_type = EntryType::HardLink;
        assert!(entry.is_hardlink());
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
    }

    #[test]
    fn test_other_entry_type() {
        let mut entry = ArchiveEntry::new("special".into(), 0);
        entry.entry_type = EntryType::Other;
        assert!(!entry.is_file());
        assert!(!entry.is_directory());
        assert!(!entry.is_symlink());
        assert!(!entry.is_hardlink());
    }

    // ── Compression ratio ──

    #[test]
    fn test_compression_ratio_normal() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(1000);
        entry.compressed_size = Some(400);
        let ratio = entry.compression_fraction().unwrap();
        assert!((ratio - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compression_ratio_store() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(500);
        entry.compressed_size = Some(500);
        let ratio = entry.compression_fraction().unwrap();
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compression_ratio_zero_original() {
        let mut entry = ArchiveEntry::new("empty.txt".into(), 0);
        entry.size = Some(0);
        entry.compressed_size = Some(0);
        // Division by zero protection: should remain None
        assert!(entry.compression_fraction().is_none());
    }

    #[test]
    fn test_compression_ratio_missing_compressed() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        entry.size = Some(1000);
        // compressed_size is None
        assert!(entry.compression_fraction().is_none());
    }

    #[test]
    fn test_compression_ratio_missing_original() {
        let mut entry = ArchiveEntry::new("data.bin".into(), 0);
        // size is None
        entry.compressed_size = Some(400);
        assert!(entry.compression_fraction().is_none());
    }

    #[test]
    fn test_compression_ratio_both_missing() {
        let entry = ArchiveEntry::new("data.bin".into(), 0);
        assert!(entry.compression_fraction().is_none());
    }

    #[test]
    fn test_expansion_ratio_none_is_ambiguous() {
        // R0001-0045: `None` covers both "no metadata" and "nonempty
        // entry declaring zero compressed bytes", which the security
        // layer refuses as an undefined ratio (R0076-0002). The two
        // states are indistinguishable through this helper, which is why
        // its rustdoc tells callers not to use it as the bomb signal.
        let unknown = ArchiveEntry::new("unknown.bin".into(), 0);
        assert!(unknown.expansion_ratio().is_none());

        let mut suspicious = ArchiveEntry::new("bomb.bin".into(), 1);
        suspicious.size = Some(10 * 1024 * 1024);
        suspicious.compressed_size = Some(0);
        assert!(
            suspicious.expansion_ratio().is_none(),
            "nonempty content with zero compressed bytes has no defined ratio"
        );

        // A real ratio is still reported whenever the denominator is
        // usable, so the ambiguity is confined to the `None` arm.
        let mut known = ArchiveEntry::new("data.bin".into(), 2);
        known.size = Some(1000);
        known.compressed_size = Some(250);
        assert!((known.expansion_ratio().unwrap() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compression_ratio_larger_compressed() {
        // Incompressible data can have ratio > 1.0
        let mut entry = ArchiveEntry::new("random.bin".into(), 0);
        entry.size = Some(100);
        entry.compressed_size = Some(120);
        let ratio = entry.compression_fraction().unwrap();
        assert!((ratio - 1.2).abs() < f64::EPSILON);
    }

    // ── EntryType traits ──

    #[test]
    fn test_entry_type_clone_copy() {
        let t = EntryType::File;
        let copied = t; // Copy
        assert_eq!(t, copied);
    }

    #[test]
    fn test_entry_type_debug() {
        assert_eq!(format!("{:?}", EntryType::File), "File");
        assert_eq!(format!("{:?}", EntryType::Directory), "Directory");
        assert_eq!(format!("{:?}", EntryType::Symlink), "Symlink");
        assert_eq!(format!("{:?}", EntryType::HardLink), "HardLink");
        assert_eq!(format!("{:?}", EntryType::Other), "Other");
    }

    #[test]
    fn test_entry_type_equality() {
        assert_eq!(EntryType::File, EntryType::File);
        assert_ne!(EntryType::File, EntryType::Directory);
        assert_ne!(EntryType::Symlink, EntryType::HardLink);
    }

    // ── FileAttributes ──

    #[test]
    fn test_file_attributes_default() {
        let attrs = FileAttributes::default();
        assert!(attrs.windows.is_none());
        assert!(attrs.unix_xattr.is_none());
        assert!(attrs.archive_specific.is_none());
    }

    #[test]
    fn test_file_attributes_with_values() {
        let attrs = FileAttributes {
            windows: Some(0x20), // FILE_ATTRIBUTE_ARCHIVE
            unix_xattr: Some(vec![("user.mime_type".into(), b"text/plain".to_vec())]),
            archive_specific: Some("solid".into()),
        };
        assert_eq!(attrs.windows, Some(0x20));
        assert_eq!(attrs.unix_xattr.as_ref().unwrap().len(), 1);
        assert_eq!(attrs.archive_specific.as_deref(), Some("solid"));
    }

    #[test]
    fn test_file_attributes_clone() {
        let attrs = FileAttributes {
            windows: Some(0x01),
            unix_xattr: None,
            archive_specific: Some("test".into()),
        };
        let cloned = attrs.clone();
        assert_eq!(cloned.windows, Some(0x01));
        assert_eq!(cloned.archive_specific.as_deref(), Some("test"));
    }

    // ── ArchiveEntry clone ──

    #[test]
    fn test_archive_entry_clone() {
        let mut entry = ArchiveEntry::new("file.txt".into(), 5);
        entry.size = Some(1024);
        entry.crc32 = Some(0xDEADBEEF);
        entry.is_encrypted = true;
        entry.comment = Some("test comment".into());
        entry.entry_type = EntryType::File;

        let cloned = entry.clone();
        assert_eq!(cloned.path, "file.txt");
        assert_eq!(cloned.id, 5);
        assert_eq!(cloned.size, Some(1024));
        assert_eq!(cloned.crc32, Some(0xDEADBEEF));
        assert!(cloned.is_encrypted);
        assert_eq!(cloned.comment.as_deref(), Some("test comment"));
    }

    // ── Edge cases ──

    #[test]
    fn test_entry_with_empty_path() {
        let entry = ArchiveEntry::new(String::new(), 0);
        assert_eq!(entry.path, "");
    }

    #[test]
    fn test_try_dir_at_rejects_empty_path() {
        assert!(
            ArchiveEntry::try_dir_at("", 0).is_err(),
            "empty path must be rejected by try_dir_at"
        );
    }

    #[test]
    fn test_try_dir_at_accepts_non_empty_path() {
        let entry = ArchiveEntry::try_dir_at("d", 3)
            .expect("non-empty path must be accepted")
            .build();
        assert_eq!(entry.path, "d");
        assert_eq!(entry.id, 3);
        assert_eq!(entry.entry_type, EntryType::Directory);
    }

    #[test]
    fn test_entry_with_unicode_path() {
        let entry = ArchiveEntry::new("日本語/ファイル.txt".into(), 0);
        assert_eq!(entry.path, "日本語/ファイル.txt");
    }

    #[test]
    fn test_entry_with_large_id() {
        let entry = ArchiveEntry::new("file.txt".into(), usize::MAX);
        assert_eq!(entry.id, usize::MAX);
    }

    #[test]
    fn test_entry_with_large_sizes() {
        let mut entry = ArchiveEntry::new("huge.bin".into(), 0);
        entry.size = Some(u64::MAX);
        entry.compressed_size = Some(u64::MAX);
        // u64::MAX / u64::MAX = 1.0
        let ratio = entry.compression_fraction().unwrap();
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }
}
