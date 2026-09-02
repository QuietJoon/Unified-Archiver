---
type: ADR
title: "AD: Reject blanket portability sweep over `/Volumes/Temp/claude/` scratch paths"
description: "REJECT reversed (2026-07-20 owner reversal); superseded by the consolidated scratch-path policy in AD 0036 (Option 3) and AD 0041 — test scratch literals now route through temp_test_dir()."
tags: [decision, ADR-0029, R0060-0013, R0060-0075]
timestamp: 2026-04-18T00:00:00Z
status: superseded
---

# AD: Reject blanket portability sweep over `/Volumes/Temp/claude/` scratch paths

## Context and Problem Statement
Found in Review 0060 (Issues R0060-0013 through R0060-0075, Severity: Low).

Review 0060 raised 63 separate Low issues flagging every occurrence of
`/Volumes/Temp/claude/...` as machine-specific and recommending replacement
with `tempfile::tempdir()` or `std::env::temp_dir()`. The flagged locations
span:

- `CLAUDE.md:16`, `AGENTS.md:25` — project development guidelines
- `examples/create_archive.rs`, `examples/modify_archive.rs`
- `tests/integrity_comprehensive_test.rs`, `tests/integrity_edge_cases_test.rs`,
  `tests/integration/modification.rs`, `tests/recovery_percentage_edge_cases.rs`,
  `tests/stream_crc_test.rs`, `tests/batch_extraction_test.rs`,
  `tests/windows_rename_test.rs`, `tests/large_zip_mmap_test.rs`,
  `tests/common/config.rs`
- `docs/architecture/bootstrap-config.md`, `docs/project/intake.md`

## Decision Drivers
* `CLAUDE.md:16` ("Use/Pass /Volumes/Temp/claude/7zip/ temp dir when develop
  and test") is the **project's explicit scratch-path convention**. The path
  is not an accident — it is the documented instruction.
* `AGENTS.md:25` says "for example, `/Volumes/Temp/claude/7zip/` on macOS" —
  same convention in the sibling agent-instruction file.
* User's `.claude/memory/feedback_temp_dir_fallback.md` records the same
  convention: "Use /Volumes/Temp/claude when /tmp has permission issues."
* The repository is a single-developer workspace. CI portability is out of
  scope for the MVP.
* Many tests already combine the hardcoded path with `tempfile::tempdir_in(...)`
  or `env::set_var("TMPDIR", ...)` for per-test isolation, giving the hybrid
  the best of both worlds — fixed scratch root, per-test unique subdirs.
* Blanket replacement with `tempfile::tempdir()` (platform default) would:
  - Drop the project's central-scratch convention on the floor.
  - Move scratch to `/var/folders/...` on macOS, outside the developer's
    documented and backed-up scratch volume.
  - Trigger a 63-file sweep affecting test determinism in ways the reviewer
    did not analyze per-test.

## Considered Options
1. Accept and sweep all 63 locations to `tempfile::tempdir()` — rejected:
   breaks the documented convention and provides no benefit on a
   single-developer workspace.
2. Accept only the user-facing examples (`examples/*.rs`) — rejected:
   examples serve as manual-run demos for this developer; consistency
   with the rest of the repo is preferable.
3. Reject the whole class with a pointer to the convention — selected.

## Decision Outcome
REJECT: We decided for option 3 because the `/Volumes/Temp/claude/`
scratch-path usage is a deliberate, documented, centrally-recorded project
convention — not an oversight. The reviewer flagged a pattern without
recognizing the documentation that mandates it.

Status: Implemented (review 0060 archived with no code changes in this class).

### Implementation
No code change. The convention remains:

- New tests/examples SHOULD use `/Volumes/Temp/claude/...` as their scratch
  root, optionally combined with `tempfile::tempdir_in(...)` for per-test
  isolation inside that root.
- Tests that need platform-portable scratch (e.g., CI-first additions) MAY
  use `tempfile::tempdir()` or `std::env::temp_dir()`, but existing tests
  SHOULD NOT be mass-rewritten.

If this repository ever acquires CI or ships to multiple developers, a
one-shot cleanup will become appropriate — at which point this AD can be
superseded.

## Consequences
* Good, because the documented scratch-path convention remains authoritative
  across code, tests, examples, and documentation.
* Good, because 63 separate diffs are avoided — no churn, no review surface
  inflation, no merge-conflict risk with in-flight work.
* Good, because the single-developer workspace continues to rely on a known
  local scratch volume (mounted from known storage, not `/var/folders/...`).
* Bad, because the project cannot be cloned on a Linux/Windows/CI machine
  and have its tests pass without first creating `/Volumes/Temp/claude/` or
  overriding scratch paths. This is acceptable given MVP scope; it becomes
  a problem to revisit if/when CI lands.

## Amendment (2026-07-20, owner reversal)

**The REJECT above is reversed.** The load-bearing premise of the
original decision — "The repository is a single-developer workspace. CI
portability is out of scope for the MVP." — no longer holds:

* The platform matrix is now **first-class Win / macOS / Linux**, with
  committed Windows CI. Tests must pass on a clean checkout of any of the
  three without the operator first creating a machine-specific scratch
  directory or overriding scratch paths — precisely the "Bad" consequence
  the original record flagged as acceptable-only-under-MVP-scope.
* The examples/library half of this class was **already reversed by
  [AD 0041](AD-0041-remove-hardcoded-temp-path-from-library.md)**
  ("Remove hardcoded `/Volumes/Temp/claude` from library and examples"),
  which stripped the host-specific path out of `open_at_offset()` and both
  example binaries. AD 0041 explicitly left *test* code untouched; this
  amendment closes that remaining gap.

### What changed in the tests

The ~21 remaining hardcoded scratch-path occurrences under `tests/` were
removed, following **[AD 0036](AD-0036-reject-tempfile-tempdir-for-test-scratch-paths.md)
Option 3** — centralise scratch-path construction in
`tests/common/mod.rs::temp_test_dir()` rather than fan
`tempfile::tempdir()` across every call site:

* **15 live scratch-path literals** now route through
  `common::temp_test_dir()`, which resolves via
  `UNIFIED_ARCHIVE_TEMP_DIR` → `.unified-archive.toml` →
  `std::env::temp_dir()` and hardcodes no absolute path:
  * `tests/integration/modification.rs` — 11 (archive-create / extract
    scratch paths; the now-unused `PathBuf` import was dropped)
  * `tests/batch_extraction_test.rs` — 1 (the former `TEST_TEMP_DIR`
    const, now `common::temp_test_dir().join(test_name)`)
  * `tests/recovery_percentage_edge_cases.rs` — 3 (invalid / truncated /
    empty `.rar` scratch files; the file now imports `common`)
* **6 remaining occurrences** were scrubbed of the machine-specific
  absolute path with no behavioural change:
  * `tests/common/config.rs` — 2 TOML-parser fixtures re-pointed to a
    neutral example path (`/example/archive-tmp`). That test exercises
    the config *parser*, not scratch-dir creation, so it is deliberately
    not routed through the helper.
  * historical prose in `tests/windows_rename_test.rs`,
    `tests/integration/compression_builder_split.rs`,
    `tests/integrity_comprehensive_test.rs`, and `tests/README.md` — the
    literal path in comments/docs was reworded to a generic
    "machine-specific scratch path".

The runtime escape hatch is unchanged: the suite is still invoked with
`TMPDIR=/Volumes/Temp/claude`, so on this host `temp_test_dir()` (via
`std::env::temp_dir()`) still lands on that scratch volume. The point of
the reversal is that the **committed source no longer bakes in that
path** — it is supplied at run time by the environment, exactly as
AD 0041 arranged for the library and examples.

### Consolidated scratch-path policy

Three records now define the policy and should be read together:

* **[AD 0036](AD-0036-reject-tempfile-tempdir-for-test-scratch-paths.md)** —
  do not fan `tempfile::tempdir()` across call sites; centralise in
  `temp_test_dir()` (Option 3). This amendment is that Option 3 applied to
  the last hardcoded literals.
* **[AD 0041](AD-0041-remove-hardcoded-temp-path-from-library.md)** —
  library and example code honour `TMPDIR` / `tempfile`; no host-specific
  path in shipped artefacts.
* **This record (docs/records/0029, as amended)** — test code carries no
  hardcoded absolute scratch path in source; it routes through
  `temp_test_dir()` (project-rooted when configured, `TMPDIR` / OS-temp
  otherwise).

The original Decision Outcome, Implementation, and Consequences above are
retained verbatim as the historical record; this amendment supersedes
their operative guidance for new and existing tests, and the frontmatter
`status` is updated to `superseded` to reflect that.

## Amendment (2026-09-03, AD-0070 — "committed Windows CI" is not a thing this project has)

The owner-reversal amendment above justifies first-class Windows support partly
by pointing at "committed Windows CI". **There is no committed CI job, and
under AD-0070 there will not be one.** The owner runs verification on each
platform themselves and the result is recorded under `docs/verification/`; no
hosted CI service is adopted for any lane.

**This changes nothing about the decision.** The rejection of the scratch-path
portability sweep stands on its own reasoning, and so does the first-class
status of Windows — what changes is only the mechanism by which Windows gets
verified, which is the owner's Windows machine rather than a job in a hosted
runner. The phrase is corrected here because a reader checking whether this
record's premise holds would go looking for a CI configuration that does not
exist and conclude the premise had failed.

See AD-0070 for the definition of what counts as verified, and for why three of
the four items previously blocked on "no CI" were never blocked on a service at
all.
