//! Configuration options for archive operations

use crate::ArchiveFormat;
use crate::entry::ArchiveEntry;
use crate::security::ExtractionLimits;
use std::path::PathBuf;

/// Entry filter type alias for filtering archive entries
pub type EntryFilter = Box<dyn Fn(&ArchiveEntry) -> bool + Send + Sync>;

/// Configuration for archive extraction operations
pub struct ExtractionOptions {
    /// Destination directory
    pub destination: PathBuf,

    /// Password for encrypted archives
    pub password: Option<String>,

    /// Overwrite existing files (default: false, fails with error if files exist)
    pub overwrite: bool,

    /// Preserve file permissions (Unix)
    pub preserve_permissions: bool,

    /// Preserve modification times
    pub preserve_times: bool,

    /// Verify CRC32 checksums during extraction
    pub verify_crc32: bool,

    /// Resource limits for extraction (zip bomb protection)
    pub limits: ExtractionLimits,

    /// Filter: only extract matching paths
    pub filter: Option<EntryFilter>,

    /// Progress callback
    pub progress: Option<Box<dyn ProgressCallback>>,
}

impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            destination: PathBuf::from("."),
            password: None,
            overwrite: false,
            preserve_permissions: true,
            preserve_times: true,
            verify_crc32: true,
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

/// Configuration for archive creation operations
pub struct CompressionOptions {
    /// Output archive format
    pub format: ArchiveFormat,

    /// Compression level
    pub level: CompressionLevel,

    /// Password for encryption (if format supports it)
    pub password: Option<String>,

    /// Split archive into parts (bytes per part)
    pub split_size: Option<u64>,

    /// Progress callback
    pub progress: Option<Box<dyn ProgressCallback>>,
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

    /// Create a builder for compression options (alias for new)
    pub fn builder(format: ArchiveFormat) -> Self {
        Self::new(format)
    }

    /// Get the archive format
    pub fn format(&self) -> ArchiveFormat {
        self.format
    }
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self::new(ArchiveFormat::Zip)
    }
}

/// Progress callback trait for long-running operations
pub trait ProgressCallback: Send + Sync {
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
    F: FnMut(u64, Option<u64>) -> std::ops::ControlFlow<()> + Send + Sync,
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
    last_update: std::time::Instant,
}

impl RateLimiter {
    /// Create a new rate limiter with default 16ms interval (~60 updates/sec)
    pub fn new() -> Self {
        let interval = std::time::Duration::from_millis(16);
        Self {
            interval,
            // Initialize to now - interval so first call always passes
            last_update: std::time::Instant::now() - interval,
        }
    }

    /// Create a rate limiter with a custom interval
    pub fn with_interval(interval: std::time::Duration) -> Self {
        Self {
            interval,
            last_update: std::time::Instant::now() - interval,
        }
    }

    /// Check if enough time has elapsed since last update
    pub fn should_update(&mut self) -> bool {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_update) >= self.interval {
            self.last_update = now;
            true
        } else {
            false
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
        assert!(opts.verify_crc32);
        assert!(opts.filter.is_none());
        assert!(opts.progress.is_none());
    }

    #[test]
    fn test_extraction_options_custom() {
        let opts = ExtractionOptions {
            destination: PathBuf::from("/tmp/extract"),
            password: Some("secret".into()),
            overwrite: true,
            preserve_permissions: false,
            preserve_times: false,
            verify_crc32: false,
            limits: ExtractionLimits::default(),
            filter: None,
            progress: None,
        };
        assert_eq!(opts.destination, PathBuf::from("/tmp/extract"));
        assert_eq!(opts.password.as_deref(), Some("secret"));
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
    fn test_compression_options_builder_alias() {
        let opts = CompressionOptions::builder(ArchiveFormat::Tar);
        assert_eq!(opts.format(), ArchiveFormat::Tar);
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
        assert_eq!(opts.password.as_deref(), Some("pw123"));
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
        let cloned = level.clone();
        let copied = level; // Copy
        assert_eq!(level, cloned);
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
        // First call should always pass (initialized to now - interval)
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

        let filter: EntryFilter = Box::new(|entry: &ArchiveEntry| entry.path.ends_with(".txt"));

        let txt_entry = ArchiveEntry::new("readme.txt".into(), 0);
        let bin_entry = ArchiveEntry::new("data.bin".into(), 1);

        assert!(filter(&txt_entry));
        assert!(!filter(&bin_entry));
    }
}
