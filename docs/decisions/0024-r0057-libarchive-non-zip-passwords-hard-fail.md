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

Status: Implemented.

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
