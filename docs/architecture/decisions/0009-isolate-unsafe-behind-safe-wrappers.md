# ADR: Isolate Unsafe Behind Safe Wrappers

## Context and Problem Statement
RAR and libarchive require native FFI with unsafe code, but the crate wants a stable Rust-facing API and auditable safety boundaries. Allowing unsafe to leak across the crate makes auditing difficult and increases the risk of soundness bugs.

## Decision Drivers
- Concentrated unsafe code in as few modules as possible
- Centralized error translation from C error codes to Rust error types
- Stable public API that does not expose FFI details

## Considered Alternatives
- **Call raw FFI from orchestration** -- rejected because unsafe leaks across the entire crate, making safety audits impractical.
- **Reimplement all formats in pure Rust** -- rejected because capability and compatibility gaps are too large for RAR and some libarchive-supported formats.

## Decision Outcome
We decided to keep low-level bindings in `unrar.rs`/`libarchive.rs` and expose only safe wrapper modules to the orchestration layer, because it concentrates unsafe code, centralizes resource management and error translation, and prevents unsafe from leaking across the crate.

## Consequences
- Good: Concentrates unsafe code, centralizes resource management and error translation, and prevents unsafe from leaking across the crate.
- Bad: Wrapper layers duplicate some concepts and add conversion code; backend-specific capabilities are harder to surface cleanly through the safe abstraction.
