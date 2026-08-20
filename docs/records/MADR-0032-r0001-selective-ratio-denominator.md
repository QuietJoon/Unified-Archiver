---
type: ADR
title: "AD: For selective extraction the archive-ratio guard is informational; the byte caps bind"
description: "Disposes of R0001-0041: `extract_some` divides the selected uncompressed sum by the whole archive's on-disk size, which is more permissive than a true per-entry ratio on CRC-less formats. The decoded-byte caps are the binding check for subsets; the ratio result is advisory there."
tags: [decision, ADR-0032, R0001-0041, ADR-0003, ADR-0029]
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-09T00:00:00Z
status: active
---

# AD: For selective extraction the archive-ratio guard is informational; the byte caps bind

## Context and Problem Statement

Found in Review 0001 (Issue R0001-0041, Severity: Medium).
Location: `src/extraction.rs` (`Archive::extract_some`, the archive-level compression-ratio gate).

`ExtractionLimits::max_compression_ratio` is evaluated as *selected uncompressed size* divided by
*the whole archive's on-disk size*. For a full extraction those two terms describe the same object
and the ratio means what it says. For a **subset**, the numerator shrinks with the selection while
the denominator does not, so the computed ratio falls below a true per-entry ratio — and on formats
that expose no per-entry compressed size (TAR.GZ, TAR.BZ2, TAR.XZ), there is no way to attribute
compressed bytes to the selection at all. A hostile archive can therefore pack one high-expansion
entry alongside unrelated compressed bulk and rely on the small selection to pull the ratio under
the threshold.

The behaviour is already documented in the `extract_some` rustdoc, which names the caveat and tells
callers to tighten `max_total_size` / `max_file_size` for untrusted archives in those formats. What
was missing is a disposition: no record and no open issue said whether that caveat *is* the answer
or a placeholder for a fix, so every reviewer re-opens it.

## Decision Drivers

* The proposed alternative is not available on the formats that carry the risk. A format-specific
  accounting pass can attribute compressed bytes only where per-entry compressed sizes exist — ZIP,
  7z, RAR — which are precisely the formats where the ratio already behaves. For the compressed-TAR
  family the compressed stream is a single undifferentiated blob; no correct denominator exists
  short of decoding, which is the thing the guard is meant to avoid.
* The ratio gate was never the binding control. AD-0003 places it among several gates, and the
  decoded-byte ceilings are the ones that hold regardless of metadata honesty: `max_file_size` and
  `max_total_size` are enforced against **decoded** bytes inside the backend copy loops, so an entry
  that lies about its size is stopped while decoding, not by arithmetic on its header.
* Tightening the ratio for subsets in the obvious way — dividing by an estimated share of the
  archive — would invent a denominator from nothing and produce false refusals on ordinary archives
  with uneven entry sizes. A guard that fires on legitimate input is worse than one that defers to a
  stronger sibling.
* The recommendation in the finding says the same thing: "rely on a decoded-byte cap as the binding
  check and label the archive-ratio result informational for subsets". This record adopts that half
  of the recommendation and declines the format-specific accounting pass.

## Considered Options

1. **Record the disposition** — state that for selective extraction the ratio is advisory and the
   decoded-byte caps bind, keeping the code and the existing rustdoc caveat as they are.
2. **Format-specific accounting** — attribute compressed bytes to the selection where per-entry
   compressed sizes exist, and skip the ratio entirely where they do not. Adds per-format logic and
   changes nothing for the at-risk formats.
3. **Refuse subsets on CRC-less compressed formats** unless the caller raises a limit explicitly.
   Safe, and hostile to the ordinary case of pulling one file out of a `.tar.gz`.

## Decision Outcome

Option 1. For `extract_some` and every selective path built on it, the archive-level
compression-ratio result is **informational**; `ExtractionLimits::max_total_size` and
`max_file_size`, enforced against decoded bytes, are the binding protection. Callers extracting
subsets from untrusted CRC-less compressed archives must tighten those two limits and must not treat
a passing ratio as a bomb check.

Status: Implemented (decision-of-record only; no code change — the rustdoc caveat in
`src/extraction.rs` already states the operative guidance and is now backed by this record).

### Implementation

No source change. The `extract_some` rustdoc already carries the caveat and the mitigation; this
record supplies the missing disposition so the caveat reads as a settled boundary rather than an
unclosed hole. Reviewers who reach this gate should be answered with this record.

## Consequences

* Good, because the guarantee is now stated honestly: the ratio gate is not a per-entry bomb check
  for subsets, and no caller can be misled into treating it as one.
* Good, because it avoids a per-format accounting pass that would add complexity without covering
  the formats that actually carry the risk.
* Bad, because a caller who configures only `max_compression_ratio` and leaves the byte caps at
  their defaults is less protected on a selective extraction than on a full one. Accepted and
  documented; the defaults are finite, so the exposure is bounded by `ExtractionLimits::default`
  rather than unbounded.
