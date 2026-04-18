# AD: Reject Review 0060 items already tracked by DEF-* / OI-* entries

## Context and Problem Statement
Found in Review 0060 (Issues R0060-0001, R0060-0002, R0060-0119, R0060-0120, R0060-0121, Severity: Medium/Low).

Review 0060 re-raised five issues that are already recorded as deliberately-deferred
gaps in the project's tracking artifacts:

- **R0060-0001 / R0060-0002** — `CompressionOptions::split_size` is a public field
  that no writer consumes, and `Archive::create` silently ignores it. This is
  recorded as **DEF-002** in `docs/project/stub-manifest.md` ("`CompressionOptions::split_size`
  — Field exists but ignored by all writers … At least one writer honors split_size
  [exit criterion] — Open").
- **R0060-0119 / R0060-0120 / R0060-0121** — `sevenz_wrapper.rs`, `zip_wrapper.rs`,
  and `piz_wrapper.rs` all contain `TODO: Implement true streaming` after a
  memory-backed fallback in their `extract_to_stream` paths. This is recorded
  as **DEF-004** in the stub manifest ("All four non-libarchive backends … 
  internally call `extract_to_memory()` and wrap the buffer in `Cursor` … Only
  libarchive actually streams.") and as **OI-0057-007** in `reviews/Open_Issues.md`.

Repeatedly re-accepting tracked deferred gaps would duplicate work across
decision-tracking surfaces and thrash status documents. A single reject record
prevents future reviewers from treating these as newly-discovered issues.

## Decision Drivers
* Tracked gaps already have exit criteria and owners in `stub-manifest.md` and
  `Open_Issues.md`; an additional accept record in this gate would be duplication.
* DEF-002 exit criterion (at least one writer honors `split_size`) is an MVP-out-of-scope
  item and remains deferred.
* DEF-004 / OI-0057-007 are actively scoped for post-MVP work (SevenZ retained-entry
  streaming via `sevenz-rust2`, Piz reader-backed streaming, ZipReader streaming).
* Per gate guidance: "Record only decisions that are … Likely to recur (future
  reviewers may raise the same concern)." These five items are exactly that.

## Considered Options
1. Accept and fix in this gate — rejected: premature; contradicts DEF-002 /
   DEF-004 / OI-0057-007 deferred scoping.
2. Accept and add new OI entries — rejected: duplicates existing tracking.
3. Reject with pointer to existing DEF/OI entries — selected. Future reviewers
   that re-raise these issues can be pointed at this record.

## Decision Outcome
REJECT: We decided for option 3 because these five issues already have
authoritative tracking entries with documented exit criteria. Re-accepting them
in this gate would duplicate work without changing the path to closure.

Status: Implemented (review 0060 decisions applied and archived).

### Implementation
No code change. The underlying gaps remain open under their existing identifiers:

- **DEF-002** in `docs/project/stub-manifest.md` (row 12) — covers R0060-0001,
  R0060-0002.
- **DEF-004** in `docs/project/stub-manifest.md` (row 14) — covers R0060-0119
  through R0060-0121.
- **OI-0057-007** in `reviews/Open_Issues.md` — covers the ZIP/SevenZ/libarchive
  unknown-size streaming side of DEF-004.

Future reviewers noticing `split_size` or the streaming TODOs should consult
these entries before raising new issues.

## Consequences
* Good, because tracking remains centralized in the stub manifest and Open_Issues
  ledger rather than fragmenting across decision records.
* Good, because gate throughput is preserved — 5 auto-reject items require no
  code change.
* Bad, because the gap continues to be visible in the public API surface
  (`split_size` field) and in rustdoc TODOs. Mitigated by R0060-0003's rustdoc
  fix, which documents the non-libarchive streaming fallback at the `extract_to_stream`
  entry point.
