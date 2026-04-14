# AD: Entry Metadata Caching

Status: Accepted

## Context and Problem Statement
Repeated listing calls were expensive and some backends required heavy work for CRC metadata. UnRAR iterators are single-pass, making re-listing particularly costly. Preflight safety checks also need entry metadata but do not require full CRC precomputation.

## Decision Drivers
- Repeated-read performance for consumers that call `list_files()` multiple times
- Preflight scan cost reduction when only entry counts and sizes are needed
- UnRAR iterator exhaustion forcing full re-open on repeated list calls

## Considered Alternatives
- **No caching** -- rejected because it results in repeated expensive scans, especially for RAR archives where the iterator must be re-opened.
- **Fully backend-managed caches** -- rejected because each backend would implement caching with inconsistent semantics and lifetimes.

## Decision Outcome
We decided to cache `list_files()` results with `OnceCell` and introduce `list_files_for_limits()` to avoid heavy CRC precomputation when only limits are needed, because it balances performance with the need for a cheaper preflight path.

## Consequences
- Good: Improves repeated-read performance while preserving a cheaper preflight path for safety checks.
- Bad: Cache freshness is tied to handle lifecycle; some backend-specific edge cases still leak through the caching abstraction.
