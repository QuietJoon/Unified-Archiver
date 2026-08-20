---
type: ADR
title: "AD: `Archive::open_at_offset` Returns `NotImplemented`, Not a Forged-Format `Unsupported`"
description: "Superseded — Archive::open_at_offset shipped 2026-04-18 (DEF-001 closure); the NotImplemented return this record mandated no longer exists."
tags: [decision, ADR-0023, R0057-0013]
timestamp: 2026-04-17T00:00:00Z
status: superseded
---

# AD: `Archive::open_at_offset` Returns `NotImplemented`, Not a Forged-Format `Unsupported`

Status: Superseded — `Archive::open_at_offset` was implemented on 2026-04-18 (DEF-001 closure); see the 2026-08-04 amendment.

## Context and Problem Statement

Found in Review 0057 (Issue R0057-0013, Severity: Medium).
Location: `src/archive.rs::open_at_offset`.

`open_at_offset` is a deferred feature (DEF-001). The previous implementation returned:

```rust
ArchiveError::unsupported("open_at_offset", ArchiveFormat::Zip, Some("..."))
```

`ArchiveFormat::Zip` is a placeholder — the real archive at the supplied offset might be any format. Callers pattern-matching on the format field would see a false claim.

## Decision Drivers

* Deferred features should use `NotImplemented`, not `Unsupported`. The `Unsupported` variant semantically means "this format cannot do this operation," which is false here — every format could eventually.
* Returning a fake format in a structured error is worse than omitting the format.

## Considered Options

1. Switch to `ArchiveError::not_implemented("open_at_offset", "DEF-001 deferred…")` with no format field.
2. Keep `Unsupported` but change the format to `Option<ArchiveFormat>::None`.
3. Remove the function entirely until implemented.

## Decision Outcome

**ACCEPT Option 1.** The error is now `NotImplemented`, matching the variant split in commit `c9b6685`. The updated library test (`test_open_at_offset_returns_not_implemented`) asserts the new variant. Option 3 would break the SFX workflow that already references this API.

Status: Implemented.

### Implementation

- `src/archive.rs::open_at_offset` — returns `ArchiveError::not_implemented("open_at_offset", "Offset-based archive opening is deferred (DEF-001). Use extract_stub() to materialize the embedded archive as a temporary file.")`.
- `src/archive.rs` test `test_open_at_offset_returns_unsupported` → `test_open_at_offset_returns_not_implemented`.

## Consequences

* Good, because callers can now reliably pattern-match `NotImplemented { feature: "open_at_offset", .. }` to detect the deferral.
* Good, because no fabricated format leaks into error telemetry.
* Neutral — the caller-facing message is strictly more honest.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — superseded by the DEF-001 implementation)

The premise of this record — `open_at_offset` is an unimplemented deferred feature — died on
2026-04-18, when DEF-001 closed in the pre-v0.1.0 gap-closure pass (CHANGELOG v0.1.0;
`docs/project/stub-manifest.md` DEF-001 row). `Archive::open_at_offset(path, offset)` is a
working API in `src/archive.rs`: `offset == 0` short-circuits to `Archive::open`, and a non-zero
offset stages the embedded payload into a tempfile (`stage_sfx_payload`, capped at
`MAX_SFX_PAYLOAD_SIZE` = 16 GiB per AD 0040) that the normal backend constructors then open;
`ReadArchive::open_at_offset` (`src/archive/mode_split.rs`) mirrors it on the typed read handle.
The `ArchiveError::not_implemented(...)` return this record mandated, and the
`test_open_at_offset_returns_not_implemented` test that asserted it, no longer exist anywhere in
`src/`; the current tests are `test_open_at_offset_zero_is_plain_open` and
`test_open_at_offset_past_eof_rejected` (`src/archive/tests.rs`). Two closure-era details have
drifted since and are worth pinning: the ZIP `OffsetReader` adapter recorded at closure
(CHANGELOG v0.1.0) no longer exists in the tree — ZIP payloads stage through the same tempfile
path as every other format — and the shipped API is tempfile-backed, not in-place; a true no-copy
`open_at_offset` remains explicitly out of scope (`docs/project/design-baseline.md`). This record
is therefore **superseded by the implementation**: its error-taxonomy ruling — deferred features
return `NotImplemented`, never an `Unsupported` carrying a forged format — was correct and
remains good guidance for future deferrals, but it has no remaining call site. AD 0006's stale
"`open_at_offset` … remains unimplemented" consequence is corrected by a same-date amendment
there.
