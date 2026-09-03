---
type: ADR
title: "AD 0066: Path sanitization policy — preserve current \"lossy repair\" baseline, queue strict-reject opt-in"
description: "Accepted (2026-05-01) — closes Review 0076 R0076-0004 routing question."
tags: [decision, ADR-0066, ADR-0044, R0076-0004, R0070-0057, R0070-0058, R0076-0005, OI-0076-005, OI-0076-003]
timestamp: 2026-05-01T00:00:00Z
status: active
---

# AD 0066: Path sanitization policy — preserve current "lossy repair" baseline, queue strict-reject opt-in

## Status

Accepted (2026-05-01) — closes Review 0076 R0076-0004 routing question.

## Context

`src/security.rs::normalize_entry_components` rewrites entry paths
containing traversal (`..`), root (`/`), or current-directory (`.`)
components into a name-only "safe" form, dropping the offending
components rather than rejecting the entry. Review 0076's R0076-0004
flagged this as a security-policy gap:

> Malicious entries such as `../../etc/passwd` are rewritten to
> safe-looking output names, which hides hostile archive intent and
> can collide with legitimate entries.

The reviewer recommended splitting "validate" from "repair": reject
traversal/root/current-directory components by default, and reserve
lossy repair for an explicitly named compatibility mode.

The current behavior was carried forward through R0070-0057 / R0070-0058
without an ADR documenting "lossy repair is the long-term policy".
Review 0076 forces the question: ratify the current behaviour or
commit to changing it.

## Decision

We adopt **the current "lossy repair" behaviour as the v0.3 baseline**
and queue a strict-reject opt-in for v0.4. Concretely:

- **No code change in v0.3.** `normalize_entry_components` continues
  to drop traversal / root / current-directory components and pass
  the resulting safe-named entry through. Existing callers that
  expect "extract all entries from a wild archive" do not break.
- **An ADR exists (this one).** The previously-implicit policy is now
  explicit: lossy repair is the *baseline*, not strict reject.
- **Future option lands as a flag, not a default.** A new
  `ExtractionLimits::reject_unsafe_paths: bool` (default `false`)
  is queued for v0.4. When set, `normalize_entry_components` returns
  an `OperationBlocked { operation: EXTRACT, reason: "unsafe path
  components" }` instead of rewriting. The default remains the
  current behaviour to preserve source-compat across the v0.3.x line.
- **Warning surface, not silent.** The R0076-0004 implicit concern
  ("hides hostile archive intent") is partially mitigated by the
  existing `ArchiveWarning` channel. Audit work for v0.4 will ensure
  every lossy rewrite emits a warning the caller can inspect, even
  in default mode.

## Consequences

### Good

- **Stability.** The most common archive shape — a wild-typed archive
  with stray traversal components from a careless `tar c .` — keeps
  extracting in default mode. Breaking that for v0.3 callers risks
  more breakage than it prevents.
- **Documented.** The policy is no longer a comment in
  R0070-0057 / R0070-0058 — it is an ADR with explicit scope and a
  v0.4 migration path.
- **Strict mode is opt-in.** Security-conscious callers gain a hard
  reject by setting `reject_unsafe_paths`. No silent rewrites under
  that flag.

### Bad / costs

- **Default remains permissive.** Until v0.4 lands the flag, the
  default behaviour is exactly what R0076-0004 flagged — silent
  rewrites that obscure hostile archive intent.
- **Two paths.** `normalize_entry_components` will need to branch on
  `reject_unsafe_paths` once the flag exists; tests must cover both
  branches.

## Alternatives considered

- **Switch default to strict-reject in v0.3.** Rejected: source-compat
  break across the v0.3.x line is too aggressive for what is, today,
  a security *posture* improvement (the lossy form is still safe;
  the rewrite never lets the entry escape `dest_path`).
- **Mark the public sanitization API `#[deprecated]` and ship a new
  one in v0.3.** Rejected: it provides no benefit over the simpler
  flag, while introducing a parallel API surface during the v0.3 line.
- **Add the flag now in v0.3 with a non-breaking default.** Rejected
  for *this* decision but explicitly queued for v0.4 alongside
  `OI-0076-005` (encapsulating public-field structs, including
  `ExtractionLimits`). Adding the field now would land before the
  builder API is the documented primary path; coordinating with
  v0.4 keeps the surface coherent.

## Verification

- The behavior is the no-op of v0.3 — no test changes needed for the
  baseline.
- `tests/integration/path_traversal_extract.rs` already exercises
  the lossy-repair branch and asserts entries land *under* the
  destination, never above it.
- v0.4 work will add a strict-reject regression test
  (`tests/integration/strict_path_rejection.rs`) when the flag
  lands.

## Related

- R0070-0057 / R0070-0058 — earlier comments documenting the lossy
  approach in code (no ADR existed at the time).
- AD 0044 — validate archive internal paths at facade boundary
  (canonicalises but does not reject).
- OI-0076-003 — security/durability boundary refactors. The
  parent-creation race (R0076-0005) interacts with the sanitization
  pipeline; both should be revisited together when the v0.4 strict
  flag lands.
- OI-0076-005 — `ExtractionLimits` encapsulation; the new
  `reject_unsafe_paths` field will be added through the new builder
  rather than as a public mutable field.

## Amendment (2026-07-22, R0081 I1)

`reject_unsafe_paths` now exists as a **first-class, validated field** of
`ExtractionLimits` (default `false`), set through the new
`ExtractionLimits::builder().reject_unsafe_paths(bool)` and read via
`ExtractionLimits::reject_unsafe_paths()`. This lands exactly as this ADR
anticipated: "the new `reject_unsafe_paths` field will be added through
the new builder rather than as a public mutable field." AD 0066 is
therefore no longer a *bare* deferral — the flag has a typed home and the
`ExtractionLimits` encapsulation prerequisite (OI-0076-005) is in place.

**The strict-reject behaviour itself remains deferred.** The field is
plumbed but `normalize_entry_components` does not yet branch on it; the
default lossy-repair baseline is unchanged, and no code path reads the
flag to reject rather than repair. Wiring the behaviour (and its
regression test, `tests/integration/strict_path_rejection.rs`) stays
OI-0076-003 item 1 work. Status remains: Accepted; behaviour deferred.

## Amendment (2026-08-21, OI-0076-005 — `ArchivePathPolicy` stays ambient, gains a thread scope)

Two updates: one correction to the 2026-07-22 amendment, and the decision
on where the write-side naming policy lives.

### Correction: strict-reject is wired, not just plumbed

The 2026-07-22 amendment said "the field is plumbed but
`normalize_entry_components` does not yet branch on it". That is no longer
true. `src/security.rs` now carries a `UnsafePathPolicy { Repair, Reject }`
selected by `UnsafePathPolicy::from_limits`, `normalize_entry_components`
takes it, and under `Reject` it returns
`OperationBlocked { operation: EXTRACT, reason: "unsafe path components in
entry '…': … (ExtractionLimits::reject_unsafe_paths is set)" }` before any
rewrite happens — the variant and wording this record fixed. `Repair`
remains the default, so the baseline this record ratified is unchanged.
The deferral recorded on 2026-07-22 is therefore closed; what stays open is
only whatever OI-0076-003 tracks separately.

### Decision: keep the ambient policy, add a thread-scoped override, do *not* move it to per-operation options

`ArchivePathPolicy` (AD 0044's write-side naming rules) was reachable only
through `set_archive_path_policy`, a process-wide `AtomicU8`. The routed
question was whether to move it to a per-operation options field instead.

**Decided: keep it ambient, and add `with_archive_path_policy(policy, f)` —
a thread-scoped override that shadows the process-wide default for the
duration of the closure and restores the previous state on the way out,
including while unwinding from a panic.** `archive_path_policy()` now
resolves innermost-thread-scope → process-wide default → `Portable`.

Why not per-operation options:

- **The write path takes no options.** `add_file_from_data`,
  `add_file_from_path_as`, `add_directory`, `add_entry`,
  `add_directory_entry` and `remove_entry` each take a name and (sometimes)
  data — nothing else. A policy field means either a signature change on
  all six public methods plus their `ModifyArchive`/`WriteArchive`
  counterparts, or a parallel `*_with_policy` method for each. That is a
  wide, permanent API cost.
- **What it would buy is already bought.** The reason to want per-operation
  granularity is the two-consumers hazard below, and the hazard is
  per-*thread* in practice: every write-side validation runs synchronously
  on the thread that made the call, so a thread scope confines the choice
  exactly as tightly as an argument would for any realistic consumer, at
  zero API cost. A genuinely per-entry mix of policies within one archive
  is not a use case anyone has asked for — the policy describes the
  archive's intended portability, which is a property of the archive.
- **It is a naming knob, not a security gate.** Both policies refuse
  empty, NUL, absolute-per-host, `.` and `..` names. `Host` only stops
  refusing names that a *different* host could not represent (`C:/x`,
  `n:stream`, `CON`). The extraction-side sanitiser is untouched by either
  value. So the blast radius of getting the policy wrong is a portability
  regression in an archive you authored, not an escape.
- **Precedent.** AD 0019's process-wide UnRAR mutex already establishes
  that this crate accepts process-global state where the alternative is a
  worse API. The difference is that the mutex has one correct value and
  this has two, which is precisely why the scoped form was added rather
  than leaving only the latch.

### The two-consumers hazard, recorded so nobody rediscovers it

`set_archive_path_policy` is **process-wide mutable state in a library**.
Two consumers linked into one process share one slot and the last writer
wins, silently:

- Library A calls `set_archive_path_policy(Host)` during its
  initialisation. Application B, in the same process, now finds that its
  own `add_file_from_data("CON", …)` succeeds where it used to be
  rejected — B's archives quietly gain names that no Windows consumer can
  extract, and nothing in B's code changed.
- The mirror case: A sets `Portable` back (or is loaded second and never
  sets anything after B set `Host`), and B's deliberately host-specific
  writes start failing with `InvalidPath` for reasons that are nowhere in
  B's call stack.
- Neither is detectable from the API: the setter returns the *previous*
  value, so a caller can observe that someone else had changed it, but
  only if it happens to look, and there is no notification.

The mitigation is a documented convention, not a mechanism:
`set_archive_path_policy` is for an **application's** own start-up, called
once, before any archive is written. **Library code must use
`with_archive_path_policy`** — the scoped form cannot be observed by
another thread and cannot outlive its own closure, so two libraries using
it cannot interfere. The rustdoc on both functions now says this, and the
setter's docs name the hazard directly.

Residual, accepted: an application that calls the process-wide setter
still perturbs libraries that read the ambient policy without scoping. The
scoped form makes the correct pattern available and cheap; it does not
make the incorrect one impossible. Making it impossible requires deleting
`set_archive_path_policy`, which would leave applications with no way to
set a default at all — rejected as a worse trade.

### Verification

`src/security.rs` tests cover scope-and-restore, nesting, restore-on-panic,
non-propagation to spawned threads, and scope-wins-over-process-default.
All are `#[serial_test::serial]` because they touch the global slot.

## Amendment (2026-09-03, two claims corrected)

**The "warning surface, not silent" mitigation is not met.** The Decision section offers, as partial
mitigation for R0076-0004, that a repaired path surfaces a warning. It does not: `src/security.rs`
constructs no `ArchiveWarning` at all, and no warning variant covers path repair. A lossy repair is
therefore silent to the caller today, which is exactly the concern R0076-0004 raised. The deferral
stands on its other grounds; this particular mitigation should not be relied on when re-deciding.

**`reject_unsafe_paths` is enforced.** Any surviving description of the flag as "recorded but not
consulted" is stale — `src/security.rs` branches on `limits.reject_unsafe_paths` and blocks
pre-extraction when it is set. The default (`false`) repairs, as this record intends.
