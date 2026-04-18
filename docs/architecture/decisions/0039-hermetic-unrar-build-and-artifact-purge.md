# AD 0039: Hermetic UnRAR build and purge of checked-in build artifacts

## Context and Problem Statement

Review 0062 raised three related findings about `build.rs` and the UnRAR vendored source tree:

* **R0062-0001** — `build.rs` ran `make lib` directly in `src/ffi/native/unrar/`, writing `.o`/`.a` outputs next to the vendored C++ source. Every RAR-enabled build mutated the tracked source tree.
* **R0062-0002** — `cargo:rerun-if-changed=src/ffi/native/unrar` watched the same directory that `make lib` rewrites, producing avoidable rebuild churn.
* **R0062-0013..0062** (50 findings) — 50 stale `.o` object files from a previous pre-hermetic build were sitting in the vendored source tree. They were in `.gitignore` and therefore not tracked, but they were on disk and obscured the source tree.

Review 0062 itself ran `cargo test` and hit a different failure (`creation_roundtrip_test` / 7z temp path), but the hermetic-build finding and the stale-artifact cluster are independent of that.

## Decision Drivers

* The vendored UnRAR source tree is a third-party snapshot checked into the repo. Build artifacts have no business living there — the build must not mutate its own inputs.
* Cargo's `rerun-if-changed` must watch a set that does not intersect the build's own output set; otherwise it forces rebuilds that are not driven by source changes.
* A source tree that looks mutable (because `make` writes into it) creates friction for `git status`, tarball packaging, and anyone inspecting the tree.

## Considered Options

1. **Leave the current build in place; just `.gitignore` the artifacts.** Rejected: the tree still appears "dirty" to tools that operate on the working directory; `rerun-if-changed` still churns.
2. **Run `make lib` with `-C` pointing at `$OUT_DIR` after copying headers.** Rejected: the vendored makefile uses relative paths and assumes cpp files live alongside the makefile; redirecting just the object-dir would require patching the makefile.
3. **Stage the full vendored source tree into `$OUT_DIR/unrar-build/` and run `make lib` there.** Accepted.

## Decision Outcome

ACCEPT option 3. Status: Implemented.

## Implementation

### `build.rs` — hermetic build

* `build_unrar()` now stages the vendored sources into `$OUT_DIR/unrar-build/` via `stage_unrar_sources()` and runs `make lib` in the staged copy.
* `is_unrar_artifact()` filters out stale `.o`/`.a`/`.so`/`.dylib`/`unrar`/`default.sfx` from the staging copy so no pre-existing objects are staged.
* `copy_dir_recursive()` performs the staging walk.
* `emit_unrar_rerun()` now watches only the makefile plus `*.cpp`/`*.hpp`/`*.h` under the vendored tree — explicitly not the directory itself, and not the makefile-emitted artifacts.
* The final `libunrar.a` is copied from `$OUT_DIR/unrar-build/` into the link-search dir that `cargo:rustc-link-search` points at.

### Source-tree cleanup

All 50 stale `.o` files under `src/ffi/native/unrar/` (R0062-0013..0062) were deleted from disk. They were not tracked by git (listed in `.gitignore`), so this was a delete-from-disk-only operation; no `git rm --cached` was needed. The vendored tree is now clean.

### Verification

`touch build.rs && cargo build --all-features` leaves `src/ffi/native/unrar/` untouched; artifacts land under `$OUT_DIR/unrar-build/` exclusively.

## Consequences

* RAR-enabled builds are hermetic: the source tree is an input only.
* `cargo:rerun-if-changed` is scoped to actual inputs; no more spurious rebuilds from the build's own outputs.
* Staging copies the full vendored tree on every clean build. This is a one-time cost (~300 files, < 5 MB) and is dominated by the subsequent C++ compile.
* Future contributors editing UnRAR sources now need to edit the original `src/ffi/native/unrar/*.cpp` — editing the staged copy under `target/` will be wiped on the next build.

## Revisit trigger

Re-open if:

* The staging copy cost becomes measurable (e.g. on an NFS-backed `CARGO_TARGET_DIR`).
* We move to an entirely different RAR implementation that no longer needs a vendored C++ source tree.
