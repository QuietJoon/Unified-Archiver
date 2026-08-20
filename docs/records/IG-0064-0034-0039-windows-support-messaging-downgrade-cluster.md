---
type: ADR
title: "IG-0064-0034..0039: Windows-support messaging downgrade cluster"
description: "Reviewer asked to downgrade every user-facing Windows mention from \"present but not release-verified\" to \"not currently supported\" or \"incomplete,\" citing two build-script artifacts: `build.rs:52` still prints `Windows…"
tags: [decision, R0064-0034, R0064-0039]
timestamp: 2026-04-21T00:00:00Z
status: active
---

# IG-0064-0034..0039: Windows-support messaging downgrade cluster

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0064-0034 through R0064-0039 (Review 0064, 6 findings)
- **Date:** 2026-04-21
- **Decision:** REJECT
- **Severity:** Medium (per reviewer) — project assessment: Low policy-level concern

## Location

`build.rs:52`, `build.rs:81`, `README.md:17`, `README.md:48`, `README.md:227`, `docs/USER_MANUAL.md:62`, `Limitations.md:111`.

## Issue Summary

Reviewer asked to downgrade every user-facing Windows mention from "present but not release-verified" to "not currently supported" or "incomplete," citing two build-script artifacts: `build.rs:52` still prints `Windows libarchive linking will be configured in future implementation`, and `build.rs:81` unconditionally invokes `make` for the bundled UnRAR SDK.

## Rationale

- Windows is a real target for this crate, and the "present but not release-verified" wording is deliberate: it signals that downstream integrators supplying their own `make` and libarchive (MSYS2, Cygwin, WSL) build and run successfully today. Downgrading to "not supported" would preemptively shut a door the project intentionally holds open.
- The two build-script facts the reviewer cites are accurate but describe *incomplete automation*, not *absence of support*. The correct fix is to finish the Windows automation (gate `make`, wire the libarchive path), not to rewrite six downstream doc sites to reflect a policy change the project hasn't made.
- Reopening this messaging every review cycle is churn; a single REJECT record lets future reviewers land here.

## Future Consideration

If Windows automation becomes a priority, open a single OI to gate `make` on platform/tool availability and finish the libarchive link path. Downstream messaging stays as-is until that work lands.

## Related

- AD 0046 — policy record for this REJECT.
- `build.rs:52`, `build.rs:81` — the build-script artifacts cited by the cluster.
