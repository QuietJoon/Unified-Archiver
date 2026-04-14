# AD: Split Archive Behavior Across Modules

Status: Accepted

## Context and Problem Statement
The public API centers on one `Archive` type, but inspection, extraction, creation, and modification have materially different control flow. Placing all logic in a single file would make it unmaintainable, while splitting into separate public types would fragment the API.

## Decision Drivers
- Single public facade for consumers
- Manageable file sizes for maintainers
- Module boundaries aligned with user operations (inspect, extract, create, modify)

## Considered Alternatives
- **All logic in one file** -- rejected because it quickly becomes unmaintainable as operations grow.
- **Separate service structs per operation** -- rejected because it fragments the public API and forces consumers to juggle multiple types.

## Decision Outcome
We decided to keep `Archive` state and backend routing in `src/archive.rs` and implement most behavior in separate `impl Archive` blocks in operation-focused modules, because it preserves the single public facade without turning `archive.rs` into an unmaintainable catch-all.

## Consequences
- Good: Preserves single public facade without turning `archive.rs` into an unmaintainable catch-all.
- Bad: Cross-file discoverability is weaker for contributors, and `Archive` remains the coupling hub for all operations.
