//! Shared SFX size policy.
//!
//! Pulls every SFX-related size cap into one module so the relationships
//! between the detection scan window, the executable header probe, the
//! payload staging ceiling, and the stub extraction ceiling are visible
//! in a single place. Detection and extraction read from the same
//! constants, so a future tightening of the scan window cannot drift away
//! from the corresponding extraction guard without an obvious diff here.

/// Maximum size to scan for archive signatures (1 MiB).
///
/// Used by [`super::detection::detect_sfx`] for Stage 2 signature
/// scanning. A signature past this offset will not be detected, so an
/// SFX whose payload starts beyond 1 MiB will appear as
/// `is_sfx == false`. This bound trades off memory/time vs. coverage —
/// real-world SFX stubs almost always sit comfortably under this
/// window.
///
/// Post-R0069-0007 this is also the **effective stub ceiling**:
/// [`crate::Archive::extract_stub`] only honours an offset that
/// survives a fresh `detect_sfx` probe, and detection offsets always
/// fall inside this window — see [`MAX_STUB_SIZE`].
pub(crate) const MAX_SCAN_SIZE: usize = 1_048_576;

/// Extra bytes read past [`MAX_SCAN_SIZE`] into the detection buffer
/// (R0079-0043).
///
/// The Stage-3 structural probes inspect up to 32 header bytes after a
/// candidate signature offset. Over-reading this tail keeps the probes
/// working for a signature found just inside the 1 MiB boundary; the
/// signature scan itself never looks past [`MAX_SCAN_SIZE`]. Every
/// per-format probe minimum that indexes into the buffer must stay
/// ≤ this value (the catch-all 100-byte arm excepted — it inspects no
/// bytes).
pub(crate) const PROBE_TAIL: usize = 32;

/// Header window read for executable format detection (4 KiB).
///
/// PE/ELF/Mach-O signatures live in the first few hundred bytes; the
/// 4 KiB read covers them with margin and is a slice of the larger
/// [`MAX_SCAN_SIZE`] buffer. Detection only reads `min(HEADER_SIZE,
/// scan_buffer.len())` so a file shorter than 4 KiB is handled.
pub(crate) const HEADER_SIZE: usize = 4096;

/// Maximum payload tail copied to a tempfile by
/// [`crate::Archive::open_at_offset`] (16 GiB) — AD 0040.
///
/// `open_at_offset` stages the embedded archive into a tempfile before
/// dispatching to a backend. This ceiling caps the disk hit so a
/// hostile or malformed SFX cannot drag arbitrary bytes through staging.
/// The ceiling is **independent of [`MAX_SCAN_SIZE`]** — detection looks
/// at the *prefix* (1 MiB), staging copies the *suffix* (≤ 16 GiB).
///
/// **Authoritative home moved (AD 0040 amendment 2026-07-22, R0081 I1):**
/// the ceiling is now a first-class field
/// [`crate::ExtractionLimits::max_sfx_payload_size`], defaulting to
/// [`crate::security::DEFAULT_MAX_SFX_PAYLOAD_SIZE`]. This constant is a
/// thin alias re-exporting that default so the SFX size-relationship
/// documentation stays gathered in one module; `open_at_offset`, which
/// has no `ExtractionLimits` in scope, reads this alias.
pub(crate) const MAX_SFX_PAYLOAD_SIZE: u64 = crate::security::DEFAULT_MAX_SFX_PAYLOAD_SIZE;

/// Sanity bound on the stub size accepted by
/// [`crate::Archive::extract_stub`] (50 MiB).
///
/// **Currently a dead gate** (R0079-0042): since R0069-0007,
/// `extract_stub` re-runs detection and only honours an offset that
/// matches the fresh probe, and detection offsets always fall inside
/// the [`MAX_SCAN_SIZE`] scan window — so the effective stub ceiling
/// is 1 MiB and this 50 MiB check cannot bind. It is kept as defence
/// in depth for a future `extract_stub` that verifies caller offsets
/// by other means (e.g. probing the format header at the claimed
/// offset directly), at which point this constant would become the
/// binding ceiling again.
pub(crate) const MAX_STUB_SIZE: u64 = 50 * 1024 * 1024;
