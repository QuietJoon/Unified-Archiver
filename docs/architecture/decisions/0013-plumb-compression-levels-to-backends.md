# AD: Plumb compression levels through to ZIP and 7z creation backends

## Context and Problem Statement
Found in Review 025 (Issues R025-017, R025-018, Severity: MEDIUM).
Location: `src/ffi/zip_writer.rs`, `src/ffi/libarchive_wrapper.rs`

`CompressionLevel` enum (Store/Fastest/Fast/Normal/Maximum/Ultra) was accepted by the public API but silently ignored. ZipWriter always used `Deflated` with default level. Libarchive only mapped levels for TAR filter compression, not for ZIP or 7z format compression.

## Decision Drivers
* Public API promises compression level control
* Users choosing `Store` or `Ultra` expect measurably different behavior
* Both the `zip` crate and libarchive support level parameters

## Considered Options
1. Plumb levels through both backends with appropriate mapping
2. Remove `CompressionLevel` from API — rejected as it's a useful feature
3. Document as unsupported — rejected as the backends support it

## Decision Outcome
ACCEPT: Map compression levels to backend-specific parameters in both ZipWriter and libarchive.

Status: Implemented

### Implementation
- ZipWriter: Maps Store->Stored/None, Fastest->Deflated/1, Fast->3, Normal->6, Maximum->8, Ultra->9
- Libarchive ZIP: Uses `archive_write_set_format_option("zip", "compression-level", N)`
- Libarchive 7z: Uses `archive_write_set_format_option("7zip", "compression-level", N)`
- TAR variants: Existing filter-based approach retained

## Consequences
* Good, because `CompressionLevel::Store` now actually stores without compression
* Good, because users get meaningful size/speed trade-offs
* Neutral: if libarchive doesn't support a particular option string, it returns an error that we ignore (format option failure is non-fatal in libarchive)
