---
type: ADR
title: "IG-0061-0094..0123: tempfile::tempdir() migration for test scratch paths"
description: "Reviewer recommended replacing every `std::env::temp_dir()`-based scratch path with `tempfile::tempdir()` for automatic cleanup and isolation."
tags: [decision, R0061-0094, R0061-0123]
timestamp: 2026-04-18T00:00:00Z
status: active
---

# IG-0061-0094..0123: tempfile::tempdir() migration for test scratch paths

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0061-0094 through R0061-0123 (Review 0061, 30 findings)
- **Date:** 2026-04-18
- **Decision:** REJECT
- **Severity:** Low (style/consistency)

## Location

Numerous `tests/*.rs` call sites using `std::env::temp_dir().join(...)`.

## Issue Summary

Reviewer recommended replacing every `std::env::temp_dir()`-based scratch path with `tempfile::tempdir()` for automatic cleanup and isolation.

## Rationale

- `std::env::temp_dir()` has caused development problems in this project (shared `/tmp/` collisions between concurrent test runs and different system users on the same machine — see the stale `/tmp/serial-test-rar` lock file incident where a lock created by another user blocks subsequent runs).
- `tempfile::tempdir()` resolves under the same `TMPDIR` root (`env::temp_dir()` underneath), so it does not address the underlying shared-root concern.
- The project already supplies `tests/common/mod.rs::temp_test_dir()` which produces PID+nanoseconds-unique paths under a project-rooted scratch dir (`/Volumes/Temp/claude/7zip/` per `CLAUDE.md`), falling back to `env::temp_dir()` only when the project root is unreachable. That helper is the correct vehicle for fixing the shared-root problem.
- Migrating call sites to `tempfile::tempdir()` would mechanically churn 30 files without improving isolation. Individual sites that genuinely need RAII cleanup can adopt `tempfile` selectively.

## Future Consideration

If the shared-root problem needs a global fix, replace the `env::temp_dir()` fallback *inside* `tests/common/mod.rs::temp_test_dir()`. Do not fan the migration out across every call site.

## Related

- `tests/common/mod.rs::temp_test_dir()` — the canonical per-test scratch-path helper.
- `CLAUDE.md` — project temp-dir convention (`/Volumes/Temp/claude`).
