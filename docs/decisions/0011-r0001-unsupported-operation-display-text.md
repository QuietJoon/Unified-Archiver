# AD: UnsupportedOperation display text — "cannot be performed" replaces "not implemented"

## Context and Problem Statement
Found in Review 0001 (Issue R0001-0003, Severity: High).
Location: `src/error.rs:137`

The `Display` implementation for `ArchiveError::UnsupportedOperation` printed
"Operation '…' not implemented: …", but the same variant is used for runtime
states that are implemented but disallowed: lock contention, write-mode
extraction, mmap size limits, and path collisions. The text misled users into
thinking the feature was missing when the actual cause was a runtime constraint.

## Decision Drivers
* 40+ usage sites span four categories: feature deferral, mode mismatch, resource constraints, boundary enforcement
* Users read the Display text in error messages and logs
* Programmatic matching uses the variant name, not the Display text — changing the text is backward-compatible for error-handling code

## Considered Options
1. Change Display text to neutral wording: "cannot be performed"
2. Split into separate error variants (UnsupportedFeature, InvalidOperation, ResourceError)
3. Keep current text — callers can read the `reason` field

## Decision Outcome
ACCEPT (Option 1): Display text changed from "not implemented" to "cannot be performed".
Option 2 (variant split) tracked as future work in Open_Issues.md (OI-0001-003).

Status: Implemented

### Implementation
- `src/error.rs`: Display impl changed to `"Operation '{}' cannot be performed: {}"`
- `src/error.rs`: Updated test `test_display_unsupported_operation` assertion

## Consequences
* Good, because error messages are now accurate for all usage sites
* Good, because no API breakage — variant name and fields unchanged
* Bad, because log messages and user-facing strings change text (could affect grep-based monitoring)
