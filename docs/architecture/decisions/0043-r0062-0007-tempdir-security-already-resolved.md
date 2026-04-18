# AD 0043: R0062-0007 temp-dir helper security already resolved by Review 0061

## Context and Problem Statement

Review 0062 R0062-0007 flagged a potential race condition in the test helper that creates unique scratch directories, recommending a migration to `tempfile::tempdir()` for atomicity guarantees.

By the time Review 0062 landed, Review 0061 had already addressed this concern. The current `common::temp_test_dir()` helper uses a PID+nanoseconds-unique path (`/Volumes/Temp/claude/test-<pid>-<ns>`) which eliminates the collision surface the review was worried about — parallel test binaries get per-process IDs, and parallel tests within a binary get per-nanosecond discriminators. The atomicity concern the `tempfile` migration was meant to solve is structurally impossible in the current design.

This decision records R0062-0007 as ACCEPTED (the concern was valid when the review was written) but notes that the fix already shipped, so no code change is required for this review.

## Decision Drivers

* The concern — two tests racing to create the same scratch directory — is real in the abstract and deserves a decision record, even when the implementation already addresses it.
* Future reviewers may raise the same concern if they see the helper without context; this record points them at the PID+ns design.
* The project has a separate, explicit rejection of `tempfile::tempdir()` migration for scratch-path use in tests (AD 0036). That rejection applies to R0062-0007 as well: `tempfile::tempdir()` resolves to the same root (`TMPDIR` / OS default), so swapping the helper's construction does not change the uniqueness guarantee — only the RAII drop behaviour.

## Considered Options

1. **Swap `common::temp_test_dir()` to `tempfile::tempdir()`.** Rejected — see AD 0036. The collision concern is already addressed by PID+ns; the only thing a swap would buy is automatic cleanup on drop, which we have traded off in favour of post-hoc inspection of failed test output (per AD 0036).
2. **Record R0062-0007 as already-resolved and leave the helper in place.** Accepted.

## Decision Outcome

ACCEPT R0062-0007 (concern valid), no code change required. Status: Resolved by prior work under Review 0061.

## Implementation

No code change. The concern was already addressed by the PID+ns-unique path construction in `tests/common/mod.rs::temp_test_dir()`.

## Consequences

* R0062-0007 is closed without code change.
* Future reviewers raising the same concern will land on this record, which points at AD 0036 for the rejection rationale and at `tests/common/mod.rs` for the actual uniqueness construction.

## Revisit trigger

Re-open if:

* A concrete test-race incident is observed that the PID+ns scheme does not prevent (e.g. if we ever start sharing a scratch dir across concurrent processes with colliding PIDs, which should not happen on a single host).
* AD 0036 is itself revisited and `tempfile::tempdir()` becomes the project-wide standard.
