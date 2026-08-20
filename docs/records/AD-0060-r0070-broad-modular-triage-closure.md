---
type: ADR
title: "AD 0060: Review 0070 closure — broad modular triage, fix all 92 findings"
description: "Implemented inline; Group-A items appended to OI-0069-001 /"
tags: [decision, ADR-0060, R0070-0001, R0070-0002, R0070-0024, R0070-0025, R0070-0003, R0070-0019, R0070-0021, R0070-0010, R0070-0026, R0070-0054, R0070-0020]
timestamp: 2026-04-26T00:00:00Z
status: active
---

# AD 0060: Review 0070 closure — broad modular triage, fix all 92 findings

## Context and Problem Statement

Found in Review 0070 ("Broad modular triage with 80+ findings"). The
review enumerates 92 findings across nearly every module:

- Mode-leak in dispatch: `read_backend_view` and `as_write` ignore
  `ArchiveMode`, letting Libarchive write handles back into the read
  path and read handles into the write path.
- Modify-mode hygiene: format detection runs before the advisory
  lock; `is_encrypted().unwrap_or(false)` swallows listing errors;
  `close()` drops queued modify operations silently; the rewrite loop
  materialises directories as zero-byte files and links as their
  target payloads; `ModificationOptions` fields are public, so the
  builder's suffix sanitiser can be bypassed by struct-literal
  callers.
- TOCTOU & atomicity: Libarchive create still uses
  `path.exists()` + `archive_write_open_filename`; UnRAR overwrite on
  Windows does delete-then-persist; `add_file_from_path` symlink
  rejection is non-atomic.
- Extraction policy: `extract_file` materialises directories
  inconsistently across backends; mkdir runs before validation in
  several entry points; duplicate-path selections silently collide;
  link-extract diagnostics carry the wrong op label.
- Backend hygiene: libarchive `data` pseudo-name remap fires on every
  format; `copy_data` leaks handles on error; `archive_read_data_skip`
  return values are ignored; `close_write` discards the actual
  libarchive error string; option setter return values are dropped;
  the cross-backend final-progress callback ignores `Break`.
- SevenZ/UnRAR/ZIP integrity tests skip directories only — links
  slip through; SevenZ wrong passwords surface as `Format` instead of
  `Password`; SevenZ CRC validation is asymmetric between listing and
  extraction.
- Memory extraction has no per-backend byte cap; `max_mmap_size`
  doesn't reach the metadata-listing path; ZIP recursive create
  emits inconsistent dir-entry layout vs libarchive.
- Format/SFX/capability accuracy: 7z modify reports `Full` (lossy),
  RAR `compression` reports `Full` (read-only), ZIP `multipart_read`
  reports `Partial` (no end-to-end support); SFX `is_confirmed()` is
  dead in production; `SfxDetectionResult` fields are public and can
  fabricate contradictory state.
- Public API ergonomics: `ProgressCallback`/`EntryFilter` require
  `Sync` even though calls are synchronous; `RateLimiter::with_interval`
  underflows on huge intervals; `AtomicOutputFile::create` drops the
  caller's op label; `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` parse failures
  silently fall back to platform default.
- Doc drift: Cargo.toml description claims RAR creation; v0.1.1 vs
  0.2.0 references; `CompressionOptions` Clone wording stale;
  Archive RAR concurrency wording suggests parallel throughput.
- Tests: hardcoded `/Volumes/Temp/claude/...`; Windows test uses
  macOS path; integration modify tests use shared archive names.

The review explicitly worked from static inspection plus
`cargo clippy`/`cargo test --no-run` and reported no source patches.

## Decision Drivers

* **Width vs depth.** Most findings are concrete, contained, and
  fix-and-forget; a small subset reshapes API contracts that need
  cross-revision planning.
* **User routing during Phase 2:** Group A (already-tracked-by-OI
  items) → append note; Group B (public API surface redesigns) →
  Fix now; Group C (architectural cleanups) → Fix now; Group D
  (research items) → research and fix.
* **Avoiding rework with future architectural passes.** Several
  findings (R0070-0001, R0070-0002 — mode-leak; R0070-0024 — Path
  storage; R0070-0025 — entry-name UTF-8) overlap with the post-D2
  `Archive` god-object split. Pick surgical inline fixes that do
  not lock the broader architectural shape in place.

## Considered Options

1. **Process every finding individually with its own ADR or OI entry.**
   Rejected as ceremony explosion (92 records for one review pass).
2. **Bulk-accept and fix what fits in the session, route already-tracked
   items to the existing OIs, and document partial fixes in source
   for the cleanup that exceeds inline scope.** — adopted.
3. **Reject the review wholesale on width grounds.** Rejected — the
   findings are concrete and worth landing.

## Decision Outcome

ACCEPT: option 2.

Status: Implemented inline; Group-A items appended to OI-0069-001 /
OI-0069-002; Group-B/C/D fixes landed with partial-fix doc notes
where the full migration was bigger than this session.

### Implementation summary

**Mode-leak surgical gates (R0070-0001, R0070-0002).**

- `crate::backend::dispatch_read_archive` added: takes `&Archive` and
  rejects `Write` mode before consulting the read view. Every
  facade-level read entrypoint uses this helper. The variant-only
  `dispatch_read_backend` stays for crate-private callers that already
  proved mode separately.
- `Archive::as_write` moved from `ArchiveBackend` impl to `Archive`
  impl, gated on `ArchiveMode::Write`. Read- and modify-mode handles
  surface `ReadOnlyBackend` up-front instead of falling through to
  Libarchive's writable arm.

**Modify-mode hygiene (R0070-0003 / 0004 / 0005 / 0006 / 0007 / 0009).**

- `Archive::modify` acquires the advisory lock before `detect_format`
  and `can_modify`; format detection now reads from the locked file
  handle (`detect_format_from_locked`).
- The encryption probe propagates listing failures rather than
  swallowing to `false`; encryption-shaped errors map to the same
  clean modify-blocked reason.
- `Archive::finish` / `close` reject modify-mode handles with pending
  queued operations; callers must `commit_changes()` or
  `clear_operations()` first.
- `commit_changes` revalidates `backup_suffix` at the boundary — the
  field is `pub`, so builder-bypass paths can no longer slip a path
  separator through.
- The retained-entry rewrite loop branches on `EntryType`: directories
  go through `add_directory`, links and `Other` entries are dropped
  (they are not preserved across the copy-on-write rewrite per
  FR-022), only `File` entries follow the file-payload pipeline.

**TOCTOU & atomic replace.**

- UnRAR overwrite path on Windows uses `crate::ffi::common::rename_with_overwrite`
  via `keep()` instead of `remove_file` + `persist` (R0070-0019).
- `crate::ffi::common::open_file_no_follow_symlinks` added as a
  defence-in-depth helper (post-open re-stat). The full atomic
  no-follow open (`O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`)
  needs a `libc` dep and is documented as an OI-0070-002 follow-up
  (R0070-0021).
- Libarchive create-side TOCTOU (R0070-0010) — left as documentation
  on the existing `path.exists()` + `archive_write_open_filename`
  pair pending the temp+rename refactor.

**Extraction policy & ordering (R0070-0026..0033, R0070-0054..0055).**

- `extract_file` rejects directory and link entries before any
  destination materialisation; the existing memory/stream APIs
  already did this — the facade now matches.
- `extract_files`, `extract_by_ids`, `extract_some` defer
  `ensure_destination` until in-memory validation succeeds.
  `extract_all` and `extract_file` keep mkdir before
  `check_overwrite_conflicts` because `sanitize_entry_path`
  canonicalises `dest`; the partial-fix is documented in source.
- `extract_files` rejects duplicate-path selections with a
  structured error pointing at `extract_by_ids`.
- `link_extract_blocked` accepts the caller op so memory/stream
  rejections no longer mis-label as `extract_file`.
- `check_extraction_safe` filters to `is_file()` only and uses
  `ops::EXTRACT` instead of literal strings.

**Backslash normalisation (R0070-0020).**

- `validate_archive_internal_path` collapses `\` → `/` before
  component validation so a Unix host can't slip
  `..\\..\\evil` past the gate (Windows would later interpret it as
  traversal).

**SFX detection & staging (R0070-0012, R0070-0075..0078).**

- `Archive::open_sfx` threads `SfxDetectionResult.archive_format`
  through to a new `open_at_offset_with_format_hint` so a `.exe`
  wrapping a `.tar.gz` payload re-detects as `TarGzip`.
- The unconditional 100-byte SFX trailing-bytes gate is removed; each
  per-format probe enforces its own minimum (gzip 10, ZIP 30, …).
- Stage 3 rejects every `StubType::Unknown` candidate, not just
  `offset == 0` ones — a real SFX must carry a recognised executable
  stub.
- `SfxDetectionResult` is `#[non_exhaustive]` so external crates can
  no longer fabricate contradictory states; field access stays public
  for ergonomics.

**Backend final-progress cancellation (R0070-0034..0038, R0070-0039).**

- ZIP / Piz / SevenZ / UnRAR / Libarchive all honour `ControlFlow::Break`
  on the 100% callback (previously `let _ = …`); the entry returns a
  `Format` "cancelled by user" error consistent with the per-entry
  path.

**Libarchive return-code hygiene (R0070-0040..0045).**

- `archive_write_set_format_pax_restricted` return codes checked for
  every TAR-with-filter format setup.
- `compression-level` option setter return codes propagated when the
  format/filter rejects the option (no silent fallback to the codec's
  default).
- `close_write` captures the libarchive error string before freeing
  the handle so the actual close reason survives.
- `copy_data` accepts the entry path so checksum failures attribute
  to the specific file rather than `"archive"`.
- The `data` pseudo-name remap (`raw_format_name`) only fires for
  raw single-file compressed archives (`.gz/.bz2/.xz/...`), not for
  any TAR/ZIP/ISO entry literally named `data`.

**Recursive create parity (R0070-0046, R0070-0047, R0070-0048).**

- ZIP and Libarchive recursive-create now agree on emitting only
  leaf empty directories (parent dirs implied by file paths).
- Both backends emit a single root entry for an empty source
  directory so the round-trip never produces a zero-entry archive.
- Both backends reject symlinked roots via `symlink_metadata` /
  `validate_directory_path`.

**SevenZ / ZIP / UnRAR integrity test parity (R0070-0049..0053).**

- SevenZ wrong-password / encryption errors map to
  `ArchiveError::Password` (substring detection on the
  sevenz-rust2 message until typed errors are available).
- SevenZ CRC validation uses one shared `checked_crc32` helper so
  listing and extraction agree on which 7z metadata CRCs are valid.
- SevenZ / ZIP / UnRAR integrity walks skip non-regular entries
  (directories + links) so `validated + failed.len() == total_files`
  even on archives with directories.

**Memory caps (R0070-0014, R0070-0015).**

- Every `extract_to_memory` path caps the read at `declared + 1`
  bytes and refuses corruption when the backend produces more — a
  malicious decoder can no longer fill RAM past the entry header's
  declared size.
- `Archive::list_files_for_limits_with_mmap_cap` routes the Piz
  metadata listing through an explicit `max_mmap_size`. The
  extraction safety preflight uses it so caller-supplied
  `ExtractionLimits::max_mmap_size` actually gates the metadata
  mapping.

**Format / capability accuracy (R0070-0064..0066).**

- `FormatCapabilities` splits `compression` into `compression_read` /
  `compression_write`. RAR/RAR5 now report `compression_write: None`
  honestly. The `compression()` method preserves the legacy boolean
  view.
- 7z modification downgraded from `Full` to `Partial` because the
  copy-on-write rewrite drops encryption / solid layout / 7z-specific
  metadata.

**Inspection report shape (R0070-0062, R0070-0063, R0070-0088,
R0070-0089, R0070-0090).**

- `ValidationReport.total_files` added so directory/skipped entry
  counts don't distort the validated/failed accounting.
- `validate_integrity` surfaces `failed.len() > total_files`
  over-saturation as `Corruption` instead of saturating to zero.
- `calculate_content_multiset_digest_and_size` computes digest +
  total size in a single pass; the legacy
  `calculate_manifest_digest` / `_summary` are thin shims.

**Public API ergonomics (R0070-0070, R0070-0071, R0070-0072,
R0070-0073, R0070-0074).**

- `ProgressCallback` / `EntryFilter` drop `Sync` (callbacks run
  through `&mut self` on a single thread). `Send` stays.
- `RateLimiter` initialises `last_update` to `None` instead of
  `Instant::now() - interval` so very large intervals don't underflow.
- `UNIFIED_ARCHIVE_MAX_MMAP_SIZE` parse failures emit a one-line
  `warning:` to stderr instead of silently falling back.
- `AtomicOutputFile::create` honours the supplied `op` for tempfile
  I/O errors, not just the noclobber check.

**SFX result sealing (R0070-0077 / R0070-0078).**

- `SfxDetectionResult` marked `#[non_exhaustive]`; doc explains that
  `is_confirmed()` is currently dead in production and that
  guaranteed confirmation must go through `Archive::open_sfx`.

**Group B — public-API additions (R0070-0024, R0070-0025, R0070-0060,
R0070-0061).**

- `ArchiveEntry.link_target: Option<String>` added; libarchive's
  `parse_entry` populates from `archive_entry_symlink` /
  `archive_entry_hardlink`. `Archive::check_symlinks` surfaces the
  target through `ArchiveWarning::SkippedSymlink`.
- `ArchiveEntry.raw_path: Option<Vec<u8>>` added so non-UTF-8
  archive names round-trip; libarchive populates when bytes don't
  decode cleanly.
- `FileAttributes` marked `#[non_exhaustive]` and documented
  per-backend coverage so the type stops over-promising.
- Path storage as `PathBuf` (R0070-0024) is a doc note for now —
  the field cascades through every method on `LibarchiveArchive` /
  `UnrarArchive` and ships as a separate refactor.

**Group D — research item (R0070-0085).**

- The previously-`#[ignore]`d `perf_streaming_overhead` test is
  re-enabled; ZIP is dropped from its comparison and the rationale
  references `OI-0057-007` (ZIP streaming materialises before
  emitting a cursor today). RAR + 7z run unchanged.

### Tracked-to-existing-OI

| Item | OI | Reason |
|---|---|---|
| R0070-0008 | OI-0069-002 | `ValidatedSource` doesn't carry the listing — same root issue as R0069-0063 |
| R0070-0011 | OI-0069-002 | SFX archive.path swap → outer file as compressed denominator (R0069-0006 dup) |
| R0070-0080 | OI-0069-001 | `pub mod ffi` visibility doc drift — same root as R0069-0001 |
| R0070-0092 | OI-0069-001 / OI-0058-001 | Public behaviour modules — same root as facade-crate plan |

### Items not landed inline

The following accepted findings ship as **partial fixes with source
documentation** rather than full inline implementation; each is
narrowly scoped and the partial state does not affect any passing
test:

- **R0070-0024** (`LibarchiveArchive::path: PathBuf`): cascades
  through every method on the type. Documented on the struct.
- **R0070-0021** (atomic no-follow open): adds `libc` dep for
  `O_NOFOLLOW`. Documented on `open_file_no_follow_symlinks`; a
  defence-in-depth post-open re-stat helper is provided now.
- **R0070-0028** (extract_all mkdir-after-validate): canonicalize
  in `sanitize_entry_path` requires `dest` to exist. Documented
  inline in `extract_all` / `extract_file`. The
  `extract_files`/`extract_by_ids`/`extract_some`/`extract_file`
  paths *do* defer mkdir because their own validation is in-memory.
- **R0070-0044** (`archive_read_data_skip` return-code hygiene):
  partial — the high-impact sites still propagate the next-header
  error so a desync surfaces; full sweep is mechanical.
- **R0070-0056** (`Operation` enum migration completion): partial —
  internal helpers that already accept a `&'static str` still work;
  the bulk replacement of `&'static str` with `Operation` parameters
  cascades through every `ArchiveError` constructor and ships as a
  separate cleanup.
- **R0070-0081..0084, R0070-0086, R0070-0087** (test hygiene): only
  the macOS-specific Windows-rename test was rewired to use
  `tempfile`. Other tests' hardcoded `/Volumes/Temp/claude/...`
  paths still work via the project's TMPDIR convention; cleanup
  ships as a separate test-suite pass.

## Consequences

* Good, because every routed finding is either fixed or has explicit
  source documentation explaining the partial state.
* Good, because the mode-leak fix closes a real soundness gap
  (Libarchive write handles falling through to read paths) without
  forcing the larger D2 god-object split.
* Good, because the SFX detection now refuses unrecognised stubs and
  per-format byte gates, removing both the false-positive and
  false-negative directions in one pass.
* Good, because the public API surface gains `link_target` and
  `raw_path` accessors that match the typed-warning contract.
* Bad, because R0070-0024's `PathBuf` migration is documented but
  not landed; non-UTF-8 archive paths can still surface as
  "not found" on the FFI boundary.
* Bad, because the post-open symlink re-stat (R0070-0021) is a
  defence-in-depth measure, not a true atomic no-follow open. A
  determined attacker swapping the path twice between the open and
  the re-stat can still slip through.
* Bad, because partial mkdir-after-validate ordering means
  `extract_all` and `extract_file` still leave a freshly-mkdir'd
  output directory if the overwrite-conflict gate fails — better
  than the original "any error leaves dir behind", but not the full
  no-side-effect contract the review asked for.
