# ADR: Hybrid Backend Selection

## Context and Problem Statement
No single backend met all requirements. UnRAR is needed for RAR/RAR5 support, libarchive provides broad format coverage and read-write capability, and native Rust crates (piz, sevenz-rust2, zip) offer better performance and metadata quality for specific formats.

## Decision Drivers
- Format coverage across RAR, 7z, ZIP, tar, and compressed streams
- Performance characteristics (especially for ZIP and 7z)
- Metadata quality and completeness

## Considered Alternatives
- **libarchive-only** -- rejected because of RAR constraints and metadata limitations that reduce extraction fidelity.
- **UnRAR-only extension model** -- rejected because the format scope is too narrow to serve as a general-purpose archive library.

## Decision Outcome
We decided to combine UnRAR (RAR/RAR5), libarchive (broad format fallback and read-write), and native Rust backends (piz, sevenz-rust2, zip writer/reader) because it maximizes format coverage while letting each backend contribute its strongest capability.

## Consequences
- Good: Improves capability coverage and performance/metadata quality where native crates are stronger.
- Bad: More integration surfaces, duplicated extraction logic patterns, and backend inconsistency risks that require careful testing.
