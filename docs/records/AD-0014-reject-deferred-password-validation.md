---
type: ADR
title: "AD: Reject proposal to validate passwords at open time"
description: "No change needed"
tags: [decision, ADR-0014, R0025-0029, R0025-0030]
generated:
  by: unknown/unknown
  at: 2026-04-23T00:00:00Z
category: security-safety
status: draft
---

# AD: Reject proposal to validate passwords at open time

## Context and Problem Statement
Found in Review 0025 (Issues R0025-0029, R0025-0030, Severity: MEDIUM).
Location: `src/archive.rs` — `open_encrypted()`

Reviewer proposed that `open_encrypted()` should immediately validate the password by attempting a trial decryption, rather than deferring validation to first extraction. The concern was that users could proceed with an invalid password and only discover the error later.

## Decision Drivers
* Current deferred validation is by design — it matches how archive formats work natively
* RAR, 7z, and ZIP all defer password validation to entry access time
* Trial decryption at open time would require decompressing an entry, adding latency
* Some archives have mixed encryption (some entries encrypted, some not)
* Password correctness is entry-specific in formats that support per-entry encryption

## Considered Options
1. Accept: add trial decryption at open time
2. Reject: keep deferred validation (current design)

## Decision Outcome
REJECT: Deferred password validation is the correct design for this library.

Status: No change needed

### Implementation
No code changes. The current behavior correctly defers password validation to extraction time, matching native archive format behavior.

## Consequences
* Good, because `open_encrypted()` remains fast (no I/O beyond header parsing)
* Good, because mixed-encryption archives work correctly
* Neutral: users discover bad passwords at extraction time, which is standard behavior for archive tools
