# AD: Single Archive Facade with Backend Enum

Status: Accepted

## Context and Problem Statement
Consumers need a format-agnostic API instead of per-format call paths. Without a unified entry point, callers must know which backend to use for each archive format, leading to duplicated dispatch logic and tight coupling to implementation details.

## Decision Drivers
- API simplicity for consumers
- Format-agnostic code paths
- Consistent operation signatures across all supported formats

## Considered Alternatives
- **Separate public types per format** -- rejected because it fragments the API surface and forces consumers to handle format dispatch themselves.
- **Trait-object plugin registry** -- rejected due to extra complexity and lifetime/ownership overhead that outweighs the flexibility gained.

## Decision Outcome
We decided to centralize the public API around `Archive`, with backend-specific behavior hidden in `ArchiveBackend` variants, because it minimizes the surface area consumers must learn and keeps operation signatures consistent regardless of the underlying format.

## Consequences
- Good: Minimizes user-facing complexity and keeps operation signatures consistent across formats.
- Bad: Central orchestration code carries broad responsibility and can become a coupling hotspot as new formats and operations are added.
