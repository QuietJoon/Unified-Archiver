---
type: ADR
title: "AD 0067: Review 0076 closure and routing record"
description: "Accepted (2026-05-01) — closes the gate workflow for Review 0076"
tags: [decision, ADR-0067, ADR-0066, ADR-0059, ADR-0064, R0076-0001, R0076-0002, R0076-0003, R0076-0004, R0076-0005, R0076-0006, R0076-0007, R0076-0008]
timestamp: 2026-05-01T00:00:00Z
status: active
---

# AD 0067: Review 0076 closure and routing record

## Status

Accepted (2026-05-01) — closes the gate workflow for Review 0076
(deep modular and correctness audit, 95 issues).

## Context

Review 0076 flagged 95 issues spanning correctness, defense-in-depth,
durability, lossy non-UTF-8 path handling, semver hygiene, typed-handle
parity, and architectural refactors. This ADR records the gate's
routing decisions for the full set so future audits can trace each
RNNNN to its end state.

The gate's "accept-fix-only" rule prohibits silently routing accepted
findings to OI/ADR; every accept must either be fixed in-session or
explicitly routed by user choice. Review 0076's volume forced
group-level Phase 2 routing (auto mode default), which produced 8
new OI trackers + this closure ADR + AD 0066 (sanitization policy).

## Decision

| Issue | Severity | Route |
|---|---|---|
| R0076-0001 | High | Fix in-session — `check_ratio` rejects non-finite/non-positive ratio limits |
| R0076-0002 | High | Fix in-session — `check_archive_ratio` rejects zero compressed with non-zero uncompressed |
| R0076-0003 | Medium | OI-0076-005 (encapsulate ExtractionLimits in v0.4) |
| R0076-0004 | High | AD 0066 (sanitize-vs-reject — preserve baseline, queue v0.4 flag) |
| R0076-0005 | High | OI-0076-003 (security/durability boundary refactors) |
| R0076-0006 | Medium | Fix in-session — `dispatch_extract_core` routes via `dispatch_read_archive` |
| R0076-0007 | Low | Fix in-session — default `extract_all` returns op-blocked, not fake `ArchiveFormat::Tar` |
| R0076-0008 | High | Fix in-session — `Piz::extract_to_stream_with_caps` honours `max_bytes` via `with_hard_cap` |
| R0076-0009 | High | OI-0076-008 (covered by SevenZ/Piz cap-before-allocation family — actually a Piz item; will route under OI-0076-008 alongside R0076-0062) |
| R0076-0010 | Medium | Fix in-session — `copy_with_progress` uses `saturating_add` |
| R0076-0011 | Medium | Fix in-session — `CopyByteBudget::Unbounded` uses `saturating_add` |
| R0076-0012, 0064, 0065 | Medium | OI-0076-008 (cancellation typed) |
| R0076-0013 | Low | OI-0076-008 (parent fsync warning surface) |
| R0076-0014 | High | OI-0076-003 (no-follow opener migration) |
| R0076-0015 | Medium | OI-0076-008 (interruptible chunked write) |
| R0076-0016 | High | Fix in-session — `expected_size + 1` → `saturating_add(1)` |
| R0076-0017 | Medium | OI-0076-003 (ZIP create durability) |
| R0076-0018 | Low | OI-0076-008 (Drop warning) |
| R0076-0019, 0020, 0021, 0039, 0040, 0041, 0042, 0066, 0067, 0083, 0092, 0094 | Low–Medium | OI-0076-001 (non-UTF-8 path fidelity round 2) |
| R0076-0022 | High | Fix in-session — `[target.'cfg(windows)'.dependencies] libc = "0.2"` |
| R0076-0023, 0024, 0025, 0026 | Low–Medium | OI-0076-008 (build.rs improvements) |
| R0076-0027, 0028 | High | Fix in-session — `archive_write_disk_set_options` return code checked at both call sites |
| R0076-0029, 0030, 0031, 0032, 0033, 0034, 0044, 0068 | Medium | OI-0076-007 (libarchive `archive_read_data_skip` return-code sweep — AD 0059 mechanical follow-up) |
| R0076-0035, 0045, 0089 | High–Medium | OI-0076-003 (security/durability boundary refactors) |
| R0076-0036, 0037, 0038, 0050, 0053, 0054, 0055, 0056, 0057, 0058, 0059, 0060, 0061, 0090 | Medium | OI-0076-002 (`ValidatedEntry` token unifies the 14 sites) |
| R0076-0043 | Low | Fix in-session — `archive_entry_*time_is_set` probes added; UNIX_EPOCH timestamps no longer dropped |
| R0076-0046 | Low | REJECTED (silent close) — libarchive doesn't expose structured error codes per checksum failure; substring matching is the documented best-effort approach. |
| R0076-0047 | Low | OI-0076-008 (op label thread-through) |
| R0076-0048 | Medium | OI-0076-008 (ZIP normalized-index lookup) |
| R0076-0049 | Medium | Fix in-session — sanitize uses `normalized_name` so warning + selection + output paths agree |
| R0076-0052 | Medium | Fix in-session — `saturating_add` for ZIP progress total |
| R0076-0062, 0063 | High–Low | OI-0076-008 (SevenZ allocation/overflow) |
| R0076-0069, 0070 | Low | OI-0076-008 (UnRAR FILETIME nanos / progress overflow) |
| R0076-0071 | Low | Fix in-session — `find_rar_exe` doc points at `with_rar_exe_path` |
| R0076-0072, 0073 | Medium | OI-0076-006 (external RAR design hardening) |
| R0076-0074, 0075 | Low | OI-0076-008 (SFX fallback diagnostic) |
| R0076-0076, 0077, 0078, 0079, 0080, 0081 | Low–Medium | Fix in-session — `#[non_exhaustive]` on `ArchiveError`, `Operation`, `ArchiveFormat`, `EntryType`, `Support`, `FormatCapabilities`. **Not** applied to `CompressionOptions`: the flat-bag's struct-literal source-compat was preserved by R0075-0081 and breaking it now contradicts that decision; tracked under OI-0076-005. |
| R0076-0082 | Medium | OI-0076-005 (encapsulate ArchiveEntry) |
| R0076-0084 | Low | Fix in-session — `debug_assert_eq!(inner.mode, ArchiveMode::Read)` in every typed read constructor |
| R0076-0085, 0086, 0087, 0088 | Low–Medium | OI-0076-004 (v2-api typed handle parity) |
| R0076-0091 | Medium | OI-0076-008 (manifest digest streaming limits) |
| R0076-0093 | Low | OI-0076-008 (format detection extension fallback for tiny files) |
| R0076-0095 | Low | OI-0076-005 (`CompressionOptions` — see R0076-0076 row above) |

### Summary

- **Fixed in-session:** R0076-0001, 0002, 0006, 0007, 0008, 0010, 0011, 0016, 0022, 0027, 0028, 0043, 0049, 0052, 0071, 0076, 0077, 0078, 0079, 0080, 0081, 0084 (22 issues).
- **Rejected (silently):** R0076-0046 (1 issue — see row).
- **Tracked in OI:** OI-0076-001 (12), OI-0076-002 (14), OI-0076-003 (6), OI-0076-004 (4), OI-0076-005 (3), OI-0076-006 (2), OI-0076-007 (8), OI-0076-008 (21) — 70 issues across 8 trackers (with overlapping membership documented in OI-0076-008's table).
- **ADR:** AD 0066 (sanitization policy, R0076-0004).
- **This ADR (0067):** the routing record itself.

## Consequences

### Good

- **Auditable.** Every R0076-NNNN has an explicit end state.
- **Bounded session scope.** The 22 in-session fixes were uniformly
  small (saturating/checked arithmetic, non-exhaustive markers, return-code
  checks, doc fixes) and verified by `cargo check --all-features`,
  `cargo fmt`, and `cargo clippy --all-targets --all-features`. No
  test changes required.
- **Coherent OI grouping.** The 8 trackers each unify a theme; e.g.
  OI-0076-002's `ValidatedEntry` token is the architectural answer to
  14 individual backend defense findings, avoiding 14 isolated patches.

### Bad / costs

- **Eight new trackers.** OI-0076-001 through 008 expand the open-issues
  surface meaningfully. The pre-existing OIs (OI-0058-001, OI-0065-001,
  OI-0069-*, OI-0075-*) remain, and the v0.4 cut needs to revisit
  whether all 16 open trackers can land before that cut or whether
  some must slip to v0.5.
- **Lossy-path fidelity work doubles.** OI-0075-001 closed the
  read-side and `add_file_from_path`; OI-0076-001 picks up the 12
  write/extract sites left over. The *policy* is settled (AD 0064);
  the migration is just incomplete.
- **Sanitization policy stays permissive in default mode.** AD 0066
  documents this explicitly but does not change v0.3 behaviour. R0076's
  R0076-0004 concern is acknowledged and queued.

## Related

- AD 0064 (Non-UTF-8 path policy)
- AD 0066 (Sanitization policy — R0076-0004)
- OI-0075-001 (RESOLVED — predecessor of OI-0076-001)
- OI-0076-001 through OI-0076-008 (this review's tracked routes)

## Amendment (2026-08-07, indy-review-cleanup — one finding missing from the routing table)

**R0076-0051 was never routed.** The routing table above jumps from R0076-0049 to R0076-0052
(R0076-0050 is picked up only inside the OI-0076-002 row), and R0076-0051 — "[LOW] ZIP integrity
failures report raw names", `src/ffi/zip_wrapper.rs`, where `test_integrity` pushed
`zip_file.name()` while listing normalized it — appears in no OI-0076-* Source line and in no
summary count. The omission is harmless today: the tree fixed it under R0080-0063, and
`test_integrity` now pushes `normalized_name` (the same value `parse_entry` uses), so failed-entry
names correlate with listing IDs exactly as the finding asked. Recorded here because the closure
record's completeness is the thing a future reader trusts, and because Review 0076 itself was
moved to the cold store on 2026-08-07 — this amendment is now the only in-repo trace of the gap.
