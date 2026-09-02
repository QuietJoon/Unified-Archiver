---
type: ADR
title: "AD 0046: Reject R0064 Windows-support messaging downgrade cluster"
description: "Rejected R0064-0034..0039's downgrade of Windows-support messaging; ruling reaffirmed under the 2026-07-17 first-class-platform directive (amended 2026-08-04, §B)."
tags: [decision, ADR-0046, R0064-0034, R0064-0039]
timestamp: 2026-04-21T00:00:00Z
status: active
---

# AD 0046: Reject R0064 Windows-support messaging downgrade cluster

## Context and Problem Statement

Review 0064 raised six Medium findings (R0064-0034 through R0064-0039) asking to downgrade every user-facing mention of Windows support from "present but not release-verified" to "not currently supported" or "incomplete," citing two build-script artifacts:

1. `build.rs:52` still emits `Windows libarchive linking will be configured in future implementation`.
2. `build.rs:81` unconditionally invokes `Command::new("make")` for the bundled UnRAR SDK, which a stock Windows environment lacks.

The six findings touch `build.rs:52`, `build.rs:81`, `README.md:17` and `:227`, `README.md:48`, `docs/USER_MANUAL.md:62`, and `Limitations.md:111`.

## Decision Drivers

* Windows is a **real target** for this crate, not an aspirational one: the code compiles on Windows in CI-less ad-hoc verification, and the project has intentionally kept release-time Windows support in the "present but not release-verified" wording to allow downstream integrators to experiment without implying a turnkey build. Downgrading the messaging to "not supported" would preemptively shut a door the project is still holding open.
* The two build-script facts the reviewer cites are accurate but they describe *an incomplete automation path*, not an absence of Windows support. Users who supply their own libarchive and `make` (via MSYS2, Cygwin, or WSL) build and run successfully today — that is what the "present but not release-verified" wording is meant to capture.
* The correct fix for the build-script drift is to finish the Windows automation (gate `make` on platform/tool availability, wire the libarchive path), not to rewrite six downstream doc sites to reflect a policy change that hasn't been made.
* Reopening the messaging every review cycle is churn; a single REJECT record lets future reviewers land here instead of refiling the same cluster.

## Considered Options

1. **Accept and rewrite six doc sites to say "Windows is not supported."** Rejected: overstates the policy, regresses on downstream experimentation, and loses the distinction between "incomplete automation" and "unsupported."
2. **Accept partially — leave README/USER_MANUAL/Limitations wording but silence the `build.rs` warnings.** Rejected: silencing a build-script warning that accurately describes an unfinished automation path trades honesty for noise reduction.
3. **Reject the cluster. Document the policy here and track the real work (finish Windows automation) separately if/when it becomes a priority.** Accepted.

## Decision Outcome

REJECT R0064-0034..0039. Status: Closed by this record; no code or doc edits.

### Implementation

* No changes to `build.rs`, `README.md`, `docs/USER_MANUAL.md`, or `Limitations.md`.
* Ignores entry [`IG-0064-0034..0039`](IG-0064-0034-0039-windows-support-messaging-downgrade-cluster.md) recorded.
* If the Windows build-automation gap becomes a priority later, open a single OI to finish the libarchive link path and gate `make` on availability; the messaging stays as-is until that work lands.

## Consequences

* Good: preserves the project's stated Windows posture; avoids doc churn; concentrates a recurring reviewer concern into one citable record.
* Bad: the `build.rs:52` warning and the README "present but not release-verified" wording will keep appearing to future reviewers as a mismatch until the Windows automation is finished. That is an accepted cost of keeping the door open.

## Related

* `build.rs:52`, `build.rs:81` — the build-script artifacts the cluster cites.
* `README.md:17`, `README.md:48`, `README.md:227` — downstream messaging.
* `docs/USER_MANUAL.md:62`, `Limitations.md:111` — downstream messaging.

## Amendment (2026-08-04, decision-review-2026-07-19 §B — premises refreshed, ruling stands)

The owner's platform directive (Review 0080 gate, 2026-07-17) **partially inverted this record's
premises**: Windows, macOS, and Linux are now all first-class native targets, with
cross-compilation explicitly out of scope pre-v2 — recorded in OI-0080-001's impact statement
("native Windows/macOS/Linux builds are the support matrix") and restated in the MADR-0029
owner-reversal amendment ("first-class Win / macOS / Linux, with committed Windows CI") and in
OI-0081-005. Windows is therefore no longer the "door the project is still holding open" for
downstream experimentation that the Decision Drivers describe; it is a committed target. Of the
two build-script artifacts the R0064 cluster cited, one is gone and one remains:

* The unconditional `Command::new("make")` invocation for the vendored UnRAR SDK was removed by
  Innovation I4 (2026-07-22): `build.rs::build_unrar` now compiles the vendored C++ with the `cc`
  crate, selecting sources and defines from `CARGO_CFG_TARGET_OS` / `CARGO_CFG_TARGET_ENV` with an
  MSVC-compatible Windows source set, and the Review-0065-era Windows `panic!` fallback is gone
  (see the OI-0080-001 resolution note, the MADR-0020 amendment — MADR-0020 is the record the
  build.rs comment and the pre-2026-07-23 documents cite as "AD 0020"; the current `AD-0020` id
  belongs to the unrelated additive-modification-options record — and the AD 0039 amendment).
* The `cargo:warning=Windows libarchive linking will be configured in future implementation` line
  is still emitted from build.rs's `target_os = "windows"` block; Windows-native libarchive
  discovery remains the open half of OI-0065-001 / OI-0080-001.

**The ruling stands and the record stays ACTIVE.** README.md ("present but not release-verified"),
Limitations.md (section "Windows support is not release-verified yet"), and docs/USER_MANUAL.md
still carry the wording this record defended, and that wording is still accurate today: the
Windows CI job is committed under OI-0065-001 (its Required Actions) but has **not landed** —
OI-0065-001 is OPEN, and OI-0081-005's Windows arm is explicitly "runtime verification pending
Windows CI". Under the directive, the rejected downgrade to "not supported" would now additionally
contradict stated project policy. The expected next change to this messaging is an **upgrade**
once the OI-0065-001 Windows CI job lands and release-verification becomes true — not the
downgrade this record rejected.

One deliberate departure from the review that prompted this amendment: §B reads the "not
release-verified" wording as *itself* the stale artifact now that the platform directive commits
to Windows. On the evidence that inversion has not happened yet — the repository contains no CI
configuration at all, so nothing has release-verified a Windows build — and stating verification
that has not run would be a stronger defect than under-claiming. The messaging upgrade is
therefore recorded as due-on-CI, not as due-now.

## Amendment (2026-09-03, AD-0070 — the Windows CI job is not coming; the reasoning survives it)

This record repeatedly anticipates a "Windows CI job … committed under
OI-0065-001" and treats its landing as the trigger for release-verification
becoming true. **Under AD-0070 that job will never land.** The owner has ruled
that this project adopts no hosted CI of any kind; verification is run by the
owner on each platform and recorded under `docs/verification/`.

**The rejection this record makes is unaffected.** Downgrading the Windows
support message was rejected because the support is real and the messaging
should describe it accurately — that argument never depended on *how* the
verification is produced, only on whether it exists. Substitute the mechanism
and every sentence of the reasoning still holds.

The trigger is therefore restated: release-verification for Windows becomes
true when a `docs/verification/` record exists, produced on a Windows host,
whose fingerprint matches the released commit. As of this amendment the owner's
Windows machine is the intended host and the run is pending, so the current
"present but not release-verified" wording remains correct — which is the
outcome this record argued for in the first place.
