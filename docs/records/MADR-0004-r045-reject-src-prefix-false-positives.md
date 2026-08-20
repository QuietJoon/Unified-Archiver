---
type: ADR
title: "AD: Reject the 39 \"missing source file\" issues — reviewer false positives"
description: "Implemented (2026-04-14, no source changes — decision-only)."
tags: [decision, ADR-0004, R0045-0077, R0045-0115]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Reject the 39 "missing source file" issues — reviewer false positives

## Context and Problem Statement

Found in Review 0045 (Issues R0045-0077 through R0045-0115 — 39 issues, all Low).

The reviewer flagged paths like `ffi/wrapper.rs`, `ffi/libarchive_wrapper.rs`,
`ffi/piz_wrapper.rs`, `ffi/sevenz_wrapper.rs`, `ffi/zip_wrapper.rs`,
`ffi/zip_writer.rs`, `sfx/detection.rs`, `sfx/signatures.rs`,
`sfx/stub_types.rs`, and `external/rar.rs` as "missing" in a series of
architecture documents.

Those files all exist, and the source documents reference them with the
correct `src/` prefix. For example, `docs/architecture/README.md` writes:

```
| Backend Adapters | `src/ffi/wrapper.rs`, `src/ffi/libarchive_wrapper.rs`,
  `src/ffi/piz_wrapper.rs`, `src/ffi/sevenz_wrapper.rs`,
  `src/ffi/zip_wrapper.rs`, `src/ffi/zip_writer.rs` | ... |
```

The reviewer's broken-link checker appears to strip the `src/` prefix before
existence-checking, then reports paths as missing.

Source documents verified by direct read:

* `docs/architecture/README.md` — Backend Adapters / Native Bindings / SFX
  Pipeline / External Tools rows in the "Major Module Groups" table all
  use the correct `src/...` prefix.
* `docs/architecture/config-surface.md` — references to `src/ffi/...` and
  `src/sfx/...` are correct.
* `docs/architecture/persistence-and-files.md` — references are correct.

## Decision Drivers

* The documentation is correct as written; "fixing" it by removing the
  `src/` prefix would break links that are currently relative to the repo
  root and are correct.
* The same reviewer (or one with the same checker) is likely to flag this
  again on the next pass; a written decision short-circuits the relitigation.

## Considered Options

1. Reject all 39 issues; record a decision identifying the reviewer's
   prefix-stripping bug so future passes can skip them.
2. Rewrite all references to use repo-root-relative paths without the `src/`
   prefix to placate the broken checker.
3. Add a `.linkcheckignore` or similar configuration to silence the checker
   (only useful if the checker is configurable; the reviewer here is an
   automated agent, not a CI tool).

## Decision Outcome

**REJECT all 39 issues** — Option 1.

The source documents are correct. Mutating correct documentation to satisfy
a checker bug would degrade clarity (paths would no longer match how they
appear when developers `cd` into the repo and `ls src/`).

Status: Implemented (2026-04-14, no source changes — decision-only).

### Implementation

No file changes. This decision record is the implementation; future review
gates will cite it when the same false positives reappear.

## Consequences

* Good — The architecture docs continue to use clear, correct,
  repo-root-relative paths.
* Good — Future review gates have a documented basis for batch-rejecting
  the same false positives without re-investigating each one.
* Bad — None directly. If a future reviewer's checker is fixed and the
  paths become genuinely missing (e.g., due to a refactor that moves files
  out of `src/`), this AD will not catch it; the next review's findings
  must be evaluated on their own merits.
