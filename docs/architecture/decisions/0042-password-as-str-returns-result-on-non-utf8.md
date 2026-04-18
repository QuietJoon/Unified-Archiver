# AD 0042: `password_as_str` returns `Result`; reject non-UTF-8 passwords

## Context and Problem Statement

Review 0062 R0062-0005 flagged that `options::password_as_str()` — the helper that converts `Option<SecStr>` into `Option<&str>` for backend FFI calls — silently returned `Ok(None)` when the password bytes were not valid UTF-8:

```rust
// old:
pub(crate) fn password_as_str(password: &Option<SecStr>) -> Option<&str> {
    password
        .as_ref()
        .and_then(|p| std::str::from_utf8(p.unsecure()).ok())
}
```

A caller who constructed `SecStr` from non-UTF-8 bytes — which `SecStr::from_bytes` accepts — would have their password silently *dropped* at the FFI boundary. The backend would then either extract the archive as if no password had been supplied (succeeds on unencrypted archives, fails on encrypted ones with a "password required" error), or attempt decryption with an empty password and fail with a wrong-password error. Neither of those surfaces the real problem: the caller's password bytes were not representable as UTF-8.

This supersedes the softer handling implied by R0059-0014, which asked the helper to "return `Option<&str>` cleanly" without specifying the invalid-UTF-8 policy.

## Decision Drivers

* Silent data loss at a security-relevant API boundary is a correctness bug. Passwords must not be silently discarded.
* All backends this crate wraps (`unrar`, `zip`, `sevenz-rust2`, libarchive) expect UTF-8 passwords. A non-UTF-8 password cannot be used; the only correct action is to refuse it with a clear error.
* The helper is called from 17 call sites spread across `extraction.rs`, `wrapper.rs`, `zip_wrapper.rs`, `sevenz_wrapper.rs`, `external/rar.rs`. All of them already return `Result<_, ArchiveError>`, so propagating a new error via `?` is free.

## Considered Options

1. **Keep silent-drop behaviour and document it.** Rejected: documentation does not prevent the security bug.
2. **Panic on non-UTF-8.** Rejected: a user-supplied password is not a programmer error; it deserves an `Err`, not a panic.
3. **Return `Result<Option<&str>, ArchiveError>`; map invalid UTF-8 to `ArchiveError::Password`.** Accepted.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

### `src/options.rs`

```rust
pub(crate) fn password_as_str(
    password: &Option<SecStr>,
) -> std::result::Result<Option<&str>, ArchiveError> {
    match password.as_ref() {
        None => Ok(None),
        Some(s) => std::str::from_utf8(s.unsecure())
            .map(Some)
            .map_err(|_| ArchiveError::password("password bytes are not valid UTF-8")),
    }
}
```

Test coverage in the same module was updated: the previous `test_password_as_str_non_utf8_returns_none` was renamed to `test_password_as_str_non_utf8_errors` and asserts `matches!(err, ArchiveError::Password { .. })`.

### Call-site migration

All 17 call sites gained a `?`:

* `src/extraction.rs` — 8 call sites (pre-backend dispatch for each extract entry point)
* `src/ffi/wrapper.rs` — 2 call sites (UnRAR header-decryption path)
* `src/ffi/zip_wrapper.rs` — 4 call sites (ZipReader / ZipWriter paths)
* `src/ffi/sevenz_wrapper.rs` — 1 call site (SevenZ password wiring)
* `src/external/rar.rs` — 1 call site (external-rar-create flow)
* Tests — 1 renamed assertion

All surrounding scopes already returned `Result<_, ArchiveError>`, so the migration was mechanical.

## Consequences

* A password constructed from non-UTF-8 bytes now fails immediately with `ArchiveError::Password { message: "password bytes are not valid UTF-8" }` instead of causing a misleading "password required" or "wrong password" downstream.
* The helper signature changed from `Option<&str>` to `Result<Option<&str>, ArchiveError>`. This is a private/`pub(crate)` change — no public API surface moved.
* The error message is in the `Password` family rather than `InvalidInput` because the password family already covers all password-related error surfaces in this crate, and keeping one family minimises the caller's pattern-match burden.

## Supersedes

R0059-0014 (which accepted a weaker cleanup of this helper).

## Revisit trigger

Re-open if:

* A backend is added that accepts non-UTF-8 password bytes directly (unlikely — all current-generation archive formats specify UTF-8 passwords).
* We expose `password_as_str` publicly for some reason (it is currently `pub(crate)` and has no external consumers).
