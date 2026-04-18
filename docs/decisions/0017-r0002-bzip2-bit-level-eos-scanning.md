# AD: Bit-level BZIP2 EOS marker scanning

## Context and Problem Statement
Found in Review 0002 (Issue R0002-0026, Severity: Medium).
Location: `src/stream_crc.rs:118`, `tests/stream_crc_test.rs:31`

`extract_bzip2_stream_crc()` scanned the last 1KB of a BZIP2 file for the
48-bit EOS marker (`0x177245385090`) using byte-aligned pattern matching.
BZIP2 is a bit-oriented format, so the EOS marker can appear at any bit
offset — not just on byte boundaries. The integration test against the real
fixture was `#[ignore]`-gated because the byte-aligned scanner failed.

## Decision Drivers
* The existing scanner produced false negatives on real BZIP2 files
* BZIP2 block boundaries are bit-aligned, not byte-aligned
* The 32-bit stream CRC immediately follows the EOS marker at the same bit alignment

## Considered Options
1. Implement bit-level scanning that checks every bit position in the tail buffer
2. Keep byte-aligned scanning with documentation noting the limitation

## Decision Outcome
ACCEPT (Option 1): Implement bit-level scanning.

Status: Implemented

### Implementation
- `src/stream_crc.rs`: Added `extract_bits_u64()` helper that extracts N bits
  from a byte buffer at any byte+bit offset. Replaced `find_pattern_last()`
  byte search with a bit-level scan that checks every bit position in the
  tail buffer for the 48-bit EOS magic, then extracts the following 32-bit CRC
  at the same alignment.
- `src/stream_crc.rs`: Added unit tests for `extract_bits_u64()` (aligned and
  shifted cases) and a test constructing a bit-shifted EOS+CRC payload.
- `tests/stream_crc_test.rs`: Removed `#[ignore]` from `test_bzip2_stream_crc_fixture`.

## Consequences
* Good, because `extract_bzip2_stream_crc()` now works on real BZIP2 files
* Good, because the previously-ignored integration test now passes
* Bad, because bit-level scanning is O(8×buffer_size) instead of O(buffer_size),
  but the buffer is only 1KB so the overhead is negligible
