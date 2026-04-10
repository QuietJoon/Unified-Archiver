# AD: Remove standalone gz/bz2/xz support claims

## Context and Problem Statement
Found in Review 026 (Issues R026-006, R026-007, R026-008, Severity: HIGH).
Location: `src/lib.rs`, `docs/API_REFERENCE.md`

The crate documentation and supported-formats table claimed read/extract support for standalone `.gz`, `.bz2`, and `.xz` files. In reality, these `ArchiveFormat` variants only work as part of TAR compound formats (`.tar.gz`, `.tar.bz2`, `.tar.xz`). Attempting to open a standalone compressed file results in a runtime error.

## Decision Drivers
* Documentation must not promise capabilities that don't work
* Removing enum variants would be a breaking API change
* Standalone format support is a reasonable future feature

## Considered Options
1. Remove the enum variants entirely (breaking change)
2. Document the limitation and track standalone support as a future task
3. Implement standalone support immediately

## Decision Outcome
ACCEPT: Option 2 — keep enum variants but document the limitation. Added footnotes in `src/lib.rs` and `docs/API_REFERENCE.md`. Tracked as OI-026-003 for future implementation.

Status: Implemented (documentation), tracked (feature)

### Implementation
- `src/lib.rs`: Added `‡` footnote to GZIP/BZIP2/XZ rows explaining limitation
- `docs/API_REFERENCE.md`: Added note to Archive::open supported formats and ArchiveFormat enum

## Consequences
* Good, because users are no longer misled about standalone format support
* Good, because no breaking API change
* Bad, because the limitation still exists (tracked for future work)
