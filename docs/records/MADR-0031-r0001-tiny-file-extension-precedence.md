---
type: ADR
title: "AD: Files too small to carry magic bytes fall back to any extension mapping"
description: "Rejected R0001-0035's recommendation to apply the `is_extension_fallback` whitelist to the sub-four-byte branch of format detection; the tiny-file branch and the post-magic fallback answer different questions and are deliberately governed by different rules."
tags: [decision, ADR-0031, R0001-0035, R0076-0093, OI-0080-008]
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-09T00:00:00Z
status: active
---

# AD: Files too small to carry magic bytes fall back to any extension mapping

## Context and Problem Statement

Found in Review 0001 (Issue R0001-0035, Severity: Medium).
Location: `src/format.rs` (`ArchiveFormat::detect`, the `bytes_read < 4` branch).

`ArchiveFormat::detect` reads a probe buffer and then decides in three stages: content magic first,
then — only for formats where the extension *is* the intended detection mechanism
(`ArchiveFormat::is_extension_fallback`, today `{Tar, Iso, Lzma, TarLzma}`) — an extension fallback,
and finally failure. Ahead of all three sits a fourth path: when fewer than four bytes were read,
there is no usable content signal at all, and the detector consults `format_from_extension` with
**no** whitelist gate (R0076-0093).

The reviewer observed that the two extension consultations therefore apply different trust rules to
the same question, and recommended applying `is_extension_fallback` in the tiny-file branch as well.
The consequence of the current rule is that a zero-byte or three-byte `foo.zip` classifies as
`ArchiveFormat::Zip`, and a `foo.7z` as `SevenZip`, even though both formats are otherwise strictly
magic-gated.

## Decision Drivers

* The two branches answer different questions. The post-magic fallback fires when content **was**
  read and **did not** match — there, trusting the extension means overriding positive contrary
  evidence, which is why it is whitelisted. The tiny-file branch fires when no content signal exists
  at all; there is no evidence to override, only the absence of any.
* `detect` is a classifier, not a validator. Nothing downstream trusts its answer as proof the file
  is well-formed: `Archive::open` re-derives the format and every backend rejects a truncated or
  empty payload on its own terms. An empty `foo.zip` reported as `Zip` still fails to open — it just
  fails with a ZIP-flavoured error instead of "File too small to detect format".
* The error the whitelist would restore is strictly less informative. For a genuinely truncated
  archive, `Zip` plus the backend's own diagnostic tells the caller more about what they have than a
  bare size complaint does.
* R0076-0093 introduced this branch deliberately and the source comment already states the rule and
  its boundary ("This does not weaken the magic-first precedence below: it only fires when there is
  no usable content signal"). The finding is the second independent reviewer to raise it, which is
  evidence the rule is under-documented, not that it is wrong.
* The residual risk is bounded and does not compound: `format_from_extension` is a pure name→format
  map with no I/O, and a wrong answer for a file that cannot be opened has no security consequence.

## Considered Options

1. **Reject** — keep the tiny-file branch ungated, and record the precedence rule so it stops being
   re-litigated (this decision).
2. **Accept** — apply `is_extension_fallback` in the tiny-file branch, making the two consultations
   identical. Tiny `.zip` / `.7z` / `.rar` files then return "File too small to detect format".
3. **Restructure** — return a typed "insufficient evidence, extension suggests X" state instead of a
   bare `ArchiveFormat`, letting callers decide. A public API change, disproportionate to the stakes.

## Decision Outcome

REJECT: we keep option 1. The recommendation is coherent and the inconsistency it names is real, but
it treats "no evidence" and "contrary evidence" as the same condition, and the two are not the same.
Option 2 buys internal symmetry at the cost of a less useful diagnostic for the exact inputs that
reach this branch — truncated and empty files — while changing nothing about what can actually be
opened.

Status: Implemented (decision-of-record only; no code change).

### Implementation

No source change. This record exists because a reviewer reading the `bytes_read < 4` branch beside
the `is_extension_fallback` gate will keep raising the asymmetry: the controlling rule is that
content evidence, when present, is never overridden by a name, and that a name is the only signal
available when content is absent. The source comment in `src/format.rs` states the same rule.

Note the adjacent, genuinely open concern this decision does **not** cover: OI-0080-008 records that
`Zst` is missing from `is_extension_fallback` while zstd archives with a leading skippable frame are
undetectable by magic. That is a defect in the whitelist's membership, not in the tiny-file
precedence rule, and it remains open on its own terms.

## Consequences

* Good, because a truncated archive keeps a format-specific diagnostic path instead of degrading to
  a size complaint that names no format.
* Good, because the precedence rule is now written down once, in the place reviewers look, instead of
  living only in a source comment that each new reviewer re-derives.
* Bad, because `ArchiveFormat::detect` can return `Zip` for a file that is not a ZIP by any content
  test. Accepted: `detect` classifies, it does not certify, and no caller may treat its answer as
  validation.
