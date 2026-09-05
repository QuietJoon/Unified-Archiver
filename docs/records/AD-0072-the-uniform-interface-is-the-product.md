---
type: ADR
title: "AD 0072: The uniform interface is the product; the crate absorbs dependency gaps"
description: "Accepted (2026-09-05, owner ruling) — this library exists to provide one common interface across every archive format. One dependency per format is preferred, not required: when a dependency cannot provide every feature, or performance differs greatly, the crate uses several and pays the code cost itself. This governs every design decision, and it inverts several reasoning shapes previous records were built on."
tags: [decision, ADR-0072, project-control, architecture, dependencies, core-purpose]
status: active
---

# AD 0072: The uniform interface is the product; the crate absorbs dependency gaps

## Status

Accepted (2026-09-05) — owner ruling, in the owner's own words:

> Unified-Archiver는 모든 archive format에 대해서 공통된 interface를 제공하는 것입니다.
> 가능하면 한개의 archive format에 단일 dependency/library를 사용하면 좋지만 성능 차이가
> 매우 크거나, 각각의 dependency/library가 모든 기능을 제공하지 못할 때, Unified-Archiver가
> 각각의 dependency/library를 사용해서, Unified-Archiver가 code cost를 대신 지불해서
> 공통의 interface를 제공하는 wrapper가 되어야 합니다.

Re-stated the same day, after an agent narrowed it to a criterion for one open
item: **핵심 목적은 이 라이브러리 그 자체의 목적입니다** — it is the purpose of the
library itself, and governs every design decision, not any particular one.

## Why this record has to exist

The ruling had been stated twice and written down nowhere. That is not a
bookkeeping problem; it is why the same wrong answer kept getting produced.
Without it recorded, every reader — human or agent — falls back on defaults that
sound like good engineering and are wrong here: fewer dependencies is safer, an
asymmetric backend split is untidy, a migration that buys no immediate
user-visible feature is not worth its risk. Each of those has already decided at
least one question in this repository, and each optimises something this project
did not set out to optimise.

## The decision

**The product is the uniform interface.** A caller writes one piece of code and
it works across ZIP, 7z, RAR, the TAR family, ISO and the standalone compressed
streams. Everything else in the crate is in service of that.

**One dependency per format is preferred, not required.** Where a single library
covers a format completely and performs well, use it and stop.

**Where it does not, use more than one and pay the cost here.** The two triggers
are explicit: a dependency that cannot provide every feature, and a performance
difference that is large. In either case the crate takes on the code, the
testing and the maintenance of a second backend so that the *interface* stays
uniform. Being the wrapper that absorbs that cost is the job; it is not evidence
of a design that failed to converge.

**The size that follows is real, and the answer to it is the compile-time
split, not a narrower interface.** Absorbing per-format cost makes the crate
large. That is why smaller compile paths — read-only above all — exist as a
deliberate counterweight (AD-0058). A consumer who wants only ZIP reading should
be able to compile only that. Shrinking the *interface* to keep the crate small
is the wrong trade and is ruled out here.

## What this inverts

These reasoning shapes are no longer sufficient justification anywhere in this
repository. Where one of them is load-bearing in an existing record, that record
is open to re-examination:

- *"Fewer dependencies is simpler / safer."* Dependency count is not a goal.
  Risk management can still justify a temporary reduction — that is what DCR-009
  did for ZIP — but a reduction taken for risk does not become an architectural
  ruling by sitting still.
- *"The split is ugly rather than harmful."* Internal symmetry is not a goal.
- *"Migrating buys no user-visible gain on its own."* Closing a capability gap so
  that one format stops behaving differently from the others *is* the gain.
- *"Record the gap as an accepted limitation / standing caveat."* A capability a
  backend cannot provide is not a limitation to document. It is the precise case
  this library was built to absorb, and documenting it instead is choosing a
  non-uniform interface.
- *"Disproportionate for the value at this stage."* Still admissible as a
  scheduling argument. Not admissible as a permanent answer.

None of this makes a second backend automatic. It changes the default: the
question stops being "is a second dependency justified" and becomes "is there a
reason this gap must stay in the interface".

## Consequences

- Capability gaps between formats become defects rather than documented
  behaviour, and are tracked as work.
- Records resting on the shapes above need re-reading. **AD-0071** (libarchive
  owns ZIP modification permanently, with DEF-005's ZIP64, encrypted-ZIP and
  journaling caveats recorded as standing) is the clearest candidate: it is
  argued on "ugly rather than harmful" and "no user-visible gain on its own",
  and what it defers is a set of capability gaps. It is flagged here rather than
  overturned — that is the owner's call, on evidence, not a consequence this
  record may take on its own.
- AD-0058's feature-first split gains its real motivation. It is not an
  optimisation; it is the counterweight that makes absorbing per-format cost
  affordable for consumers.
- The cost is honest: more code, more dependencies, more testing surface, and a
  larger default build. The split is how that is paid for, so the split matters
  more than it appeared to.

## Related

AD-0058 (feature-first footprint split — the counterweight), AD-0071 (ZIP
modification ownership — flagged for re-examination), DCR-009 (the ZIP backend
reduction, taken as risk management), DEF-005 (the ZIP-modify caveats), README's
"Why this library exists" section, which states the same thing for readers who
never open `docs/records/`.

## Amendment (2026-09-05, "uniform interface" does not mean "uniform capability")

The body above overstates the ruling in one sentence, and the overstatement is
the kind that produces wrong work. It says a capability gap between formats

> is not a limitation to document. It is the precise case this library was built
> to absorb, and documenting it instead is choosing a non-uniform interface.

The owner corrected that the same day, and the correction applies to every
feature rather than to the item that prompted it:

> 모든 기능에 대해서 동일한 인터페이스는 제공하되 할 수 없는 것은 할 수 없다고
> 문서화합니다.

**Provide the same interface for every feature — and document what cannot be
done as something that cannot be done.**

### What "uniform interface" actually means

It is a statement about **shape and honesty**, not about coverage:

- **Same API.** One call spelling for an operation across every format. A caller
  does not learn a different method, a different options type, or a different
  error taxonomy per format.
- **Same way of asking.** Capability is discoverable through one mechanism —
  `FormatCapabilities`, `can_create()`, `Support::*` — so a caller finds out what
  a format can do by asking the same question everywhere.
- **Same way of refusing.** An unsupported operation fails as a typed,
  facade-level refusal naming the limitation, not as a backend-specific error
  leaking through, and not as a silent degradation.

It is **not** a promise that every format supports every operation. Some cannot,
some will not, and saying so plainly is part of the uniform interface rather
than a departure from it. A documented "no" that every format delivers the same
way is uniform. A "no" you discover by reading backend source is not.

### So when does the absorb-the-cost rule fire?

The body's rule is unchanged where it applies: when a dependency cannot provide
a feature **that this project has decided to offer**, the crate may use a second
dependency and pay the cost rather than let one format behave differently.

What the body got wrong is treating the existence of a gap as automatically
settling that the feature is offered. Scope comes first, and scope is the
owner's call:

1. **Is this capability in scope for the crate at all?** If the answer is no, it
   is documented as unsupported — honestly, discoverably, and identically for
   every format in that position. That ends it, and no second dependency
   argument applies.
2. **If it is in scope, does one format behave differently from the others?**
   That is the gap this record is about, and there the absorb-the-cost rule
   governs: fewer dependencies, internal symmetry, and "no user-visible gain on
   its own" are not sufficient reasons to leave it.

The inverted reasoning shapes listed in the body remain inverted — but they are
answers to question 2, not to question 1. Declining a capability on scope
grounds was never one of them.

### Consequences of this amendment

- **AD-0018 stands.** Standalone `.gz` / `.bz2` / `.xz` / `.zst` / `.lz4` /
  `.lzma` *creation* is out of scope by owner ruling, documented as unsupported,
  and is not work. That libarchive could technically produce those streams does
  not make it a gap to absorb, because the capability is not offered. The
  crate's reporting already meets the honesty half: `compression_write` is
  `Support::None` for those variants, `can_create()` excludes them, and the
  refusal is a facade-level `OperationBlocked`.
- The re-examination finding that flagged AD-0018 as a gap to absorb is
  **withdrawn**, and it was wrong for exactly the reason this amendment names —
  it inferred scope from capability.
- **AD-0019 is unaffected by this amendment.** Its finding was about a large
  per-format *performance* difference inside a capability the crate does offer
  (RAR reading), which is question 2, and the disproportionality reasoning there
  is still inverted.
