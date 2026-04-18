# AD: ZIP AES-256 creation encryption

## Context and Problem Statement

Found in Review 051 (Issues R051-102 through R051-111, Severity: Low).
Location: `src/ffi/zip_writer.rs`; `src/options.rs` (`CompressionOptions`).

Multiple documentation locations claimed encryption was unsupported, but the
`zip` crate's `aes-crypto` feature was already enabled in `Cargo.toml`. The
`CompressionOptions` struct already has a `password: Option<String>` field
that was not wired to any backend. The gap between documented surface and
actual capability was flagged by 10 related review issues.

## Decision Drivers

* The `zip` crate provides AES-256 encryption via `with_aes_encryption()`.
* The `aes-crypto` feature was already enabled — no new dependency.
* Only ZIP creation supports encryption through the `zip` crate; other
  format backends (libarchive, 7z, RAR) do not expose creation-time
  encryption APIs.
* The `password` field in `CompressionOptions` was already public API surface.

## Considered Options

1. Wire `CompressionOptions.password` to `ZipWriter` only; document that
   creation-time encryption is ZIP-only (AES-256).
2. Implement encryption for all backends that support it (ZIP + 7z).
3. Remove the `password` field and document encryption as unsupported.

## Decision Outcome

**ACCEPT** — Option 1.

ZIP creation now applies AES-256 encryption when `CompressionOptions.password`
is `Some`. Other formats reject the password field with `UnsupportedOperation`
(corrected in AD 0012). Documentation updated to state "creation-time encryption
is ZIP-only (AES-256); other formats do not support creation encryption."

Status: Implemented (2026-04-15).

### Implementation

`src/ffi/zip_writer.rs`:
- Added `password: Option<String>` field to `ZipWriter` struct, populated
  from `CompressionOptions.password` during `create()`.
- Added `apply_encryption()` static function that wraps
  `SimpleFileOptions.with_aes_encryption(AesMode::Aes256, pw)` when a
  password is set, returning `FileOptions<'a, ()>`.
- All five call sites (`add_file_from_data`, `add_file_from_data_with_metadata`,
  `add_file_from_reader_with_metadata`, `add_file_from_path`,
  `add_directory_entry`) updated to apply encryption.
- Two new tests: `test_zip_writer_encrypted_roundtrip` and
  `test_zip_writer_encrypted_wrong_password_fails`.

## Consequences

* Good — the existing `password` field in public API now does something for
  ZIP archives; previously it was dead surface.
* Good — AES-256 is the strongest option available in the ZIP specification.
* Bad — encryption is ZIP-only; users expecting encryption for 7z or other
  formats get silent no-op (password ignored). This is documented.
* Bad — encrypted ZIPs cannot use streaming extraction through the libarchive
  backend; they must use the `zip` crate path.
