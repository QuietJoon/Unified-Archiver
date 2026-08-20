//! Streaming extraction for memory-efficient archive processing
//!
//! Phase 2.4: Provides streaming extraction to meet SC-009 (<100MB memory for 10GB+ archives)

use std::io::{self, Read};

/// Explicit output-size bound for a streaming extraction
/// (I2 / AD 0062 A.2 / DCR-006).
///
/// Selects how [`Archive::extract_to_stream`](crate::Archive::extract_to_stream)
/// and
/// [`Archive::extract_to_stream_with_options`](crate::Archive::extract_to_stream_with_options)
/// cap the returned [`StreamingExtractor`]. "Unbounded" is chosen here
/// — as one value among three, in exactly one place — instead of via a
/// separate method, so the bounded and unbounded stream surfaces cannot
/// drift apart (this structurally retires R0081-0027, where the two
/// former `_unbounded` methods disagreed on the mmap cap of the day —
/// no backend memory-maps any more).
///
/// The safety pre-check (`ExtractionLimits` size/ratio gate) runs the
/// same way for every variant.
///
/// R0001-0011 / DEF-004: the bound shapes *both* the materialization
/// budget handed to the backend and the cap enforced on the returned
/// reader. The backend budget is always `min`-ed with the caller's
/// [`ExtractionLimits`](crate::ExtractionLimits) — the minimum of
/// `max_file_size` and `max_total_size`, which a `StreamBound` may
/// tighten but never loosen.
///
/// # What is actually bounded, per backend
///
/// Only libarchive hands out a genuinely incremental reader today
/// (AD 0035 / DEF-004); ZIP, 7z and RAR *stage* the entry — to memory or
/// to a temporary file — before the reader exists. The bound therefore
/// means different things at different backends, and this is the honest
/// statement of which:
///
/// | Backend | What the budget bounds | `Cap(n)` below the declared size |
/// |---|---|---|
/// | libarchive (TAR family, ISO, raw gzip/bzip2/xz) | the reader itself — the wrapper *is* the materialization bound | serves a prefix of `n` bytes; reading past `n` is [`io::ErrorKind::InvalidData`] |
/// | ZIP | the staged `Vec<u8>`, refused before it is reserved | refused at the extract call, [`ArchiveError::OperationBlocked`](crate::ArchiveError::OperationBlocked) |
/// | 7z | the staged `Vec<u8>`, refused before it is reserved | refused at the extract call, `OperationBlocked` |
/// | RAR | the staged temporary file, refused before `RARProcessFile` runs | refused at the extract call, `OperationBlocked` |
///
/// A 7z solid block still decodes the members preceding the target to
/// reach it; the budget bounds our buffer, not that decode work.
///
/// # Violation classification (DCR-006 Amendment 4)
///
/// **Read this bound-first: the guarantee is not the same under all three
/// variants**, and an earlier draft of this table said it was.
///
/// ## Under [`DeclaredSize`](Self::DeclaredSize)
///
/// Every backend holds the entry to its declaration, so a violation
/// surfaces **no later than the read that observes it, and never as silent
/// success**. A backend that observes the violation while materializing
/// reports it earlier, as a typed [`ArchiveError`](crate::ArchiveError)
/// from the extract call itself — there, call-time reporting is an
/// *earlier delivery of the same verdict*. The two deliveries pair:
///
/// | Violation | Call-time (ZIP/7z/RAR) | Read-time (libarchive) |
/// |---|---|---|
/// | budget exceeded | [`ArchiveError::OperationBlocked`](crate::ArchiveError::OperationBlocked) | [`io::ErrorKind::InvalidData`] at the cap |
/// | truncation under the declaration | [`ArchiveError::Corruption`](crate::ArchiveError::Corruption) | [`io::ErrorKind::UnexpectedEof`] |
/// | over-production past the declaration | [`ArchiveError::Corruption`](crate::ArchiveError::Corruption) | [`io::ErrorKind::InvalidData`] |
///
/// So under `DeclaredSize` a caller writes one handler for "this entry was
/// damaged" — match the extract call's `Result` **and** `e.kind()` inside
/// the read loop — and never has to know which backend it is on, only
/// which of the two arrival points fired. Which one fires is not promised
/// to stay fixed: when DEF-004 lands genuine incremental readers for the
/// staging backends, those violations move from call time to read time
/// without the contract changing.
///
/// ## Under [`Cap`](Self::Cap) and [`Unbounded`](Self::Unbounded)
///
/// Only the *budget* row above holds on every backend. Declaration
/// integrity is **not** part of what these bounds promise, and what you
/// actually get differs by backend as a side effect of how each one
/// materializes:
///
/// - **ZIP, 7z and RAR** still enforce the declaration, because staging is
///   how they work at all — ZIP and 7z stage with `require_exact`, and RAR
///   length-checks the staged payload. A truncated entry is a call-time
///   [`ArchiveError::Corruption`](crate::ArchiveError::Corruption) even
///   under `Unbounded`.
/// - **libarchive** does not. Its reader is incremental, nothing compares
///   the byte count to a declaration these bounds did not ask about, and a
///   truncated entry reads to a **clean short EOF**.
///
/// That is a genuinely different verdict, not an earlier one, and it is
/// the reason `DeclaredSize` — not `Cap` — is the safe default for
/// untrusted input. If you need declaration integrity on every backend,
/// choose `DeclaredSize`; `Cap(n)` is a resource budget and answers only
/// the budget question. To read a window of a larger entry *and* keep the
/// integrity check, use `DeclaredSize` with [`Read::take`].
///
/// Both read errors are **sticky**: a retrying caller gets the same error
/// back, never a clean `Ok(0)` (R3 / ti-4ba1ff1a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamBound {
    /// Hold the stream to **exactly** the entry's declared uncompressed
    /// size, taken from the archive's authoritative preflight listing
    /// (R0001-0008) rather than from the decoder's own output.
    ///
    /// - Over-production past the declared size surfaces as an
    ///   [`io::ErrorKind::InvalidData`] read error (DCR-006).
    /// - Under-production — a clean end-of-stream before the declared
    ///   size — surfaces as an [`io::ErrorKind::UnexpectedEof`] read
    ///   error, so a truncated entry in a format without a per-entry
    ///   checksum (plain TAR, CPIO, ISO) cannot read as a short but
    ///   valid payload (R0001-0011 / OI-0001-001).
    ///
    /// Both verdicts are **sticky** (R3 / ti-4ba1ff1a): a caller that
    /// retries the read gets the same error, never a clean `Ok(0)`, and the
    /// retry does not consume another byte from the underlying stream.
    ///
    /// The under-production check binds on the read that observes the
    /// end of the stream; a caller that stops reading early sees no
    /// error, which is inherent to a pull-based reader. Backends that
    /// materialize the entry before returning the reader (ZIP/7z/RAR —
    /// AD 0035 / DEF-004) surface either violation earlier, as a typed
    /// [`ArchiveError`](crate::ArchiveError) from the extract call itself
    /// instead of from a later read — see the classification table on
    /// [`StreamBound`] for the lossless pairing between the two.
    ///
    /// When the entry declares no size (raw gzip/bzip2/xz single-file
    /// readers, and libarchive entries whose size field is unset) there
    /// is no declaration to hold the stream to: the reader degrades to a
    /// ceiling-only cap at the effective `ExtractionLimits` entry
    /// ceiling (the minimum of `max_file_size` and `max_total_size`),
    /// and an early end-of-stream is an ordinary EOF.
    ///
    /// This is the safe default for untrusted input.
    DeclaredSize,
    /// Ceiling-only cap at `n` bytes: a resource budget, **not** an
    /// assertion about the entry's size. An entry smaller than `n` ends
    /// with a normal EOF; production past `n` surfaces as an
    /// [`io::ErrorKind::InvalidData`] read error, exactly like
    /// [`StreamBound::DeclaredSize`], but at a caller-chosen ceiling.
    ///
    /// R0001-0011: the backend's materialization step also receives
    /// `min(n, limits)` as its budget, so a backend that must buffer or
    /// stage the entry before returning the reader refuses an oversized
    /// entry at call time rather than materializing the whole thing first.
    ///
    /// DCR-006 Amendment 4: the trigger is uniform across all three staging
    /// backends — ZIP, 7z **and** RAR refuse on the *declared* size, judged
    /// before any decode, so an entry whose header declares more than the
    /// budget fails without the decoder ever running. A header that
    /// under-declares and then over-produces is caught by the same
    /// decoded-byte backstop each backend already had (ZIP/7z read
    /// `declared + 1`; UnRAR aborts inside `UCM_PROCESSDATA`). Entries whose
    /// header declares no size skip the pre-check entirely — no declaration
    /// is invented — and rely on that backstop alone. Only the incremental
    /// libarchive path serves a *prefix* of a larger entry up to `n`.
    ///
    /// `Cap(n)` is a **total-resource assertion** — "no more than `n` bytes
    /// may exist on my behalf, anywhere" — not a read window. For a window
    /// of a larger entry use [`StreamBound::DeclaredSize`] plus
    /// [`Read::take`]`(n)`, which is portable across every backend; and size
    /// [`ExtractionLimits`](crate::ExtractionLimits) to what you are willing
    /// to have materialized on a staging backend, because on those backends
    /// the window genuinely costs a full materialization (AD 0035).
    Cap(u64),
    /// No caller-chosen output cap. The returned reader forwards decoded
    /// bytes verbatim, so a hostile archive whose decoded size exceeds its
    /// declared size can drive a caller-side `read_to_end` to allocate up
    /// to the extraction limits' own ceiling. R0001-0007 / R0001-0011: the
    /// public stream APIs hand the effective `ExtractionLimits` entry
    /// ceiling — the minimum of `max_file_size` and `max_total_size` — to
    /// the backend materialization step, so even here a decoder cannot
    /// produce past that ceiling — but nothing tighter is enforced. Where
    /// that ceiling surfaces differs by backend, exactly as under
    /// [`StreamBound::DeclaredSize`]: on the incremental libarchive path it
    /// is an [`io::ErrorKind::InvalidData`] read error, while a backend that
    /// materializes the entry before returning the reader (ZIP/7z/RAR)
    /// reports it as a typed [`crate::ArchiveError`] from the extract call
    /// itself.
    /// Callers who genuinely want no ceiling must raise the limits, not
    /// choose `Unbounded`. Opt in only for trusted input, or wrap the
    /// result in your own [`Read::take`] /
    /// [`StreamingExtractor::take_bounded`] sized to a budget.
    Unbounded,
}

/// Streaming extractor that implements Read trait
///
/// Exposes an archive entry as a `Read` without the caller having to
/// `Vec<u8>`-buffer it first. Note the bounded-memory guarantee: the
/// libarchive-backed formats (TAR, ISO, …) stream the decompressor
/// directly, whereas ZIP/7z/RAR materialize the entry to a temporary
/// buffer or file before returning the reader. For truly multi-GiB
/// inputs prefer libarchive-backed formats or apply your own
/// `Read::take`/rate limiting.
///
/// Propagate read errors instead of treating them as EOF — corruption
/// and I/O failures surface through `Err`, and `extract_to_stream` /
/// `extract_to_stream_with_options` report cap violations as `Err`
/// rather than a silent EOF at the cap (R0080-0007 / DCR-006) under
/// **every** [`StreamBound`]. [`StreamBound::Unbounded`] only declines
/// to add a *caller-chosen* cap: the effective `ExtractionLimits` entry
/// ceiling (the minimum of `max_file_size` and `max_total_size`) is
/// still handed to the backend materialization step, so production past
/// it is an [`io::ErrorKind::InvalidData`] read error on the incremental
/// libarchive path and a typed [`ArchiveError`](crate::ArchiveError)
/// from the extract call on the staging backends (R0001-0007 /
/// R0001-0011). Under
/// [`StreamBound::DeclaredSize`] with a known listing size the stream is
/// additionally *exact-length*: an end of stream before the declared
/// byte count is an [`io::ErrorKind::UnexpectedEof`] error, not a short
/// read (R0001-0011 / OI-0001-001).
///
/// # Example
/// ```no_run
/// use unified_archive::{Archive, StreamBound};
/// use std::io::Read;
///
/// let archive = Archive::open("large.rar")?;
/// let mut extractor = archive.extract_to_stream("large_file.bin", StreamBound::DeclaredSize)?;
///
/// let mut buffer = [0u8; 8192];
/// let mut total = 0;
/// loop {
///     let n = extractor.read(&mut buffer)?;
///     if n == 0 { break; }
///     // Process chunk without loading entire file
///     total += n;
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct StreamingExtractor {
    /// Internal reader - either from temporary file or direct stream
    reader: Box<dyn Read + Send>,
    /// Total bytes available (if known)
    total_size: Option<u64>,
    /// Bytes read so far
    bytes_read: u64,
}

impl StreamingExtractor {
    /// Create a new streaming extractor from a reader
    pub(crate) fn new(reader: Box<dyn Read + Send>, total_size: Option<u64>) -> Self {
        Self {
            reader,
            total_size,
            bytes_read: 0,
        }
    }

    /// Wrap an already-buffered entry payload in a size-reporting
    /// extractor. Used by the backends whose upstream API cannot hand
    /// out an owned entry reader (ZIP/7z/RAR — see AD 0035 / DEF-004),
    /// which materialize the entry and stream from the buffer.
    ///
    /// R0001-0008: the reported `total_size` is the *materialized* length,
    /// not the archive's declaration — an over-producing decoder would
    /// otherwise define its own declared size. Callers enforcing
    /// [`StreamBound::DeclaredSize`] must take the cap from the entry
    /// listing (`Archive::extract_to_stream_impl` does), never from
    /// [`Self::total_size`]; R0001-0011's [`Self::with_exact_size`] then
    /// restamps `total_size` from that listing value.
    pub(crate) fn from_bytes(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self::new(Box::new(std::io::Cursor::new(data)), Some(size))
    }

    /// Get total size if known
    pub fn total_size(&self) -> Option<u64> {
        self.total_size
    }

    /// Get bytes read so far
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Get progress as percentage (0.0 to 1.0) if total size is known.
    ///
    /// Clamped to `1.0` so a backend emitting more bytes than the
    /// declared size cannot push progress above 100% (R0069-0081).
    /// Use [`take_bounded`](Self::take_bounded) to clamp manual reads at the
    /// declared or fallback limit. The returned `Take` reader reaches EOF at
    /// the cap; callers that need an explicit over-emission error must compare
    /// the observed byte count with the expected size.
    pub fn progress(&self) -> Option<f64> {
        self.total_size.map(|total| {
            if total == 0 {
                1.0
            } else {
                let raw = self.bytes_read as f64 / total as f64;
                if raw > 1.0 { 1.0 } else { raw }
            }
        })
    }

    /// Wrap this extractor in a [`std::io::Take`] sized to the declared
    /// `total_size`, falling back to `fallback` when the size is unknown.
    ///
    /// The bare [`Read`] impl forwards bytes verbatim — a malicious or
    /// corrupt archive can return more data than the entry header
    /// promised. This method is the *silent* defence: the returned `Take`
    /// reaches a clean EOF at the cap and reports nothing. For untrusted
    /// input prefer [`StreamBound::DeclaredSize`], which is the safe
    /// default and turns both over- and under-production into read errors
    /// (DCR-006 / R0001-0011); reach for `take_bounded` when a silent clamp
    /// at a caller-chosen fallback is what you actually want.
    ///
    /// ```no_run
    /// use unified_archive::{Archive, StreamBound};
    /// use std::io::Read;
    ///
    /// let archive = Archive::open("untrusted.tar.gz")?;
    /// // AD 0062 A.2: `extract_to_stream` with `StreamBound::DeclaredSize`
    /// // is bounded (returns a hard-capped `StreamingExtractor` that
    /// // errors on over-production); `take_bounded` is instead the silent
    /// // `io::Take` clamp with a custom fallback size, layered on the
    /// // `StreamBound::Unbounded` reader. `Unbounded` appears here only to
    /// // isolate what `take_bounded` adds — for untrusted input the default
    /// // is `StreamBound::DeclaredSize`.
    /// let stream = archive.extract_to_stream("payload.bin", StreamBound::Unbounded)?;
    /// // Cap reads to the entry's declared size, or 16 MiB if unknown.
    /// let mut bounded = stream.take_bounded(16 * 1024 * 1024);
    /// let mut buf = Vec::new();
    /// bounded.read_to_end(&mut buf)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn take_bounded(self, fallback: u64) -> std::io::Take<Self> {
        let limit = self.total_size.unwrap_or(fallback);
        std::io::Read::take(self, limit)
    }

    /// Wrap the inner reader with a hard cap that surfaces over-emission
    /// as a read error rather than silently truncating.
    ///
    /// R0075-0018: the trait-default `extract_to_stream_with_limit` calls
    /// this when the caller supplies `max_bytes`; R0080-0007 also routes
    /// the public bounded `Archive::extract_to_stream` /
    /// `Archive::extract_to_stream_with_options` through it, so both
    /// enforce the cap on every `Read::read` instead of clamping to a
    /// silent EOF. The constructed [`StreamingExtractor`] still reports
    /// the original `total_size` for progress reporting.
    ///
    /// This is the **ceiling-only** wrapper: an entry shorter than
    /// `max_bytes` ends with a normal EOF. Use
    /// [`Self::with_exact_size`] where the byte count is a declaration
    /// the stream must satisfy exactly (R0001-0011).
    pub(crate) fn with_hard_cap(self, max_bytes: u64) -> Self {
        let total_size = self.total_size;
        let bytes_read = self.bytes_read;
        let bounded = HardCapReader::ceiling(self.reader, max_bytes);
        Self {
            reader: Box::new(bounded),
            total_size,
            bytes_read,
        }
    }

    /// Wrap the inner reader with an **exact-length** contract at
    /// `declared` bytes: over-production is an
    /// [`io::ErrorKind::InvalidData`] read error exactly as under
    /// [`Self::with_hard_cap`], and under-production — a clean inner EOF
    /// before `declared` bytes — is an
    /// [`io::ErrorKind::UnexpectedEof`] read error instead of an
    /// ordinary end of stream (R0001-0011 / OI-0001-001).
    ///
    /// The reported `total_size` is normalized to `declared`, so the
    /// cap, the exactness check and the [`Self::progress`] denominator
    /// all come from the one authority the caller resolved — the
    /// preflight listing (R0001-0008) — rather than from the
    /// materialized buffer length a staging backend happens to hand
    /// back.
    ///
    /// Only [`StreamBound::DeclaredSize`] with a known listing size
    /// reaches this; the ceiling-only bounds keep
    /// [`Self::with_hard_cap`].
    pub(crate) fn with_exact_size(self, declared: u64) -> Self {
        let bytes_read = self.bytes_read;
        let bounded = HardCapReader::exact(self.reader, declared);
        Self {
            reader: Box::new(bounded),
            total_size: Some(declared),
            bytes_read,
        }
    }
}

/// Reader adapter that returns an [`io::ErrorKind::InvalidData`] error
/// when the inner reader has emitted more than `cap` bytes total. Lets
/// callers rely on the read API to surface contract violations instead
/// of having to compare an after-the-fact byte count themselves.
///
/// R0001-0011: with `expected = Some(n)` the adapter additionally holds
/// the stream to *at least* `n` bytes — an inner clean EOF below that
/// count is an [`io::ErrorKind::UnexpectedEof`] error rather than an
/// ordinary end of stream. `expected = None` keeps the ceiling-only
/// contract every other caller relies on.
///
/// `pub(crate)` since OI-0001-009: the single-traversal digest walk
/// (`Archive::calculate_content_multiset_digest_and_size`) receives a
/// borrowed `&mut dyn Read` from the backend rather than an owned
/// [`StreamingExtractor`], so it applies the same two bounds through
/// [`Self::exact`] / [`Self::ceiling`] directly. Those constructors and
/// `StreamingExtractor::{with_exact_size, with_hard_cap}` are the same
/// adapter with the same verdicts — deliberately, so the DCR-011
/// truncation contract cannot diverge between the one-pass and the
/// per-entry resolution paths.
pub(crate) struct HardCapReader<R: Read> {
    inner: R,
    cap: u64,
    /// `Some(n)`: the stream must deliver exactly `n` bytes (in practice
    /// always equal to `cap`). `None`: ceiling-only.
    expected: Option<u64>,
    seen: u64,
    /// R3 (ti-4ba1ff1a): latches the bound verdict. `io::Read` callers and
    /// adapters are free to retry after an error; a deterministic contract
    /// violation must not disappear on the second attempt and let the
    /// caller fall through to a clean `Ok(0)`. Both violation classes latch
    /// — truncation *and* over-production — so the two are handled
    /// symmetrically (before, an over-producing stream consumed one probe
    /// byte per retry and decayed into an ordinary EOF once the inner
    /// stream drained).
    violation: Option<CapViolation>,
}

/// R3 (ti-4ba1ff1a): the two deterministic bound verdicts a
/// [`HardCapReader`] can reach. No payload fields — `seen`, `cap` and
/// `expected` already live on the struct, so the error text is
/// regenerated per call from the reader's own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapViolation {
    /// Inner clean EOF below `expected` (exact bound only).
    Truncated,
    /// The inner stream produced at least one byte past `cap`.
    Overrun,
}

impl<R: Read> HardCapReader<R> {
    fn new(inner: R, cap: u64, expected: Option<u64>) -> Self {
        Self {
            inner,
            cap,
            expected,
            seen: 0,
            violation: None,
        }
    }

    /// Ceiling-only bound: the stream may end early with an ordinary EOF,
    /// but a byte past `cap` is an [`io::ErrorKind::InvalidData`] error.
    /// The bound for an entry whose listing declares no size — there is no
    /// declaration to violate, only the crate's policy limit
    /// (DCR-011 hard constraint 3).
    pub(crate) fn ceiling(inner: R, cap: u64) -> Self {
        Self::new(inner, cap, None)
    }

    /// Exact-length bound at `declared` bytes: under-production is an
    /// [`io::ErrorKind::UnexpectedEof`] error and over-production an
    /// [`io::ErrorKind::InvalidData`] one (R6 / DCR-011). The bound for an
    /// entry whose listing declares a size, so a truncated CRC-less member
    /// cannot digest its short payload and report success.
    pub(crate) fn exact(inner: R, declared: u64) -> Self {
        Self::new(inner, declared, Some(declared))
    }

    fn truncation_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "entry truncated: stream ended after {} of the declared {} bytes",
                self.seen,
                self.expected.unwrap_or(self.cap)
            ),
        )
    }

    /// R3: sole producer of the over-production diagnostic, so the latched
    /// replay and the first observation cannot drift apart.
    fn overrun_error(&self) -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("stream emitted more than the {} byte cap", self.cap),
        )
    }

    /// R3: regenerate the latched verdict's error.
    fn violation_error(&self, violation: CapViolation) -> io::Error {
        match violation {
            CapViolation::Truncated => self.truncation_error(),
            CapViolation::Overrun => self.overrun_error(),
        }
    }
}

impl<R: Read> Read for HardCapReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // R3 (ti-4ba1ff1a): the sticky verdict — truncation *or*
        // over-production — is checked FIRST, ahead of the empty-buffer
        // shortcut below: once the stream is known to have violated the
        // bound, an empty-buffer read must not be a way to step past the
        // verdict. `Read::read` permits either ordering for a zero-length
        // buffer, and failing closed is the right choice for a violation
        // that has already been established. Checking the latch before the
        // probe arm is also what stops a retrying caller from consuming one
        // inner byte per retry after an overrun.
        if let Some(violation) = self.violation {
            return Err(self.violation_error(violation));
        }
        // R0001-0011: a zero-length caller buffer cannot observe EOF, so it
        // must never be mistaken for the inner stream ending early under an
        // exact bound. Handled before the probe arm too: probing on behalf of
        // a caller who asked for nothing would consume a byte the caller never
        // receives.
        if buf.is_empty() {
            return Ok(0);
        }
        if self.seen >= self.cap {
            // Probe one more byte to distinguish a clean EOF (cap is
            // also the entry size) from an over-producing decoder.
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe) {
                // A clean EOF at the cap is not a violation, so nothing
                // latches: the entry simply ended exactly at the ceiling.
                Ok(0) => Ok(0),
                Ok(_) => {
                    // R3 (ti-4ba1ff1a): latch before returning, so a retry
                    // replays the verdict instead of probing again.
                    self.violation = Some(CapViolation::Overrun);
                    Err(self.overrun_error())
                }
                // A transient inner I/O failure is not a deterministic
                // verdict about the bound — it may legitimately succeed on
                // retry — so it passes through unlatched (R3).
                Err(e) => Err(e),
            };
        }
        let remaining = self.cap - self.seen;
        // R0001-0038: clamp in `u64` before narrowing. Casting `remaining`
        // to `usize` first truncates a cap above `usize::MAX` on a 32-bit
        // target, which can hand the inner reader a zero-length buffer and
        // surface as a premature EOF well before the cap.
        let want = remaining.min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..want])?;
        if n == 0 {
            // `want >= 1` here (non-empty `buf`, `seen < cap`), so an
            // inner `Ok(0)` is a genuine end of stream — not the
            // "nothing was asked for" case.
            if let Some(expected) = self.expected {
                if self.seen < expected {
                    // R3 (ti-4ba1ff1a): latch, same as the overrun arm.
                    self.violation = Some(CapViolation::Truncated);
                    return Err(self.truncation_error());
                }
            }
            return Ok(0);
        }
        self.seen = self.seen.saturating_add(n as u64);
        Ok(n)
    }
}

impl Read for StreamingExtractor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        // Saturating add so a pathological reader emitting > u64::MAX
        // bytes lifetime-cumulative cannot wrap the counter (R0069-0082).
        self.bytes_read = self.bytes_read.saturating_add(n as u64);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_streaming_extractor_basic() {
        let data = b"Hello, streaming world!";
        let cursor = Cursor::new(data.to_vec());
        let mut extractor = StreamingExtractor::new(Box::new(cursor), Some(data.len() as u64));

        // Check initial state
        assert_eq!(extractor.total_size(), Some(data.len() as u64));
        assert_eq!(extractor.bytes_read(), 0);
        assert_eq!(extractor.progress(), Some(0.0));

        // Read some data
        let mut buffer = [0u8; 10];
        let n = extractor.read(&mut buffer).unwrap();
        assert_eq!(n, 10);
        assert_eq!(&buffer[..n], b"Hello, str");
        assert_eq!(extractor.bytes_read(), 10);

        // Check progress
        let progress = extractor.progress().unwrap();
        assert!((progress - 0.434).abs() < 0.01); // ~43.4% progress

        // Read rest
        let mut rest = Vec::new();
        extractor.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"eaming world!");
        assert_eq!(extractor.bytes_read(), data.len() as u64);
        assert_eq!(extractor.progress(), Some(1.0));
    }

    #[test]
    fn test_hard_cap_violation_surfaces_as_err_not_eof() {
        // Doc contract (R0079-0041): hard-capped readers report cap
        // violations through `Err`, so a `read()?` loop cannot
        // mistake an over-emitting stream for a clean EOF.
        let data = vec![0u8; 32];
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(data)), Some(32));
        let mut capped = extractor.with_hard_cap(16);

        let mut buffer = [0u8; 8];
        let mut total = 0usize;
        let err = loop {
            match capped.read(&mut buffer) {
                Ok(0) => panic!("cap violation must surface as Err, not EOF"),
                Ok(n) => total += n,
                Err(e) => break e,
            }
        };
        assert_eq!(total, 16);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_hard_cap_large_remaining_does_not_truncate_read() {
        // R0001-0038: a cap far above `u32::MAX` must still let a full
        // buffer through. The pre-fix `remaining as usize` narrowing could
        // wrap such a cap to a short (or zero) read on a 32-bit target.
        let data = vec![7u8; 64];
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(data)), None);
        let mut capped = extractor.with_hard_cap(8 * 1024 * 1024 * 1024);

        let mut buffer = [0u8; 64];
        let n = capped.read(&mut buffer).unwrap();
        assert_eq!(n, 64);
        assert!(buffer.iter().all(|b| *b == 7));
    }

    /// R0001-0011 / OI-0001-001: under an exact bound a clean inner EOF
    /// short of the declaration is a truncation error, never a short
    /// read that a `read()?` loop would mistake for a complete entry.
    #[test]
    fn test_exact_size_under_production_surfaces_unexpected_eof() {
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(vec![9u8; 16])), Some(16));
        let mut exact = extractor.with_exact_size(32);

        let mut buffer = [0u8; 8];
        let mut total = 0usize;
        let err = loop {
            match exact.read(&mut buffer) {
                Ok(0) => panic!("truncation must surface as Err, not EOF"),
                Ok(n) => total += n,
                Err(e) => break e,
            }
        };
        assert_eq!(total, 16, "the delivered prefix is handed to the caller");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

        // Sticky: a retrying caller cannot fall through to a clean EOF.
        let again = exact.read(&mut buffer).expect_err("error must repeat");
        assert_eq!(again.kind(), io::ErrorKind::UnexpectedEof);
    }

    /// The exact bound must not turn a well-formed entry into an error:
    /// exactly `declared` bytes then a clean EOF.
    #[test]
    fn test_exact_size_exact_delivery_reaches_clean_eof() {
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(vec![3u8; 32])), Some(32));
        let mut exact = extractor.with_exact_size(32);

        let mut buf = Vec::new();
        exact.read_to_end(&mut buf).expect("read_to_end");
        assert_eq!(buf.len(), 32);
        assert_eq!(exact.read(&mut [0u8; 8]).expect("post-EOF read"), 0);
        assert_eq!(exact.total_size(), Some(32));
        assert_eq!(exact.progress(), Some(1.0));
    }

    /// DCR-006's over-production contract must survive the new path:
    /// the probe arm still reports `InvalidData` at the declared size.
    #[test]
    fn test_exact_size_over_production_still_invalid_data() {
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(vec![1u8; 48])), Some(48));
        let mut exact = extractor.with_exact_size(32);

        let mut buffer = [0u8; 8];
        let mut total = 0usize;
        let err = loop {
            match exact.read(&mut buffer) {
                Ok(0) => panic!("over-production must surface as Err, not EOF"),
                Ok(n) => total += n,
                Err(e) => break e,
            }
        };
        assert_eq!(total, 32);
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// R0081-0062 keeps a known-empty entry as `Some(0)`; the exact
    /// bound must accept its immediate EOF and still reject a byte.
    #[test]
    fn test_exact_size_zero_declared_entry() {
        let empty = StreamingExtractor::new(Box::new(Cursor::new(Vec::new())), Some(0));
        let mut exact = empty.with_exact_size(0);
        assert_eq!(exact.read(&mut [0u8; 4]).expect("clean EOF"), 0);

        let over = StreamingExtractor::new(Box::new(Cursor::new(vec![7u8])), Some(1));
        let mut exact = over.with_exact_size(0);
        let err = exact.read(&mut [0u8; 4]).expect_err("one byte is too many");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// A zero-length caller buffer must return `Ok(0)` without being
    /// read as an end of stream — otherwise `read(&mut [])` mid-entry
    /// would report a bogus truncation.
    #[test]
    fn test_exact_size_empty_caller_buffer_is_not_eof() {
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(vec![5u8; 16])), Some(16));
        let mut exact = extractor.with_exact_size(16);

        assert_eq!(exact.read(&mut []).expect("empty buffer read"), 0);
        let mut buf = Vec::new();
        exact.read_to_end(&mut buf).expect("stream still readable");
        assert_eq!(buf.len(), 16);
        assert_eq!(exact.read(&mut []).expect("empty buffer at EOF"), 0);
    }

    /// Constraint check for `with_hard_cap`: `expected = None` keeps the
    /// ceiling-only contract, so a short entry still EOFs cleanly.
    #[test]
    fn test_hard_cap_short_entry_still_reaches_clean_eof() {
        let extractor = StreamingExtractor::new(Box::new(Cursor::new(vec![2u8; 8])), Some(8));
        let mut capped = extractor.with_hard_cap(4096);

        let mut buf = Vec::new();
        capped.read_to_end(&mut buf).expect("read_to_end");
        assert_eq!(buf.len(), 8);
        assert_eq!(capped.read(&mut [0u8; 4]).expect("clean EOF"), 0);
    }

    /// Counting adapter used by the R3 latch tests: records how many times
    /// the inner reader was actually asked for bytes, so "the latch replays
    /// without touching the source" is provable rather than inferred.
    struct CountingReader<R: Read> {
        inner: R,
        reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl<R: Read> Read for CountingReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.inner.read(buf)
        }
    }

    /// Snapshot of the counter above.
    fn inner_reads(reads: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> usize {
        reads.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// R3 (ti-4ba1ff1a): over-production latches exactly like truncation.
    /// Before this, a caller retrying after `InvalidData` consumed one probe
    /// byte per retry and, once the inner stream drained, got a clean
    /// `Ok(0)` — an integrity error decaying into an ordinary EOF.
    #[test]
    fn test_hard_cap_over_production_verdict_is_sticky() {
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = CountingReader {
            inner: Cursor::new(vec![4u8; 24]),
            reads: std::sync::Arc::clone(&reads),
        };
        let extractor = StreamingExtractor::new(Box::new(counting), Some(24));
        let mut capped = extractor.with_hard_cap(8);

        let mut buffer = [0u8; 4];
        let err = loop {
            match capped.read(&mut buffer) {
                Ok(0) => panic!("over-production must surface as Err, not EOF"),
                Ok(_) => continue,
                Err(e) => break e,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let after_latch = inner_reads(&reads);
        for attempt in 0..2 {
            let again = capped
                .read(&mut buffer)
                .expect_err("the overrun verdict must repeat, never decay to Ok(0)");
            assert_eq!(
                again.kind(),
                io::ErrorKind::InvalidData,
                "retry {attempt} kept the class"
            );
        }
        assert_eq!(
            inner_reads(&reads),
            after_latch,
            "a latched verdict must be replayed without consuming another inner byte"
        );
    }

    /// R3: the same stickiness under the exact bound — `with_exact_size`
    /// must not have its own over-production path.
    #[test]
    fn test_exact_size_over_production_verdict_is_sticky() {
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = CountingReader {
            inner: Cursor::new(vec![6u8; 20]),
            reads: std::sync::Arc::clone(&reads),
        };
        let extractor = StreamingExtractor::new(Box::new(counting), Some(20));
        let mut exact = extractor.with_exact_size(8);

        let mut buffer = [0u8; 8];
        let err = loop {
            match exact.read(&mut buffer) {
                Ok(0) => panic!("over-production must surface as Err, not EOF"),
                Ok(_) => continue,
                Err(e) => break e,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let after_latch = inner_reads(&reads);
        for _ in 0..2 {
            let again = exact.read(&mut buffer).expect_err("verdict must repeat");
            assert_eq!(again.kind(), io::ErrorKind::InvalidData);
        }
        // A zero-length buffer must not step past the verdict either.
        let empty = exact
            .read(&mut [])
            .expect_err("an empty buffer cannot clear a latched verdict");
        assert_eq!(empty.kind(), io::ErrorKind::InvalidData);
        assert_eq!(inner_reads(&reads), after_latch, "no further inner reads");
    }

    /// R3: the truncation latch keeps the same no-inner-read replay
    /// property the overrun latch just gained.
    #[test]
    fn test_exact_size_truncation_latch_does_not_reread_inner() {
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = CountingReader {
            inner: Cursor::new(vec![1u8; 4]),
            reads: std::sync::Arc::clone(&reads),
        };
        let extractor = StreamingExtractor::new(Box::new(counting), Some(4));
        let mut exact = extractor.with_exact_size(16);

        let mut buffer = [0u8; 8];
        let err = loop {
            match exact.read(&mut buffer) {
                Ok(0) => panic!("truncation must surface as Err, not EOF"),
                Ok(_) => continue,
                Err(e) => break e,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

        let after_latch = inner_reads(&reads);
        let again = exact.read(&mut buffer).expect_err("verdict must repeat");
        assert_eq!(again.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(inner_reads(&reads), after_latch);
    }

    #[test]
    fn test_streaming_extractor_unknown_size() {
        let data = b"Unknown size data";
        let cursor = Cursor::new(data.to_vec());
        let mut extractor = StreamingExtractor::new(Box::new(cursor), None);

        assert_eq!(extractor.total_size(), None);
        assert_eq!(extractor.progress(), None);

        let mut buffer = Vec::new();
        extractor.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, data);
        assert_eq!(extractor.bytes_read(), data.len() as u64);
    }
}
