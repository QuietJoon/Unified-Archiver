---
type: ADR
title: "AD 0035: Defer SevenZ source-side streaming during commit_changes"
description: "OI-0057-007 (in docs/project/open-issues.md) flagged that Archive::commitchanges on a 7z source materialises each retained entry to Vec<u8> before…"
tags: [decision, ADR-0035, OI-0057-007]
timestamp: 2026-04-23T00:00:00Z
status: active
---

# AD 0035: Defer SevenZ source-side streaming during commit_changes

## Context and Problem Statement

OI-0057-007 (in `docs/project/open-issues.md`) flagged that `Archive::commit_changes` on a 7z source materialises each retained entry to `Vec<u8>` before rewriting it, instead of streaming entry payloads through a bounded reader. The write path (`LibarchiveArchive::add_file_from_reader_*` / `ZipWriter::add_file_from_reader_with_metadata`) already accepts an arbitrary `std::io::Read`, so the constraint is the SevenZ *source* side.

`SevenZArchive::extract_to_stream` in `src/ffi/sevenz_wrapper.rs` today calls `extract_to_memory` and wraps the resulting `Vec<u8>` in `std::io::Cursor`, making it a streaming adapter in name only. The commit-time path at `src/modification.rs` calls `extract_to_stream_unchecked`, which dispatches into the same buffered implementation for 7z.

This decision records why this gap is **not** being closed in v0.1.0.

## Decision Drivers

* sevenz-rust2 is the only maintained pure-Rust 7z backend available. Replacing it would blow up scope well past a v0.1 patch.
* Modifying a 7z as large as the entry-payload buffer forces peak RSS to scale with the largest single entry (not archive size), which is unpleasant but not catastrophic for typical archives.
* The plan explicitly allowed a docs-only close if the upstream crate did not expose an entry-level `Read`.

## Investigation (sevenz-rust2 0.19.4)

The crate's public surface for reading entry data is:

| API | Shape | Buffers entry? |
| --- | --- | --- |
| `ArchiveReader::for_each_entries(F)` where `F: FnMut(&ArchiveEntry, &mut dyn Read) -> Result<bool, Error>` | callback-scoped `&mut dyn Read` | no, but the reader cannot outlive the closure |
| `ArchiveReader::read_file(&mut self, name: &str) -> Result<Vec<u8>, Error>` | owned `Vec<u8>` | **yes** — unconditional buffering |
| `BlockDecoder::for_each_entries(&mut F)` | callback-scoped `&mut dyn Read` | no, but same lifetime constraint |
| `decompress_with_extract_fn_and_password` | callback-scoped | same |

There is no public function that returns an owned `Read` impl tied to a single entry. The callback-based designs stem from sevenz-rust2's need to hold and mutate the decoder state across entries (critical for solid blocks), which is fundamentally incompatible with yielding a free-standing `Read` back to the caller.

Confirmed by grep on `/Volumes/Common/cargo/registry/src/index.crates.io-*/sevenz-rust2-0.19.4/src/reader.rs` at lines 1432, 1466, 1539, 1637 — all public entry-data entry points are either callback-scoped or return `Vec<u8>`.

## Considered Options

1. **Thread-plus-pipe adapter.** Spawn a background thread that runs `for_each_entries`, writing decompressed bytes into the write end of a `os_pipe`/`mpsc::SyncSender`, and return the read end wrapped in `Box<dyn Read>`. Adds a real concurrency surface (deadlock on early-drop of the reader, error propagation across the thread boundary, lifetime of password/secstr across threads) and a new dependency.
2. **Self-referential struct via `ouroboros` or unsafe.** Keep the `ArchiveReader` and the active callback's `&mut dyn Read` in one owned struct. Fragile — sevenz-rust2 does not export a stable type for the callback-scoped reader, so we cannot name it in a struct field. Self-ref via unsafe would bind us to internals that can change.
3. **Do nothing; document the constraint and defer.** Keep the current buffered `extract_to_stream` on SevenZ, close OI-0057-007 as *partially addressed* (the broader non-libarchive streaming gap is tracked as DEF-004), and pick up the work when sevenz-rust2 (or a successor) exposes an owned entry-level reader.

## Decision Outcome

ACCEPT option 3 for v0.1.0.

Rationale:

* Options 1 and 2 are both disproportionate to the actual risk surface on a v0.1 crate. The existing buffer cap (`ExtractionLimits::max_per_file_size`) already bounds peak RSS per commit, so the defect is a *space inefficiency*, not a correctness or DOS hole.
* The plan authorised this fork explicitly: "If not: leave code unchanged, archive OI-0057-007 as a partial-close with a written decision record citing the sevenz-rust2 API limitation, and remove the SevenZ row from OI-0057-007's remaining-backends list."
* DEF-004 (in `docs/project/stub-manifest.md`) already tracks the broader non-libarchive streaming gap, so deferring here does not drop the issue from the backlog.

## Consequences

* `src/ffi/sevenz_wrapper.rs::extract_to_stream` retains its `Cursor<Vec<u8>>` implementation. The existing rustdoc note already warns that "The sevenz-rust2 API uses a callback-based extraction model that doesn't support streaming reads."
* `commit_changes` against a 7z source will continue to buffer each retained entry up to the `ExtractionLimits::max_per_file_size` cap.
* OI-0057-007 in `docs/project/open-issues.md` is updated to reflect *partial* resolution: LibarchiveArchive and ZipReader sources stream; SevenZ source is deferred to DEF-004, contingent on upstream-crate support.

## Revisit trigger

Re-open this decision when **any** of the following is true:

* sevenz-rust2 (or a maintained fork) exposes an owned entry-level `Read` implementation.
* A real-world usage report shows RSS pressure from the per-entry buffer that `ExtractionLimits` cannot mitigate.
* The library migrates away from sevenz-rust2 for an unrelated reason; streaming should be reconsidered as part of that migration.

## Amendment (2026-09-03, pointer refresh and a field misnaming)

**Pointer.** OI-0057-007 no longer lives in `docs/project/open-issues.md`. It was closed and moved
to `docs/project/open-issues-resolved.md`; look for it there.

**Misnaming.** Both mentions of `ExtractionLimits::max_per_file_size` above name a field that does
not exist and never did. The field and its accessor are `ExtractionLimits::max_file_size`
(`src/security.rs`). The reasoning is unchanged — a per-entry ceiling does bound peak RSS per commit
— only the identifier was wrong.

## Amendment (2026-09-05, the option set was incomplete; "disproportionate" is withdrawn as the verdict on the gap)

The owner has since defined what **this library** means by streaming, and it is
narrower than what any dependency means by it:

> extract an entry **without writing it to disk**, process it in memory —
> mainly for checksum and integrity computation.

Judged against that definition, this record's investigation stands and its
conclusion does not.

**What stands.** The upstream finding is still true at the pinned version:
`sevenz-rust2` exposes no owned entry-level `Read`. A caller cannot be handed a
reader it keeps. That was checked again, not assumed.

**What was missed.** The two options priced here — a thread-plus-pipe adapter,
and a self-referential owning struct — are both attempts to manufacture a **pull**
reader from a **push** source. A third option was never considered: *do not
manufacture a reader at all; push the decoded chunks into a caller-supplied
sink.* That option is not blocked upstream, is not exotic, and **is already the
shape used twice in this crate** — by `ReadBackend::visit_payloads_by_listing_id`
and by `SevenZArchive::test_integrity`, which decodes an entry incrementally with
no temp file and no whole-entry buffer today.

So "disproportionate" is retained as a correct verdict **on options 1 and 2**,
and **withdrawn as the verdict on the gap**. The gap has a cheap answer that was
not on the table when this record was written.

**Why the push shape is the right one here, and not merely cheaper.** The
owner's definition is about *not touching disk* and *processing in memory* — a
hasher being fed bytes. That is inherently a push. Forcing it through a pull
reader is what made the problem look hard, and it is why the one backend that
genuinely cannot supply a pull reader (RAR, whose SDK emits blocks through a
callback) looked worst while being the most natural fit for a sink: its callback
already receives the decoded buffer and this crate currently ignores that
pointer.

**Consequences.**

- DEF-004's exit criterion is rewritten from "all three backends provide true
  streaming readers" to the owner's definition: every backend can deliver an
  entry's payload to a caller-supplied sink without a temp file. Under the old
  wording DEF-004 could never close, because it demanded of 7z something the
  dependency does not offer.
- 7z is no longer parked on an upstream revisit trigger. The revisit trigger in
  this record's body applies to *owned readers*, which remain unavailable; it
  does not gate the sink route.
- This record's `status` should move to `superseded` once the sink decision has
  its own record to name as successor.
