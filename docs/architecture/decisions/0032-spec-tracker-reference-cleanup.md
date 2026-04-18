# AD 0032: Spec tracker reference and implementation detail cleanup

## Context and Problem Statement
Found in Review 0006 (Issues R0006-0001, R0006-0026 through R0006-0051, Severity: Critical/Medium).

The specification (`spec.md`) accumulated 36+ inline tracker references (OI-xxx, AD-xxx, R0xxx-xxx, T-xxx) and implementation details (crate names, internal struct/method names, buffer constants) throughout its requirement text, user stories, acceptance scenarios, and edge cases over six review cycles. The spec read as a hybrid requirements-plus-changelog document rather than a stable product-level specification.

## Decision Drivers

* The spec should express *what* the product does, not *how* it was implemented or *when* issues were resolved
* Tracker references in normative text create temporal instability — the text ages as issues close
* Implementation details (crate names, struct names, buffer sizes) in FR text couple requirements to current backend choices
* Provenance traceability is valuable but belongs in a separate section, not inline

## Considered Options

1. Full spec split — separate product requirements document from implementation-facing architecture layer
2. In-place cleanup — remove inline tracker references and implementation details, add provenance appendix
3. Leave as-is — the spec serves developers and the hybrid format works for the current audience

## Decision Outcome

ACCEPT (option 2): In-place cleanup. Inline tracker references removed from all normative text (user stories, scenarios, edge cases, FRs, SCs). Implementation details (crate names, struct names, buffer constants) stripped from FR text and replaced with behavioral language. A Provenance appendix at the end of spec.md preserves all original traceability links.

Option 1 (full split) is tracked as a potential future improvement but was rejected for this pass due to scope and the risk of maintaining two parallel documents.

Status: Implemented.

### Implementation

- 36+ tracker references removed from normative text across 15 FRs, 15 edge cases, 4 user stories, and acceptance scenarios
- Implementation details (Piz, ZipReader, SevenZ, UnRAR, goblin, UNRAR_LOCK, etc.) replaced with behavioral descriptions
- FR-025: "within the first 1MB" changed to "within a configurable initial region"
- Status annotations ([RESOLVED], [PARTIAL]) removed and rewritten as plain behavioral text
- SfxDetectionResult description reordered to lead with current behavior
- Technology stack secstr entry simplified
- Provenance appendix added preserving all original traceability

Files modified: `specs/001-unified-archive/spec.md`

## Consequences

- Good: spec reads as a product requirements document, not a developer changelog
- Good: FRs are implementation-agnostic and less likely to drift with backend changes
- Good: provenance preserved in appendix for audit trail
- Neutral: future reviews adding implementation annotations will need to follow the new convention
- Bad: inline provenance is slightly less convenient to trace (requires checking the appendix)

## References

- R0006-0001 (Critical: spec implementation-contaminated)
- R0006-0026 through R0006-0051 (26 Medium issues: individual tracker/detail leakage)
