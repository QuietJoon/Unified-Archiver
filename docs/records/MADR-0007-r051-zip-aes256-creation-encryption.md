---
type: ADR
title: "AD: ZIP AES-256 creation encryption"
description: "Superseded by MADR-0027 (as amended 2026-07-20) — the claimed implementation never shipped; creation passwords are rejected today. Successor tracking: OI-0081-006."
tags: [decision, ADR-0007, ADR-0012, R0051-0102, R0051-0111]
timestamp: 2026-04-23T00:00:00Z
status: superseded
---

# AD: ZIP AES-256 creation encryption

Status: Superseded by MADR-0027 (as amended 2026-07-20) — creation passwords are rejected at every boundary today, with an explicit opt-in deferred to OI-0081-006; the implementation claimed below never shipped.

## Context and Problem Statement

Found in Review 0051 (Issues R0051-0102 through R0051-0111, Severity: Low).
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

## Amendment (2026-08-04, decision-review-2026-07-19 §B — superseded by MADR-0027 as amended)

This record is **superseded by MADR-0027** ("This Library Does Not Produce Encrypted Archives"),
as amended 2026-07-20, and its implementation claim is not supported by the evidence. The
"Implementation" section above — a `password` field on `ZipWriter`, an `apply_encryption()`
wrapper over `SimpleFileOptions::with_aes_encryption`, five updated call sites, and the tests
`test_zip_writer_encrypted_roundtrip` / `test_zip_writer_encrypted_wrong_password_fails` —
describes code that **no committed tree ever contained**. Review 0057, days after the claimed
"Implemented (2026-04-15)" date, found `CompressionOptions.password` never consulted during ZIP
creation and no `with_aes_encryption` call in the writer (MADR-0022), and MADR-0027 recorded the
claim as "never fully realized". Git history corroborates: no commit ever added or removed
`apply_encryption` or a writer-side `with_aes_encryption`, and the first commit to introduce the
string "password" into `src/ffi/zip_writer.rs` at all is the 2026-04-18 change that added the
creation-password **rejection** (before it, `ZipWriter::create` opened the output file with no
password handling whatsoever). The accurate history is therefore **accepted-but-never-shipped**,
not shipped-then-regressed. The only `with_aes_encryption` call in today's tree is
`build_aes_zip` in `src/ffi/zip_wrapper.rs` — a test fixture helper that builds encrypted
archives for *read-side* coverage.

Current behaviour is the inverse of the Decision Outcome above — creation passwords are
**rejected** at three layers:

- `src/options.rs::CompressionOptions::validate_for_format` rejects `password.is_some()` for
  every format with `OperationBlocked`, and `src/creation.rs::Archive::create` calls it before
  constructing any backend;
- `src/ffi/zip_writer.rs::ZipWriter::create` rejects ZIP creation passwords with the same
  variant;
- `src/ffi/libarchive_wrapper/writer.rs::LibarchiveArchive::create` rejects them for all other
  formats, and the typed builders (`ZipCompressionOptions`, `SevenZCompressionOptions`) expose
  no password setter at all (R0081-0005).

The `aes-crypto` feature remains enabled in `Cargo.toml` solely because read-side decryption
relies on it. Full chain: this record accepted ZIP AES-256 creation (never shipped) →
MADR-0022 tracked the gap as OI-0057-001 → MADR-0027 resolved OI-0057-001 by rejecting
encrypted creation at every boundary → MADR-0027's 2026-07-20 amendment reversed the permanent
rejection to **deferred behind an explicit opt-in**, tracked as OI-0081-006 (OPEN in
`docs/project/open-issues.md`). Whether and how that opt-in ships is OI-0081-006's owner
decision; nothing in this record governs it. Naming note: MADR-0022 and MADR-0027 cite the
AES-creation acceptance as "AD 0007" (pre-2026-07-23 numbering); after the renumber that
acceptance is this record — MADR-0007, old gate-0007 — distinct from AD 0007 (the dual-ZIP
read-backend strategy, which never claimed creation-side encryption and whose dual arrangement
was itself retired by DCR-009).
