//! The raw central-directory value layer.
//!
//! Split out of `src/ffi/zip_wrapper.rs` as one of the two seams AD 0057's
//! 2026-08-21 amendment names as unblocked — it touches nothing on the
//! `ZipArchive` facade and depends on neither D2 nor D9.
//!
//! What lives here is the *value* half: one record as the file physically
//! carries it, the index built over all of them, and the exact read that keeps
//! "reached the EOCD", "the record is cut short" and "the OS read failed"
//! distinguishable. The *building* of that index stays on the facade, in
//! `ZipArchive::scan_raw_central_directory`, because it needs the cached
//! descriptor — which is why the struct fields are `pub(super)` rather than
//! private: the parent constructs these values.
//!
//! `unaddressable_records` and `ambiguous_names` are used only from inside this
//! file and are private to it now, where before they were visible across a
//! 1700-line module. `raw_len` is `pub(super)` rather than private because the
//! sibling `tests` child asserts on it — `pub(super)` reaches the whole
//! `zip_wrapper` subtree, which is exactly the reach it had before the move.

use crate::error::{ArchiveError, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Read exactly `buf.len()` bytes of the raw central directory, keeping
/// the three outcomes the duplicate scan must distinguish apart.
///
/// R0001-0027: the scan previously ran `read_exact(..).is_err() { break }`
/// over a whole 46-byte header, which collapsed "reached the EOCD"
/// (expected), "the record is cut short" (truncation), and "the OS read
/// failed" (I/O fault) into one silent loop exit — and a scan that ends
/// early leaves `counts` incomplete, i.e. it *disables* duplicate
/// detection. The EOCD the `zip` crate already parsed always follows the
/// last central-directory record, so a short read here is a truncated
/// directory (`Corruption`); anything else is a real archive-file read
/// failure (`Io`). Normal termination is decided by the record signature,
/// never by a read error.
pub(super) fn read_central_directory_exact(
    file: &mut File,
    buf: &mut [u8],
    path: &Path,
    what: &str,
) -> Result<()> {
    file.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ArchiveError::corruption(
                path.display().to_string(),
                format!("central directory ends mid-record: incomplete {}", what),
            )
        } else {
            ArchiveError::io("read", path.to_path_buf(), e)
        }
    })
}

/// One raw central-directory record, as the file physically carries it.
pub(super) struct RawRecord {
    /// The entry name exactly as stored — never decoded here, because the
    /// `zip` crate collapses on these bytes (R0001-0029).
    pub(super) name: Vec<u8>,
    /// The `zip`-crate listing indices whose stored name is these bytes.
    /// Empty when the crate never parsed a record with this spelling (e.g.
    /// the EOCD undercounts the directory), and several indices when one
    /// raw spelling backs more than one listed entry because two records
    /// disagree about the general-purpose UTF-8 flag.
    pub(super) listed_indices: Vec<usize>,
}

/// The single raw-central-directory index every ZIP operation consults
/// (R0079-0026 / DCR-009 / OI-0001-003).
///
/// **Why an index rather than more guard calls.** Before OI-0001-003 the
/// collapse was re-scanned only by the by-name single-entry paths, and the
/// scan re-opened `self.path` independently of the cached handle — so the
/// scan and the extractor could read two different files, and every other
/// surface (listing, counts, by-id and bulk extraction, integrity) consumed
/// the `zip` crate's already-collapsed view without ever learning the
/// archive was ambiguous. This index closes both halves at once: it is read
/// through the descriptor the cached `RawZipArchive` already owns
/// (`ZipArchive::with_cached_file` in the parent module, never a second
/// `File::open`), and it is
/// memoised once per handle so every operation can afford to consult it.
pub(super) struct RawCentralDirectory {
    /// Every record the raw directory physically carries, in stored order.
    pub(super) records: Vec<RawRecord>,
    /// How many entries the `zip` crate's deduped map exposes — the count
    /// every `zip.len()` walk in this file iterates.
    pub(super) deduped_len: usize,
    /// Normalized paths that appear more than once across the raw
    /// central-directory records. The by-name single-entry paths refuse
    /// these because the `zip` crate's deduped listing would otherwise
    /// silently hand back the surviving (last) record's payload.
    pub(super) duplicate_names: HashSet<String>,
    /// `true` when at least one record the `zip` crate collapsed could not
    /// be attributed to a specific listed name (e.g. an exotic
    /// mixed-encoding collision whose two raw byte strings decode to one
    /// name, or a record the crate never parsed because the EOCD undercounts
    /// the directory). In that case even the by-name surface is refused
    /// wholesale rather than risk silently returning the wrong payload — a
    /// safe over-rejection for a genuinely ambiguous archive.
    ///
    /// R0001-0028: this is decided by counting, not by
    /// `duplicate_names.is_empty()`. The old heuristic disarmed itself the
    /// moment a single ordinary duplicate was attributed, so a crafted
    /// archive could hide an unattributable collision behind an ordinary
    /// one.
    pub(super) any_undetected: bool,
}

impl RawCentralDirectory {
    /// How many records the raw directory physically carries.
    pub(super) fn raw_len(&self) -> usize {
        self.records.len()
    }

    /// Did the `zip` crate collapse anything at all — an ambiguous name, or
    /// a record it could not account for?
    pub(super) fn is_collapsed(&self) -> bool {
        self.any_undetected || !self.duplicate_names.is_empty()
    }

    /// The records the `zip` crate never mapped to a listing index — the
    /// ones no id can address and no name can name. A non-empty result is
    /// what `any_undetected` is usually reporting.
    fn unaddressable_records(&self) -> Vec<&RawRecord> {
        self.records
            .iter()
            .filter(|record| record.listed_indices.is_empty())
            .collect()
    }

    /// The ambiguous listed paths, sorted so a refusal message is stable.
    fn ambiguous_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.duplicate_names.iter().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Refusal text for an operation that would consume the collapsed view.
    ///
    /// The name list is capped: entry names are attacker-controlled, so an
    /// archive with thousands of colliding names must not be able to turn a
    /// rejection into an unbounded string.
    pub(super) fn collapsed_reason(&self) -> String {
        const MAX_NAMES: usize = 8;
        const NAME_PREVIEW_CHARS: usize = 64;

        let mut reason = format!(
            "ambiguous ZIP central directory: {} raw record(s) resolve to {} addressable entry/entries",
            self.raw_len(),
            self.deduped_len
        );
        let names = self.ambiguous_names();
        if !names.is_empty() {
            reason.push_str("; ambiguous name(s): ");
            reason.push_str(&names[..names.len().min(MAX_NAMES)].join(", "));
            if names.len() > MAX_NAMES {
                reason.push_str(&format!(" (and {} more)", names.len() - MAX_NAMES));
            }
        }
        if self.any_undetected {
            reason.push_str(
                "; at least one collapsed record could not be attributed to a listed name",
            );
            let orphans = self.unaddressable_records();
            if let Some(first) = orphans.first() {
                // A diagnostic preview only — never a key. Attacker-controlled
                // bytes are decoded lossily and truncated, and the raw
                // spelling is what the reader failed to parse, so the CP437
                // vs UTF-8 question the tally cares about (R0001-0029) does
                // not arise here.
                let preview: String = String::from_utf8_lossy(&first.name)
                    .chars()
                    .take(NAME_PREVIEW_CHARS)
                    .collect();
                reason.push_str(&format!(
                    " ({} record(s) carry a name the reader never parsed, first '{}')",
                    orphans.len(),
                    preview
                ));
            }
        }
        reason.push_str(
            ". A shadowed record has no listing id, so it can be neither addressed nor \
             extracted, and this operation would silently omit it \
             (R0079-0026 / DCR-009 / OI-0001-003)",
        );
        reason
    }
}
