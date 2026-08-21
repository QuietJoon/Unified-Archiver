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
///
/// # Reading is the common case; construction goes through the builder
///
/// Every listing call *returns* `ArchiveEntry` values, so the type is read
/// far more often than it is built. Both shapes are supported, and they
/// are deliberately asymmetric (OI-0076-005):
///
/// - **Reading** stays ergonomic. Each field is public *and* has a
///   same-named accessor ([`path`](method@Self::path),
///   [`size`](method@Self::size), [`entry_type`](method@Self::entry_type),
///   …). New code should prefer the accessors: they are the shape that
///   survives the field demotion this type is queued for (see below), so
///   a call site written against `entry.size()` needs no edit when
///   `entry.size` stops being public.
/// - **Construction** is narrowing. The struct is `#[non_exhaustive]`, so
///   downstream crates can no longer build it with a struct literal or a
///   `..spread`; use [`ArchiveEntry::file`], [`ArchiveEntry::dir_at`],
///   [`ArchiveEntry::symlink_at`], [`ArchiveEntry::hardlink_at`] (or their
///   `try_*` counterparts) and finish with
///   [`ArchiveEntryBuilder::build`] / [`ArchiveEntryBuilder::build_checked`].
///
/// # Why construction is being funnelled
///
/// The public fields let a caller assemble an entry that contradicts its
/// own `entry_type`: a `Directory` carrying a `size`, a `Symlink` with no
/// `link_target`, a `File` that has one. Nothing rejects those today, and
/// the backends never produce them, so the malformed shapes only ever
/// arrive from caller-built values. Routing construction through the
/// builder gives the invariant a single place to live —
/// [`ArchiveEntryBuilder::build_checked`] enforces it now, and the
/// unchecked [`ArchiveEntryBuilder::build`] is what keeps existing call
/// sites compiling meanwhile (tracked as `ArchiveEntryBuilder can
/// construct entries that violate entry-kind invariants`).
///
/// **Field demotion is queued for 0.5.0.** The fields whose *mutation*
/// can break the entry-kind invariant — `size`, `compressed_size`,
/// `entry_type`, `link_target`, `path`, `id` — become non-`pub` then, read
/// through the accessors. The purely descriptive ones (`modified`,
/// `created`, `accessed`, `crc32`, `permissions`, `is_encrypted`,
/// `comment`, `attributes`, `raw_path`) carry no cross-field invariant, so
/// they are demoted for uniformity only and reading them through the
/// accessors is all a caller needs to do to be ready.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// [`Self::attributes`](field@Self::attributes)`.windows`.
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

    /// Finalize the builder into an [`ArchiveEntry`] without checking
    /// cross-field consistency.
    ///
    /// Kept unchecked so existing call sites keep compiling and keep
    /// behaving; use [`Self::build_checked`] when you want the entry-kind
    /// invariants enforced (OI-0076-005).
    pub fn build(self) -> ArchiveEntry {
        self.inner
    }

    /// Finalize the builder, rejecting an entry that contradicts its own
    /// [`EntryType`] (OI-0076-005).
    ///
    /// This is the single place the entry-kind invariant lives. The rules
    /// are the ones every backend already satisfies, stated so a
    /// caller-built entry can be held to them too:
    ///
    /// - the path must not be empty;
    /// - [`EntryType::Directory`] carries no `size`, no
    ///   `compressed_size` and no `link_target`;
    /// - [`EntryType::Symlink`] and [`EntryType::HardLink`] must carry a
    ///   `link_target`;
    /// - [`EntryType::File`] carries no `link_target`.
    ///
    /// [`EntryType::Other`] is unconstrained — it exists precisely for
    /// entry kinds this crate does not model, so there is no invariant to
    /// assert about it.
    ///
    /// # Errors
    ///
    /// [`ArchiveError::InvalidPath`](crate::ArchiveError::InvalidPath)
    /// naming the field combination that was refused.
    ///
    /// ```
    /// use unified_archive::ArchiveEntry;
    ///
    /// // A directory with a size contradicts its own kind.
    /// assert!(ArchiveEntry::dir_at("d/", 0).size(10).build_checked().is_err());
    /// // A symlink built through the typed entry point always has a target.
    /// assert!(ArchiveEntry::symlink_at("l", 1, "t").build_checked().is_ok());
    /// ```
    pub fn build_checked(self) -> std::result::Result<ArchiveEntry, crate::ArchiveError> {
        match entry_kind_violation(&self.inner) {
            None => Ok(self.inner),
            Some(reason) => Err(crate::ArchiveError::invalid_path(
                self.inner.path.clone(),
                reason,
            )),
        }
    }
}

/// Describe how `entry` contradicts its own [`EntryType`], or `None` when
/// the combination is consistent. Backing check for
/// [`ArchiveEntryBuilder::build_checked`].
fn entry_kind_violation(entry: &ArchiveEntry) -> Option<&'static str> {
    if entry.path.is_empty() {
        return Some("ArchiveEntry path must not be empty");
    }
    match entry.entry_type {
        EntryType::Directory => {
            if entry.size.is_some() {
                Some("a Directory entry must not carry a size")
            } else if entry.compressed_size.is_some() {
                Some("a Directory entry must not carry a compressed_size")
            } else if entry.link_target.is_some() {
                Some("a Directory entry must not carry a link_target")
            } else {
                None
            }
        }
        EntryType::Symlink | EntryType::HardLink => {
            if entry.link_target.is_none() {
                Some("a Symlink / HardLink entry must carry a link_target")
            } else {
                None
            }
        }
        EntryType::File => {
            if entry.link_target.is_some() {
                Some("a File entry must not carry a link_target")
            } else {
                None
            }
        }
        // `Other` is the escape hatch for entry kinds this crate does not
        // model (devices, FIFOs, sockets); there is no invariant to assert.
        _ => None,
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
    /// ([`Self::file`], [`Self::dir_at`], [`Self::symlink_at`],
    /// [`Self::hardlink_at`], [`Self::new`], [`Self::directory`]) share
    /// one policy: they assume the caller supplies a non-empty path, and
    /// the `try_*` variants are the single gate that enforces the
    /// empty-path invariant (R0081-0004). [`ArchiveEntryBuilder::build_checked`]
    /// re-checks it at the other end for callers who took the infallible
    /// route.
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

    /// Begin building a symlink-typed entry (OI-0076-005).
    ///
    /// Takes the target up front, so `entry_type == Symlink` with
    /// `link_target == None` is unrepresentable through this path — the
    /// invariant [`ArchiveEntryBuilder::build_checked`] enforces is
    /// satisfied by construction. Path is not validated; use
    /// [`Self::try_symlink_at`] for the empty-path check.
    pub fn symlink_at(
        path: impl Into<String>,
        id: usize,
        target: impl Into<String>,
    ) -> ArchiveEntryBuilder {
        let mut inner = Self::with_type(path.into(), id, EntryType::Symlink);
        inner.link_target = Some(target.into());
        ArchiveEntryBuilder { inner }
    }

    /// Begin building a symlink-typed entry, rejecting empty paths.
    /// Fallible counterpart to [`Self::symlink_at`].
    ///
    /// # Errors
    ///
    /// [`ArchiveError::InvalidPath`](crate::ArchiveError::InvalidPath)
    /// when `path` is empty.
    pub fn try_symlink_at(
        path: impl Into<String>,
        id: usize,
        target: impl Into<String>,
    ) -> std::result::Result<ArchiveEntryBuilder, crate::ArchiveError> {
        let path = path.into();
        if path.is_empty() {
            return Err(crate::ArchiveError::invalid_path(
                "",
                "ArchiveEntry path must not be empty",
            ));
        }
        Ok(Self::symlink_at(path, id, target))
    }

    /// Begin building a hardlink-typed entry (OI-0076-005). Mirrors
    /// [`Self::symlink_at`]: the target is required at construction, so
    /// the kind invariant cannot be violated through this path.
    pub fn hardlink_at(
        path: impl Into<String>,
        id: usize,
        target: impl Into<String>,
    ) -> ArchiveEntryBuilder {
        let mut inner = Self::with_type(path.into(), id, EntryType::HardLink);
        inner.link_target = Some(target.into());
        ArchiveEntryBuilder { inner }
    }

    /// Begin building a hardlink-typed entry, rejecting empty paths.
    /// Fallible counterpart to [`Self::hardlink_at`].
    ///
    /// # Errors
    ///
    /// [`ArchiveError::InvalidPath`](crate::ArchiveError::InvalidPath)
    /// when `path` is empty.
    pub fn try_hardlink_at(
        path: impl Into<String>,
        id: usize,
        target: impl Into<String>,
    ) -> std::result::Result<ArchiveEntryBuilder, crate::ArchiveError> {
        let path = path.into();
        if path.is_empty() {
            return Err(crate::ArchiveError::invalid_path(
                "",
                "ArchiveEntry path must not be empty",
            ));
        }
        Ok(Self::hardlink_at(path, id, target))
    }

    /// Create a new file-typed archive entry. Prefer
    /// [`ArchiveEntry::file`] for new code (R0075-0078) — the builder
    /// API allows fluent setter chains and rejects empty paths via
    /// [`ArchiveEntry::try_file`].
    ///
    /// # Deprecation timeline (OI-0076-005)
    ///
    /// - **0.4.x** — kept, un-attributed. The crate itself still calls
    ///   this from four backend readers and three integration tests; a
    ///   `#[deprecated]` attribute would emit in-crate warnings, and this
    ///   crate's gate is warning-free. The attribute lands in the same
    ///   change that migrates those call sites.
    /// - **0.5.0** — gains `#[deprecated]`.
    /// - **0.6.0** — removed. Migrate to
    ///   `ArchiveEntry::file(path, id).build()`, which takes
    ///   `impl Into<String>` and so accepts everything this does.
    pub fn new(path: String, id: usize) -> Self {
        Self::with_type(path, id, EntryType::File)
    }

    /// Create a new directory-typed archive entry. Prefer
    /// [`ArchiveEntry::dir_at`] for new code (R0075-0078). Same
    /// deprecation timeline as [`Self::new`]: `#[deprecated]` in 0.5.0,
    /// removed in 0.6.0, replaced by
    /// `ArchiveEntry::dir_at(path, id).build()`.
    pub fn directory(path: String, id: usize) -> Self {
        Self::with_type(path, id, EntryType::Directory)
    }

    /// Construct a symlink-typed entry with the supplied link target.
    /// Stamps `link_target` directly so callers don't have to set the
    /// field after the fact, which would otherwise leave a window
    /// where `entry_type == Symlink` but `link_target == None`.
    ///
    /// # Deprecated in favour of the builder entry point
    ///
    /// [`Self::symlink_at`] gives the same guarantee and returns an
    /// [`ArchiveEntryBuilder`], so the remaining metadata can be set in
    /// the same expression instead of by field assignment afterwards —
    /// which is the mutation path OI-0076-005 is closing. Behaviour is
    /// identical; only the return type differs.
    #[deprecated(
        since = "0.5.0",
        note = "use ArchiveEntry::symlink_at(path, id, target).build(), which returns a builder so metadata needs no field assignment (OI-0076-005); removed in 0.6.0"
    )]
    pub fn symlink(path: String, id: usize, target: String) -> Self {
        Self::symlink_at(path, id, target).build()
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

    /// The entry's path within the archive (UTF-8, forward slashes).
    ///
    /// Accessor for [`Self::path`](field@Self::path); see the type-level docs for why new
    /// code should prefer it over the field.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Sequential 0-based index of this entry within the archive.
    /// Accessor for [`Self::id`](field@Self::id).
    pub fn id(&self) -> usize {
        self.id
    }

    /// Uncompressed size in bytes, `None` for directories and for
    /// backends that do not report it. Accessor for [`Self::size`](field@Self::size).
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// Compressed size in bytes. Accessor for [`Self::compressed_size`](field@Self::compressed_size).
    pub fn compressed_size(&self) -> Option<u64> {
        self.compressed_size
    }

    /// Last-modification time. Accessor for [`Self::modified`](field@Self::modified).
    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }

    /// Creation time. Accessor for [`Self::created`](field@Self::created).
    pub fn created(&self) -> Option<SystemTime> {
        self.created
    }

    /// Last-access time. Accessor for [`Self::accessed`](field@Self::accessed).
    pub fn accessed(&self) -> Option<SystemTime> {
        self.accessed
    }

    /// CRC32 checksum when the format carries one. Accessor for
    /// [`Self::crc32`](field@Self::crc32).
    pub fn crc32(&self) -> Option<u32> {
        self.crc32
    }

    /// The entry's kind. Accessor for [`Self::entry_type`](field@Self::entry_type); the
    /// [`is_file`](Self::is_file) / [`is_directory`](Self::is_directory)
    /// family remains the terser way to ask about one kind.
    pub fn entry_type(&self) -> EntryType {
        self.entry_type
    }

    /// Unix permission bits (`mode & 0o7777`) when known. Accessor for
    /// [`Self::permissions`](field@Self::permissions), including its
    /// `permissions & !0o7777 == 0` contract.
    pub fn permissions(&self) -> Option<u32> {
        self.permissions
    }

    /// Whether the entry is password-protected. Accessor for
    /// [`Self::is_encrypted`](field@Self::is_encrypted).
    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
    }

    /// Per-entry comment when the format carries one. Accessor for
    /// [`Self::comment`](field@Self::comment).
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    /// Platform-specific attributes. Accessor for [`Self::attributes`](field@Self::attributes);
    /// see [`FileAttributes`] for the per-backend coverage table.
    pub fn attributes(&self) -> Option<&FileAttributes> {
        self.attributes.as_ref()
    }

    /// Raw (possibly non-UTF-8) entry name bytes. Accessor for
    /// [`Self::raw_path`](field@Self::raw_path); `None` means [`Self::path`](field@Self::path) is byte-exact or
    /// the backend does not surface raw names.
    pub fn raw_path(&self) -> Option<&[u8]> {
        self.raw_path.as_deref()
    }

    /// Symlink / hardlink target. Accessor for [`Self::link_target`](field@Self::link_target).
    pub fn link_target(&self) -> Option<&str> {
        self.link_target.as_deref()
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

    // ── OI-0076-005: accessors mirror the fields ──

    #[test]
    fn accessors_agree_with_the_fields_they_mirror() {
        let now = SystemTime::now();
        let entry = ArchiveEntry::file("dir/f.bin", 9)
            .size(1000)
            .compressed_size(250)
            .modified(now)
            .created(now)
            .accessed(now)
            .crc32(0xDEAD_BEEF)
            .permissions(0o7777 | 0o40000)
            .comment("note".into())
            .encrypted(true)
            .raw_path(vec![0xFF, 0xFE])
            .attributes(FileAttributes {
                windows: Some(0x20),
                ..FileAttributes::default()
            })
            .build();

        assert_eq!(entry.path(), entry.path);
        assert_eq!(entry.id(), entry.id);
        assert_eq!(entry.size(), entry.size);
        assert_eq!(entry.compressed_size(), entry.compressed_size);
        assert_eq!(entry.modified(), entry.modified);
        assert_eq!(entry.created(), entry.created);
        assert_eq!(entry.accessed(), entry.accessed);
        assert_eq!(entry.crc32(), entry.crc32);
        assert_eq!(entry.entry_type(), entry.entry_type);
        assert_eq!(entry.permissions(), entry.permissions);
        assert_eq!(entry.is_encrypted(), entry.is_encrypted);
        assert_eq!(entry.comment(), entry.comment.as_deref());
        assert_eq!(entry.raw_path(), entry.raw_path.as_deref());
        assert_eq!(entry.link_target(), entry.link_target.as_deref());
        assert!(entry.attributes().is_some());
        assert_eq!(
            entry.attributes().and_then(|a| a.windows),
            entry.attributes.as_ref().and_then(|a| a.windows)
        );

        // The builder's permission mask is what the accessor reports, so
        // the `permissions & !0o7777 == 0` contract holds through it too.
        assert_eq!(entry.permissions(), Some(0o7777));
    }

    #[test]
    fn accessors_report_none_on_a_bare_entry() {
        let entry = ArchiveEntry::dir_at("d/", 0).build();
        assert_eq!(entry.path(), "d/");
        assert_eq!(entry.id(), 0);
        assert_eq!(entry.entry_type(), EntryType::Directory);
        assert!(entry.size().is_none());
        assert!(entry.compressed_size().is_none());
        assert!(entry.modified().is_none());
        assert!(entry.created().is_none());
        assert!(entry.accessed().is_none());
        assert!(entry.crc32().is_none());
        assert!(entry.permissions().is_none());
        assert!(entry.comment().is_none());
        assert!(entry.attributes().is_none());
        assert!(entry.raw_path().is_none());
        assert!(entry.link_target().is_none());
        assert!(!entry.is_encrypted());
    }

    // ── OI-0076-005: typed link builders ──

    #[test]
    fn symlink_at_stamps_the_target_at_construction() {
        let entry = ArchiveEntry::symlink_at("link", 4, "target/file").build();
        assert_eq!(entry.entry_type(), EntryType::Symlink);
        assert_eq!(entry.link_target(), Some("target/file"));
        assert_eq!(entry.path(), "link");
        assert_eq!(entry.id(), 4);
    }

    #[test]
    fn hardlink_at_stamps_the_target_at_construction() {
        let entry = ArchiveEntry::hardlink_at("hl", 5, "orig").build();
        assert_eq!(entry.entry_type(), EntryType::HardLink);
        assert_eq!(entry.link_target(), Some("orig"));
    }

    #[test]
    fn try_link_builders_reject_empty_paths() {
        assert!(ArchiveEntry::try_symlink_at("", 0, "t").is_err());
        assert!(ArchiveEntry::try_hardlink_at("", 0, "t").is_err());
        assert!(
            ArchiveEntry::try_symlink_at("l", 0, "t")
                .expect("non-empty path accepted")
                .build_checked()
                .is_ok()
        );
        assert!(
            ArchiveEntry::try_hardlink_at("h", 0, "t")
                .expect("non-empty path accepted")
                .build_checked()
                .is_ok()
        );
    }

    #[test]
    #[allow(deprecated)] // the deprecated path must keep behaving until removal
    fn deprecated_symlink_constructor_matches_symlink_at() {
        let legacy = ArchiveEntry::symlink("l".into(), 2, "t".into());
        let builder = ArchiveEntry::symlink_at("l", 2, "t").build();
        assert_eq!(legacy.entry_type(), builder.entry_type());
        assert_eq!(legacy.link_target(), builder.link_target());
        assert_eq!(legacy.path(), builder.path());
        assert_eq!(legacy.id(), builder.id());
    }

    // ── OI-0076-005: build_checked enforces the entry-kind invariants ──

    #[test]
    fn build_checked_accepts_consistent_entries() {
        assert!(
            ArchiveEntry::file("f", 0)
                .size(10)
                .compressed_size(4)
                .build_checked()
                .is_ok()
        );
        assert!(ArchiveEntry::dir_at("d/", 1).build_checked().is_ok());
        assert!(
            ArchiveEntry::symlink_at("l", 2, "t")
                .build_checked()
                .is_ok()
        );
        assert!(
            ArchiveEntry::hardlink_at("h", 3, "t")
                .build_checked()
                .is_ok()
        );
    }

    #[test]
    fn build_checked_rejects_a_sized_directory() {
        let err = ArchiveEntry::dir_at("d/", 0)
            .size(10)
            .build_checked()
            .expect_err("a Directory with a size must be refused");
        assert!(
            matches!(err, crate::ArchiveError::InvalidPath { .. }),
            "{err:?}"
        );
        assert!(
            ArchiveEntry::dir_at("d/", 0)
                .compressed_size(3)
                .build_checked()
                .is_err()
        );
        assert!(
            ArchiveEntry::dir_at("d/", 0)
                .link_target("t".into())
                .build_checked()
                .is_err()
        );
    }

    #[test]
    fn build_checked_rejects_a_file_carrying_a_link_target() {
        assert!(
            ArchiveEntry::file("f", 0)
                .link_target("t".into())
                .build_checked()
                .is_err()
        );
    }

    #[test]
    fn build_checked_rejects_an_empty_path() {
        assert!(ArchiveEntry::file("", 0).build_checked().is_err());
        assert!(ArchiveEntry::dir_at("", 0).build_checked().is_err());
    }

    #[test]
    fn build_stays_unchecked_so_existing_call_sites_keep_behaving() {
        // `build` is deliberately not a checked path (OI-0076-005): the
        // migration is additive, so the shape that violates the invariant
        // must still be constructible until callers have moved.
        let bad = ArchiveEntry::dir_at("d/", 0).size(10).build();
        assert_eq!(bad.size(), Some(10));
        assert!(bad.is_directory());
    }

    #[test]
    fn build_checked_ignores_the_other_entry_type() {
        // `Other` models kinds this crate does not describe, so there is
        // no invariant to assert — reachable only through the fields.
        let mut entry = ArchiveEntry::file("special", 0).size(1).build();
        entry.entry_type = EntryType::Other;
        entry.link_target = Some("whatever".into());
        assert!(entry_kind_violation(&entry).is_none());
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
