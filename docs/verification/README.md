# Verification records

Each file here is one run of `scripts/release-gate.sh`, recorded. Together they
are what this project has instead of a hosted CI service — see **AD-0070**,
which defines "CI-covered" as a property of evidence rather than of a vendor.

## What a record is, and is not

A record is evidence **for the fingerprint it names and nothing else.** It
carries the commit, whether the worktree was dirty (with a digest if it was),
the host, the toolchain, and every lane's own exit status. A green record for
commit `X` says nothing about commit `Y`, and a record taken from a dirty tree
says nothing about any commit at all — it says what that particular pile of
bytes did once.

Absence of a record is an **absence, not a failure, and not a pass.** If no
record exists for a platform, that platform is unverified and the project's
documentation must say so rather than implying coverage. This is deliberate:
the alternative is what the repository had before, where "no CI" was quoted as
a blocker in four places while two records simultaneously claimed the project
had "committed Windows CI".

## Why records are committed rather than generated on demand

The same reason `tests/fixtures/sparse.tar` is committed rather than built by
the test that needs it: a result you can only reproduce by having the right
machine, the right tool and the right afternoon is not a result the next reader
can check. Run it once, commit what happened, let the documentation cite it.

## Reading the "Not run" section

Every record ends with what was **not** run and why, split into two kinds that
must not be confused:

- **Lanes disabled by choice** — the Windows cfg-compile lane is owner-invoked
  only (2026-09-01 hold); the Linux cross-check is parked pre-v2. These could
  run here and deliberately did not.
- **Lanes this host cannot observe** — a Windows test run and a Linux test run.
  No script fixes these; they need a different machine. This is the only
  category where "no CI" was ever a real blocker, and per AD-0070 it applies to
  Windows alone.

## Naming

`YYYY-MM-DD-<short-sha>.md`, plus the platform when it is not the macOS dev
host — for example `2026-09-10-abc1234-windows.md`. One file per run; do not
edit a record after the fact. If a run was wrong, take another and say so in
the new one.
