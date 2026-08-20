---
type: DCR
title: "DCR-003: Extend `Archive::open` extension-fallback set to LZMA / TAR.LZMA"
description: "Archive::open's post-magic-byte fallback consulted only .tar and"
tags: [change, DCR-003, ADR-0064, ADR-0065, ADR-0062, R0078-0001, R0078-0002, R0078-0003, R0078-0084, R0075-0031, R0079-0034, OI-0075-004, OI-0078-001]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# DCR-003: Extend `Archive::open` extension-fallback set to LZMA / TAR.LZMA

- **Date:** 2026-05-04
- **Source:** Review 0078, Issues R0078-0001 / R0078-0002 / R0078-0003 / R0078-0084
- **Affected ADRs:** [AD-0062-r0069-group-a-v0.3-api-shaping.md](AD-0062-r0069-group-a-v0.3-api-shaping.md) (updated)

## What Changed

`Archive::open`'s post-magic-byte fallback consulted only `.tar` and
`.iso` extensions before AD 0064/AD 0065. After Review 0078 it
additionally accepts `.lzma`, `.tar.lzma`, and `.tlz` because raw LZMA
streams have no stable short magic marker. The fallback is otherwise
unchanged — corrupt `foo.zip` still fails with an unknown-format error
rather than being trusted as ZIP.

Implementation: the inline `match ext_str { "tar" | "iso" }` ladder in
`ArchiveFormat::detect` is replaced by a typed `format_from_extension`
call whose return is filtered to the documented fallback set
(`Tar | Iso | Lzma | TarLzma`). All other extensions return
`Unknown archive format`.

## Why

Lzma and TarLzma variants were added in R0075-0031 (OI-0075-004) but
their open path was unreachable for raw streams without magic bytes —
the public docs claimed `.lzma` read/extract while
`Archive::open("file.lzma")` failed with `Unknown archive format`.
Extending the fallback set closes that contract gap without expanding
extension trust to formats that do have magic bytes.

## Affected Areas

- `src/format.rs` — fallback now routes through `format_from_extension`
- `src/archive.rs` — `Archive::open` rustdoc enumerates the exact
  fallback set rather than describing the policy abstractly
- `src/format.rs` — `format_from_extension` rustdoc clarifies its
  diagnostic-only nature (callers asking "what format would Archive::open
  pick?" can still get `Some(Zip)` from this helper, but `Archive::open`
  will not trust it for ZIP)
- `src/modification.rs` — `detect_format_from_locked` (the advisory-lock
  mirror of `ArchiveFormat::detect` used by `Archive::modify`) routes
  through the same `format_from_extension` + `is_extension_fallback`
  policy instead of its stale inline `{tar, iso}` ladder (missed site,
  found by R0079-0034). `.lzma` / `.tar.lzma` / `.tlz` modify attempts
  now fail with the capability-gated `OperationBlocked` instead of
  `Unknown archive format`.

## Migration / Follow-up

- AD 0062 documents the extension-guided detection policy at a high
  level; the fallback set is now an explicit list rather than an
  implicit `{tar, iso}` tuple. AD 0062's narrative remains correct.
- No public API change; behavior is strictly additive (more files open
  successfully, none change format).
- Test coverage: `tests/format_compatibility_test.rs` exercises the
  extension table for every new variant; full open/list/extract
  fixtures for `.lzma` / `.tar.lzma` / `.tlz` are tracked in
  OI-0078-001.
- Review 0079 (R0079-0034) found the original change had missed the
  modify-path mirror `detect_format_from_locked`, which still carried
  the pre-DCR inline ladder. It now consumes the centralized policy,
  so future fallback-set changes need only touch
  `ArchiveFormat::is_extension_fallback`.

## Amendment (2026-08-07, indy-review-cleanup — Review 0078 disposition trail transcribed)

Review 0078 never received a review-level closure record — unlike reviews 0068/0069/0070/0071/
0075/0076, each of which has one (AD 0051/0059/0060/0061/0063/0067). Its disposition trail was
this DCR, OI-0078-001, and the `Issue mapping` header inside `reviews/reviewed/0078.patch`. That
patch file was moved to the cold store on 2026-08-07, so its mapping is transcribed here verbatim
— it is the only record of how ~80 of the review's 86 findings were dispositioned:

```
# Summary:
# - Fix LZMA / TAR.LZMA detection fallback while keeping corrupt magic-first formats rejected.
# - Expand format capability tests to include the newer codec variants.
# - Repair the highest-impact public documentation drift around multipart layout, v2-api,
#   backend internals, and standalone codec caveats.
# - Clean up representative rustdoc warnings from generated docs.
#
# Issue mapping:
# - R0078-0001, R0078-0002, R0078-0003, R0078-0051, R0078-0083, R0078-0084:
#     src/format.rs detection/docs
# - R0078-0053, R0078-0054, R0078-0055, R0078-0056, R0078-0057: format tests
# - R0078-0004..R0078-0008, R0078-0017..R0078-0022: API reference
# - R0078-0026, R0078-0027, R0078-0028, R0078-0030..R0078-0035, R0078-0038:
#     public/architecture docs
# - R0078-0058, R0078-0059, R0078-0067, R0078-0068: rustdoc warning cleanup
#
# Affected files: src/{format,inspection,lib,entry,security}.rs, src/archive/mode_split.rs,
#   docs/API_REFERENCE.md, README.md, docs/USER_MANUAL.md, docs/architecture/README.md,
#   docs/implementation/module-map.md, tests/format_compatibility_test.rs
# CLAIMED_AT 2026-05-01T05:48:32Z / COMPLETED_AT 2026-05-03T04:15:08Z
```

One R0078 finding survived the cleanup triage as still-live and was routed on 2026-08-07:
**R0078-0061** (rustdoc linked the `#[cfg(test)]` `SfxDetectionResult::detected` constructor,
producing a real `cargo doc` warning) was fixed inline in `src/sfx/result.rs` — the link is gone
and the prose now names the constructor without linking it. Every other assessed R0078 finding
verdicted FIXED, DEAD, or covered by a broader ruling.
