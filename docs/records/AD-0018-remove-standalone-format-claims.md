---
type: ADR
title: "AD: Remove standalone gz/bz2/xz support claims"
description: "Implemented (documentation); re-scoped 2026-08-05 (§B) to creation-only — standalone .gz/.bz2/.xz/.zst/.lz4/.lzma read/extract has shipped since 2026-04-14 (OI-0026-003, verified by MADR-0019); only creation/modification remain out of scope."
tags: [decision, ADR-0018, R0026-0006, R0026-0007, R0026-0008, OI-0026-003]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Remove standalone gz/bz2/xz support claims

## Context and Problem Statement
Found in Review 0026 (Issues R0026-0006, R0026-0007, R0026-0008, Severity: HIGH).
Location: `src/lib.rs`, `docs/API_REFERENCE.md`

The crate documentation and supported-formats table claimed read/extract support for standalone `.gz`, `.bz2`, and `.xz` files. In reality, these `ArchiveFormat` variants only work as part of TAR compound formats (`.tar.gz`, `.tar.bz2`, `.tar.xz`). Attempting to open a standalone compressed file results in a runtime error.

## Decision Drivers
* Documentation must not promise capabilities that don't work
* Removing enum variants would be a breaking API change
* Standalone format support is a reasonable future feature

## Considered Options
1. Remove the enum variants entirely (breaking change)
2. Document the limitation and track standalone support as a future task
3. Implement standalone support immediately

## Decision Outcome
ACCEPT: Option 2 — keep enum variants but document the limitation. Added footnotes in `src/lib.rs` and `docs/API_REFERENCE.md`. Tracked as OI-0026-003 for future implementation.

Status: Implemented (documentation), tracked (feature)

### Implementation
- `src/lib.rs`: Added `‡` footnote to GZIP/BZIP2/XZ rows explaining limitation
- `docs/API_REFERENCE.md`: Added note to Archive::open supported formats and ArchiveFormat enum

## Consequences
* Good, because users are no longer misled about standalone format support
* Good, because no breaking API change
* Bad, because the limitation still exists (tracked for future work)

## Amendment (2026-08-05, decision-review-2026-07-19 §B — re-scoped to creation-only, ruling stands)

The 2026-07-19 review flagged this record's text as "factually false (standalone gz/bz2/xz
read/extract shipped per gate-0019); re-scope to creation-only", and as a blanket statement it is:
the Context still asserts that "these `ArchiveFormat` variants only work as part of TAR compound
formats (`.tar.gz`, `.tar.bz2`, `.tar.xz`)" and that "Attempting to open a standalone compressed
file results in a runtime error." **Standalone open/inspect has worked since 2026-04-14, and
extraction since 2026-04-23** —
OI-0026-003 is RESOLVED in `docs/project/open-issues-resolved.md` ("Added
`archive_read_support_format_raw` FFI binding. Standalone compressed files exposed as single-entry
archives"), four days after the findings this record processes (R0026-0006/0007/0008, dated
2026-04-10; the record's legacy file `docs/architecture/decisions/0018-remove-standalone-format-claims.md`
was added in commit `4deab2e`, 2026-04-10). It was then verified end-to-end by **MADR-0019**
(legacy `docs/decisions/0019-r0003-raw-format-verification-and-entry-name-normalization.md`, the
record `src/lib.rs` and `docs/API_REFERENCE.md` cite as "AD 0019" — today's `AD-0019` is the
unrelated UnRAR process-wide-mutex record), which un-ignored the ten raw-format compatibility tests
and fixed the `"data"` → file-stem normalization that now lives in
`src/ffi/libarchive_wrapper/reader.rs::raw_format_name` (gated by `is_raw_compressed_archive`).
That gap is worth stating precisely, because MADR-0019's own Consequences do: between the two dates
`open`/`list` worked for all three formats, but `extract_to_memory()` on standalone `.bz2` / `.xz`
returned a spurious "not found" (only `.gz` happened to work, "by coincidence") — so the read half
was complete in stages, not at once.
R0075-0031 later added `Zst` / `Lz4` / `Lzma` on the same read-only policy, with end-to-end coverage
in `tests/integration/readonly_codec_formats.rs` (OI-0078-001, RESOLVED 2026-07-06).

**What survives is exactly the creation half, and it still governs.** Current matrix for the six
standalone streams `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma`:

* **Open / inspect / extract: supported** through libarchive's raw filter
  (`archive_read_support_format_raw`, declared in `src/ffi/libarchive.rs` and registered in the
  reader setup in `src/ffi/libarchive_wrapper/reader.rs`);
  `src/format.rs::ArchiveFormat::capabilities` gives all six `compression_read: Support::Full`,
  and the raw single entry surfaces under the archive's file stem.
* **Creation / modification: not supported** — the same match arm sets
  `compression_write: Support::None` and `modification: Support::None`, and
  `ArchiveFormat::can_create` excludes all six with a rustdoc that cites this record by name:
  "**Standalone Gzip / Bzip2 / Xz / Zst / Lz4 / Lzma**: AD 0018 keeps standalone single-file
  compressor *creation* out of scope". README.md's format matrix (Create ❌ / Modify ❌ on the
  GZIP/BZIP2/XZ/ZST/LZ4/LZMA rows, "Standalone `.gz` is read/extract only"), Limitations.md
  section 5 ("Standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` / `.lzma` are read-only", with
  open/inspect and extraction listed as supported), and the `Archive::open` note in
  `docs/API_REFERENCE.md` all state that creation-only scope.

Do not conflate the standalone streams with the compound formats: **TAR.ZST / TAR.LZ4 / TAR.LZMA
creation landed 2026-08-04** (CHANGELOG.md `[Unreleased]` → "Added (2026-08-04)"; `can_create` now
matches `TarZst | TarLz4 | TarLzma` and the compressed-tar capability arm carries
`compression_write: Support::Full`), while standalone `.zst` / `.lz4` / `.lzma` stay read-only.
Three pieces of surrounding prose have not caught up with that landing and remain open
documentation follow-ups (not decided here): the per-variant rustdoc on `ArchiveFormat::TarZst` /
`TarLz4` / `TarLzma` in `src/format.rs` still says "create deferred to v0.4 … (R0075-0031)",
contradicting `can_create`; the "If you need compressed archive creation, use" list in
Limitations.md section 5 and the equivalent list in the `docs/API_REFERENCE.md` note still name only
`.tar.gz` / `.tar.bz2` / `.tar.xz`; and the crate-root note in `src/lib.rs` still describes
standalone read support for `.gz`/`.bz2`/`.xz` alone, omitting the R0075-0031 codecs.

**Disposition: active.** No later record reversed this ruling — the over-claim it removed was real
on 2026-04-10, and the standalone-creation scope it set is still what `can_create` enforces and what
every user-facing document states. Only the record's *tracked* half is closed: the "Implemented
(documentation), tracked (feature)" status above should be read as read-support-resolved
(OI-0026-003, 2026-04-14), with creation the remaining out-of-scope half and no open issue tracking
it — whether standalone-compressor creation is ever added is untouched by this amendment. Cross-refs: MADR-0019 (legacy
"AD 0019", raw-format verification and stem normalization); OI-0026-003 (read support);
OI-0078-001 (ZST/LZ4/LZMA fixture coverage); R0075-0031 (variant addition, creation deferral closed
2026-08-04); DCR-003 (raw-LZMA extension fallback, since `.lzma` has no stable short magic).
