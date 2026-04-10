# Stub Manifest (Retrospective)

> This is a retrospective record. Instead of planned STUB markers, this documents remaining incomplete features as DEFERRED entries.

- **Manifest version:** 1
- **Date:** 2026-04-10
- **Notes:** Implementation is complete for all in-scope MVP scenarios. These entries represent explicitly out-of-scope features that are architecturally present but not yet functional.

| ID | Marker | Scope | Path | Binary / Module | Scenario / Slice | Contract / Interface | Dummy Behavior | Exit Condition | Status |
|---|---|---|---|---|---|---|---|---|---|
| DEF-001 | DEFERRED | Future | `src/archive.rs` | archive | SCN-SFX-* (SFX open flow) | `Archive::open_at_offset` | Returns `ArchiveError::Unsupported` | All backends support offset-aware opening | Open |
| DEF-002 | DEFERRED | Future | `src/options.rs` | options | SCN-CRE-* (split creation) | `CompressionOptions::split_size` | Field exists but ignored by all writers | At least one writer honors split_size | Open |
| DEF-003 | DEFERRED | Future | `src/options.rs`, backends | options, archive, ffi/* | All password scenarios | Password fields (String -> SecStr) | Passwords stored in heap Strings; no auto-zeroing | All password fields use SecStr or equivalent | Open |
| DEF-004 | DEFERRED | Future | `src/ffi/wrapper.rs`, `src/streaming.rs` | ffi/wrapper, streaming | SCN-EXT-* (streaming) | `extract_to_stream` | Extracts to memory then wraps in Cursor | Backend-native streaming readers (no full buffer) | Open |
| DEF-005 | DEFERRED | Future | `src/modification.rs`, tests | modification | SCN-MOD-* (ZIP modify) | `commit_changes` for ZIP | Some ZIP scenarios unreliable; ignore-gated tests | ZIP-native read/write pipeline replaces libarchive | Open |
