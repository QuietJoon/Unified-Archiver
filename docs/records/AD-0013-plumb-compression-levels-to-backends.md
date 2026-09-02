---
type: ADR
title: "AD: Plumb compression levels through to ZIP and 7z creation backends"
description: "Implemented"
tags: [decision, ADR-0013, R0025-0017, R0025-0018]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Plumb compression levels through to ZIP and 7z creation backends

## Context and Problem Statement
Found in Review 0025 (Issues R0025-0017, R0025-0018, Severity: MEDIUM).
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

## Amendment (2026-07-20, R0081 I8 — already satisfied)

The R0081 design review's innovation I8 proposed making a refused libarchive compression-level
option "loud instead of silently swallowed." On inspection that premise is **stale**: the option
failure is already surfaced as a hard error, and has been since **R0070-0041**. In
`LibarchiveArchive::create` (now `src/ffi/libarchive_wrapper/writer.rs` after the AD 0056 D9 split),
any non-`ARCHIVE_OK` return from `archive_write_set_filter_option` / `archive_write_set_format_option`
— including `ARCHIVE_WARN` ("option ignored") — frees the native handle and returns
`ArchiveError::Format`, using the same error-return pattern as every other libarchive setup failure
in the backend. That is *stronger* than the warning I8 suggested, so no code change was made.

The stale artifact was this record's own Consequences bullet ("if libarchive doesn't support a
particular option string, it returns an error that we ignore … non-fatal"), which described
pre-R0070-0041 behavior and was never updated. Treat that bullet as superseded: a refused
compression-level option is a hard `Format` error, not ignored. I8 is closed as already-satisfied.

## Amendment (2026-09-03, ti-9909d449 — the "Libarchive ZIP" bullet documents a path nobody takes)

The Implementation bullet **"Libarchive ZIP: Uses `archive_write_set_format_option("zip",
"compression-level", N)`"** describes code that exists but is not on any facade call path, and
has not been since ZIP creation was given its own backend.

`Archive::create` (`src/creation.rs`) matches `ArchiveFormat::Zip` to `ZipWriter` — the `zip`
crate — and routes only the remaining formats to `LibarchiveArchive::create`. So the ZIP arms in
`src/ffi/libarchive_wrapper/writer.rs` (`archive_write_set_format_zip`, and the `Zip | SevenZip`
compression-level option) are never reached from the public facade. The level mapping a caller
actually gets for ZIP is the `ZipWriter` one in the first Implementation bullet
(Store→Stored, Fastest→Deflated/1, Fast→3, Normal→6, Maximum→8, Ultra→9); the libarchive mapping
in the second bullet is a parallel implementation of the same table that no `Archive::create` call
exercises. Read that bullet as describing `LibarchiveArchive::create`'s own behaviour, not the
library's ZIP behaviour.

The decision itself is unchanged and stays `active`: levels *are* plumbed to both creation
backends, and the 7z and TAR.* bullets are accurate. Only the ZIP row's implied reachability was
wrong.

Per the 2026-09-03 ruling the dead ZIP arms were **kept, not deleted**. They sit in an FFI wrapper
whose `create` entry point is publicly reachable through the `ffi` module, so an external caller
can still pass `ArchiveFormat::Zip` and get a correctly configured native writer rather than an
opaque `write_header` failure. Both arms now carry comments saying the path is deliberately not
wired, so a future reader does not connect `creation.rs` to it on the assumption that it is the
library's ZIP writer — doing so would bypass everything the ZIP backend owns (the MADR-0027
encrypted-creation refusal, the namespace tracker, CRC handling). Moving ZIP creation to
libarchive would need its own record.
