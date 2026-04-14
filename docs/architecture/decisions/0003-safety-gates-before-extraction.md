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
