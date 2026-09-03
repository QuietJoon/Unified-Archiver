//! Archive inspection operations
//!
//! This module provides methods for inspecting archive contents, validating integrity,
//! and querying archive metadata without extraction.

use crate::archive::{Archive, ArchiveBackend};
use crate::entry::{ArchiveEntry, EntryType};
use crate::error::ops;
use crate::error::{ArchiveError, ArchiveWarning, Result};
use crate::format::multipart::{parse_volume_name, parse_volume_set_for};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Typed return for [`Archive::multipart_layout`] (R0075-0083).
///
/// Replaces the historical `(bool, Vec<PathBuf>)` shape from
/// [`Archive::detect_multipart`] — callers that ignored the boolean
/// would otherwise misinterpret a single-part archive as a one-volume
/// set. The enum makes the two states distinguishable in the type
/// system, with the volume list always non-empty in `Multi` and
/// always exactly the source path in `Single`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartLayout {
    /// Archive is a single-volume archive. `path` is its on-disk path.
    Single { path: PathBuf },
    /// Archive is part of a multipart set. `parts` lists the volume
    /// files in volume order (always non-empty; the caller's source
    /// archive is one of the entries).
    Multi { parts: Vec<PathBuf> },
}

/// Report from archive integrity validation
///
/// **Construction (OI-0076-005).** The struct is `#[non_exhaustive]`: it is
/// an *output* type, produced only by `Archive::validate_integrity`, so no
/// constructor is offered and none is owed. External code reads the fields —
/// they stay `pub` — but cannot fabricate a report, which also means a new
/// counter can be added here without a major bump.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValidationReport {
    /// Total number of entries inspected (including directories and skipped
    /// entries).
    pub total_entries: usize,
    /// Number of regular file entries considered for validation.
    ///
    /// `total_entries - total_files` equals the count of directories and
    /// other non-file entries skipped by the integrity walk, so
    /// `validated + failed.len() == total_files` even when the archive
    /// holds directory entries (R0070-0062).
    pub total_files: usize,
    /// Number of regular file entries successfully validated. Always
    /// satisfies `validated + failed.len() == total_files`.
    pub validated: usize,
    /// Paths of regular file entries that failed validation.
    pub failed: Vec<String>,
}

/// The uncompressed byte total returned beside a content-multiset digest,
/// carrying how much of the listing it actually covers (OI-0001-007).
///
/// **Why this is not a `u64`.** The total is accumulated only from entries
/// whose listing declares a size. Two populations do not: raw single-file
/// `.gz` / `.bz2` / `.xz` members, whose readers set no uncompressed size
/// at all, and any libarchive entry whose `archive_entry_size_is_set` is
/// false (R0001-0014 — the reason `ArchiveEntry::size` is an `Option` in
/// the first place). Those entries used to contribute zero to a bare `u64`
/// and say nothing about it, so a caller comparing two archives, or
/// pre-sizing a destination, worked from a number that looked exact and
/// silently was not. The wrapper makes the shortfall part of the value:
/// [`Self::exact`] refuses to hand back a total that is missing entries,
/// and reaching for the old number is now a deliberate
/// [`Self::sized_bytes`] call.
///
/// **Why not `Option<u64>`.** Collapsing to `None` throws away the partial
/// total, which is the useful part — a caller that can size three of five
/// entries can still act on it (progress denominators, "at least N bytes"
/// checks, cheap inequality tests between two archives). Both the number
/// and its coverage survive here.
///
/// **What this does NOT buy.** It does not make the missing sizes
/// knowable: nothing here decompresses an unsized member to measure it,
/// and no accessor estimates one. It does not say *which* entries were
/// unsized — only how many — because the digest surface deliberately
/// discards path and order information (DCR-012), so naming them would
/// mean carrying data this call has no other reason to hold. And it says
/// nothing about the digest, which is computed over every file entry
/// including the unsized ones; an incomplete total does not weaken the
/// digest beside it.
///
/// Fields are private and there is no public constructor: like
/// [`ValidationReport`] this is an *output* type, produced only by
/// [`Archive::calculate_content_multiset_digest_and_size`] and the shims
/// over it, so the invariant `sized_entries + unsized_entries == the file
/// entries walked` cannot be broken from outside. Private fields also mean
/// a future counter can be added without a major bump, so no
/// `#[non_exhaustive]` is needed to buy that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizedContentTotal {
    sized_bytes: u64,
    sized_entries: usize,
    unsized_entries: usize,
}

impl SizedContentTotal {
    /// Sum of the declared uncompressed sizes of the file entries that
    /// declared one — unknown-size entries counted as zero.
    ///
    /// This is the pre-0.5.0 `u64` verbatim. It is still the right answer
    /// for a lower bound, a progress denominator, or a cheap "is this
    /// archive bigger" test; it is the wrong answer for anything that
    /// treats the number as the archive's true uncompressed size. Use
    /// [`Self::exact`] when the caller cannot tolerate a shortfall, and
    /// check [`Self::is_complete`] when it wants to degrade rather than
    /// fail.
    #[must_use]
    pub const fn sized_bytes(self) -> u64 {
        self.sized_bytes
    }

    /// The total, but only when every file entry declared a size.
    ///
    /// `None` means at least one entry contributed nothing and the number
    /// would understate the archive. Prefer this constructor-of-last-resort
    /// shape wherever the old `u64` was consumed as an exact figure —
    /// allocating a destination, asserting an equality, reporting a size to
    /// a user — since it turns a silent undercount into a case the caller
    /// must handle.
    #[must_use]
    pub const fn exact(self) -> Option<u64> {
        if self.is_complete() {
            Some(self.sized_bytes)
        } else {
            None
        }
    }

    /// True when no file entry was skipped, i.e. [`Self::sized_bytes`] is
    /// the archive's full uncompressed size.
    ///
    /// An archive with no file entries at all is complete: the total is a
    /// truthful zero, matching the empty digest the same call returns.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.unsized_entries == 0
    }

    /// Number of file entries that declared a size and are therefore
    /// included in [`Self::sized_bytes`].
    #[must_use]
    pub const fn sized_entries(self) -> usize {
        self.sized_entries
    }

    /// Number of file entries that declared no size and contributed
    /// nothing. Zero exactly when [`Self::is_complete`].
    #[must_use]
    pub const fn unsized_entries(self) -> usize {
        self.unsized_entries
    }

    /// Total file entries walked — the denominator for "sized `n` of `m`".
    ///
    /// Non-file entries (directories, symlinks, hard links) are not
    /// counted, matching the digest's own population.
    #[must_use]
    pub const fn file_entries(self) -> usize {
        self.sized_entries.saturating_add(self.unsized_entries)
    }
}

impl std::fmt::Display for SizedContentTotal {
    /// Renders the shortfall inline, so a caller that prints the value
    /// cannot present a partial total as a complete one by accident — the
    /// single most likely way the old bare `u64` misled a human reader.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_complete() {
            write!(f, "{} bytes", self.sized_bytes)
        } else {
            write!(
                f,
                "at least {} bytes ({} of {} file entries declare no size)",
                self.sized_bytes,
                self.unsized_entries,
                self.file_entries()
            )
        }
    }
}

impl Archive {
    /// List all files in the archive
    ///
    /// Returns cached `&[ArchiveEntry]` (zero-cost repeated access).
    /// - First call: reads from backend, caches in `OnceCell`
    /// - Subsequent calls: returns cached slice (O(1), no allocation)
    pub fn list_files(&self) -> Result<&[ArchiveEntry]> {
        self.entry_cache
            .get_or_try_init(|| self.list_entries(ops::LIST_FILES))
            .map(|v| v.as_slice())
    }

    /// List files for limit checking (metadata only, no CRC32 computation)
    ///
    /// Same contract as [`list_files`](Self::list_files) but without the
    /// facade `OnceCell` caching step — used by callers that want a
    /// listing without populating the facade cache. The backend-level
    /// cache (AD 0065) still serves the walk; one `Vec` clone pays for
    /// the owned-return contract.
    pub fn list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>> {
        self.list_entries(ops::LIST_FILES_FOR_LIMITS)
            .map(|arc| (*arc).clone())
    }

    /// Get count of entries in archive.
    ///
    /// **Mode-dependent semantics.** The meaning of the
    /// returned count differs by archive mode:
    ///
    /// - **Read / Modify mode** — returns the number of entries listed
    ///   in the archive's central directory / TOC. This is the number
    ///   of entries [`Archive::list_files`] would surface.
    /// - **Write mode** — returns the number of entries the writer has
    ///   already emitted (i.e. successful `add_*` calls). This is the
    ///   *write-progress* count, not the eventual entry count of the
    ///   completed archive — additional entries may still be added
    ///   before [`Archive::finish`].
    ///
    /// Callers that care about the difference should branch on
    /// [`Archive::mode`](crate::Archive) (read mode is the default
    /// after [`Archive::open`]).
    pub fn entry_count(&self) -> Result<usize> {
        use crate::archive::ArchiveMode;
        if self.mode == ArchiveMode::Write {
            return match &self.backend {
                ArchiveBackend::ZipWriter(w) => Ok(w.entries_written()),
                ArchiveBackend::Libarchive(b) => Ok(b.entries_written()),
                // R0071-0013: a Write-mode archive whose backend is
                // not a writer would mean a constructor / invariant
                // bug — surface it as an internal error instead of
                // silently returning 0 (which makes the broken state
                // look like an empty archive).
                other => Err(ArchiveError::operation_blocked(
                    "entry_count",
                    format!(
                        "entry_count: Write-mode archive has non-writer backend variant {:?}; this is a library invariant bug",
                        std::mem::discriminant(other)
                    ),
                )),
            };
        }
        self.list_files().map(|entries| entries.len())
    }

    /// Find a specific entry by path.
    ///
    /// **First-match-only contract.** Archives may
    /// legitimately contain duplicate paths (ZIP/7z central
    /// directories permit this); this helper returns the *first*
    /// match in `list_files()` order. Callers that need
    /// duplicate-aware lookup should use [`find_entries`](Self::find_entries)
    /// or `extract_by_ids`, which keys on
    /// [`ArchiveEntry::id`] instead.
    ///
    /// Performs O(n) linear search through all entries.
    pub fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>> {
        let entries = self.list_files()?;
        Ok(entries.iter().find(|entry| entry.path == path).cloned())
    }

    /// Find every entry whose path matches `path`.
    ///
    /// Returns *all* entries with the given path in `list_files()`
    /// order. Most archives have unique paths and this returns a
    /// single-element vector, but ZIP/7z central directories can
    /// legitimately contain duplicate paths (e.g. an archive that
    /// shadows an older entry with a newer one); callers that need
    /// every match should use this instead of [`find_entry`](Self::find_entry).
    pub fn find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>> {
        let entries = self.list_files()?;
        Ok(entries
            .iter()
            .filter(|entry| entry.path == path)
            .cloned()
            .collect())
    }

    /// Validate integrity of all entries by verifying checksums
    ///
    /// This method performs actual integrity verification by:
    /// - RAR/RAR5: Using UnRAR's test mode (RAR_TEST) - no disk extraction
    /// - ZIP: Extracting to memory and verifying CRC32
    /// - 7z: Extracting to memory with built-in CRC32 verification
    /// - TAR/etc: Extracting to memory and verifying checksums
    ///
    /// Returns a report with the number of files validated and list of failures.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.rar")?;
    /// let report = archive.validate_integrity()?;
    ///
    /// if report.failed.is_empty() {
    ///     println!("✓ All {} files validated successfully", report.validated);
    /// } else {
    ///     println!("✗ {} files failed validation:", report.failed.len());
    ///     for path in &report.failed {
    ///         println!("  - {}", path);
    ///     }
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn validate_integrity(&self) -> Result<ValidationReport> {
        let entries = self.list_files()?;
        let total_entries = entries.len();

        // Count files (excluding directories)
        let file_count = entries.iter().filter(|e| e.is_file()).count();

        // Backend-specific test_integrity goes through the mode-aware
        // `dispatch_read_archive` helper (D1 / R0068-0029, R0070-0001);
        // ZipWriter — and any other Write-mode handle — surfaces as
        // `WriteModeOnly`.
        let failed = crate::backend::dispatch_read_archive(self, ops::VALIDATE_INTEGRITY, |b| {
            b.test_integrity()
        })?;

        // `failed.len() > file_count` would mean the backend reported failures
        // for entries we did not even count as files — a programmer or
        // backend-state error. Surface it as `Corruption` rather than
        // saturating to zero (R0070-0063).
        if failed.len() > file_count {
            return Err(ArchiveError::corruption(
                self.path().display().to_string(),
                format!(
                    "validate_integrity: backend reported {} failed entries against {} regular files (over-saturation; backend state inconsistency)",
                    failed.len(),
                    file_count
                ),
            ));
        }
        let validated = file_count - failed.len();

        Ok(ValidationReport {
            total_entries,
            total_files: file_count,
            validated,
            failed,
        })
    }

    /// Cheap archive-level CRC32 summary built from the listing's
    /// CRC32 metadata only.
    ///
    /// Computes the wrapping arithmetic sum of every entry's
    /// **already-stored** CRC32 — the same value 7-Zip displays as the
    /// "archive CRC". Entries whose listing has no CRC32 (libarchive
    /// formats: TAR, TAR.GZ, TAR.BZ2, TAR.XZ, ISO; standalone Gzip /
    /// Bzip2 / Xz) contribute nothing to the sum.
    ///
    /// # Format independence (R0072-0011)
    ///
    /// This method is **format-independent only when every entry of
    /// every archive being compared exposes a CRC32 in its listing**.
    /// RAR always does. ZIP and 7z do for the ordinary case but not
    /// unconditionally: an AE-2 AES ZIP entry stores the specification's
    /// placeholder rather than a checksum and lists `None` (DCR-012), and
    /// 7z's kCRC digest is optional, so an entry written without it lists
    /// `None` too. A ZIP and a TAR.GZ holding the same files never agree —
    /// TAR.GZ has no per-entry CRC32 metadata at all.
    ///
    /// For a true content-identity hash that handles CRC-less formats
    /// uniformly, use [`Archive::calculate_manifest_digest`], which
    /// streams each entry's payload through a CRC32 hasher when the
    /// listing does not carry one (AD 0047 / `docs/STREAM_CRC32.md`).
    ///
    /// # A zero result is ambiguous
    ///
    /// A returned `0` can mean any of three distinct states, and this
    /// method cannot tell them apart:
    ///
    /// - the archive is empty;
    /// - no entry exposes a CRC32 in its listing (every CRC-less format
    ///   named above), so nothing contributed to the sum;
    /// - the summed CRC32s genuinely wrap to `0` — a valid CRC32 value
    ///   and a valid sum (AD 0012), not a sentinel.
    ///
    /// Treat the result as a cheap fingerprint, not a presence check. To
    /// distinguish "no checksummable content" from a real zero sum, use
    /// [`Archive::calculate_manifest_digest`], which streams payloads for
    /// CRC-less entries and never conflates the empty and zero cases.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("file.zip")?;
    /// let crc = archive.calculate_archive_crc()?;
    /// println!("Archive CRC: {:08X}", crc);
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn calculate_archive_crc(&self) -> Result<u32> {
        let entries = self.list_files()?;

        let mut sum: u32 = 0;
        for entry in entries {
            if let Some(crc32) = entry.crc32 {
                // Arithmetic addition with 32-bit overflow (wrapping)
                sum = sum.wrapping_add(crc32);
            }
        }

        Ok(sum)
    }

    /// Resolve a per-entry CRC32 for digest computation.
    ///
    /// When the archive's metadata already carries a CRC32, use it
    /// directly. Otherwise stream the entry data through a CRC32 hasher —
    /// so the digest stays a pure content-identity hash rather than
    /// leaking path/size into the input. Two listing shapes reach the
    /// streaming arm: the wholly CRC-less formats (TAR and its compression
    /// wrappers, ISO) and, since DCR-012, AE-2 AES ZIP entries, whose
    /// stored CRC32 is the specification's placeholder 0 rather than a
    /// checksum. Streaming an AE-2 entry decrypts it, so it needs the
    /// handle's password.
    ///
    /// This is the per-entry resolver. When the backend offers a
    /// single-traversal walk, `calculate_content_multiset_digest_and_size`
    /// resolves the same values through
    /// [`Self::resolve_crc32_single_pass`] first and only falls back here
    /// for whatever that pass did not cover (OI-0001-009).
    ///
    /// CRC-less entries stream by their stable listing id, not by path
    /// (ti-2a6e3153), so an archive that legitimately repeats a path (a
    /// `tar -rf` append) hashes each occurrence's real payload instead of
    /// re-hashing the first match. Before OI-0076-002 the by-path walk
    /// silently re-hashed the first occurrence for every duplicate (wrong
    /// digest, R0079-0028); OI-0076-002 then routed by-path streaming
    /// through `validate_single_entry`, so duplicate-path CRC-less
    /// archives errored ("Multiple entries match") at the single-entry
    /// gate rather than producing a digest. Seeking by id restores digest
    /// support for them, so each occurrence contributes its own payload's
    /// CRC32 to the content multiset — which, after DCR-012 removed the
    /// per-path occurrence ordinal from the encoding, is the *sole*
    /// mechanism distinguishing shadowed duplicates in the digest. A
    /// future "cleanup" back to by-path resolution would silently
    /// re-hash the first occurrence and make two genuinely different
    /// archives digest equal; `duplicate_path_tar_digests_both_payloads_distinctly`
    /// is the sentinel against that and must not be deleted.
    ///
    /// All three backends that can list a CRC-less file entry implement
    /// the id-based stream: libarchive (TAR family, ISO), the zip crate
    /// (AE-2 AES entries, DCR-012) and sevenz-rust2 (entries written
    /// without the format's *optional* kCRC digest — `entry_crc32` gates
    /// on `has_crc`). The by-path fallback below is therefore unreachable
    /// today; it exists for a future CRC-less backend, and it fails closed
    /// rather than silently — `security::validate_single_entry` rejects
    /// duplicate paths, so such a backend surfaces `OperationBlocked`
    /// instead of aliasing occurrences. Note that a backend which lists
    /// `crc32: None` but forgets to forward
    /// `ReadBackend::extract_to_stream_by_listing_id` lands in exactly that
    /// fallback with no compiler diagnostic, and "this backend always has a
    /// CRC" is the assumption that puts it there: ZIP landed in it between
    /// the DCR-012 listing change and the dispatch being wired, and 7z sat
    /// in it latently until its own forward was added, because both
    /// rustdocs asserted a per-entry CRC32 that the format does not
    /// guarantee.
    fn entry_crc32_for_digest(&self, entry: &ArchiveEntry) -> Result<u32> {
        if let Some(crc) = entry.crc32 {
            return Ok(crc);
        }
        // The caller only reaches here for entries it sourced from
        // `self.list_files()`, so the `ValidatedSource` token is the
        // right shape to express "no need to re-validate" (D3 /
        // R0068-0040). Stream by the entry's stable listing id so
        // duplicate-path CRC-less archives hash each occurrence's real
        // payload instead of erroring at the by-path single-entry gate
        // (ti-2a6e3153).
        //
        // R0076-0091 deliberately bounds the digest hasher at each
        // entry's *declared* size (the wrapper below), not at
        // `max_file_size`, so a large honestly-declared member still
        // digests. Pass unlimited limits to `extract_to_stream_by_id` so
        // its size/ratio arms reproduce the by-path stream's absence of
        // per-entry rejection exactly; that bound remains the sole
        // limit on what the hasher consumes. R6 (ti-c0d6fad6) makes it an
        // *exact* bound where a declaration exists, so the digest surface
        // detects truncation instead of hashing a short payload.
        let reader = match self.validated_source().extract_to_stream_by_id(
            entry,
            &crate::security::ExtractionLimits::unlimited(),
            "calculate_content_multiset_digest",
        ) {
            Ok(reader) => reader,
            // A backend with no id-based stream hits the trait default.
            // Every backend that can list a CRC-less file entry overrides
            // it — libarchive for the TAR family and ISO, the zip crate for
            // AE-2 AES entries (DCR-012), sevenz-rust2 for entries lacking
            // the optional kCRC digest — so this arm is the safety net for
            // a *future* CRC-less backend, not a live path. It is a
            // fail-closed net rather than a silent one: the by-path stream
            // runs `security::validate_single_entry`, which refuses
            // duplicate paths, so such a backend would surface
            // `OperationBlocked` rather than aliasing occurrences. That is
            // exactly what a missing `ReadBackend` forward looked like for
            // ZIP before it was wired, and for 7z before the same forward
            // was added there (see
            // `ReadBackend::extract_to_stream_by_listing_id`).
            Err(ArchiveError::NotImplemented { .. }) => {
                self.validated_source().extract_to_stream(&entry.path)?
            }
            Err(e) => return Err(e),
        };
        crc32_of_bounded_payload(reader, entry.size, &entry.path, &self.path)
    }

    /// Resolve every CRC-less file entry's CRC32 in a **single** traversal
    /// of the archive (OI-0001-009 / ticgit `82bf8fd4`).
    ///
    /// Returns a map from listing id to resolved CRC32. An empty map means
    /// "this backend has no one-pass walk" (or there was nothing to
    /// resolve); the caller then falls back to
    /// [`Self::entry_crc32_for_digest`] per entry, which is correct and
    /// merely quadratic.
    ///
    /// **Why id-keyed, and why it must stay that way.** Targets carry the
    /// stable listing id, and the backend seeks by index and uses the
    /// listing name only as a drift guard. A pass keyed on the path would
    /// re-hash the first occurrence of every repeated path — the
    /// OI-0076-002 aliasing that ti-2a6e3153 removed and that DCR-012 made
    /// load-bearing when it dropped the occurrence ordinal from the
    /// encoding. The map is likewise keyed on the id, not the path.
    ///
    /// **Bail-out on non-unique ids.** Ids are positional listing indices
    /// and unique for every shipped backend. If a backend ever violated
    /// that, one map slot would silently absorb two payloads, so a
    /// duplicate id abandons the one-pass route entirely rather than
    /// producing a digest over a collapsed multiset.
    ///
    /// **Error ordering.** Payload faults (a truncated CRC-less member)
    /// now surface before the aggregation loop's `total_size` overflow
    /// check rather than interleaved with it. Both are still errors and
    /// both still name their cause; only which one an archive that would
    /// trip both reports first has changed.
    fn resolve_crc32_single_pass(&self, entries: &[ArchiveEntry]) -> Result<HashMap<usize, u32>> {
        let targets: Vec<crate::backend::PayloadTarget<'_>> = entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File && e.crc32.is_none())
            .map(|e| crate::backend::PayloadTarget {
                id: e.id,
                validated_path: e.path.as_str(),
                declared_size: e.size,
            })
            .collect();
        if targets.is_empty() {
            return Ok(HashMap::new());
        }
        let distinct_ids: std::collections::HashSet<usize> = targets.iter().map(|t| t.id).collect();
        if distinct_ids.len() != targets.len() {
            return Ok(HashMap::new());
        }

        let mut resolved: HashMap<usize, u32> = HashMap::with_capacity(targets.len());
        let archive_path = self.path.as_path();
        let outcome = {
            let mut visit = |target: &crate::backend::PayloadTarget<'_>,
                             reader: &mut dyn std::io::Read|
             -> Result<()> {
                // Same bound, same verdicts, same diagnostics as the
                // per-entry resolver — one helper, so DCR-011 cannot mean
                // two different things on the two routes.
                let crc = crc32_of_bounded_payload(
                    reader,
                    target.declared_size,
                    target.validated_path,
                    archive_path,
                )?;
                resolved.insert(target.id, crc);
                Ok(())
            };
            crate::backend::dispatch_read_archive(self, "calculate_content_multiset_digest", |b| {
                b.visit_payloads_by_listing_id(&targets, &mut visit)
            })
        };

        match outcome {
            Ok(()) => Ok(resolved),
            // No one-pass walk on this backend: the per-entry resolver
            // still produces the identical digest, just more slowly.
            Err(ArchiveError::NotImplemented { .. }) => Ok(HashMap::new()),
            Err(e) => Err(e),
        }
    }

    /// Calculate the manifest digest for archive identity
    ///
    /// Computes a CRC32-based digest from sorted per-entry CRC32 values.
    /// This is deterministic: same entries (regardless of order in archive)
    /// produce the same digest — unconditionally, duplicate-path archives
    /// included (DCR-012). See
    /// [`Self::calculate_content_multiset_digest_and_size`] for how
    /// CRC-less formats materialise the per-entry CRC32.
    ///
    /// Used by AdvancedDeduplicator for content-identity matching — two archives
    /// with identical file contents produce the same manifest_digest even if
    /// compressed differently or stored in different archive formats.
    ///
    /// Unlike [`Self::calculate_archive_crc`] (wrapping sum — less collision-resistant),
    /// this method sorts individual CRC32 hex representations and hashes the
    /// joined string, preserving per-entry identity.
    ///
    /// Returns empty string if the archive contains no file entries.
    ///
    /// **Naming note.** The historical name "manifest"
    /// suggests the digest covers paths and metadata — it does not.
    /// The digest is a *content multiset* hash: archives with the same
    /// per-entry CRC32 set produce the same digest regardless of
    /// filenames, directory layout, modification times, or
    /// permissions. New callers should prefer
    /// [`Self::calculate_content_multiset_digest_and_size`], which
    /// names the contract correctly and computes the total size in
    /// one pass. This shim is kept for source-compat.
    ///
    /// # Algorithm
    ///
    /// 1. For each file entry, resolve a CRC32:
    ///    - Use `entry.crc32` when the listing carries one (most ZIP, 7z
    ///      and RAR5 entries).
    ///    - Otherwise — the TAR family and ISO always, plus AE-2 AES ZIP
    ///      entries and 7z entries lacking the optional kCRC digest —
    ///      stream the entry through a CRC32 hasher so the digest remains
    ///      a content-identity hash.
    /// 2. Convert each CRC32 to 8-char hex — one element per file entry,
    ///    so repeated contents appear as repeated elements
    /// 3. Sort lexicographically
    /// 4. Join with ","
    /// 5. CRC32-hash the joined string
    /// 6. Return as 8-char lowercase hex
    ///
    /// # Password required for AE-2 AES ZIP entries
    ///
    /// Inherited from
    /// [`Self::calculate_content_multiset_digest_and_size`]: AE-2 entries
    /// list no CRC32 (DCR-012), so their payloads are decrypted and read,
    /// and a handle opened without a password errors instead of returning
    /// a digest. See that method for the full note, including the
    /// still-mislabelled error variant (ticgit `9bdf2c`).
    ///
    /// # Performance
    ///
    /// On CRC-less formats (TAR, TAR+gz/bz2/xz, ISO) each file entry is
    /// decompressed to compute its CRC32. Since OI-0001-009 (ticgit
    /// `82bf8fd4`) the libarchive backend resolves them in a single
    /// traversal instead of re-opening the archive per entry, so a
    /// compressed TAR is decompressed once rather than once per member —
    /// still use [`Self::calculate_archive_crc`] when a cheaper summary
    /// suffices, since every payload is read either way.
    ///
    /// # Truncated entries (R6 / DCR-011)
    ///
    /// Inherited from
    /// [`Self::calculate_content_multiset_digest_and_size`]: a CRC-less
    /// entry whose payload does not match its declared size returns
    /// [`ArchiveError::Corruption`]
    /// rather than a digest computed over the short payload.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("file.zip")?;
    /// let digest = archive.calculate_manifest_digest()?;
    /// if !digest.is_empty() {
    ///     println!("Manifest digest: {}", digest);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn calculate_manifest_digest(&self) -> Result<String> {
        let (digest, _) = self.calculate_content_multiset_digest_and_size()?;
        Ok(digest)
    }

    /// Calculate content-multiset digest and total uncompressed file size in
    /// a single pass over the entry list (R0070-0089).
    ///
    /// The previous `calculate_manifest_summary` implementation called
    /// `calculate_manifest_digest` separately, which re-walked the
    /// listing and re-computed CRC32 for every entry on CRC-less
    /// formats. Folding the two passes into one halves the cost on
    /// TAR/ISO-like backends where CRC32 is materialised by streaming
    /// the entry payload.
    ///
    /// `digest` represents content identity, not archive layout —
    /// archives with the same files but different names / directory
    /// structures produce identical digests, with no remaining
    /// exception (DCR-012). The naming
    /// "manifest digest" is misleading because no path data
    /// participates; the new helper name (`content_multiset_digest`)
    /// makes that explicit (R0070-0088). The legacy
    /// `calculate_manifest_digest` / `calculate_manifest_summary`
    /// methods are kept as thin shims for source-compat.
    ///
    /// # What the digest is a function of
    ///
    /// The sorted multiset of per-file-entry CRC32 values, and nothing
    /// else. Not paths, not directory layout, not entry order, not
    /// timestamps or permissions, not the compression method, and not
    /// which entries happen to share a name. Non-file entries
    /// (directories, symlinks, hard links) do not participate at all;
    /// an archive with no file entries returns the empty string.
    /// Collision resistance is CRC32-grade — the final fold is 32 bits
    /// wide, so this is an identity hint, not a cryptographic identity
    /// (AD-0047 amendment (a)).
    ///
    /// The digest is defined over the archive's **listing** multiset,
    /// not over what an extraction would materialise on disk. A
    /// shadowed duplicate contributes an element even though unpacking
    /// materialises only the surviving occurrence, and `total_size`
    /// likewise sums every occurrence. A caller who needs "the same
    /// files in the same places", or "the same files after extraction",
    /// must not use this method: it does not answer those questions and
    /// never did.
    ///
    /// # Duplicate entry paths
    ///
    /// Archives may legitimately repeat a path (TAR append/update
    /// semantics; ZIP/7z central directories permit shadowed entries).
    /// Each occurrence contributes one ordinary element — no occurrence
    /// ordinal, no positional suffix, nothing derived from the path. Two
    /// archives holding the same multiset of file contents therefore
    /// digest identically whether the duplicates share one path or
    /// occupy distinct paths (DCR-012 / OI-0001-008).
    ///
    /// Multiplicity is still significant: `n` copies of one payload
    /// contribute `n` elements, so a listing of `a.txt` alone and a
    /// listing of `a.txt` + `b.txt` with identical contents do *not*
    /// collide.
    ///
    /// On CRC-less formats (TAR family, ISO) the per-entry CRC32 is
    /// resolved by streaming each entry *by its stable listing id*
    /// (ti-2a6e3153), so every occurrence hashes its own payload and two
    /// duplicate-path archives that differ only in a shadowed payload
    /// digest differently. With the ordinal gone that id-keyed
    /// resolution is the only thing standing between those two archives:
    /// nothing in the *encoding* distinguishes them any more. Reverting
    /// the resolver to a by-path walk would produce a silent false
    /// positive.
    ///
    /// # Encoding change in 0.4.0 (DCR-012)
    ///
    /// Duplicate-path archives previously carried a zero-based per-path
    /// occurrence ordinal in the digest input (`<crc32-hex>#<ordinal>`,
    /// R0079-0028); their digests changed value once when it was
    /// removed. Unique-path archives are unaffected — ordinal 0 already
    /// emitted the bare hex, so their digests are byte-identical. The
    /// ordinal was a compensating control over the by-path CRC-less
    /// resolver that ticgit `2a6e3153` replaced; once resolution became
    /// id-keyed the ordinal encoded archive layout and nothing else,
    /// contradicting this method's own contract.
    ///
    /// The bump is a *minor* one despite no signature changing: a
    /// public-API diff against 0.3.1 shows no added, removed or altered
    /// `pub` item. What changed is the value this method returns, which
    /// callers compare against stored results, so code that still
    /// compiles no longer still agrees.
    ///
    /// # Password required for AE-2 AES ZIP entries
    ///
    /// **Behaviour change (DCR-012).** An AE-2 AES entry stores the
    /// specification's placeholder 0 in its CRC32 field rather than a
    /// checksum, so the listing reports `crc32: None` and this method
    /// resolves the value by decrypting and reading the payload. That
    /// needs the password.
    ///
    /// Consequently, on an archive holding AE-2 entries this method — and
    /// [`Self::calculate_manifest_digest`] and
    /// [`Self::calculate_manifest_summary`], which delegate to it —
    /// **requires a handle opened with
    /// [`Archive::open_encrypted`](crate::Archive::open_encrypted)**.
    /// Called on a password-less handle it returns an error instead of the
    /// `Ok` it previously produced. It previously produced that `Ok` by
    /// folding the placeholder 0 into the digest for every AE-2 entry,
    /// which made two AES ZIPs with entirely different contents collide —
    /// so the old success was wrong, not merely cheaper.
    ///
    /// Which error is raised is not yet what it should be: the ZIP
    /// no-password read path classifies the failure as
    /// [`ArchiveError::Format`] carrying a "Password required to decrypt
    /// file" message rather than [`ArchiveError::Password`]. That
    /// mislabel predates this change and is tracked as ticgit `9bdf2c`;
    /// do not pattern-match on `Format` to detect a missing password.
    ///
    /// Plaintext, ZipCrypto and AE-1 entries are unaffected — their
    /// listings carry a real CRC32 and never stream here.
    ///
    /// # Performance
    ///
    /// Entries that carry a per-entry CRC32 in their metadata cost one
    /// listing walk and no payload reads at all. That is every RAR5 file
    /// entry, every ZIP entry except the AE-2 AES case below, and every 7z
    /// entry that was written with the format's *optional* kCRC digest.
    ///
    /// On CRC-less formats (TAR family, ISO) every file entry's payload is
    /// decoded to materialise its CRC32. Since OI-0001-009 (ticgit
    /// `82bf8fd4`) the libarchive backend resolves all of them in a
    /// **single traversal** rather than re-opening the archive once per
    /// entry, so the cost is linear in the archive's bytes: a compressed
    /// TAR is decompressed once, not once per member. Prefer
    /// [`Self::calculate_archive_crc`] when a cheap summary suffices —
    /// this call still reads every payload, and still reports no progress.
    ///
    /// Two cases make a nominally CRC-carrying format pay payload cost.
    /// An AE-2 AES ZIP entry has no checksum to read, so each is decrypted
    /// and read in full (and, per the section above, a password is
    /// required); a 7z entry written without the optional kCRC digest is
    /// read in full for the same reason. Neither is resolved by a
    /// sequential walk — ZIP seeks into the indexed central directory, and
    /// sevenz-rust2 seeks by index into the TOC it materialized on open.
    ///
    /// # Truncated entries (R6 / DCR-011)
    ///
    /// On CRC-less formats the payload stream used to resolve a per-entry
    /// CRC32 is held to the listing's declared size **exactly**. An entry
    /// whose payload ends before its declaration — the shape a truncated
    /// TAR/CPIO/ISO member takes, where no per-entry checksum exists to
    /// catch it — now returns
    /// [`ArchiveError::Corruption`]
    /// naming that entry, instead of digesting the short payload and
    /// reporting success. Over-production past the declaration returns the
    /// same error. The digest surface is therefore a truncation detector on
    /// the TAR family and ISO, not only a content hash.
    ///
    /// Entries whose listing carries no size (raw gzip/bzip2/xz single-file
    /// readers) get no invented declaration: they keep a ceiling-only bound
    /// at the crate-default per-file limit, and an early end of stream stays
    /// an ordinary EOF. An entry whose listing *does* carry a CRC32 never
    /// streams here at all — which is most ZIP, 7z and RAR5 entries, but
    /// not the AE-2 AES ZIP or kCRC-less 7z exceptions named above.
    ///
    /// # The size term is typed, and may be incomplete (OI-0001-007)
    ///
    /// **Breaking change in 0.5.0.** The second element was a bare `u64`
    /// and is now a [`SizedContentTotal`]. It always was a partial sum —
    /// an entry whose listing declares no size contributed nothing, and
    /// the `u64` had no way to say so. The two populations that hit this
    /// are named in the section above: raw single-file `.gz`/`.bz2`/`.xz`
    /// members, and libarchive entries with an unset size field
    /// (R0001-0014). An all-`.gz` handle therefore returned a confident
    /// `0`.
    ///
    /// Migration is mechanical and the compiler finds every site:
    /// `total.sized_bytes()` restores the old number exactly, and
    /// `total.exact()` is the one to reach for when the caller was
    /// treating it as the archive's true size. See
    /// [`SizedContentTotal`] for why this is a struct rather than an
    /// `Option<u64>` (which would discard the partial sum) or a tuple.
    ///
    /// The digest is unaffected: unsized entries are hashed like any
    /// other, so an incomplete total sits beside a complete digest.
    pub fn calculate_content_multiset_digest_and_size(
        &self,
    ) -> Result<(String, SizedContentTotal)> {
        let entries = self.list_files()?;
        // OI-0001-009: resolve every CRC-less entry in one traversal where
        // the backend supports it. `resolved` is keyed on the stable
        // listing id, never the path, so duplicate-path occurrences stay
        // distinct. An empty map means the backend has no one-pass walk;
        // the per-entry resolver below then produces the identical digest.
        let resolved = self.resolve_crc32_single_pass(entries)?;
        content_multiset_digest_and_size(entries, |entry| match entry.crc32 {
            Some(crc) => Ok(crc),
            None => match resolved.get(&entry.id) {
                Some(crc) => Ok(*crc),
                None => self.entry_crc32_for_digest(entry),
            },
        })
    }

    /// Calculate content-multiset digest and total uncompressed file size.
    ///
    /// Thin shim over [`Self::calculate_content_multiset_digest_and_size`]
    /// kept for source-compat. New callers should prefer the typed
    /// helper directly (R0070-0088 / R0070-0089).
    ///
    /// # Why this shim moved with the primary (OI-0001-007)
    ///
    /// The shim exists for source-compat, so keeping its historical
    /// `(String, u64)` shape was the obvious call and is the wrong one.
    /// The only `u64` it could return is `SizedContentTotal::sized_bytes`
    /// — the partial sum — which means source-compat here would preserve
    /// a number that is silently short on raw `.gz`/`.bz2`/`.xz` handles
    /// and on libarchive entries with an unset size field, in a method
    /// whose own name promises a *summary*. Compatibility that keeps code
    /// compiling while it keeps producing the wrong figure is worse than
    /// the compile error: the error is found once, the figure is found by
    /// a user. So the shim returns [`SizedContentTotal`] too, and the
    /// 0.5.0 breaking window pays for it. Callers that genuinely want the
    /// old number say `.sized_bytes()` and have said so on purpose.
    ///
    /// # Performance
    ///
    /// Inherited from the underlying helper, and the reason this is not a
    /// metadata-only call: on formats without per-entry CRC32 in their
    /// metadata every payload is decompressed to compute one. Since
    /// OI-0001-009 the libarchive backend resolves all of them in a single
    /// traversal, so a compressed TAR is decompressed once rather than once
    /// per member — linear in the archive's bytes, not quadratic in its
    /// entry count. It is still a full read. When a cheap summary is all
    /// that is wanted, [`Self::calculate_archive_crc`] reads listing
    /// metadata only. See
    /// [`Self::calculate_content_multiset_digest_and_size`] for the full
    /// note.
    ///
    /// # Password required for AE-2 AES ZIP entries
    ///
    /// Inherited from the underlying helper: AE-2 entries list no CRC32
    /// (DCR-012), so their payloads are decrypted and read, and a handle
    /// opened without a password errors instead of returning a summary.
    /// See [`Self::calculate_content_multiset_digest_and_size`] for the
    /// full note.
    ///
    /// # Truncated entries (R6 / DCR-011)
    ///
    /// Inherited from the underlying helper: a CRC-less entry whose payload
    /// does not match its declared size returns
    /// [`ArchiveError::Corruption`]
    /// instead of a summary computed over the short payload.
    pub fn calculate_manifest_summary(&self) -> Result<(String, SizedContentTotal)> {
        self.calculate_content_multiset_digest_and_size()
    }

    /// Detect if this archive is part of a multi-part archive set (FR-019)
    ///
    /// Multi-part archives split data across multiple files. End-to-end
    /// split-volume support currently covers RAR/RAR5 archives only.
    /// ZIP split-volume names may be discovered heuristically, but ZIP split
    /// extraction is not supported end-to-end. 7z numeric split volumes are
    /// not supported.
    ///
    /// Recognized naming schemes: ZIP split sets (`x.zip` + `x.zNN`),
    /// new-style RAR volumes (`x.partN.rar`), old-style RAR volumes
    /// (`x.rar` + `x.rNN`, rolling into `x.sNN` after `.r99`;
    /// R0079-0029), and pure numeric suffixes (`x.001`, `x.002`, ...).
    /// Sibling volumes are matched by file name only — their contents
    /// are never opened.
    ///
    /// Name classification, grouping and volume order are delegated to
    /// [`crate::format::multipart`], and the set is *anchored* on the
    /// member that was opened. Every member of one set therefore reports
    /// the same list: opening `x.rar`, `x.r00` or `x.r01` of an old-style
    /// set all return the whole series (OI-0001-006), and a directory
    /// holding a second, larger set cannot capture this handle.
    ///
    /// Returns `(is_multipart, part_files)` where:
    /// - `is_multipart`: true if this archive is part of a multi-part set
    /// - `part_files`: list of all detected part files in the set (sorted)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("backup.part1.rar")?;
    /// let (is_multipart, parts) = archive.detect_multipart()?;
    /// if is_multipart {
    ///     println!("Found {} parts: {:?}", parts.len(), parts);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)> {
        // Adapt the typed report to the legacy tuple. An `Unvolumed` report
        // is not a set, so it returns the same single-element shape as the
        // non-multipart gate rather than the report's `paths`, which are
        // every candidate in the directory. Defects — holes, duplicates,
        // foreign siblings — are dropped here because this tuple has nowhere
        // to put them; [`Self::volume_set_report`] is where they survive.
        let report = self.volume_set_report()?;
        let parts = match report.set() {
            Some(set) => set.paths(),
            None => vec![self.path.clone()],
        };
        Ok((parts.len() > 1, parts))
    }

    /// The typed volume-set report for this archive: which files form its
    /// set, in what order, and **what is wrong with it**.
    ///
    /// This is the answer [`Self::detect_multipart`] and
    /// [`Self::multipart_layout`] cannot give. Both reduce the set to a list
    /// of paths, which can say "these three files" but not "and a fourth is
    /// missing between them". A hole, a duplicated volume number and a
    /// foreign sibling all arrive here as [`crate::format::multipart::VolumeSetDefect`]s; through the
    /// other two they arrive as a shorter list, or as no signal at all.
    ///
    /// Nothing is recomputed to provide this. The parser already produced a
    /// report on every `detect_multipart` call and the tuple simply discarded
    /// it — which is why this is an additive method rather than a change to
    /// either existing shape. [`MultipartLayout`] is deliberately left alone:
    /// widening it would be a breaking change to a type callers match on, and
    /// a caller who does not care about defects should not have to.
    ///
    /// # Errors
    ///
    /// Write-mode handles are refused: multipart discovery would walk the
    /// destination directory for siblings of an archive still being written
    /// and report filesystem noise as a set. An unreadable parent directory
    /// surfaces as [`ArchiveError::Io`] rather than as "not multipart", so a
    /// permission problem is distinguishable from an honest single-volume
    /// answer.
    ///
    /// # Formats that cannot be multi-volume
    ///
    /// A format whose `supports_multipart()` is false reports
    /// [`crate::format::multipart::VolumeSetReport::Unvolumed`] without scanning the directory at all.
    /// That includes 7z numeric splits today: the parser understands
    /// `VolumeScheme::Numeric`, but routing 7z through it is a capability
    /// decision that has not been taken (R0080-0093).
    pub fn volume_set_report(&self) -> Result<crate::format::multipart::VolumeSetReport> {
        use crate::format::multipart::VolumeSetReport;
        use std::fs;

        let unvolumed = || VolumeSetReport::Unvolumed {
            paths: vec![self.path.clone()],
        };

        // R0071-0014: write-mode handles describe an archive that is
        // still being assembled — multipart sibling discovery would
        // walk the destination directory looking for `archive.part2`
        // siblings of an in-progress output and return misleading
        // filesystem-derived parts. Reject the call up front.
        if self.mode == crate::archive::ArchiveMode::Write {
            return Err(ArchiveError::write_mode_only("detect_multipart"));
        }

        // Non-UTF-8 path note (AD 0064 / R0075-0082): the source name and
        // every candidate name flow through to_string_lossy() before the
        // parser sees them — the anchor in `parse_volume_set_for`, the
        // candidates in its `build_report`. Because the source path and
        // its siblings undergo the *same* substitution (U+FFFD for
        // invalid sequences), base-name matching still works correctly
        // for the common multi-volume case where names share the same
        // byte sequence except for volume-number suffixes. The edge case
        // where two byte-different non-UTF-8 base names coalesce into the
        // same lossy form is documented as a v0.4 follow-up
        // (OI-0075-004 R0075-0083 typed multipart return shape).

        // Documentation note (R0070-0090): for non-multipart formats
        // the result is `(false, [self.path])` rather than
        // `(false, [])`. The single-element list is intentional and
        // documented — callers can still match on `is_multipart` to
        // distinguish "not a multi-volume set" from "set with a
        // single part" since `is_multipart=false` is unambiguous.
        // A future version may switch to an empty list once
        // downstream consumers are audited; for now the existing
        // shape is preserved to keep the test suite stable.
        //
        // This gate is also what keeps 7z numeric splits away from the
        // parser: `SevenZip::supports_multipart()` is false, so a 7z
        // `.001` set exits here and is never grouped. Routing it is a
        // capability decision tracked separately (R0080-0093); the
        // parser already understands `VolumeScheme::Numeric`, so when
        // that decision is taken it is a capability flip plus its own
        // fixtures, not a change to this body.
        if !self.format.supports_multipart() {
            return Ok(unvolumed());
        }

        let file_name = self
            .path
            .file_name()
            .ok_or_else(|| {
                ArchiveError::invalid_path(self.path.display().to_string(), "No filename")
            })?
            .to_string_lossy();

        let parent_dir = sibling_scan_dir(&self.path);

        // Collect directory entries once. Propagate I/O failures so a
        // permission error or missing directory is distinguishable from
        // "this archive is not multipart"; previously `.ok().unwrap_or_default()`
        // hid every failure behind a silent empty-list. The scan stays
        // ahead of the anchor check below so an unreadable parent surfaces
        // as `ArchiveError::Io` whatever the source archive is named
        // (R0068-0082).
        let dir_entries: Vec<PathBuf> = fs::read_dir(parent_dir)
            .map_err(|e| ArchiveError::io("read_dir", parent_dir.to_path_buf(), e))?
            .map(|entry| {
                let entry = entry
                    .map_err(|e| ArchiveError::io("read_dir_entry", parent_dir.to_path_buf(), e))?;
                let path = entry.path();
                let file_type = entry
                    .file_type()
                    .map_err(|e| ArchiveError::io("file_type", path.clone(), e))?;
                Ok((file_type.is_file() && path != self.path).then_some(path))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();

        // A source whose own name parses as no volume name belongs to no
        // set, and must say so before the parser is consulted:
        // `parse_volume_set_for` falls back to the unanchored
        // largest-group rule when the anchor does not parse, which would
        // bind this handle to whichever *unrelated* set happens to be the
        // biggest in the same directory.
        if parse_volume_name(&file_name).is_none() {
            return Ok(unvolumed());
        }

        // Everything below — which names are volumes, which of them belong
        // to this archive's set, and in what order — is the typed parser's
        // (OI-0080-004). It matches ASCII-case-insensitively while
        // returning the original-case paths (R0080-0087), anchors
        // `.part<digits>.rar` at the terminal suffix (R0080-0088 /
        // R0080-0095), and numbers volumes so the MADR-0013 order falls
        // out of one sort: unnumbered main first, then ascending, `.sNN`
        // after `.rNN` (R0079-0029). Anchoring on the member that was
        // opened is what makes `x.rar`, `x.r00` and `x.r01` of one
        // old-style set report the same list (OI-0001-006).
        let mut candidates = Vec::with_capacity(dir_entries.len() + 1);
        candidates.push(self.path.clone());
        candidates.extend(dir_entries);
        Ok(parse_volume_set_for(&self.path, &candidates))
    }

    /// Detect this archive's multipart layout, returning a typed
    /// [`MultipartLayout`] (R0075-0083).
    ///
    /// Replaces the historical `(bool, Vec<PathBuf>)` shape from
    /// [`Self::detect_multipart`]. Single-part archives surface as
    /// [`MultipartLayout::Single { path }`](MultipartLayout::Single)
    /// with `path` equal to the source archive; multipart archives
    /// surface as [`MultipartLayout::Multi { parts }`](MultipartLayout::Multi)
    /// with all detected volumes in volume order.
    ///
    /// New code should prefer this method. `detect_multipart` is kept
    /// non-deprecated for v0.3 to avoid migration noise across
    /// existing call sites; v0.4 will deprecate it formally.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::{Archive, MultipartLayout};
    ///
    /// let archive = Archive::open("backup.part1.rar")?;
    /// match archive.multipart_layout()? {
    ///     MultipartLayout::Single { path } => {
    ///         println!("Single archive at {}", path.display());
    ///     }
    ///     MultipartLayout::Multi { parts } => {
    ///         println!("Multipart set with {} volumes", parts.len());
    ///     }
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn multipart_layout(&self) -> Result<MultipartLayout> {
        let (is_multi, parts) = self.detect_multipart()?;
        if is_multi {
            Ok(MultipartLayout::Multi { parts })
        } else {
            // detect_multipart's documented contract for non-multipart
            // archives is `(false, vec![self.path.clone()])`. Carry
            // that path through verbatim so the typed shape preserves
            // identity with the legacy return.
            let path = parts
                .into_iter()
                .next()
                .unwrap_or_else(|| self.path().to_path_buf());
            Ok(MultipartLayout::Single { path })
        }
    }

    /// Check for symlinks in archive and return warnings (FR-022)
    ///
    /// Scans entries for symlink and hard link types. All backends now classify
    /// link entries: libarchive reads link types from archive metadata,
    /// ZipReader uses `is_symlink()`, SevenZ inspects
    /// `windows_attributes` (Unix mode + reparse point), and UnRAR uses `redir_type`
    /// for symlinks, junctions, and hard links.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use unified_archive::Archive;
    ///
    /// let archive = Archive::open("data.tar.gz")?;
    /// let warnings = archive.check_symlinks()?;
    /// for warning in &warnings {
    ///     eprintln!("Warning: {}", warning);
    /// }
    /// # Ok::<(), unified_archive::ArchiveError>(())
    /// ```
    pub fn check_symlinks(&self) -> Result<Vec<ArchiveWarning>> {
        let entries = self.list_files()?;
        let mut warnings = Vec::new();

        for entry in entries {
            match entry.entry_type {
                EntryType::Symlink => {
                    // R0070-0059: surface backend-populated link targets
                    // through `ArchiveWarning::SkippedSymlink.target` when
                    // available. Backends that surface link targets during
                    // listing populate `entry.link_target`; backends that
                    // don't (today most of them) leave `None` and the
                    // warning preserves the legacy "target unknown" shape.
                    warnings.push(ArchiveWarning::SkippedSymlink {
                        path: entry.path.clone(),
                        target: entry.link_target.clone(),
                    });
                }
                EntryType::HardLink => {
                    warnings.push(ArchiveWarning::SkippedHardLink {
                        path: entry.path.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(warnings)
    }
}

/// Hash one CRC-less entry's payload under the DCR-011 size contract.
///
/// R0076-0091: bound what the hasher consumes. A hostile or buggy decoder
/// must not stream unbounded bytes into the digest, so the payload is
/// capped at the listing's declared size — or, when the listing declares
/// none, at the crate-default per-file ceiling.
///
/// R6 (ti-c0d6fad6 / DCR-011): where a declaration exists the bound is
/// *exact*, not ceiling-only. A ceiling alone catches over-emission but
/// lets a truncated CRC-less entry digest its short payload and report
/// success — the same defect class as OI-0001-001 on `extract_to_stream`,
/// surfacing through a different public API. An unknown-size entry gets no
/// invented declaration (hard constraint 3): it keeps the ceiling-only cap,
/// where an early EOF is an ordinary EOF.
///
/// The bound's verdicts arrive as `io::Error`s and
/// [`crate::ffi::common::compute_crc32_reader`] wraps anything that is not
/// a decoder checksum message as [`ArchiveError::Io`]. They are
/// re-classified here — at the one place both digest resolution routes
/// share — so a declared-size violation reads as the
/// [`ArchiveError::Corruption`] every other declared-size violation in the
/// crate produces, naming the entry. Deliberately not done inside
/// `map_entry_read_error`: that helper is on every backend's read path,
/// and reclassifying every `UnexpectedEof` crate-wide is a far larger
/// blast radius than this defect warrants. Keying on the error kind is
/// safe here because the only reader in play is the crate's own capped
/// stream.
///
/// The reclassification is scoped to the declared branch. An unknown-size
/// entry has no declaration to violate — what it can breach is the
/// crate-default per-file ceiling, a policy limit rather than archive
/// damage. Rewrapping that as `Corruption` would both mislabel the failure
/// and invent the declaration this path exists to avoid inventing.
fn crc32_of_bounded_payload<R: std::io::Read>(
    reader: R,
    declared_size: Option<u64>,
    entry_path: &str,
    archive_path: &Path,
) -> Result<u32> {
    let mut bounded = match declared_size {
        Some(declared) => crate::streaming::HardCapReader::exact(reader, declared),
        None => {
            crate::streaming::HardCapReader::ceiling(reader, crate::security::DEFAULT_MAX_FILE_SIZE)
        }
    };
    crate::ffi::common::compute_crc32_reader(&mut bounded, archive_path).map_err(|e| match e {
        ArchiveError::Io { ref source, .. }
            if declared_size.is_some()
                && matches!(
                    source.kind(),
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::InvalidData
                ) =>
        {
            ArchiveError::Corruption {
                path: entry_path.to_string(),
                details: format!("digest stream violated the declared size: {}", source),
            }
        }
        other => other,
    })
}

/// Encode one element of the content-multiset digest input.
///
/// Always exactly eight lowercase zero-padded hex characters, for every
/// file entry — first occurrence or twentieth, unique path or shadowed
/// duplicate. No path, name, size, listing id or occurrence index
/// participates.
///
/// **Injectivity.** Elements are fixed-width over `[0-9a-f]` and the
/// join separator `,` is outside that alphabet, so `join(",")` is
/// uniquely decodable and the caller's `sort()` canonicalises the
/// order. The joined string is therefore a bijection with the sorted
/// CRC32 multiset, and the only residual collisions are the final
/// 32-bit fold's (AD-0047 amendment (a)). A future "simplification"
/// that widened the element alphabet, or made the width variable,
/// would break that property.
///
/// **Encoding change in 0.4.0 (DCR-012).** R0079-0028 appended a zero-based
/// per-path occurrence ordinal (`<hex>#<ordinal>`) from the second
/// occurrence of a repeated path on. That ordinal was a compensating
/// control over a CRC-less resolver that walked *by path* and re-hashed
/// the first occurrence for every duplicate; ti-2a6e3153 replaced that
/// resolver with an id-keyed one, leaving the ordinal encoding nothing
/// but archive *layout* — which this digest exists not to carry. It is
/// gone. Because ordinal 0 already emitted the bare hex, every
/// unique-path archive's digest is byte-identical across the change.
fn content_digest_element(crc32: u32) -> String {
    format!("{crc32:08x}")
}

/// Directory scanned for sibling volumes of `path`.
///
/// R0001-0047: `Path::parent()` yields `Some("")` — an *empty* path,
/// not `None` — for a one-component relative name like `archive.rar`,
/// so a bare `unwrap_or(".")` never fires and `read_dir("")` fails
/// with a spurious `NotFound`, turning a common relative path into an
/// I/O error instead of a multipart result. Filter the empty parent
/// explicitly, matching `ffi::common::sync_parent_dir`.
fn sibling_scan_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Digest/size aggregation shared by
/// [`Archive::calculate_content_multiset_digest_and_size`] and its
/// regression tests. `crc32_for` resolves each file entry's CRC32 —
/// listing metadata when present, payload streaming otherwise.
///
/// Entries are consumed in listing order *only* to resolve CRC32s (the
/// CRC-less resolver seeks forward by listing id). Every trace of that
/// order is then removed by the sort, so the returned digest is a
/// function of the resolved CRC32 multiset alone — see
/// [`content_digest_element`] for why the encoding is injective over
/// that multiset (DCR-012).
///
/// The size term is a [`SizedContentTotal`] rather than a `u64` because
/// the accumulation is conditional on `entry.size` being declared: this
/// is the one place that knows how many entries were skipped, so it is
/// the one place that can report it (OI-0001-007).
fn content_multiset_digest_and_size<F>(
    entries: &[ArchiveEntry],
    mut crc32_for: F,
) -> Result<(String, SizedContentTotal)>
where
    F: FnMut(&ArchiveEntry) -> Result<u32>,
{
    let mut hashes: Vec<String> = Vec::new();
    let mut total = SizedContentTotal {
        sized_bytes: 0,
        sized_entries: 0,
        unsized_entries: 0,
    };
    for entry in entries {
        if entry.entry_type != EntryType::File {
            continue;
        }
        // OI-0001-007: the two arms are counted, not just the first. The
        // `None` arm used to fall through silently, so an archive of raw
        // `.gz` members returned a confident 0 — the accumulator and the
        // coverage counters are updated in the same place precisely so a
        // future edit cannot reintroduce that gap by touching one and not
        // the other.
        match entry.size {
            Some(size) => {
                // Keep the "exact total" contract honest: a saturating add
                // would silently report `u64::MAX` as if precise. Surface the
                // overflow instead (R0081-0081).
                total.sized_bytes = total.sized_bytes.checked_add(size).ok_or_else(|| {
                    ArchiveError::operation_blocked(
                        "calculate_content_multiset_digest",
                        "total uncompressed size exceeds u64::MAX",
                    )
                })?;
                total.sized_entries += 1;
            }
            None => total.unsized_entries += 1,
        }
        // One element per file entry, with no occurrence suffix:
        // multiplicity is carried by *repetition* in this vector, so `n`
        // copies of one payload contribute `n` identical elements and a
        // two-copy archive still differs from a one-copy archive. Do not
        // "simplify" this into a set (DCR-012).
        hashes.push(content_digest_element(crc32_for(entry)?));
    }

    let digest = if hashes.is_empty() {
        String::new()
    } else {
        hashes.sort();
        let joined = hashes.join(",");
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(joined.as_bytes());
        format!("{:08x}", hasher.finalize())
    };

    Ok((digest, total))
}

#[cfg(test)]
mod tests;

/// R0001-0047 regression cover for [`sibling_scan_dir`]. Lives inline
/// rather than in `src/inspection/tests.rs` so the pure-function check
/// sits next to the helper it pins; the archive-level `detect_multipart`
/// cases stay in the sibling test module.
#[cfg(test)]
mod sibling_scan_dir_tests {
    use super::sibling_scan_dir;
    use std::path::Path;

    #[test]
    fn bare_relative_name_scans_current_dir() {
        // `Path::new("archive.rar").parent()` is `Some("")`, so the
        // pre-fix `unwrap_or(".")` handed `read_dir` an empty path.
        assert_eq!(Path::new("archive.rar").parent(), Some(Path::new("")));
        assert_eq!(sibling_scan_dir(Path::new("archive.rar")), Path::new("."));
    }

    #[test]
    fn nested_relative_name_keeps_its_parent() {
        assert_eq!(
            sibling_scan_dir(Path::new("sub/archive.part1.rar")),
            Path::new("sub")
        );
    }

    #[test]
    fn absolute_name_keeps_its_parent() {
        let path = Path::new("/data/vol/archive.z01");
        assert_eq!(sibling_scan_dir(path), Path::new("/data/vol"));
    }

    #[test]
    fn root_relative_name_keeps_the_root() {
        // A path whose only component is the root still has a
        // non-empty parent and must not be rewritten to `.`.
        assert_eq!(sibling_scan_dir(Path::new("/archive.zip")), Path::new("/"));
    }
}

/// DCR-012 / OI-0001-008 cover for the content-multiset digest encoding.
///
/// Lives inline rather than in `src/inspection/tests.rs` so the pinned
/// golden values sit next to [`content_digest_element`] and
/// [`content_multiset_digest_and_size`], the two private functions they
/// constrain. Every literal below was computed from the real algorithm
/// (CRC32 of the sorted, comma-joined element string) and is here to
/// catch a *value* regression that a purely relational test would miss —
/// notably a reintroduced occurrence ordinal under some different
/// keying.
#[cfg(test)]
mod content_digest_encoding_tests {
    use super::{ArchiveEntry, Result, content_digest_element, content_multiset_digest_and_size};

    fn synthetic_file(path: &str, id: usize, size: u64) -> ArchiveEntry {
        ArchiveEntry::file(path, id).size(size).build()
    }

    /// Every element is exactly eight lowercase hex characters and
    /// contains neither the old `#` ordinal marker nor the `,`
    /// separator. Fixed width plus separator-exclusion is precisely what
    /// makes `join(",")` uniquely decodable, so this pins the
    /// injectivity argument in `content_digest_element`'s rustdoc rather
    /// than merely a format string.
    #[test]
    fn test_content_digest_element_is_fixed_width_lowercase_hex() {
        for (input, expected) in [
            (0u32, "00000000"),
            (0xff, "000000ff"),
            (0x1234_5678, "12345678"),
            (u32::MAX, "ffffffff"),
        ] {
            let element = content_digest_element(input);
            assert_eq!(element, expected);
            assert_eq!(element.len(), 8, "elements must be fixed-width");
            assert!(!element.contains('#'), "the R0079-0028 ordinal is gone");
            assert!(
                !element.contains(','),
                "the separator must stay outside the alphabet"
            );
            assert!(
                element
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            );
        }
    }

    /// The OI-0001-008 headline case. Two listings with the same content
    /// multiset — one repeating a path (TAR append semantics), the other
    /// spreading the same payloads over distinct paths — must now digest
    /// **identically**.
    ///
    /// This replaces `test_content_digest_duplicate_path_differs_from_unique_paths`,
    /// whose assertion was itself wrong. That test's stated model — "the
    /// constant resolver models the CRC-less streaming path, where every
    /// occurrence of a duplicated path resolves to the first
    /// occurrence's payload" — described the by-path resolver that
    /// ticgit `2a6e3153` deleted on 2026-07-19. Against the id-keyed
    /// resolver a constant CRC no longer models an aliasing bug; it
    /// models a genuine `{X, X}` content multiset, which under a
    /// content-identity contract must equal the spread listing's
    /// (DCR-012).
    #[test]
    fn test_content_digest_grouped_and_spread_duplicates_agree() {
        let grouped = vec![
            synthetic_file("data.txt", 0, 4),
            synthetic_file("data.txt", 1, 4),
        ];
        let spread = vec![
            synthetic_file("data.txt", 0, 4),
            synthetic_file("other.txt", 1, 4),
        ];
        let constant_crc = |_: &ArchiveEntry| -> Result<u32> { Ok(0xDEAD_BEEF) };

        let (grouped_digest, grouped_size) =
            content_multiset_digest_and_size(&grouped, constant_crc).unwrap();
        let (spread_digest, spread_size) =
            content_multiset_digest_and_size(&spread, constant_crc).unwrap();

        assert_eq!(
            grouped_size, spread_size,
            "sizes are identical by construction"
        );
        assert_eq!(
            grouped_digest, spread_digest,
            "duplicate-path grouping is layout, not content, and must not move the digest"
        );
    }

    /// Golden values for the class this change moves. A relational test
    /// alone would still pass if an ordinal were reintroduced under some
    /// other keying; a pinned literal catches that by value.
    ///
    /// `247f72d4` is `crc32("deadbeef")` and `e212a99e` is
    /// `crc32("deadbeef,deadbeef")`. Before DCR-012 the grouped
    /// duplicate-path listing produced `ee663b6e`
    /// (`crc32("deadbeef,deadbeef#1")`).
    #[test]
    fn test_duplicate_path_digest_equals_bare_hex_join() {
        let single = vec![synthetic_file("data.txt", 0, 4)];
        let grouped = vec![
            synthetic_file("data.txt", 0, 4),
            synthetic_file("data.txt", 1, 4),
        ];
        let constant_crc = |_: &ArchiveEntry| -> Result<u32> { Ok(0xDEAD_BEEF) };

        let (single_digest, _) = content_multiset_digest_and_size(&single, constant_crc).unwrap();
        let (grouped_digest, _) = content_multiset_digest_and_size(&grouped, constant_crc).unwrap();

        assert_eq!(single_digest, "247f72d4", "crc32(\"deadbeef\")");
        assert_eq!(grouped_digest, "e212a99e", "crc32(\"deadbeef,deadbeef\")");
        assert_ne!(
            grouped_digest, "ee663b6e",
            "the pre-DCR-012 ordinal-bearing value must not come back"
        );
    }

    /// Multiplicity is carried by repetition in the element vector, so
    /// the vector must never be collapsed to a set. "Just dedupe the
    /// hashes" is the tempting next simplification and would make a
    /// two-copy archive digest as a one-copy archive.
    #[test]
    fn test_content_digest_multiplicity_is_significant() {
        const X: u32 = 0xDEAD_BEEF;
        const Y: u32 = 0x0BAD_F00D;
        let crc_of = |entries: &[ArchiveEntry], crcs: &'static [u32]| {
            content_multiset_digest_and_size(entries, |e: &ArchiveEntry| Ok(crcs[e.id]))
                .unwrap()
                .0
        };

        let one = vec![synthetic_file("a.txt", 0, 1)];
        let two_same_path = vec![synthetic_file("a.txt", 0, 1), synthetic_file("a.txt", 1, 1)];
        let two_spread = vec![synthetic_file("a.txt", 0, 1), synthetic_file("b.txt", 1, 1)];

        let d_one = crc_of(&one, &[X]);
        let d_two_same_path = crc_of(&two_same_path, &[X, X]);
        let d_two_spread = crc_of(&two_spread, &[X, X]);
        let d_two_distinct = crc_of(&two_spread, &[X, Y]);

        assert_ne!(d_one, d_two_spread, "a second copy must change the digest");
        assert_ne!(d_one, d_two_same_path, "…including a shadowed second copy");
        assert_ne!(
            d_two_spread, d_two_distinct,
            "distinct payloads must not collide"
        );
    }

    /// Order-independence is now **unconditional**: the same archive
    /// re-listed with its two same-path occurrences swapped digests
    /// identically. Under the path-keyed ordinal the two orders produced
    /// `"X,Y#1"` and `"Y,X#1"` and differed, so one archive had two
    /// digests depending only on which occurrence the backend happened
    /// to enumerate first (DCR-012).
    #[test]
    fn test_content_digest_is_order_independent_for_duplicate_paths() {
        const X: u32 = 0xDEAD_BEEF;
        const Y: u32 = 0x0BAD_F00D;
        // The resolver is keyed on the *listing id*, exactly as the
        // CRC-less streaming path is (ti-2a6e3153) — so swapping the two
        // entries in the vector swaps the enumeration order without
        // changing which payload belongs to which occurrence.
        let by_id = |e: &ArchiveEntry| -> Result<u32> { Ok(if e.id == 0 { X } else { Y }) };

        let forward = vec![synthetic_file("m", 0, 2), synthetic_file("m", 1, 2)];
        let swapped = vec![synthetic_file("m", 1, 2), synthetic_file("m", 0, 2)];

        let (forward_digest, forward_size) =
            content_multiset_digest_and_size(&forward, by_id).unwrap();
        let (swapped_digest, swapped_size) =
            content_multiset_digest_and_size(&swapped, by_id).unwrap();

        assert_eq!(forward_size, swapped_size);
        assert_eq!(
            forward_digest, swapped_digest,
            "re-listing one archive in a different order must not move its digest"
        );
    }
}

/// OI-0001-007 cover for [`SizedContentTotal`], the typed size term.
///
/// Lives inline for the same reason the encoding module above does: the
/// accumulation and its coverage counters are private to
/// [`content_multiset_digest_and_size`], and the defect being pinned is
/// in that accumulation rather than at the facade. The real-backend
/// half — a raw `.gz` handle whose listing declares no size — is pinned
/// at the facade in `tests/digest_exactness_test.rs`, because only a
/// real backend proves the `None` branch is reachable in production and
/// not merely constructible in a test.
#[cfg(test)]
mod content_size_total_tests {
    use super::{ArchiveEntry, Result, content_multiset_digest_and_size};

    fn sized(path: &str, id: usize, size: u64) -> ArchiveEntry {
        ArchiveEntry::file(path, id).size(size).build()
    }

    /// The builder leaves `size` unset, which is exactly the shape the
    /// raw gzip/bzip2/xz readers and `archive_entry_size_is_set`-false
    /// libarchive entries produce (R0001-0014).
    fn unsized_file(path: &str, id: usize) -> ArchiveEntry {
        ArchiveEntry::file(path, id).build()
    }

    fn constant_crc(_: &ArchiveEntry) -> Result<u32> {
        Ok(0xDEAD_BEEF)
    }

    /// The defect itself. Two of three entries declare a size; the total
    /// must report the partial sum *and* say it is partial, with the
    /// unknown count exact. Before OI-0001-007 this returned a bare `9`
    /// that a caller had no way to distinguish from a complete total.
    #[test]
    fn unknown_size_entry_makes_the_total_incomplete() {
        let entries = vec![
            sized("a.txt", 0, 4),
            unsized_file("b.gz", 1),
            sized("c.txt", 2, 5),
        ];

        let (_, total) = content_multiset_digest_and_size(&entries, constant_crc).unwrap();

        assert!(!total.is_complete(), "one entry declared no size");
        assert_eq!(total.exact(), None, "an incomplete total is not exact");
        assert_eq!(total.sized_bytes(), 9, "the partial sum survives");
        assert_eq!(total.unsized_entries(), 1);
        assert_eq!(total.sized_entries(), 2);
        assert_eq!(total.file_entries(), 3, "sized 2 of 3");
    }

    /// Every file entry unsized: the total is `0` bytes, which is
    /// precisely the number the old `u64` returned while claiming to be
    /// a summary. Here the `0` is unmistakably a non-answer.
    #[test]
    fn all_entries_unsized_reports_zero_and_says_so() {
        let entries = vec![unsized_file("a.gz", 0), unsized_file("b.gz", 1)];

        let (digest, total) = content_multiset_digest_and_size(&entries, constant_crc).unwrap();

        assert_eq!(total.sized_bytes(), 0);
        assert_eq!(total.exact(), None);
        assert_eq!(total.unsized_entries(), 2);
        assert_eq!(total.sized_entries(), 0);
        assert!(
            !digest.is_empty(),
            "an unsized entry still participates in the digest"
        );
    }

    /// The complement, so the incompleteness flag cannot be stuck on: an
    /// all-declared listing reports complete and hands back the sum.
    #[test]
    fn all_sized_entries_report_a_complete_exact_total() {
        let entries = vec![sized("a.txt", 0, 4), sized("b.txt", 1, 5)];

        let (_, total) = content_multiset_digest_and_size(&entries, constant_crc).unwrap();

        assert!(total.is_complete());
        assert_eq!(total.exact(), Some(9));
        assert_eq!(total.sized_bytes(), 9);
        assert_eq!(total.unsized_entries(), 0);
        assert_eq!(total.file_entries(), 2);
    }

    /// Non-file entries are outside the digest's population, so they must
    /// stay outside the coverage counters too — otherwise a directory
    /// entry (always `size: None`) would report a spurious shortfall on
    /// an archive whose every *file* is sized.
    #[test]
    fn directories_do_not_count_as_unsized_entries() {
        let entries = vec![
            ArchiveEntry::dir_at("sub/", 0).build(),
            sized("sub/a.txt", 1, 7),
        ];

        let (_, total) = content_multiset_digest_and_size(&entries, constant_crc).unwrap();

        assert!(total.is_complete(), "a directory is not a missing size");
        assert_eq!(total.exact(), Some(7));
        assert_eq!(total.file_entries(), 1);
    }

    /// An archive with no file entries returns the empty digest; the
    /// total beside it is a truthful zero, not an unknown.
    #[test]
    fn empty_listing_is_a_complete_zero() {
        let (digest, total) = content_multiset_digest_and_size(&[], constant_crc).unwrap();

        assert!(digest.is_empty());
        assert!(total.is_complete());
        assert_eq!(total.exact(), Some(0));
        assert_eq!(total.file_entries(), 0);
    }

    /// `Display` is the accessor a human reads, so pin that it never
    /// renders a partial total as a plain byte count.
    #[test]
    fn display_names_the_shortfall() {
        let (_, complete) =
            content_multiset_digest_and_size(&[sized("a.txt", 0, 9)], constant_crc).unwrap();
        assert_eq!(complete.to_string(), "9 bytes");

        let (_, partial) = content_multiset_digest_and_size(
            &[sized("a.txt", 0, 9), unsized_file("b.gz", 1)],
            constant_crc,
        )
        .unwrap();
        assert_eq!(
            partial.to_string(),
            "at least 9 bytes (1 of 2 file entries declare no size)"
        );
    }

    /// Equality is over the whole value, not just the byte count: two
    /// totals that agree on `sized_bytes` but disagree on coverage must
    /// not compare equal, or a caller diffing two archives would call an
    /// incomplete total a match.
    #[test]
    fn coverage_participates_in_equality() {
        let (_, complete) =
            content_multiset_digest_and_size(&[sized("a.txt", 0, 9)], constant_crc).unwrap();
        let (_, partial) = content_multiset_digest_and_size(
            &[sized("a.txt", 0, 9), unsized_file("b.gz", 1)],
            constant_crc,
        )
        .unwrap();

        assert_eq!(complete.sized_bytes(), partial.sized_bytes());
        assert_ne!(
            complete, partial,
            "same byte count, different coverage — not the same total"
        );
    }
}
