---
type: ADR
title: "IG-0081-0088: BSD targets omit the C++ runtime for the vendored UnRAR build"
description: "C++ runtime linkage has branches only for macOS (`-lc++`), Linux (`-lstdc++`), and Windows; a native BSD/DragonFly build links no C++ runtime and fails at link time."
tags: [decision, R0081-0088]
timestamp: 2026-07-18T00:00:00Z
status: active
---

# IG-0081-0088: BSD targets omit the C++ runtime for the vendored UnRAR build

> **Legacy review disposition — migrated 2026-08-04.** Split out of the retired
> `docs/records/legacy/Ignores.md` write-path log into its own record file. Body
> preserved verbatim; section headings re-levelled (`###` → `##`) now that the record
> is a standalone document. Content-immutable: append a dated `## Amendment` section
> rather than editing the text above it.

- **Source:** R0081-0088 (Review 0081)
- **Date:** 2026-07-18
- **Decision:** REJECT
- **Severity:** High (claimed)

## Location

`build.rs` (`build_unrar` C++ runtime linkage branches)

## Issue Summary

C++ runtime linkage has branches only for macOS (`-lc++`), Linux (`-lstdc++`), and Windows; a
native BSD/DragonFly build links no C++ runtime and fails at link time.

## Rationale

Platform out of scope. The support matrix is native Windows, macOS, and Linux (owner directive
2026-07-17, recorded as OI-0080-001); BSD is not a target pre-v2. On the three supported platforms
the correct branch fires. This is the same host-`cfg` target-selection mechanism already tracked
out-of-scope as R0080-0049/R0080-0050 (OI-0080-001). If BSD is ever added, it folds into
OI-0080-001's "model supported targets explicitly" work. An optional non-blocking nicety would be a
clear "unsupported platform" `panic!` on non-matrix targets, but it is not required by scope.

## Related

- OI-0080-001 (cross-compilation / target-selection tracking), R0080-0049/0050
