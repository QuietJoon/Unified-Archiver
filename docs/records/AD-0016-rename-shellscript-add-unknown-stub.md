---
type: ADR
title: "AD: Rename ShellScript to ScriptInterpreter and add Unknown stub variant"
description: "Implemented"
tags: [decision, ADR-0016, R0027-0001, R0027-0004, R0027-0005, OI-0027-001]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# AD: Rename ShellScript to ScriptInterpreter and add Unknown stub variant

## Context and Problem Statement
Found in Review 0027 (Issues R0027-0001, R0027-0004, R0027-0005).
Location: `src/sfx/stub_types.rs`

The `StubType::ShellScript` variant name was misleading since shebang-based detection covers Python, Perl, Ruby, and other interpreted scripts beyond shell. Additionally, there was no way to represent an SFX archive with an unrecognized stub type.

## Decision Drivers
* Accuracy: shebang detection covers all `#!` interpreters, not just shells
* Extensibility: future SFX scanning may encounter unknown executable formats
* `is_native()` semantics must remain correct for all variants

## Considered Options
1. Rename `ShellScript` to `ScriptInterpreter` and add `Unknown` variant
2. Keep `ShellScript` name, only add `Unknown`
3. Add `Unknown` variant and change detection to scan through unknown stubs

## Decision Outcome
ACCEPT: Option 1 — rename to `ScriptInterpreter`, add `Unknown` variant. Detection behavior unchanged (Unknown exists for future use; current detection still returns `not_sfx()` for unrecognized stubs). Scanning through Unknown stubs tracked as OI-0027-001.

Status: Implemented

### Implementation
- `src/sfx/stub_types.rs`: Renamed `ShellScript` to `ScriptInterpreter`, added `Unknown` variant
- `ScriptInterpreter` remains native on Unix platforms (shebangs are executable)
- `Unknown` returns `false` for `is_native()` on all platforms
- All references updated in detection.rs, result.rs, and test files

## Consequences
* Good, because the name accurately reflects what shebang detection covers
* Good, because `Unknown` enables future extension without breaking changes
* Neutral: detection behavior unchanged for now (OI-0027-001 tracks the extension)
