# AD: Advisory File Locking for Modify Mode (R052-004)

## Context and Problem Statement
Found in Review 052 (Issue R052-004, Severity: High).
Location: `src/modification.rs` — `Archive::modify()`

The `modify()` operation reads an archive, applies changes, and writes the result back to the same path. Without file-level locking, concurrent `modify()` calls on the same archive could corrupt it. No external coordination existed.

## Decision Drivers
* Concurrent modify calls on the same file must be prevented
* Advisory (not mandatory) locking is appropriate — the library is cooperative, not a security boundary
* Non-blocking lock acquisition (`try_lock_exclusive`) avoids deadlocks
* Lock must release automatically on drop (RAII pattern)

## Considered Options
1. Advisory file locking via `fs2` crate (cross-platform `flock`/`LockFileEx`)
2. PID-file approach (create `archive.zip.lock` marker file)
3. No locking — document as caller responsibility (rejected)

## Decision Outcome
ACCEPT (Option 1): Advisory file locking via `fs2::FileExt::try_lock_exclusive()`.

The lock is acquired early in `modify()` (after format/encryption checks, before backend open). The `File` handle is stored in the `Archive` struct and released automatically when the handle drops.

Status: Implemented

### Implementation
- `Cargo.toml`: Added `fs2 = "0.4"` dependency
- `src/modification.rs`: Lock acquisition after encrypted check, stored in Archive struct
- `src/archive.rs`: Added `lock_file: Option<std::fs::File>` field; releases on drop
- `src/creation.rs`: Write-mode constructor sets `lock_file: None`
- Test fixtures: modification tests use temp copies to avoid lock contention between parallel tests

## Consequences
* Good, because concurrent `modify()` on the same file fails fast with a clear error
* Good, because lock releases automatically — no cleanup code, no leaked locks
* Good, because `try_lock_exclusive` is non-blocking — no deadlock risk
* Bad, because advisory locks are cooperative — processes that don't use this library can still modify the file concurrently
* Neutral: `fs2` is a lightweight, stable dependency (~200 lines wrapping OS-level APIs)
