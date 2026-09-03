---
type: DCR
title: "Digest methods hold CRC-less entries to their declared size exactly"
description: "calculate_content_multiset_digest_and_size and its shims return Corruption for a truncated CRC-less entry instead of digesting the short payload."
tags: [change, project-control, DCR-011]
status: active
---

# DCR-011: Digest methods hold CRC-less entries to their declared size exactly

- **Date:** 2026-08-13
- **Source:** Review 0001, residual R6; TicGit `c0d6fad6`
- **Related:** DCR-006 (bounded streaming; Amendments 3 and 4), OI-0001-001, TicGit `82bf8fd4`

## What Changed

`Archive::entry_crc32_for_digest` — the private step behind
`calculate_content_multiset_digest_and_size`, `calculate_manifest_digest` and
`calculate_manifest_summary` — bounded the CRC hasher with
`StreamingExtractor::with_hard_cap(declared)`. That is a *ceiling*: it caught an over-producing
decoder but let a stream that ended early be hashed as-is. For the formats that reach this code path
at all — the ones without a per-entry checksum in their metadata, i.e. the TAR family, CPIO and ISO —
a truncated member therefore digested its short payload and the call returned `Ok`.

The bound is now:

- `Some(declared)` → `with_exact_size(declared)`. A stream that ends before the declaration, or
  produces past it, is a violation.
- `None` → `with_hard_cap(DEFAULT_MAX_FILE_SIZE)`, unchanged. No declaration is invented for an entry
  that does not carry one (raw gzip/bzip2/xz single-file readers); an early end of stream stays an
  ordinary EOF.

At this one call site the resulting `ArchiveError::Io` — whose `source.kind()` is `UnexpectedEof` for
truncation or `InvalidData` for over-production — is rewrapped as `ArchiveError::Corruption` naming
the entry, so a declared-size violation reaching the caller through the digest API is classified the
same way it is everywhere else in the crate.

## Why

This is the same defect class as OI-0001-001, reached through a different public API. DCR-006
Amendment 3 made `extract_to_stream` under `StreamBound::DeclaredSize` exact-length precisely so a
truncated entry in a checksum-less format could not read as a short but valid payload. The digest
surface — whose entire purpose is content identity, and whose known consumer uses it for
deduplication decisions — kept the weaker bound, so the same damaged archive that `extract_to_stream`
now rejects still produced a confident digest. A digest computed over half an entry is not a weaker
digest; it is a wrong answer that looks like a right one.

## Why the rewrap is local, not global

The obvious alternative was to teach `ffi::common::map_entry_read_error` to classify every
`UnexpectedEof` as `Corruption`. That helper is on *every* backend's read path — it is called from
the shared bounded in-memory read and from the CRC reader used by all backends — so the change would
have reclassified errors crate-wide, far outside the surface this defect touches, with a blast radius
neither design pass analysed. The site-local rewrap avoids it entirely.

Keying on `io::ErrorKind` is safe *here* specifically because the only reader in play at this call
site is the crate's own capped stream: the kinds cannot arrive from a foreign reader whose meaning
for `InvalidData` differs.

## Observable Behaviour Change

Archives that digested successfully for years now return `ArchiveError::Corruption` if a CRC-less
member is truncated or over-produced relative to its declaration. This is intended and is the point
of the change, but it is a real behaviour change for existing callers: AdvancedDeduplicator is the
known consumer and must handle the error rather than assuming a digest is always obtainable.

The digest *value* for a healthy archive is unchanged. A regression test pins
`tests/fixtures/test.tar`'s digest to the value measured on the tree immediately before this change
landed, so exactness cannot silently move the identity of well-formed archives.

## Known Risk: Sparse Entries

Exactness assumes libarchive delivers an entry's full *logical* extent, holes materialised as zeros.
If some format or reader returned only the stored data blocks, an exact bound would read that as
truncation and produce a false `Corruption`. A sparse-TAR fixture test is the tripwire: it builds a
sparse member with the `tar` CLI (skipping when unavailable), then asserts both that a
`DeclaredSize` stream reaches a clean EOF at the logical size and that the digest succeeds. If a
real-world variant ever trips it, the correct response is to gate exactness on non-sparse entries and
file a follow-up — not to ship a false `Corruption`.

## Scope

`82bf8fd4` (the OI-0001-009 single-traversal digest rewrite) stays a separate, open work item. This
change is deliberately decoupled from it so the semantic fix does not wait on a performance rewrite;
that ticket gains an acceptance criterion requiring the rewrite to preserve `with_exact_size`
exactness. Closes `c0d6fad6`.

## Amendment (2026-08-13, the sparse tripwire does not fire on the primary dev platform)

**The Known Risk above overstates its own mitigation, and this amendment records what the tripwire
actually covers.** A reviewer verified empirically that bsdtar 3.5.3 — the `tar` on macOS, which is
this project's primary development platform — accepts `tar -cSf` and exits 0, but documents `-S` as
extract-mode-only and therefore writes a **dense** 1,052,160-byte archive. The test keyed its skip on
the exit status alone, so on macOS it did not skip: it ran to completion against a non-sparse member
and asserted that a mostly-zero TAR entry digests and streams exactly. That is a true statement and
an entirely different one from "libarchive materialises holes as zeros", which is the assumption
exactness actually rests on.

The test now checks the produced archive's SIZE and skips loudly when it is at or above the logical
member size, naming bsdtar as the expected cause and stating that the Known Risk stays unexercised on
that host. So the tripwire is honest about when it is armed:

- **GNU-tar hosts (Linux, or macOS with gtar on PATH):** armed, and it is a real tripwire.
- **bsdtar hosts (macOS default):** skips with an explicit message, and this risk is **unmitigated
  there**.

The recorded response if it ever fires is unchanged — gate exactness on non-sparse entries and file a
follow-up, never ship a false `Corruption`. What changes is that a green run on a developer's Mac can
no longer be read as evidence that the sparse case is covered. Closing it properly needs either a
committed sparse-TAR fixture (so no `tar` CLI is involved at all) or a CI job on a GNU-tar host —
the latter is `1340e934`'s territory, since the repository has no CI configuration at all.

## Amendment (2026-08-17, the Scope section's open-work premise no longer holds — `82bf8fd4` landed)

**The `## Scope` section above says `82bf8fd4` (the OI-0001-009 single-traversal digest rewrite)
"stays a separate, open work item". That is no longer true of the tree**, and this amendment records
the change rather than editing the sentence, which was accurate when written.

The rewrite has landed. `src/backend.rs` carries `PayloadTarget`, `PayloadVisitor` and
`ReadBackend::visit_payloads_by_listing_id`; `src/ffi/libarchive_wrapper/reader.rs` overrides it with
a single-traversal walk that hands each target a `BorrowedEntryReader` over one shared handle; and
`src/inspection.rs::resolve_crc32_single_pass` drives it, falling back to the per-entry resolver on
`NotImplemented` or on non-unique listing ids. What stays correct in that section is its *reasoning*:
the decoupling worked exactly as intended — this record's semantic fix shipped first and did not wait
on the performance rewrite.

**The acceptance criterion this record imposed on that ticket was met.** The Scope section required
the rewrite to preserve `with_exact_size` exactness. It does, and not by reimplementation: both the
one-pass route and the per-entry fallback compute their CRC through the same `crc32_of_bounded_payload`
helper under the same declared-size bound, so this record's truncation verdict and its diagnostics are
shared code on both routes. The walk itself applies no size bound — the exact/ceiling contract stays
with the caller, which is the only party that knows whether the listing declared a size.

Verified by execution on 2026-08-17, not by reading a changelog:

* `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test digest_exactness_test --
  --test-threads=4` — exit 0, **17 passed / 0 failed / 0 ignored**. This record's own tripwire
  `truncated_tar_entry_makes_the_digest_report_corruption` still returns `Corruption` naming the
  truncated entry, so the rewrite did not weaken the verdict.
* `TMPDIR=/Volumes/Temp/claude cargo test --all-features --test integration_tests --
  manifest_digest_perf --test-threads=4 --nocapture` — exit 0, 1 passed, reporting
  `list_files 1.352166ms, manifest_digest 3.245833ms over 1000 entries (ratio 2.4x, bound 50x)`.
  The sentinel runs in the **default** lane; its `#[ignore]` is gone.

The sparse-entry Known Risk and its 2026-08-13 amendment are untouched by any of this: the one-pass
walk reads the same payload bytes libarchive would have produced on a re-open, so whether holes
materialise as zeros is still libarchive's behaviour and still unmitigated on bsdtar hosts.

## Amendment (2026-09-03, the sparse tripwire is now armed on every host — fixture committed)

The 2026-08-13 amendment's host table is no longer accurate, and this is the good direction. It
reads:

- **GNU-tar hosts:** armed, and it is a real tripwire.
- **bsdtar hosts (macOS default):** skips with an explicit message, and this risk is **unmitigated
  there**.

**Both bullets are now obsolete: the tripwire is armed everywhere, unconditionally.** That
amendment closed by naming two exits — "a committed sparse-TAR fixture (so no `tar` CLI is involved
at all) or a CI job on a GNU-tar host — the latter is `1340e934`'s territory, since the repository
has no CI configuration at all." The first exit was taken on 2026-09-03, which also makes the
second moot twice over: AD-0070 has since ruled that no hosted CI will ever exist here, so a lane
waiting on one would have waited forever.

What landed: `scripts/generate-tar-fixtures.sh` produces the archive once on a GNU-tar host, and
`tests/fixtures/sparse.tar` — 10,240 bytes on disk, typeflag `S`, 1 MiB logical — is committed
beside it. `sparse_tar_entry_digests_and_streams_to_clean_eof` in `tests/digest_exactness_test.rs`
is now a plain `#[test]`: no `tar` CLI, no runtime skip, no `#[ignore]`. It ran green in the
2026-09-03 gate.

The lane also defends its own meaning, which matters because the failure mode here was never a red
test — it was a green one that proved nothing. It asserts the fixture is smaller than the logical
member size and that the member's typeflag is `S`, and fails loudly naming
`scripts/generate-tar-fixtures.sh --force` if either breaks. A dense or re-generated-on-bsdtar
fixture therefore cannot quietly degrade this back into "a mostly-zero member digests exactly",
which is true, trivially passes, and is not the Known Risk.

**The Known Risk section itself is unchanged and still governs**: if hole-materialisation ever does
differ, the recorded response is to gate exactness on non-sparse entries and file a follow-up,
never to ship a false `Corruption`. What changed is only that the risk is now actually exercised
rather than declared unexercised.
