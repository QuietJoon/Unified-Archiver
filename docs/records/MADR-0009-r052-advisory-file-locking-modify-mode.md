---
type: ADR
title: "AD: Advisory File Locking for Modify Mode (R0052-0004)"
description: "Implemented"
tags: [decision, ADR-0009, R0052-0004]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD: Advisory File Locking for Modify Mode (R0052-0004)

## Context and Problem Statement
Found in Review 0052 (Issue R0052-0004, Severity: High).
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

## Amendment (2026-07-17, DCR-007 / R0080-0005, R0080-0041..0046)

The advisory lock binds to the inode, but the original implementation re-resolved
`self.path` by name for every subsequent operation (encryption probe, read backend,
ZIP extras, retained replay, permission preservation, backup, final replace) — so a
non-cooperating process replacing the pathname could have modify operate on, back up,
or atomically clobber an **unrelated** file. That escalation was never inside this
decision's accepted risk (which covers corruption of the archive itself).

`modify()` now captures the locked file's identity (Unix `dev`/`ino` from the lock
descriptor) and revalidates it immediately before each pathname-based use, aborting
with a typed identity-drift error on mismatch. Non-Unix targets skip revalidation
(Windows handle-identity capture is a tracked follow-up). A small check-to-use window
remains and is documented in the `modify()` rustdoc; the cooperative-lock caveat in
Consequences still applies to in-place mutation of the *same* inode. See DCR-007.

## Amendment (2026-07-22, R0081 I6)

The DCR-007 amendment above narrows the check-to-use window with a by-name
revalidation rather than closing it. The stronger fix — capture one
identity-verified descriptor at `modify()` open (the advisory-lock fd already
provides this) and hand *that* descriptor to the read backend so no pathname is
ever re-resolved — was evaluated this pass (`archive_read_open_fd`) and found
unworkable for this crate's libarchive backend:

- libarchive's read handle is iterator-shaped and cannot be rewound, so the
  backend reopens the file once per operation — there is no single fd to hold
  for the handle's lifetime.
- Re-handing one owned fd per operation shares a single file offset across
  every reopen, but the public API hands out `StreamingExtractor` values that
  outlive the call and permits reads in modify mode, so overlapping libarchive
  handles on one offset would corrupt each other (the path-based reopen gives
  each an independent offset today).
- No portable way exists to derive an *independent* open-file description from
  an existing fd: `dup(2)` shares the offset by definition, and on macOS
  `/dev/fd/N` shares it too (verified on the dev host) — only Linux
  `/proc/self/fd/N` gives a fresh description, and macOS + Windows are
  first-class targets.
- The one fd-anchored reopen that keeps independent offsets — open by name,
  then `fstat` the fresh fd against the pinned identity — *is* the DCR-007
  revalidation. Since capture already reads the authoritative advisory-lock
  descriptor (`File::metadata` is an `fstat`), the identity baseline is already
  as strong as an fd hand-off would make it.

The residual check-to-use window is therefore accepted-permanent for this
architecture (it stays within the cooperating-process threat model above), not
a pending fd follow-up. What landed instead: the triplicated `(dev, ino)`
capture (modify / read / UnRAR) was collapsed into one audited primitive,
[`crate::fs_identity::InodeId`]. See the DCR-007 amendment of the same date.

## Amendment (2026-08-05, decision-review-2026-07-19 §B — lock dependency migrated to `fs4`)

The `fs2` crate this record's Option 1 named has been unmaintained since 2019;
the 2026-07-19 design review flagged the dependency (not the decision) as the
wrong primitive to keep. The advisory-locking decision itself is unchanged and
now rides on **`fs4` 1.x**, the maintained continuation of `fs2` with the same
`flock`/`LockFileEx` mechanics:

- `Cargo.toml`: `fs2 = "0.4"` → `fs4 = "1"`.
- `src/modification.rs::Archive::modify`: `fs2::FileExt::try_lock_exclusive`
  → `fs4::FileExt::try_lock` (fs4 1.x renamed the exclusive non-blocking call
  to match `std`'s emerging `File::try_lock` naming). The contract is
  byte-identical: fs4 reports both contention (`TryLockError::WouldBlock`) and
  I/O failure as `Err`, exactly the shape the fs2 call had, so every
  non-acquisition still maps to the same
  `OperationBlocked("Another Modify session holds the advisory lock …")`.
- The lock still releases on descriptor drop; the DCR-007 identity capture
  from the lock descriptor and both amendments above are unaffected.
- New regression guard: `test_second_modify_blocked_by_advisory_lock`
  (`src/modification/tests.rs`) pins the contention contract — second
  `modify()` on the same path is `OperationBlocked` while the first handle
  lives, and reacquires after drop.

`std::fs::File::lock`/`try_lock` (stabilised in Rust 1.89) would remove the
dependency entirely but exceeds the recorded 1.85 MSRV floor; revisit on the
next MSRV bump.
