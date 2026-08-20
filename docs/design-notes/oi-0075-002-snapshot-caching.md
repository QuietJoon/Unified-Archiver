---
type: Design Note
title: "OI-0075-002 — Snapshot semantics & per-backend caching baseline: cons / pros"
description: "Status: parked design note."
tags: [design-note, R0075-0004, OI-0075-002]
timestamp: 2026-04-29T00:00:00Z
status: active
---

# OI-0075-002 — Snapshot semantics & per-backend caching baseline: cons / pros

**Status:** parked design note. Send-safety half is closed (Review 0075). Caching-baseline half is open.

## Problem (one sentence)
`Archive`'s rustdoc was tightened in Review 0075 to "best-effort metadata cache" with a per-backend caching list, but the backends do not agree on a single caching policy: Piz caches mmap+central directory, ZipReader caches a Mutex'd handle, 7z caches the TOC, libarchive lazily reopens for every operation, UnRAR keeps the FFI handle alive.

## Two paths

### Option A — every read backend memoises listing/metadata once on first use ("freeze the view")
Make `ReadBackend::list_files()` cache its output in a `OnceLock<Vec<ArchiveEntry>>` on the backend struct. Every other read method consumes the cached listing. The libarchive backend gains a parsed-metadata cache after open-time validation.

**Pros**
- Strict snapshot semantics: `archive.list_files()` returns the same vec twice in a row even if the file was rewritten on disk between calls.
- Removes the surprise factor for callers who treat `Archive` as a logical view of the archive (the rustdoc wording today admits this is best-effort).
- Closes the divergence: Piz/Zip/7z already cache; only libarchive doesn't.
- Compile-time `Send` assertion (already landed) is unchanged.

**Cons**
- Memory cost grows: a single `Archive::open` of a 1M-entry tar carries ~200 MB of cached entry metadata for the lifetime of the handle. Today, that's only paid by Piz/Zip/7z.
- Subtle behavior change: a user who edits the archive and re-opens via the same handle would today see new contents on libarchive (lazy reopen sees the new file); under Option A they'd see the cached listing forever. Caller migration story matters.
- Cache invalidation: any "this archive changed under us" pathway needs an explicit `Archive::reopen()` API.

### Option B — accept current behaviour, formalise it as ADR
Lock the wording: "best-effort metadata cache; per-backend caching is documented per backend; no global snapshot guarantee."

**Pros**
- Zero code change. Ship as-is.
- Memory cost stays where it was.
- libarchive's lazy reopen lets long-lived `Archive` handles see disk-side rewrites, which is the more useful behaviour for some callers (build systems, watch loops).

**Cons**
- Future-reviewer hazard: Option A keeps getting suggested every code review because the heterogeneity violates principle of least surprise. ADR would close that loop.
- The compile-time `Send` assertion guards drift but doesn't pin caching.

## Recommendation (for the user, not a decision here)
**Option B + ADR**. Reason: the backends were heterogeneous on purpose (each format gets the caching that matches its FFI shape); forcing them to align would either bloat libarchive memory or weaken Piz/Zip/7z. Codify the heterogeneity instead.

## Effort
- Option A: 2 days for libarchive caching + 0.5 day per backend for cache-invalidation tests + 1 day for the `Archive::reopen()` API.
- Option B: 0.5 day for an ADR. Done.

## Send-safety audit (R0075-0004)
Already resolved in Review 0075:
- SAFETY comment refreshed against current fields.
- Compile-time `Send` assertion at `src/archive.rs` (`const _: fn() = || { ... };`) anchors the invariant.

No further work on the Send half.

## Why this isn't an ADR
The user routed it as cons/pros doc. The Option-A-vs-B call is the user's; the ADR follows the decision.
