---
type: ADR
title: "AD 0050: Reject R0066-0003/0004 — `extract_to_{memory,stream}_with_options` options-ignored is documented"
description: "Superseded by AD 0051 — the Review 0068 closure pass wired options.password through extract_to_{memory,stream}_with_options (R0068-0003/0004, R0072-0002), reversing this record's rejection of Option 1; the quoted limits-only rustdoc no longer exists. The narrowing rejection (R0066-0004) is unreversed."
tags: [decision, ADR-0050, R0066-0003, R0066-0004, R0066-0027, R0066-0005]
timestamp: 2026-04-24T00:00:00Z
status: superseded
---

# AD 0050: Reject R0066-0003/0004 — `extract_to_{memory,stream}_with_options` options-ignored is documented

Status: Superseded by AD 0051 (Review 0068 closure) — `options.password` is now honored by both entry points via `reopen_with_password_if_set` / `Archive::open_encrypted`, so the "limits-only, password-ignored" contract below no longer describes the code; the R0066-0004 narrowing rejection still stands. See the §B amendment at the end of this record.

## Context and Problem Statement

Found in Review 0066 (Issues R0066-0003 and R0066-0004, both Critical).
Location: `src/extraction.rs::extract_to_memory_with_options` and
`extract_to_stream_with_options`.

Both methods take a `&ExtractionOptions` parameter but honor only the
`limits` field. `password`, `destination`, `filter`, and `progress` are
ignored. The reviewer framed this as a silent option drop that the API
shape ("with_options") invited.

The concrete remediation options the reviewer suggested were:

1. **Wire the remaining options through.** `password` especially: reopen
   the archive with the caller-supplied password before extracting.
2. **Narrow the parameter** to `&ExtractionLimits`, matching what the
   method actually honors.

## Decision Drivers

* The rustdoc on both methods *explicitly* documents the limits-only
  contract today: *"Fields of `options` other than `limits` (e.g.
  `destination`, `password`, `filter`, `progress`) are ignored by this
  entry point — only the resource caps participate."* So the contract is
  discoverable, not hidden.
* Option 1 (wire password through) requires a reopen path for every
  non-libarchive backend. That reopen plumbing is the kind of
  backend-specific coupling that the v0.2+ trait-based backend refactor
  (R0066-0027/0028/0029) is designed to absorb cleanly. Bolting it onto
  the current enum-match dispatch now would add code that immediately
  gets rewritten.
* Option 2 (narrow the parameter to `&ExtractionLimits`) is a second
  breaking API change on top of the several landing in v0.2.0 for less
  payoff than sequencing it behind the god-object split, where
  extraction, modification, and creation all share a single
  session/options surface.
* Review 0066 was otherwise processed aggressively — 17 Cat-A fixes,
  several Cat-C2 corrections (R0066-0005, 0011/0012, 0021/0022/0023,
  0049, 0050) applied in the same release. Rejecting this specific pair
  is a deliberate scope call, not a priority slip.

## Considered Options

1. Accept R0066-0003: wire `options.password` through the memory path by
   reopening the archive inside `extract_to_memory_with_options` (and
   analogous for stream).
2. Accept R0066-0004 narrowing: change the parameter type to
   `&ExtractionLimits`, making the limits-only contract part of the
   signature.
3. Reject with pointer to existing rustdoc and defer a unified
   options surface to the v0.2+ backend refactor.

## Decision Outcome

REJECT: option 3. The current rustdoc already forces the caller to
discover the limits-only contract, and both narrowing and password
plumbing are better sequenced after the god-object/trait-backend split
that this release explicitly defers.

Status: Implemented (no code change; rustdoc already stated the
constraint before this gate). Revisit when the v0.2+ backend-trait
refactor lands — at that point, consider:

* Replacing `ExtractionOptions` parameters here with a purpose-built
  type (e.g. `ExtractionLimits` or a narrow `ExtractionBudget`), OR
* Building password/filter/progress/destination into the shared session
  type so every `_with_options` entry point honors the full surface
  uniformly.

## Consequences

* Good, because the v0.2.0 breaking surface is kept to changes that
  actually improve correctness (`pending_operations` returning `Result`,
  dup-path rejection, `Clone` removal on `CompressionOptions`, etc.)
  rather than churning an API that will be re-shaped by the next
  structural refactor.
* Good, because the concrete misuse the reviewer warned about
  (encrypted extraction via these paths) already fails loudly today —
  non-libarchive backends do not support encrypted read, so the caller
  sees an error from the backend, not a silent data leak.
* Bad, because the API shape continues to invite the misreading that
  `password` / `filter` are honored. Mitigated by the explicit rustdoc
  on both methods, which the gate confirms is present and accurate.

## References

- R0066-0003, R0066-0004 (Review 0066)
- `src/extraction.rs::extract_to_memory_with_options` rustdoc
- `src/extraction.rs::extract_to_stream_with_options` rustdoc
- CHANGELOG.md `[0.2.0] - Known limitations / follow-up`

## Amendment (2026-08-05, decision-review-2026-07-19 §B — superseded by AD 0051)

The 2026-07-19 review found that this record "claims a 'limits-only, password-ignored' stream
contract that the code (R0072-0002) and its own cited rustdoc now contradict" — correct on both
counts. The rustdoc quoted in the Decision Drivers, *"Fields of `options` other than `limits` (e.g.
`destination`, `password`, `filter`, `progress`) are ignored by this entry point — only the resource
caps participate."*, no longer exists in the crate's sources or documents — its only remaining
occurrences are this record and the translation-pipeline snapshots of it. Both entry points honor
`options.password` today.
`src/extraction.rs::extract_to_memory_with_options` and `extract_to_stream_impl` — the single body
behind both `extract_to_stream` and `extract_to_stream_with_options` (I2) — call
`reopen_with_password_if_set(self.source_path_for_reopen(), &options.password)` and then run the
safety pre-check and the read against the reopened handle. `reopen_with_password_if_set` funnels
through `open_archive_for_extraction`, which hands a supplied password to `Archive::open_encrypted`
instead of falling back to the plain opener, so a password given for a format without encryption
support surfaces `open_encrypted`'s `Unsupported` reason rather than a silently successful plaintext
extraction (**R0072-0002**); SFX/offset handles reopen the staged payload through
`Archive::source_path_for_reopen` (R0071-0001).

That is **Option 1 of this record's own Considered Options**, rejected here and deferred to the
post-v0.2 trait-backend refactor — it landed instead on the current enum-match dispatch during the
Review 0068 closure pass, on owner routing. **AD 0051** records the reversal in its own title
("Review 0068 closure — bulk-reject stale findings, accept the residual fixes, supersede AD 0050")
and in its finding table, where the R0068-0003/0004 row reads "**Supersedes AD 0050**, which had
rejected this on the grounds that the limits-only contract was documented and the unified-options
surface should wait for the post-v0.2 trait-backend refactor"; the CHANGELOG entry for the
password-aware `extract_to_memory_with_options` / `extract_to_stream_with_options` change repeats
"**Supersedes AD 0050**". (Both citations mean this record: the legacy `gate-00NN` series renumbered
to MADR ends at MADR-0030, so "AD 0050" cannot be a legacy gate record.)

The current contract of the two `_with_options` entry points, corrected:

* **Honored** — `limits` (per-entry safety pre-check via `check_single_entry_safe_with_archive`,
  the listing budget via `list_files_for_limits_budgeted`, the per-entry byte cap passed to
  `ValidatedSource::extract_to_memory_with_limit`, and for `StreamBound::DeclaredSize` the
  unknown-size hard-cap fallback `options.limits.max_file_size`); `password` (reopen through
  `Archive::open_encrypted`); and `verify_crc32` as an up-front admissibility gate —
  `assert_verify_crc32_supported` rejects `verify_crc32 = true` on a libarchive-backed handle with
  `ArchiveError::Unsupported` before any I/O runs, though the flag itself is not forwarded to the
  backend read, since `extract_to_memory_with_limit` / `extract_to_stream_with_limit` take only
  `max_bytes`.
* **Still not honored, and now documented as inapplicable rather than dropped** — `destination`,
  `overwrite`, `preserve_permissions`, `preserve_times`, `filter`, `progress`; the rustdoc on both
  methods states these "describe disk-write semantics or selective traversal and have no effect on a
  single in-memory read" (and the stream analogue).

The narrowing half (R0066-0004, parameter → `&ExtractionLimits`) was never executed — the signature
still takes `&ExtractionOptions` — and that outcome is now overdetermined, because `password` and the
CRC gate participate and an `&ExtractionLimits` parameter could not carry them. One Consequence above
is also no longer the operative mechanism: encrypted read *is* supported for RAR/RAR5/ZIP/7z (README.md
format matrix), so the loud failure the record credited to "non-libarchive backends do not support
encrypted read" now comes from the explicit `open_encrypted` funnel instead.

**Disposition: superseded** — by AD 0051, per that record's title and table and the CHANGELOG entry,
not by this amendment. This departs from the §B brief, which expected the record to stay `active`
because "the rejection ruling itself was not reversed": half of it was, by an owner routing decision,
and the superseding record says so in its title. What remains unreversed is the narrowing rejection;
the two follow-ups this record named — a purpose-built narrow parameter type, or a shared
session/options surface that honors the full option set uniformly — are still open questions and are
not decided here. Cross-refs: AD 0051 (supersession, R0068-0003/0004); R0072-0002 (`open_encrypted`
funnel); AD 0062 A.2 / DCR-006 (the `StreamBound` hard-cap contract the stream surface now carries).

## Amendment (2026-08-12, R0001-0011 — how `limits` participates on the stream path)

One line of the §B "Honored" list has moved. It read that
`extract_to_stream_with_options` uses `options.limits.max_file_size` as the
`StreamBound::DeclaredSize` unknown-size hard-cap fallback, and that the per-entry byte cap handed
to `ValidatedSource::extract_to_stream_with_limit` is that same value.

Both are now the *effective entry ceiling*, `min(max_file_size, max_total_size)`, and the cap handed
to the backend is `min(bound-derived cap, that ceiling)` — see DCR-006's 2026-08-12 amendment. So
`options.limits` participates through four channels rather than three: the per-entry safety
pre-check, the listing budget, the bound-derived backend materialisation budget, and the
unknown-size reader fallback. Nothing about this record's disposition or the R0066-0004 narrowing
rejection changes; an `&ExtractionLimits` parameter still could not carry `password` or the CRC gate.
