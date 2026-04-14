# AD: `Archive::modify_with_options` is an additive extension, not a replacement

## Context and Problem Statement
Found in DCR-001 / OI-025-003 (Severity: MEDIUM).
Location: `src/modification.rs`, `src/archive.rs`

`ModificationOptions` was defined in the public surface (`preserve_metadata`, `create_backup`, `backup_suffix`) but never threaded into the modify/commit pipeline. The struct existed as dead API. Closing the gap required a way to pass these options into `commit_changes()` without breaking the existing `Archive::modify(path)` callers — many of which sit on the public API surface and in user code.

## Decision Drivers
* Non-breaking: existing callers of `Archive::modify(path)` must compile and behave identically.
* Discoverable: the new options surface should be obvious to new callers, not hidden behind a builder or a config file.
* Symmetric with `Archive::create()`, which takes an options struct as its second argument.
* Single source of truth: the option values live with the `Archive` handle for the duration of the modify session, so `commit_changes()` can read them without an extra parameter.

## Considered Options
1. **Add `Archive::modify_with_options(path, opts)` as a sibling to `modify(path)`** (chosen). Existing `modify()` keeps its zero-config signature; new method is the entry point for opt-in behaviors.
2. **Change `modify(path)` to `modify(path, opts)`.** Breaking change for every existing caller.
3. **Add `commit_changes_with_options(opts)`.** Less symmetric with `create()`; also leaks the option lifetime into the commit boundary instead of treating it as session-scoped.
4. **Builder pattern: `Archive::modify(path).with_backup(...).commit()`.** More machinery for a struct with three fields; doesn't compose well with the existing `add_entry` / `remove_entry` / `replace_entry` flow that mutates `&mut self`.
5. **Read options from a thread-local or environment variable.** Hidden coupling; rejected on principle.

## Decision Outcome
ACCEPT: Option 1.

```rust
impl Archive {
    pub fn modify(path: impl AsRef<Path>) -> Result<Self> { /* unchanged */ }
    pub fn modify_with_options(
        path: impl AsRef<Path>,
        options: ModificationOptions,
    ) -> Result<Self> { /* new */ }
}
```

Internally, `modify_with_options` delegates to `modify(path)` and stores the options on a new `pub(crate) mod_options: Option<ModificationOptions>` field on `Archive`. `commit_changes()` reads the field once via `take()` and applies each honored field before the atomic rename:

- `create_backup = true`: copy the original archive to `<path>.<normalized_suffix>` *after* the temp file is fully written but *before* the atomic rename. If the commit aborts before the rename, the backup is not left behind for a no-op commit.
- `backup_suffix`: normalized via `backup_path_for()` — empty → `.bak`, `".bak"` → `.bak`, `"bak"` → `.bak`. Three cases collapse to one filesystem rule.
- `preserve_metadata`: accepted but currently a no-op (metadata pipeline lands with Phase C.2 / OI-025-002). Documented in the contract as "scheduled for Phase C.2."

Status: Implemented (Phase B.2 of the in-flight remediation plan).

### Implementation
- `src/archive.rs`: added `pub(crate) mod_options: Option<ModificationOptions>`; initialized to `None` in `new_read` and `Self::create`.
- `src/modification.rs`: added `modify_with_options`; added `backup_path_for()` free function; updated `commit_changes()` to honor `create_backup` + `backup_suffix`.
- `tests/modification_options_test.rs`: 4 tests (backup written before rename; default disabled; suffix normalization; no-op commit skips backup). All passing.
- `specs/001-unified-archive/contracts/archive.md`: documents `Archive::modify_with_options` and `ModificationOptions`.

## Consequences
* Good, because every existing caller of `Archive::modify` continues to work with no source change.
* Good, because new functionality is opt-in and discoverable through the type system (the `ModificationOptions` argument).
* Good, because `commit_changes()` keeps its zero-argument signature; option state is carried by the `Archive` handle, matching the lifetime where the options actually need to live.
* Good, because the backup write happens *after* the temp file is built — no stale `.bak` if the commit fails before rename.
* Bad, because `preserve_metadata = true` is currently a silent acceptance (no warning emitted yet). Will be honored when Phase C.2 lands; acceptable to ship this way because no caller depended on metadata preservation before this commit.
* Neutral, because `modify_with_options` delegates to `modify()`, so any future format-gating changes to `modify()` (e.g., enabling RAR/TAR modify) flow through automatically.
