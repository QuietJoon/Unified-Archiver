# AD 0022: Reject documentation restructuring proposed by Reviews 042 and 043

## Context and Problem Statement

Found in Review 042 (Issues R042-061, R042-070, R042-089, Severity: High/Medium).

Reviews 042 and 043 together flagged 254 potential documentation drift issues
across `docs/`, `specs/`, `README.md`, `CHANGELOG.md`, `Cargo.toml`, and inline
rustdoc. The overwhelming majority (239) were straightforward drift corrections
that were applied directly. This record captures the three REJECT decisions
from Review 042 so a future reviewer encountering the same documents does not
repeat the same suggestions.

The three rejected issues asked for:

1. **R042-061** — Rewrite the modification walkthrough to highlight the
   `modify_with_options()` backup path alongside the default `modify()` flow.
2. **R042-070** — Create a new dedicated SFX contract document under
   `specs/001-unified-archive/contracts/` separate from the existing SFX
   section in `contracts/archive.md`.
3. **R042-089** — Move shell workaround notes (`cat` / `copy /b` used to
   reassemble split-ZIPs) out of the extraction contract and into a new
   troubleshooting document.

## Decision Drivers

- Minimize documentation sprawl: each new surface adds a future drift risk.
- Keep walkthroughs focused on the happy path; surface variants where the
  reader can already see them (source rustdoc, contracts).
- Prefer colocated workaround notes over a separate troubleshooting silo that
  nothing else cross-links.

## Considered Options

1. Apply the review recommendations as-is (expand walkthrough, create new SFX
   contract doc, move shell workarounds).
2. Reject the restructuring and keep the existing colocated structure.
3. Partial acceptance — e.g., accept R042-061 only.

## Decision Outcome

REJECT all three. We chose option 2 because:

- **R042-061**: The walkthrough already accurately describes the default
  `modify()` flow ("Atomic rename: replaces original with new archive; no
  backup is created"). Backup behavior via `modify_with_options()` is already
  documented in the contract, `ModificationOptions` rustdoc, and AD 0020.
  Restating it inside a happy-path walkthrough would dilute the walkthrough's
  purpose without adding information that isn't one click away.
- **R042-070**: The SFX section in `specs/001-unified-archive/contracts/archive.md`
  already lists the full SFX API surface, references the in-source modules,
  and links to AD 0015. A dedicated file would need to duplicate that content
  or become a stub that points back — either option increases drift surface
  without helping readers.
- **R042-089**: The shell workarounds live next to the `extract_from_parts()`
  description that motivates them. Readers who need the fallback are already
  in the extraction contract. A separate troubleshooting page would either
  duplicate the context or force a cross-reference with no net gain.

Status: Implemented (nothing to change in the codebase; this record
documents the deliberate no-op).

### Implementation

No code or doc changes beyond writing this record. The reviewed documents are
left intact:

- `docs/architecture/walkthroughs.md` (modification section unchanged)
- `specs/001-unified-archive/contracts/archive.md` (SFX section unchanged)
- `specs/001-unified-archive/contracts/extraction.md` (shell workaround
  section unchanged)

## Consequences

- Good: no new doc files to maintain; existing cross-linking remains
  stable; future reviewers have an explicit reference when the same
  suggestions reappear.
- Bad: readers who prefer one-topic-per-file navigation may occasionally
  need to search within a larger document.

## References

- Review 042: `reviews/reviewed/042.md`
- Related decisions: AD 0015 (SFX detection pipeline), AD 0020 (additive
  modification options extension)
