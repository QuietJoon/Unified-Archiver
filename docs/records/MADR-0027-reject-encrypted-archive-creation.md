---
type: ADR
title: "AD: This Library Does Not Produce Encrypted Archives"
description: "Implemented; amended 2026-07-20 — reversed to deferred opt-in."
tags: [decision, ADR-0027, ADR-0007, ADR-0024, R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093]
timestamp: 2026-04-17T00:00:00Z
status: active
---

# AD: This Library Does Not Produce Encrypted Archives

## Context and Problem Statement

Originates from Review 0057 (Issues R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093). Amends AD 0007 ("Dual ZIP backend strategy") which had claimed AES creation encryption as a supported capability.

AD 0007's claim was never fully realized:
* `src/ffi/zip_writer.rs` did not wire `CompressionOptions.password` into `zip::write::FileOptions::with_aes_encryption`.
* `src/ffi/libarchive_wrapper.rs` (pre-2026-04-17) forwarded passwords into libarchive, which silently dropped them for non-ZIP formats.

Rather than plumb encryption through two backends, add round-trip tests, and own the password-handling surface, we scope encryption to *reading* only.

## Decision Drivers

* Owning crypto correctness (password derivation, AE mode selection, tamper detection, side-channel hygiene) is a large ongoing commitment.
* Callers that need encrypted creation have dedicated tools (WinRAR, `zip -e`, GPG wrappers) with audited implementations.
* Read-side encryption is already valuable for the archiver's main job (inspecting and extracting user-supplied archives).
* The dependency on `secstr` still pays off on the read side: passwords are held in zeroizing memory across open/list/extract calls.

## Considered Options

1. **Reject encrypted creation.** `Archive::create` with `password.is_some()` returns `OperationBlocked`. Open/extract with passwords keeps working.
2. **Wire ZIP-AES creation through `zip_writer.rs`.** Adds the `with_aes_encryption` path, plus a test matrix, plus a migration story for the libarchive creation path (which cannot support this).
3. **Silently accept and ignore passwords on creation.** Status quo pre-2026-04-17 — already ruled out as a security defect.

## Decision Outcome

**ACCEPT Option 1.** `CompressionOptions.password.is_some()` is rejected at both ZIP and non-ZIP creation boundaries. The rustdoc capability table marks ZIP Encryption as `📖§` (read-only) with a footnote pointing here.

Status: Implemented; amended 2026-07-20 — reversed to deferred opt-in.

### Implementation

* `src/ffi/zip_writer.rs::create` — checks `compression_options.password.is_some()` before touching the file; returns `ArchiveError::operation_blocked("create", "Encrypted ZIP creation is not supported by this library. Use open_encrypted() to read existing encrypted archives.")`.
* `src/ffi/libarchive_wrapper.rs` (already landed 2026-04-17 via AD 0024) — rejects non-ZIP creation passwords with the same variant.
* `src/lib.rs` rustdoc — capability table cell updated from `⚠️§` to `📖§`, footnote rewritten to cite AD 0027.

### Amendment to AD 0007

AD 0007's claim that the `zip` crate's `aes-crypto` feature supports creation is technically true, but this library deliberately does not expose that capability. The feature is retained in `Cargo.toml` because read-side decryption of AES-encrypted ZIPs relies on it.

## Consequences

* Good, because the security surface is reduced: this library cannot produce a broken or weakly-encrypted archive.
* Good, because the error is loud and actionable — callers learn about the policy at first create attempt rather than discovering a plaintext payload in production.
* Good, because tests and docs can converge on a single narrative ("read encrypted, create plain").
* Bad, because callers who need encrypted creation must shell out to an external tool. Mitigation: the error message points them to `open_encrypted()` for the read case and names the limit explicitly.
* Neutral, because the dependency graph is unchanged (`secstr` and `zip[aes-crypto]` still required for read-side decryption).

## Amendment (2026-07-20, owner decision)

The owner has decided to **REVERSE** the permanent rejection recorded in the Decision Outcome above. Encrypted-archive **creation** is no longer *permanently* rejected — it is now **deferred behind an explicit opt-in**. The blanket rejection at the creation boundary stands only until that opt-in ships; the original Decision Outcome is downgraded from "permanent" to "deferred behind an explicit opt-in."

Why the original premise no longer holds:

* **"Owning crypto is too large a commitment" is no longer accurate.** We would not own the cipher implementations. The `zip` crate (its `aes-crypto` feature is already enabled in this tree for read-side decryption) and `sevenz-rust2` (`aes256` plus header encryption, already a dependency) own the ciphers. Between them they cover encrypted **ZIP** and **7z** creation.
* **"No backend exposes creation encryption" is no longer accurate.** Both crates above expose creation-side encryption APIs; the earlier record conflated "we did not wire it up" with "the capability does not exist."
* **The harder half is already done.** The read side already parses and decrypts the more difficult ciphertext surface (open/list/extract of encrypted archives). Creation is the easier direction.
* **Encrypted-ZIP creation is a mainstream user expectation** across the Win/macOS/Linux target matrix, not a niche request.

Honest counter-cost (why opt-in, not default): the real objection is **AE-2 tool-compatibility and the resulting support burden** — encrypted archives we emit may interoperate poorly with some third-party tools, generating support load. That cost justifies gating the feature behind an explicit opt-in; it does not justify a permanent ban. Encrypted creation therefore stays **off by default** and unavailable until the opt-in lands.

A tracking Open Issue and a TicGit ticket for the opt-in feature are being filed by the coordinator — do not create them from this record.
