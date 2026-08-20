---
type: ADR
title: "AD: Reject non-ZIP creation password at facade boundary"
description: "Superseded by MADR-0024 — the UnsupportedOperation guard recorded here never shipped; creation passwords are rejected today with OperationBlocked at three layers, and MADR-0027 (as amended 2026-07-20, OI-0081-006) governs the encrypted-creation question overall."
tags: [decision, ADR-0012, ADR-0007, R0001-0006]
timestamp: 2026-04-16T00:00:00Z
status: superseded
---

# AD: Reject non-ZIP creation password at facade boundary

Status: Superseded by MADR-0024 — the creation-password hard-fail shipped 2026-04-18 as `OperationBlocked`, not the `UnsupportedOperation` recorded here; the encrypted-creation question overall is governed by MADR-0027 as amended 2026-07-20 (OI-0081-006). See the §B amendment below.

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

## Amendment (2026-08-05, decision-review-2026-07-19 §B — superseded by MADR-0024)

The 2026-07-19 review marked this record superseded because MADR-0024 replaced its error variant;
verification confirms that, and adds a correction: **the Implementation recorded above never
shipped**. Git history over `src/ffi/libarchive_wrapper.rs` shows the unconditional
`archive_write_set_passphrase` forwarding was present from the initial commit until 2026-04-18,
when commit `026f4b2` removed it and introduced the first creation-password guard on that path —
already as `ArchiveError::operation_blocked`, citing AD 0027. No commit ever contained the early
`Err(UnsupportedOperation)` return this record claims (the same accepted-but-never-shipped pattern
MADR-0007's §B amendment documents for the AES-creation acceptance). The variant could not have
survived in any case: `ArchiveError::UnsupportedOperation` was removed by the OI-0001-003 split
(commit `c9b6685`, 2026-04-17 — see MADR-0011's §B amendment). **MADR-0024** (Review 0057,
R0057-0005 / R0057-0082) re-found the live passphrase forwarding this record believed fixed and
re-ruled the hard-fail with `ArchiveError::operation_blocked("create", …)`; that ruling is what
shipped, which is why this record is superseded rather than merely refreshed.

Today the rejection is three-layered and format-blind, not only non-ZIP at the libarchive
boundary:

- `CompressionOptions::validate_for_format` (`src/options.rs`) rejects `password.is_some()` for
  every format with `OperationBlocked` ("encrypted creation … is out of scope (AD 0027)"), and
  `Archive::create` (`src/creation.rs`) calls it before constructing any backend.
- `LibarchiveArchive::create` — now in `src/ffi/libarchive_wrapper/writer.rs` after the wrapper's
  reader/writer split — keeps the backend-level `OperationBlocked` guard, placed before any
  libarchive state is allocated (R0071-0011).
- `ZipWriter::create` (`src/ffi/zip_writer.rs`) rejects ZIP creation passwords with the same
  variant, and the typed `SevenZCompressionOptions` builder exposes no password setter at all
  (R0081-0005).

The encrypted-creation question overall is now governed by **MADR-0027**, whose 2026-07-20
amendment reversed the permanent rejection to a **deferred explicit opt-in**, tracked as
OI-0081-006 (OPEN in `docs/project/open-issues.md`). Whether and how that opt-in ships — and which
formats keep rejecting — is OI-0081-006's owner decision; nothing in this record governs it.
