---
type: ADR
title: "AD 0042: `password_as_str` returns `Result`; reject non-UTF-8 passwords"
description: "Review 0062 R0062-0005 flagged that options::passwordasstr() — the helper that converts Option<SecStr> into Option<&str> for backend FFI calls —…"
tags: [decision, ADR-0042, R0062-0005, R0059-0014]
timestamp: 2026-04-18T00:00:00Z
status: active
---

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

## Amendment (2026-07-22, R0081 I5 — fallible accessor superseded by construction-time validation)

The R0081 design review's innovation I5 introduced a crate-owned [`Password`] newtype
(`src/password.rs`, re-exported as `unified_archive::Password`). This **supersedes the
Implementation and call-site machinery of this record** while preserving its core decision — a
non-UTF-8 password must never be silently dropped.

What changed:

* **Validation moved from the accessor to the constructor.** `Password` can only be built from
  `impl Into<String>` (`Password::new`, and `From<String>`/`From<&str>`/`From<&String>`), so the
  wrapped bytes are UTF-8 *by construction*. The invalid-UTF-8 case this record guarded against is
  now **unrepresentable through the public API** — there is no longer a byte-oriented entry point
  for a password to reach a backend.
* **The fallible `password_as_str` accessor is gone.** Its `Result<Option<&str>, ArchiveError>`
  signature and all ~17 `?` call sites (`extraction.rs`, `ffi/wrapper.rs`, `ffi/zip_wrapper.rs`,
  `ffi/sevenz_wrapper.rs`, `external/rar.rs`) are replaced by the **infallible**
  `Password::as_str(&self) -> &str` — read at call sites as `self.password.as_ref().map(Password::as_str)`.
  There is no decode step left to fail, so the surrounding scopes no longer thread an error for it.
* **`secstr` no longer leaks.** The two `pub password: Option<SecStr>` fields on
  `ExtractionOptions` and `CompressionOptions` became `Option<Password>`, and the internal wrapper
  fields (`UnrarArchive`, `ZipArchive`, `SevenZArchive`, `RarCreator`) likewise. `SecStr` is now an
  implementation detail wholly inside `Password`; it appears in no public signature or field. Both
  option structs also gained a `.password(impl Into<String>)` builder-setter (a small step toward
  OI-0076-005, without adopting its full encapsulation).

What is retained:

* The `ArchiveError::Password` variant stays. It remains the family for genuine password failures
  (wrong password, encryption-required, null byte in a RAR password). The specific
  `"password bytes are not valid UTF-8"` message this record added is now **internal-only** — it is
  no longer reachable via the public API because construction validates UTF-8; it would only matter
  to a hypothetical future legacy byte-oriented path (see the Revisit trigger's first bullet).

The Implementation section's `password_as_str` snippet and its "Call-site migration" list describe
the pre-I5 mechanism; treat them as historical. The Consequences bullet noting the accessor
signature changed from `Option<&str>` to `Result<Option<&str>, ArchiveError>` is likewise
superseded — the accessor is now `Password::as_str -> &str`.

Cross-links: `src/password.rs`; OI-0076-005 (encapsulate public-field structs — the `.password`
builders and the newtype are a partial, pre-1.0 down-payment on that issue);
`docs/architecture/decision-review-2026-07-19.md` (I5).
