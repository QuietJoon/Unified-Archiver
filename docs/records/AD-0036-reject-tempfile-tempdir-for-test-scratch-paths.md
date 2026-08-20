---
type: ADR
title: "AD 0036: Reject tempfile::tempdir migration for test scratch paths"
description: "Review 0061 raised 30 findings (R0061-0094 through R0061-0123) recommending that every std::env::tempdir()-based scratch path across tests/ be…"
tags: [decision, ADR-0036, R0061-0094, R0061-0123]
timestamp: 2026-04-18T00:00:00Z
status: active
---

# AD 0036: Reject tempfile::tempdir migration for test scratch paths

## Context and Problem Statement

Review 0061 raised 30 findings (R0061-0094 through R0061-0123) recommending that every `std::env::temp_dir()`-based scratch path across `tests/` be migrated to `tempfile::tempdir()`. The motivation cited was automatic cleanup and isolation between concurrent test runs.

This decision records why the migration is rejected for v0.1.0, and what should happen instead if the underlying shared-root problem needs a global fix.

## Decision Drivers

* The project has observed shared-root problems with `env::temp_dir()` — for example, a stale `/tmp/serial-test-rar` lock file owned by one system user blocking subsequent runs under a different user during this very review sweep.
* `tempfile::tempdir()` ultimately resolves under `env::temp_dir()` too, so it inherits the shared root. The migration does not address the concern it was motivated by.
* The project already supplies `tests/common/mod.rs::temp_test_dir()` which produces PID+nanoseconds-unique paths under a project-rooted scratch dir (`/Volumes/Temp/claude/7zip/` per `CLAUDE.md`) and only falls back to `env::temp_dir()` when the project root is unavailable. That helper — not `tempfile::tempdir()` — is the right place to fix shared-root issues.
* The 30 findings are mechanical changes across 30 files with no behaviour improvement; the cost is real churn and larger diffs for future reviewers to wade through, with no corresponding benefit.

## Considered Options

1. **Accept the migration.** Replace every `env::temp_dir().join(...)` in `tests/` with a `tempfile::TempDir` scoped to each test. Large diff; inherits the shared-root problem.
2. **Reject and document; continue using `temp_test_dir()`.** Leave the call sites alone and record the rationale so future reviewers do not re-raise the same findings.
3. **Centralise the fix inside `temp_test_dir()`.** If the shared-root problem ever proves intolerable, change the fallback branch of the helper — a single-file edit — rather than fanning the change across every call site.

## Decision Outcome

ACCEPT option 2 for v0.1.0; option 3 is the pre-authorised path if a centralised fix becomes necessary.

Rationale:

* Options 1 is cosmetic and inherits the same shared root it purports to fix.
* Option 3 concentrates any future change in one place, which is how the codebase already centralises scratch-path construction.
* The rejection is captured in [`IG-0061-0094..0123`](IG-0061-0094-0123-tempfile-tempdir-migration-for-test-scratch-paths.md) so future review rounds can point to this record rather than relitigating.

## Consequences

* Tests continue to use `std::env::temp_dir()` where convenient and `tests/common/mod.rs::temp_test_dir()` where a project-rooted path is needed.
* Any future reviewer raising the same 30-site `tempfile::tempdir()` migration should be pointed at this record and at IG-0061-0094..0123.
* A future global fix (if required) is a single edit inside `temp_test_dir()`'s fallback branch, not a 30-file sweep.

## Revisit trigger

Re-open this decision when **either** of the following is true:

* A reproducible test failure is traced to `env::temp_dir()` isolation specifically (not to the stale-lock class of problem, which the helper already addresses), AND `tempfile::tempdir()` would demonstrably fix it.
* The project adopts a different scratch-path policy in `CLAUDE.md` that makes `env::temp_dir()`-based paths unacceptable.
