---
type: ADR
title: "AD: Non-ZIP Creation Rejects Passwords With `OperationBlocked`"
description: "Superseded by MADR-0027, which generalised the rejection to every format including ZIP; the rejection site itself survives. See the 2026-08-07 amendment."
tags: [decision, ADR-0024, ADR-0012, R0057-0005, R0057-0082, OI-0057-001]
timestamp: 2026-04-17T00:00:00Z
status: superseded
---

# AD: Non-ZIP Creation Rejects Passwords With `OperationBlocked`

## Context and Problem Statement

Found in Review 0057 (Issues R0057-0005, R0057-0082, Severity: Critical).
Location: `src/ffi/libarchive_wrapper.rs` — `create_with_options()` flow.

`CompressionOptions.password` was forwarded into libarchive's `archive_write_set_passphrase` for every non-ZIP format. libarchive silently ignores the passphrase for TAR, GZIP, BZ2, XZ, and ISO. The caller received a successful result from `Archive::create`, shipped the archive, and later found out it was not encrypted.

AD 0012 already rejects passwords at archive *open* time for non-ZIP formats. Creation lacked the symmetric guard.

## Decision Drivers

* Silent encryption failure is a security defect; the loudest available error is the right response.
* Symmetry with AD 0012 is easier to reason about than an asymmetric "read rejects, write accepts-and-ignores" policy.
* Hard-fail is compatible with the planned ZIP creation encryption wiring (OI-0057-001) because ZIP goes through a different writer.

## Considered Options

1. Hard-fail creation at the libarchive boundary when `password.is_some()` for non-ZIP formats.
2. Ignore silently (status quo) and document the limitation.
3. Attempt to encrypt post-hoc with an external tool invocation.

## Decision Outcome

**ACCEPT Option 1.** The libarchive wrapper now returns `ArchiveError::operation_blocked("create", "Creation-time encryption is not supported for {format:?}; only ZIP supports encrypted creation")` when a password is supplied to any non-ZIP format.

Status: Superseded by MADR-0027 — the only-ZIP carve-out recorded here was abolished when MADR-0027 rejected creation passwords for every format; see the 2026-08-07 amendment.

### Implementation

```rust
if options.password.is_some() {
    archive_write_free(archive);
    return Err(ArchiveError::operation_blocked(
        "create",
        format!(
            "Creation-time encryption is not supported for {:?}; only ZIP supports encrypted creation",
            format
        ),
    ));
}
```

Placed before `archive_write_set_passphrase` so the free-on-error happens without handle leaks.

## Consequences

* Good, because a silent security regression is converted into a loud error.
* Good, because AD 0012's open-time policy is now symmetric with the create-time policy.
* Bad, because any downstream tooling that passed a password "just in case" now errors. Mitigation: the error message names the only format that accepts creation passwords (ZIP).

## Amendment (2026-08-07, indy-review-prune — superseded by MADR-0027)

**Superseded by MADR-0027** ("This Library Does Not Produce Encrypted Archives"), which generalised
this record's rejection from "every non-ZIP format" to **every format including ZIP** — abolishing
the carve-out this record's error message named ("only ZIP supports encrypted creation"). The
rejection site itself survives and still fires: `src/ffi/libarchive_wrapper/writer.rs::
LibarchiveArchive::create` returns `operation_blocked` before allocating any libarchive state, but
its message is now MADR-0027's ("Encrypted archive creation is not supported by this library
(format={:?}). Use open_encrypted() to read existing encrypted archives.") and the first line of
defence sits earlier, in `src/options.rs::CompressionOptions::validate_for_format` invoked by
`Archive::create`. The loud-failure-over-silent-ignore rationale recorded here, and the AD 0012
read/write symmetry argument, remain the reasoning behind that gate — they carried forward into
MADR-0027 rather than being reversed. Chain: MADR-0012 (facade-boundary rejection for non-ZIP) →
this record (backend-boundary rejection, non-ZIP) → MADR-0027 (every format; amended 2026-07-20 to
a deferred opt-in, tracked as OI-0081-006). Cross-refs: MADR-0027 (successor); MADR-0012
(predecessor, superseded by MADR-0024 per its own §B amendment); OI-0081-006 (live end of the
chain).
