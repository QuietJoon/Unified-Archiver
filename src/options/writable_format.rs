//! Creation-capable archive formats, as a type-level invariant
//! (OI-0081-002).
//!
//! [`ArchiveFormat`] names every format the crate can *recognise*, but
//! only a subset can be *written* — see [`ArchiveFormat::can_create`].
//! Passing a read-only format into a creation options builder therefore
//! produces a value that is already wrong, and the caller only finds
//! out at the `Archive::create*` call.
//!
//! [`WritableFormat`] closes that gap: it is a newtype whose sole
//! invariant is `inner.can_create() == true`, checked once at
//! construction. The checked constructor is the *only* way to build one
//! from an arbitrary `ArchiveFormat`, so any API that takes a
//! `WritableFormat` cannot be handed a non-creatable format at all.
//!
//! **Why a newtype and not a parallel enum.** The writable set is not
//! stable — `TarZst` / `TarLz4` / `TarLzma` joined it on 2026-08-04 — so
//! a hand-listed mirror enum would drift silently the next time
//! `can_create()` grows an arm. Deriving the invariant from
//! `can_create()` means the predicate has exactly one definition, in
//! `format.rs`, and this module cannot disagree with it. The
//! `WritableFormat::TAR` style associated constants are conveniences for
//! the formats that are writable today; the completeness tests at the
//! bottom of this file fail if a constant stops matching `can_create()`
//! or if a newly-creatable format is missing from [`WritableFormat::ALL`].

use crate::error::{ArchiveError, Result, ops};
use crate::format::ArchiveFormat;

/// An [`ArchiveFormat`] that [`ArchiveFormat::can_create`] accepts.
///
/// Construct with [`WritableFormat::new`] (checked) or one of the
/// associated constants (unconditional — those formats are creatable by
/// definition). Recover the underlying format with
/// [`WritableFormat::format`].
///
/// ```
/// use unified_archive::ArchiveFormat;
/// use unified_archive::options::WritableFormat;
///
/// // Creatable: accepted.
/// let tar = WritableFormat::new(ArchiveFormat::Tar).expect("tar is creatable");
/// assert_eq!(tar.format(), ArchiveFormat::Tar);
/// assert_eq!(tar, WritableFormat::TAR);
///
/// // Read-only via the main facade: rejected at construction, not at
/// // `Archive::create` time.
/// assert!(WritableFormat::new(ArchiveFormat::Rar).is_err());
/// assert!(WritableFormat::new(ArchiveFormat::Iso).is_err());
/// ```
///
/// # What this does *not* guarantee
///
/// `can_create()` describes the *format*, not the current build. TAR.ZST
/// / TAR.LZ4 / TAR.LZMA creation additionally needs a libarchive built
/// with the matching write filter, and that can only be discovered by
/// asking libarchive — so a `WritableFormat` can still fail at writer
/// construction on a build that lacks the codec (R0070-0041). See
/// [`crate::options::CompressionOptions::validate_for_format`] for the
/// full list of invariants that stay runtime checks and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WritableFormat(ArchiveFormat);

impl WritableFormat {
    /// ZIP (`.zip`).
    pub const ZIP: Self = Self(ArchiveFormat::Zip);
    /// 7-Zip (`.7z`).
    pub const SEVEN_ZIP: Self = Self(ArchiveFormat::SevenZip);
    /// Uncompressed TAR (`.tar`).
    pub const TAR: Self = Self(ArchiveFormat::Tar);
    /// gzip-compressed TAR (`.tar.gz`, `.tgz`).
    pub const TAR_GZIP: Self = Self(ArchiveFormat::TarGzip);
    /// bzip2-compressed TAR (`.tar.bz2`, `.tbz2`, `.tb2`).
    pub const TAR_BZIP2: Self = Self(ArchiveFormat::TarBzip2);
    /// xz-compressed TAR (`.tar.xz`, `.txz`).
    pub const TAR_XZ: Self = Self(ArchiveFormat::TarXz);
    /// zstd-compressed TAR (`.tar.zst`, `.tzst`). Creation needs a
    /// libarchive built with the zstd write filter.
    pub const TAR_ZST: Self = Self(ArchiveFormat::TarZst);
    /// lz4-compressed TAR (`.tar.lz4`). Creation needs a libarchive
    /// built with the lz4 write filter.
    pub const TAR_LZ4: Self = Self(ArchiveFormat::TarLz4);
    /// lzma-compressed TAR (`.tar.lzma`, `.tlz`). Creation needs a
    /// libarchive built with the lzma write filter.
    pub const TAR_LZMA: Self = Self(ArchiveFormat::TarLzma);

    /// Every format that is creatable today, in the order
    /// [`ArchiveFormat`] declares them.
    ///
    /// Kept in sync with [`ArchiveFormat::can_create`] by the
    /// completeness tests in this module — a format that becomes
    /// creatable without being added here fails
    /// `all_covers_every_creatable_format`.
    pub const ALL: &'static [Self] = &[
        Self::SEVEN_ZIP,
        Self::ZIP,
        Self::TAR,
        Self::TAR_GZIP,
        Self::TAR_BZIP2,
        Self::TAR_XZ,
        Self::TAR_ZST,
        Self::TAR_LZ4,
        Self::TAR_LZMA,
    ];

    /// Wrap `format` if [`ArchiveFormat::can_create`] accepts it.
    ///
    /// # Errors
    ///
    /// [`ArchiveError::OperationBlocked`](crate::ArchiveError::OperationBlocked)
    /// with the same operation label and wording
    /// [`crate::Archive::create`] would report for the same format, so
    /// moving a rejection from create time to construction time does not
    /// change what the caller sees.
    pub fn new(format: ArchiveFormat) -> Result<Self> {
        if format.can_create() {
            Ok(Self(format))
        } else {
            Err(ArchiveError::operation_blocked(
                ops::CREATE,
                format!("format {format:?} is not supported for creation via Archive::create"),
            ))
        }
    }

    /// The wrapped format. Creatable by construction.
    pub const fn format(self) -> ArchiveFormat {
        self.0
    }
}

impl TryFrom<ArchiveFormat> for WritableFormat {
    type Error = ArchiveError;

    fn try_from(format: ArchiveFormat) -> Result<Self> {
        Self::new(format)
    }
}

impl From<WritableFormat> for ArchiveFormat {
    fn from(writable: WritableFormat) -> Self {
        writable.format()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every [`ArchiveFormat`] variant.
    ///
    /// Two-tier drift alarm: the exhaustive match in
    /// `format_list_is_exhaustive` stops compiling when a variant is
    /// added (`#[non_exhaustive]` does not apply inside the defining
    /// crate), and the length assertion there fails until the new
    /// variant is appended here as well.
    const ALL_ARCHIVE_FORMATS: &[ArchiveFormat] = &[
        ArchiveFormat::SevenZip,
        ArchiveFormat::Zip,
        ArchiveFormat::Rar,
        ArchiveFormat::Rar5,
        ArchiveFormat::Tar,
        ArchiveFormat::TarGzip,
        ArchiveFormat::TarBzip2,
        ArchiveFormat::TarXz,
        ArchiveFormat::TarZst,
        ArchiveFormat::TarLz4,
        ArchiveFormat::TarLzma,
        ArchiveFormat::Gzip,
        ArchiveFormat::Bzip2,
        ArchiveFormat::Xz,
        ArchiveFormat::Zst,
        ArchiveFormat::Lz4,
        ArchiveFormat::Lzma,
        ArchiveFormat::Iso,
    ];

    #[test]
    fn format_list_is_exhaustive() {
        for &format in ALL_ARCHIVE_FORMATS {
            // Wildcard-free on purpose: a new `ArchiveFormat` variant
            // must be added here, which then trips the count below.
            match format {
                ArchiveFormat::SevenZip
                | ArchiveFormat::Zip
                | ArchiveFormat::Rar
                | ArchiveFormat::Rar5
                | ArchiveFormat::Tar
                | ArchiveFormat::TarGzip
                | ArchiveFormat::TarBzip2
                | ArchiveFormat::TarXz
                | ArchiveFormat::TarZst
                | ArchiveFormat::TarLz4
                | ArchiveFormat::TarLzma
                | ArchiveFormat::Gzip
                | ArchiveFormat::Bzip2
                | ArchiveFormat::Xz
                | ArchiveFormat::Zst
                | ArchiveFormat::Lz4
                | ArchiveFormat::Lzma
                | ArchiveFormat::Iso => {}
            }
        }
        assert_eq!(
            ALL_ARCHIVE_FORMATS.len(),
            18,
            "ArchiveFormat gained or lost a variant — update ALL_ARCHIVE_FORMATS and re-check WritableFormat::ALL"
        );
        for &format in ALL_ARCHIVE_FORMATS {
            let occurrences = ALL_ARCHIVE_FORMATS.iter().filter(|f| **f == format).count();
            assert_eq!(occurrences, 1, "{format:?} listed more than once");
        }
    }

    #[test]
    fn new_accepts_exactly_the_creatable_formats() {
        for &format in ALL_ARCHIVE_FORMATS {
            assert_eq!(
                WritableFormat::new(format).is_ok(),
                format.can_create(),
                "WritableFormat::new disagrees with ArchiveFormat::can_create for {format:?}"
            );
        }
    }

    #[test]
    fn all_covers_every_creatable_format() {
        for &format in ALL_ARCHIVE_FORMATS {
            let listed = WritableFormat::ALL.iter().any(|w| w.format() == format);
            assert_eq!(
                listed,
                format.can_create(),
                "WritableFormat::ALL membership disagrees with can_create for {format:?}"
            );
        }
        assert_eq!(WritableFormat::ALL.len(), 9);
    }

    #[test]
    fn associated_constants_hold_the_invariant() {
        for writable in WritableFormat::ALL {
            assert!(
                writable.format().can_create(),
                "{:?} is a WritableFormat constant but is not creatable",
                writable.format()
            );
        }
    }

    #[test]
    fn rejection_matches_the_create_time_error() {
        let err = WritableFormat::new(ArchiveFormat::Iso).expect_err("ISO is read-only");
        let rendered = err.to_string();
        assert!(
            rendered.contains("Iso"),
            "rejection should name the format: {rendered}"
        );
        // Same gate `CompressionOptions::validate_for_format` reports,
        // so preflight and construction agree.
        //
        // Deliberately the deprecated constructor: this assertion exists to
        // prove the loose path still accepts a non-creatable format and
        // defers the rejection. `WritableFormat` cannot express `Iso` at
        // all, so migrating it would delete the test's subject.
        #[allow(deprecated)]
        let loose = crate::options::CompressionOptions::new(ArchiveFormat::Iso);
        assert!(loose.validate_for_format().is_err());
    }

    #[test]
    fn round_trips_through_archive_format() {
        for &writable in WritableFormat::ALL {
            let format: ArchiveFormat = writable.into();
            assert_eq!(
                WritableFormat::try_from(format).expect("creatable by construction"),
                writable
            );
        }
    }
}
