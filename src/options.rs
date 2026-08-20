//! Configuration options for archive operations

use crate::ArchiveFormat;
use crate::entry::ArchiveEntry;
use crate::password::Password;
use crate::security::ExtractionLimits;
use std::path::PathBuf;

/// Entry filter type alias for filtering archive entries.
///
/// Filters are invoked synchronously during selective extraction, so the
/// trait object only needs to be `Send` (R0070-0071). The earlier
/// `Send + Sync` bound rejected reasonable closures whose captures
/// were `Send` but not `Sync` (e.g. `Cell`-bearing accumulators).
///
/// **`FnMut` (R0075-0080).** The bound is `FnMut`, not `Fn`, so
/// stateful closures (counters, accumulators, dedup tables) work
/// without interior-mutability shims. Plain `Fn` closures still
/// satisfy `FnMut`, so existing call sites that boxed an `Fn` closure
/// keep compiling — but `Box<dyn Fn>` itself does not coerce to
/// `Box<dyn FnMut>`, so callers building the filter through an
/// intermediate alias must rebox. Use [`entry_filter_from_fn`] when
/// you have a function-pointer or known-`Fn` closure and want it
/// lifted into `EntryFilter` without the `Box::new` ceremony.
pub type EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>;

/// Helper: lift an `Fn`-bounded closure into the `FnMut`-typed
/// [`EntryFilter`] (R0075-0080 source-compat shim).
pub fn entry_filter_from_fn<F>(f: F) -> EntryFilter
where
    F: Fn(&ArchiveEntry) -> bool + Send + 'static,
{
    Box::new(f)
}

/// Configuration for archive extraction operations
pub struct ExtractionOptions {
    /// Destination directory
    pub destination: PathBuf,

    /// Password for encrypted archives
    pub password: Option<Password>,

    /// Overwrite existing files (default: false, fails with error if files exist)
    pub overwrite: bool,

    /// Preserve file permissions (Unix mode bits) on extracted files
    /// (R0079-0019).
    ///
    /// Per-backend behaviour:
    /// - **ZIP**: Unix mode bits from the central directory are
    ///   applied (masked to the permission bits) when the entry was
    ///   written by a Unix-style host; entries without Unix metadata
    ///   keep the staging file's default mode.
    /// - **7z**: applied when the archive stores a Unix mode in the
    ///   attribute field's upper half (the p7zip convention).
    /// - **RAR / RAR5**: honoured as an opt-out (R0001-0046). The UnRAR
    ///   library stamps the entry's mode on the file it writes, so
    ///   `false` is applied *after* that write: the staged file keeps the
    ///   extractor's own `0o600` staging mode instead of the archived
    ///   one (R0080-0023 / R0081-0067). `true` restores the mode UnRAR
    ///   applied. Unix only — on other platforms the flag makes no
    ///   difference for this backend.
    /// - **Libarchive-backed (TAR family, ISO, raw compressed
    ///   streams)**: honoured via `ARCHIVE_EXTRACT_PERM`.
    ///
    /// No effect on non-Unix platforms for the native Rust backends
    /// (ZIP / 7z).
    pub preserve_permissions: bool,

    /// Preserve modification times on extracted files (R0079-0019).
    ///
    /// Per-backend behaviour:
    /// - **ZIP**: the DOS-precision (2-second) modification time
    ///   is applied to each extracted file.
    /// - **7z**: the NT-time modification timestamp is applied when the
    ///   archive carries one.
    /// - **RAR / RAR5**: honoured as an opt-out (R0001-0046). The UnRAR
    ///   library stamps the archived modification time on the file it
    ///   writes, so `false` overrides it with the current time after the
    ///   staged write (R0080-0023 / R0081-0067); `true` keeps the
    ///   archived timestamp.
    /// - **Libarchive-backed (TAR family, ISO, raw compressed
    ///   streams)**: honoured via `ARCHIVE_EXTRACT_TIME`.
    ///
    /// Only the modification time is restored by the native Rust
    /// backends; accessed/created times are listing-only metadata
    /// (OI-0065-002).
    pub preserve_times: bool,

    /// Verify CRC32 checksums during extraction (AD 0062 A.3).
    ///
    /// Per-backend behaviour:
    /// - **ZIP**: explicit per-entry CRC32 verification is honoured.
    /// - **7z**: CRC32 always validates regardless of this flag (the
    ///   codec checks integrity unconditionally); set to `false` to
    ///   express "I do not require CRC32" without changing behaviour.
    /// - **RAR / RAR5**: same as 7z — CRC32 always validates.
    /// - **Libarchive-backed (TAR family, ISO, raw compressed streams)**:
    ///   no per-entry CRC32 is surfaced; `verify_crc32 = true` returns
    ///   [`ArchiveError::Unsupported`](crate::ArchiveError::Unsupported). The wrapping codec's built-in
    ///   integrity check still runs.
    pub verify_crc32: bool,

    /// Resource limits for extraction (zip bomb protection)
    pub limits: ExtractionLimits,

    /// Filter: only extract matching paths
    pub filter: Option<EntryFilter>,

    /// Progress callback
    pub progress: Option<Box<dyn ProgressCallback>>,
}

impl ExtractionOptions {
    /// Set the password used to open an encrypted archive during
    /// extraction, consuming and returning `self` for chaining off
    /// [`ExtractionOptions::default`].
    ///
    /// The password is stored as a [`Password`], which is UTF-8 by
    /// construction and redacts itself in `Debug`/`Display`. Prefer this
    /// over assigning the public `password` field directly.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(Password::new(password));
        self
    }
}

impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            destination: PathBuf::from("."),
            password: None,
            overwrite: false,
            preserve_permissions: true,
            preserve_times: true,
            // AD 0062 A.3: default `false`; see the `verify_crc32`
            // field rustdoc for per-backend semantics.
            verify_crc32: false,
            limits: ExtractionLimits::default(),
            filter: None,
            progress: None,
        }
    }
}

/// Compression level for archive creation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    Store,
    Fastest,
    Fast,
    Normal,
    Maximum,
    Ultra,
}

/// Configuration for archive creation operations.
///
/// Does not implement [`Clone`] — the `progress` field is a trait
/// object that cannot be deep-cloned. Use
/// [`CompressionOptions::strip_progress`] to obtain a fresh value
/// without progress reporting (R0070-0068).
///
/// **Field-vs-format compatibility.** The struct is a
/// flat bag of fields shared across formats. Not every combination
/// is honoured — for example `password` is rejected at
/// [`crate::Archive::create`] for every format (MADR-0027) and
/// `split_size` is currently unused (DEF-002). Callers can sanity-check
/// their configuration with [`CompressionOptions::validate_for_format`]
/// before invoking `Archive::create`; see that method for the exact
/// per-format compatibility rules.
///
/// R0076-0095: NOT marked `#[non_exhaustive]` because R0075-0081 explicitly
/// preserves struct-literal source-compat for v0.3 — examples and v0.3
/// callers construct this via the public-fields shape. Tightening the
/// envelope is queued behind the v0.4 deprecation cycle alongside the
/// format-specific builders (`ZipCompressionOptions` etc., already
/// landed) becoming the primary API. Tracked under OI-0076-005.
pub struct CompressionOptions {
    /// Output archive format
    pub format: ArchiveFormat,

    /// Compression level
    pub level: CompressionLevel,

    /// Password for encryption.
    ///
    /// **Currently always rejected by [`crate::Archive::create`]** (per
    /// MADR-0027 / R0070-0069 — encrypted archive creation is deferred
    /// behind an explicit opt-in that has not shipped). Setting this
    /// field has no effect today; it is kept on the type so callers
    /// building `CompressionOptions` from a struct literal don't have to
    /// learn a new shape when encrypted creation lands.
    pub password: Option<Password>,

    /// Split archive into parts (bytes per part)
    pub split_size: Option<u64>,

    /// Progress callback
    pub progress: Option<Box<dyn ProgressCallback>>,
}

// `CompressionOptions` deliberately does NOT implement `Clone`. The
// `progress` field holds a `Box<dyn ProgressCallback>` which cannot be
// cloned without silently dropping it, and the prior `Clone` impl did
// exactly that — a cloned options value stopped emitting progress without
// the caller noticing. Callers that need a fresh options
// value without progress reporting can use `strip_progress`, which makes
// the intent explicit.
impl CompressionOptions {
    /// Return a copy of these options with `progress` cleared.
    ///
    /// Use this when you need to hand options to another operation that
    /// shouldn't receive progress events. The original callback remains
    /// on `self` unless you assign the returned value back.
    pub fn strip_progress(&self) -> Self {
        Self {
            format: self.format,
            level: self.level,
            password: self.password.clone(),
            split_size: self.split_size,
            progress: None,
        }
    }

    /// Validate that this configuration's `format` field is compatible
    /// with the other settings.
    ///
    /// Reports an [`ArchiveError::OperationBlocked`](crate::ArchiveError::OperationBlocked) when the
    /// configuration combines a format with a field that is
    /// unsupported, out-of-scope, or feature-gated. Today the rules
    /// are:
    ///
    /// - `password` with **any** format is rejected (MADR-0027 —
    ///   encrypted archive creation is deferred behind an explicit
    ///   opt-in that has not shipped).
    /// - `split_size` set to `Some(_)` is rejected for every format
    ///   because end-to-end split-archive creation is not yet
    ///   implemented (DEF-002).
    /// - `format` itself is checked against
    ///   [`ArchiveFormat::can_create`]; non-creatable formats are
    ///   rejected with the same error
    ///   [`Archive::create`](crate::Archive::create) would surface.
    ///
    /// Calling this is optional — `Archive::create` performs the same
    /// gates internally. The method exists so callers can preflight
    /// before constructing the writer (e.g. UI confirmation flows
    /// that want to validate the configuration without touching the
    /// filesystem).
    ///
    /// **Prefer per-format builders for new code.** When the target
    /// format is known up front, the typed builders
    /// [`ZipCompressionOptions`], [`SevenZCompressionOptions`], and
    /// [`LibarchiveCompressionOptions`] narrow the surface by exposing
    /// only the fields that apply to that format (e.g. no `password`
    /// on 7-Zip, no `split_size` anywhere), so those combinations are
    /// unrepresentable. Constraints the type system cannot express —
    /// notably *which* formats a backend can actually write — are
    /// still validated at create time, so a typed builder can
    /// construct options that `Archive::create_*` later rejects
    /// (R0075-0081, R0081-0005, R0081-0006).
    pub fn validate_for_format(&self) -> crate::error::Result<()> {
        use crate::error::{ArchiveError, ops};
        if !self.format.can_create() {
            return Err(ArchiveError::operation_blocked(
                ops::CREATE,
                format!(
                    "format {:?} is not supported for creation via Archive::create",
                    self.format
                ),
            ));
        }
        if self.password.is_some() {
            return Err(ArchiveError::operation_blocked(
                ops::CREATE,
                format!(
                    "encrypted creation for {:?} is not supported (MADR-0027); password must be None",
                    self.format
                ),
            ));
        }
        if self.split_size.is_some() {
            return Err(ArchiveError::operation_blocked(
                ops::CREATE,
                "split_size is not supported by any backend yet (DEF-002); leave it None",
            ));
        }
        Ok(())
    }
}

impl std::fmt::Debug for CompressionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompressionOptions")
            .field("format", &self.format)
            .field("level", &self.level)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("split_size", &self.split_size)
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

impl CompressionOptions {
    /// Create new compression options with defaults
    pub fn new(format: ArchiveFormat) -> Self {
        Self {
            format,
            level: CompressionLevel::Normal,
            password: None,
            split_size: None,
            progress: None,
        }
    }

    /// Get the archive format
    pub fn format(&self) -> ArchiveFormat {
        self.format
    }

    /// Set the encryption password, consuming and returning `self`.
    ///
    /// The password is stored as a [`Password`], which is UTF-8 by
    /// construction and redacts itself in `Debug`/`Display`. Prefer this
    /// over assigning the public `password` field directly.
    ///
    /// Note: [`Archive::create`](crate::Archive::create) currently
    /// rejects any password for every format (MADR-0027); see the
    /// `password` field docs for the full rationale.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(Password::new(password));
        self
    }
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self::new(ArchiveFormat::Zip)
    }
}

/// Per-format ZIP compression options (R0075-0081).
///
/// Use this builder when you know the output is ZIP at compile time —
/// the type system rules out passing fields that ZIP creation does
/// not honour (e.g. `split_size`, which is reserved for split-volume
/// support that ZIP creation does not yet implement, and `password`,
/// which is rejected by `Archive::create` per MADR-0027).
///
/// Pair with [`crate::Archive::create_zip`].
pub struct ZipCompressionOptions {
    pub(crate) level: CompressionLevel,
    pub(crate) progress: Option<Box<dyn ProgressCallback>>,
}

impl ZipCompressionOptions {
    /// Create options with defaults (Normal level, no progress).
    pub fn new() -> Self {
        Self {
            level: CompressionLevel::Normal,
            progress: None,
        }
    }

    /// Set the compression level.
    pub fn level(mut self, l: CompressionLevel) -> Self {
        self.level = l;
        self
    }

    /// Set a progress callback.
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self {
        self.progress = Some(p);
        self
    }
}

impl Default for ZipCompressionOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ZipCompressionOptions> for CompressionOptions {
    fn from(opts: ZipCompressionOptions) -> Self {
        Self {
            format: ArchiveFormat::Zip,
            level: opts.level,
            password: None,
            split_size: None,
            progress: opts.progress,
        }
    }
}

/// Per-format 7-Zip compression options (R0075-0081).
///
/// Surfaces only the fields 7-Zip creation honours today. Create-time
/// encryption is deferred behind an explicit opt-in that has not
/// shipped (MADR-0027, amended 2026-07-20), so this builder
/// deliberately exposes no `password` field — an encrypted 7-Zip is
/// unrepresentable here rather than constructible into a state that
/// `Archive::create_seven_zip` is guaranteed to reject (R0081-0005).
///
/// Pair with [`crate::Archive::create_seven_zip`].
pub struct SevenZCompressionOptions {
    pub(crate) level: CompressionLevel,
    pub(crate) progress: Option<Box<dyn ProgressCallback>>,
}

impl SevenZCompressionOptions {
    /// Create options with defaults.
    pub fn new() -> Self {
        Self {
            level: CompressionLevel::Normal,
            progress: None,
        }
    }

    /// Set the compression level.
    pub fn level(mut self, l: CompressionLevel) -> Self {
        self.level = l;
        self
    }

    /// Set a progress callback.
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self {
        self.progress = Some(p);
        self
    }
}

impl Default for SevenZCompressionOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SevenZCompressionOptions> for CompressionOptions {
    fn from(opts: SevenZCompressionOptions) -> Self {
        Self {
            format: ArchiveFormat::SevenZip,
            level: opts.level,
            // Encrypted 7-Zip creation is not supported (MADR-0027) and the
            // typed builder no longer surfaces a password field, so this
            // is always `None` (R0081-0005).
            password: None,
            split_size: None,
            progress: opts.progress,
        }
    }
}

/// Per-format libarchive compression options (R0075-0081).
///
/// Use for libarchive-backed creation formats that are not ZIP.
/// Constructed with the target format up front so the call site
/// documents what's being created. See
/// [`ArchiveFormat::can_create`] for the accepted set.
///
/// Pair with [`crate::Archive::create_libarchive`].
pub struct LibarchiveCompressionOptions {
    pub(crate) format: ArchiveFormat,
    pub(crate) level: CompressionLevel,
    pub(crate) progress: Option<Box<dyn ProgressCallback>>,
}

impl LibarchiveCompressionOptions {
    /// Create options for a libarchive-creatable format. The format
    /// is validated at `Archive::create_libarchive` time; passing a
    /// non-libarchive-creatable format here defers the rejection to
    /// the call.
    pub fn new(format: ArchiveFormat) -> Self {
        Self {
            format,
            level: CompressionLevel::Normal,
            progress: None,
        }
    }

    /// Set the compression level.
    pub fn level(mut self, l: CompressionLevel) -> Self {
        self.level = l;
        self
    }

    /// Set a progress callback.
    pub fn progress(mut self, p: Box<dyn ProgressCallback>) -> Self {
        self.progress = Some(p);
        self
    }
}

impl From<LibarchiveCompressionOptions> for CompressionOptions {
    fn from(opts: LibarchiveCompressionOptions) -> Self {
        Self {
            format: opts.format,
            level: opts.level,
            password: None,
            split_size: None,
            progress: opts.progress,
        }
    }
}

/// Progress callback trait for long-running operations.
///
/// Callbacks are invoked through `&mut self` from a single thread per
/// archive operation, so the bound is `Send` only — `Sync` is not
/// required (R0070-0070). This lets `FnMut` closures with non-`Sync`
/// captures (`Cell`, `RefCell`, etc.) participate without
/// `Mutex<RefCell<…>>` boilerplate.
///
/// # Re-entrancy constraint
///
/// The callback **must not** perform any archive operation (open,
/// list, extract, create, etc.) from inside [`on_progress`]. Backends
/// invoke it synchronously while holding backend-internal state, and
/// for UnRAR the call happens while the process-global UnRAR lock is
/// held. Re-entering the archiver on the same thread during a UnRAR
/// operation is detected and rejected precisely because it would
/// otherwise deadlock the non-reentrant lock and hang every UnRAR
/// operation in the process (R0081-0063). Do work that touches
/// archives after the operation returns, not from within the
/// callback.
///
/// [`on_progress`]: ProgressCallback::on_progress
pub trait ProgressCallback: Send {
    /// Called periodically during operations
    ///
    /// # Arguments
    /// * `processed` - Number of bytes processed
    /// * `total` - Total bytes to process (if known)
    ///
    /// # Returns
    /// * `ControlFlow::Continue(())` to continue extraction
    /// * `ControlFlow::Break(())` to cancel extraction
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> std::ops::ControlFlow<()>;
}

// Implement ProgressCallback for FnMut closures
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> std::ops::ControlFlow<()> + Send,
{
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> std::ops::ControlFlow<()> {
        self(processed, total)
    }
}

/// Rate limiter for progress callbacks
///
/// Throttles progress callback frequency using `Instant`-based timing.
/// Default interval is 16ms (~60 updates/sec).
pub struct RateLimiter {
    interval: std::time::Duration,
    /// `None` flags "first call must always pass". We previously
    /// initialised this to `Instant::now() - interval`, but for very
    /// large intervals that subtraction underflows on platforms where
    /// `Instant` is monotonic from boot (R0070-0072). The flag form
    /// is allocation-free and lets `with_interval(Duration::MAX)`
    /// behave the same as the default constructor.
    last_update: Option<std::time::Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter with default 16ms interval (~60 updates/sec)
    pub fn new() -> Self {
        Self {
            interval: std::time::Duration::from_millis(16),
            last_update: None,
        }
    }

    /// Create a rate limiter with a custom interval
    pub fn with_interval(interval: std::time::Duration) -> Self {
        Self {
            interval,
            last_update: None,
        }
    }

    /// Check if enough time has elapsed since last update
    pub fn should_update(&mut self) -> bool {
        let now = std::time::Instant::now();
        match self.last_update {
            None => {
                self.last_update = Some(now);
                true
            }
            Some(prev) if now.duration_since(prev) >= self.interval => {
                self.last_update = Some(now);
                true
            }
            _ => false,
        }
    }

    /// Check if callback should be called (alias for should_update)
    pub fn should_call(&mut self) -> bool {
        self.should_update()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Progress + cancellation hook for SFX payload staging (R0075-0003).
///
/// `Archive::open` on a self-extracting archive copies the embedded
/// archive payload into a tempfile so a normal backend can re-open it
/// at offset zero. Large payloads (multi-GB installers) make this copy
/// observable, so the public open API can carry a hook that observes
/// cumulative bytes copied and may abort the copy partway through.
///
/// Build with [`SfxStagingProgress::new`] for an observe-only callback,
/// or [`SfxStagingProgress::with_cancel`] when the closure also needs
/// to signal cancellation. Pass the value to
/// [`Archive::open_with_sfx_progress`](crate::Archive::open_with_sfx_progress).
///
/// # Cancellation
///
/// When `with_cancel`'s callback returns `false`, staging aborts and
/// `Archive::open_with_sfx_progress` returns
/// [`ArchiveError::Cancelled { operation: "sfx_staging" }`](crate::ArchiveError::Cancelled).
/// The partial tempfile is dropped automatically.
pub struct SfxStagingProgress {
    cb: Box<dyn FnMut(u64) -> bool + Send>,
}

impl SfxStagingProgress {
    /// Wrap an observe-only progress callback. The closure cannot
    /// cancel staging — for cancellation use [`Self::with_cancel`].
    pub fn new(cb: impl FnMut(u64) + Send + 'static) -> Self {
        let mut cb = cb;
        Self {
            cb: Box::new(move |b| {
                cb(b);
                true
            }),
        }
    }

    /// Wrap a callback that may signal cancellation by returning
    /// `false`. The closure receives cumulative bytes copied.
    pub fn with_cancel(cb: impl FnMut(u64) -> bool + Send + 'static) -> Self {
        Self { cb: Box::new(cb) }
    }

    /// Invoke the callback with the running byte count. Returns
    /// `false` when the caller requested cancellation.
    pub(crate) fn emit(&mut self, bytes: u64) -> bool {
        (self.cb)(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::ControlFlow;

    // ── ExtractionOptions defaults ──

    #[test]
    fn test_extraction_options_default() {
        let opts = ExtractionOptions::default();
        assert_eq!(opts.destination, PathBuf::from("."));
        assert!(opts.password.is_none());
        assert!(!opts.overwrite);
        assert!(opts.preserve_permissions);
        assert!(opts.preserve_times);
        // AD 0062 A.3: defaults to false (opt-in CRC verification);
        // a `true` request against libarchive-backed formats now
        // returns `Unsupported` so it must be set consciously.
        assert!(!opts.verify_crc32);
        assert!(opts.filter.is_none());
        assert!(opts.progress.is_none());
    }

    #[test]
    fn test_extraction_options_custom() {
        let opts = ExtractionOptions {
            destination: PathBuf::from("/tmp/extract"),
            password: Some("secret".to_string().into()),
            overwrite: true,
            preserve_permissions: false,
            preserve_times: false,
            verify_crc32: false,
            limits: ExtractionLimits::default(),
            filter: None,
            progress: None,
        };
        assert_eq!(opts.destination, PathBuf::from("/tmp/extract"));
        assert_eq!(opts.password.as_ref().map(Password::as_str), Some("secret"));
        assert!(opts.overwrite);
        assert!(!opts.preserve_permissions);
        assert!(!opts.preserve_times);
        assert!(!opts.verify_crc32);
    }

    // ── CompressionOptions ──

    #[test]
    fn test_compression_options_new() {
        let opts = CompressionOptions::new(ArchiveFormat::SevenZip);
        assert_eq!(opts.format(), ArchiveFormat::SevenZip);
        assert_eq!(opts.level, CompressionLevel::Normal);
        assert!(opts.password.is_none());
        assert!(opts.split_size.is_none());
        assert!(opts.progress.is_none());
    }

    #[test]
    fn test_compression_options_default() {
        let opts = CompressionOptions::default();
        assert_eq!(opts.format(), ArchiveFormat::Zip);
        assert_eq!(opts.level, CompressionLevel::Normal);
    }

    #[test]
    fn test_compression_options_with_password() {
        let mut opts = CompressionOptions::new(ArchiveFormat::Zip);
        opts.password = Some("pw123".into());
        assert_eq!(opts.password.as_ref().map(Password::as_str), Some("pw123"));
    }

    #[test]
    fn test_compression_options_password_builder() {
        // R0081 I5: the builder-setter stores a `Password` and the
        // infallible accessor reads it back. Non-UTF-8 is unrepresentable
        // because construction only accepts `impl Into<String>`, so the
        // fallible `password_as_str` path (AD 0042) is gone.
        let opts = CompressionOptions::new(ArchiveFormat::Zip).password("builder-pw");
        assert_eq!(
            opts.password.as_ref().map(Password::as_str),
            Some("builder-pw")
        );
    }

    #[test]
    fn test_extraction_options_password_builder() {
        let opts = ExtractionOptions::default().password("open-pw");
        assert_eq!(
            opts.password.as_ref().map(Password::as_str),
            Some("open-pw")
        );
    }

    #[test]
    fn test_compression_options_with_split_size() {
        let mut opts = CompressionOptions::new(ArchiveFormat::SevenZip);
        opts.split_size = Some(1024 * 1024); // 1MB
        assert_eq!(opts.split_size, Some(1_048_576));
    }

    // ── CompressionLevel ──

    #[test]
    fn test_compression_level_debug() {
        assert_eq!(format!("{:?}", CompressionLevel::Store), "Store");
        assert_eq!(format!("{:?}", CompressionLevel::Fastest), "Fastest");
        assert_eq!(format!("{:?}", CompressionLevel::Fast), "Fast");
        assert_eq!(format!("{:?}", CompressionLevel::Normal), "Normal");
        assert_eq!(format!("{:?}", CompressionLevel::Maximum), "Maximum");
        assert_eq!(format!("{:?}", CompressionLevel::Ultra), "Ultra");
    }

    #[test]
    fn test_compression_level_clone_copy() {
        let level = CompressionLevel::Maximum;
        let copied = level; // Copy
        assert_eq!(level, copied);
    }

    #[test]
    fn test_compression_level_equality() {
        assert_eq!(CompressionLevel::Normal, CompressionLevel::Normal);
        assert_ne!(CompressionLevel::Store, CompressionLevel::Ultra);
    }

    // ── ProgressCallback trait ──

    #[test]
    fn test_progress_callback_closure() {
        let mut calls = Vec::new();
        let mut callback = |processed: u64, total: Option<u64>| -> ControlFlow<()> {
            calls.push((processed, total));
            ControlFlow::Continue(())
        };

        let result = callback.on_progress(100, Some(1000));
        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (100, Some(1000)));
    }

    #[test]
    fn test_progress_callback_cancellation() {
        let mut callback =
            |_processed: u64, _total: Option<u64>| -> ControlFlow<()> { ControlFlow::Break(()) };

        let result = callback.on_progress(50, Some(100));
        assert!(matches!(result, ControlFlow::Break(())));
    }

    #[test]
    fn test_progress_callback_with_none_total() {
        let mut called = false;
        let mut callback = |_processed: u64, total: Option<u64>| -> ControlFlow<()> {
            assert!(total.is_none());
            called = true;
            ControlFlow::Continue(())
        };

        let _ = callback.on_progress(42, None);
        assert!(called);
    }

    // ── RateLimiter ──

    #[test]
    fn test_rate_limiter_first_call_passes() {
        let mut limiter = RateLimiter::new();
        // First call should always pass
        assert!(limiter.should_update());
    }

    #[test]
    fn test_rate_limiter_throttles_immediate_second_call() {
        let mut limiter = RateLimiter::new();
        assert!(limiter.should_update()); // first call passes
        assert!(!limiter.should_update()); // immediate second call blocked
    }

    #[test]
    fn test_rate_limiter_should_call_alias() {
        let mut limiter = RateLimiter::new();
        assert!(limiter.should_call()); // first call passes
        assert!(!limiter.should_call()); // immediate second call blocked
    }

    #[test]
    fn test_rate_limiter_custom_interval() {
        let mut limiter = RateLimiter::with_interval(std::time::Duration::from_millis(1));
        assert!(limiter.should_update()); // first call passes
        // Sleep just past the interval
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(limiter.should_update()); // should pass after interval
    }

    #[test]
    fn test_rate_limiter_default() {
        let mut limiter = RateLimiter::default();
        assert!(limiter.should_update());
        assert!(!limiter.should_update()); // throttled
    }

    // ── EntryFilter ──

    #[test]
    fn test_entry_filter_usage() {
        use crate::entry::ArchiveEntry;

        let mut filter: EntryFilter = Box::new(|entry: &ArchiveEntry| entry.path.ends_with(".txt"));

        let txt_entry = ArchiveEntry::new("readme.txt".into(), 0);
        let bin_entry = ArchiveEntry::new("data.bin".into(), 1);

        assert!(filter(&txt_entry));
        assert!(!filter(&bin_entry));
    }
}
