//! Archive format detection and capabilities

pub mod multipart;

use crate::error::{ArchiveError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Level of support for a format capability
///
/// R0076-0080: marked `#[non_exhaustive]` so future capability states
/// (e.g. "Read-only", "Behind-feature") can be added without breaking
/// downstream exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
///
/// R0076-0081: marked `#[non_exhaustive]` so a future capability field
/// (e.g. random-access reads, recovery records) does not break struct
/// literal initialisers downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Compression support for reading/decompression.
    ///
    /// Split into read-vs-write so RAR/RAR5 (which can decompress but
    /// not create archives through the main facade) and ZIP/7z (which
    /// can do both) report their actual support without the
    /// previously-collapsed boolean (R0070-0065).
    pub compression_read: Support,
    /// Compression support for writing/creation.
    pub compression_write: Support,
}

impl FormatCapabilities {
    /// Backward-compatible "compression" view collapsing the typed
    /// read/write split. Returns `Full` if either side is `Full`,
    /// `Partial` if either side is `Partial`, otherwise `None`.
    /// New callers should consult [`Self::compression_read`] /
    /// [`Self::compression_write`] directly (R0070-0065).
    pub fn compression(self) -> Support {
        merge_support(self.compression_read, self.compression_write)
    }
}

const fn merge_support(a: Support, b: Support) -> Support {
    match (a, b) {
        (Support::Full, _) | (_, Support::Full) => Support::Full,
        (Support::Partial, _) | (_, Support::Partial) => Support::Partial,
        _ => Support::None,
    }
}

/// Supported archive formats
///
/// R0076-0078: marked `#[non_exhaustive]` so adding a new format
/// (e.g. zip64 zstd, dwarfs) is not a breaking change for downstream
/// exhaustive matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArchiveFormat {
    SevenZip,
    Zip,
    Rar,
    Rar5,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    /// TAR with zstd compression (`.tar.zst`, `.tzst`). Read/extract and
    /// create supported via libarchive; creation additionally requires a
    /// libarchive built with the zstd write filter (R0075-0031).
    TarZst,
    /// TAR with lz4 compression (`.tar.lz4`). Read/extract and create
    /// supported via libarchive; creation additionally requires a
    /// libarchive built with the lz4 write filter (R0075-0031).
    TarLz4,
    /// TAR with lzma compression (`.tar.lzma`, `.tlz`). Read/extract and
    /// create supported via libarchive; creation additionally requires a
    /// libarchive built with the lzma write filter (R0075-0031).
    TarLzma,
    Gzip,
    Bzip2,
    Xz,
    /// Standalone zstd-compressed stream (`.zst`). Read-only via
    /// libarchive's raw filter, matching the existing
    /// Gzip/Bzip2/Xz policy (R0075-0031).
    Zst,
    /// Standalone lz4-compressed stream (`.lz4`). Read-only via
    /// libarchive's raw filter (R0075-0031).
    Lz4,
    /// Standalone lzma-compressed stream (`.lzma`). Read-only via
    /// libarchive's raw filter (R0075-0031).
    Lzma,
    Iso,
}

impl ArchiveFormat {
    /// Detect archive format from file magic bytes.
    ///
    /// **Magic-buffer sizing (AD 0062 A.5).** The default read covers
    /// 512 bytes — sufficient for ZIP, RAR, 7z, GZIP, BZIP2, XZ, ZST,
    /// LZ4, and the TAR `ustar` marker at offset 257. ISO 9660's PVD identifier
    /// (`CD001`) lives at offset 32769, which the default buffer cannot
    /// reach. To keep ISO detect-from-file working without paying for
    /// the wider read on every call, the buffer is widened to 33 KiB
    /// when the file's extension hints at ISO content (`.iso`, `.bin`,
    /// `.img`, `.cd`). Files with non-ISO extensions stay on the cheap
    /// 512-byte path; an ISO renamed `mystery.dat` therefore detects as
    /// `Unknown archive format` from-file (false negative — accepted).
    /// Callers who already have the bytes in memory can route through
    /// [`Self::detect_from_bytes`] with the full buffer to bypass the
    /// extension hint. Raw LZMA streams do not have a stable short magic
    /// marker, so `.lzma` / `.tar.lzma` / `.tlz` use an extension fallback.
    ///
    /// The 512-byte window also bounds how far the zstd probe can walk a
    /// leading skippable-frame prefix (see `zstd_frame_offset`): a `.zst`
    /// whose first skippable frame carries a payload long enough to push
    /// the standard frame magic past the window detects as
    /// `Unknown archive format` from-file (false negative — accepted;
    /// callers holding the bytes can pass a wider buffer to
    /// [`Self::detect_from_bytes`]).
    pub fn detect(path: &Path) -> Result<Self> {
        let mut file =
            File::open(path).map_err(|e| ArchiveError::io("open", path.to_path_buf(), e))?;

        // Default magic buffer is 512 bytes; widened to 33 KiB when the
        // path's extension suggests ISO content so the PVD at offset
        // 32769 is reachable.
        let needs_iso_window = extension_suggests_iso(path);
        let buffer_size: usize = if needs_iso_window {
            // 32774 = ISO PVD offset (32769) + b"CD001".len() (5).
            // Round up to a comfortable 33 KiB to absorb future
            // tail-of-buffer additions cheaply.
            33 * 1024
        } else {
            512
        };
        // R0001-0034: `Read::read` may legally return fewer bytes than the
        // buffer holds before EOF, so a single call can under-fill the probe
        // and misclassify a valid archive on a filesystem or reader wrapper
        // that short-reads. A `take`-bounded `read_to_end` loops until the
        // probe window is full or the file genuinely ends.
        let mut magic = Vec::with_capacity(buffer_size);
        file.by_ref()
            .take(buffer_size as u64)
            .read_to_end(&mut magic)
            .map_err(|e| ArchiveError::io("read", path.to_path_buf(), e))?;
        let bytes_read = magic.len();

        if bytes_read < 4 {
            // R0076-0093: the file is too small for any magic probe, so
            // content cannot classify it at all — consult the extension
            // map before surfacing the failure. This does not weaken the
            // magic-first precedence below: it only fires when there is
            // no usable content signal, unlike the post-magic fallback
            // which stays gated on `is_extension_fallback`.
            if let Some(format) = format_from_extension(path) {
                return Ok(format);
            }
            return Err(ArchiveError::format(
                None,
                "File too small to detect format",
            ));
        }

        // Try detection from bytes first.
        if let Ok(format) = Self::detect_from_bytes(&magic[..bytes_read]) {
            return Ok(promote_to_compound_tar(format, path));
        }

        // Fallback only for formats where extension is the intentional
        // detection mechanism (see `ArchiveFormat::is_extension_fallback`).
        // Other extensions are not trusted: a corrupt `foo.zip` still fails
        // magic-first detection.
        if let Some(format) = format_from_extension(path)
            && format.is_extension_fallback()
        {
            return Ok(format);
        }

        Err(ArchiveError::format(None, "Unknown archive format"))
    }

    // (helper below the `impl`)

    /// Detect archive format from raw bytes (magic bytes)
    ///
    /// Phase 7: Used for SFX detection to identify embedded archives at arbitrary offsets.
    /// Does not use file extension hints - only magic bytes.
    ///
    /// **ISO 9660** is recognized by the `CD001` Primary Volume Descriptor
    /// identifier at byte offset `32769` (sector 16) plus the version byte
    /// at offset `32774`. The caller must therefore pass at least `32775`
    /// bytes for the ISO branch to fire. A typical 1 MiB SFX scan buffer is
    /// sufficient; smaller buffers fall through and `Iso` will not be
    /// returned.
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

        // ZIP: local-file-header "PK\x03\x04" or end-of-central-directory
        // "PK\x05\x06". The data-descriptor signature "PK\x07\x08" is
        // deliberately excluded — a data descriptor is an intra-stream
        // record, never a standalone archive header, so accepting it at
        // offset zero misclassifies arbitrary data (R0080-0072). The EOCD
        // arm additionally requires the full 22-byte fixed record; the bare
        // four-byte signature is not a complete end-of-central-directory and
        // would be an avoidable false positive (R0081-0014). R0001-0036: the
        // fixed fields must also be structurally coherent — see
        // `zip_eocd_header_ok`.
        if magic.starts_with(b"PK\x03\x04") || zip_eocd_header_ok(magic) {
            return Ok(ArchiveFormat::Zip);
        }

        // 7z: "7z\xBC\xAF\x27\x1C"
        if magic.starts_with(b"7z\xBC\xAF\x27\x1C") {
            return Ok(ArchiveFormat::SevenZip);
        }

        // GZIP: ID1 ID2 = 0x1F 0x8B, CM = 8 (deflate), and the reserved
        // FLG bits (0xE0) clear over the fixed 10-byte header. Mirrors the
        // stricter SFX probe so two incidental magic bytes in noise no
        // longer route into gzip handling (R0080-0073).
        if magic.len() >= 10
            && magic[0] == 0x1F
            && magic[1] == 0x8B
            && magic[2] == 0x08
            && (magic[3] & 0xE0) == 0
        {
            return Ok(ArchiveFormat::Gzip);
        }

        // BZIP2: "BZh" followed by the block-size digit '1'..='9'. The
        // digit gate rejects a bare "BZh" prefix in noise (R0080-0074).
        if magic.starts_with(b"BZh") && matches!(magic[3], b'1'..=b'9') {
            return Ok(ArchiveFormat::Bzip2);
        }

        // XZ: 0xFD 0x37 0x7A 0x58 0x5A 0x00
        if magic.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
            return Ok(ArchiveFormat::Xz);
        }

        // Zstandard: 0x28 0xB5 0x2F 0xFD (the standard zstd frame magic),
        // optionally preceded by skippable frames. RFC 8878 §3.1.2 places
        // skippable frames (magic 0x184D2A50–0x184D2A5F) anywhere between
        // frames — the first position included — so a conforming `.zst`
        // need not open with the standard magic, and `Zst` is deliberately
        // absent from the extension fallback set, which left such a file
        // undetectable by every path (OI-0080-008). `zstd_frame_offset`
        // walks the skippable prefix by its stored length (an exact jump,
        // not a scan) and returns `None` when the probe window ends inside
        // the prefix or the skip budget is exhausted, so a short buffer or
        // a crafted frame chain falls through to "undetected" rather than
        // being guessed. Verified against the linked libarchive (3.8.9):
        // its zstd read filter bids on the skippable magic and extracts
        // such a stream, so detection does not promise an extraction the
        // decompressor would refuse.
        if let Some(offset) = zstd_frame_offset(magic)
            && magic[offset..].starts_with(&[0x28, 0xB5, 0x2F, 0xFD])
        {
            return Ok(ArchiveFormat::Zst);
        }

        // LZ4 frame: the modern frame magic 0x04 0x22 0x4D 0x18 and the
        // legacy frame magic 0x184C2102 (little-endian bytes
        // 0x02 0x21 0x4C 0x18). libarchive still reads legacy frames, so
        // recognising both keeps content detection in step with the
        // decompressor (R0081-0012).
        if magic.starts_with(&[0x04, 0x22, 0x4D, 0x18])
            || magic.starts_with(&[0x02, 0x21, 0x4C, 0x18])
        {
            return Ok(ArchiveFormat::Lz4);
        }

        // TAR (POSIX ustar). The ustar marker at offset 257 alone is too
        // weak — five incidental bytes misclassify arbitrary data
        // (R0080-0076). Require a full 512-byte header whose stored
        // checksum matches the one computed with the checksum field taken
        // as spaces (both the unsigned and signed byte-sum variants are
        // accepted, per the ustar convention). Pre-POSIX V7 tar, which
        // predates the ustar marker, is handled by the strict fallback
        // probe below (OI-0081-003).
        if magic.len() >= 512
            && &magic[257..262] == b"ustar"
            && tar_header_checksum_ok(&magic[..512])
        {
            return Ok(ArchiveFormat::Tar);
        }

        // TAR (pre-POSIX V7). V7 has no ustar marker, so extensionless or
        // misnamed V7 archives never match the branch above even though
        // libarchive reads them (OI-0081-003). This probe runs only after
        // the ustar branch fails and keeps the false-positive surface at
        // or below what the ustar checksum gate established (R0080-0076):
        // it reuses the same 512-byte checksum validation plus V7-specific
        // structural checks (absent ustar magic, non-empty name, octal
        // size/mtime, a V7 linkflag). See `tar_v7_header_ok`.
        if magic.len() >= 512 && tar_v7_header_ok(&magic[..512]) {
            return Ok(ArchiveFormat::Tar);
        }

        // ISO 9660: the volume descriptor at logical sector 16 (byte
        // 32768) must look like a Primary Volume Descriptor. `CD001`
        // alone is too weak — five incidental bytes at the PVD offset
        // misclassify arbitrary large files (R0080-0077). Add a light
        // structural check: descriptor type byte (32768) == 0x01 and
        // version byte (32774, immediately after `CD001`) == 0x01. No
        // terminator walk. The caller must therefore hand us at least
        // sector 16 plus the version byte — typical SFX scan buffers
        // (1 MiB) and the ISO detect-window widen both do.
        if magic.len() >= 32775
            && magic[32768] == 0x01
            && &magic[32769..32774] == b"CD001"
            && magic[32774] == 0x01
        {
            return Ok(ArchiveFormat::Iso);
        }

        Err(ArchiveError::format(
            None,
            "Unknown archive format from magic bytes",
        ))
    }

    /// Per-operation capability report for this format
    ///
    /// **The answer describes the format, not the current build**
    /// (R0001-0063). [`Rar`](Self::Rar) / [`Rar5`](Self::Rar5) report the
    /// full RAR feature surface unconditionally, but RAR open dispatch is
    /// gated on the `rar-support` Cargo feature: in a build compiled
    /// without it, [`Archive::open`](crate::Archive::open) rejects RAR
    /// archives with an
    /// [`Unsupported`](crate::ArchiveError::Unsupported) error even though
    /// this matrix advertises `Full`. Keeping the report build-independent
    /// is deliberate — the capability matrix stays a stable description of
    /// the format across feature combinations. Callers that need a
    /// build-aware answer must test `cfg!(feature = "rar-support")`
    /// themselves.
    pub fn capabilities(&self) -> FormatCapabilities {
        match self {
            ArchiveFormat::Zip => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                // R0070-0066 documentation: ZIP split-volume reading
                // (`.z01`, `.z02`, …) is detected at the *name* level
                // only — `Archive::detect_multipart` enumerates sibling
                // file names in the parent directory and never opens a
                // volume — but extraction across parts is not
                // implemented end-to-end. `Partial` is preserved here
                // so the capability matrix stays compatible with
                // existing tests; consumers that need a true
                // end-to-end answer should consult the
                // multipart-extraction docs in `docs/USER_MANUAL.md`.
                multipart_read: Support::Partial,
                multipart_write: Support::None,
                // Modify is a copy-on-write rewrite: entries are copied
                // into a fresh archive, so symlinks, hardlinks, and
                // special entries are dropped (FR-022) and metadata/layout
                // are normalized. Downgrade from `Full` to `Partial` so
                // callers see the lossy contract, matching the analogous
                // SevenZ report (R0080-0048). The rewrite path itself
                // remains supported.
                modification: Support::Partial,
                compression_read: Support::Full,
                compression_write: Support::Full,
            },
            ArchiveFormat::SevenZip => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                // `Full` since 2026-09-05 (OI-0080-004). A 7z volume set is a
                // plain byte split of one archive — the format specification
                // has no volume concept at all, and 7-Zip itself handles
                // `.001` sets through a signature-less pseudo-format whose
                // reader is a bare concatenating adapter. Verified on this
                // machine: `7zz a -v64k` output `cat`-ed back together `cmp`s
                // identical to the same content written unsplit. So the
                // backend reads a set through `VolumeChain`, which presents
                // the members as one source, and nothing format-specific was
                // needed. Writing split sets is a different job and is still
                // `None`.
                multipart_read: Support::Full,
                multipart_write: Support::None,
                // Modify is a copy-on-write rewrite that drops solid/block
                // layout, encryption, and several 7z-specific metadata
                // fields. Downgrade from `Full` to `Partial` so callers
                // see the lossy contract surface (R0070-0064). The
                // copy-on-write path itself remains supported.
                modification: Support::Partial,
                compression_read: Support::Full,
                compression_write: Support::Full,
            },
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => FormatCapabilities {
                encryption_read: Support::Full,
                encryption_write: Support::None,
                // Corrected 2026-08-26 from `Full`. RAR gets further than
                // ZIP does — UnRAR opens the set and lists it, rather than
                // ZIP's name-level enumeration — and unlike ZIP, extraction
                // across volumes works end to end, so this is `Full`.
                //
                // It was `Partial` until 2026-09-03, for a reason that turned
                // out to be one layer lower than it looked: a split file is
                // stored once per volume, and the listing surfaced each of
                // those headers as a separate entry sharing one path. Three
                // entries at one path closed every extraction route —
                // `extract_all` tripped the duplicate-output-path guard,
                // `extract_file` and `extract_to_memory` refused to
                // disambiguate, and `extract_by_ids` (the call those errors
                // recommend) failed with listing drift.
                //
                // `walk_entries` now folds continuation headers into their
                // predecessor using `RHDF_SPLITBEFORE`, which UnRAR was
                // already reporting and the wrapper was discarding. That
                // alone restored extraction: the SDK opens the continuation
                // volumes itself once it is asked for one logical entry, so
                // no volume-change callback was needed. Verified byte-for-byte
                // against `unrar x` on the committed three-volume fixture
                // (ticgit 3b4d15); `tests/rar_multivolume_test.rs` pins it.
                multipart_read: Support::Full,
                multipart_write: Support::None,
                modification: Support::None,
                // RAR is read-only through the main facade. The optional
                // `external-rar-create` Windows path adds creation but
                // is feature-gated and not represented here. Splitting
                // read/write makes the actual coverage explicit
                // (R0070-0065). R0001-0063: these values describe the RAR
                // format, not the build — a crate compiled without
                // `rar-support` still reports them while `Archive::open`
                // refuses RAR archives. Documented on `capabilities()`.
                compression_read: Support::Full,
                compression_write: Support::None,
            },
            ArchiveFormat::Tar => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression_read: Support::None,
                compression_write: Support::None,
            },
            ArchiveFormat::TarGzip
            | ArchiveFormat::TarBzip2
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZst
            | ArchiveFormat::TarLz4
            | ArchiveFormat::TarLzma => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression_read: Support::Full,
                compression_write: Support::Full,
            },
            // Standalone single-file compressed streams. The library
            // can decompress them through libarchive's raw filter, but
            // `Archive::create()` rejects them at the facade. Surface
            // that asymmetry in the capability matrix instead of
            // promising symmetric support (R0072-0003).
            ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Zst
            | ArchiveFormat::Lz4
            | ArchiveFormat::Lzma => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression_read: Support::Full,
                compression_write: Support::None,
            },
            ArchiveFormat::Iso => FormatCapabilities {
                encryption_read: Support::None,
                encryption_write: Support::None,
                multipart_read: Support::None,
                multipart_write: Support::None,
                modification: Support::None,
                compression_read: Support::None,
                compression_write: Support::None,
            },
        }
    }

    /// Check if format supports compression on either read or write side.
    ///
    /// New callers should pick [`Self::supports_compression_read`] or
    /// [`Self::supports_compression_write`] based on their actual
    /// question — the boolean form collapses RAR's read-only support
    /// into the same `true` as ZIP's symmetric coverage (R0070-0065).
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_compression(&self) -> bool {
        self.capabilities().compression() != Support::None
    }

    /// Compression support on the read/decompress side.
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_compression_read(&self) -> bool {
        self.capabilities().compression_read != Support::None
    }

    /// Compression support on the write/create side.
    pub fn supports_compression_write(&self) -> bool {
        self.capabilities().compression_write != Support::None
    }

    /// Check if format supports encryption on the read side.
    ///
    /// R0001-0064: this returns `true` for read-only-encrypted formats —
    /// ZIP, 7z, and RAR/RAR5 all decrypt but refuse to encrypt, and every
    /// one of them answers `true` here. (The previous wording claimed the
    /// exact inverse of the body.) Write-side callers must ask
    /// [`supports_encryption_write`](Self::supports_encryption_write)
    /// instead — `supports_encryption()` collapsed both directions and
    /// produced a misleading `true` for callers asking "can I create an
    /// encrypted archive of this format?" (R0069-0073).
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_encryption_read(&self) -> bool {
        self.capabilities().encryption_read != Support::None
    }

    /// Check if format supports encryption on the write/create side.
    pub fn supports_encryption_write(&self) -> bool {
        self.capabilities().encryption_write != Support::None
    }

    /// Check if format supports encryption (read or write).
    ///
    /// Kept for backward compatibility — new callers should pick
    /// [`supports_encryption_read`](Self::supports_encryption_read) or
    /// [`supports_encryption_write`](Self::supports_encryption_write)
    /// based on their actual question.
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_encryption(&self) -> bool {
        self.supports_encryption_read() || self.supports_encryption_write()
    }

    /// Check if format supports multi-part archives on the read side.
    ///
    /// Use the typed [`FormatCapabilities`] / [`Support`] enum
    /// (`capabilities().multipart_read`) to distinguish `Full` from
    /// `Partial` support — the boolean form collapses those into the
    /// same `true` (R0069-0072).
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_multipart_read(&self) -> bool {
        self.capabilities().multipart_read != Support::None
    }

    /// Check if format supports multi-part archives on the write side.
    pub fn supports_multipart_write(&self) -> bool {
        self.capabilities().multipart_write != Support::None
    }

    /// Check if format supports multi-part archives (read or write).
    ///
    /// Like [`capabilities`](Self::capabilities), the answer describes the
    /// format, not the current build: `Rar` / `Rar5` answer `true` even
    /// when the `rar-support` Cargo feature is off (R0001-0063).
    pub fn supports_multipart(&self) -> bool {
        self.supports_multipart_read() || self.supports_multipart_write()
    }

    /// Check if format supports modification
    pub fn can_modify(&self) -> bool {
        self.capabilities().modification != Support::None
    }

    /// Whether [`Archive::create`](crate::Archive::create) accepts this format.
    ///
    /// True for ZIP, 7z, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, TAR.ZST, TAR.LZ4,
    /// TAR.LZMA. False for the remaining variants because of explicit
    /// out-of-scope decisions:
    ///
    /// - **RAR / RAR5**: read-only via the main facade; creation requires
    ///   the optional `external-rar-create` feature on Windows.
    /// - **Standalone Gzip / Bzip2 / Xz / Zst / Lz4 / Lzma**: AD 0018
    ///   keeps standalone single-file compressor *creation* out of scope;
    ///   the codecs are only producible via the TAR.* compound formats.
    /// - **ISO**: not creatable here, and **the reason is not recorded
    ///   anywhere**. This bullet used to say "libarchive's ISO support is
    ///   read-only", which is false: the linked libarchive declares and
    ///   exports `archive_write_set_format_iso9660`. The crate simply never
    ///   binds that symbol, and no decision record says whether ISO writing
    ///   is out of scope or merely unbuilt. See
    ///   `docs/CAPABILITY_MATRIX.md`, "Reasons not established".
    ///
    /// TAR.ZST / TAR.LZ4 / TAR.LZMA creation requires a libarchive built
    /// with the matching codec library; a build without it fails loudly at
    /// writer construction rather than degrading (R0070-0041).
    ///
    /// `Archive::create` consults this predicate before constructing a
    /// writer backend so the rejection surfaces as a stable
    /// facade-level [`OperationBlocked`](crate::ArchiveError::OperationBlocked)
    /// instead of a backend-specific format error.
    pub fn can_create(self) -> bool {
        matches!(
            self,
            ArchiveFormat::Zip
                | ArchiveFormat::SevenZip
                | ArchiveFormat::Tar
                | ArchiveFormat::TarGzip
                | ArchiveFormat::TarBzip2
                | ArchiveFormat::TarXz
                | ArchiveFormat::TarZst
                | ArchiveFormat::TarLz4
                | ArchiveFormat::TarLzma
        )
    }

    /// Whether [`ArchiveFormat::detect`] should trust this format's
    /// extension after a missed magic-byte check (DCR-003).
    ///
    /// Only formats whose authoritative detection signal *is* the
    /// extension qualify: `.tar` (no consistent magic), `.iso` (PVD
    /// lives past the default read window), and the raw LZMA family
    /// (`.lzma` / `.tar.lzma` / `.tlz`, no stable short magic). Every
    /// other format must succeed magic-byte detection on its own.
    pub(crate) fn is_extension_fallback(self) -> bool {
        matches!(
            self,
            ArchiveFormat::Tar | ArchiveFormat::Iso | ArchiveFormat::Lzma | ArchiveFormat::TarLzma
        )
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
            ArchiveFormat::TarBzip2 => &["tar.bz2", "tbz2", "tb2"],
            ArchiveFormat::TarXz => &["tar.xz", "txz"],
            ArchiveFormat::TarZst => &["tar.zst", "tzst"],
            ArchiveFormat::TarLz4 => &["tar.lz4"],
            ArchiveFormat::TarLzma => &["tar.lzma", "tlz"],
            ArchiveFormat::Gzip => &["gz"],
            ArchiveFormat::Bzip2 => &["bz2"],
            ArchiveFormat::Xz => &["xz"],
            ArchiveFormat::Zst => &["zst"],
            ArchiveFormat::Lz4 => &["lz4"],
            ArchiveFormat::Lzma => &["lzma"],
            ArchiveFormat::Iso => &["iso"],
        }
    }

    /// Canonical staging-tempfile suffix for this format.
    ///
    /// Used by `Archive::open_at_offset` to preserve the source's archive
    /// extension when staging a payload to a tempfile. Compound tar-wrapped
    /// extensions (`.tar.gz`, `.tar.bz2`, `.tar.xz`) keep the compound
    /// suffix so libarchive's filter chain still sees the tar wrapper;
    /// single extensions are returned verbatim.
    pub(crate) fn suffix(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => ".zip",
            ArchiveFormat::SevenZip => ".7z",
            ArchiveFormat::Rar | ArchiveFormat::Rar5 => ".rar",
            ArchiveFormat::Tar => ".tar",
            ArchiveFormat::TarGzip => ".tar.gz",
            ArchiveFormat::TarBzip2 => ".tar.bz2",
            ArchiveFormat::TarXz => ".tar.xz",
            ArchiveFormat::TarZst => ".tar.zst",
            ArchiveFormat::TarLz4 => ".tar.lz4",
            ArchiveFormat::TarLzma => ".tar.lzma",
            ArchiveFormat::Gzip => ".gz",
            ArchiveFormat::Bzip2 => ".bz2",
            ArchiveFormat::Xz => ".xz",
            ArchiveFormat::Zst => ".zst",
            ArchiveFormat::Lz4 => ".lz4",
            ArchiveFormat::Lzma => ".lzma",
            ArchiveFormat::Iso => ".iso",
        }
    }
}

/// Stage suffix derived from a source path's filename. Compound
/// tar-wrapped suffixes (including `tgz` / `tbz2` / `tb2` / `txz`
/// short forms) are normalized to their long form so the staged
/// tempfile still routes through libarchive's filter chain. Single
/// extensions are preserved verbatim; absent or ambiguous extensions
/// (`.rar`, anything not in [`format_from_extension`]) keep their
/// original spelling, and a missing extension falls back to `.bin`.
pub(crate) fn stage_suffix_for(source: &Path) -> std::borrow::Cow<'static, str> {
    use std::borrow::Cow;
    if let Some(format) = format_from_extension(source) {
        return Cow::Borrowed(format.suffix());
    }
    match source.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => Cow::Owned(format!(".{}", ext.to_lowercase())),
        _ => Cow::Borrowed(".bin"),
    }
}

/// True when `path`'s lowercased extension is in `set`. Shared by
/// the extension-hint predicates below.
fn extension_in(path: &Path, set: &[&str]) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|ext| set.contains(&ext.as_str()))
}

/// Hint that the file might be ISO content. Widens
/// [`ArchiveFormat::detect`]'s magic-buffer read so the PVD at
/// offset 32769 is reachable.
fn extension_suggests_iso(path: &Path) -> bool {
    extension_in(path, &["iso", "bin", "img", "cd"])
}

/// Hint that the file might be a self-extracting archive. Used by
/// [`Archive::open`]'s SFX fallback when neither magic-byte
/// detection nor the extension-to-format map yields a result.
pub(crate) fn extension_suggests_executable(path: &Path) -> bool {
    extension_in(path, &["exe", "com", "scr", "app", "run", "sh", "bash"])
}

/// Promote a bare-codec detection (Gzip / Bzip2 / Xz / Zst / Lz4) to its
/// compound-tar variant when the path's filename ends with the
/// matching `.tar.gz` / `.tar.bz2` / `.tar.xz` family of suffixes.
/// Other formats pass through unchanged.
///
/// **Only codecs that [`ArchiveFormat::detect_from_bytes`] can return
/// get an arm here** (ti-9909d449, 2026-09-03). Both callers — the
/// magic-first branch of [`ArchiveFormat::detect`] and `modify()`'s
/// locked-handle mirror — feed this function nothing but a
/// `detect_from_bytes` result, so an arm for a format that probe never
/// yields is dead dispatch. That is why there is no `Lzma` arm: raw
/// LZMA has no stable short magic (see
/// [`ArchiveFormat::is_extension_fallback`]), so it is classified by
/// extension, and `format_from_extension` already answers `.tar.lzma` /
/// `.tlz` with [`ArchiveFormat::TarLzma`] directly without passing
/// through here.
///
/// The corollary, since nothing enforces it at compile time: adding an
/// LZMA magic probe to `detect_from_bytes` **must** come with an `Lzma`
/// arm here, or a magic-detected `foo.tar.lzma` would silently open as a
/// bare LZMA stream.
pub(crate) fn promote_to_compound_tar(detected: ArchiveFormat, path: &Path) -> ArchiveFormat {
    let Some(name) = path.file_name() else {
        return detected;
    };
    let name = name.to_string_lossy().to_lowercase();
    match detected {
        ArchiveFormat::Gzip if name.ends_with(".tar.gz") || name.ends_with(".tgz") => {
            ArchiveFormat::TarGzip
        }
        ArchiveFormat::Bzip2
            if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tb2") =>
        {
            ArchiveFormat::TarBzip2
        }
        ArchiveFormat::Xz if name.ends_with(".tar.xz") || name.ends_with(".txz") => {
            ArchiveFormat::TarXz
        }
        ArchiveFormat::Zst if name.ends_with(".tar.zst") || name.ends_with(".tzst") => {
            ArchiveFormat::TarZst
        }
        ArchiveFormat::Lz4 if name.ends_with(".tar.lz4") => ArchiveFormat::TarLz4,
        _ => detected,
    }
}

/// How many leading zstd skippable frames the magic probe will walk past
/// before giving up (OI-0080-008).
///
/// Each hop is an exact jump over a stored length, so the walk already
/// terminates; the budget additionally keeps a crafted file (a long chain
/// of empty skippable frames) from making detection walk the whole probe
/// window one 8-byte header at a time. Mainstream encoders emit at most a
/// single leading skippable frame (dictionary/metadata wrappers), so the
/// budget is generous in practice. A conforming stream carrying more than
/// this many leading skippable frames stays undetected even though
/// libarchive would read it — an accepted false negative, chosen over an
/// unbounded prefix walk.
const MAX_LEADING_SKIPPABLE_FRAMES: usize = 8;

/// Offset within `magic` of the first frame that is not a zstd skippable
/// frame, or `None` when the buffer ends inside the skippable prefix or
/// the prefix is longer than [`MAX_LEADING_SKIPPABLE_FRAMES`].
///
/// A skippable frame is a 4-byte magic in 0x184D2A50–0x184D2A5F followed
/// by a 4-byte little-endian payload length (RFC 8878 §3.1.2), so the
/// header size is known and the hop is exact. `None` deliberately covers
/// "cannot tell yet": returning an offset the caller could not read past
/// would turn a truncated probe into a guess.
fn zstd_frame_offset(magic: &[u8]) -> Option<usize> {
    /// 4-byte magic + 4-byte little-endian frame size.
    const SKIPPABLE_HEADER_LEN: usize = 8;
    const SKIPPABLE_MAGIC_BASE: u32 = 0x184D_2A50;
    /// The low nibble of the magic is the frame's user-chosen variant.
    const SKIPPABLE_MAGIC_MASK: u32 = 0xFFFF_FFF0;

    let mut offset = 0usize;
    // One more pass than the budget: the final pass is what reports the
    // frame that follows the last tolerated skippable frame.
    for _ in 0..=MAX_LEADING_SKIPPABLE_FRAMES {
        let rest = magic.get(offset..)?;
        let head = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?);
        if head & SKIPPABLE_MAGIC_MASK != SKIPPABLE_MAGIC_BASE {
            return Some(offset);
        }
        let frame_size = u32::from_le_bytes(rest.get(4..SKIPPABLE_HEADER_LEN)?.try_into().ok()?);
        offset = offset
            .checked_add(SKIPPABLE_HEADER_LEN)?
            .checked_add(frame_size as usize)?;
    }
    None
}

/// Validate the 22-byte end-of-central-directory record that an empty ZIP
/// carries at offset zero.
///
/// R0001-0036: the signature plus a 22-byte length was too weak — every
/// buffer beginning `PK\x05\x06` classified as ZIP. An EOCD *at offset
/// zero* can only belong to an empty, single-disk archive: no local
/// headers and no central directory can precede it. Require the fixed
/// fields to spell exactly that — both disk numbers zero, the
/// entries-on-this-disk count equal to the total count, and (because
/// nothing precedes the record) that count plus the central-directory size
/// and offset all zero.
///
/// `magic` is only a prefix of the file, so the trailing comment length is
/// deliberately *not* validated against a total size we do not have.
fn zip_eocd_header_ok(magic: &[u8]) -> bool {
    if magic.len() < 22 || !magic.starts_with(b"PK\x05\x06") {
        return false;
    }
    let le16 = |at: usize| u16::from_le_bytes([magic[at], magic[at + 1]]);
    let le32 =
        |at: usize| u32::from_le_bytes([magic[at], magic[at + 1], magic[at + 2], magic[at + 3]]);
    // Number of this disk (4..6) and the disk holding the start of the
    // central directory (6..8): single-disk archives store zero in both.
    if le16(4) != 0 || le16(6) != 0 {
        return false;
    }
    // Entries on this disk (8..10) must agree with the total entry count
    // (10..12), and both must be zero — an entry would need a local header
    // before the record we are sitting on.
    if le16(8) != le16(10) || le16(8) != 0 {
        return false;
    }
    // Central-directory size (12..16) and offset (16..20) follow from the
    // empty record: both zero.
    le32(12) == 0 && le32(16) == 0
}

/// Validate a 512-byte `ustar`/GNU tar header by its stored checksum.
///
/// The checksum field (bytes 148..156) holds an octal ASCII value equal
/// to the sum of every header byte with that field taken as ASCII
/// spaces. Some historic writers summed the bytes as signed `i8`; both
/// the unsigned and signed variants are accepted (R0080-0076).
fn tar_header_checksum_ok(header: &[u8]) -> bool {
    if header.len() < 512 {
        return false;
    }
    let Some(stored) = parse_tar_octal(&header[148..156]) else {
        return false;
    };
    let mut unsigned: u32 = 0;
    let mut signed: i32 = 0;
    for (i, &byte) in header[..512].iter().enumerate() {
        let v = if (148..156).contains(&i) { b' ' } else { byte };
        unsigned += u32::from(v);
        signed += i32::from(v as i8);
    }
    unsigned == stored || signed == stored as i32
}

/// Validate a 512-byte header as a pre-POSIX V7 (old-style) tar header.
///
/// V7 tar predates the `ustar` marker at offset 257, so extensionless or
/// misnamed V7 archives never match the `ustar` content branch even
/// though libarchive reads them (OI-0081-003). This probe runs only
/// after that branch fails and deliberately keeps the false-positive
/// surface at or below the tightened `ustar` gate (R0080-0076): the
/// dominant gate is the same 512-byte header checksum, and the remaining
/// checks only narrow the residual surface. All of the following must
/// hold:
///
/// * the `ustar` marker is absent (those headers belong to the stricter
///   branch above);
/// * the stored header checksum validates (see [`tar_header_checksum_ok`],
///   which accepts both the unsigned and historic signed-byte sums);
/// * the name field is non-empty (`name[0] != 0`);
/// * the size (124..136) and mtime (136..148) fields are octal numeric
///   fields;
/// * the linkflag byte at offset 156 is one of the V7 set: NUL (regular,
///   old convention), `b'0'` (regular), `b'1'` (hard link), `b'2'`
///   (symlink).
fn tar_v7_header_ok(header: &[u8]) -> bool {
    if header.len() < 512 {
        return false;
    }
    // The `ustar` dialect owns headers carrying the marker at offset 257;
    // this probe only classifies headers that lack it (R0080-0076).
    if &header[257..262] == b"ustar" {
        return false;
    }
    // The checksum is the strong gate; everything below narrows the
    // residual false-positive surface.
    if !tar_header_checksum_ok(header) {
        return false;
    }
    // Non-empty name (V7 stores the path in name[0..100]).
    if header[0] == 0 {
        return false;
    }
    // size (124..136) and mtime (136..148) must be octal numeric fields.
    if parse_tar_octal(&header[124..136]).is_none() || parse_tar_octal(&header[136..148]).is_none()
    {
        return false;
    }
    // linkflag at offset 156 restricted to the V7 set.
    matches!(header[156], 0 | b'0' | b'1' | b'2')
}

/// Parse an octal ASCII field (as used by tar numeric headers),
/// skipping leading spaces/NULs and stopping at the first trailing
/// space/NUL terminator. Returns `None` when the field carries no octal
/// digit, overflows, or holds anything but padding after the terminator.
///
/// R0001-0037: the terminator used to `break`, so the bytes behind it were
/// never inspected and "octal digits, NUL, arbitrary garbage" passed as a
/// valid numeric field — weakening the V7 tar header heuristic that leans
/// on it. Everything after the terminator must now be NUL or space. The
/// `_ => return None` rejection is kept, which also keeps GNU base-256
/// (high-bit) encoded fields out.
fn parse_tar_octal(field: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    let mut seen = false;
    let mut terminated = false;
    for &byte in field {
        match byte {
            // Past the terminator the field must be pure padding.
            _ if terminated => {
                if byte != b' ' && byte != 0 {
                    return None;
                }
            }
            b'0'..=b'7' => {
                value = value.checked_mul(8)?.checked_add(u32::from(byte - b'0'))?;
                seen = true;
            }
            b' ' | 0 if !seen => {}
            b' ' | 0 => terminated = true,
            _ => return None,
        }
    }
    seen.then_some(value)
}

/// Pure extension-derived format guess. Returns `Some(format)` when
/// the path's extension cleanly maps to a single format and `None`
/// otherwise. Diagnostic only: compare with [`crate::Archive::format`]
/// to detect cases where the extension lies about content. The
/// extension-only fallback that [`ArchiveFormat::detect`] actually
/// honours is narrower than this map — only `.tar`, `.iso`, and the
/// raw LZMA family pass through there.
///
/// **`.rar` is intentionally returned as `None`.** Both
/// [`ArchiveFormat::Rar`] and [`ArchiveFormat::Rar5`] advertise `rar`
/// as their extension; the variant cannot be inferred from the
/// extension alone, so this helper refuses to guess. Real RAR file
/// detection happens in [`ArchiveFormat::detect`] /
/// [`ArchiveFormat::detect_from_bytes`] where the magic-byte version
/// distinguishes the two.
///
/// **ISO ambiguity.** `.iso` is the only canonical ISO
/// extension and maps cleanly. `.bin` / `.img` / `.cd` are *hints*
/// that widen [`ArchiveFormat::detect`]'s magic-byte buffer (so the
/// PVD at offset 32769 becomes reachable) but they do not unambiguously
/// claim ISO content — they are commonly disc images, but also raw
/// binary blobs. This function therefore reports `None` for those
/// extensions.
pub fn format_from_extension(path: &Path) -> Option<ArchiveFormat> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(ArchiveFormat::TarGzip);
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tb2") {
        return Some(ArchiveFormat::TarBzip2);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return Some(ArchiveFormat::TarXz);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        return Some(ArchiveFormat::TarZst);
    }
    if name.ends_with(".tar.lz4") {
        return Some(ArchiveFormat::TarLz4);
    }
    if name.ends_with(".tar.lzma") || name.ends_with(".tlz") {
        return Some(ArchiveFormat::TarLzma);
    }
    match ext.as_str() {
        "zip" => Some(ArchiveFormat::Zip),
        // `.rar` is ambiguous between Rar (4.x) and Rar5 — leave it
        // to magic-byte detection.
        "rar" => None,
        "7z" => Some(ArchiveFormat::SevenZip),
        "tar" => Some(ArchiveFormat::Tar),
        "gz" => Some(ArchiveFormat::Gzip),
        "bz2" => Some(ArchiveFormat::Bzip2),
        "xz" => Some(ArchiveFormat::Xz),
        "zst" => Some(ArchiveFormat::Zst),
        "lz4" => Some(ArchiveFormat::Lz4),
        "lzma" => Some(ArchiveFormat::Lzma),
        // Only `.iso` claims ISO content unambiguously.
        // `.bin` / `.img` / `.cd` are detect-window
        // hints, not format claims.
        "iso" => Some(ArchiveFormat::Iso),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// Canonical list of every `ArchiveFormat` variant. Tests that
    /// iterate over all variants must use this constant so a new
    /// variant is exercised everywhere with a single edit
    /// (R0078-0082).
    const ALL_FORMATS: &[ArchiveFormat] = &[
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

    /// Helper: write bytes to a temp file and return the path
    fn temp_file_with_bytes(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    /// Write `value` as zero-padded octal ASCII into `field`, followed by
    /// a NUL terminator, matching the tar numeric-field convention. Used
    /// only by the V7 tar synthesizer (OI-0081-003).
    fn write_octal_field(field: &mut [u8], value: u64) {
        let digits = field.len() - 1; // leave room for the NUL terminator
        let text = format!("{value:0digits$o}");
        assert!(text.len() <= digits, "octal value too large for field");
        field[..digits].copy_from_slice(text.as_bytes());
        field[digits] = 0;
    }

    /// Recompute and store the tar header checksum over `buf[..512]`,
    /// treating the checksum field (148..156) as spaces and encoding the
    /// unsigned sum as six octal digits + NUL + space — the layout
    /// [`tar_header_checksum_ok`] validates.
    fn set_v7_checksum(buf: &mut [u8]) {
        for byte in &mut buf[148..156] {
            *byte = b' ';
        }
        let sum: u32 = buf[..512].iter().map(|&b| u32::from(b)).sum();
        buf[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    }

    /// Build a minimal pre-POSIX V7 tar image: one 512-byte header for a
    /// single regular file, one 512-byte content block, and two 512-byte
    /// zero trailer blocks. The header carries no `ustar` marker and a
    /// correctly computed checksum (OI-0081-003).
    fn build_v7_tar(name: &[u8], contents: &[u8]) -> Vec<u8> {
        assert!(name.len() < 100, "V7 name field is 100 bytes");
        assert!(contents.len() <= 512, "helper writes one content block");
        let mut buf = vec![0u8; 512 * 4];
        // name (0..100)
        buf[..name.len()].copy_from_slice(name);
        // mode (100..108), uid (108..116), gid (116..124): octal ASCII.
        write_octal_field(&mut buf[100..108], 0o644);
        write_octal_field(&mut buf[108..116], 0);
        write_octal_field(&mut buf[116..124], 0);
        // size (124..136), mtime (136..148): octal ASCII.
        write_octal_field(&mut buf[124..136], contents.len() as u64);
        write_octal_field(&mut buf[136..148], 0);
        // linkflag (156): '0' == regular file (V7 set).
        buf[156] = b'0';
        // checksum (148..156): computed last, over the finished header.
        set_v7_checksum(&mut buf);
        // Content block follows the header.
        buf[512..512 + contents.len()].copy_from_slice(contents);
        buf
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
        // A complete 22-byte end-of-central-directory record (empty ZIP):
        // the signature, the fixed fields, and a zero-length comment. Four
        // signature bytes alone no longer classify as ZIP (R0081-0014).
        let magic: &[u8] = &[
            0x50, 0x4B, 0x05, 0x06, // "PK\x05\x06"
            0x00, 0x00, // number of this disk
            0x00, 0x00, // disk with the central directory
            0x00, 0x00, // central-directory entries on this disk
            0x00, 0x00, // total central-directory entries
            0x00, 0x00, 0x00, 0x00, // size of the central directory
            0x00, 0x00, 0x00, 0x00, // offset of the central directory
            0x00, 0x00, // ZIP-file comment length
        ];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_zip_short_eocd_rejected() {
        // Fewer than the fixed 22 EOCD bytes is not a complete record, so
        // the bare signature must not be classified as ZIP (R0081-0014).
        let magic = b"PK\x05\x06rest_of_data";
        assert!(ArchiveFormat::detect_from_bytes(magic).is_err());
    }

    #[test]
    fn test_detect_zip_eocd_inconsistent_fields_rejected() {
        // R0001-0036: the fixed EOCD fields must describe an empty,
        // single-disk archive. An end-of-central-directory record sitting at
        // offset zero has nothing in front of it, so a non-zero disk number,
        // entry count, central-directory size, or central-directory offset is
        // structurally impossible and must not classify as ZIP.
        let mut base = [0u8; 22];
        base[..4].copy_from_slice(b"PK\x05\x06");
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&base).unwrap(),
            ArchiveFormat::Zip,
            "the all-zero empty-archive record must still classify"
        );

        for offset in 4..20 {
            let mut buf = base;
            buf[offset] = 0x01;
            assert!(
                ArchiveFormat::detect_from_bytes(&buf).is_err(),
                "a non-zero byte at offset {offset} must reject the record"
            );
        }

        // The ZIP-file comment length (20..22) is deliberately not
        // validated: the probe sees only a prefix of the file, so there is
        // no total size to check the comment against.
        let mut commented = base;
        commented[20] = 0x10;
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&commented).unwrap(),
            ArchiveFormat::Zip
        );
    }

    #[test]
    fn test_detect_zip_data_descriptor_rejected() {
        // R0080-0072: the data-descriptor signature is an intra-stream
        // record, not a standalone archive header, so it must not be
        // classified as a ZIP at offset zero.
        let magic = b"PK\x07\x08rest_of_data";
        assert!(ArchiveFormat::detect_from_bytes(magic).is_err());
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
        let magic: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
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
    fn test_detect_zst_magic() {
        let magic: &[u8] = &[0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Zst
        );
    }

    /// Build a zstd skippable frame (RFC 8878 §3.1.2): a magic in
    /// 0x184D2A50–0x184D2A5F, a 4-byte little-endian payload length, then
    /// that many payload bytes.
    fn zstd_skippable_frame(variant: u8, payload_len: usize) -> Vec<u8> {
        let magic = 0x184D_2A50u32 | u32::from(variant & 0x0F);
        let mut frame = Vec::with_capacity(8 + payload_len);
        frame.extend_from_slice(&magic.to_le_bytes());
        frame.extend_from_slice(&(payload_len as u32).to_le_bytes());
        frame.extend(std::iter::repeat_n(0xA5u8, payload_len));
        frame
    }

    const ZSTD_FRAME_MAGIC: &[u8] = &[0x28, 0xB5, 0x2F, 0xFD];

    #[test]
    fn test_detect_zst_behind_leading_skippable_frame() {
        // RFC 8878 §3.1.2 allows a skippable frame in the first position,
        // so the standard frame magic may sit past offset zero. Such a
        // file used to reach neither the magic branch nor the extension
        // fallback (`Zst` is not in it) and was reported as an unknown
        // format, leaving it unopenable by any path.
        let mut bytes = zstd_skippable_frame(0x00, 9);
        bytes.extend_from_slice(ZSTD_FRAME_MAGIC);
        bytes.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&bytes).unwrap(),
            ArchiveFormat::Zst
        );
    }

    #[test]
    fn test_detect_zst_behind_zero_length_and_high_variant_skippable_frames() {
        // A zero-length payload and the top variant nibble (0x5F) are both
        // legal; neither may stall or divert the walk.
        let mut bytes = zstd_skippable_frame(0x0F, 0);
        bytes.extend_from_slice(&zstd_skippable_frame(0x07, 3));
        bytes.extend_from_slice(ZSTD_FRAME_MAGIC);
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&bytes).unwrap(),
            ArchiveFormat::Zst
        );
    }

    #[test]
    fn test_detect_zst_skippable_frame_budget_boundary() {
        // Exactly the budget is still detected; one frame more falls
        // through to "undetected" rather than walking an unbounded chain.
        let at_budget: Vec<u8> = (0..MAX_LEADING_SKIPPABLE_FRAMES)
            .flat_map(|_| zstd_skippable_frame(0x00, 0))
            .chain(ZSTD_FRAME_MAGIC.iter().copied())
            .collect();
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&at_budget).unwrap(),
            ArchiveFormat::Zst
        );

        let over_budget: Vec<u8> = (0..MAX_LEADING_SKIPPABLE_FRAMES + 1)
            .flat_map(|_| zstd_skippable_frame(0x00, 0))
            .chain(ZSTD_FRAME_MAGIC.iter().copied())
            .collect();
        assert!(ArchiveFormat::detect_from_bytes(&over_budget).is_err());
    }

    #[test]
    fn test_detect_zst_skippable_prefix_truncated_probe_is_undetected() {
        // Ends inside the skippable header (no length field yet).
        let short_header = &zstd_skippable_frame(0x00, 0)[..6];
        assert!(ArchiveFormat::detect_from_bytes(short_header).is_err());

        // Header complete, but the declared payload runs past the probe
        // window, so the following frame magic is not visible.
        let mut truncated_payload = zstd_skippable_frame(0x00, 64);
        truncated_payload.truncate(20);
        assert!(ArchiveFormat::detect_from_bytes(&truncated_payload).is_err());

        // Prefix consumed exactly, nothing after it: still undetected
        // (a lone skippable frame is not evidence of a zstd stream).
        let prefix_only = zstd_skippable_frame(0x00, 4);
        assert!(ArchiveFormat::detect_from_bytes(&prefix_only).is_err());
    }

    #[test]
    fn test_detect_zst_skippable_prefix_does_not_widen_the_zst_claim() {
        // The walk must not turn "skippable frame followed by something
        // that is not the zstd frame magic" into a zstd claim. Neither a
        // foreign archive magic (which the other branches only accept at
        // offset zero) nor noise may come back as `Zst`.
        let mut zip_after = zstd_skippable_frame(0x00, 0);
        zip_after.extend_from_slice(b"PK\x03\x04");
        assert!(ArchiveFormat::detect_from_bytes(&zip_after).is_err());

        let mut noise = zstd_skippable_frame(0x00, 0);
        noise.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(ArchiveFormat::detect_from_bytes(&noise).is_err());
    }

    #[test]
    fn test_detect_lz4_magic() {
        let magic: &[u8] = &[0x04, 0x22, 0x4D, 0x18, 0x00, 0x00];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Lz4
        );
    }

    #[test]
    fn test_detect_lz4_legacy_magic() {
        // Legacy LZ4 frame magic 0x184C2102 (little-endian bytes
        // 0x02 0x21 0x4C 0x18). libarchive still reads legacy frames, so
        // content detection recognises them too (R0081-0012).
        let magic: &[u8] = &[0x02, 0x21, 0x4C, 0x18, 0x00, 0x00];
        assert_eq!(
            ArchiveFormat::detect_from_bytes(magic).unwrap(),
            ArchiveFormat::Lz4
        );
    }

    #[test]
    fn test_detect_iso_magic_exact_boundary() {
        // The ISO branch reads the version byte at offset 32774, so it needs
        // at least 32775 bytes. A buffer one byte short must fall through
        // even when the PVD type byte and `CD001` identifier are present
        // (R0081-0009).
        let mut short = vec![0u8; 32774];
        short[32768] = 0x01;
        short[32769..32774].copy_from_slice(b"CD001");
        assert!(ArchiveFormat::detect_from_bytes(&short).is_err());

        let mut exact = vec![0u8; 32775];
        exact[32768] = 0x01;
        exact[32769..32774].copy_from_slice(b"CD001");
        exact[32774] = 0x01;
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&exact).unwrap(),
            ArchiveFormat::Iso
        );
    }

    #[test]
    fn test_detect_tar_magic() {
        // R0080-0076: a valid ustar header whose stored checksum matches
        // the computed sum. The former all-zero fixture (only the ustar
        // bytes set) is now correctly rejected — see
        // `test_detect_tar_magic_bad_checksum_rejected`.
        let mut buf = vec![0u8; 512];
        buf[0..4].copy_from_slice(b"foo\0");
        buf[257..263].copy_from_slice(b"ustar\0");
        buf[263..265].copy_from_slice(b"00");
        for byte in &mut buf[148..156] {
            *byte = b' ';
        }
        let sum: u32 = buf.iter().map(|&byte| u32::from(byte)).sum();
        buf[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&buf).unwrap(),
            ArchiveFormat::Tar
        );
    }

    #[test]
    fn test_detect_tar_magic_bad_checksum_rejected() {
        // An all-zero header with only the ustar marker set has a blank
        // (zero) checksum field that never matches the computed sum, so
        // detection must reject it (R0080-0076).
        let mut buf = vec![0u8; 512];
        buf[257..262].copy_from_slice(b"ustar");
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    // ── detect_from_bytes: pre-POSIX V7 (old-style) tar (OI-0081-003) ──

    #[test]
    fn test_detect_v7_tar_content() {
        // OI-0081-003: pre-POSIX V7 tar has no `ustar` marker, so it must
        // be recognised by the strict V7 checksum probe. The synthesized
        // 512-byte header (non-empty name, octal size/mtime, '0'
        // linkflag, correct checksum) plus one content block and two zero
        // trailer blocks (R0080-0076).
        let buf = build_v7_tar(b"hello.txt", b"hi\n");
        // The header must genuinely lack the ustar marker, otherwise the
        // stricter branch — not the V7 probe — would be doing the work.
        assert_ne!(&buf[257..262], b"ustar");
        assert_eq!(
            ArchiveFormat::detect_from_bytes(&buf).unwrap(),
            ArchiveFormat::Tar
        );
    }

    #[test]
    fn test_detect_v7_tar_all_zero_rejected() {
        // The all-zero 512-byte buffer must stay rejected: the V7 probe
        // fails both the empty-name gate and the checksum gate, and no
        // other branch matches (R0080-0076, OI-0081-003).
        let buf = vec![0u8; 512];
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_bad_checksum_rejected() {
        // A plausible V7 header (non-empty name, octal fields, valid
        // linkflag) whose stored checksum is corrupted must be rejected:
        // the checksum is the dominant gate (OI-0081-003, R0080-0076).
        let mut buf = build_v7_tar(b"data.bin", b"payload");
        // Corrupt the stored checksum without recomputing so it no longer
        // matches the sum over the header.
        buf[148] = if buf[148] == b'7' { b'0' } else { buf[148] + 1 };
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_random_bytes_rejected() {
        // Pseudo-random bytes with an invalid (non-octal) size field and
        // an out-of-set linkflag must be rejected even though the name is
        // non-empty; the checksum will not validate either (OI-0081-003).
        let mut buf = vec![0u8; 512];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = ((i * 37 + 13) % 251) as u8;
        }
        buf[0] = b'g'; // non-empty name so the name gate is not the reason
        buf[124] = b'9'; // size field: '9' is not an octal digit
        buf[156] = b'x'; // linkflag: not in the V7 set {NUL,'0','1','2'}
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_empty_name_rejected() {
        // Isolate the name gate: a header with a valid (recomputed)
        // checksum but an empty name must still be rejected (OI-0081-003).
        let mut buf = build_v7_tar(b"named.txt", b"x");
        for byte in &mut buf[..100] {
            *byte = 0;
        }
        set_v7_checksum(&mut buf); // keep the checksum valid after zeroing
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_bad_linkflag_rejected() {
        // Isolate the linkflag gate: a valid header whose linkflag is
        // outside the V7 set (here '5', a POSIX directory typeflag) must
        // be rejected even with a recomputed checksum (OI-0081-003).
        let mut buf = build_v7_tar(b"dir.entry", b"");
        buf[156] = b'5';
        set_v7_checksum(&mut buf);
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_non_octal_size_rejected() {
        // Isolate the numeric-field gate: a non-octal byte in the size
        // field must be rejected even with a recomputed checksum
        // (OI-0081-003).
        let mut buf = build_v7_tar(b"num.bin", b"z");
        buf[124] = b'9'; // '9' is not an octal digit
        set_v7_checksum(&mut buf);
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_detect_v7_tar_trailing_garbage_in_size_rejected() {
        // R0001-0037: a numeric field is digits, a NUL/space terminator, and
        // then nothing but padding. Terminating the size field early and
        // parking garbage behind it must be rejected even though the
        // checksum is recomputed and every other V7 gate still passes.
        let mut buf = build_v7_tar(b"garbage.bin", b"q");
        buf[134] = 0; // terminate the size field one byte early
        buf[135] = b'X'; // …then garbage where only padding may live
        set_v7_checksum(&mut buf);
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
    }

    #[test]
    fn test_parse_tar_octal_rejects_trailing_garbage() {
        // R0001-0037: the parser used to `break` at the first terminator and
        // never look at the rest of the field.
        assert_eq!(parse_tar_octal(b"000644\0 "), Some(0o644));
        assert_eq!(parse_tar_octal(b"  644 \0"), Some(0o644));
        assert_eq!(parse_tar_octal(b"000644\0X"), None);
        assert_eq!(parse_tar_octal(b"644 12345"), None);
        // Pre-existing rejections stay: a non-octal digit, GNU base-256
        // (high-bit) encoded fields, and a field carrying no digit at all.
        assert_eq!(parse_tar_octal(b"0009\0\0\0"), None);
        assert_eq!(parse_tar_octal(&[0x80, 0, 0, 0, 0, 0, 0, 1]), None);
        assert_eq!(parse_tar_octal(b"        "), None);
    }

    #[test]
    fn test_detect_v7_tar_does_not_shadow_ustar_bad_checksum() {
        // A ustar-marked header with a bad checksum must remain rejected:
        // the V7 probe explicitly excludes ustar-marked headers, so it
        // cannot rescue what the stricter branch rejects (R0080-0076).
        let mut buf = vec![0u8; 512];
        buf[0] = b'f'; // non-empty name
        buf[257..263].copy_from_slice(b"ustar\0");
        write_octal_field(&mut buf[124..136], 1); // valid octal size
        write_octal_field(&mut buf[136..148], 0); // valid octal mtime
        buf[156] = b'0';
        // Deliberately leave the checksum field zero (invalid).
        assert!(ArchiveFormat::detect_from_bytes(&buf).is_err());
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
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar.gz", gzip_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarGzip
        );
    }

    #[test]
    fn test_detect_tgz_by_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tgz", gzip_bytes);
        assert_eq!(
            ArchiveFormat::detect(&path).unwrap(),
            ArchiveFormat::TarGzip
        );
    }

    #[test]
    fn test_detect_tar_zst_behind_leading_skippable_frame() {
        // The compound-tar promotion runs on whatever `detect_from_bytes`
        // returned, so a `.tar.zst` opening with a skippable frame must
        // reach `TarZst` and not the "unknown format" error.
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = zstd_skippable_frame(0x00, 12);
        bytes.extend_from_slice(&[0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00]);
        let path = temp_file_with_bytes(tmp.path(), "test_detect.tar.zst", &bytes);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::TarZst);

        let plain = temp_file_with_bytes(tmp.path(), "skipped.zst", &bytes);
        assert_eq!(ArchiveFormat::detect(&plain).unwrap(), ArchiveFormat::Zst);
    }

    #[test]
    fn test_detect_zst_extension_alone_is_still_not_enough() {
        // Content-based detection stays the policy: `Zst` is deliberately
        // absent from the extension-fallback set, so a garbage `.zst`
        // (or a truncated skippable prefix) must fail detection rather
        // than be accepted on its name and fail later with a worse error.
        let tmp = tempfile::tempdir().unwrap();
        let garbage = temp_file_with_bytes(tmp.path(), "junk.zst", &[0xDE, 0xAD, 0xBE, 0xEF, 0x01]);
        assert!(ArchiveFormat::detect(&garbage).is_err());

        let truncated_prefix = temp_file_with_bytes(
            tmp.path(),
            "truncated.tar.zst",
            &zstd_skippable_frame(0x00, 0),
        );
        assert!(ArchiveFormat::detect(&truncated_prefix).is_err());
    }

    // ── detect (file-based): tiny-file extension fallback (R0076-0093) ──

    #[test]
    fn test_detect_tiny_file_with_zip_extension_falls_back_to_extension() {
        let tmp = tempfile::tempdir().unwrap();
        // 2 bytes: below the 4-byte magic-probe minimum, so content
        // cannot classify — the extension must still be consulted.
        let path = temp_file_with_bytes(tmp.path(), "tiny.zip", b"PK");
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Zip);
    }

    #[test]
    fn test_detect_tiny_file_with_tar_extension_falls_back_to_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_file_with_bytes(tmp.path(), "tiny.tar", b"..");
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Tar);
    }

    #[test]
    fn test_detect_plain_gzip_not_tar() {
        let tmp = tempfile::tempdir().unwrap();
        let gzip_bytes: &[u8] = &[0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
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

    /// AD 0062 A.5: when the path's extension hints at ISO, the
    /// detect path widens its magic buffer to 33 KiB so the PVD
    /// `CD001` marker at offset 32769 is reachable.
    #[test]
    fn test_detect_iso_via_pvd_with_iso_extension_widens_buffer() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = vec![0u8; 32775];
        // A minimal Primary Volume Descriptor: the type byte, the
        // `CD001` identifier at 32769, and the version byte right after.
        bytes[32768] = 0x01;
        bytes[32769..32774].copy_from_slice(b"CD001");
        bytes[32774] = 0x01;
        let path = temp_file_with_bytes(tmp.path(), "image.iso", &bytes);
        assert_eq!(ArchiveFormat::detect(&path).unwrap(), ArchiveFormat::Iso);
    }

    /// AD 0062 A.5 documented limitation: an ISO file that lives at a
    /// non-ISO extension (here `.dat`) detects as Unknown — the cheap
    /// 512-byte default never reaches the PVD. False negative is
    /// accepted in exchange for the cheap default.
    #[test]
    fn test_detect_iso_via_pvd_without_iso_extension_misses() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = vec![0u8; 32775];
        bytes[32768] = 0x01;
        bytes[32769..32774].copy_from_slice(b"CD001");
        bytes[32774] = 0x01;
        let path = temp_file_with_bytes(tmp.path(), "mystery.dat", &bytes);
        let err = ArchiveFormat::detect(&path).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unknown"));
    }

    /// AD 0062 A.5: `.bin` / `.img` / `.cd` are also recognised as
    /// ISO-extension hints so common disc-image filenames still
    /// detect when their extension isn't literally `.iso`.
    #[test]
    fn test_detect_iso_via_pvd_with_bin_extension_widens_buffer() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = vec![0u8; 32775];
        bytes[32768] = 0x01;
        bytes[32769..32774].copy_from_slice(b"CD001");
        bytes[32774] = 0x01;
        let path = temp_file_with_bytes(tmp.path(), "image.bin", &bytes);
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
        // R0076-0093: a too-small file with a *known* archive extension
        // now falls back to extension detection (see the tiny-file
        // fallback tests above), so the too-small error is only
        // reachable with an unmapped or missing extension.
        let tmp = tempfile::tempdir().unwrap();
        let path = temp_file_with_bytes(tmp.path(), "tiny.dat", &[0x50]); // only 1 byte
        let err = ArchiveFormat::detect(&path).unwrap_err();
        assert!(
            err.to_string().contains("too small"),
            "expected the too-small diagnostic, got: {err}"
        );

        let path = temp_file_with_bytes(tmp.path(), "tiny", &[0x50]);
        assert!(ArchiveFormat::detect(&path).is_err());
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

        // 7z reads split sets since OI-0080-004: `.7z.001`, `.7z.002`, … is a
        // byte split of one archive, so the backend reads the concatenation.
        assert!(ArchiveFormat::SevenZip.supports_multipart());
        assert_eq!(
            ArchiveFormat::SevenZip.capabilities().multipart_read,
            Support::Full
        );
        assert_eq!(
            ArchiveFormat::SevenZip.capabilities().multipart_write,
            Support::None,
            "reading a split set and writing one are different jobs"
        );
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
        for fmt in ALL_FORMATS {
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
        // Per MADR-0027: encrypted archive creation is rejected; read-only support.
        assert_eq!(caps.encryption_write, Support::None);
        assert_eq!(caps.multipart_read, Support::Partial);
        assert_eq!(caps.multipart_write, Support::None);
        // R0080-0048: ZIP modify is a lossy copy-on-write rewrite.
        assert_eq!(caps.modification, Support::Partial);
        assert_eq!(caps.compression(), Support::Full);
        assert_eq!(caps.compression_read, Support::Full);
        assert_eq!(caps.compression_write, Support::Full);
    }

    #[test]
    fn test_create_with_password_rejected_for_zip() {
        use crate::options::CompressionOptions;
        let tmp = std::env::temp_dir().join("ua_encrypted_create_rejected.zip");
        let _ = std::fs::remove_file(&tmp);
        let opts = CompressionOptions {
            format: ArchiveFormat::Zip,
            password: Some(crate::Password::new("secret")),
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
        // R0070-0064: 7z modify is a copy-on-write rewrite that drops
        // encryption, solid layout, and other 7z-specific metadata.
        assert_eq!(caps.modification, Support::Partial);
    }

    #[test]
    fn test_rar_read_only_capabilities() {
        for fmt in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
            let caps = fmt.capabilities();
            assert_eq!(caps.encryption_read, Support::Full);
            assert_eq!(caps.encryption_write, Support::None);
            // Not `Full`: UnRAR lists a volume set but no extraction
            // path can reassemble one. See the capability comment and
            // `tests/rar_multivolume_test.rs`.
            // Full since 2026-09-03: a complete volume set lists as one
            // logical entry and extracts through every public route
            // (ticgit 3b4d15). `tests/rar_multivolume_test.rs` is the
            // end-to-end proof; this is the matrix's own copy of the answer.
            assert_eq!(caps.multipart_read, Support::Full);
            assert_eq!(caps.modification, Support::None);
            // R0070-0065: RAR is decode-only through the main facade.
            assert_eq!(caps.compression_read, Support::Full);
            assert_eq!(caps.compression_write, Support::None);
        }
    }

    #[test]
    fn test_tar_no_capabilities() {
        let caps = ArchiveFormat::Tar.capabilities();
        assert_eq!(caps.encryption_read, Support::None);
        assert_eq!(caps.compression(), Support::None);
        assert_eq!(caps.modification, Support::None);
    }

    #[test]
    fn test_compressed_tar_only_compression() {
        // All six compressed-tar variants read AND write since the
        // zstd/lz4/lzma write filters were wired (R0075-0031 closure).
        for fmt in [
            ArchiveFormat::TarGzip,
            ArchiveFormat::TarBzip2,
            ArchiveFormat::TarXz,
            ArchiveFormat::TarZst,
            ArchiveFormat::TarLz4,
            ArchiveFormat::TarLzma,
        ] {
            let caps = fmt.capabilities();
            assert_eq!(caps.compression(), Support::Full);
            assert_eq!(caps.encryption_read, Support::None);
            assert_eq!(caps.modification, Support::None);
        }
    }

    #[test]
    fn test_boolean_methods_delegate_to_capabilities() {
        // Verify boolean methods stay in sync with capabilities()
        for fmt in ALL_FORMATS {
            let caps = fmt.capabilities();
            assert_eq!(
                fmt.supports_compression(),
                caps.compression() != Support::None,
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
