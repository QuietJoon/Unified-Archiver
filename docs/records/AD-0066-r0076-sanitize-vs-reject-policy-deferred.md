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
