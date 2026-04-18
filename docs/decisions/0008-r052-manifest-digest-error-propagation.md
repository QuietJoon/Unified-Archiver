# AD: Manifest Digest Error Propagation (R052-001)

## Context and Problem Statement
Found in Review 052 (Issue R052-001, Severity: High).
Location: `src/inspection.rs` — `calculate_manifest_digest()` and `have_metadata_digest()`

`calculate_manifest_digest()` silently fell back to `{path}:{size}` when `extract_to_memory()` failed, mixing two different semantics (true content digest vs surrogate) inside one success path. Callers could not distinguish a genuine content-identity digest from a degraded fallback.

## Decision Drivers
* Content-identity digests must be trustworthy — a silent fallback hides corruption, password errors, and backend read failures
* `have_metadata_digest()` should distinguish "can compute" from "cannot compute" for encrypted entries without stored CRC32
* Callers should receive clear errors rather than unreliable data

## Considered Options
1. Propagate extraction errors — `calculate_manifest_digest()` returns `Err` on any entry read failure
2. Return a degraded-status result (e.g., enum with `Full` vs `Degraded` variant)
3. Keep silent fallback (rejected)

## Decision Outcome
ACCEPT (Option 1): Extraction errors now propagate to the caller. No silent `{path}:{size}` fallback.

Additionally, `have_metadata_digest()` was extended to detect encrypted entries without stored CRC32 and return `Err(ArchiveError::Password)` for those — these entries cannot have their digest computed without a password.

Status: Implemented

### Implementation
- `src/inspection.rs`: `calculate_manifest_digest()` — removed `Err(_) => format!(...)` fallback; errors propagate via `?`
- `src/inspection.rs`: `have_metadata_digest()` — extended to return `Err(Password)` for encrypted entries without CRC32
- `docs/decisions/0006-r051-manifest-digest-computes-crc32-from-content.md` — updated to reflect error propagation

## Consequences
* Good, because callers always receive a trustworthy content digest or a clear error
* Good, because encrypted entries without CRC32 are detected upfront via `have_metadata_digest()`
* Bad, because callers that previously relied on the fallback now receive errors — this is the correct behavior but may require caller-side error handling updates
