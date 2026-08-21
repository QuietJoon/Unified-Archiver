//! Backend capability traits (Group D D1 / R0068-0029).
//!
//! Each archive-format implementation in `src/ffi/*` exposes a small set
//! of methods with congruent signatures: `list_files`, `extract_to_memory`,
//! `extract_to_stream`, `extract_file`, `test_integrity`. Historically
//! every dispatch site re-walked the `ArchiveBackend` enum manually,
//! producing match ladders that drifted whenever a new backend or
//! capability landed.
//!
//! The [`ReadBackend`] trait formalises that pattern. It does not yet
//! replace the `ArchiveBackend` enum — D2 (Archive god-object split)
//! has already landed additively as the `v2-api` typed-handle surface
//! (`crate::v2::{ReadArchive, WriteArchive, ModifyArchive}` per AD 0053);
//! the remaining work is operation-parity and registry cleanup tracked
//! in OI-0076-004. For now the trait gives:
//!
//! 1. A single place to read each backend's read-side contract.
//! 2. A drop-in `&dyn ReadBackend` view that
//!    [`crate::extraction::dispatch_read_backend`] can hand off to
//!    callers, eliminating the per-call-site match.
//! 3. The home of [`ReadBackend::validate`] (AD 0052, D8 — landed
//!    2026-08-21 when the deferral was retired): the cheap
//!    "is this a usable archive?" probe callers opt into, with the
//!    per-backend timing differences expressed once, in the trait's
//!    default body, instead of per call site.
//!
//! Mode-specific surfaces are not yet trait-shaped: write support is a
//! local enum in `creation.rs` and modify routing remains on `Archive`.
//! This file is the read core only; the planned `WriteBackend` /
//! `ModifyBackend` traits sit alongside the OI-0076-004 parity work.
//!
//! # Architectural roadmap notes
//!
//! Known follow-ups, kept here so future work has the context the
//! `ReadBackend` contract assumes (parity status itself is tracked in
//! OI-0076-004):
//!
//! - **Half-trait/half-enum dispatch.** Plan-based extraction has
//!   landed via [`ReadBackend::extract_all(&mut ExtractionPlan)`];
//!   the remaining work is consolidating the per-backend extraction
//!   loops behind that single dispatch point so `extract_filtered`,
//!   selection, limits, warnings, and progress stop pattern-matching
//!   `ArchiveBackend` variants in `extraction.rs`.
//! - **Extraction loop duplication.** Each
//!   backend has its own listing/progress/copy/commit loop. Sharing
//!   them needs the plan-based dispatch to avoid
//!   freezing the current per-backend signatures into the trait.
//! - **`extract_to_stream` overload.** ZIP/7z/RAR
//!   buffer the entry into memory before exposing a `Read` adapter;
//!   only libarchive truly streams. The capability is documented in
//!   prose in `Archive::extract_to_stream`'s rustdoc but a
//!   typed `streaming_mode()` capability query would surface the
//!   distinction in the type system.
//! - **Link/special policy spread.** Each backend
//!   classifies and skips/extracts links per its metadata API. A
//!   normalised internal `EntryKind` would let extraction policy
//!   live in one place, but it requires a parallel `link_target` /
//!   `hardlink_target` enrichment pass per backend.
//! - **Progress total computation backend-local.** Folds
//!   into the plan-based dispatch above.
//! - **Atomic output sequencing.** Reasonable to share
//!   via a small `ExtractionOutput` writer but blocked on the
//!   trait-vs-enum decision.
//! - **`ffi/common.rs` grab-bag.** Mechanical split
//!   into `path_policy`, `atomic_output`, `progress`, `copy` is safe
//!   but pure churn; deferred to keep the diff-history manageable
//!   while higher-priority architectural items are still open.
//! - **Backend op-label normalisation.** Threading the
//!   facade-side operation through every backend error-construction
//!   site (started for `copy_with_optional_crc_bounded`)
//!   is a long mechanical pass; tackled incrementally.
//! - **Capability vs dispatch joined.** A backend
//!   registry that owns both `ArchiveFormat::can_*` predicates and
//!   the `ArchiveBackend` constructor pointers would ensure they
//!   stay in sync; needs the `v2-api` feature gate from D2.
//! - **Offset-capable opens are not a trait capability yet** (ticket
//!   `1ddc37ec`). Reading an archive that begins partway into a file
//!   (an SFX payload) is a per-backend property, not a shared one: the
//!   ZIP reader can do it because the `zip` crate resolves prepended
//!   data itself, so `Archive::try_open_in_place` hands that backend
//!   the outer file and skips the payload copy entirely. libarchive, 7z
//!   and UnRAR cannot, so `Archive` copies the payload to a tempfile
//!   for them (which is what the AD 0040 ceiling bounds). The routing
//!   therefore lives in `Archive`, keyed on the detected payload
//!   format, rather than behind a `ReadBackend` method. A
//!   `fn open_at_offset(path, offset) -> Option<Self>` capability on
//!   the read trait would move the decision to the backends that can
//!   answer it — worth doing once a second backend can, since one
//!   `Some` arm and three `None` arms is a worse shape than the
//!   current single call site.
//!
//! These do not have individual ADRs because the open-issue surface
//! they share (D1/D2 backend trait + Archive split) already has AD
//! 0053. Use that as the umbrella reference.

use crate::archive::{Archive, ArchiveBackend, ArchiveMode};
use crate::entry::ArchiveEntry;
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::streaming::StreamingExtractor;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

/// All inputs a backend needs to perform a full-archive (or
/// selection-driven) extraction (R0069-0003 / D1).
///
/// Built once per call from
/// [`crate::options::ExtractionOptions`] + the active `Archive` state,
/// then handed to the backend's [`ReadBackend::extract_all`]
/// implementation. Replaces the per-backend match ladder in
/// `dispatch_extract_core` whose argument shape drifted whenever a new
/// option landed (the `Libarchive` arm grew two extra parameters past
/// the others over time).
///
/// Fields that only some backends consume
/// (`max_file_size` / `max_total_size` for libarchive and UnRAR) are
/// carried through verbatim; backends that don't honour them ignore
/// the fields. The trait surface stays uniform.
pub(crate) struct ExtractionPlan<'a> {
    /// Destination directory (caller-supplied; backend respects verbatim).
    pub destination: &'a Path,
    /// Progress callback (`None` when caller didn't set one). The
    /// concrete type matches what every backend's existing
    /// `extract_all_with_options` already takes (`Option<&mut Box<dyn
    /// ProgressCallback>>`), so the trait forwarders need no
    /// argument-shape adapters.
    pub progress: Option<&'a mut Box<dyn crate::options::ProgressCallback>>,
    /// Overwrite existing files at the destination.
    pub overwrite: bool,
    /// Preserve Unix permissions on extracted files (R0079-0019). UnRAR
    /// honours the opt-out too (R0080-0023): it stamps the archive's
    /// attributes on the file it writes, so `false` means the staged
    /// write's mode is overridden afterwards.
    pub preserve_permissions: bool,
    /// Preserve modification times on extracted files (R0079-0019). UnRAR
    /// honours the opt-out too (R0080-0023): the archive's entry times it
    /// stamps on the staged file are overridden when `false`.
    pub preserve_times: bool,
    /// Verify per-entry CRC32 during extraction (ZIP / 7z).
    pub verify_crc32: bool,
    /// Selection subset by entry id; `None` means "extract everything".
    pub selection: Option<&'a HashSet<usize>>,
    /// Per-entry size cap. Enforced by libarchive and by UnRAR inside its
    /// data callback (R0080-0008/0022); other backends ignore it.
    pub max_file_size: Option<u64>,
    /// Cumulative total-bytes cap. Enforced by libarchive and by UnRAR
    /// inside its data callback (R0080-0008/0022); other backends ignore it.
    pub max_total_size: Option<u64>,
}

/// One entry the single-traversal payload walk must visit, addressed by
/// its stable listing id (OI-0001-009 / ticgit `82bf8fd4`).
///
/// Carries exactly what the visitor needs and nothing it could be
/// tempted to re-resolve: the id the walk seeks to, the normalized
/// listing name the backend uses as its drift guard, and the listing's
/// declared size — the one authority the DCR-011 exact bound is applied
/// against. Deliberately *not* the `ArchiveEntry`: the walk must never
/// be able to fall back to matching by `entry.path`, which is precisely
/// the OI-0076-002 regression an id-keyed resolver exists to prevent.
pub(crate) struct PayloadTarget<'a> {
    /// Stable listing id (positional index in the backend's listing).
    pub id: usize,
    /// Normalized listing name already resolved for `id`. Backends use it
    /// as a drift guard and must refuse a header whose name disagrees.
    pub validated_path: &'a str,
    /// Listing's declared uncompressed size, `None` when the format never
    /// declared one. The caller — not the backend — decides what bound to
    /// hold the payload to (DCR-011 hard constraint 3).
    pub declared_size: Option<u64>,
}

/// Visitor handed each target's payload reader by
/// [`ReadBackend::visit_payloads_by_listing_id`].
///
/// The reader is borrowed from the walk's shared handle and is valid only
/// for the duration of the call; the visitor must not retain it. Bounding
/// the payload (declared-size exactness, ceiling caps) is the visitor's
/// job — see [`PayloadTarget::declared_size`].
pub(crate) type PayloadVisitor<'v> =
    dyn FnMut(&PayloadTarget<'_>, &mut dyn std::io::Read) -> Result<()> + 'v;

/// Reject a decoded payload that exceeds the caller's per-entry cap.
/// Shared by the trait-default `extract_to_memory_with_limit` and the
/// RAR override so the diagnostic keeps one shape.
fn check_decoded_cap(buf: &[u8], max_bytes: Option<u64>, file_path: &str) -> Result<()> {
    if let Some(cap) = max_bytes {
        if buf.len() as u64 > cap {
            return Err(ArchiveError::OperationBlocked {
                operation: crate::error::ops::EXTRACT_TO_MEMORY.to_string(),
                reason: format!(
                    "Decoded payload for '{}' is {} bytes; exceeds the configured per-entry limit of {} bytes",
                    file_path,
                    buf.len(),
                    cap
                ),
            });
        }
    }
    Ok(())
}

/// Read-side capability surface shared by every archive backend.
///
/// The trait deliberately covers only the core read path. Operations
/// with backend-specific parameter shapes
/// (`extract_all_with_options`, `extract_file_with_options`,
/// metadata-only listings, password-aware reopens) stay on the concrete
/// types until the trait surface for those is stabilised — adding them
/// here would freeze parameter ordering before the
/// [`crate::options::ExtractionOptions`] surface itself converges.
///
/// Crate-internal: external callers continue to drive everything
/// through the public [`crate::Archive`] facade.
pub(crate) trait ReadBackend {
    /// List every entry's metadata (path, size, type, encryption flag,
    /// crc32 when surfaced cheaply by the format).
    ///
    /// Per MADR-0001 implementations must NOT walk payload data — CRC32
    /// is read from central-directory / TOC metadata only. Backends
    /// that need a real walk (libarchive validate path, RAR
    /// integrity test) live on [`Self::test_integrity`].
    ///
    /// Returns shared ownership of the backend's memoised listing
    /// (OI-0065-003): every impl caches `Arc<Vec<ArchiveEntry>>` at
    /// first use, so the facade `entry_cache` and the backend cache
    /// pin one `Vec` instead of two per handle.
    ///
    /// `budget = Some(n)` caps the listing at `n` entries: the parse aborts
    /// with [`crate::security::too_many_entries_parsed`] before allocating
    /// past the budget, so an attacker-controlled record count cannot force a
    /// full parse + allocation before the safety gate is consulted
    /// (OI-0080-003). `None` is unbudgeted.
    ///
    /// **Per-backend guarantee (honest residual).** The budget always bounds
    /// *our* `Vec<ArchiveEntry>`, but how early the abort lands depends on how
    /// the backend reads its directory:
    /// - **libarchive, UnRAR** — streaming header-by-header walks; the abort
    ///   stops reading further records, so neither our `Vec` nor the library
    ///   reads past `budget + 1` entries (true early abort).
    /// - **zip crate, sevenz-rust2** — the library materializes its own
    ///   central-directory / TOC `Vec` when the reader/handle is constructed
    ///   (before our walk begins). The budget therefore caps only our `Vec`
    ///   (plus the ZIP secondary timestamp walk); the library-internal
    ///   allocation is not bounded. For these formats the post-materialization
    ///   gate ([`crate::security::check_extraction_safe`]) stays the real cap.
    ///
    /// **Caching.** The budget applies only when the backend's listing cache
    /// is first populated. A call that hits the cache returns the cached `Arc`
    /// unchanged regardless of `budget`. An aborted budgeted parse does not
    /// populate (nor poison) the cache — `get_or_try_init` stores only on
    /// `Ok`, so a later unbudgeted call can still parse successfully.
    fn list_files_budgeted(&self, budget: Option<usize>) -> Result<Arc<Vec<ArchiveEntry>>>;

    /// Unbudgeted listing — delegates to [`Self::list_files_budgeted`] with no
    /// cap. Plain listing is policy-free (OI-0080-003); per-operation limits
    /// are applied by the extraction gate, not here.
    fn list_files(&self) -> Result<Arc<Vec<ArchiveEntry>>> {
        self.list_files_budgeted(None)
    }

    /// Force the archive's first-operation parse and report whether the
    /// input is a usable archive (AD 0052 / D8).
    ///
    /// # Why this exists
    ///
    /// A backend's `open()` does not parse the archive — it sets up a
    /// handle. `open()` returning `Ok` therefore does **not** mean the
    /// input is a valid archive: a corrupt ZIP opens fine and fails at
    /// `list_files`. That is the official contract (AD 0052, deferral
    /// retired 2026-08-21), not an accident. Two backends fail earlier
    /// because they read a little at open-time: libarchive probes the
    /// first entry header (R0001-0012) and UnRAR reads the archive's
    /// main header. Neither looks past that point.
    ///
    /// `validate` is the explicit way to ask the question `open()` does
    /// not answer: it runs exactly the parse the *next* real operation
    /// would have run, and reports its outcome. It never reports
    /// anything about payload bytes.
    ///
    /// # Cost — and how it differs from `validate_integrity`
    ///
    /// The two are deliberately *not* the same name because they are
    /// orders of magnitude apart:
    ///
    /// | | what it reads | cost |
    /// |---|---|---|
    /// | `validate` (this) | the directory / TOC / header stream only | one metadata parse, `O(entries)` |
    /// | [`crate::Archive::validate_integrity`] | **every entry's payload**, decoded and checksummed | full decode of the archive, `O(uncompressed bytes)` |
    ///
    /// So `validate` answers "can this archive be read at all?" and
    /// `validate_integrity` answers "is every stored byte intact?". A
    /// call site that wants the first and reaches for the second pays a
    /// full decompression pass for a well-formedness check.
    ///
    /// # It is paid at most once (AD 0065)
    ///
    /// The parse `validate` forces is the *same* parse the first
    /// `list_files` would have forced, and every backend memoises it as
    /// `Arc<Vec<ArchiveEntry>>` in its listing cache (OI-0065-003 — see
    /// [`Self::list_files_budgeted`]). A `list_files` after a `validate`
    /// therefore hits that cache and returns the very `Arc` the
    /// `validate` populated: **no second parse**. That memoisation is
    /// what makes this probe cheap enough to be worth having (it is
    /// also what dissolved the "redundant parse" cost that originally
    /// motivated deferring eager validation), and it is proven — not
    /// assumed — by
    /// `ffi::zip_wrapper::tests::validate_then_list_files_serves_the_cached_listing`,
    /// which fails if the listing is parsed twice.
    ///
    /// A failed `validate` does not poison the cache: `get_or_try_init`
    /// stores only on `Ok`, so a later call can still succeed if the
    /// cause was transient.
    ///
    /// # Per-backend timing
    ///
    /// One default body covers every backend, so the timing differences
    /// live here rather than in per-backend overrides:
    ///
    /// - **zip crate, sevenz-rust2** — nothing has been read yet at
    ///   `open`; this call is the first read, and it is where a corrupt
    ///   central directory / TOC surfaces.
    /// - **libarchive, UnRAR** — streaming header walks; this call reads
    ///   every header. For libarchive it is strictly more than `open`'s
    ///   first-header probe already did, so it can still fail here on an
    ///   archive whose *later* headers are damaged.
    ///
    /// Write-mode handles never reach this method: the facade gates on
    /// [`ArchiveMode`] first (see [`dispatch_read_archive`]).
    fn validate(&self) -> Result<()> {
        // Unbudgeted on purpose: the probe is policy-free, exactly like
        // plain `list_files` (OI-0080-003). A caller that wants an
        // entry-count cap enforced during the parse asks for the
        // budgeted listing instead.
        self.list_files().map(|_| ())
    }

    /// Decode `file_path` into an in-memory `Vec<u8>` and return it.
    ///
    /// Callers requesting bounded memory should clamp via
    /// [`crate::security::ExtractionLimits::max_file_size`] before
    /// reaching this method; the trait itself does not enforce a cap.
    fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>>;

    /// Decode `file_path` into a `Vec<u8>` while enforcing
    /// `max_bytes` as a hard ceiling on actual decoded payload size.
    ///
    /// Default implementation calls [`Self::extract_to_memory`] then
    /// rejects the result if it exceeds `max_bytes` — sufficient for
    /// backends that already enforce the declared-size cap inside the
    /// decoder copy path (`copy_with_optional_crc_bounded`). RAR
    /// overrides this method because its `extract_file` must stage the
    /// payload to a temp directory before the actual size is known
    /// (R0072-0006), so the override gets to short-circuit before the
    /// `read_to_end` happens.
    fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        let buf = self.extract_to_memory(file_path)?;
        check_decoded_cap(&buf, max_bytes, file_path)?;
        Ok(buf)
    }

    /// Return a streaming reader for `file_path`.
    ///
    /// Only libarchive-backed formats stream without first buffering
    /// the entry; ZIP, 7z, and RAR currently materialize and wrap
    /// the buffer in a `Cursor` (tracked by DEF-004 and OI-0057-007).
    fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor>;

    /// Streaming variant of [`Self::extract_to_memory_with_limit`].
    ///
    /// R0075-0018: when the caller supplies `max_bytes`, the default
    /// impl now wraps the returned [`StreamingExtractor`] in a custom
    /// bounded reader so the cap is actually enforced on every
    /// [`Read::read`] call. Previously the parameter was accepted and
    /// silently dropped, leaving direct backend callers with a
    /// "bounded" stream that wasn't bounded.
    ///
    /// Backends that stage to memory or disk before wrapping a `Cursor`
    /// (ZIP, 7z and RAR — every backend except libarchive) **must**
    /// override this to enforce the cap *before* the underlying buffer is
    /// built (R0072-0006 / R0001-0011); they can avoid the extra wrapper
    /// by ensuring `max_bytes` is applied during the stage step. This
    /// default materializes the entry in full and only then caps the
    /// reader, which is correct only for the genuinely incremental
    /// libarchive reader, where the wrapper *is* the materialization
    /// bound.
    fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<StreamingExtractor> {
        let stream = self.extract_to_stream(file_path)?;
        Ok(match max_bytes {
            Some(cap) => stream.with_hard_cap(cap),
            None => stream,
        })
    }

    /// Stream a single entry addressed by its stable listing `id` rather
    /// than by path (ti-2a6e3153).
    ///
    /// The content-multiset digest walk
    /// ([`crate::Archive::calculate_content_multiset_digest_and_size`])
    /// streams CRC-less entries to hash their payloads. Addressing by
    /// path routes through
    /// [`crate::security::validate_single_entry`], which refuses
    /// duplicate paths — so a legitimate duplicate-path tar (`tar -rf`
    /// append) could not be digested (OI-0076-002 / R0079-0028). Seeking
    /// by the stable listing id lets each occurrence's real payload be
    /// hashed. `validated_path` is the normalized listing name the caller
    /// already resolved for `id`; the libarchive override uses it as a
    /// drift guard (it refuses a header whose name doesn't match), reusing
    /// the OI-0076-002 index-walk machinery.
    ///
    /// The default returns [`ArchiveError::NotImplemented`] naming the
    /// backend so the caller can fall back to the path-based stream.
    ///
    /// **Which backends must override it.** Every backend whose listing
    /// can surface a file entry with `crc32: None`:
    /// - **libarchive** — the TAR family and ISO carry no per-entry
    ///   checksum at all (the original ti-2a6e3153 case);
    /// - **the zip crate** — since DCR-012 an AE-2 AES entry lists `None`,
    ///   because AE-2 stores the specification's placeholder 0 in the
    ///   CRC32 field rather than a checksum;
    /// - **7z** — the kCRC digest is *optional* in the format, and
    ///   `SevenZArchive::entry_crc32` gates on `entry.has_crc`, so an
    ///   entry written without it (7-Zip does this for no-stream empty
    ///   files, and nothing obliges a writer to store it for a streamed
    ///   member) lists `None`.
    ///
    /// Only RAR5 is genuinely exempt: unrar reports a per-entry CRC32 for
    /// every file entry, so the digest walk short-circuits on
    /// `entry.crc32` and never reaches this method for it. A backend that
    /// starts listing `None` without overriding this method does not fail
    /// — it silently falls back to the by-path stream and loses
    /// duplicate-path support (OI-0076-002), which no compiler diagnostic
    /// reports. Prove the wiring from the facade, not from the inherent
    /// method.
    fn extract_to_stream_by_listing_id(
        &self,
        _id: usize,
        _validated_path: &str,
    ) -> Result<StreamingExtractor> {
        Err(ArchiveError::not_implemented(
            crate::error::ops::EXTRACT_TO_STREAM,
            format!(
                "id-based streaming is not implemented for backend {}",
                std::any::type_name::<Self>()
            ),
        ))
    }

    /// Resolve **every** listed target's payload in a *single* traversal
    /// of the archive (OI-0001-009 / ticgit `82bf8fd4`).
    ///
    /// [`Self::extract_to_stream_by_listing_id`] re-opens the archive and
    /// re-walks it from the start for each entry, so the content-multiset
    /// digest of a CRC-less archive costs one full decode pass per
    /// CRC-less entry — quadratic in the entry count, and on a compressed
    /// TAR that means decompressing from byte zero once per member. This
    /// method collapses that into one pass: the backend walks its headers
    /// once and calls `visit` with a borrowed reader positioned at each
    /// target, in ascending id order.
    ///
    /// **Contract for implementors.**
    /// - Targets are addressed by id; resolution must stay id-keyed. A
    ///   pass that matched by `validated_path` instead would re-hash the
    ///   first occurrence of every duplicate path (OI-0076-002).
    /// - `validated_path` is a drift guard, not a selector: refuse a
    ///   header whose normalized name disagrees with it.
    /// - The reader handed to `visit` yields the entry's payload and
    ///   nothing else, and it is *unbounded* — the visitor applies the
    ///   DCR-011 size contract, because only the caller knows whether a
    ///   declared size exists to be held to exactly.
    /// - Every target must be visited exactly once, or an error returned.
    ///   Silently skipping one would drop an element from the digest's
    ///   multiset.
    /// - A `visit` error propagates verbatim, so the visitor's own
    ///   diagnostics (which name the offending entry) survive.
    ///
    /// The default returns [`ArchiveError::NotImplemented`] naming the
    /// backend; callers fall back to the per-entry
    /// [`Self::extract_to_stream_by_listing_id`] path, which is correct,
    /// just quadratic. Only libarchive overrides this — it is the only
    /// backend whose CRC-less entries can number in the thousands, and the
    /// only one that must re-decompress from byte zero to reach each of
    /// them. The ZIP backend's CRC-less set is the AE-2 AES entries, each
    /// of which is already a random-access seek into an indexed central
    /// directory; the 7z backend's is the entries written without the
    /// optional kCRC digest, and sevenz-rust2 materializes the whole TOC
    /// up front so each seek is an indexed walk rather than a fresh
    /// decode of the container. A sequential walk would buy either of them
    /// nothing.
    fn visit_payloads_by_listing_id(
        &self,
        _targets: &[PayloadTarget<'_>],
        _visit: &mut PayloadVisitor<'_>,
    ) -> Result<()> {
        Err(ArchiveError::not_implemented(
            crate::error::ops::EXTRACT_TO_STREAM,
            format!(
                "single-traversal payload resolution is not implemented for backend {}",
                std::any::type_name::<Self>()
            ),
        ))
    }

    /// Decode `file_path` directly to disk under `dest_path`.
    ///
    /// Implementations sanitize the entry path against `dest_path`
    /// themselves (all five backends route through
    /// `security::sanitize_entry_path`), so a backend written against
    /// this trait must not skip that step. Hoisting per-entry
    /// sanitization to the facade/plan layer — so backends honour
    /// resolved paths verbatim — is part of the D1 loop consolidation.
    ///
    /// Currently a thin alias for the legacy public path on each
    /// wrapper; once D2 collapses the `extract_file_with_options` family
    /// behind a uniform options surface, this method becomes the
    /// canonical disk-write entry. Provided as a default-method-less
    /// requirement so every backend explicitly opts in.
    #[expect(
        dead_code,
        reason = "canonical disk-write entry per the D2 (Archive god-object split) refactor; every backend implements it, but no trait-level caller invokes it yet"
    )]
    fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()>;

    /// Walk every file entry, surface a list of paths whose payloads
    /// fail integrity checks. Empty result means the archive is sound.
    fn test_integrity(&self) -> Result<Vec<String>>;

    /// Extract every entry (or, if [`ExtractionPlan::selection`] is
    /// `Some`, just the selected entries) under
    /// [`ExtractionPlan::destination`] (R0069-0003 / D1).
    ///
    /// Each backend forwards to its existing
    /// `extract_all_with_options` helper, slicing whichever fields
    /// of the plan it actually consumes. The trait method exists so
    /// `extraction.rs::dispatch_extract_core` can call a single
    /// `&dyn ReadBackend` method instead of a per-variant match.
    ///
    /// Default impl returns `OperationBlocked` so a future read backend
    /// that hasn't wired up this method fails loudly rather than
    /// silently no-op'ing through `dispatch_extract_core`. R0076-0007:
    /// uses an op-blocked error rather than a synthetic `ArchiveFormat::Tar`
    /// so the diagnostic doesn't lie about which format is unsupported —
    /// the real backend's format is already in the surrounding `Archive`
    /// context.
    fn extract_all(&self, _plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        Err(ArchiveError::OperationBlocked {
            operation: crate::error::ops::EXTRACT_ALL.to_string(),
            reason: "extract_all not implemented for this backend".to_string(),
        })
    }
}

// ──────────────────────────────────────────────────────────────────────
// Per-backend impls — thin forwarders to the existing inherent methods.
// Keeping the bodies trivial means future signature changes on the
// concrete types do not have to update both sides at once.
// ──────────────────────────────────────────────────────────────────────

// `#[inline]` on each forwarder lets LLVM see through the trait
// dispatch when the concrete type is known statically (e.g. inside a
// match arm before the `&dyn ReadBackend` view is built). Trait-object
// dispatch through `dispatch_read_backend` is still a vtable call —
// these hints help the rare direct callers, not the dyn path.
impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive {
    #[inline]
    fn list_files_budgeted(&self, budget: Option<usize>) -> Result<Arc<Vec<ArchiveEntry>>> {
        // OI-0080-003: the `zip` crate reads the whole central directory when
        // the `ZipArchive` is constructed, before our walk. The budget caps
        // only our `Vec<ArchiveEntry>`; the library-internal directory is not
        // bounded (see trait rustdoc). The post-materialization gate stays the
        // real cap.
        Self::list_files_budgeted(self, budget)
    }

    #[inline]
    fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        Self::extract_to_memory(self, file_path)
    }

    /// R0081-0030: honour the caller cap before buffering — the ZIP
    /// memory path rejects an over-cap declared size up front instead of
    /// inheriting the trait default that buffers the whole payload and
    /// only then compares against the cap.
    ///
    /// R5 (ti-581bcda4): this is the *memory* entry point, so it keeps
    /// `extract_to_memory`; only the stream override switches the label.
    #[inline]
    fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        Self::extract_to_memory_capped(
            self,
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_MEMORY,
        )
    }

    #[inline]
    fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> {
        Self::extract_to_stream(self, file_path)
    }

    /// R0001-0011: same pre-materialization cap as the memory path. The
    /// trait default would decode the whole entry and only then cap the
    /// reader, so a caller's `StreamBound`-derived budget bounded reads
    /// while the entry had already been buffered in full.
    #[inline]
    fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<StreamingExtractor> {
        Self::extract_to_stream_with_limit(self, file_path, max_bytes)
    }

    /// DCR-012: AE-2 AES entries list `crc32: None` (their stored CRC is
    /// the specification's placeholder, not a checksum), so the digest
    /// walk streams them — and it addresses entries by stable listing id.
    /// Without this forwarder the trait default returned
    /// `NotImplemented`, the caller fell back to the by-*path* stream, and
    /// an AES ZIP with duplicate (or duplicate-after-normalization) names
    /// stopped digesting at `validate_single_entry` /
    /// `reject_if_duplicate` with `OperationBlocked` — the OI-0076-002
    /// defect, reintroduced through ZIP. The inherent method's own rustdoc
    /// claims that outcome is avoided; only this wiring makes the claim
    /// true, so exercise it through
    /// [`crate::Archive::calculate_content_multiset_digest_and_size`]
    /// (`tests/integration/digest_ae2_duplicate_path.rs`) rather than by
    /// calling the inherent method, which cannot detect a missing forward.
    #[inline]
    fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<StreamingExtractor> {
        Self::extract_to_stream_by_listing_id(self, id, validated_path)
    }

    #[inline]
    fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        Self::extract_file(self, file_path, dest_path)
    }

    #[inline]
    fn test_integrity(&self) -> Result<Vec<String>> {
        Self::test_integrity(self)
    }

    #[inline]
    fn extract_all(&self, plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        Self::extract_all_with_options(
            self,
            plan.destination,
            plan.progress.as_deref_mut(),
            plan.overwrite,
            plan.preserve_permissions,
            plan.preserve_times,
            plan.verify_crc32,
            plan.selection,
        )
    }
}

impl ReadBackend for crate::ffi::sevenz_wrapper::SevenZArchive {
    #[inline]
    fn list_files_budgeted(&self, budget: Option<usize>) -> Result<Arc<Vec<ArchiveEntry>>> {
        // OI-0080-003: sevenz-rust2 materializes its full TOC (`archive.files`)
        // when the reader is constructed, before our walk. The budget caps only
        // our `Vec<ArchiveEntry>`; the library-internal TOC is not bounded (see
        // trait rustdoc). The post-materialization gate stays the real cap.
        Self::list_files_budgeted(self, budget)
    }

    #[inline]
    fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        Self::extract_to_memory(self, file_path)
    }

    /// R0076-0062: honour the caller cap before buffering — the 7z
    /// memory path rejects an over-cap declared size up front instead
    /// of decoding the full entry and post-checking.
    ///
    /// R5 (ti-581bcda4): memory entry point — keeps `extract_to_memory`.
    #[inline]
    fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        Self::extract_to_memory_capped(
            self,
            file_path,
            max_bytes,
            crate::error::ops::EXTRACT_TO_MEMORY,
        )
    }

    #[inline]
    fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> {
        Self::extract_to_stream(self, file_path)
    }

    /// R0001-0011: same pre-materialization cap as the memory path — the
    /// trait default would decode the entry in full before capping the
    /// reader (AD 0035 keeps the buffering itself; what changes is that
    /// the caller's budget now bounds it).
    #[inline]
    fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<StreamingExtractor> {
        Self::extract_to_stream_with_limit(self, file_path, max_bytes)
    }

    /// A 7z file entry lists `crc32: None` whenever its record carries no
    /// optional kCRC digest (`sevenz_wrapper::SevenZArchive::entry_crc32`
    /// gates on `entry.has_crc`), so 7z reaches the digest walk's
    /// streaming arm exactly as the TAR family and AE-2 ZIP do. Without
    /// this forwarder the trait default returned `NotImplemented`, the
    /// walk fell back to the by-*path* stream, and a 7z repeating a path
    /// with a CRC-less occurrence stopped digesting at
    /// `validate_single_entry` with `OperationBlocked` — the OI-0076-002
    /// defect, reached through 7z. Same lesson as the ZIP forwarder: the
    /// gap compiles clean and every test written against the inherent
    /// method still passes, so exercise it through
    /// [`crate::Archive::calculate_content_multiset_digest_and_size`].
    #[inline]
    fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<StreamingExtractor> {
        Self::extract_to_stream_by_listing_id(self, id, validated_path)
    }

    #[inline]
    fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        Self::extract_file(self, file_path, dest_path)
    }

    #[inline]
    fn test_integrity(&self) -> Result<Vec<String>> {
        Self::test_integrity(self)
    }

    #[inline]
    fn extract_all(&self, plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        Self::extract_all_with_options(
            self,
            plan.destination,
            plan.progress.as_deref_mut(),
            plan.overwrite,
            plan.preserve_permissions,
            plan.preserve_times,
            plan.verify_crc32,
            plan.selection,
        )
    }
}

impl ReadBackend for crate::ffi::libarchive_wrapper::LibarchiveArchive {
    #[inline]
    fn list_files_budgeted(&self, budget: Option<usize>) -> Result<Arc<Vec<ArchiveEntry>>> {
        // Libarchive is metadata-only by MADR-0001; the inherent method keeps
        // its `list_files_metadata_only` name on the concrete type.
        // OI-0080-003: libarchive walks headers one-by-one via
        // `archive_read_next_header`, so the budget is a true streaming early
        // abort — it bounds the library's header reads too, not just our `Vec`
        // (see trait rustdoc).
        Self::list_files_metadata_only_budgeted(self, budget)
    }

    #[inline]
    fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        Self::extract_to_memory(self, file_path)
    }

    /// R0075-0017: route the caller cap into libarchive's internal
    /// memory loop so unknown-size entries respect a tighter
    /// per-entry limit before allocating.
    #[inline]
    fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        Self::extract_to_memory_with_max_bytes(self, file_path, max_bytes)
    }

    #[inline]
    fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> {
        Self::extract_to_stream(self, file_path)
    }

    /// ti-2a6e3153: reuse the OI-0076-002 seek machinery to stream by
    /// stable listing id (not path), so the digest walk can hash each
    /// occurrence of a duplicate-path CRC-less tar. Libarchive's listings
    /// are CRC-less for the whole TAR family and ISO; the ZIP backend
    /// overrides the same method for its own narrower CRC-less case
    /// (AE-2 AES entries, DCR-012).
    #[inline]
    fn extract_to_stream_by_listing_id(
        &self,
        id: usize,
        validated_path: &str,
    ) -> Result<StreamingExtractor> {
        Self::extract_to_stream_by_listing_id(self, id, validated_path)
    }

    /// OI-0001-009 (ticgit `82bf8fd4`): libarchive is the backend whose
    /// per-entry re-open was quadratic — a compressed TAR was decompressed
    /// from byte zero once per CRC-less member. The inherent walk resolves
    /// every target from one handle.
    #[inline]
    fn visit_payloads_by_listing_id(
        &self,
        targets: &[PayloadTarget<'_>],
        visit: &mut PayloadVisitor<'_>,
    ) -> Result<()> {
        Self::visit_payloads_by_listing_id(self, targets, visit)
    }

    #[inline]
    fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        Self::extract_file(self, file_path, dest_path)
    }

    #[inline]
    fn test_integrity(&self) -> Result<Vec<String>> {
        Self::test_integrity(self)
    }

    #[inline]
    fn extract_all(&self, plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        Self::extract_all_with_options(
            self,
            plan.destination,
            plan.progress.as_deref_mut(),
            plan.overwrite,
            plan.preserve_permissions,
            plan.preserve_times,
            plan.selection,
            plan.max_file_size,
            plan.max_total_size,
        )
    }
}

// ──────────────────────────────────────────────────────────────────────
// Centralised dispatch helpers — single source of truth for "match
// `ArchiveBackend`, hand the read variants to a `&dyn ReadBackend`,
// surface `WriteModeOnly` for `ZipWriter`."
// ──────────────────────────────────────────────────────────────────────

/// Borrow a read-capable backend variant as a `&dyn ReadBackend`.
///
/// Returns `None` for `ZipWriter`, the only non-readable variant. Callers
/// that need to dispatch + branch on the missing case should use
/// [`dispatch_read_backend`] (variant-only, no mode check) or
/// [`dispatch_read_archive`] (preferred — also gates on
/// [`ArchiveMode`]).
pub(crate) fn read_backend_view(backend: &ArchiveBackend) -> Option<&dyn ReadBackend> {
    match backend {
        #[cfg(feature = "rar-support")]
        ArchiveBackend::Unrar(u) => Some(u.as_ref()),
        ArchiveBackend::SevenZ(s) => Some(s.as_ref()),
        ArchiveBackend::ZipReader(z) => Some(z.as_ref()),
        ArchiveBackend::Libarchive(l) => Some(l.as_ref()),
        ArchiveBackend::ZipWriter(_) => None,
    }
}

/// Run `f` against the backend's `&dyn ReadBackend` view, surfacing
/// `write_mode_only` when the variant is `ZipWriter`.
///
/// Variant-only check — does **not** consult [`ArchiveMode`]. Prefer
/// [`dispatch_read_archive`] from facade entry points so a Libarchive
/// handle that was opened in `Write`/`Modify` mode cannot silently be
/// used as a reader (R0070-0001).
pub(crate) fn dispatch_read_backend<T>(
    backend: &ArchiveBackend,
    op: &'static str,
    f: impl FnOnce(&dyn ReadBackend) -> Result<T>,
) -> Result<T> {
    match read_backend_view(backend) {
        Some(b) => f(b),
        None => Err(ArchiveError::write_mode_only(op)),
    }
}

/// Mode-aware variant of [`dispatch_read_backend`]: rejects archives
/// whose mode is `Write` (regardless of which backend variant they
/// happen to wrap) before consulting the trait view.
///
/// Use this at every facade-level read entrypoint so Libarchive-backed
/// write handles surface a clean `WriteModeOnly` instead of falling
/// through to the ReadBackend impl on `LibarchiveArchive` and reopening
/// the in-progress output file (R0070-0001). `Modify` mode passes
/// through — modification needs the read view to enumerate retained
/// entries during `commit_changes`.
pub(crate) fn dispatch_read_archive<T>(
    archive: &Archive,
    op: &'static str,
    f: impl FnOnce(&dyn ReadBackend) -> Result<T>,
) -> Result<T> {
    if archive.mode == ArchiveMode::Write {
        return Err(ArchiveError::write_mode_only(op));
    }
    dispatch_read_backend(&archive.backend, op, f)
}

impl Archive {
    /// Centralized listing dispatch shared by the cached `Archive::list_files`
    /// and the uncached `Archive::list_files_for_limits` paths. Lives here
    /// alongside [`dispatch_read_archive`] so every per-backend match
    /// ladder is sourced from one module and benefits from the mode gate.
    pub(crate) fn list_entries(&self, op: &'static str) -> Result<Arc<Vec<ArchiveEntry>>> {
        dispatch_read_archive(self, op, |b| b.list_files())
    }

    /// Budgeted sibling of [`Self::list_entries`] (OI-0080-003): threads
    /// the parse-time entry-count budget down to the backend so a listing
    /// materialized on behalf of a limits-carrying operation aborts before
    /// allocating past `budget`. `None` is unbudgeted (plain listing is
    /// policy-free). See [`crate::backend::ReadBackend::list_files_budgeted`]
    /// for the per-backend residual — three backends can only bound our
    /// `Vec`, not the library-internal TOC.
    pub(crate) fn list_entries_budgeted(
        &self,
        op: &'static str,
        budget: Option<usize>,
    ) -> Result<Arc<Vec<ArchiveEntry>>> {
        dispatch_read_archive(self, op, |b| b.list_files_budgeted(budget))
    }
}

#[cfg(feature = "rar-support")]
impl ReadBackend for crate::ffi::wrapper::UnrarArchive {
    #[inline]
    fn list_files_budgeted(&self, budget: Option<usize>) -> Result<Arc<Vec<ArchiveEntry>>> {
        // OI-0080-003: UnRAR walks headers one at a time (`read_header` +
        // `skip_entry`), so the budget aborts before reading past the
        // budget-th record — a true streaming early abort that bounds the
        // library's header reads too, not just our `Vec` (see trait rustdoc).
        Self::list_files_budgeted(self, budget)
    }

    #[inline]
    fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>> {
        Self::extract_to_memory(self, file_path)
    }

    #[inline]
    fn extract_to_memory_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        // R0072-0006: forward the per-entry cap so the temp-staged
        // payload's length is checked against the caller's
        // `ExtractionLimits` *before* the buffer is allocated and read.
        Self::extract_to_memory_with_limit(self, file_path, max_bytes)
    }

    #[inline]
    fn extract_to_stream(&self, file_path: &str) -> Result<StreamingExtractor> {
        Self::extract_to_stream(self, file_path)
    }

    #[inline]
    fn extract_to_stream_with_limit(
        &self,
        file_path: &str,
        max_bytes: Option<u64>,
    ) -> Result<StreamingExtractor> {
        // R0072-0006: same pre-allocation cap as the memory path.
        Self::extract_to_stream_with_limit(self, file_path, max_bytes)
    }

    #[inline]
    fn extract_file(&self, file_path: &str, dest_path: &Path) -> Result<()> {
        // Drop the path UnRAR returns — the trait surface only commits to
        // success/failure, but the inherent helper now carries the
        // canonical write path for `extract_to_memory_with_limit`'s use
        // (OI-0069-003).
        Self::extract_file(self, file_path, dest_path).map(|_| ())
    }

    #[inline]
    fn test_integrity(&self) -> Result<Vec<String>> {
        Self::test_integrity(self)
    }

    #[inline]
    fn extract_all(&self, plan: &mut ExtractionPlan<'_>) -> Result<Vec<ArchiveWarning>> {
        // R0080-0023: forward the metadata-preservation opt-outs. UnRAR
        // stamps the archive's mode/times on the file it writes, so honouring
        // `preserve_* == false` means overriding those after the staged write
        // (see `unrar_extract_atomic` / `finalize_staged_file`). R0080-0008:
        // the per-file and archive-wide byte caps are enforced inside the
        // UnRAR data callback.
        Self::extract_all_with_options(
            self,
            plan.destination,
            plan.progress.as_deref_mut(),
            plan.overwrite,
            plan.preserve_permissions,
            plan.preserve_times,
            plan.selection,
            plan.max_file_size,
            plan.max_total_size,
        )
    }
}
