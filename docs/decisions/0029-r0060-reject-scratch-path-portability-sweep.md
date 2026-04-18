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
