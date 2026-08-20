---
type: ADR
title: "AD: AD 0007 ZIP-AES Creation Regression — Track as Open Issue"
description: "Superseded by MADR-0027 (as amended 2026-07-20) — OI-0057-001 resolved 2026-04-18; successor tracking: OI-0081-006."
tags: [decision, ADR-0022, ADR-0007, R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093, OI-0057-001]
timestamp: 2026-04-23T00:00:00Z
status: superseded
---

# AD: AD 0007 ZIP-AES Creation Regression — Track as Open Issue

Status: Superseded by MADR-0027 (as amended 2026-07-20) — OI-0057-001 was resolved 2026-04-18 by rejecting encrypted creation outright; the deferred opt-in successor is OI-0081-006.

## Context and Problem Statement

Found in Review 0057 (Issues R0057-0005, R0057-0006, R0057-0082, R0057-0092, R0057-0093; Severity: High).
Location: `src/ffi/zip_writer.rs::add_file_from_data`, `src/lib.rs` rustdoc capability table, `AD-0007-dual-zip-backend-strategy.md`.

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

Status: Tracked in `docs/project/open-issues.md` as OI-0057-001.

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

## Amendment (2026-08-04, decision-review-2026-07-19 §B — superseded by MADR-0027 as amended)

The open-issue routing this record ruled for has completed its lifecycle, and the record is
**superseded by MADR-0027** ("This Library Does Not Produce Encrypted Archives"), as amended
2026-07-20. OI-0057-001 was RESOLVED 2026-04-18 (`docs/project/open-issues-resolved.md`): the
follow-up took the "reject at `Archive::create`" branch of this record's Option 2 rather than
the wiring branch, and MADR-0027 made that rejection policy for every format. Today
`src/options.rs::CompressionOptions::validate_for_format` rejects `password.is_some()` for any
format with `OperationBlocked` (invoked by `src/creation.rs::Archive::create` before backend
construction), `src/ffi/zip_writer.rs::ZipWriter::create` rejects ZIP creation passwords, and
`src/ffi/libarchive_wrapper/writer.rs::LibarchiveArchive::create` rejects the rest — the
`libarchive_wrapper.rs` this record cites has since been split into `reader.rs`/`writer.rs`
(AD 0056 amendment).

One framing correction: "regression" in this record's title implies the capability once worked;
the evidence says it never did. Git history contains no commit that ever wired
`with_aes_encryption` or any password handling into `src/ffi/zip_writer.rs` — the first commit
to touch a password there is the 2026-04-18 rejection itself — and MADR-0027 records the claim
as "never fully realized". This record's own finding ("no `FileOptions::with_aes_encryption`
call exists in the writer") was the first accurate statement of that; the underlying defect was
a **false implementation claim in MADR-0007 (old gate-0007), not a code regression**.

MADR-0027's 2026-07-20 amendment subsequently reversed the permanent rejection to **deferred
behind an explicit opt-in**, tracked as OI-0081-006 (OPEN) — the successor to this record's
OI-0057-001. Full chain: MADR-0007 accepted ZIP AES-256 creation (never shipped) → this record
found the gap and opened OI-0057-001 → MADR-0027 rejected encrypted creation and resolved
OI-0057-001 → the MADR-0027 amendment deferred it behind an explicit opt-in (OI-0081-006,
undecided). Naming note: this record's "AD 0007" references — including the
`AD-0007-dual-zip-backend-strategy.md` location citation — predate the 2026-07-23 renumber; the
AES-creation acceptance actually lives in MADR-0007 (old gate-0007), while the dual-ZIP AD 0007
never claimed creation-side encryption and its dual arrangement was itself retired by DCR-009.
