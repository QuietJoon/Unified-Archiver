//! WinZip AES vendor-version probe and the CRC32-exemption gate.
//!
//! Split out of `src/ffi/zip_wrapper.rs` as one of the two seams AD 0057's
//! 2026-08-21 amendment names as unblocked — it depends on neither D2 nor
//! D9. The cluster is self-contained: only [`crc32_check_exempt`] and
//! [`drain_entry_crc32_counted`] are reachable from the parent, and the
//! header id, the AE-2 vendor constant and the extra-field walk are now
//! private to this file rather than visible across a 1700-line module.

use crate::error::Result;
use std::io::Read;
use std::path::Path;

/// WinZip AES extra-field header id (`0x9901`) and the vendor version
/// that marks an entry as AE-2 — the variant that stores no CRC32 at all.
/// AE-1 (`0x0001`) stores the real CRC32 and keeps every checksum
/// comparison in this file.
const AES_EXTRA_FIELD_ID: u16 = 0x9901;
const AES_VENDOR_VERSION_AE2: u16 = 0x0002;

/// Read the AES vendor version out of an entry's `0x9901` extra field.
///
/// The `zip` crate parses this field into a private `aes_mode` tuple and
/// exposes no accessor for it, but it leaves the raw field in
/// `ZipFile::extra_data()` (only the ZIP64 field is stripped), so the two
/// bytes are readable here. Layout per the WinZip AES specification: a
/// 7-byte body of `version:u16 | vendor_id:"AE" | strength:u8 |
/// method:u16`, little-endian.
///
/// Returns `None` when the entry carries no AES field — a plaintext entry
/// or a legacy ZipCrypto one — and `None` for a malformed or truncated
/// extra-field chain rather than guessing.
fn aes_vendor_version(zip_file: &zip::read::ZipFile) -> Option<u16> {
    let extra = zip_file.extra_data()?;
    let mut cursor = 0usize;
    // Extra fields are a chain of `id:u16 | len:u16 | body[len]`.
    while cursor + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[cursor], extra[cursor + 1]]);
        let len = u16::from_le_bytes([extra[cursor + 2], extra[cursor + 3]]) as usize;
        let body = cursor + 4;
        let end = body.checked_add(len)?;
        if end > extra.len() {
            // Truncated chain: stop rather than read past the field.
            return None;
        }
        if id == AES_EXTRA_FIELD_ID {
            if len < 2 {
                return None;
            }
            return Some(u16::from_le_bytes([extra[body], extra[body + 1]]));
        }
        cursor = end;
    }
    None
}

/// AE-2 AES entries store 0 in the central-directory CRC32 field by
/// specification, so comparing decrypted bytes against that placeholder
/// would flag every non-empty entry as corrupt. Mirror the zip crate's
/// own gate (it disables its internal `Crc32Reader` for AE-2 and relies
/// on the AES authentication tag for integrity instead) and exempt those
/// entries from every CRC comparison in this file (R0079-0007) — and,
/// since DCR-012, from the listing's `crc32` field too.
///
/// **The gate is `encrypted() && crc == 0 && AE-2`, and the third
/// conjunct is load-bearing.** `encrypted() && crc == 0` alone is a
/// *superset* of AE-2: it also sweeps in any encrypted entry whose
/// payload is genuinely empty — a legacy ZipCrypto empty file, or an AE-1
/// empty file from a writer that does not use the `zip` crate's
/// "under 20 bytes ⇒ AE-2" rule. Those carry a *real* stored CRC32,
/// `CRC32(b"") == 0`, which AD 0012 says is a valid checksum and not an
/// absent one. Exempting them would drop a checksum the archive actually
/// carried, and (post-DCR-012) would list `crc32: None` and force a
/// needless decrypt-and-stream during the digest walk. Reading the
/// `0x9901` vendor version distinguishes the placeholder from the real
/// zero, so the gate now means what its name says.
///
/// Residual, stated rather than hidden: an AE-2 entry whose central
/// record omits the `0x9901` field is not recognised here. Such an entry
/// is unreadable anyway — the `zip` crate rejects
/// "AES encryption without AES extra data field" when parsing the central
/// directory — so it cannot reach a CRC comparison in the first place.
pub(super) fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted()
        && zip_file.crc32() == 0
        && aes_vendor_version(zip_file) == Some(AES_VENDOR_VERSION_AE2)
}

/// Drain an entry's decoded payload, returning both its CRC32 and the
/// number of bytes produced.
///
/// R0001-0032: the shared `common::compute_crc32_reader` returns only a
/// checksum, so the integrity walk could not tell an entry that decoded
/// short from an intact one — a truncated payload whose stored CRC
/// matches the shortened bytes, or an AE-2 entry exempt from the CRC
/// compare, was reported clean. Counting here lets the caller require the
/// byte count to equal the authoritative central-directory size. Read
/// errors route through the same `map_entry_read_error` the shared helper
/// uses, so decoder-side CRC failures keep the R0079-0046 `Corruption`
/// mapping the caller classifies on.
pub(super) fn drain_entry_crc32_counted<R: Read + ?Sized>(
    reader: &mut R,
    error_path: &Path,
) -> Result<(u32, u64)> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = [0u8; 8192];
    let mut bytes_read: u64 = 0;

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| crate::ffi::common::map_entry_read_error(e, error_path))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        bytes_read += n as u64;
    }

    Ok((hasher.finalize(), bytes_read))
}
