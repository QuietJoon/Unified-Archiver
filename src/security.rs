//! Security utilities for safe archive extraction
//!
//! This module provides security checks and sanitization to prevent common
//! archive-based attacks like path traversal (Zip Slip) and zip bombs.

use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, Result};
use std::borrow::Borrow;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Maximum allowed total uncompressed size for extraction (10 GiB)
pub const DEFAULT_MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024 * 1024;

/// Maximum allowed single file size (1 GiB)
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum allowed compression ratio (1000:1)
///
/// Integer since R0081 innovation I1: the compression-ratio gate is now
/// an exact rational ([`CompressionRatio`]), so the default is a whole
/// number rather than an `f64`.
pub const DEFAULT_MAX_COMPRESSION_RATIO: u64 = 1000;

/// Maximum allowed entry count (100,000 files)
pub const DEFAULT_MAX_ENTRY_COUNT: usize = 100_000;

/// Maximum SFX payload staged to disk by `Archive::open_at_offset` (16 GiB).
///
/// Authoritative home of AD 0040's ceiling since the R0081 innovation I1
/// folded it into [`ExtractionLimits::max_sfx_payload_size`]. The
/// `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE` alias re-exports this value
/// so the SFX size-relationship documentation stays in one module.
pub const DEFAULT_MAX_SFX_PAYLOAD_SIZE: u64 = 16 * 1024 * 1024 * 1024;

/// Maximum allowed archive comment size (65,535 bytes).
///
/// R0001-0065: the ZIP end-of-central-directory record encodes the comment
/// length in a `u16`, so 65,535 bytes is the largest comment a writer can
/// serialize without truncating that field. The former `64 * 1024` value was
/// one byte above what the field can express, and no code consulted it.
/// The ZIP writer's `set_archive_comment` now enforces this bound
/// (R0001-0033), so the constant describes a gate that exists.
pub const MAX_COMMENT_SIZE: u32 = u16::MAX as u32;

/// A single resource ceiling: a concrete limit or unbounded.
///
/// Replaces the former `u64::MAX` sentinel used for "no limit". Making
/// "unlimited" a distinct variant rather than an extreme numeric value
/// removes the class of bug where a sentinel is either mistaken for a
/// real bound or silently disables a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// Bounded at this many bytes (or entries).
    Limited(u64),
    /// No ceiling.
    Unlimited,
}

impl Cap {
    /// The ceiling as a raw value; `u64::MAX` for [`Cap::Unlimited`].
    ///
    /// Use where a concrete fallback bound is required (e.g. capping an
    /// unknown-size stream): `Unlimited` yields `u64::MAX`, an
    /// effectively-unbounded byte ceiling.
    #[must_use]
    pub const fn get(self) -> u64 {
        match self {
            Cap::Limited(v) => v,
            Cap::Unlimited => u64::MAX,
        }
    }

    /// As `Option<u64>`: `Some(v)` when limited, `None` when unlimited.
    ///
    /// Matches the backend plan's "`None` = no cap" convention.
    #[must_use]
    pub const fn to_option(self) -> Option<u64> {
        match self {
            Cap::Limited(v) => Some(v),
            Cap::Unlimited => None,
        }
    }

    /// The ceiling as a `usize`, saturating on 32-bit targets;
    /// `usize::MAX` for [`Cap::Unlimited`].
    #[must_use]
    pub const fn as_usize(self) -> usize {
        match self {
            Cap::Limited(v) => {
                if v > usize::MAX as u64 {
                    usize::MAX
                } else {
                    v as usize
                }
            }
            Cap::Unlimited => usize::MAX,
        }
    }

    /// True when `value` exceeds this ceiling. [`Cap::Unlimited`] is
    /// never exceeded.
    #[must_use]
    pub const fn exceeded_by(self, value: u64) -> bool {
        match self {
            Cap::Limited(v) => value > v,
            Cap::Unlimited => false,
        }
    }

    /// True when this is [`Cap::Unlimited`].
    #[must_use]
    pub const fn is_unlimited(self) -> bool {
        matches!(self, Cap::Unlimited)
    }
}

impl From<u64> for Cap {
    /// A bare `u64` becomes a [`Cap::Limited`]; pass [`Cap::Unlimited`]
    /// explicitly for no ceiling.
    fn from(value: u64) -> Self {
        Cap::Limited(value)
    }
}

/// A maximum compression ratio expressed as an exact rational
/// `numerator / denominator`, both strictly positive.
///
/// The zip-bomb gate compares `uncompressed / compressed` against this
/// bound via integer cross-multiplication in `u128`
/// (`uncompressed * denominator > numerator * compressed`), so the
/// decision is exact for every `u64` input. This structurally retires
/// two hazards the former `f64` ratio required hand-written guards for:
///
/// * **The `f64::INFINITY` / `f64::MAX` sentinel hazard (R0080-0006).**
///   A non-finite `f64` limit compared as `>= f64::MAX` and silently
///   disabled the gate. There is no `f64` here and "no limit" is a
///   separate state ([`ExtractionLimits::max_compression_ratio`] is
///   `None`), so the sentinel is unrepresentable.
/// * **The 2^53 precision cliff (R0081-0024).** Casting a `u64`
///   uncompressed size above 2^53 to `f64` lost integer precision and
///   could flip an allow/reject decision at the boundary. Integer
///   cross-multiplication keeps every operand exact.
///
/// Fields are private and the only constructors reject a zero numerator
/// or denominator, so an invalid ratio cannot be constructed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionRatio {
    numerator: u64,
    denominator: u64,
}

impl CompressionRatio {
    /// Construct an exact `numerator / denominator` ratio.
    ///
    /// # Errors
    /// Returns [`ArchiveError::OperationBlocked`] if either operand is
    /// zero: a zero denominator is undefined, and a zero numerator is a
    /// `0:1` ratio that would reject every non-empty entry.
    pub fn new(numerator: u64, denominator: u64) -> Result<Self> {
        if numerator == 0 || denominator == 0 {
            return Err(ArchiveError::operation_blocked(
                "extraction_limits",
                format!(
                    "invalid compression ratio {numerator}/{denominator}: numerator and denominator must both be non-zero"
                ),
            ));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// Construct a whole-number `ratio : 1` bound (the common case; the
    /// shipped default is `1000:1`).
    ///
    /// # Errors
    /// Returns [`ArchiveError::OperationBlocked`] if `ratio` is zero.
    pub fn whole(ratio: u64) -> Result<Self> {
        Self::new(ratio, 1)
    }

    /// The ratio numerator.
    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    /// The ratio denominator.
    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    /// True when `uncompressed / compressed` exceeds this ratio, computed
    /// exactly via `u128` cross-multiplication. The `compressed == 0`
    /// (undefined ratio) case is gated by the caller, not here.
    ///
    /// For an integer `uncompressed`, `uncompressed * denominator >
    /// numerator * compressed` is exactly `uncompressed / compressed >
    /// numerator / denominator`, preserving the R0081-0024 semantics
    /// (which compared against the floor of the threshold) without any
    /// `f64` rounding.
    pub(crate) const fn exceeded_by(self, uncompressed: u64, compressed: u64) -> bool {
        (uncompressed as u128) * (self.denominator as u128)
            > (self.numerator as u128) * (compressed as u128)
    }

    /// Approximate `f64` value, for diagnostics only — never the gate
    /// decision.
    pub(crate) fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

/// Resource limits applied by the pre-extraction safety gate (AD 0003).
///
/// All fields are private; construct via [`ExtractionLimits::default`]
/// for the shipped defaults or [`ExtractionLimits::builder`] to override
/// individual ceilings. Per-field ceilings use the typed [`Cap`] and
/// [`CompressionRatio`] wrappers instead of raw `u64` / `f64` sentinels,
/// so "unlimited" is a distinct state and the compression-ratio gate is
/// exact (see [`CompressionRatio`] for the R0080-0006 / R0081-0024
/// hazards this structurally retires). Encapsulating the fields behind a
/// validating builder partially advances OI-0076-005 for this type.
#[derive(Debug, Clone)]
pub struct ExtractionLimits {
    /// Maximum total uncompressed size across all extracted entries.
    max_total_size: Cap,
    /// Maximum uncompressed size of any single entry.
    max_file_size: Cap,
    /// Maximum compression ratio, or `None` to disable the ratio gate.
    max_compression_ratio: Option<CompressionRatio>,
    /// Maximum number of entries.
    max_entry_count: Cap,
    /// Maximum SFX payload staged to disk by `Archive::open_at_offset`
    /// (AD 0040).
    max_sfx_payload_size: Cap,
    /// Reject (rather than lossily repair) unsafe entry paths (AD 0066).
    ///
    /// Behaviour is still deferred: the flag has a typed home so AD 0066
    /// is no longer a bare deferral, but the strict-reject path itself is
    /// not yet implemented (OI-0076-003 item 1). Default `false` preserves
    /// the current lossy-repair baseline.
    reject_unsafe_paths: bool,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_total_size: Cap::Limited(DEFAULT_MAX_TOTAL_SIZE),
            max_file_size: Cap::Limited(DEFAULT_MAX_FILE_SIZE),
            // 1000:1. Constructed directly (private fields) so `Default`
            // stays infallible; the value is valid by construction.
            max_compression_ratio: Some(CompressionRatio {
                numerator: DEFAULT_MAX_COMPRESSION_RATIO,
                denominator: 1,
            }),
            max_entry_count: Cap::Limited(DEFAULT_MAX_ENTRY_COUNT as u64),
            max_sfx_payload_size: Cap::Limited(DEFAULT_MAX_SFX_PAYLOAD_SIZE),
            reject_unsafe_paths: false,
        }
    }
}

impl ExtractionLimits {
    /// Start building a customized set of limits from the shipped
    /// defaults. See [`ExtractionLimitsBuilder`].
    #[must_use]
    pub fn builder() -> ExtractionLimitsBuilder {
        ExtractionLimitsBuilder {
            inner: Self::default(),
        }
    }

    /// Maximum total uncompressed size across all extracted entries.
    #[must_use]
    pub const fn max_total_size(&self) -> Cap {
        self.max_total_size
    }

    /// Maximum uncompressed size of any single entry.
    #[must_use]
    pub const fn max_file_size(&self) -> Cap {
        self.max_file_size
    }

    /// Maximum compression ratio, or `None` when the ratio gate is
    /// disabled.
    #[must_use]
    pub const fn max_compression_ratio(&self) -> Option<CompressionRatio> {
        self.max_compression_ratio
    }

    /// Maximum number of entries.
    #[must_use]
    pub const fn max_entry_count(&self) -> Cap {
        self.max_entry_count
    }

    /// Maximum SFX payload staged to disk by `Archive::open_at_offset`
    /// (AD 0040).
    #[must_use]
    pub const fn max_sfx_payload_size(&self) -> Cap {
        self.max_sfx_payload_size
    }

    /// Whether the extractor rejects (rather than lossily repairs) unsafe
    /// entry paths (AD 0066).
    ///
    /// Behaviour is deferred: this reports the configured flag, but the
    /// strict-reject path is not yet wired (OI-0076-003 item 1).
    #[must_use]
    pub const fn reject_unsafe_paths(&self) -> bool {
        self.reject_unsafe_paths
    }

    /// Internal "all ceilings disabled" preset for callers that must
    /// bypass every guard (e.g. the content-multiset digest, which bounds
    /// itself by each entry's declared size instead).
    ///
    /// Typed successor to the removed public `unlimited()` sentinel
    /// constructor: it sets every [`Cap`] to [`Cap::Unlimited`] and the
    /// ratio to `None`, so no `f64::MAX` / `u64::MAX` sentinel exists to
    /// misread.
    pub(crate) fn unlimited() -> Self {
        Self {
            max_total_size: Cap::Unlimited,
            max_file_size: Cap::Unlimited,
            max_compression_ratio: None,
            max_entry_count: Cap::Unlimited,
            max_sfx_payload_size: Cap::Unlimited,
            reject_unsafe_paths: false,
        }
    }

    /// Reject suspiciously high uncompressed/compressed ratios (zip-bomb gate).
    ///
    /// `label` is interpolated into the error to identify the entry or archive
    /// the violation belongs to. Returns `Ok(())` when the ratio gate is
    /// disabled (`max_compression_ratio` is `None`).
    ///
    /// **Zero compressed size + non-zero uncompressed size is treated as a
    /// ratio violation** rather than waved through (R0069-0015). A
    /// well-formed archive only reports zero compressed size for empty
    /// payloads; a non-empty entry claiming zero compressed bytes is an
    /// undefined ratio that historically slipped past the gate.
    ///
    /// The comparison is exact integer cross-multiplication inside
    /// [`CompressionRatio::exceeded_by`]; the former non-finite/precision
    /// guards (R0080-0006 / R0081-0024) are gone because the typed ratio
    /// makes those inputs unrepresentable.
    pub(crate) fn check_ratio(
        &self,
        uncompressed: u64,
        compressed: u64,
        label: &str,
        op: &'static str,
    ) -> Result<()> {
        let Some(ratio) = self.max_compression_ratio else {
            // Ratio gate disabled.
            return Ok(());
        };
        if compressed == 0 {
            if uncompressed == 0 {
                return Ok(());
            }
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "{label} reports zero compressed size with {uncompressed} uncompressed bytes (undefined ratio, refusing extract)"
                ),
            });
        }
        if ratio.exceeded_by(uncompressed, compressed) {
            let actual = uncompressed as f64 / compressed as f64;
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "{label} compression ratio {actual:.1}:1 exceeds limit of {:.1}:1 (possible zip bomb)",
                    ratio.as_f64()
                ),
            });
        }
        Ok(())
    }
}

/// Builder for [`ExtractionLimits`] (OI-0076-005).
///
/// Setters are infallible and chainable; the only fallible step is
/// constructing a [`CompressionRatio`], which validates its operands up
/// front. [`build`](ExtractionLimitsBuilder::build) returns the finished,
/// always-valid [`ExtractionLimits`]. Start from the shipped defaults via
/// [`ExtractionLimits::builder`] and override only the ceilings you care
/// about.
///
/// ```
/// use unified_archive::{Cap, CompressionRatio, ExtractionLimits};
///
/// let limits = ExtractionLimits::builder()
///     .max_file_size(64 * 1024 * 1024)         // 64 MiB, via `From<u64>`
///     .max_total_size(Cap::Unlimited)          // no cumulative ceiling
///     .max_compression_ratio(CompressionRatio::whole(500)?)
///     .reject_unsafe_paths(true)         // records intent; enforcement deferred (AD 0066)
///     .build();
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
#[derive(Debug, Clone)]
pub struct ExtractionLimitsBuilder {
    inner: ExtractionLimits,
}

impl ExtractionLimitsBuilder {
    /// Set the maximum total uncompressed size. Accepts a `u64` (becomes
    /// [`Cap::Limited`]) or [`Cap::Unlimited`].
    #[must_use]
    pub fn max_total_size(mut self, cap: impl Into<Cap>) -> Self {
        self.inner.max_total_size = cap.into();
        self
    }

    /// Set the maximum single-entry uncompressed size. Accepts a `u64`
    /// (becomes [`Cap::Limited`]) or [`Cap::Unlimited`].
    #[must_use]
    pub fn max_file_size(mut self, cap: impl Into<Cap>) -> Self {
        self.inner.max_file_size = cap.into();
        self
    }

    /// Set the maximum entry count. Accepts a `u64` (becomes
    /// [`Cap::Limited`]) or [`Cap::Unlimited`].
    #[must_use]
    pub fn max_entry_count(mut self, cap: impl Into<Cap>) -> Self {
        self.inner.max_entry_count = cap.into();
        self
    }

    /// Set the compression-ratio gate to `ratio`. Construct `ratio` with
    /// [`CompressionRatio::whole`] or [`CompressionRatio::new`] (both
    /// validate their operands).
    #[must_use]
    pub fn max_compression_ratio(mut self, ratio: CompressionRatio) -> Self {
        self.inner.max_compression_ratio = Some(ratio);
        self
    }

    /// Disable the compression-ratio gate entirely.
    #[must_use]
    pub fn unlimited_compression_ratio(mut self) -> Self {
        self.inner.max_compression_ratio = None;
        self
    }

    /// Set the maximum SFX payload staged to disk (AD 0040). Accepts a
    /// `u64` (becomes [`Cap::Limited`]) or [`Cap::Unlimited`].
    #[must_use]
    pub fn max_sfx_payload_size(mut self, cap: impl Into<Cap>) -> Self {
        self.inner.max_sfx_payload_size = cap.into();
        self
    }

    /// Reject (rather than lossily repair) unsafe entry paths (AD 0066).
    ///
    /// Behaviour is deferred (OI-0076-003 item 1); this only records the
    /// flag.
    #[must_use]
    pub fn reject_unsafe_paths(mut self, reject: bool) -> Self {
        self.inner.reject_unsafe_paths = reject;
        self
    }

    /// Finish building. Always valid — every field is a pre-validated
    /// typed cap.
    #[must_use]
    pub fn build(self) -> ExtractionLimits {
        self.inner
    }
}

/// Normalize entry path by stripping non-normal components (traversal, root, current dir)
fn normalize_entry_components(entry_path: &str) -> Result<PathBuf> {
    // R0001-0072: reject an embedded NUL before any component processing.
    // `Component::Normal` happily carries a NUL through, so the name used to
    // reach the extractor, get its parent directories created, and only then
    // fail at an FFI/filesystem boundary with a backend-specific error. The
    // write-side `validate_archive_internal_path` already refuses NUL; fail
    // closed here with the same reason wording so both sides agree.
    if entry_path.contains('\0') {
        return Err(ArchiveError::invalid_path(
            entry_path,
            "entry path must not contain NUL",
        ));
    }

    let normalized = Path::new(entry_path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<PathBuf>();

    if normalized.as_os_str().is_empty() {
        return Err(ArchiveError::InvalidPath {
            path: entry_path.to_string(),
            reason: "Path contains only traversal components".to_string(),
        });
    }

    Ok(normalized)
}

/// Validate an archive-internal name at the creation/modification boundary.
///
/// Reject names the extraction side would refuse to honour verbatim — traversal
/// segments (`..`), absolute prefixes (`/`, `C:\\`), NUL bytes, and empty
/// strings. Callers on the write path are expected to pass names that round-trip
/// cleanly through `sanitize_entry_path`; this helper fails fast instead of
/// silently writing an entry whose name the extractor would later rewrite.
///
/// **Backslash normalization (R0070-0020).** Path components are inspected
/// after collapsing `\\` → `/`. Without this step, a Unix host treats
/// `..\\..\\evil` as a single `Normal` component and lets it into the
/// archive — where a Windows consumer would later interpret the
/// backslashes as separators and traverse upward at extract time.
/// Normalising the boundary input keeps the same path shape rejected on
/// both platforms.
pub(crate) fn validate_archive_internal_path(archive_path: &str) -> Result<()> {
    let reason = if archive_path.is_empty() {
        Some("must not be empty")
    } else if archive_path.contains('\0') {
        Some("must not contain NUL")
    } else {
        let normalized = archive_path.replace('\\', "/");
        Path::new(&normalized)
            .components()
            .find_map(|component| match component {
                Component::Normal(_) => None,
                Component::CurDir => Some("must not contain '.' segments"),
                Component::ParentDir => Some("must not contain '..' segments"),
                Component::RootDir | Component::Prefix(_) => Some("must be relative"),
            })
    };

    match reason {
        None => Ok(()),
        Some(r) => Err(ArchiveError::invalid_path(
            archive_path,
            format!("archive-internal path {r}"),
        )),
    }
}

/// Sanitize an archive entry path to prevent path traversal attacks.
///
/// **Lossy normalisation** (R0070-0057, R0070-0058).
///
/// This function removes:
/// - Absolute path components (e.g., `/etc/passwd` → `etc/passwd`)
/// - Parent directory references (e.g., `../../etc/passwd` → `etc/passwd`)
/// - Current directory references (e.g., `./file` → `file`)
///
/// **Caveat — silent hierarchy rewrite (R0070-0058).** Component-level
/// stripping turns `a/../b` into `a/b`, not `b`. The function is not a
/// general path normaliser; it preserves hierarchy that a normalising
/// pass would collapse. Callers needing real normalisation should wrap
/// the result in their own stack-based pass.
///
/// **Caveat — collisions (R0070-0057).** Stripping `../../etc/passwd`
/// to `etc/passwd` can collide with a legitimate, safely-named entry
/// `etc/passwd` in the same archive. The collision is detected at
/// `check_overwrite_conflicts` time, but the lossy rewrite hides the
/// archive's hostile intent — the user only sees a generic
/// "duplicate output" error.
///
/// # Security
///
/// Prevents Zip Slip attacks where malicious archives contain entries with
/// paths like `../../../../../../tmp/malicious.sh` that escape the extraction
/// directory.
///
/// **Crate-internal** — callers go through
/// [`crate::Archive`]'s extraction APIs, which apply this sanitiser
/// internally. The example below is illustrative and lives in
/// `tests/extraction/tests.rs`.
pub(crate) fn sanitize_entry_path(entry_path: &str, dest: &Path) -> Result<PathBuf> {
    let canonical_dest = canonicalize_dest_base(dest)?;
    sanitize_entry_path_with_base(entry_path, dest, &canonical_dest)
}

/// Canonicalize the destination directory once per extraction operation.
///
/// After `normalize_entry_components` strips all `..`/absolute
/// components an entry path cannot lexically escape `dest`; the
/// canonical base exists so the per-entry symlink-ancestor check in
/// [`sanitize_entry_path_with_base`] compares against a stable prefix.
/// `dest` must already exist (extraction creates it before the first
/// entry) and is never moved during the loop, so multi-entry callers
/// canonicalize once and pass the result per entry instead of paying
/// the canonicalize syscall chain N times.
pub(crate) fn canonicalize_dest_base(dest: &Path) -> Result<PathBuf> {
    dest.canonicalize()
        .map_err(|e| ArchiveError::io("canonicalize destination", dest.to_path_buf(), e))
}

/// Per-entry half of [`sanitize_entry_path`]: component normalization,
/// symlink-ancestor escape rejection against the pre-canonicalized
/// base, and the destination-symlink stat. We deliberately do NOT
/// create parent directories here to keep this function
/// side-effect-free (callers create dirs if needed).
pub(crate) fn sanitize_entry_path_with_base(
    entry_path: &str,
    dest: &Path,
    canonical_dest: &Path,
) -> Result<PathBuf> {
    let normalized = normalize_entry_components(entry_path)?;

    // Join with destination
    let full_path = dest.join(&normalized);

    // Find the deepest existing ancestor and verify it is within canonical_dest.
    // If an attacker placed a symlink inside `dest` pointing outside, this catches
    // it without creating any new directories on disk.
    if let Some(parent) = full_path.parent() {
        let mut existing: Option<&Path> = None;
        let mut candidate: Option<&Path> = Some(parent);
        while let Some(p) = candidate {
            // R0081-0021/0022: `Path::exists` swallows EACCES/EIO as
            // "absent" and follows symlinks, so a dangling symlink
            // ancestor reads as missing and never faces the containment
            // check below. `symlink_metadata` (lstat) reports the link
            // itself, so a symlink — dangling or not — counts as the
            // deepest existing component subject to that check. Mirror
            // the leaf stat further down: only `NotFound` means "keep
            // walking up"; any other error fails closed with path context.
            match std::fs::symlink_metadata(p) {
                Ok(_) => {
                    existing = Some(p);
                    break;
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(ArchiveError::io("stat ancestor", p.to_path_buf(), err));
                }
            }
            candidate = p.parent();
        }
        if let Some(p) = existing {
            let canonical_ancestor = p
                .canonicalize()
                .map_err(|e| ArchiveError::io("canonicalize ancestor", p.to_path_buf(), e))?;
            if !canonical_ancestor.starts_with(canonical_dest) {
                return Err(ArchiveError::InvalidPath {
                    path: entry_path.to_string(),
                    reason: "Path traversal attempt detected".to_string(),
                });
            }
        }
    }

    // Check if destination path is a symlink (FR-022: prevent Zip Slip via destination symlinks)
    match std::fs::symlink_metadata(&full_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(ArchiveError::invalid_path(
                    entry_path,
                    "Destination path is a symlink",
                ));
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(ArchiveError::io("stat", full_path.clone(), err));
        }
    }

    Ok(full_path)
}

/// Check if extraction is safe based on resource limits
///
/// Validates:
/// - Total uncompressed size doesn't exceed limit
/// - Individual file sizes are within limit
/// - Compression ratios are reasonable (zip bomb detection)
/// - Entry count is within limit
///
/// # Security
///
/// Prevents zip bomb attacks where small compressed files expand to
/// enormous sizes (e.g., 42.zip: 42KB → 4.5PB).
///
/// **Crate-internal.** Public callers configure the
/// guard via [`ExtractionLimits`] on `ExtractionOptions::limits`; the
/// extraction facade runs this gate internally before any backend
/// touches disk. Generic over `Borrow<ArchiveEntry>` so selective
/// extraction can gate borrowed selections without deep-cloning every
/// entry first.
pub(crate) fn check_extraction_safe<E: Borrow<ArchiveEntry>>(
    entries: &[E],
    limits: &ExtractionLimits,
) -> Result<()> {
    // Check entry count.
    //
    // OI-0080-003: this post-materialization gate stays the authoritative
    // per-operation count policy — it fires for every call, including ones
    // served from a cached listing where no parse ran. A separate parse-time
    // budget (`ReadBackend::list_files_budgeted`, surfaced by
    // `too_many_entries_parsed`) now bounds the *first* materialization so a
    // crafted archive cannot force a full parse + allocation before this gate
    // is reached. Residual: only the streaming backends (libarchive, UnRAR)
    // abort before reading past the budget-th record; the zip/sevenz TOCs
    // are materialized by their libraries at construction, so their budget
    // caps only OUR `Vec<ArchiveEntry>`, and this gate stays the real cap for
    // those formats.
    if limits.max_entry_count.exceeded_by(entries.len() as u64) {
        return Err(ArchiveError::OperationBlocked {
            operation: crate::error::ops::EXTRACT.to_string(),
            reason: format!(
                "Too many entries: {} exceeds limit of {}",
                entries.len(),
                limits.max_entry_count.get()
            ),
        });
    }

    let mut total_uncompressed: u64 = 0;

    for entry in entries {
        let entry = entry.borrow();
        // Size accounting only follows real file payloads. Symlinks,
        // hardlinks, and special entries do not consume archive payload
        // bytes proportionally, and the extractor skips them anyway —
        // counting them used to charge a large link-target string
        // against `max_total_size` even when no payload would be
        // materialized (R0070-0026, R0069-0014).
        if !entry.is_file() {
            continue;
        }

        // Check individual file size
        if let Some(size) = entry.size {
            if limits.max_file_size.exceeded_by(size) {
                return Err(ArchiveError::OperationBlocked {
                    operation: crate::error::ops::EXTRACT.to_string(),
                    reason: format!(
                        "File '{}' too large: {} bytes exceeds limit of {} bytes",
                        entry.path,
                        size,
                        limits.max_file_size.get()
                    ),
                });
            }

            // R0001-0039: a saturating sum presents more than `u64::MAX` of
            // declared content as an apparently valid capped total, so the
            // `max_total_size` gate below would compare against a number the
            // archive never claimed. Fail closed on overflow, matching the
            // content-total routine in `crate::inspection` (R0081-0081).
            total_uncompressed = total_uncompressed.checked_add(size).ok_or_else(|| {
                ArchiveError::operation_blocked(
                    crate::error::ops::EXTRACT,
                    "Total uncompressed size exceeds u64::MAX (malformed archive metadata)",
                )
            })?;
        }
        // R0075-0010: unknown-size entries cannot be summed at
        // preflight without false-positives on legitimate small
        // archives (TAR family, raw gz/bz2/xz). R0080-0008: runtime
        // enforcement of `max_total_size` is not uniform across
        // backends. Only libarchive tallies each entry's decoded byte
        // count into a cumulative counter checked against
        // `max_total_size` inside its extract loop; ZIP and
        // SevenZ instead bound each entry to its declared size via
        // `CopyByteBudget::Exact(declared)`, which stops a per-entry
        // decode overrun but does not police a cross-entry total when
        // metadata under-reports sizes; RAR runtime caps are being
        // added separately. `dispatch_extract_core` still plumbs
        // `max_total_size` through to every backend.

        // Check compression ratio (zip bomb detection)
        if let (Some(uncompressed), Some(compressed)) = (entry.size, entry.compressed_size) {
            limits.check_ratio(
                uncompressed,
                compressed,
                &format!("File '{}'", entry.path),
                crate::error::ops::EXTRACT,
            )?;
        }
    }

    // Check total size
    if limits.max_total_size.exceeded_by(total_uncompressed) {
        return Err(ArchiveError::OperationBlocked {
            operation: crate::error::ops::EXTRACT.to_string(),
            reason: format!(
                "Total uncompressed size {} bytes exceeds limit of {} bytes",
                total_uncompressed,
                limits.max_total_size.get()
            ),
        });
    }

    Ok(())
}

/// Check overall archive compression ratio using archive file size.
///
/// This complements `check_extraction_safe` for formats where per-file
/// compressed sizes are unavailable (TAR.GZ, TAR.BZ2, TAR.XZ via libarchive).
/// Takes the compressed size denominator directly so SFX archives can pass
/// the staged payload tempfile size instead of the outer-executable size
/// (R0069-0006). Callers obtain the right value via
/// [`crate::archive::Archive::payload_size_for_ratio`].
pub(crate) fn check_archive_ratio<E: Borrow<ArchiveEntry>>(
    entries: &[E],
    compressed_size: u64,
    limits: &ExtractionLimits,
    op: &'static str,
) -> Result<()> {
    // R0081 I1: the former non-finite / `>= f64::MAX` guards are gone —
    // the typed `CompressionRatio` cannot represent INFINITY / MAX and
    // "no limit" is `None`, so a disabled gate is the only short-circuit.
    if limits.max_compression_ratio.is_none() {
        return Ok(());
    }

    // Ratio accounting must include only regular file payloads. Symlinks,
    // hardlinks, and special entries do not consume archive payload bytes
    // proportionally, so summing them distorted the bomb-detection
    // denominator (R0069-0014).
    // R0001-0039: the ratio numerator fails closed on overflow for the same
    // reason as the extraction total — a saturated `u64::MAX` numerator is a
    // number the archive never declared, and gating on it silently changes
    // the ratio being enforced.
    let total_uncompressed: u64 = entries
        .iter()
        .map(|e| e.borrow())
        .filter(|e| e.is_file())
        .filter_map(|e| e.size)
        .try_fold(0u64, |acc, size| {
            acc.checked_add(size).ok_or_else(|| {
                ArchiveError::operation_blocked(
                    op,
                    "Total uncompressed size exceeds u64::MAX (malformed archive metadata)",
                )
            })
        })?;

    // R0076-0002: zero compressed size is only safe when nothing is being
    // delivered. A non-empty payload claiming zero compressed bytes is an
    // undefined ratio and indistinguishable from a crafted bomb header.
    if compressed_size == 0 {
        if total_uncompressed == 0 {
            return Ok(());
        }
        return Err(ArchiveError::OperationBlocked {
            operation: op.to_string(),
            reason: format!(
                "Archive reports zero compressed bytes with {} uncompressed bytes (undefined ratio, refusing extract)",
                total_uncompressed
            ),
        });
    }

    limits.check_ratio(total_uncompressed, compressed_size, "Archive", op)
}

/// Archive-wide entry-count gate for selective extraction (R0080-0011).
///
/// [`check_extraction_safe`] only sees the caller's *selection*, so an
/// archive carrying millions of records could slip past `max_entry_count`
/// when a one-entry selection is handed to the gate. Selective entry
/// points validate the full source-listing population through this check
/// before narrowing to the selection. Uses the same error shape as the
/// entry-count branch inside [`check_extraction_safe`].
pub(crate) fn check_archive_entry_count(
    entry_count: usize,
    limits: &ExtractionLimits,
) -> Result<()> {
    if limits.max_entry_count.exceeded_by(entry_count as u64) {
        return Err(ArchiveError::OperationBlocked {
            operation: crate::error::ops::EXTRACT.to_string(),
            reason: format!(
                "Too many entries: {} exceeds limit of {}",
                entry_count,
                limits.max_entry_count.get()
            ),
        });
    }
    Ok(())
}

/// Parse-time entry-count budget rejection (OI-0080-003).
///
/// Raised from inside a backend's listing parser
/// ([`crate::backend::ReadBackend::list_files_budgeted`]) when the
/// accumulating entry count crosses the caller's budget, *before* the full
/// `Vec<ArchiveEntry>` is allocated. Distinct message from the
/// post-materialization checks in [`check_extraction_safe`] /
/// [`check_archive_entry_count`] ("… exceeds limit of …"): those remain the
/// per-operation policy gate (and are the only cap seen by cached listings and
/// by the library-internal TOCs the zip/sevenz backends cannot bound).
/// This one only reports that the first parse was aborted early.
pub(crate) fn too_many_entries_parsed(budget: usize) -> ArchiveError {
    ArchiveError::OperationBlocked {
        operation: crate::error::ops::LIST_FILES.to_string(),
        reason: format!("Too many entries: parsed more than {} entries", budget),
    }
}

/// Combined per-archive safety check: runs `check_extraction_safe` and
/// `check_archive_ratio` against the same entry slice. Use this from
/// extraction backends to avoid two separate traversals. The
/// `compressed_size` denominator should come from
/// [`crate::archive::Archive::payload_size_for_ratio`] so SFX archives
/// use the staged payload size, not the outer executable.
pub(crate) fn check_extraction_safe_with_archive<E: Borrow<ArchiveEntry>>(
    entries: &[E],
    compressed_size: u64,
    limits: &ExtractionLimits,
) -> Result<()> {
    check_extraction_safe(entries, limits)?;
    check_archive_ratio(entries, compressed_size, limits, crate::error::ops::EXTRACT)
}

/// Entry-level validation token (OI-0076-002 / R0076-0090, following
/// the D3 `ValidatedSource` precedent from AD 0055).
///
/// Holding a `ValidatedEntry` is proof that the wrapped entry was
/// resolved from a listing snapshot by the single-entry policy gate
/// ([`validate_single_entry`]): the path matched **exactly one**
/// listing entry and that entry is a regular file. Backends consume
/// the token's [`id`](Self::id) to seek by stable listing position
/// instead of re-scanning by name — the per-backend name scans are
/// what previously made duplicate-name resolution first-match and let
/// entry-kind checks drift between the facade and direct backend
/// calls.
///
/// No public constructor: the only way to obtain one is through the
/// crate-internal policy gates below.
pub(crate) struct ValidatedEntry<'a> {
    entry: &'a ArchiveEntry,
}

impl<'a> ValidatedEntry<'a> {
    /// The validated listing entry.
    pub(crate) fn entry(&self) -> &'a ArchiveEntry {
        self.entry
    }

    /// Stable listing position (`ArchiveEntry::id`). Backends seek by
    /// this instead of re-matching names.
    pub(crate) fn id(&self) -> usize {
        self.entry.id
    }

    /// Normalized listing path — the canonical name. Use this for
    /// output naming, never the raw stored name, so the sanitize
    /// pipeline and the match namespace agree (R0076-0050).
    pub(crate) fn path(&self) -> &'a str {
        &self.entry.path
    }
}

/// Reason string for the "ambiguous single-entry name" refusal, shared
/// by [`validate_single_entry`] and the ZIP backend's raw-central-directory
/// duplicate guard so both surfaces word the rejection identically
/// (R0079-0026 / DCR-009). The `zip` crate collapses byte-identical
/// central-directory names, so the ZIP backend detects the ambiguity out
/// of band (see `ZipArchive::reject_if_duplicate`) and reuses this text.
pub(crate) fn multiple_entries_reason(file_path: &str) -> String {
    format!(
        "Multiple entries match '{}'; refuse to pick one for single-entry extraction. \
         Use Archive::extract_by_ids() with the desired entry ID from list_files() instead.",
        file_path
    )
}

/// Single-entry policy gate shared by the facade and every backend
/// single-entry method (OI-0076-002): the path must match **exactly
/// one** listing entry, and that entry must be a regular file.
///
/// This is [`check_single_entry_safe`] minus the `ExtractionLimits`
/// budget checks. Backends run it against their own memoised listing
/// so a direct backend call cannot bypass the facade contract
/// (R0076-0036..0038, R0076-0050, R0076-0053..0061).
///
/// `op` labels the originating public API (`extract_to_memory`,
/// `extract_to_stream`, `extract_file`, …) so a link/non-regular
/// rejection surfaces with the caller's op rather than a hard-coded
/// label (R0070-0054).
pub(crate) fn validate_single_entry<'a>(
    entries: &'a [ArchiveEntry],
    file_path: &str,
    op: &'static str,
) -> Result<ValidatedEntry<'a>> {
    // R0075-0011: reject duplicate paths the same way the disk
    // single-file extract path does. A silent first-match would let
    // memory, stream, and disk paths disagree on which payload they
    // return. Counting first surfaces the ambiguity loudly.
    let mut matching = entries.iter().filter(|e| e.path == file_path);
    let entry = matching
        .next()
        .ok_or_else(|| ArchiveError::OperationBlocked {
            operation: op.to_string(),
            reason: format!("Entry '{}' not found in archive metadata", file_path),
        })?;
    if matching.next().is_some() {
        return Err(ArchiveError::OperationBlocked {
            operation: op.to_string(),
            reason: multiple_entries_reason(file_path),
        });
    }

    // Reject non-regular entries before any backend touches them.
    // Single-entry APIs commit to materializing regular-file payloads;
    // links and directories must be observed via metadata APIs (FR-022).
    use crate::entry::EntryType;
    match entry.entry_type {
        EntryType::File => {}
        EntryType::Symlink => {
            return Err(crate::error::link_extract_blocked(
                &entry.path,
                crate::error::LinkKind::Symbolic,
                op,
            ));
        }
        EntryType::HardLink => {
            return Err(crate::error::link_extract_blocked(
                &entry.path,
                crate::error::LinkKind::Hard,
                op,
            ));
        }
        EntryType::Directory | EntryType::Other => {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "Entry '{}' is not a regular file ({:?}); single-entry APIs require a regular file payload",
                    entry.path, entry.entry_type
                ),
            });
        }
    }

    Ok(ValidatedEntry { entry })
}

/// Check a single-entry extraction against resource limits and entry-kind
/// policy.
///
/// Used by `extract_to_memory` and `extract_to_stream`, which otherwise bypass
/// the `check_extraction_safe` gate that wraps disk-based extraction paths.
/// The entry is identified by `file_path` (normalized archive path).
///
/// Composed of the shared [`validate_single_entry`] policy gate
/// (existence, uniqueness, regular-file kind) plus the
/// `ExtractionLimits` budget checks:
/// * uncompressed size must not exceed `limits.max_file_size`;
/// * uncompressed size must not exceed `limits.max_total_size` — a
///   single-entry op materializes exactly one file, so the effective
///   ceiling is `min(max_file_size, max_total_size)` (R0081-0025);
/// * per-entry compression ratio must not exceed
///   `limits.max_compression_ratio`.
///
/// Entries without size metadata are allowed through — the backend stream is
/// still responsible for honoring the limit as bytes arrive.
///
/// Returns the [`ValidatedEntry`] token so callers (notably
/// [`check_single_entry_safe_with_archive`]) can reuse the resolved
/// entry without a second listing scan.
pub(crate) fn check_single_entry_safe<'a>(
    entries: &'a [ArchiveEntry],
    file_path: &str,
    limits: &ExtractionLimits,
    op: &'static str,
) -> Result<ValidatedEntry<'a>> {
    let validated = validate_single_entry(entries, file_path, op)?;
    let entry = validated.entry();

    if let Some(size) = entry.size {
        if limits.max_file_size.exceeded_by(size) {
            // R0071-0010: surface the caller's op label rather than
            // a hard-coded `extract`, so telemetry/tests/error
            // strings reflect the actual public API surface that
            // failed (`extract_to_memory`, `extract_to_stream`, etc.).
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "File '{}' too large: {} bytes exceeds limit of {} bytes",
                    entry.path,
                    size,
                    limits.max_file_size.get()
                ),
            });
        }

        // R0081-0025: a single-entry extraction materializes exactly one
        // file, so its extracted total equals this entry's size. The bulk
        // gate enforces `max_total_size` against the running total; the
        // single-entry gate must apply the same cap or a caller whose total
        // budget is below the per-file cap gets no protection on the
        // memory/stream paths. The effective ceiling is therefore
        // `min(max_file_size, max_total_size)`. Same error shape as the
        // total-size branch in `check_extraction_safe`.
        if limits.max_total_size.exceeded_by(size) {
            return Err(ArchiveError::OperationBlocked {
                operation: op.to_string(),
                reason: format!(
                    "Total uncompressed size {} bytes exceeds limit of {} bytes",
                    size,
                    limits.max_total_size.get()
                ),
            });
        }

        if let Some(compressed) = entry.compressed_size {
            limits.check_ratio(size, compressed, &format!("File '{}'", entry.path), op)?;
        }
    }

    Ok(validated)
}

/// Single-entry safety check that adds an archive-level ratio fallback
/// for entries without per-entry compressed size (R0071-0003).
///
/// Same contract as [`check_single_entry_safe`] (entry kind, max file
/// size, per-entry ratio). When the entry's `compressed_size` is `None`
/// — typically CRC-less compressed formats such as TAR.GZ / TAR.BZ2 /
/// TAR.XZ through libarchive's raw single-file readers — the per-entry
/// ratio guard silently skipped before this helper existed. The
/// fallback denominates the entry's uncompressed size against the
/// archive file's on-disk size, mirroring [`check_archive_ratio`] for
/// the disk-write extraction paths so memory/stream APIs cover the
/// same zip-bomb gates.
pub(crate) fn check_single_entry_safe_with_archive(
    entries: &[ArchiveEntry],
    file_path: &str,
    compressed_size: u64,
    limits: &ExtractionLimits,
    op: &'static str,
) -> Result<()> {
    let entry = check_single_entry_safe(entries, file_path, limits, op)?.entry();

    if entry.size.is_some() && entry.compressed_size.is_none() {
        check_archive_ratio(std::slice::from_ref(entry), compressed_size, limits, op)?;
    }
    Ok(())
}

/// Verify CRC32 checksum of extracted data
///
/// Compares the computed CRC32 of the data against the expected value
/// from archive metadata.
///
/// # Security
///
/// Detects corrupted or tampered archive contents during extraction.
///
/// **Crate-internal.** Public-API CRC32 verification happens
/// automatically inside extraction when `ExtractionOptions::verify_crc32`
/// is set; this raw helper is kept on the crate boundary for backends
/// that need to verify already-buffered data.
pub(crate) fn verify_crc32(data: &[u8], expected_crc: Option<u32>, file_path: &str) -> Result<()> {
    if let Some(expected) = expected_crc {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(data);
        let actual = hasher.finalize();

        verify_crc32_value(actual, Some(expected), file_path)?;
    }
    Ok(())
}

pub(crate) fn verify_crc32_value(
    actual: u32,
    expected_crc: Option<u32>,
    file_path: &str,
) -> Result<()> {
    if let Some(expected) = expected_crc {
        if actual != expected {
            return Err(ArchiveError::Corruption {
                path: file_path.to_string(),
                details: format!(
                    "CRC32 mismatch: expected {:08X}, got {:08X}",
                    expected, actual
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sanitize_normal_path() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_normal");
        std::fs::create_dir_all(&dest)?;

        let result =
            sanitize_entry_path("subdir/file.txt", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("subdir/file.txt"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_path_traversal() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_traversal");
        std::fs::create_dir_all(&dest)?;

        // Parent directory references should be removed
        let result =
            sanitize_entry_path("../../etc/passwd", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("etc/passwd"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_absolute_path() -> std::io::Result<()> {
        let dest = PathBuf::from("/tmp/test_unified_archive_absolute");
        std::fs::create_dir_all(&dest)?;

        // Absolute path should be converted to relative
        let result =
            sanitize_entry_path("/etc/passwd", &dest).expect("Sanitization should succeed");
        assert_eq!(result, dest.join("etc/passwd"));

        let _ = std::fs::remove_dir_all(&dest);
        Ok(())
    }

    #[test]
    fn test_sanitize_empty_path() {
        let dest = PathBuf::from("/tmp/test");

        // Path with only traversal components
        let result = sanitize_entry_path("../../..", &dest);
        assert!(result.is_err());
    }

    /// R0001-0072: a NUL-bearing entry name must be rejected by the shared
    /// extraction sanitizer, not carried through as a `Component::Normal`
    /// segment that fails later at an FFI/filesystem boundary.
    #[test]
    fn test_normalize_entry_components_rejects_nul() {
        for input in ["evil\0.txt", "dir/evil\0.txt", "\0"] {
            let err = normalize_entry_components(input)
                .expect_err("NUL-bearing entry path must be rejected");
            match err {
                ArchiveError::InvalidPath { reason, .. } => assert!(
                    reason.contains("must not contain NUL"),
                    "reason {reason:?} missing the NUL rejection wording",
                ),
                other => panic!("expected InvalidPath, got {other:?}"),
            }
        }

        // The per-entry sanitizer rejects before touching the filesystem, so
        // no destination directory has to exist for this to fail closed.
        let dest = Path::new("/tmp/test_unified_archive_nul");
        assert!(sanitize_entry_path_with_base("evil\0.txt", dest, dest).is_err());
    }

    #[test]
    fn test_verify_crc32_valid() {
        let data = b"Hello, world!";
        let expected = 0xEBE6C6E6;
        verify_crc32(data, Some(expected), "test.txt").expect("CRC32 verification should succeed");
    }

    #[test]
    fn test_verify_crc32_invalid() {
        let data = b"Hello, world!";
        let wrong_crc = 0xDEADBEEF;
        let result = verify_crc32(data, Some(wrong_crc), "test.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_crc32_none() {
        let data = b"Hello, world!";
        verify_crc32(data, None, "test.txt").expect("CRC32 verification with None should succeed");
    }

    #[test]
    fn test_check_extraction_safe_normal() {
        let mut entry = ArchiveEntry::new("file.txt".to_string(), 0);
        entry.size = Some(1024);
        entry.compressed_size = Some(512);

        let entries = vec![entry];
        check_extraction_safe(&entries, &ExtractionLimits::default())
            .expect("Normal extraction should be safe");
    }

    #[test]
    fn test_check_extraction_safe_zip_bomb() {
        let mut entry = ArchiveEntry::new("bomb.txt".to_string(), 0);
        entry.size = Some(10 * 1024 * 1024 * 1024); // 10 GB
        entry.compressed_size = Some(1024); // 1 KB - 10,000,000:1 ratio!

        let entries = vec![entry];
        let result = check_extraction_safe(&entries, &ExtractionLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_check_extraction_safe_too_large() {
        let mut entry = ArchiveEntry::new("huge.bin".to_string(), 0);
        entry.size = Some(2 * 1024 * 1024 * 1024); // 2 GB

        let entries = vec![entry];
        let limits = ExtractionLimits::builder()
            .max_file_size(Cap::Limited(1024 * 1024 * 1024)) // 1 GB limit
            .build();

        let result = check_extraction_safe(&entries, &limits);
        assert!(result.is_err());
    }

    /// R0001-0039: declared sizes summing past `u64::MAX` are malformed
    /// metadata, not a "valid" capped total — the preflight must fail closed
    /// instead of gating on a saturated number the archive never claimed.
    #[test]
    fn test_check_extraction_safe_rejects_total_size_overflow() {
        let mut first = ArchiveEntry::new("a.bin".to_string(), 0);
        first.size = Some(u64::MAX);
        let mut second = ArchiveEntry::new("b.bin".to_string(), 1);
        second.size = Some(1);

        // Every other gate is disabled so only the total-size accumulation
        // can produce the error.
        let limits = ExtractionLimits::builder()
            .max_file_size(Cap::Unlimited)
            .max_total_size(Cap::Unlimited)
            .max_entry_count(Cap::Unlimited)
            .unlimited_compression_ratio()
            .build();

        let err = check_extraction_safe(&[first, second], &limits)
            .expect_err("total-size overflow must fail closed");
        assert!(
            err.to_string().contains("exceeds u64::MAX"),
            "expected an overflow error, got: {err}"
        );
    }

    /// R0001-0039: the archive-ratio numerator uses the same checked
    /// accumulation, so an overflowing declared total cannot silently change
    /// the ratio being enforced.
    #[test]
    fn test_check_archive_ratio_rejects_total_size_overflow() {
        let mut first = ArchiveEntry::new("a.bin".to_string(), 0);
        first.size = Some(u64::MAX);
        let mut second = ArchiveEntry::new("b.bin".to_string(), 1);
        second.size = Some(1);

        let err = check_archive_ratio(
            &[first, second],
            1024,
            &ExtractionLimits::default(),
            crate::error::ops::EXTRACT,
        )
        .expect_err("ratio numerator overflow must fail closed");
        assert!(
            err.to_string().contains("exceeds u64::MAX"),
            "expected an overflow error, got: {err}"
        );
    }

    #[test]
    fn test_validate_archive_internal_path_accepts_normal_names() {
        validate_archive_internal_path("file.txt").unwrap();
        validate_archive_internal_path("dir/file.txt").unwrap();
        validate_archive_internal_path("a/b/c/d.ext").unwrap();
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_curdir() {
        // `./file.txt` keeps a leading CurDir component (Path::components
        // preserves it at the start); an interior `a/./b` is normalized away
        // by Path, so only the leading form is observable here.
        assert_invalid_path_with_reason("./file.txt", "'.'");
        assert_invalid_path_with_reason(".", "'.'");
    }

    fn assert_invalid_path_with_reason(input: &str, reason_substring: &str) {
        let err = validate_archive_internal_path(input).unwrap_err();
        match err {
            ArchiveError::InvalidPath { reason, .. } => {
                assert!(
                    reason.contains(reason_substring),
                    "reason {reason:?} missing substring {reason_substring:?}",
                );
            }
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_empty() {
        assert_invalid_path_with_reason("", "must not be empty");
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_nul() {
        assert_invalid_path_with_reason("a\0b", "must not contain NUL");
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_traversal() {
        assert_invalid_path_with_reason("../secret", "'..'");
        assert_invalid_path_with_reason("a/../secret", "'..'");
        assert_invalid_path_with_reason("a/b/..", "'..'");
    }

    #[test]
    fn test_validate_archive_internal_path_rejects_absolute() {
        assert_invalid_path_with_reason("/etc/passwd", "must be relative");
    }

    #[test]
    fn test_check_extraction_safe_too_many_entries() {
        let entries: Vec<_> = (0..1001)
            .map(|i| ArchiveEntry::new(format!("file{}.txt", i), i))
            .collect();

        let limits = ExtractionLimits::builder()
            .max_entry_count(Cap::Limited(1000))
            .build();

        let result = check_extraction_safe(&entries, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_ratio_rejects_invalid_operands() {
        // R0081 I1: INFINITY / NaN / non-positive ratios are no longer
        // *representable* — there is no `f64` field to hold them. The only
        // invalid inputs left (a zero numerator or denominator) are rejected
        // at construction, so an invalid ratio can never reach `check_ratio`.
        // This is the structural successor to the former runtime non-finite
        // guard (R0080-0006 / R0076-0001).
        assert!(CompressionRatio::new(0, 1).is_err(), "zero numerator");
        assert!(CompressionRatio::new(1, 0).is_err(), "zero denominator");
        assert!(CompressionRatio::new(0, 0).is_err(), "both zero");
        assert!(CompressionRatio::whole(0).is_err(), "zero whole ratio");
        // A valid ratio round-trips its operands.
        let ratio = CompressionRatio::whole(1000).expect("valid");
        assert_eq!(ratio.numerator(), 1000);
        assert_eq!(ratio.denominator(), 1);
    }

    #[test]
    fn test_check_ratio_unlimited_waves_through_extreme_ratio() {
        // R0081 I1: disabling the ratio gate (`unlimited_compression_ratio`,
        // stored as `None`) waves through even an extreme ratio. This is the
        // typed successor to the former finite `f64::MAX` "unlimited"
        // sentinel — there is no sentinel value left to misread.
        let limits = ExtractionLimits::builder()
            .unlimited_compression_ratio()
            .build();
        assert!(
            limits
                .check_ratio(u64::MAX, 1, "File 'x'", crate::error::ops::EXTRACT)
                .is_ok()
        );
        let entries = [ArchiveEntry::new("x".to_string(), 0)];
        assert!(check_archive_ratio(&entries, 1, &limits, crate::error::ops::EXTRACT).is_ok());
    }

    #[test]
    fn test_check_archive_entry_count_gates_full_listing() {
        // R0080-0011: the archive-wide population is gated even when a
        // caller ultimately selects a single entry.
        let limits = ExtractionLimits::builder()
            .max_entry_count(Cap::Limited(1000))
            .build();
        assert!(check_archive_entry_count(1001, &limits).is_err());
        assert!(check_archive_entry_count(1000, &limits).is_ok());
    }

    #[test]
    fn test_check_ratio_precision_at_2_pow_54_boundary() {
        // R0081-0024: above 2^53 the old `uncompressed as f64` cast lost
        // integer precision, so a payload one byte over a 2:1 limit rounded
        // back down to exactly 2.0 and slipped through. The u128 comparison
        // keeps `uncompressed` exact, so the boundary decides correctly.
        let limits = ExtractionLimits::builder()
            .max_compression_ratio(CompressionRatio::whole(2).expect("valid ratio"))
            .build();
        let compressed = 1u64 << 53; // 2^53
        let at_limit = 1u64 << 54; // exactly 2:1 — allowed
        let over_limit = (1u64 << 54) + 1; // one byte over 2:1 — must reject

        assert!(
            limits
                .check_ratio(at_limit, compressed, "File 'x'", crate::error::ops::EXTRACT)
                .is_ok(),
            "exactly-at-limit ratio must pass"
        );
        assert!(
            limits
                .check_ratio(
                    over_limit,
                    compressed,
                    "File 'x'",
                    crate::error::ops::EXTRACT
                )
                .is_err(),
            "one byte over the limit above 2^53 must be rejected (f64 division would have allowed it)"
        );
    }

    #[test]
    fn test_check_single_entry_safe_enforces_max_total_size() {
        // R0081-0025: a total cap below the per-file cap must still block a
        // single-entry extraction whose one file exceeds the total budget.
        let mut entry = ArchiveEntry::new("big.bin".to_string(), 0);
        entry.size = Some(2 * 1024 * 1024); // 2 MiB
        entry.compressed_size = Some(2 * 1024 * 1024);
        let entries = vec![entry];

        let limits = ExtractionLimits::builder()
            .max_file_size(Cap::Limited(100 * 1024 * 1024)) // generous per-file cap
            .max_total_size(Cap::Limited(1024 * 1024)) // 1 MiB total cap (below file size)
            .build();

        // `ValidatedEntry` is not `Debug`, so capture the error via a match
        // rather than `expect_err`.
        let err = match check_single_entry_safe(
            &entries,
            "big.bin",
            &limits,
            crate::error::ops::EXTRACT_TO_MEMORY,
        ) {
            Ok(_) => panic!("single-entry extraction must respect max_total_size"),
            Err(e) => e,
        };
        match err {
            ArchiveError::OperationBlocked { operation, reason } => {
                assert_eq!(operation, crate::error::ops::EXTRACT_TO_MEMORY);
                assert!(
                    reason.contains("Total uncompressed size"),
                    "reason {reason:?} should reference the total-size cap"
                );
            }
            other => panic!("expected OperationBlocked, got {other:?}"),
        }

        // Raising the total cap above the file size lets it through.
        let ok_limits = ExtractionLimits::builder()
            .max_file_size(Cap::Limited(100 * 1024 * 1024))
            .max_total_size(Cap::Limited(100 * 1024 * 1024))
            .build();
        check_single_entry_safe(
            &entries,
            "big.bin",
            &ok_limits,
            crate::error::ops::EXTRACT_TO_MEMORY,
        )
        .expect("within both caps should pass");
    }
}
