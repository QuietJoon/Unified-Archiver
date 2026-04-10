# AD: Treat CRC32 value zero as valid, not absent

## Context and Problem Statement
Found in Review 025 (Issues R025-009, R025-010, R025-011, Severity: HIGH/MEDIUM).
Location: `src/ffi/zip_wrapper.rs`, `src/ffi/sevenz_wrapper.rs`, `src/ffi/wrapper.rs`

All three native backends (ZipReader, SevenZ, UnRAR) treated CRC32 value `0` as "absent" and set `entry.crc32 = None`. CRC32 of empty content is legitimately `0x00000000`. This caused:
- Empty files silently dropped from manifest digest calculations
- Integrity checks skipping empty files entirely
- Incorrect metadata reporting

## Decision Drivers
* CRC32(b"") == 0 is mathematically correct — `0` is a valid checksum
* Three backends all had the same bug independently
* Manifest digest and integrity verification both depend on CRC being present

## Considered Options
1. Always store the CRC32 from metadata for file entries (value 0 is valid)
2. Use a sentinel value (e.g., `u32::MAX`) for absent — rejected as fragile
3. Use `Option<Option<u32>>` (None=unknown, Some(None)=absent, Some(v)=present) — rejected as over-engineered

## Decision Outcome
ACCEPT: All backends now unconditionally store CRC32 for file entries. Only directory entries get `None`.

Status: Implemented

### Implementation
- ZipReader: Always `Some(zip_file.crc32())`, removed compute_crc32_reader fallback in listing path
- SevenZ: Removed `entry.crc != 0` guard, always store when fits in u32
- UnRAR: Changed from `if is_directory || header.file_crc == 0 { None }` to `if is_directory { None } else { Some(...) }`
- ZipReader `test_integrity()`: Removed skip for `expected_crc == 0`
- ZipReader `extract_to_memory()`: Added CRC32 verification after read

## Consequences
* Good, because empty files are now correctly tracked in manifests and integrity checks
* Good, because listing is faster (no decompression fallback for CRC computation)
* Neutral: entries that genuinely lack CRC metadata (rare) will show CRC=0 — acceptable since backends provide CRC from central directory/header
