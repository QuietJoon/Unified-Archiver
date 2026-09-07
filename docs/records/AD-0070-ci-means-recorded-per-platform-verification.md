---
type: ADR
title: "AD 0070: CI means recorded per-platform verification, not a hosted service"
description: "Accepted (2026-09-03, owner ruling) — this project adopts no hosted CI of any kind. A property is CI-covered when a committed command is run by the owner on a host that can observe it and the native exit status is recorded against a source fingerprint. Three of the four items blocked on \"no CI\" were never blocked on a service; only Windows needs a different machine."
tags: [decision, ADR-0070, project-control, ci, verification]
status: active
---

# AD 0070: CI means recorded per-platform verification, not a hosted service

## Status

Accepted (2026-09-03) — owner ruling, in the owner's own words: *"I have no
host-independent CI. No hosted CI. So I would run CI for each OS
machine/platform. We would check/record it when we run each platform."*

## Why this record has to exist

Four separate work items were blocked on the phrase "no CI", and nobody had
defined it. Worse, the paper trail pointed both ways: MADR-0029's
owner-reversal amendment and AD-0046 both say the project has "committed
Windows CI", and `docs/architecture/decision-review-2026-07-19.md` states that
*"CI is out of scope" no longer holds*. Meanwhile the owner's actual standing
ruling — no GitHub or GitLab CI — appeared in no record at all. Anyone reading
the repository to decide whether an item was blocked would have found two
answers and neither of them the owner's.

That is the failure this record closes. It is not a process document; it is a
definition that makes four existing blockers decidable.

## The decision

**Adopt no hosted CI.** Not GitHub Actions, not GitLab CI, not a rented
runner as a general mechanism. The owner runs verification on each target
platform, on machines they control, and the result is recorded here.

**"CI-covered" is a property of evidence, not of a vendor.** A property `P` is
CI-covered when all four hold:

1. **Committed recipe.** The exact command that checks `P` is committed and
   re-runnable from a clean checkout. Not remembered, not reconstructed from a
   commit message.
2. **Observing host.** It runs on a host that can actually observe `P`. A
   macOS run cannot observe whether libarchive links on Windows, and no amount
   of scripting changes that.
3. **Native terminal status.** The result is the command's own exit status,
   recorded *after* the command terminates. A missing marker is `lost`, never
   green — the rule the project already follows for its own gates.
4. **Fingerprint-bound.** The record names the commit and the dirty state it
   was produced from. A green record for commit X says nothing about commit Y.

**The consequence that matters:** condition 2 is the only one a script cannot
satisfy. So for any blocker citing "no CI", the question is which condition it
fails. If it fails only 1 or 3, it needs *repeatability* and is not blocked. If
it fails 2, it needs a *different machine* and genuinely is.

## Applying it to the four blockers

- **The `v2-api` default-feature flip.** `Cargo.toml`'s comment forbids
  flipping "until the read-minimal and full profiles have CI coverage, and this
  repository has no CI". But `v2-api` gates no platform-specific code — it
  gates a module, two `cfg` sites and a set of in-crate tests. Both sides of the
  flip are observable on the dev host. This fails condition 1 only: the
  `--no-default-features` side has simply never been run. **Not blocked;
  restate.**
- **AD 0058 Stage 1's feature matrix.** Every profile compiles and tests on
  macOS; the one host-dependent sub-property (libarchive discovery differing per
  OS) belongs to OI-0065-001, not to Stage 1. Fails condition 1. **Not blocked;
  restate.**
- **Cross-compilation smoke (OI-0080-001).** Blocked by the owner's standing
  pre-v2 scope ruling, which is a different and still-live reason. Recording it
  as "no CI" hid the real blocker behind a false one. **Blocked, on the correct
  ground.**
- **Windows libarchive discovery (OI-0065-001).** Three sub-properties: the
  discovery mechanism is a design decision and host-independent; the
  `cfg(windows)` arms can be *compiled* here (and are owner-invoked only, per the
  2026-09-01 hold); but libarchive actually linking, and the Windows tests
  actually running, need Windows. **Genuinely blocked — and this is the only one
  of the four that is.** The owner's Windows machine is the host; verification is
  pending on it.

## What this supersedes

MADR-0029's amendment and AD-0046 both describe "committed Windows CI" as a
thing this project has or is committed to. Under this ruling there is no
committed CI job and there will not be one. Both records are amended to point
here. **Their substantive decisions are untouched** — MADR-0029's rejection of
the scratch-path portability sweep and AD-0046's rejection of the Windows
support-messaging downgrade both stand on their own reasoning. Only the
mechanism they assumed for verification changes.

`decision-review-2026-07-19.md` R6 is a dated review note rather than a record;
it is left as written, and this record is the later ruling.

## What "release-verified" now means

A claim that a platform is release-verified means: a record exists under
`docs/verification/` whose fingerprint matches the released commit, showing the
lanes green **on that platform**. Absence of a record is not a failure — it is
an absence, and the platform's status must say so rather than implying
coverage. This is why README's current "macOS and Linux tested" is held to a
weaker standard than its Windows wording: no Linux run is recorded anywhere
either.

## Consequences

- Three items stop citing a blocker they never had, and one states its real one.
- "No CI" stops being a reason. Either a lane exists and was run, or the record
  says which host it needs.
- The cost is honesty about coverage: platforms without a record are visibly
  unverified, rather than quietly assumed.
- The dev host cannot observe Windows or Linux behaviour, so those platforms
  stay unverified until the owner runs them. That is a real gap and this record
  makes it visible rather than closing it.

## Related

AD-0058 (feature-first footprint split — its ordering constraint 2 is what the
`v2-api` comment borrows), AD-0046 (Windows support messaging), MADR-0029
(scratch-path portability), OI-0065-001, OI-0080-001.


## Amendment (2026-09-08, both "fails condition 1" verdicts are now satisfied)

This record's condition 1 is "a committed recipe". Two of the bullets above were
recorded as failing it. Both now pass, and the lane vocabulary the record assumes has
changed underneath it — noted here rather than edited above.

**The `v2-api` flip.** `v2-api` has been in `default` since 2026-09-03, so the
prohibition this record quotes is retired. The prohibition text is still present in
`Cargo.toml`, but only as a past-tense quotation of what the comment used to say,
followed by the reasons it was wrong.

**AD-0058 Stage 1's feature matrix.** Condition 1 is satisfied as of 2026-09-06/07:
the six profile lanes and the five per-feature isolation lanes are committed in
`scripts/release-gate.sh` and were run green — read-minimal 1137, read-zip 1338,
read-all-formats 1720, create 1487, modify 1611, full 2127 tests, 0 failed in every
one. Stage 1 Required Actions 1–5 are complete; only RA 6 (facade crates) remains, and
it is parked.

**A lane this record's readers will look for no longer exists.** The AD-0058 operation
split made every backend feature-gated, so a bare `--no-default-features` selects no
backend at all and `src/lib.rs` refuses it with a `compile_error!`. The
`test-no-default-features` lane was therefore removed on 2026-09-07 and replaced by the
six profile lanes; the floor is `read-minimal` (`--features read,zip-read`), never the
empty set. Every per-feature isolation lane now carries `read,zip-read` for the same
reason.

That change strengthens rather than weakens this record's argument: two of the five
isolation lanes found real defects on their first run — `rar-support` a coverage loss
predating the split, `sevenzip` an undocumented cross-feature dependency — neither of
which either extreme lane could reach.
