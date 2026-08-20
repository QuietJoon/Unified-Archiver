---
type: ADR
title: "AD 0038: Tighten progress-callback and overwrite contracts"
description: "Review 0061 raised four findings (R0061-0004, R0061-0005, R0061-0006, R0061-0007) pointing at assertion sites that read \"may or may not\" — patterns…"
tags: [decision, ADR-0038, R0061-0004, R0061-0005, R0061-0006, R0061-0007]
timestamp: 2026-04-18T00:00:00Z
status: active
---

# AD 0038: Tighten progress-callback and overwrite contracts

## Context and Problem Statement

Review 0061 raised four findings (R0061-0004, R0061-0005, R0061-0006, R0061-0007) pointing at assertion sites that read "may or may not" — patterns that document a nominal API surface without actually testing that the backend does the thing the API promises. These sites were:

* `tests/contract/extraction_contract.rs` — ZIP progress callback was asserted to not cause errors, but not asserted to fire.
* `tests/contract/progress_contract.rs` — cancellation contract's `extract_all` return was unbound (`let _result = …`) and the assertion about effect of `ControlFlow::Break` was a no-op inside `if calls > 1 { }`.
* `tests/progress_callback_test.rs::test_progress_callback_cancellation` — marked `#[ignore]` with a comment that "small test archives (97 bytes) complete before cancellation callback is invoked", which is really a fixture problem, not a test problem.
* `tests/integration/extraction.rs` — overwrite=false case extracted twice and documented "may or may not fail" rather than asserting the reject-on-exists default.

Soft-contract tests like these hide a class of regressions in which a backend silently starts ignoring the API surface — nothing fails because nothing is actually checked.

## Decision Drivers

* Contract tests must assert observable backend behaviour, not document API-surface tolerance.
* A backend that silently ignores `ControlFlow::Break` is a correctness bug on a cancellation-carrying API, not a permissible variation.
* The `#[ignore]` attribute should be reserved for genuinely environment-dependent tests, not for tests whose fixtures are too small — that is a content problem with a content fix.

## Considered Options

1. **Keep the soft assertions.** Rejected: they contribute no regression coverage.
2. **Delete the tests.** Rejected: the underlying behaviour (progress callbacks fire; cancellation short-circuits; overwrite=false errors) is the contract we want tested.
3. **Tighten each site to assert the actual contract, regenerating fixtures where necessary.** Accepted.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

### R0061-0004 — `tests/contract/extraction_contract.rs::contract_extraction_options_accept_progress_callback`

Added `assert!(call_count.load(...) > 0, "ZIP backend must invoke progress callback at least once")` after the existing `result.is_ok()` check. Comment now identifies this as the ZIP arm of a contract whose RAR, 7z, and libarchive arms live in `tests/progress_callback_test.rs`.

### R0061-0005 — `tests/contract/progress_contract.rs::contract_progress_cancellation`

* Switched fixture from `test.rar` (1 entry) to new `tests/fixtures/test_multi.rar` (5 entries).
* Bound `extract_all` to `result` (was `_result`).
* Added `assert!(calls < total_entries || result.is_err(), ...)` — a backend that silently ignores `ControlFlow::Break` must fail this assertion.

### R0061-0006 — `tests/progress_callback_test.rs::test_progress_callback_cancellation`

* Removed `#[ignore]`.
* Restored `#[cfg(feature = "rar-support")]` (was dropped when the test was ignored).
* Switched fixture to `test_multi.rar`.
* `ControlFlow::Break` now fires on the second callback; assertion is `result.is_err() || calls < total_entries`.
* Uses `std::process::id()` in the destination name so parallel runs of the test binary do not collide.

### R0061-0007 — `tests/integration/extraction.rs::test_extract_overwrite_handling`

Bound the no-overwrite `extract_all` call to `result_no_overwrite` and asserted `result.is_err()`. This aligns with `check_overwrite_conflicts` in `src/extraction.rs`, which rejects pre-existing destination files uniformly for every backend when `overwrite: false`.

### New fixture

`tests/fixtures/test_multi.rar` — 5-entry RAR5 archive built with the `rar` CLI (m3 compression, no paths). Each entry is ~2 KB of random text padding so that progress callbacks fire multiple times during extraction. The fixture is committed to the repo because generating RAR archives at test time requires the `rar` binary, which is a non-redistributable build dependency.

## Consequences

* The four contract tests now fail noisily if a backend regresses on progress invocation, cancellation short-circuit, or overwrite rejection.
* `test_progress_callback_cancellation` runs by default; CI no longer silently skips it.
* The `#[ignore]` attribute is reserved for its proper use in this suite (tests that genuinely require external infrastructure we cannot assume is present).

## Revisit trigger

Re-open this decision if:

* A backend is added (e.g. a future 7z streaming backend) whose cancellation semantics genuinely cannot match `ControlFlow::Break` — the assertion would need a per-backend skip list, not a softening back to "may or may not".
* The `test_multi.rar` fixture becomes too small to exercise cancellation; regenerate with more/larger entries.
