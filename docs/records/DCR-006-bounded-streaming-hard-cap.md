---
type: DCR
title: "Bounded streaming errors on over-production instead of silently truncating"
description: "Public extract_to_stream[_with_options] now return a hard-capped StreamingExtractor; io::Take silent truncation retired."
tags: [change, project-control, DCR-006]
status: active
---

# DCR-006: Bounded streaming errors on over-production instead of silently truncating

- **Date:** 2026-07-17
- **Source:** Review 0080, Issue R0080-0007
- **Affected ADRs:** AD-0062-r0069-group-a-v0.3-api-shaping.md (updated — A.2 amendment)

## What Changed

AD 0062 §A.2 chose `io::Take` for the bounded public streaming methods, which capped memory
but made an over-producing decoder indistinguishable from clean EOF (silent truncation at the
declared size). `Archive::extract_to_stream` and `extract_to_stream_with_options` (and the
v2-api `ReadArchive` wrappers) now return `StreamingExtractor` wrapped by the existing
hard-cap machinery (`with_hard_cap`): reading past the declared size yields a structured
`io::ErrorKind::InvalidData` error instead of `Ok(0)`. The return type changed from
`io::Take<StreamingExtractor>` to `StreamingExtractor`. `extract_to_stream_unbounded` is
unchanged.

## Why

Silent truncation converted a hostile-archive signal into apparent success; the hard-cap
probing reader (already used by the trait-default `_with_limit` path and manifest digests)
distinguishes the two. Full rationale in the AD 0062 amendment.

## Affected Areas

- src/extraction.rs (public bounded methods)
- src/streaming.rs (contract docs)
- src/archive/mode_split.rs (`ReadArchive` wrappers)
- docs/API_REFERENCE.md, examples/streaming_extract.rs (docs)

## Migration / Follow-up

Callers that matched on the `io::Take` type or relied on silent truncation must handle the
`InvalidData` over-production error. No other call-site changes: `StreamingExtractor`
implements `Read` the same way.

## Amendment (2026-07-22, R0081-0027 / I2)

The hard cap this DCR introduced now lives behind a single typed bound rather than a
default-vs-`_unbounded` method split. The public `Archive::extract_to_stream` /
`extract_to_stream_with_options` (and the v2-api `ReadArchive` wrappers) gained a
`bound: StreamBound` parameter — `enum StreamBound { DeclaredSize, Cap(u64), Unbounded }`,
declared in `src/streaming.rs` — and the redundant `extract_to_stream_unbounded` /
`extract_to_stream_with_options_unbounded` methods were deleted (I2).

The over-production ⇒ `io::ErrorKind::InvalidData` contract this DCR established is unchanged.
It is applied by exactly one `with_hard_cap` site inside the shared
`Archive::extract_to_stream_impl`, now reached by both `StreamBound::DeclaredSize` (the former
default bounded behaviour) and `StreamBound::Cap(n)` (a caller-chosen ceiling with the same
over-production error); `StreamBound::Unbounded` is the opt-in uncapped reader with its
documented trust caveat. Because "unbounded" is one value chosen in one place instead of a
parallel method, R0081-0027 (the two former unbounded methods disagreed on mmap-cap handling)
is structurally retired. Full rationale in AD 0062's 2026-07-22 amendment.

## Amendment (2026-08-09, Review 0001 R0001-0007/R0001-0008 — where the cap binds, and what "declared" means)

Two corrections to the mechanism this record describes, neither of which changes its decision.

**1. The output cap no longer stands alone.** `extract_to_stream_impl` passed `max_bytes = None` to
the backend and applied the bound only to the returned reader. Because ZIP, 7z and RAR materialise
an entry before exposing a reader (AD 0035 / DEF-004), `StreamBound::Cap(n)` bounded what the caller
could *read* while the backend had already staged or buffered the whole entry — protective in
appearance, not in resource use. The caller's `ExtractionLimits::max_file_size` is now threaded into
`extract_to_stream_with_limit`, so a staging backend fails before materialising past that ceiling.

Consequence worth stating plainly: **`StreamBound::Unbounded` no longer means "no cap anywhere."**
The backend read is bounded by `limits.max_file_size` regardless of the `StreamBound` value; only
the *reader* is uncapped. This bites exactly one case — an entry whose declared size is unknown
(raw gzip/bzip2/xz single-file readers) and whose decoded output exceeds the limit. Callers who
genuinely want no ceiling must raise `max_file_size`, not choose `Unbounded`.

The wider change — deriving the backend cap from `StreamBound` itself, so `Cap(n)` binds
pre-materialisation — was considered and **deliberately not made**; it stays open (the finding's
full scope, and TicGit `6277386e`).

**2. `DeclaredSize` now uses the listing's size, not the buffer's.** `StreamingExtractor::from_bytes`
sets `total_size` from the materialised `Vec<u8>`, so on every native backend `DeclaredSize` was
capping the decoder's output at the decoder's own output — an over-producing decoder defined its own
declaration and the cap could never fire. The declared size is now taken from the authoritative
preflight listing (falling back to `limits.max_file_size` when the listing size is `None`).

**Still unsettled, and named here so it is not mistaken for settled:** this record governs
over-production and says nothing about **under-production**. `HardCapReader` returns an inner `Ok(0)`
before the cap unchanged, so a truncated checksum-less entry still reads as a short but valid
payload. Tracked as OI-0001-001; correction 2 above is its prerequisite, since an exactness check
against a self-derived declaration would have been vacuous.

## Amendment (2026-08-12, R0001-0011 — `DeclaredSize` becomes exact-length, and the backend cap is derived from the bound)

This amendment closes **both** halves the 2026-08-09 amendment left open. Neither adds a
`StreamBound` variant: the enum still has exactly `DeclaredSize | Cap(u64) | Unbounded`, and both
changes are behaviour changes to existing variants (a 0.x minor, not a type-level break).

**1. Under-production is no longer a silent short read (OI-0001-001).** `HardCapReader` gained an
`expected: Option<u64>` field and `StreamingExtractor::with_exact_size`, reached by exactly one call
site — the trailing `match bound` in `Archive::extract_to_stream_impl` — so the I2
"bound interpreted in one place" property is preserved. Under `StreamBound::DeclaredSize` with a
declared size from the preflight listing (the R0001-0008 prerequisite, already landed):

- over-production past the declaration stays `io::ErrorKind::InvalidData` (this record's original
  contract, unchanged);
- an inner clean end-of-stream below the declaration is now
  `io::ErrorKind::UnexpectedEof` — std's `read_exact` truncation kind, chosen so callers can tell a
  truncated source from a hostile over-producing one with a stable `match e.kind()`;
- the truncation verdict is **sticky**: a retrying caller gets the same error, never a clean `Ok(0)`;
- a zero-length caller buffer returns `Ok(0)` without being read as an end of stream.

`Cap(n)` keeps ceiling-only reader semantics — a caller-chosen cap is a limit, not an assertion, so
an entry smaller than `n` still reaches a normal EOF. Entries that declare no size (raw
gzip/bzip2/xz single-file readers; libarchive entries whose `archive_entry_size_is_set` is 0) have no
declaration to hold a stream to, so `DeclaredSize` degrades there to a ceiling-only cap at the
effective entry limit and an early EOF stays an ordinary EOF; no declaration is invented.

`with_exact_size` also normalises the returned extractor's `total_size` to the listing value, so the
cap, the exactness check and the `progress()` denominator share one authority. On the staging
backends this replaces the materialised buffer length that `StreamingExtractor::from_bytes` reports.

**2. The backend materialisation cap is now derived from the bound (the deliberately-deferred half).**
`extract_to_stream_impl` passed `limits.max_file_size` to `extract_to_stream_with_limit`; it now
passes `stream_backend_cap(bound, declared_size, limits)` =
`min(bound tightening, max_file_size, max_total_size)`, where the tightening is the declared size for
`DeclaredSize`, `n` for `Cap(n)`, and nothing for `Unbounded`. The caller's limits stay a hard
ceiling a `StreamBound` may tighten but never loosen. Two consequences:

- `Cap(1024)` on a multi-GiB entry no longer lets a staging backend materialise the whole entry
  first. ZIP and 7z gained `extract_to_stream_with_limit` overrides routing through their capped
  memory paths (`read_entry_to_memory_capped` rejects `declared > cap` before reserving), joining
  RAR's existing override; libarchive keeps the trait default, which is correct because its reader is
  genuinely incremental, so the reader-level cap *is* its materialisation bound. Practically this
  means a `Cap(n)` below the declared size fails at the extract call with
  `ArchiveError::OperationBlocked` on ZIP and 7z, while libarchive still serves a prefix up to `n`.
  The portable way to read a window of a larger entry is `DeclaredSize` + `Read::take`.
  **RAR is not uniform with ZIP/7z here, and the distinction is worth stating precisely:** UnRAR's
  override aborts when *decoded* bytes cross the budget inside `UCM_PROCESSDATA`, not when the
  *declared* size exceeds it. So a RAR entry whose header over-declares but decodes within `n` still
  succeeds, and one that does not still pays the decode work up to `n` before aborting. That is
  arguably closer to ceiling-only in spirit than the ZIP/7z pre-check; it is recorded here so a
  reader does not infer a uniform declared-size trigger across all three staging backends.
- The stream path stops ignoring a tighter `max_total_size`. It previously handed the backend
  `max_file_size` alone, so a caller whose total budget was below the per-file budget got no runtime
  protection on unknown-size entries — the same gap `effective_entry_cap` closed for the memory paths
  (R0001-0006 / R0001-0010). `Unbounded` is bounded by that effective ceiling, reinforcing the
  2026-08-09 amendment's point that "unbounded" never meant "no cap anywhere".

**Scope not covered.** AD 0035's subject is untouched: ZIP/7z/RAR still buffer or stage the entry
when it *fits* the budget, and a 7z solid block still decodes preceding members to reach the target.
What changed is that the budget is the caller's bound rather than `max_file_size`. True incremental
readers for those backends remain deferred under DEF-004.

**Migration.** Callers that relied on `DeclaredSize` tolerating a short entry must switch to `Cap(n)`
or `Unbounded` (both keep ceiling-only semantics), or handle `UnexpectedEof`. Callers that used
`Cap(n)` to sniff the first `n` bytes of a larger entry on ZIP/7z/RAR must switch to `DeclaredSize` +
`Read::take(n)`.

## Amendment (2026-08-13, StreamBound residuals R1–R5 — the two-site contract, a uniform `Cap` trigger, symmetric latching, one knob, and honest op labels)

The 2026-08-12 amendment left six residuals. Five of them are settled here (the sixth, the digest
surface's hard cap, changes a different public API and is recorded separately in DCR-011). No public
API changed: `StreamBound` stays `{ DeclaredSize, Cap(u64), Unbounded }`, `ArchiveError` and
`Operation` gain no variants, and every code change is behind `pub(crate)`. The crate stays in 0.3.x.

### R1 — the two-site contract is kept, and promoted to a specified guarantee

A bound violation reaches the caller from one of two places depending on the backend: as an
`io::Error` from a later `read()` on the incremental libarchive path, or as a typed `ArchiveError`
returned by `extract_to_stream` itself on the staging backends, which materialise the entry before
the reader exists. That difference is not a defect to be papered over — it is what "materialises
first" *means* — but leaving it as prose invited callers to write handling for whichever site they
happened to hit. It is now a contract:

> Every bound violation surfaces no later than the read that observes it, and never as silent
> success; a backend that observes the violation during materialisation MAY report it earlier as a
> typed `ArchiveError` from `extract_to_stream` itself — call-time reporting is an earlier delivery
> of the same verdict, never a different verdict.

The two deliveries pair losslessly, and the pairing is pinned in the rustdoc on `StreamBound`,
`Archive::extract_to_stream` and `Archive::extract_to_stream_with_options`:

| Violation | Call-time (staging backends) | Read-time (any backend) |
|---|---|---|
| budget exceeded | `ArchiveError::OperationBlocked` | `io::ErrorKind::InvalidData` at the cap |
| truncation under a declaration | `ArchiveError::Corruption` | `io::ErrorKind::UnexpectedEof` |
| over-production past a declaration | `ArchiveError::Corruption` | `io::ErrorKind::InvalidData` |

A caller therefore writes one handler with two arms — one `match` on the call `Result`, one on
`e.kind()` in the read loop — and never has to know which backend it is on. `docs/API_REFERENCE.md`
carries the canonical example.

Note deliberately that call-time delivery is stated as OPTIONAL and earlier, not as a promise. When
DEF-004 lands genuine incremental readers for ZIP/7z/RAR, those violations move to read time and the
contract above still holds unchanged — which is the main reason the two-site shape is the end-state
contract rather than an interim compromise.

**Both unification directions were considered and rejected.** Uniform *call-time* reporting would
require materialising the entry on libarchive too, destroying SC-009 (<100 MB for 10 GB+ archives) —
a non-starter. Uniform *read-time* reporting requires a lenient stager that never errors on a short
or long payload and defers the verdict to the facade; that design was priced in full and rejected,
because with the facade as the only exactness judge a truncated or corrupt ZIP/7z entry streamed
under `Cap(n)` or `Unbounded` reaches a clean short EOF with the entry's integrity check skipped.
Today those fail at call time: both wrappers run the shared capped read with `require_exact = true`,
ZIP additionally passing the central directory's CRC for a call-time compare (7z's decoder verifies
its own CRC during decompression). Silently blessing exactly the violation class this work exists to
surface is worse than the site difference it removes.

**One hardening lands with the contract.** The RAR staged payload is now length-checked against the
*listing's* declared size when the header carries one, not only against the staged file's own
metadata — the previous check compared the file with itself, so RAR truncation and over-production
only ever surfaced at read time, and only under `DeclaredSize`. `extract_file_core` now returns the
header's declared size alongside the written path (crate-private), and the staging-to-memory path
rejects a mismatch as `ArchiveError::Corruption`. Entries whose header declares no size keep the
read-time `UnexpectedEof` fallback; no declaration is invented for them.

### R2 — the `Cap(n)` trigger is unified on the declared size, at the backend

The 2026-08-12 amendment recorded that "RAR is not uniform with ZIP/7z here" — ZIP and 7z refuse when
the *declared* size exceeds the budget, before decoding; UnRAR aborted only when *decoded* bytes
crossed it, mid-decode. That paragraph is **superseded**. `UnrarArchive::extract_file_core` now
refuses pre-decode when the caller's context carries a cap and the header declares more than it,
emitting the same message shape the shared capped read uses ("Entry '…' declares N bytes; exceeds the
configured per-entry limit of M bytes"), so the diagnostic reads identically on all three staging
backends. `RARProcessFile` is never invoked for a refused entry.

The `UCM_PROCESSDATA` decoded-byte abort stays, in the role ZIP's and 7z's `declared + 1` over-read
already plays: the backstop for a header that under-declares and then over-produces. Entries with no
declared size skip the pre-check and rely on that backstop alone.

The check was placed at the backend, not hoisted to the facade. A facade-level pre-check would also
refuse on libarchive, retiring prefix-serving under `Cap(n)` — a real, documented, tested capability
on the only genuinely incremental backend, and a source-visible behaviour change that would have
forced 0.4.0. Unifying the three staging backends buys the same predictability without paying that.
`extract_all`'s `begin_file` loop is untouched; the pre-check is confined to the context-carrying
single-entry paths, where the facade's metadata gate is the corresponding guard.

### R3 — over-production latches, symmetrically with truncation

`HardCapReader`'s `truncated: bool` becomes `violation: Option<CapViolation>` with a module-private
`enum CapViolation { Truncated, Overrun }`. The latch is checked first, ahead of the empty-buffer
shortcut, and both errors are regenerated per call from the reader's own state (`truncation_error()`
and its new `overrun_error()` sibling are the sole producers of their messages).

Before this, only truncation was sticky. A caller retrying after `InvalidData` consumed one probe
byte per retry, and once the inner stream drained it received a clean `Ok(0)` — an integrity error
decaying into an ordinary EOF. The probe arm now latches `Overrun` before returning, so a retry
replays the verdict without touching the inner reader at all; unit tests wrap the inner cursor in a
counting adapter and assert zero inner reads occur after the latch. An inner `Err(e)` still passes
through *unlatched*: transient I/O is not a deterministic verdict about the bound and may legitimately
succeed on retry. A probe `Ok(0)` stays a clean, unlatched EOF — the entry simply ended at the
ceiling. Closes ticket 4ba1ff1a.

### R4 — one knob is right; `StreamBound::Window` is priced and declined

The claim that `StreamBound` conflates two budgets does not survive contact with the code: the two
are already separate inputs. "Materialise at most X" is `ExtractionLimits`, threaded as
`stream_backend_cap = min(bound tightening, max_file_size, max_total_size)`. "Yield at most Y" is
`DeclaredSize` plus `Read::take(y)` — pinned by an existing test showing that a `take`-clamped window
of a larger entry is not a truncation, because the inner stream never observes EOF.

`Cap(n)` is documented as what it is: a **total-resource assertion** — "no more than `n` bytes may
exist on my behalf, anywhere" — never a read window. The caller who wants a 1 KiB window of a 4 GiB
entry gets it cheaply on libarchive and at full materialisation cost, gated by their own declared
limits, on a staging backend. Refusal when those limits are lower is the *correct* answer: on a
staging backend that window genuinely costs a full materialisation (AD 0035), and an API that hid the
cost would be lying on three of four backends.

A `StreamBound::Window { materialise, yield }` variant was priced and is **declined**: it is a
source-breaking addition to a non-`non_exhaustive` enum, and three of four backends cannot honour it
cheaply — on them it would degrade to exactly the refusal `Cap(n)` already gives. A public
violation-payload type was likewise declined; it remains addable back-compatibly later via
`io::Error` source-chaining if a caller ever needs structured violation data. Recording the decision
here is what stops it being re-litigated, and satisfies the recorded-decision option in ticket
a4071277's acceptance criteria — that ticket stays open for genuine incremental readers (DEF-004),
which is a different question.

A per-backend "what is actually bounded" table now sits in `StreamBound`'s rustdoc, so a reader can
see at a glance that libarchive bounds the reader itself while ZIP/7z bound a staged `Vec<u8>` and
RAR bounds a staged temporary file.

### R5 — stream refusals are labelled `extract_to_stream`

`Archive::extract_to_stream` refused by a staging backend reported `operation: "extract_to_memory"`,
because all three wrappers' capped memory helpers hard-coded the constant — contradicting the
R0071-0010 convention that the label names the *public* operation the caller invoked. The helpers
already took an `op` parameter or were trivially able to; only the constants were wrong.
`ZipArchive::extract_to_memory_capped`, `SevenZArchive::extract_to_memory_capped` and a new
crate-private `UnrarArchive::extract_to_memory_with_limit_op` now take `op: &'static str` and thread
it into the single-entry gate, the ZIP duplicate-path refusal, the shared capped/bounded reads, the
UnRAR extract context, `extract_file_core` and RAR's post-hoc length checks. The memory entry points
pass `extract_to_memory` unchanged; the stream entry points pass `extract_to_stream`.
`Operation::ExtractToStream` already existed, so no enum grew. A label matrix test covers all three
staging backends on both entry points. Closes ticket 581bcda4.

### Observable behaviour deltas (all within 0.3.x)

1. Stream-path refusals report `operation: "extract_to_stream"` instead of `"extract_to_memory"`.
2. A RAR `Cap(n)` refusal fires at call time on the declared size, with the "declares … limit of N
   bytes" message, instead of mid-decode on decoded bytes.
3. An over-production `InvalidData` read error is sticky on retry instead of decaying to `Ok(0)`.
4. RAR staged payloads are length-checked against the listing declaration, so a mismatch is call-time
   `Corruption`.
5. (DCR-011) The digest methods return `Corruption` for truncated CRC-less declared-size entries.

Each is error-classification and diagnostic tightening of the same kind the 2026-08-12 amendment
shipped inside 0.3.x. The affected populations — callers string-matching `operation ==
"extract_to_memory"` on stream-path errors, callers retry-looping past `InvalidData`, and callers
digesting damaged archives successfully — are named in the CHANGELOG with migration lines.

### Residual, named and not chased

On libarchive a gzip trailer CRC failure surfaces at read time rather than at the extract call, which
is a *third* arrival shape for a violation that is not a bound violation at all. It is compatible
with the contract above (no silent success, and it surfaces on the read that observes it), but it is
worth naming so a future reader does not mistake it for a gap in the table.

## Amendment 5 (2026-08-13, R1 correction — Amendment 4's pairing table overclaims)

**Amendment 4 above states a guarantee the code does not give, and this amendment narrows it.**
Two independent reviewers of the Amendment 4 implementation flagged the same defect, and they are
right.

Amendment 4 says, without qualification, that "every bound violation surfaces no later than the read
that observes it" and that call-time reporting is "an earlier delivery of the same verdict, never a
different verdict", so a caller "never has to know which backend it is on". That holds **only under
`StreamBound::DeclaredSize`**.

Under `Cap(n)` and `Unbounded` only the *budget* row of the table is uniform. Declaration integrity
is not part of what those bounds ask for, and what a caller actually gets differs by backend as a
side effect of how each materialises:

- ZIP and 7z stage with `require_exact = true` unconditionally, and RAR now length-checks its staged
  payload against the header declaration under every bound. A truncated entry is therefore a
  call-time `ArchiveError::Corruption` on all three even under `Unbounded`.
- libarchive's reader is incremental and nothing compares its byte count against a declaration the
  bound did not ask about, so the same truncated entry reads to a **clean short EOF**. The suite
  pins exactly this in `cap_bound_tolerates_a_short_entry`.

A clean short EOF versus a typed `Corruption` is a different verdict, not an earlier one. Amendment
4's own R1 rejection paragraph acknowledges the asymmetry that its table then denies — the table was
written for the `DeclaredSize` case and generalised one step too far.

**The corrected contract**, now carried in `StreamBound`'s rustdoc, `Archive::extract_to_stream`,
`Archive::extract_to_stream_with_options` and `docs/API_REFERENCE.md`:

- Under `DeclaredSize`: the Amendment 4 table holds on every backend, and the one-handler-two-arms
  pattern is correct.
- Under `Cap(n)` / `Unbounded`: the budget row holds on every backend. Declaration integrity is
  enforced incidentally by the three staging backends and not at all by libarchive, and callers must
  not rely on it. `DeclaredSize` is the bound that promises it.

This strengthens rather than weakens the practical advice already in the record: `DeclaredSize` is
the safe default for untrusted input, and `Cap(n)` is a resource budget that answers only the budget
question. A caller who wants a read window *and* the integrity check uses `DeclaredSize` with
`Read::take`, which the API reference's example already shows.

No code change accompanies this amendment — the code was correct; the description was not.

**Two smaller Amendment 4 claims corrected at the same time**, both found by the same review:

1. Amendment 4 says "a label matrix test covers all three staging backends on both entry points".
   Only the **stream** arms pin the R5 fix. The memory arm asserts a label produced by the facade's
   single-entry metadata gate, which refuses before the backend runs, so it cannot fail for a
   backend-label regression — and for ZIP/7z/RAR the backend memory-path cap refusal is unreachable
   through the public API at all, because the declared-size gate always fires first. The backend call
   site R5 actually mislabelled *is* pinned, by the stream arm on all three backends; the memory arm
   is a guard, not an oracle. The test's own comment says so; the amendment's summary did not.
2. The R2 migration note in `CHANGELOG.md` described the newly refused population as "an archive with
   an inflated RAR header that previously extracted under a tight `Cap(n)` or tight
   `ExtractionLimits`". The `ExtractionLimits` half is an empty set: the facade's
   `check_single_entry_safe` gate already refused declared-over-limit entries at call time on every
   backend and every path (ceiling `min(max_file_size, max_total_size)`, R0081-0025). Only
   `Cap(n)` can fall below the declared size without that gate having already fired, so the genuinely
   new population is RAR under `Cap(n)`-below-declared. Corrected in the CHANGELOG.
