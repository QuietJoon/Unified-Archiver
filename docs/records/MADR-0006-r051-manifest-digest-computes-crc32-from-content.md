---
type: ADR
title: "AD: Manifest digest computes CRC32 from entry content for CRC-less formats"
description: "Superseded by AD 0047, which owns content-based CRC32 on CRC-less formats today; the have_metadata_digest() surface recorded here never shipped. See the 2026-08-07 amendment."
tags: [decision, ADR-0006, R0051-0017, R0052-0001, OI-0050-006]
timestamp: 2026-04-23T00:00:00Z
status: superseded
---

# AD: Manifest digest computes CRC32 from entry content for CRC-less formats

## Context and Problem Statement

Found in Review 0051 (Issue R0051-0017, Severity: Medium) and tracked as
OI-0050-006 (accepted from Review 0050).
Location: `src/inspection.rs` — `calculate_manifest_digest()`.

The manifest digest function fell back to `"{path}:{size}"` for entries
without stored CRC32 (TAR-based formats). This made the digest unreliable
for content-identity comparison across formats — two archives with identical
file contents but different formats could produce different digests.

Additionally, there was no way for callers to know whether the digest would
be computed purely from metadata (fast) or would require reading entry data
(slow), making performance characteristics opaque.

## Decision Drivers

* `calculate_manifest_digest()` is documented as a content-identity hash;
  path+size fallback violates that contract.
* TAR-based formats never store per-entry CRC32, so the fallback was always
  triggered for them.
* Reading entry data for CRC is O(total uncompressed size) — callers need a
  way to predict this cost.

## Considered Options

1. Compute real CRC32 by reading entry data when CRC32 is not stored;
   add `have_metadata_digest()` to let callers check upfront.
2. Document the `{path}:{size}` fallback as intentional and accept reduced
   fidelity for TAR-based formats.
3. Remove TAR-based formats from manifest digest support entirely.

## Decision Outcome

**ACCEPT** — Option 1.

`calculate_manifest_digest()` now extracts entry data via
`self.extract_to_memory()` and computes CRC32 with `crc32fast::Hasher` when
stored CRC32 is unavailable. Extraction errors propagate to the caller — no
silent fallback to `{path}:{size}` (removed in R0052-0001).

A new `have_metadata_digest() -> Result<bool>` method returns `Ok(true)` when
all file entries have stored CRC32 (the digest is O(1) per entry), `Ok(false)`
when any entry requires content extraction (O(entry size) per entry), and
`Err(Password)` when any CRC-less entry is encrypted (digest computation
cannot succeed without a password).

Status: Superseded by AD 0047 — see the 2026-08-07 amendment; the `have_metadata_digest()` surface below never shipped.

### Implementation

`src/inspection.rs`:
- `calculate_manifest_digest()` restructured from iterator `.map()` to `for`
  loop (needed for `&self` borrow in `extract_to_memory()` call).
- `have_metadata_digest()` added as a public method.
- Rustdoc updated with performance note about TAR-based formats.

## Consequences

* Good — manifest digest is now a true content-identity hash across all formats.
* Good — callers can use `have_metadata_digest()` to decide whether to call
  `calculate_manifest_digest()` or use a faster alternative.
* Bad — `calculate_manifest_digest()` is now O(total uncompressed size) for
  TAR-based formats instead of O(number of entries). Callers must be aware of
  this cost for large TAR archives.
* Bad — encrypted entries without stored CRC32 cause `calculate_manifest_digest()`
  to fail with `Err`. The old `{path}:{size}` fallback has been removed
  (R0052-0001) because it produced incorrect content-identity digests.

## Amendment (2026-08-07, indy-review-prune — superseded by AD 0047)

**Superseded by AD 0047** ("Content-based manifest_digest on CRC-less formats"), the record the
tree actually cites for this behaviour: `src/inspection.rs` rustdoc, `docs/STREAM_CRC32.md`, and
CHANGELOG.md all name AD 0047; nothing outside the catalogue cites this record. Half of this
record's Decision Outcome never shipped: `have_metadata_digest()` has zero occurrences in any
commit's source (`git log -S have_metadata_digest` over all refs finds only documentation), so the
public method and its `Ok(true)`/`Ok(false)` contract were accepted but never realized — the same
pattern MADR-0007's and MADR-0012's amendments document for their claims. The surviving half
(stream the entry content into a CRC32 instead of the `{path}:{size}` fallback) is today's
behaviour, but AD 0047 owns it: its Context records that the `{path}:{size}` fallback was still
live when Review 0064 found it (2026-04-21), six days after this record's "Implemented" claim, and
the mechanism now lives in `entry_crc32_for_digest` feeding
`calculate_content_multiset_digest_and_size` (with `calculate_manifest_digest` kept as a shim,
R0070-0088/0089). Cross-refs: AD 0047 (successor); MADR-0008 (companion record, superseded in the
same pass); OI-0050-006 in `docs/project/open-issues-resolved.md`, whose Resolution line repeats
the never-shipped `have_metadata_digest()` claim and carries a correction note dated today.
