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
