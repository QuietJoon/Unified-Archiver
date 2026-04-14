# AD: Unified Stream Checksum Extraction

Status: Accepted

## Context and Problem Statement
Consumers need to verify single-file compressed formats (GZIP, BZIP2, XZ) without full decompression or archive-level metadata. Relying on libarchive verification requires reading the entire stream, which defeats the purpose of a fast integrity check.

## Decision Drivers
- Fast integrity checks for non-archive compressed containers
- No full stream read required for checksum retrieval
- Support for format-native checksums (CRC32, CRC64, etc.)

## Considered Alternatives
- **Rely solely on libarchive verification** -- rejected because it requires a full stream read, negating the speed advantage of direct checksum extraction.

## Decision Outcome
We decided to implement `stream_crc.rs` to parse trailers and headers for format-native checksums (CRC32, CRC64, etc.) because it enables fast integrity checks using internal format metadata without decompressing the full stream.

## Consequences
- Good: Enables fast integrity checks using internal format metadata without full decompression.
- Bad: Requires manual bit-level parsing of format trailers and headers; full XZ checksum extraction still requires additional parsing work beyond what is currently implemented.
