# Decision records

Single, unified store for all project decision records, consolidated 2026-07-23 from four
previously-separate stores (`docs/architecture/decisions/`, `docs/decisions/`,
`docs/project/design-change-records/`, and the `reviews/` DD-/IG- logs) whose numbering collided.

## Types & prefixes

- **AD-NNNN** — architecture decisions.
- **MADR-NNNN** — review-gate decisions, formerly `docs/decisions/`.
  The `MADR-` prefix resolves the old collision where e.g. `0013` named both an AD and a gate decision.
- **DCR-NNN** — design change records, delta pointers paired with an AD amendment.
- **DD-NNN-NNN** — legacy *accepted* review decisions (24), read-only.
- **IG-NNN-NNN** — legacy *rejected* findings with their rejection reasoning (24), read-only.

## Legacy DD-/IG- series

The DD-/IG- entries came from two prepend-ordered `reviews/` logs (`Decisions.md`,
`Ignores.md`) that were retired as a write-path on 2026-07-23, parked under `legacy/`, and
**split into one record file per entry on 2026-08-04** so every `index.yaml` id addresses a
real file. Bodies were preserved verbatim; only heading depth changed.

Read-only means read-only: do not open new DD-/IG- entries. New review dispositions are
AD-/MADR-/DCR- records. The original inclusion criteria are retained here because they still
describe what the two series contain, and because DD entries take precedence over IG entries
where the two conflict:

- **DD (accepted)** — recorded only for decisions that were high-impact (architectural,
  security, or breaking), non-obvious or disputed, or genuine trade-offs between viable
  alternatives.
- **IG (rejected)** — recorded only for rejections that were high-impact (issues that could
  seem critical but were rejected), non-obvious or disputed, or likely to recur. Rejections
  are generally more valuable to document than acceptances: they prevent re-litigation and
  explain why "obvious" fixes were skipped. Do not treat an IG record as settled forever —
  `IG-020-003` was formally reopened and fixed once its premise (no Linux support) expired.

## How to use

- [`index.yaml`](index.yaml) is the authoritative machine-readable catalogue (id, type, status,
  title, related review/OI ids, cross-record mentions). Reviewer tools should consult it first.
- [`redirects.yaml`](redirects.yaml) maps every old path to its new file for historical references;
  its `legacy_log_split:` section resolves DD-/IG- ids cited against the retired logs.
- Records are **content-immutable**: changes are appended as dated `## Amendment` sections; only
  IDs/paths were changed by the 2026-07-23 migration and the 2026-08-04 DD-/IG- split.
- Resolved open issues are **not** records — they live in
  [`../project/open-issues-resolved.md`](../project/open-issues-resolved.md).

### The two `status` fields

Two `status` fields exist in this store and they answer different questions. Audit tooling that
conflates them reports dozens of false contradictions.

- **`index.yaml` `status:`** is the record's *decision lifecycle*: `active` (the ruling still
  governs), `superseded` (a later record or a dated amendment replaced the ruling), `archived`
  (historical — the ruling is discharged, or was reversed with no single successor record to name;
  kept for provenance and audit). `archived` applies to **any** record type, not only the legacy
  DD-/IG- series that makes up most of its population: AD-/MADR- records whose subject is gone and
  cannot recur as written (e.g. AD-0030) or whose ruling was inverted by policy rather than by a
  successor record (e.g. MADR-0005) are archived too. Prefer `superseded` whenever a successor can
  be named; use `archived` when there is none. This is the field reviewer tooling should trust,
  and the one a supersession must flip.
- **A record's own frontmatter `status:`** is an OKF documentation-trust marker. Where a
  supersession is recorded, three surfaces are kept in step with `index.yaml`: the frontmatter
  `status:` and `description:`, the top-of-body `Status:` line, and the `— _superseded_` suffix
  in [`index.md`](index.md).
- Two divergences between the two fields are **deliberate and expected**:
  - `AD-0003`, `AD-0009`, `AD-0014` and `AD-0040` read `draft` in frontmatter because the OKF
    v0.2 migration resets legacy-provenance documents (`generated.by: unknown/unknown`) to
    `draft` until their content is re-verified. Their decisions remain `active`.
  - The 48 legacy `DD-`/`IG-` files still carry the v0.1 profile (`timestamp:` key,
    `status: active`) inherited from the 2026-08-04 split, while `index.yaml` records 47 of them
    as `archived` and `IG-020-003` as `superseded`. Rewriting those frontmatter blocks belongs
    to the owner-gated OKF v0.2 migration batch, not to record hygiene — the lifecycle answer is
    `index.yaml`'s.

Numbering note: `AD-0068` is the former archived `archive/0051` (renumbered to resolve its collision
with the top-level `AD-0051`). `AD-0023`/`AD-0045` are former archived rejects that filled number gaps.
