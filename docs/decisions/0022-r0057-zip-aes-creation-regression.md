# AD: AD 0007 ZIP-AES Creation Regression — Track as Open Issue

## Context and Problem Statement

Found in Review 0057 (Issues R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093; Severity: High).
Location: `src/ffi/zip_writer.rs::add_file_from_data`, `src/lib.rs` rustdoc capability table, `docs/architecture/decisions/0007-dual-zip-backend-strategy.md`.

AD 0007 claims the native ZIP backend supports AES-256 creation encryption via the `zip` crate's `aes-crypto` feature. Review 0057 found that `CompressionOptions.password` is never consulted during ZIP creation — no `FileOptions::with_aes_encryption` call exists in the writer. Encryption silently does not happen.

The libarchive (non-ZIP) path has now been corrected to reject `password.is_some()` with `operation_blocked` (R0057-0005). The ZIP path still silently ignores the password.

## Decision Drivers

* AD 0007 creates a user-visible contract. Silently failing violates it.
* Wiring `with_aes_encryption` is a multi-file code change touching tests, examples, and the rustdoc example — too big to take as a drive-by from a decision-gate pass.
* The rustdoc example and capability table can be repaired in the same pass so users do not continue to believe encryption works.

## Considered Options

1. Wire `with_aes_encryption` through `zip_writer.rs` in this gate pass.
2. Gate-reject the regression and record an OPEN issue with repro + two implementation options (wire it OR reject at `Archive::create`) for a dedicated follow-up PR.
3. Quietly remove AD 0007's encryption claim and update docs only.

## Decision Outcome

**ACCEPT as an Open Issue (Option 2).** Code scope is too large for a drive-by; the correct next step is a focused PR that either wires `with_aes_encryption` or rejects the option at the facade. The rustdoc example has been updated (password removed, `⚠️§` footnote added) so users are no longer misled in the interim.

Status: Tracked in `reviews/Open_Issues.md` as OI-0057-001.

### Implementation

- `src/lib.rs` — rustdoc Create example had `password: Some("secret".to_string().into())` removed (R0057-0006, 0092).
- `src/lib.rs` — format capability table ZIP row changed from `✅ Encryption` to `⚠️§` with a new footnote stating creation encryption is not currently wired (R0057-0093).
- `src/ffi/libarchive_wrapper.rs` — non-ZIP creation now returns `operation_blocked` instead of silently discarding the password (R0057-0005, 0082).
- ZIP writer wiring deferred to OI-0057-001.

## Consequences

* Good, because users no longer see a documented "encryption supported" claim that the code does not honor.
* Good, because the non-ZIP path now fails loudly when a password is supplied.
* Bad, because ZIP creation still accepts a password silently. The mitigation is the documentation caveat; the OPEN issue forces a follow-up.
* Good, because a dedicated PR can add the full test matrix (round-trip encrypt/decrypt, wrong-password failure, open-encrypted on freshly-created archive) rather than piecemeal fixes.
