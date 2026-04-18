# AD: Clippy lint baseline restoration and dead code strategy

## Context and Problem Statement
Found in Review 0003 (Issues R0003-001, R0003-002, R0003-011, R0003-017, R0003-018, Severity: High/Medium).
Location: multiple `src/ffi/*.rs` files, `src/archive.rs`

The crate failed `cargo clippy -- -D warnings` with 45 errors: unused FFI
bindings, unused wrapper methods, large enum variant, and style lints. The
task tracker (T124) still claimed the lint gate was clean. Three categories
needed a strategy decision rather than mechanical deletion.

## Decision Drivers
* FFI binding surfaces (`libarchive.rs`, `unrar.rs`) are intentionally
  complete — removing "unused" declarations would force re-binding on every
  feature addition
* Backend wrapper methods (`path()`, `extract_all()`, `extract_file()`) form
  a uniform internal API across all backends, even though the façade does not
  call every method on every backend
* `ArchiveBackend::ZipWriter` stored a ~664-byte `ZipWriter` inline, inflating
  all other (read-side) variant sizes

## Considered Options
1. Prune all dead code (removes FFI completeness and backend uniformity)
2. `#[allow(dead_code)]` with justification comments (retains completeness)
3. Box the large variant to solve size disparity (standard Rust pattern)

## Decision Outcome
ACCEPT (Options 2 + 3):
- FFI modules: `#![allow(dead_code)]` at module level with "intentionally
  complete binding surface" documentation
- Wrapper impl blocks: `#[allow(dead_code)]` with "uniform backend API
  surface" documentation
- `ArchiveBackend::ZipWriter`: `Box<ZipWriter>` (reduces variant from ~664
  bytes to 8 bytes)
- Style lints (hex grouping, saturating_sub, useless conversion, map_or):
  fixed directly

Status: Implemented

### Implementation
- `src/ffi/libarchive.rs`: `#![allow(dead_code)]` module-level
- `src/ffi/unrar.rs`: `#![allow(dead_code)]` module-level
- `src/ffi/{libarchive_wrapper,piz_wrapper,sevenz_wrapper,zip_wrapper,wrapper,zip_writer}.rs`:
  `#[allow(dead_code)]` on impl blocks
- `src/archive.rs`: `ZipWriter(Box<ZipWriter>)`
- `src/creation.rs`: `Box::new(writer)` at construction site
- `src/stream_crc.rs`: hex literal grouping, saturating_sub
- `src/ffi/piz_wrapper.rs`: removed useless `usize::try_from`
- `src/ffi/sevenz_wrapper.rs`: `map_or` → `is_some_and`
- `specs/001-unified-archive/tasks.md`: T124 status updated

## Consequences
* Good, because `cargo clippy -- -D warnings` now passes cleanly (0 errors)
* Good, because FFI binding completeness is preserved for future features
* Good, because backend API uniformity is maintained
* Good, because `ArchiveBackend` enum size is now dominated by read-side
  variants (~200–300 bytes) rather than the writer (~664 bytes)
* Bad, because the `#[allow]` annotations may mask genuinely dead code in the
  future — periodic audit recommended
