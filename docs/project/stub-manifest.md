# Stub Manifest (Retrospective)

> This is a retrospective record. Instead of planned STUB markers, this documents remaining incomplete features as DEFERRED entries.

- **Manifest version:** 1
- **Date:** 2026-04-13
- **Notes:** Implementation is complete for in-scope MVP scenarios with tracked gaps. These entries represent explicitly out-of-scope features that are architecturally present but not yet functional.

| ID | Marker | Scope | Path | Binary / Module | Scenario / Slice | Contract / Interface | Dummy Behavior | Exit Condition | Status |
|---|---|---|---|---|---|---|---|---|---|
| DEF-001 | DEFERRED | Future | `src/archive.rs` | archive | SCN-SFX-* (SFX open flow) | `Archive::open_at_offset` | Returns `ArchiveError::Unsupported` | All backends support offset-aware opening | Open |
| DEF-002 | DEFERRED | Future | `src/options.rs` | options | SCN-CRE-* (split creation) | `CompressionOptions::split_size` | Field exists but ignored by all writers | At least one writer honors split_size | Open |
| DEF-003 | DEFERRED | Future | `src/options.rs`, backends | options, archive, ffi/* | All password scenarios | Password fields (String -> SecStr) | Passwords stored in heap Strings; no auto-zeroing | All password fields use SecStr or equivalent | Open |
| DEF-004 | DEFERRED | Future | `src/ffi/wrapper.rs` (~688), `src/ffi/piz_wrapper.rs` (~369), `src/ffi/sevenz_wrapper.rs` (~415), `src/ffi/zip_wrapper.rs` (~394), `src/streaming.rs` | ffi/*, streaming | SCN-EXT-* (streaming) | `extract_to_stream` | All four non-libarchive backends (Piz, ZipReader, SevenZ, UnRAR) internally call `extract_to_memory()` and wrap the buffer in `Cursor`, so memory use is O(entry size) rather than O(window). Only libarchive actually streams. The streaming contract documents this caveat for callers. | All four backends provide true streaming readers (no full buffer) via `StreamingExtractor` ownership of backend handle lifetime | Open |
| DEF-005 | DEFERRED | Future | `src/modification.rs`, tests | modification | SCN-MOD-* (ZIP modify) | `commit_changes` for ZIP | Some ZIP scenarios unreliable; ignore-gated tests | ZIP-native read/write pipeline replaces libarchive | Open |
| DEF-006 | ~~DEFERRED~~ | Future | `src/creation.rs`, `src/ffi/zip_writer.rs`, `src/ffi/libarchive_wrapper.rs` | creation | SCN-CRE-04 (creation progress) | `CompressionOptions::progress` | Both ZIP (zip-rs) and libarchive backends invoke the callback per-entry; `ControlFlow::Break` surfaces as `ArchiveError::format(_, "Creation cancelled by user")` | At least one creation backend invokes progress callbacks | **Closed (2026-04-13, OI-025-003 resolved for wiring portion)** |
| DEF-007 | ~~DEFERRED~~ | Future | `src/sfx/detection.rs`, `src/sfx/stub_types.rs` | sfx/detection | SCN-SFX-08 (unknown stub) | `StubType::Unknown` | `StubType::detect()` returns `Ok(Unknown)` for unrecognized executables and signature scan proceeds | Heuristic scanning identifies archives in unknown stub types | **Closed (2026-04-13, OI-027-001 resolved)** |
| DEF-008 | DEFERRED (partial) | Future | `src/modification.rs` | modification | SCN-MOD-* (modification options) | `ModificationOptions` | `Archive::modify_with_options(path, opts)` honors `create_backup` + `backup_suffix` (sidecar copy written before atomic rename, normalized suffix). `preserve_metadata` accepted but currently a no-op pending Phase C.2. | All `ModificationOptions` fields honored end-to-end (including `preserve_metadata` after Phase C.2 / OI-025-002 closes) | **Partially Closed (2026-04-13, Phase B.2 of DCR-001)** |
