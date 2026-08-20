---
type: Explanation
title: How this project records decisions
description: The append-only decision store under docs/records, the review gates that feed it, the open-issues ledger, and how code points back at the ruling that authorised it.
tags: [decision, project-control]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: records-readme, resource: docs/records/README.md }
  - { id: records-index-yaml, resource: docs/records/index.yaml }
  - { id: records-index-md, resource: docs/records/index.md }
  - { id: amendment-example, resource: docs/records/MADR-0009-r052-advisory-file-locking-modify-mode.md }
  - { id: decision-review, resource: docs/architecture/decision-review-2026-07-19.md }
  - { id: review-0081, resource: docs/log.md }
  - { id: open-issues, resource: docs/project/open-issues.md }
  - { id: open-issues-resolved, resource: docs/project/open-issues-resolved.md }
  - { id: contributing, resource: CONTRIBUTING.md }
synced_hash: 2c8c06b240730bb463fa957928fab865d953d072937beb5d76f204bccb35f34d
---

# How this project records decisions

If you are about to change behaviour here, the first useful question is not "what does the code
do" but "what was decided, when, and on what premise". This project answers that from a single
store of numbered, append-only records, fed by review gates and drained by a ledger of accepted
but unfinished work. This page explains how those three pieces relate and why they are shaped
that way. The mechanics of the store itself are stated in `docs/records/README.md`, and the
day-to-day contribution workflow — branching, the checks to run, the pull-request process — is
in `CONTRIBUTING.md`; neither is restated here.

## One store, five prefixes

Everything lives in `docs/records/`, one file per record, with the prefix carrying the kind:

- **AD** — architecture decisions. The bulk of the store, in the MADR shape: context and problem
  statement, decision drivers, considered alternatives, decision outcome, consequences split into
  good and bad.
- **MADR** — decisions taken at a review gate rather than during design. These were previously a
  separate store, `docs/decisions/`, numbered from 0001 independently of the ADs.
- **DCR** — design change records. A DCR is a delta pointer, not a fresh ruling: it names its
  date and source, the records it affects, what changed, why, which areas of the tree it touched,
  and what callers must migrate. Each one is paired with an amendment on the AD it revises.
- **DD** — legacy *accepted* review dispositions, read-only.
- **IG** — legacy *rejected* findings, kept together with the reasoning for the rejection,
  read-only.

Read-only means read-only: no new DD or IG entries are opened. New dispositions are AD, MADR, or
DCR records. The original inclusion criteria for the two legacy series are still documented,
because they describe what those files contain and because DD takes precedence over IG where the
two conflict. The store's README makes a point worth internalising about IG entries: a recorded
rejection is not settled forever. `IG-020-003` was reopened and fixed once its premise — that
Linux was not a supported platform — expired.

That five-prefix arrangement is itself the outcome of a review. Until mid-2026 there were four
overlapping stores with colliding numbering: architecture decisions under
`docs/architecture/decisions/`, thirty review-gate decisions under `docs/decisions/` whose
numbers ran over the AD numbers, the legacy `reviews/` logs still receiving new entries, and a
design-change-record directory of its own. The number "0013" named two different decisions
depending on which directory you were in. That ambiguity had a concrete cost — a reviewer cited
"None" for a decision that existed, in a store they had not looked in — so the 2026-07-19
decision review put the record system at the top of its own revert list, and the consolidation
landed on 2026-07-23. The old directories are now stubs pointing at `docs/records/`, and
`redirects.yaml` maps every historical path to its new file, including a section that resolves
DD and IG identifiers cited against the retired logs.

## Two indexes, and the two `status` fields

`docs/records/index.yaml` is the authoritative machine-readable catalogue: one entry per record
with its id, type, title, lifecycle status and filename, plus optional lists of the review
findings it answers, the open issues it relates to, and the other records it mentions. Tooling
should read that file first. `docs/records/index.md` is a generated human view of the same data,
grouped by prefix, with superseded records marked inline.

The store carries two different fields both called `status`, and conflating them manufactures
contradictions. `index.yaml`'s `status` is the *decision lifecycle*: `active` means the ruling
still governs, `superseded` means a later record or a dated amendment replaced it, `archived`
means a historical disposition kept for provenance — which is what the whole DD and IG series
is. A record's own frontmatter `status` is a documentation-trust marker from the project's
knowledge-format profile, and answers a different question about the document rather than the
decision. Where the two diverge, the README says which divergences are deliberate: several ADs
read `draft` in frontmatter because a format migration reset legacy-provenance documents until
their content is re-verified, while their decisions remain active. When you need the lifecycle
answer, take it from `index.yaml`.

## Records are append-only, and that is the point

A record is content-immutable. When reality changes, a dated `## Amendment` section is appended;
the decision outcome above it is left exactly as written. This is not archival fussiness. The
value of a decision record is that it captures *the premise the decision rested on*, and a
rewritten ruling destroys the only evidence of what was believed at the time — which is the one
thing you need in order to judge whether the premise still holds.

`MADR-0009` is a good specimen. Its decision outcome accepts advisory file locking for modify
mode via the `fs2` crate, with the cooperative nature of advisory locks written down as an
accepted cost. Three amendments then sit under it, in date order. The first records that the lock
binds an inode while every later step re-resolved the path by name, so a non-cooperating writer
could get modify to operate on an unrelated file — an escalation the original decision never
covered — and that identity revalidation was added. The second evaluates the stronger fix
(hand one identity-verified descriptor to the backend), explains concretely why it is unworkable
for an iterator-shaped libarchive handle across all three target platforms, and accepts the
residual window as permanent for this architecture rather than leaving it as a pending
follow-up. The third notes that `fs2` has been unmaintained since 2019 and migrates the same
decision onto `fs4`, spelling out that the contract is byte-identical, and observing that the
standard-library file locks stabilised in Rust 1.89 would remove the dependency but exceed the
recorded MSRV floor.

Read top to bottom, that is the whole story: what was decided, what went wrong with it, what
stronger fix was considered and rejected with reasons, and which dependency the decision now
rides on. None of it would survive an edit-in-place policy.

Supersession works the same way. `AD-0007` still contains its original dual-ZIP-backend ruling
verbatim; below it, one amendment records the benchmark that settled the question and one
records the owner-approved collapse, with the paired `DCR-009` carrying the delta. The record's
lifecycle flipped to `superseded` in `index.yaml`, its frontmatter and its top-of-body status
line were brought into step, and the body was not touched.

## Review gates and stable finding IDs

Two kinds of gate feed the store.

**Code and architecture reviews** are numbered and archived under `reviews/reviewed/`, one file
per review, some with a proposed patch alongside. A review states its scope and method, then
enumerates findings as `R<review>-<sequence>` — for example `R0081-0005` — each with a severity,
a location, the problem, and a recommendation. Reviews are explicitly scoped to *exclude*
findings already accepted, rejected, deferred or recorded by earlier reviews and by the current
record corpus, which is the mechanism that stops the same argument being relitigated every pass.
The identifiers are stable, so a later record can name exactly which finding it answers, and
`index.yaml` can carry that mapping.

That stability is what lets the archive itself be retired. On 2026-08-07 the nineteen reviews then
in `reviews/reviewed/` (0056–0081) were triaged finding by finding and moved to a cold store
outside the repository: of 1,218 findings that no record or issue cited, all but nine had been
fixed, made moot, or absorbed by a broader ruling, and those nine were routed to new open issues
before the move. The finding IDs stay meaningful in the records that cite them even though the
review files no longer sit in the tree — `docs/log.md` carries the triage account.

**Decision-record reviews** audit the store itself rather than the code. The 2026-07-19 pass is
the model: it grouped its output into revert candidates (decisions to reverse or formally
supersede, numbered `R1` upward), record-integrity improvements, innovations (`I1` upward — the
decision is sound but a better design is now available), and an explicit list of records to leave
untouched. Beware the namespace collision when reading amendments: the "R4" in `AD-0007`'s
amendment headings is revert-candidate 4 of that review, not a finding `R0004-…`, and the
"I6" in `MADR-0009`'s second amendment is innovation 6 of the same document.

Every finding then goes one of three ways. It is **accepted** — landing as an inline fix, or as
a new AD, MADR or DCR when a design position changed. It is **rejected** — and a rejection gets
its own numbered record rather than a shrug, which is why the store contains a visible run of
records whose titles begin "reject": duplicate reviews, premises found stale, sweeps judged not
worth the churn. Or it is **routed** into the open-issues ledger, because it is real, accepted in
principle, and larger than the pass that found it.

## The open-issues ledger, and what it is not

`docs/project/open-issues.md` holds `OI-<origin>-<sequence>` entries — accepted work that has
not landed. An entry names its source (a review finding or an AD), its date, the disposition, and
its status; then it states the problem, the impact, a numbered list of required actions, and a
checkbox verification list that defines done. Some entries also carry dated update notes when
part of the work lands.

The status vocabulary is wider than open-or-closed, deliberately: entries read `OPEN`,
`RESOLVED` with a date and a resolution note, or a partial state such as `PARTIALLY LANDED
(8/9 sub-items)` or `PARTIALLY RESOLVED`, where a narrowed remainder is spelled out. The
resolution notes are substantial — they name the functions that changed, the tests added, and
what residual was accepted — so a resolved entry is frequently the best available explanation of
why a piece of code is shaped the way it is.

Two boundaries matter. First, resolved entries are project tracking, not decisions:
`docs/project/open-issues-resolved.md` holds an older batch of them as an audit trail that takes
no new entries, and the store README states plainly that resolved open issues are not records. If
you want the ruling, look in `docs/records/`; if you want the work history, look in the ledger.
Note that entries are no longer relocated on resolution — the active file retains them, so many of
its entries carry a status line reading `RESOLVED`. Trust the status line rather than which file
an entry sits in. Second, straightforward
test-hygiene fixes are removed outright without an archive entry — the ledger is not a changelog.

## How this differs from the TicGit backlog

The ledger and the ticket backlog answer different questions and are not interchangeable.

An OI entry is a durable design obligation: one problem, its accepted disposition, and the
verification that closes it. It outlives whoever is working this week, and it is the right place
for something that couples to a future decision or spans several passes.

TicGit — the `ti` command, with the `ti-pick-next`, `ti-new` and `ti-finish` workflow described
in `AGENTS.md` and `CLAUDE.md` — is the day-to-day claim mechanism. Its purpose is coordination
between parallel workers: claim before starting so two agents do not grab the same task, file a
ticket for follow-ups you discover, verify and close when done. Tickets are short-lived and
identified by a hash, and they show up in the ledger's resolution notes and in code comments as
the trace of who did the work (`ti-2a6e3153` appears across `src/backend.rs`,
`src/inspection.rs` and `src/extraction.rs` for one such change).

The practical rule: a ticket is how work gets claimed and finished; an OI entry is why the work
exists and how anyone will know it is complete. Filing a ticket does not discharge an OI entry,
and closing an OI entry without its verification list satisfied is the failure mode both
mechanisms exist to prevent.

## How code points back at the ruling

Traceability here runs through identifiers in comments and rustdoc rather than through a
separate mapping file, which means `grep` on an id finds both the ruling and every site that
implements it.

The forms you will see are the record id (`AD 0007 (2026-07-23 collapse) / DCR-009` on the ZIP
routing arm in `src/archive.rs`), the finding id (`R0080-0041` at the identity-revalidation
sites in `src/modification.rs`), the open-issue id (`OI-0080-003` on the budgeted listing paths
in `src/backend.rs`), and the ticket hash. Public rustdoc carries the same references, so the
generated documentation tells a consumer which decision governs the behaviour they are reading
about — the `Archive` type documentation names the snapshot-semantics record directly.

Two cautions when you follow such a reference. Some in-code citations predate the 2026-07-23
renumbering and use the `AD` prefix for decisions whose authoritative record is now a `MADR`, so
if the record you land on has nothing to do with the code that cited it, check for a `MADR` of
the same number before concluding the comment is nonsense. And when you add a citation, prefer
the id as `index.yaml` spells it — that file is what makes an id resolvable.

For a worked example of a design whose full rationale lives in records rather than in the code,
read [Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md).
To get a tree you can actually run these searches against, start from
[Building and testing unified-archive from source](../../../tutorials/developer/en/build-and-test-from-source.md).
