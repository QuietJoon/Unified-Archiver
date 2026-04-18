# AD: Manifest digest computes CRC32 from entry content for CRC-less formats

## Context and Problem Statement

Found in Review 051 (Issue R051-017, Severity: Medium) and tracked as
OI-050-006 (accepted from Review 050).
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
silent fallback to `{path}:{size}` (removed in R052-001).

A new `have_metadata_digest() -> Result<bool>` method returns `Ok(true)` when
all file entries have stored CRC32 (the digest is O(1) per entry), `Ok(false)`
when any entry requires content extraction (O(entry size) per entry), and
`Err(Password)` when any CRC-less entry is encrypted (digest computation
cannot succeed without a password).

Status: Implemented (2026-04-15), updated (2026-04-15 R052-001). Resolves OI-050-006.

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
  (R052-001) because it produced incorrect content-identity digests.
