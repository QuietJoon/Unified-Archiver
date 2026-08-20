---
type: ADR
title: "AD 0047: Content-based manifest_digest on CRC-less formats"
description: "Archive::calculatemanifestdigest returns a stable, content-identity digest over an archive's entries. Ruling stands; premises refreshed (amended 2026-08-05, §B): the digest is CRC32-only and integrity-grade, the O(N^2) CRC-less walk is unbounded by the OI-0080-003 entry budget, and a stored CRC of Some(0) escapes the recompute path per AD 0012. Amended 2026-08-17: open question (c) is now DECIDED-AND-IMPLEMENTED — AE-2 ZIP entries list crc32 None and are streamed (TicGit 04ba4897 / DCR-012), so the Some(0) clause holds only for non-exempt entries; the SHA-256 and walk-budget questions stay open."
tags: [decision, ADR-0047]
timestamp: 2026-04-21T00:00:00Z
status: active
---

# AD 0047: Content-based manifest_digest on CRC-less formats

## Context and Problem Statement

`Archive::calculate_manifest_digest` returns a stable, content-identity digest over an archive's entries. Review 0064 (Cluster 9, cited by the DEFER resolution) flagged that the prior implementation fell back to `"{path}:{size}"` when an entry's `crc32` was `None` — which happens unconditionally for TAR, TAR+gz/bz2/xz, and ISO, because libarchive does not expose per-entry CRC32s for those formats.

Consequence: two semantically-different archives with the same paths and sizes (e.g. a TAR rebuilt with identical layout but modified file contents) produced the same manifest digest, defeating the "content identity" contract the method advertises.

## Decision Drivers

* The digest's advertised purpose is content identity, not layout identity. Path+size collides on a realistic and easy-to-hit case (any rebuild that preserves the file list but edits one byte inside one file).
* libarchive exposes a streaming `Read` per entry even when it does not expose a stored CRC32 — streaming the content and computing CRC32 at read-time is the obvious upgrade and requires no format-specific branching.
* Streaming one entry at a time keeps the memory profile bounded; there is no need to materialize whole entries in memory.
* The per-entry reopen cost on compressed TAR (`.tar.gz` / `.tar.bz2` / `.tar.xz`) via libarchive is O(N²) in the number of entries, because each entry reopens the stream and re-decompresses everything before it. This cost is real and must be surfaced in the rustdoc so callers on large compressed-TAR archives reach for the cheaper `calculate_archive_crc` summary instead.

## Considered Options

1. **Keep the path:size fallback.** Rejected: fails the content-identity contract on the most common CRC-less formats.
2. **Stream per-entry CRC32 using `extract_to_stream_unchecked` + `compute_crc32_reader`.** Accepted. The helper `entry_crc32_for_digest` composes these two primitives, and `calculate_manifest_digest` uses it unconditionally — no path:size fallback remains.
3. **Compute a stronger hash (SHA-256) per entry.** Rejected for v0.1.x: CRC32 is adequate for manifest-equality and cheaper; upgrading is out of scope for this review and would be a separable decision.

## Decision Outcome

ACCEPT the content-based upgrade. Status: Implemented in src/inspection.rs via `entry_crc32_for_digest`; path:size fallback removed; `# Performance` rustdoc tightened to warn about the O(N²) cost on compressed TAR and to steer callers to `calculate_archive_crc` when they only need a summary.

### Implementation

* `src/inspection.rs`: new `entry_crc32_for_digest` helper; `calculate_manifest_digest` now calls it for every entry whose `crc32` is `None`.
* `docs/API_REFERENCE.md` and `src/inspection.rs` rustdoc: Performance section updated with the O(N²)-on-compressed-TAR caveat and a pointer to `calculate_archive_crc`.
* Regression test: `test_calculate_manifest_digest_tar_is_stable_and_nonempty` asserts that a TAR's manifest digest is stable across calls and non-empty.

## Consequences

* Good: manifest-identity contract now matches the rustdoc claim on every supported format; two archives with identical paths/sizes but different contents produce different digests.
* Bad: `calculate_manifest_digest` is now visibly slower on large compressed TAR archives. Callers who only need a coarse summary should use `calculate_archive_crc` — the rustdoc now says so explicitly.

## Related

* `src/inspection.rs::calculate_manifest_digest`, `src/inspection.rs::entry_crc32_for_digest`.
* `src/inspection.rs::calculate_archive_crc` — the cheap summary path, linked from the Performance rustdoc.
* Review 0064, DEFER cluster "content-based manifest_digest."

## Amendment (2026-08-05, decision-review-2026-07-19 §B — premises refreshed, ruling stands)

The 2026-07-19 review asked for three factual refreshes here. None disturbs the accepted upgrade —
the `"{path}:{size}"` fallback is still gone and `src/inspection.rs::entry_crc32_for_digest` still
streams CRC-less entries through a hasher — but each sharpens or corrects a premise this record
states.

**(a) The digest is CRC32 end to end, and therefore integrity-only.** Both layers use `crc32fast`:
per CRC-less entry via `src/ffi/common.rs::compute_crc32_reader`, and the final fold in
`src/inspection.rs::content_multiset_digest_and_size`, which sorts the per-entry
`content_digest_element` strings, joins them with `,`, and CRC32-hashes the join into eight
lowercase hex chars. `crc32fast` is the crate's only hash dependency (Cargo.toml), so Considered
Option 3 (SHA-256) is still unimplemented. A 32-bit non-cryptographic checksum is a **content
fingerprint for equality and deduplication, not a tamper-evidence mechanism**: CRC32 collisions are
cheap to construct deliberately, and the 8-hex output space makes accidental collisions likely
around the tens-of-thousands-of-archives scale. docs/API_REFERENCE.md already states the limit in
its `Archive::calculate_manifest_digest` "Contract" note ("Collision resistance is CRC32-grade …
but not cryptographically strong"); the rustdoc in `src/inspection.rs` only compares the digest
*relatively* — "Unlike [`Self::calculate_archive_crc`] (wrapping sum — less collision-resistant)" —
and never states the absolute limit, and neither does docs/STREAM_CRC32.md's comparison table, whose
collision-resistance row is likewise purely relative ("Low (addition is lossy)" against
"Higher (preserves per-entry identity)").

**(b) The O(N²) CRC-less path is real and is NOT bounded by the OI-0080-003 entry-count budget.**
`calculate_content_multiset_digest_and_size` lists through `Archive::list_files` →
`Archive::list_entries` → `ReadBackend::list_files()`, which is the provided delegate that passes
`budget = None`; the budget reaches backends only via `Archive::list_files_for_limits_budgeted`,
which the limits-carrying extraction paths call with `Some(limits.max_entry_count)`. The digest
carries no `ExtractionLimits` at all — it hands `ExtractionLimits::unlimited()` to
`ValidatedSource::extract_to_stream_by_id` deliberately (R0076-0091), so N is bounded only by the
archive. A budget would not have fixed the quadratic term either: `LibarchiveStreamReader::open`
reopens the archive and walks/`data_skip`s headers from the first entry for *every* CRC-less entry,
so on `.tar.gz` / `.tar.bz2` / `.tar.xz` the prefix is re-decompressed once per entry. The budget
bounds the listing walk — for libarchive it is a true streaming early abort that bounds the
library's header reads too, as that backend's `ReadBackend::list_files_budgeted` impl in
`src/backend.rs` records — but nothing about it
bounds the per-entry reopen, which is where the quadratic term lives. What *is* bounded is bytes-per-entry:
R0076-0091 caps the hasher at `entry.size` (hard cap) or `DEFAULT_MAX_FILE_SIZE` when the listing
declares no size.

**(c) The AD 0012 seam: a stored CRC of `Some(0)` escapes the recompute path.**
`entry_crc32_for_digest` branches on presence only (`if let Some(crc) = entry.crc32 { return
Ok(crc); }`), so it streams a payload exactly when the listing carries `None` — never when it
carries `Some(0)`, which AD 0012 ("Treat CRC32 value zero as valid, not absent") deliberately made
a valid present checksum. The consequence differs per backend:

* **ZIP** — the listing sets `entry.crc32 = Some(zip_file.crc32())` unconditionally for file
  entries (`src/ffi/zip_wrapper.rs`). AE-2 AES entries store 0 in the central-directory CRC32 field
  by specification, which is exactly why `crc32_check_exempt` disables CRC verification on the
  extract/integrity side — but the digest takes the placeholder at face value, so on an
  AE-2-encrypted ZIP every entry contributes the constant element `00000000` and the digest
  collapses to a function of the file count and the duplicate-path ordinals, not of content.
* **UnRAR** — same face-value shape: `Some(header.file_crc)` for every non-directory entry
  (`src/ffi/wrapper.rs`).
* **7z** — the exception: `SevenZArchive::entry_crc32` guards on `entry.has_crc`, so a genuinely
  CRC-less 7z entry lists `None` and does take the streaming recompute path (see
  `test_sevenz_crcless_entries_list_none_and_extract_verified`).

AD 0012's own Consequences anticipated the metadata case ("entries that genuinely lack CRC metadata
(rare) will show CRC=0") but not the digest's early return, and the extract-side AE-2 exemption is
not mirrored in `entry_crc32_for_digest`.

**Disposition: this record stays ACTIVE.** Nothing above reverses the content-based upgrade it
accepted, and no later record replaced its ruling. Three questions are left **open for the owner**
rather than decided here: whether the digest should treat a zero-CRC placeholder on an encrypted
ZIP entry as absent and stream instead (the AD 0012 interaction); whether the digest should move to
a cryptographic hash, i.e. revisit Considered Option 3; and whether the CRC-less walk should carry a
budget or a documented ceiling. Cross-references: AD 0012 (CRC32 zero is valid); OI-0080-003
(RESOLVED 2026-07-19 — the budgeted-listing work whose scope excludes this path); R0076-0091 (the
per-entry hasher cap); docs/API_REFERENCE.md (`Archive::calculate_manifest_digest` Contract note)
and docs/STREAM_CRC32.md ("Manifest Digest" section).

## Amendment (2026-08-17, pass-2 record reconciliation — open question (c) is now DECIDED-AND-IMPLEMENTED; the ZIP bullet of amendment (c) no longer describes the crate)

Two changes landed together on 2026-08-16, both citing **this record** as their authority, and both
still **uncommitted** on branch `001-unified-archive` at the time of writing:

* **DCR-012** — the content-multiset digest drops the per-path occurrence ordinal.
* **TicGit `04ba4897`** (R0079-0007) — AE-2 AES ZIP entries stop listing `crc32 = Some(0)`.

Because this record is the crate's canonical statement of digest identity, leaving amendment (c)
unamended would leave that statement describing behaviour the crate no longer has. The 2026-04-21
ruling is untouched: the content-based upgrade stands, the `"{path}:{size}"` fallback is still gone,
and this record stays **ACTIVE**.

### The ZIP bullet of amendment (c) is now false in both halves

**Half one — "the listing sets `entry.crc32 = Some(zip_file.crc32())` unconditionally for file
entries".** It no longer does. Verified in `src/ffi/zip_wrapper.rs`:

```rust
if entry.entry_type == EntryType::File && !crc32_check_exempt(&zip_file) {
    entry.crc32 = Some(zip_file.crc32());
}
```

with

```rust
fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted() && zip_file.crc32() == 0
}
```

`ArchiveEntry` initialises `crc32: None`, so an AE-2 entry lists `None` and
`entry_crc32_for_digest` takes the streaming recompute path for it — the same path 7z already took.
The extract-side AE-2 exemption this amendment said "is not mirrored in `entry_crc32_for_digest`" is
now mirrored, by sharing the very predicate the extract side uses.

The gate keys on `encrypted() && crc == 0`, never on the value alone, so the AD 0012 seam this
amendment identified is preserved rather than widened: a plaintext empty file still lists `Some(0)`
(`CRC32(b"") == 0`) and never takes the recompute path. `test_zip_wrapper_plaintext_empty_file_still_lists_crc32_zero`
pins that. AE-1 entries carry a real stored CRC and are unaffected.

**Half two — "the digest collapses to a function of the file count and the duplicate-path
ordinals".** False twice over. The collapse is gone (real per-payload CRC32s participate), and
duplicate-path ordinals no longer exist anywhere in the encoding: DCR-012 reduced
`content_digest_element` to `format!("{crc32:08x}")` for every file entry, with multiplicity carried
by repetition in the element vector. No path, name, size, listing id or occurrence index
participates in the digest at all.

The **UnRAR** and **7z** bullets of amendment (c) are unchanged and remain accurate as written
(`Some(header.file_crc)` in `src/ffi/wrapper.rs`; the `has_crc` guard in
`SevenZArchive::entry_crc32`).

### Open question (c) resolved

This record left three questions **open for the owner**. The first — *whether the digest should treat
a zero-CRC placeholder on an encrypted ZIP entry as absent and stream instead (the AD 0012
interaction)* — is now **DECIDED in the affirmative and IMPLEMENTED**, by TicGit `04ba4897` under
DCR-012. It was decided in code rather than by a separate owner ruling; it is recorded here so this
record stops soliciting a decision already taken.

The other two remain **open**, unchanged: whether the digest should move to a cryptographic hash
(Considered Option 3 / SHA-256), and whether the CRC-less walk should carry a budget or a documented
ceiling.

### What the resolution opened, and what it did not change

1. **A new precondition on the public digest API.** Digesting an AE-2 ZIP now requires the password:
   the entries must be decrypted to be hashed. `calculate_content_multiset_digest_and_size` on a
   password-protected AES ZIP opened without a password returns `Err` where it previously returned
   `Ok` — and that previous `Ok` was precisely the content-blind fold this resolution removes. The
   precondition is undocumented in the rustdoc and `docs/API_REFERENCE.md`. The error is classified
   `ArchiveError::Format` rather than `ArchiveError::Password`; that mislabel is pre-existing on the
   ZIP no-password read path and is owned by TicGit `9bdf2c` — not fixed here, and not endorsed.
2. **The ZIP CRC-less path is not fully wired, so a duplicate-path AE-2 ZIP now errors.**
   `ZipArchive::extract_to_stream_by_listing_id` exists as an inherent method but is not overridden
   in `impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive` in `src/backend.rs`
   (``cargo check`` reports it as never used). The digest walk therefore hits the trait default,
   gets `NotImplemented`, falls back to the by-*path* stream, and meets
   `security::validate_single_entry` / `reject_if_duplicate` — the OI-0076-002 gate. On an AE-2 ZIP
   holding two names that normalize to one path, the digest returns
   `OperationBlocked { operation: "extract_to_stream", reason: "Multiple entries match ..." }`. Fixing
   it is a one-line override plus a facade-level regression test; `src/backend.rs` was outside every
   pass-1 agent's file ownership. See DCR-012's 2026-08-17 amendment §2.
3. **Cost shape: amendment (b)'s quadratic term is libarchive-specific and does not extend to ZIP.**
   The O(N²) reopen-and-re-decompress behaviour belongs to `LibarchiveStreamReader::open`. `ZipArchive`
   serves every read from one cached handle (`with_zip`), so AE-2 entries entering the streaming path
   add no quadratic reopen. What they do add is the DEF-004 / OI-0057-007 buffered-adapter cost —
   each entry is fully materialized in memory and fully decrypted before the cursor is handed back —
   i.e. memory proportional to the largest entry, CPU proportional to total encrypted bytes.
   Amendment (b) otherwise stands unchanged: the digest still hands `ExtractionLimits::unlimited()`
   to `ValidatedSource::extract_to_stream_by_id` (R0076-0091), N is bounded only by the archive, and
   the sole byte bound is the per-entry cap — exact at the declared size since DCR-011, ceiling-only
   at `DEFAULT_MAX_FILE_SIZE` when the listing declares none.
4. **Amendment (a) stands verbatim.** The digest is still CRC32 end to end and integrity-grade only;
   `crc32fast` is still the crate's only hash dependency. Streaming AE-2 payloads changes which bytes
   are hashed, not the strength of the fold.
5. **Unreleased.** `Cargo.toml` still reads `version = "0.3.1"`. DCR-012 prescribes a 0.3.1 → 0.4.0
   minor bump with a BREAKING CHANGELOG entry; the owner has not authorised it, so the bump is
   deferred and none of the behaviour described in this amendment has shipped in a released version.

## Amendment (2026-08-17, pass-3 record reconciliation — items 2 and 3 of the amendment above are superseded by the tree; the `crc32_check_exempt` block it quotes is one conjunct short)

Nothing above is rewritten, the earlier 2026-08-17 amendment included. It is left intact because it
was an accurate snapshot of the tree at the hour it was written: pass 2 ran its code agent and its
documentation agents concurrently, so the documentation agents recorded — with real reproductions — a
defect the code agent was closing in the same window. The 2026-04-21 ruling is still untouched, the
`"{path}:{size}"` fallback is still gone, and this record stays **ACTIVE**.

Everything below was re-verified against the working tree of branch `001-unified-archive` on
2026-08-17, where it remains **uncommitted**. The digest value quoted was recomputed here, not copied
from an earlier report.

### Item 2 is superseded: the ZIP CRC-less path IS fully wired, and a duplicate-path AE-2 ZIP digests

Item 2 is headed "The ZIP CRC-less path is not fully wired, so a duplicate-path AE-2 ZIP now errors".
It is no longer true in any of its parts.

`impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive` in `src/backend.rs` now overrides the
trait default:

```rust
#[inline]
fn extract_to_stream_by_listing_id(
    &self,
    id: usize,
    validated_path: &str,
) -> Result<StreamingExtractor> {
    Self::extract_to_stream_by_listing_id(self, id, validated_path)
}
```

So the digest walk no longer reaches `NotImplemented`, no longer falls back to the by-*path* stream,
and never meets `security::validate_single_entry` / `reject_if_duplicate`. `cargo check` reports no
"never used" method — `cargo build --all-features --all-targets` completes with **zero** rustc
warnings. The `OperationBlocked { operation: "extract_to_stream", reason: "Multiple entries match
..." }` outcome item 2 describes does not reproduce.

Item 2 called the remedy "a one-line override plus a facade-level regression test". Both shipped.
The test is `tests/integration/digest_ae2_duplicate_path.rs`, asserting through `Archive` rather than
the wrapper — deliberately, because an inherent-method test cannot detect a missing `ReadBackend`
forward. Its fixture is one AE-2 AES ZIP whose central directory holds the raw names `sub/a.txt` and
`sub\a.txt`, which normalize to the single path `sub/a.txt` (R0076-0048), carrying payloads `b"alpha"`
and `b"bravo"`; it first asserts the fixture really lists two entries at that one path, both
`is_encrypted` and both `crc32 == None`, so the digest assertions cannot pass without exercising the
dispatch under test. Recomputed independently for this amendment: `crc32("alpha") = d0e0396a`,
`crc32("bravo") = 099bb889`, sorted join `099bb889,d0e0396a`, and the CRC32 of that join is
**`6a4fd559`** with a declared total of **10** bytes — which is exactly what
`calculate_content_multiset_digest_and_size` returns. The neighbouring `assert_ne!` against the
`[first, first]` digest is what proves the shadowed occurrence was not re-hashed from the first, so
the equality is not carrying the argument alone. Two further cases pin content sensitivity through
the shared path and the no-password error at the facade.

### The `crc32_check_exempt` code block quoted under "Half one" is one conjunct short

The earlier amendment quotes, as verified source,

```rust
fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted() && zip_file.crc32() == 0
}
```

and restates it as "The gate keys on `encrypted() && crc == 0`, never on the value alone". The shipped
predicate in `src/ffi/zip_wrapper.rs` has a third conjunct:

```rust
fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted()
        && zip_file.crc32() == 0
        && aes_vendor_version(zip_file) == Some(AES_VENDOR_VERSION_AE2)
}
```

`AES_VENDOR_VERSION_AE2` is `0x0002`, and `aes_vendor_version` reads it out of the WinZip-AES `0x9901`
extra field by walking the entry's extra-field chain (`id:u16 | len:u16 | body[len]`, little-endian)
from `ZipFile::extra_data()`; the `zip` crate parses that field into a private `aes_mode` tuple and
exposes no accessor, but leaves the raw bytes readable. It returns `None` for an entry carrying no AES
field, and `None` for a truncated or malformed chain rather than guessing.

**The conjunct is load-bearing for the very AD 0012 seam this record's amendment (c) identified.**
`CRC32(b"") == 0`, so an *encrypted empty file* under ZipCrypto or AE-1 satisfies
`encrypted() && crc32() == 0` while its stored zero is a genuine checksum. A two-conjunct gate would
sweep such an entry in, discard a checksum the archive really recorded, list it `crc32: None`, and
force a pointless decrypt-and-stream during the digest walk. Reading the vendor version separates the
specification's placeholder from a real zero.
`test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32` pins that seam;
`test_zip_wrapper_aes_vendor_version_reads_ae1_and_ae2` pins the field reader and fixes that an AE-1
entry (vendor version 1) is **not** exempt and keeps its real stored CRC — which is stronger than the
earlier amendment's bare assertion that "AE-1 entries carry a real stored CRC and are unaffected".

A residual is stated in the shipped rustdoc rather than hidden: an AE-2 entry whose central record
omits the `0x9901` field is not recognised by the gate. Such an entry is unreadable anyway — the `zip`
crate rejects "AES encryption without AES extra data field" while parsing the central directory — so
it never reaches a CRC comparison.

Everything the earlier amendment *concludes* from that block stands: AE-2 entries list `None`,
`entry_crc32_for_digest` takes the streaming recompute path for them, the extract-side exemption is
now mirrored in the digest by sharing one predicate, a plaintext empty file still lists `Some(0)`, and
the ZIP bullet of amendment (c) is still false in both halves. Only the quoted body was incomplete.

### Item 3's cost shape is superseded on the libarchive side: the quadratic reopen is gone from the digest walk

Item 3 opens with a claim that stands — the quadratic term is libarchive-specific and does not extend
to ZIP — but grounds it on a premise that has moved: "The O(N²) reopen-and-re-decompress behaviour
belongs to `LibarchiveStreamReader::open`." That is still where the behaviour lives, but the digest no
longer goes there for libarchive-backed archives. OI-0001-009 / TicGit `82bf8fd4` landed:

* `src/backend.rs` gained `PayloadTarget` / `PayloadVisitor` and
  `ReadBackend::visit_payloads_by_listing_id`, whose trait default is `NotImplemented`.
* `src/ffi/libarchive_wrapper/reader.rs` overrides it with a single-traversal walk: one read handle
  under `ReadHandleGuard`, targets sorted by id, headers walked once in ascending order,
  non-targets `data_skip`ped, each target handed a `BorrowedEntryReader` over the shared handle. The
  R0079-0022 positional-index rule and the OI-0076-002 name drift guard are both preserved verbatim,
  and no size bound is applied inside the walk — DCR-011's exact/ceiling contract stays with the
  caller, which is the only party that knows whether the listing declared a size.
* `src/inspection.rs::resolve_crc32_single_pass` drives it, bailing back to the per-entry resolver on
  `NotImplemented` or on non-unique listing ids, and sharing `crc32_of_bounded_payload` with that
  resolver so DCR-011 means one thing on both routes.
* `src/streaming.rs` raised `HardCapReader` to `pub(crate)` with `exact` and `ceiling` constructors.

So **amendment (b)'s "the prefix is re-decompressed once per entry" no longer describes the digest
path** on `.tar.gz` / `.tar.bz2` / `.tar.xz`; it describes only the fallback route, which now runs
when a backend offers no one-pass walk or a listing presents non-unique ids.

What item 3 says about **ZIP is unchanged and is now the sharper half of it**: `ZipArchive` overrides
`extract_to_stream_by_listing_id` but *not* `visit_payloads_by_listing_id`, so AE-2 entries still
resolve one per call from the cached `with_zip` handle. That adds no quadratic reopen, and it still
adds the DEF-004 / OI-0057-007 buffered-adapter cost — each entry fully materialized in memory and
fully decrypted before the cursor is handed back, i.e. memory proportional to the largest entry and
CPU proportional to total encrypted bytes.

**The third open question is NOT resolved by this.** The single-pass walk removes the per-entry
re-decode; it does not bound N. The digest still hands `ExtractionLimits::unlimited()` to
`ValidatedSource::extract_to_stream_by_id` (R0076-0091), the byte bound is still per-entry only —
exact at the declared size since DCR-011, ceiling-only at `DEFAULT_MAX_FILE_SIZE` when the listing
declares none — and the listing walk still passes `budget = None`. *Whether the CRC-less walk should
carry a budget or a documented ceiling* therefore stays **open for the owner**, as does the SHA-256
question. Amendment (a) stands verbatim: the digest is still CRC32 end to end, integrity-grade only,
and `crc32fast` is still the crate's only hash dependency.

### Items 1, 4 and 5 stand

Item 1 stands: digesting an AE-2 ZIP requires the password, and the precondition is still undocumented
in the rustdoc and `docs/API_REFERENCE.md`. It is now pinned at the facade by
`ae2_digest_without_password_is_an_error_not_a_placeholder_digest`, which deliberately asserts only
that the call errors, leaving the `Format`-versus-`Password` mislabel owned by TicGit `9bdf2c`
untouched and unendorsed. Item 4 stands verbatim. Item 5 stands: `Cargo.toml` still reads
`version = "0.3.1"`, the owner has not authorised DCR-012's prescribed 0.3.1 → 0.4.0 bump, and none of
this has shipped in a released version.

### Gate evidence for this amendment

Measured on 2026-08-17 against the uncommitted tree, by running the commands rather than quoting an
earlier report: `cargo build --all-features --all-targets` at exit 0 with **zero rustc warnings** (the
`warning:` lines it emits are all `unified-archive@0.3.1:`-prefixed build-script output from the
vendored unrar C++ sources), and `cargo test --workspace --all-features --no-fail-fast --
--test-threads=4` at exit 0 with **1585 passed / 0 failed / 5 ignored across 36 test binaries**.

The claim that the quadratic reopen is gone has its own empirical sentinel, and it now runs in the
default lane: `integration::manifest_digest_perf::manifest_digest_does_not_reopen_archive_per_entry`
passes, its `#[ignore]` lifted, its module doc recording a measured 2.6× ratio (639 µs listing,
1.65 ms digest, debug build, Apple silicon) against a 50× bound where the pre-refactor quadratic walk
produced ~150× on a 1000-entry `tar.gz`.

Take the two counts as a timestamped observation, not a constant: an earlier run in the same session
saw three fewer lib unit tests and two integration failures that were shared-scratch-directory races
on `/Volumes/Temp/claude` between concurrent suite runs, not defects in anything described here. The
tree is being edited by several agents at once, which is precisely the condition that produced the
pass-2 amendment this one corrects.
