# AD: Reject non-ZIP creation password at facade boundary

## Context and Problem Statement
Found in Review 0001 (Issue R0001-0006, Severity: High).
Location: `src/ffi/libarchive_wrapper.rs:747`

AD 0007 documented creation-time encryption as ZIP-only, but the libarchive
`create()` path unconditionally called `archive_write_set_passphrase()` for
every format when `CompressionOptions.password` was present. This delegated
password semantics to libarchive's undefined per-format behavior instead of
enforcing the library's stated contract.

## Decision Drivers
* AD 0007 explicitly states: "creation-time encryption is ZIP-only"
* ZIP creation routes through `ZipWriter` (native `zip` crate), not libarchive
* libarchive's `archive_write_set_passphrase()` behavior is undefined for TAR and 7z creation
* Silent no-op or undefined encryption is worse than a clear error

## Considered Options
1. Return `Err(UnsupportedOperation)` when password is set for non-ZIP libarchive creation
2. Silently ignore the password (current AD 0007 text says "ignore")
3. Wire libarchive passphrase for formats that support it (7z)

## Decision Outcome
ACCEPT (Option 1): `LibarchiveArchive::create()` now returns `Err(UnsupportedOperation)`
when `options.password` is `Some`. Since all non-ZIP creation routes through libarchive
and ZIP creation routes through `ZipWriter`, this enforces AD 0007's contract at the
facade boundary.

Status: Implemented

### Implementation
- `src/ffi/libarchive_wrapper.rs`: Replaced `archive_write_set_passphrase()` call with
  early `Err(UnsupportedOperation)` return when `options.password.is_some()`

## Consequences
* Good, because the stated contract (ZIP-only creation encryption) is now enforced
* Good, because callers get a clear error instead of undefined backend behavior
* Bad, because callers who relied on passphrase being silently passed to libarchive will now get errors — this is the correct behavior
