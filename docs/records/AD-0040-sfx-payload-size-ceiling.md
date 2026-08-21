---
type: ADR
title: "AD 0040: Cap SFX payload size before copying to disk"
description: "Review 0062 R0062-0004 flagged that Archive::openatoffset() copied the archive payload tail from the SFX file into a temp file before handing the…"
tags: [decision, ADR-0040, ADR-0015, R0062-0004]
generated:
  by: unknown/unknown
  at: 2026-04-18T00:00:00Z
category: security-safety
status: draft
---

# AD 0040: Cap SFX payload size before copying to disk

## Context and Problem Statement

Review 0062 R0062-0004 flagged that `Archive::open_at_offset()` copied the archive payload tail from the SFX file into a temp file before handing the temp path to the backend, with no ceiling on `file_len - offset`. A pathological SFX file — either crafted by an attacker or just malformed — could cause `open_at_offset()` to attempt to write tens or hundreds of gigabytes to the caller's temp directory before any backend validation ran.

The detection pipeline already caps the scan window at 1 MB (AD 0015), but detection only identifies a *candidate* offset; it does not guarantee the bytes after that offset are a real archive. Copying the tail to disk is the hand-off to the backend, and that hand-off was unbounded.

## Decision Drivers

* `open_at_offset()` is reachable from user input (any path the caller can open with `Archive::open_sfx()`), so its disk-write behaviour must have a ceiling.
* The legitimate upper bound on an SFX payload is governed by the archive formats we support: RAR splits at 4 GiB, 7z is practically bounded by solid-block sizes, and ZIP64 is the only format with multi-GiB single-payload support. A 16 GiB ceiling covers all realistic cases with headroom.
* The error must be distinguishable from "archive corrupt" so callers can log an informative message rather than retrying or continuing.

## Considered Options

1. **No ceiling.** Rejected: unbounded disk write from user-supplied input.
2. **Low ceiling (e.g. 1 GiB).** Rejected: would reject legitimate large SFX payloads (installer distributions, backup archives).
3. **Ceiling at 16 GiB, returning `ArchiveError::Format` on overflow.** Accepted.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

`src/archive.rs::open_at_offset()`:

```rust
const MAX_SFX_PAYLOAD_SIZE: u64 = 16 * 1024 * 1024 * 1024; // 16 GiB
let payload_size = file_len - offset;
if payload_size > MAX_SFX_PAYLOAD_SIZE {
    return Err(ArchiveError::format(
        None,
        format!(
            "SFX payload size {} bytes exceeds the {} byte ceiling",
            payload_size, MAX_SFX_PAYLOAD_SIZE
        ),
    ));
}
```

The check runs after the file-length lookup and before the tempfile is opened, so no partial temp file is created on rejection.

## Consequences

* SFX payloads larger than 16 GiB fail fast with a `Format` error rather than filling the caller's temp volume.
* Callers that legitimately need a larger ceiling will need to file an issue; no public knob is exposed yet.
* The ceiling is not a hard format guarantee — a 15 GiB payload that is actually corrupt will still be copied to disk before the backend rejects it. The ceiling is a blast-radius bound, not an integrity check.

## Revisit trigger

Raise the ceiling or expose it as configurable if:

* Legitimate user-facing SFX payloads above 16 GiB appear in real use.
* The temp-file hand-off is replaced by a streaming `ArchivePosition`-style API (would make this decision obsolete).

## Amendment (2026-07-22, R0081 I1)

The 16 GiB ceiling is now a **first-class field of `ExtractionLimits`**,
`max_sfx_payload_size: Cap` (default `Cap::Limited(16 GiB)`), settable via
`ExtractionLimits::builder().max_sfx_payload_size(..)`. Its value is
defined once as `crate::security::DEFAULT_MAX_SFX_PAYLOAD_SIZE`.

The bare `const MAX_SFX_PAYLOAD_SIZE` in `src/sfx/limits.rs` survives only
as a thin alias re-exporting that default, so the SFX size-relationship
documentation (scan window / header probe / staging ceiling / stub
ceiling) stays gathered in one module. `stage_sfx_payload` — reached from
`Archive::open_at_offset`, which has no `ExtractionLimits` in scope —
continues to read the alias, so runtime behaviour is unchanged: payloads
above 16 GiB still fail fast with a `Format` error before any tempfile is
created.

What changed is that the ceiling is no longer a *bare* deferral: it has a
typed, validated home on the limits struct (partially advancing
OI-0076-005), and the previously-noted "no public knob is exposed yet"
consequence is now resolved for callers that construct their own
`ExtractionLimits`. Threading a caller-supplied `max_sfx_payload_size`
through `open_at_offset` itself (which currently takes no limits argument)
remains future work.

## Amendment (2026-08-21, ticket `1ddc37ec` — the ceiling is a consequence of staging)

This decision was written as if the payload copy were unavoidable, and
the ceiling as if it were a property of SFX payloads. Neither holds any
more, and the distinction now has to be stated because the two halves of
the read surface behave differently.

**The ceiling bounds a copy, not a format.** Nothing about ZIP, RAR, 7z
or TAR requires 16 GiB. The number exists because
`Archive::open_at_offset()` wrote `file_len - offset` bytes into the
caller's temp directory before any backend saw them, and an unbounded
disk write driven by user-supplied input needed a bound. Where there is
no write, the bound has nothing to bind.

**Which paths still stage.** A ZIP payload behind an SFX-shaped path
(executable extension) is now read **in place**: `Archive::try_open_in_place`
hands the ZIP backend the caller's own file and the `zip` crate resolves
the prepended-stub offset itself, folding it into every local-header
position. No bytes are copied and the ceiling is not consulted. Every
other read of an embedded payload still stages under the ceiling:

| Payload | Path | Ceiling in force |
|---|---|---|
| ZIP, SFX-shaped outer path, offset agreed by the ZIP's own EOCD | in place | no — nothing is written |
| ZIP, any gate declined (non-executable outer name; offset the EOCD disagrees with; no local-file-header signature at the offset) | staged | yes |
| TAR family, ISO, and the standalone codecs (libarchive) | staged | yes |
| 7z, RAR | staged | yes |
| encrypted payloads via `Archive::open_encrypted` | staged | yes |

The libarchive case is the valuable one still outstanding — it covers the
whole TAR family plus ISO — and it is blocked on binding libarchive's
client-callback reader (`archive_read_open1` plus the
`archive_read_set_*_callback` family), which the declaration-site rule
confines to `src/ffi/libarchive.rs`. The reasoning is recorded next to
`LibarchiveArchive::open_read_handle`, including why a pre-`lseek`ed
descriptor is not a substitute.

**A caller can tell which one they got.** `Archive::payload_access()`
returns `PayloadAccess::{InPlace, Staged}`. This is deliberately public:
the answer decides whether a multi-GB SFX needs temp-volume headroom,
and whether a lowered `ExtractionLimits::max_sfx_payload_size` had any
effect on the handle just returned. Callers that budget temp space, or
that lowered the cap expecting a refusal, must read it rather than infer
it — a cap of 16 bytes admits a 10 GB in-place ZIP, because zero bytes
were staged.

**What did not change.** The caller-configured cap is still honoured
wherever staging happens (`*_with_limits` entry points thread
`max_sfx_payload_size` into `stage_sfx_payload`; the limits-free entry
points pass the documented default). Rejection still happens before the
tempfile is created, so an over-cap payload still leaves no partial
copy. The detection→open identity binding (OI-0081-001 / R0001-0002)
applies to the in-place path too — the in-place backend reopens the
pathname by name, so it is revalidated against the detection open's
identity exactly as `Archive::open` is.

**One consequence beyond the ceiling.** For an in-place handle the
backend's source file is the whole SFX, stub included, so
`payload_size_for_ratio()` — the denominator of the compression-ratio
gate — subtracts the payload offset. Leaving it as the whole-file length
would have diluted the ratio by the size of the stub and quietly
loosened a zip-bomb gate (R0069-0006).

**One exposure the staged copy used to buy, stated so it is not
rediscovered as a defect.** Staging copies the payload at open time, so
a staged handle is immune to a later rewrite of the outer file. An
in-place handle reads that file for the whole life of the handle, and
AD 0065 freezes the listing at *first observation* rather than at open —
so a concurrent rewrite between the two is visible. This is not a new
class of exposure: it is exactly what every ordinary `Archive::open` of
an on-disk archive already has, and the detection→open identity
revalidation (OI-0081-001) still refuses a same-pathname replacement up
to the open. It is recorded here because the staged path's immunity was
an unstated side effect of the copy, and the copy is what went away.

**Revisit trigger, restated.** The original trigger "the temp-file
hand-off is replaced by a streaming API (would make this decision
obsolete)" has now fired for exactly one backend. This decision becomes
obsolete when the last staging path is gone; until then it governs the
staged paths listed above, and the table is the authoritative statement
of where that is.
