# AD: Multipart detection heuristic tightening + ZIP sort fix

## Context and Problem Statement
Found in Review 0001 (Issues R0001-0004 and R0001-0005, Severity: High).
Location: `src/inspection.rs:403-509`

Two issues in `detect_multipart()`:
1. The `stem_boundary` closure accepted any sibling file whose next character
   after the stem was `.`, matching unrelated files like `archive.backup.zip`
   for base `archive.zip`.
2. The sort placed numbered parts (`.z01`, `.z02`) before the main `.zip`
   file, producing `[archive.z01, archive.z02, archive.zip]` instead of the
   expected `[archive.zip, archive.z01, archive.z02]`.

## Decision Drivers
* False positives in multipart detection can mislead operators and downstream tooling
* ZIP split convention places the main `.zip` file as the caller-facing entry point
* The boundary check ran before format-specific extension validation, widening false matches

## Considered Options
1. Tighten `stem_boundary` to require recognized multipart extension patterns; fix sort ordering
2. Move to format-specific parsers (more precise but larger refactor)

## Decision Outcome
ACCEPT (Option 1): Tightened `stem_boundary` and reversed `None` sort ordering.

Status: Implemented

### Implementation
- `src/inspection.rs` `stem_boundary`: Now validates that the suffix after the stem
  matches a recognized multipart pattern (`.zNN`, `.NNN`, `.partN`, `.zip`, `.rar`)
  instead of accepting any dot-prefixed suffix.
- `src/inspection.rs` sort: Reversed the `(Some, None)` and `(None, Some)` ordering
  so non-numbered files (`.zip` main file) sort first, before numbered parts.

## Consequences
* Good, because `archive.backup.zip` no longer matches as a multipart part of `archive.zip`
* Good, because ZIP split sets now sort with the main `.zip` file first
* Bad, because any caller that depended on the old sort order needs updating (unlikely — the old order was unintuitive)
