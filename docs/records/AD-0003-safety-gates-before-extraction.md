---
type: ADR
title: "AD: Safety Gates Before Extraction"
description: "Accepted"
tags: [decision, ADR-0003]
generated:
  by: unknown/unknown
  at: 2026-04-12T00:00:00Z
category: security-safety
status: draft
---

# AD: Safety Gates Before Extraction

Status: Accepted

## Context and Problem Statement
Archive extraction is security-sensitive. Path traversal attacks, zip bombs, and overwrite conflicts are real risks that must be mitigated consistently regardless of which backend performs the actual extraction.

## Decision Drivers
- Consistent security posture across all backends
- Centralized audit surface for safety-critical logic
- Defense in depth against malicious archive contents

## Considered Alternatives
- **Delegate all safety handling to each backend** -- rejected because behavior divergence between backends increases and the audit burden multiplies with each new backend.

## Decision Outcome
We decided to have the orchestrator apply sanitization, extraction limits, and overwrite conflict checks before backend extraction because it centralizes the security policy in one auditable location and guarantees consistent safeguards.

## Consequences
- Good: Centralized preflight policy yields consistent safeguards across all backends.
- Bad: Extra pre-scan overhead and potential mismatch with backend-native behavior when backends also perform their own validation.

## Amendment (2026-07-22, R0081 I1)

The "extraction limits" arm of this gate is now a **typed, validated
struct**. `ExtractionLimits` no longer exposes public mutable fields with
a raw `f64` compression ratio and `f64::MAX` / `u64::MAX` sentinels;
fields are private behind `ExtractionLimits::builder()`
(`ExtractionLimitsBuilder`). Each ceiling is a typed `Cap`
(`Limited(u64)` / `Unlimited`) and the compression ratio is an exact
rational `CompressionRatio` compared by integer `u128`
cross-multiplication.

This does not change what the gate *does* — the same preflight
(entry-count, per-file size, cumulative size, compression ratio) runs in
the same central location. It changes how the policy inputs are
*constructed*: "unlimited" is a distinct state rather than an extreme
numeric value, so the sentinel/precision hazards this gate previously
relied on hand-written guards for (see AD amendment cross-links below)
are structurally impossible. The SFX staging ceiling (AD 0040) and the
`reject_unsafe_paths` flag (AD 0066) are now first-class validated fields
of the same struct.

Cross-links: retires the INFINITY-vs-MAX sentinel hazard hand-patched in
R0080-0006 and the 2^53 `f64` precision cliff hand-patched in R0081-0024;
partially advances OI-0076-005 for `ExtractionLimits`.
