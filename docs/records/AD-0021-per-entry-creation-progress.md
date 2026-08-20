---
type: ADR
title: "AD: Creation-side progress callbacks fire per entry, not per byte"
description: "Implemented (Phase B.1 of the in-flight remediation plan)."
tags: [decision, ADR-0021, DCR-001, OI-0025-003]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Creation-side progress callbacks fire per entry, not per byte

## Context and Problem Statement
Found in DCR-001 / OI-0025-003 (Severity: MEDIUM).
Location: `src/ffi/zip_writer.rs`, `src/ffi/libarchive_wrapper.rs`, `src/creation.rs`

`CompressionOptions.progress: Option<Box<dyn ProgressCallback>>` existed in the public API but was never consumed by either creation backend. Closing the gap required choosing a callback granularity. Two natural candidates:
- **Per-entry**: invoke the callback once after each `add_file_*` call, with the cumulative bytes written so far.
- **Per-byte (or per-chunk)**: insert a copy-through buffer between the source reader and the backend writer, invoking the callback on every chunk.

## Decision Drivers
* Symmetry with the extraction side: extraction's `ProgressCallback` already fires per entry (see `src/extraction.rs:212` at the time of writing). Callers expect the same shape on both sides.
* Cost: per-byte requires inserting a copy-through buffer in front of every backend write, doubling memory traffic for large entries.
* Total size unknown: creation streams are not pre-sized — we are reading from a `&[u8]` or a filesystem path, building a stream as we go. There is no overall total to report at the start.
* Cancellation surface: the callback returns `ControlFlow<()>`; we want a single, well-defined cancellation point per entry rather than mid-entry partial writes.
* Backend uniformity: both ZIP (zip-rs) and libarchive can hook the per-entry boundary cheaply; a per-byte hook would require different machinery in each backend.

## Considered Options
1. **Per-entry, cumulative-bytes payload, `total = None`** (chosen). Single notification point per `add_file_*` call.
2. **Per-byte / per-chunk via copy-through buffer.** Higher fidelity progress, but doubles memory traffic and complicates cancellation (mid-entry abort would leave a partial central directory entry in ZIP).
3. **Per-entry but with `total = Some(known_total)` when input is a finite slice.** Inconsistent semantics depending on call (`add_file_from_data` would have a total; `add_file_from_path` of a giant file would not). Rejected for fragility.
4. **No callback wiring; remove the field as a breaking change.** Rejected because it removes a documented capability without alternative.

## Decision Outcome
ACCEPT: Option 1.

Both creation backends now own an `Option<Box<dyn ProgressCallback>>` field plus a `bytes_written: u64` counter. `Archive::create()` takes `mut options` and `take()`s the callback into the chosen backend. After each `add_file_from_data` and `add_file_from_path`:

```rust
self.bytes_written = self.bytes_written.saturating_add(payload_bytes);
if let Some(cb) = &mut self.progress {
    if let ControlFlow::Break(()) = cb.on_progress(self.bytes_written, None) {
        return Err(ArchiveError::format(self.format, "Creation cancelled by user"));
    }
}
```

`total` is `None` because creation streams are not pre-sized. `ControlFlow::Break(())` surfaces as `ArchiveError::format(_, "Creation cancelled by user")` — the same convention used by the extraction side, so callers get a uniform cancellation experience.

> **Amended 2026-07-06 (R0076-0012 / R0076-0064 / R0076-0065, via OI-0076-008):**
> cancellation no longer surfaces as a `Format` error. All cancellation
> paths — creation (`notify_creation_progress`), extraction loop
> boundaries (`check_extraction_cancelled`), and mid-entry chunk copies
> (`copy_with_optional_crc_bounded`) — now return the typed
> `ArchiveError::Cancelled { operation }` that SFX staging introduced
> (R0075-0003). The uniform-cancellation-experience goal above is
> preserved and strengthened: callers match one variant instead of
> sniffing message text.

Status: Implemented (Phase B.1 of the in-flight remediation plan).

### Implementation
- `src/ffi/zip_writer.rs`: `progress` + `bytes_written` fields; `notify_progress()` helper; `create()` signature changed to `&mut CompressionOptions`.
- `src/ffi/libarchive_wrapper.rs`: same fields + helper; `create()` signature changed to `&mut CompressionOptions`.
- `src/creation.rs`: thread `&mut options` into the chosen backend constructor.
- `tests/creation_progress_test.rs`: 3 tests (per-entry ZIP, per-entry TAR, cancellation via `ControlFlow::Break`).
- `docs/architecture/verification-matrix.md` SCN-CRE-04 row marked Covered with the per-entry caveat.

## Consequences
* Good, because the creation-side and extraction-side callbacks now have the same shape, so callers can write generic progress UIs.
* Good, because the implementation has zero copy-through overhead — payload bytes flow into the backend exactly once.
* Good, because cancellation has a clean per-entry boundary: `Break` returns from `add_file_*` cleanly, no half-finalized archive on disk (RAII drop closes the temp file).
* Bad, because callers that wanted byte-level progress fidelity for very large single entries get one notification per file, not one per chunk. Acceptable given the cost trade-off; can be revisited as a non-breaking refinement (a second callback or a granularity option) if real demand appears.
* Bad, because `total = None` means UI cannot show a percentage without an out-of-band total estimate. Same trade-off as the extraction side — documented and accepted.
