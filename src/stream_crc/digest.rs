//! Digest layer: what a stream checksum *is*, and how one is recovered
//! from a raw byte or bit window.
//!
//! This is the value seam of [`crate::stream_crc`]. It owns
//! [`StreamChecksum`] and [`CheckType`], the type-gated accessors that
//! keep callers from reading a field the format never populated, and the
//! two search primitives that lift a digest out of a buffer.
//!
//! Nothing here opens a file, seeks, or knows a container's framing
//! rules — that is [`super::codec`]'s job, and format detection is
//! [`super::detect`]'s. Every function in this module is a pure function
//! of the bytes handed to it, which is what makes the bit-level bzip2
//! scan testable without a fixture on disk.

/// Stream-level checksum information.
///
/// **Field-vs-`check_type` invariants.** The struct
/// tolerates contradictory states (`check_type == Crc32` with no
/// `crc32`, etc.) on its public fields. The accessor helpers below
/// (`crc32_value`, `crc64_value`) gate on `check_type` so callers
/// don't have to pattern-match the option-bag manually:
///
/// | `check_type` | populated fields                |
/// |--------------|---------------------------------|
/// | `Crc32`      | `crc32` (and usually `uncompressed_size`)        |
/// | `Crc64`      | `crc64`                                          |
/// | `None`       | neither `crc32` nor `crc64`; `uncompressed_size` may still be set if the format records it elsewhere |
/// | `Sha256`     | (reserved — no current backend emits this)       |
/// | `Unknown`    | nothing reliable; do not act on the option fields |
///
/// **Construction (OI-0076-005).** The struct is `#[non_exhaustive]`: it is
/// an *output* type, decoded from a stream trailer by the codecs in
/// `stream_crc`, so no constructor is offered and none is owed. The fields
/// stay `pub` and readable — prefer [`crc32_value`](Self::crc32_value) /
/// [`crc64_value`](Self::crc64_value), which respect `check_type`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StreamChecksum {
    /// CRC32 value from the stream (if available)
    pub crc32: Option<u32>,

    /// CRC64 value from the stream (if available, XZ only)
    pub crc64: Option<u64>,

    /// Uncompressed size from the stream metadata.
    ///
    /// **Gzip caveat.** For gzip this is the trailer `ISIZE`, i.e. the
    /// uncompressed size *modulo 2^32* (RFC 1952) — exact only for
    /// payloads below 4 GiB. A member of 4 GiB or larger reports its
    /// size mod 2^32, not the true size. (Bzip2 and Xz do not record an
    /// uncompressed size and leave this `None`.)
    pub uncompressed_size: Option<u64>,

    /// Type of checksum found
    pub check_type: CheckType,
}

impl StreamChecksum {
    /// Return the CRC32 value only when [`Self::check_type`] is
    /// [`CheckType::Crc32`].
    ///
    /// Use this in preference to inspecting `crc32` directly so a
    /// future variant change (XZ block with CRC32 marked but the
    /// option fields not populated) doesn't silently report a stale
    /// or contradictory value.
    pub fn crc32_value(&self) -> Option<u32> {
        match self.check_type {
            CheckType::Crc32 => self.crc32,
            _ => None,
        }
    }

    /// Return the CRC64 value only when [`Self::check_type`] is
    /// [`CheckType::Crc64`].
    pub fn crc64_value(&self) -> Option<u64> {
        match self.check_type {
            CheckType::Crc64 => self.crc64,
            _ => None,
        }
    }
}

/// Type of checksum in the stream
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckType {
    /// No checksum present
    None,
    /// CRC32 checksum
    Crc32,
    /// CRC64 checksum
    Crc64,
    /// SHA-256 checksum
    Sha256,
    /// Unknown or unsupported checksum type
    Unknown,
}

/// Helper function to find the last occurrence of a byte pattern in data
pub(super) fn find_pattern_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// Bit-level search for the bzip2 end-of-stream marker, returning the
/// 32-bit stream CRC stored immediately after it.
///
/// bzip2 packs blocks back-to-back with no byte-alignment padding, so
/// the 48-bit EOS magic (`0x177245385090`) can begin at any bit
/// offset. Scans the buffer with a rolling 48-bit window (MSB-first,
/// matching bzip2's bit order) and keeps the *last* match whose
/// trailing 32 CRC bits still fit in the buffer, mirroring the
/// byte-aligned fast path's last-occurrence semantics (R0079-0009).
pub(super) fn find_bzip2_eos_crc_bitwise(tail: &[u8]) -> Option<u32> {
    const EOS_MAGIC: u64 = 0x1772_4538_5090;
    const WINDOW_MASK: u64 = (1 << 48) - 1;

    let bit_at = |pos: usize| (tail[pos / 8] >> (7 - (pos % 8))) & 1;

    let total_bits = tail.len() * 8;
    let mut window = 0u64;
    let mut last_match = None;
    for pos in 0..total_bits {
        window = ((window << 1) | u64::from(bit_at(pos))) & WINDOW_MASK;
        // `pos` is the window's final bit; the candidate match starts
        // 47 bits earlier and needs 32 CRC bits after it.
        if pos + 1 >= 48 {
            let start = pos + 1 - 48;
            if window == EOS_MAGIC && start + 80 <= total_bits {
                last_match = Some(start);
            }
        }
    }

    let crc_start = last_match? + 48;
    let mut crc32 = 0u32;
    for pos in crc_start..crc_start + 32 {
        crc32 = (crc32 << 1) | u32::from(bit_at(pos));
    }
    Some(crc32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_pattern_last() {
        let data = b"hello world test";
        assert_eq!(find_pattern_last(data, b"world"), Some(6));
        assert_eq!(find_pattern_last(data, b"test"), Some(12));
        assert_eq!(find_pattern_last(data, b"notfound"), None);

        // find_pattern_last returns last occurrence, not first
        let data = b"abcXYZdefXYZghi";
        assert_eq!(find_pattern_last(data, b"XYZ"), Some(9));
    }

    #[test]
    fn test_type_gated_accessors_ignore_mismatched_fields() {
        // The option-bag tolerates contradictory states; the accessors
        // are the sanctioned read path and must gate on `check_type`.
        let contradictory = StreamChecksum {
            crc32: Some(0xDEAD_BEEF),
            crc64: Some(0x1122_3344_5566_7788),
            uncompressed_size: None,
            check_type: CheckType::Unknown,
        };
        assert_eq!(contradictory.crc32_value(), None);
        assert_eq!(contradictory.crc64_value(), None);

        let crc32_only = StreamChecksum {
            check_type: CheckType::Crc32,
            ..contradictory.clone()
        };
        assert_eq!(crc32_only.crc32_value(), Some(0xDEAD_BEEF));
        assert_eq!(crc32_only.crc64_value(), None);
    }
}
