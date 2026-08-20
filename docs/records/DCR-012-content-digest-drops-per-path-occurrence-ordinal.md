---
type: DCR
title: "Content-multiset digest drops the per-path occurrence ordinal"
description: "calculate_content_multiset_digest_and_size encodes one bare 8-hex element per file entry; duplicate-path grouping and same-path listing order no longer participate. Overturns ticgit 2a6e3153's encoding clause; its id-based streaming stands. Amended 2026-08-17: the AE-2 ZIP listing change (TicGit 04ba4897) shipped in the same change set, so Known Risks item 2 and the fail-closed trace are superseded. Amended again 2026-08-17 (pass 3): the ZIP id-stream forward IS wired in src/backend.rs and pinned at the facade by tests/integration/digest_ae2_duplicate_path.rs, OI-0001-009 landed as visit_payloads_by_listing_id, and the AE-2 exemption gate has a third conjunct (the 0x9901 vendor version) - only the prescribed 0.3.1 to 0.4.0 bump stays deferred to the owner."
tags: [change, project-control, DCR-012]
status: active
---

# DCR-012: Content-multiset digest drops the per-path occurrence ordinal

- **Date:** 2026-08-16
- **Source:** Review 0001, R0001-0050 / OI-0001-008; TicGit `61426550`
- **Overturns:** TicGit `2a6e3153` (closed `resolved` 2026-07-19) — its encoding clause only
- **Related:** AD-0047 (content-based identity on CRC-less formats), DCR-011 (declared-size
  exactness), R0079-0028 (the ordinal's introduction), OI-0001-009 / TicGit `82bf8fd4`
  (single-traversal digest rewrite — land after this)

## What Changed

`content_digest_element` lost its `ordinal` parameter. Every file entry now contributes exactly
eight lowercase zero-padded hex characters — first occurrence or twentieth, unique path or shadowed
duplicate:

```rust
fn content_digest_element(crc32: u32) -> String {
    format!("{crc32:08x}")
}
```

In `content_multiset_digest_and_size` the `path_occurrences: HashMap<&str, usize>` and its
`.entry(..).and_modify(..).or_insert(0)` block are deleted, together with the now-unused `HashMap`
import. Multiplicity is carried by *repetition* in the element vector, so `n` copies of one payload
still contribute `n` elements. Everything else is byte-identical: the `EntryType::File` filter, the
R0081-0081 `checked_add` total, the CRC resolution through `entry_crc32_for_digest`, the empty-string
sentinel for a listing with no file entries, and the `sort` → `join(",")` → `crc32fast` fold.

DCR-011's exact declared-size bound is untouched by construction: it fires inside the resolver,
before any element exists.

## Overturning `2a6e3153`, named

TicGit `2a6e3153` — *"Restore manifest digests for duplicate-path CRC-less archives (id-based digest
streaming)"* — stated as part of its goal:

> The R0079-0028 per-path occurrence ordinal in the digest encoding then reflects genuinely distinct
> payloads.

That is a **consequence clause**: it describes what becomes true of the ordinal *once* the resolver
is fixed. It is not an independently derived requirement that the encoding must carry path grouping,
and none of the ticket's four acceptance criteria (an id-based streaming path exists; the digest
succeeds on a duplicate-path tar and hashes both payloads distinctly; a regression test; a doc note)
mentions the ordinal's keying at all.

- **Overturned:** that the element encoding shall carry a per-path occurrence ordinal.
- **Stands, reaffirmed, and now load-bearing:** that each CRC-less occurrence is resolved by its
  **stable listing id** via `extract_to_stream_by_id` / `extract_to_stream_by_listing_id`. After
  this change nothing in the *encoding* distinguishes shadowed duplicates, so id-keyed resolution is
  the sole mechanism that does.

## Why the ordinal's premise lapsed

R0079-0028 introduced the ordinal while the CRC-less resolver walked **by path** and aliased: every
duplicate occurrence re-hashed the first occurrence's payload, so `{member(first), member(second)}`
collapsed to `{X, X}` and would have false-equalled a genuine double copy. The ordinal was a
compensating control over a defective resolver.

`2a6e3153` removed the defect. The compensator outlived it by one ticket. The written evidence sat in
the tree until this change: the unit test `test_content_digest_duplicate_path_differs_from_unique_paths`
still justified itself with

> the constant resolver models the CRC-less streaming path, where every occurrence of a duplicated
> path resolves to the first occurrence's payload

— a sentence that has been **false since 2026-07-19**. With the resolver fixed, the only thing the
ordinal discriminates is *path grouping*, which is layout.

## The property table

`X` and `Y` are distinct payload CRC32s; `m` is one repeated path.

| # | Two listings | Contract says | Before | After |
|---|---|---|---|---|
| R1 | `[a:X]` vs `[a:X]` | equal | equal | equal |
| R2 | `[a:X, b:Y]` vs `[p:X, q:Y]` (renamed) | equal | equal | equal |
| R3 | `[a:X, b:Y]` vs `[b:Y, a:X]` (reordered, unique paths) | equal | equal | equal |
| R4 | `[a:X]` vs `[a:X, b:X]` (multiplicity) | differ | differ | differ |
| R5 | `[a:X, b:X]` vs `[a:X, b:Y]` | differ | differ | differ |
| R6 | truncated CRC-less member | `Corruption` (DCR-011) | `Corruption` | `Corruption` |
| R7 | `[]` (no file entries) | `""` | `""` | `""` |
| R8 | **`[m:X, m:X]` vs `[a:X, b:X]`** (grouped vs spread) | equal | **differ** | equal |
| R9 | **`[m:X, m:Y]` vs `[m:Y, m:X]`** (one archive, re-listed) | equal | **differ** | equal |
| R10 | **duplicate-path tar vs repacked flat zip, same contents** | equal | **differ** | equal |

The old encoding failed exactly R8, R9 and R10. R9 is a second contract violation the OI did not
name: *one and the same archive* digested differently when re-listed in a different order, because
ordinal 0 went to whichever occurrence the backend happened to enumerate first. No reading defends
that as intentional.

Worked values, computed against the real algorithm: with `X = 0xdeadbeef`, both the grouped listing
`[m:X, m:X]` and the spread listing `[a:X, b:X]` now join to `"deadbeef,deadbeef"` and digest
`e212a99e`. Before this change the grouped listing joined to `"deadbeef,deadbeef#1"` and digested
`ee663b6e` — the OI's false negative, reproduced by value.

## The documented-contract gap decided it

`docs/API_REFERENCE.md` §`calculate_manifest_digest` already reads, and has always read:

> **Contract:** the digest is a *content-multiset* hash — archives with the same per-entry content
> produce the same digest regardless of filenames, directory layout, modification times,
> permissions, **or entry order**.

and its Algorithm step 2 is simply *"Convert each CRC32 to 8-char hex"*. The crate's canonical public
reference therefore **already documents exactly the encoding this change installs**, and has never
mentioned the ordinal. Dropping the ordinal makes that document true with zero edits to it.

Keeping the ordinal would have required weakening that Contract paragraph and rewriting step 2, plus
amending the rustdoc on three methods and `docs/STREAM_CRC32.md` — enshrining **two** misbehaviours
(path-grouping dependence and same-path listing-order dependence) across four documents, on behalf of
a property no record argues for and no consumer requested. The honest amended contract would have had
to say the digest is a function of *(content multiset, duplicate-path grouping, relative listing order
of same-path occurrences)* — three components, two of which are not content — in an API whose headline
says layout is ignored.

## Consumer asymmetry

The rustdoc names AdvancedDeduplicator as the consumer, for content-identity matching. Its error modes
are asymmetric: a false negative costs storage (a duplicate is not collapsed); a false positive costs
data. Today's behaviour is the **false negative**, in the exact shape a deduplicator meets in the wild
(`tar -rf` / `-uf` appends). The ordinal bought no false-positive protection worth the name: it fired
only where duplicate paths exist, while the far more common layout difference — the same files under
different names — has collided by design since AD-0047.

## Observable Behaviour Change

- **Unchanged, structurally:** every archive in which each path occurs at most once. Ordinal 0
  already emitted the bare hex, so those digests are byte-identical. Two existing tests are the
  compat proof and stay green **unmodified**:
  `test_content_digest_unique_paths_match_prefix_algorithm`, which recomputes the expected value
  from the pre-R0079-0028 algorithm, and `HEALTHY_TAR_DIGEST = "f9413c0f"` in
  `tests/digest_exactness_test.rs`, which pins `tests/fixtures/test.tar` (one member,
  `test_file.txt`, unique path).
- **Changed:** exactly the archives with two or more `EntryType::File` entries sharing one path. No
  fix for this defect can change fewer — those are precisely the archives whose digest was wrong.
- On CRC-carrying formats a duplicate-path archive's digest **reverts** to its pre-R0079-0028 value,
  so values stored before that change become valid again.
- On CRC-less formats the affected stored population is bounded to values computed after
  2026-07-19: before `2a6e3153` those archives errored (*"Multiple entries match"*) rather than
  digesting at all.
- Version handling: a behavioural break in a documented output → minor bump 0.3.1 → 0.4.0 with a
  BREAKING `CHANGELOG.md` entry naming the class ("duplicate-path archives only; unique-path digests
  are byte-identical and need no recomputation"). Deliberately **no** schema marker or prefix on the
  digest: an 8-hex output has no in-band room and any tag would change *every* archive's digest,
  converting a narrow break into a universal one.

## Why `2a6e3153`'s regression test is not wrong and is not replaced

`duplicate_path_tar_digests_both_payloads_distinctly` keeps passing with no edits, and the ordinal was
never what made it pass. By value: `crc32("first-payload") = bfd59004`,
`crc32("second-payload") = bb1b7c95`. The duplicate-path archive joins `"bb1b7c95,bfd59004"` and
digests `837083ae`; the control, whose two occurrences share one payload, joins `"bfd59004,bfd59004"`
and digests `8d15253a`. Different content multisets, so the `assert_ne!` holds with no ordinal in
play. That test is now the **sole sentinel** against a revert to by-path resolution and must never be
deleted.

## Why the deleted unit test's assertion was wrong

`test_content_digest_duplicate_path_differs_from_unique_paths` asserted the defect, on a premise its
own doc comment names and that `2a6e3153` falsified (see *Why the ordinal's premise lapsed*). Against
an id-keyed resolver, a constant CRC no longer models an aliasing bug; it models a genuine `{X, X}`
content multiset, which under a content-identity contract must equal the spread listing's. It is
replaced by `test_content_digest_grouped_and_spread_duplicates_agree`, which asserts equality. If a
reviewer rejects this paragraph, the whole change falls — which is the correct dependency.

## Fail-closed trace of the non-libarchive fallback

Dropping the ordinal could in principle have opened a silent *false positive* on the one path where
occurrences are not id-keyed. It does not. `entry_crc32_for_digest`'s `NotImplemented` arm falls back
to `ValidatedSource::extract_to_stream(&entry.path)`, whose backend implementations call
`security::validate_single_entry`, which rejects duplicate paths (*"Multiple entries match"*). A
CRC-less duplicate-path entry on a backend without an id seek therefore **errors** rather than
silently re-hashing the first occurrence. This is recorded rather than tested: a 7z entry with
`has_crc == false` and duplicate names is not practically synthesizable, and a vacuous test would be
worse than the trace.

## Rejected Alternatives

- **Keep path-keyed ordinals and amend the rustdoc** (the OI's other legitimate outcome) — rejected:
  see *The documented-contract gap decided it*. The listing-order component (R9) is indefensible
  under any reading.
- **Content-keyed ordinal** (`{crc}#k` for the k-th occurrence of the same CRC — the OI's Required
  Action 1 sketch) — rejected as information-equivalent to bare repetition (`m` elements
  `c#0…c#(m-1)` determine and are determined by multiplicity `m` exactly as `m` bare elements are),
  while changing the digest of every archive containing any repeated CRC32 under distinct paths — two
  empty files, duplicated licence headers, vendored copies. Identical semantics, far larger blast
  radius. Strictly dominated.
- **XOR or wrapping-sum fold** (order-independence without a sort) — rejected for catastrophic *new*
  collisions: XOR makes `{X, X} ≡ {}`. That is `calculate_archive_crc`'s documented weakness.
- **Run-length element `{crc}x{n}`** — injective, but moves every digest containing repeated content
  and adds a second alphabet character for nothing.
- **Widening the element to `{crc}:{size}`** — changes every digest, re-couples the digest to
  declared size (metadata on some backends), and belongs with the SHA-256 question AD-0047 left open
  to the owner. Do not bundle.
- **Deduplicating the element vector ("it's a set now")** — flatly wrong: it would make a two-copy
  archive equal a one-copy archive. Pinned against by `test_content_digest_multiplicity_is_significant`.
- **Tagging the digest with a schema marker** — see *Observable Behaviour Change*.

## Injectivity, stated because a future reader will otherwise "simplify" it away

Elements are fixed-width over `[0-9a-f]`; the separator `,` is outside that alphabet; therefore
`join(",")` is uniquely decodable and `sort()` canonicalises. The joined string is a bijection with
the sorted CRC32 multiset. Removing `#` in fact *strengthens* this — all elements become fixed-width,
so no prefix ambiguity is even expressible. The only residual collisions are the final 32-bit fold's,
already recorded in AD-0047 amendment (a).

## Known Risks

1. **Nothing in the encoding distinguishes shadowed duplicates any more.** Correctness of the
   `{X, Y} ≠ {X, X}` row rests *entirely* on `extract_to_stream_by_id`. A future "cleanup" back to
   by-path resolution would produce a silent **false positive** — the mirror of today's false
   negative. `duplicate_path_tar_digests_both_payloads_distinctly` is the sentinel; the rustdoc on
   `entry_crc32_for_digest` now says so.
2. **AE-2 ZIP degeneracy widens.** Every entry lists `crc32 = Some(0)` by spec (AD-0047 amendment
   (c)), so the digest there is already content-blind; it degrades from "a function of the file count
   and the duplicate-path grouping" to "a function of the file count". Layout-only loss on an already
   broken surface. The real fix is AD-0047's still-open owner question — treat a zero CRC on a
   `crc32_check_exempt` entry as absent and stream — and is not bundled here.
3. **Listing versus materialisation.** The digest is defined over the archive's *listing* multiset. A
   `tar -rf`-appended duplicate participates although extraction materialises only the surviving
   occurrence, and `total_size` likewise sums both. This is **pre-existing and orthogonal**: the
   ordinal never made such an archive equal the one-file archive it actually unpacks to; it only made
   it differ from a two-path archive. So the ordinal cannot be defended as a mitigation for this gap.
   The rustdoc now states the listing-multiset definition explicitly. A materialised-view identity,
   if ever wanted, is a separate method — not a contaminated content digest.
4. **The out-of-tree AdvancedDeduplicator store cannot be inspected from this repository.** A
   cross-repo check before landing 0.4.0 is cheap and should be done.

## Scope and Sequencing

Land **before** OI-0001-009 (TicGit `82bf8fd4`, the single-traversal rewrite). Removing the ordinal
removes the walk's dependence on same-path listing order, so a single-pass traversal is then free to
resolve CRCs in whatever order it reaches them. If `82bf8fd4` lands first it bakes in a grouping
constraint and may pin old digest values in its own regressions. That ticket gains an acceptance
criterion requiring the rewrite to preserve the ordinal-free encoding and unconditional
order-independence — mirroring how DCR-011 added its `with_exact_size` criterion to the same ticket.

Closes `61426550`.

## Amendment (2026-08-17, pass-2 record reconciliation — the AE-2 ZIP change shipped *with* this DCR, falsifying two of its claims)

Nothing above is rewritten; the body stands as authored on 2026-08-16. This amendment corrects two
statements that the change set this record shipped in made false, records a new precondition on the
public digest API, and states plainly that the prescribed version bump has not been performed.

Everything below was verified against the working tree of branch `001-unified-archive` on
2026-08-17, where all of it is still **uncommitted**:

* `src/ffi/zip_wrapper.rs`, listing walk —
  `if entry.entry_type == EntryType::File && !crc32_check_exempt(&zip_file) { entry.crc32 = Some(zip_file.crc32()); }`,
  with `fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool { zip_file.encrypted() && zip_file.crc32() == 0 }`.
* `src/inspection.rs` — `fn content_digest_element(crc32: u32) -> String { format!("{crc32:08x}") }`,
  no `path_occurrences` map, multiplicity carried by repetition. The encoding this record installs is
  in the tree exactly as written.

### 1. Known Risks item 2 is superseded — the AE-2 fix *is* bundled here

Item 2 states, in the present tense, that on an AE-2 ZIP "every entry lists `crc32 = Some(0)` by spec
(AD-0047 amendment (c))", that the digest there "is already content-blind", and that the real fix —
"treat a zero CRC on a `crc32_check_exempt` entry as absent and stream" — is "AD-0047's still-open
owner question" and "is not bundled here".

That fix landed in the same uncommitted change set as this record, as TicGit `04ba4897`
(R0079-0007). AE-2 entries now list `None`, so the digest walk streams their decrypted payloads and
folds real per-payload CRC32s. The degeneracy item 2 describes — and the AD-0047 open question it
defers to — no longer exist; see AD-0047's own 2026-08-17 amendment, which resolves question (c) as
DECIDED-AND-IMPLEMENTED.

The gate is not the value alone: it is `encrypted() && crc == 0`, so AD 0012 survives intact and a
plaintext empty file still lists `Some(0)`. Note also that the ZIP unit tests cite **this record** as
their authority — `test_zip_wrapper_plaintext_empty_file_still_lists_crc32_zero`,
`test_zip_wrapper_extract_to_stream_by_listing_id_ae2`,
`test_zip_wrapper_ae2_stream_by_id_without_password_errors` — so DCR-012 is load-bearing for the AE-2
gate whether or not its body intended to be.

### 2. The "Fail-closed trace of the non-libarchive fallback" section's justification is falsified, and that untested arm is now a live failure path

That section declined to test the `NotImplemented` arm on the grounds that a CRC-less duplicate-path
entry on a backend without an id seek is "not practically synthesizable, and a vacuous test would be
worse than the trace". The AE-2 change makes it **trivially** synthesizable: any AE-2 AES ZIP whose
central directory holds two names that normalize to one path (`sub/a.txt` and `sub\a.txt`,
R0076-0048) now presents exactly that shape.

Worse, the arm is not merely reachable — it fires, because
`ZipArchive::extract_to_stream_by_listing_id` was added as an **inherent** method on `ZipArchive` and
was never wired into `impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive` in `src/backend.rs`.
Only the libarchive impl overrides the trait default. Confirmed empirically here:
`cargo check --all-features` emits ``warning: method `extract_to_stream_by_listing_id` is never
used`` against `src/ffi/zip_wrapper.rs`. The chain is:

`entry.crc32 == None` → `entry_crc32_for_digest` → `ValidatedSource::extract_to_stream_by_id` →
`ReadBackend::extract_to_stream_by_listing_id` → **trait default** → `NotImplemented` → this
section's fallback `extract_to_stream(&entry.path)` → `ZipArchive::extract_to_memory_capped` →
`security::validate_single_entry` + `Self::reject_if_duplicate`.

A pass-1 reviewer reproduced the end state: on a duplicate-after-normalization AE-2 ZIP,
`calculate_content_multiset_digest_and_size()` returns
`Err(OperationBlocked { operation: "extract_to_stream", reason: "Multiple entries match ..." })`
where before the AE-2 change it returned `Ok`. That is OI-0076-002 re-opened on the ZIP backend.

Two corrections follow, and only the second is a correction of substance:

* The trace's **conclusion** still holds. The path fails closed: it errors rather than silently
  re-hashing the first occurrence, so no false positive is introduced. The digest's correctness
  argument is unharmed.
* The trace's **premise** — that the arm is unreachable in practice and therefore not worth a test —
  is false as of the same change set, and the missing test is exactly the one that would have caught
  this. Note too that `extract_to_stream_by_listing_id`'s own rustdoc in `src/ffi/zip_wrapper.rs`
  claims the by-path gate is a thing this method *avoids*; it avoids nothing while no caller reaches
  it.

**To discharge:** add the one-line override to `impl ReadBackend for ZipArchive` in `src/backend.rs`,
mirroring the libarchive one, and add a regression test at the
`calculate_content_multiset_digest_and_size` level over a duplicate-after-normalization AE-2 ZIP.
`src/backend.rs` fell outside every pass-1 agent's file ownership, which is why the gap survived
review; it is outside this amendment's ownership too, so this is reported, not fixed.

### 3. New, undocumented precondition: digesting an AE-2 ZIP now requires the password

Because AE-2 entries list `crc32 = None`, the digest must stream their payloads, and streaming
requires decryption. `calculate_content_multiset_digest_and_size` on a password-protected AES ZIP
opened **without** a password now returns `Err` where it previously returned `Ok`. The old `Ok` was
the defect — a content-blind fold over constant `00000000` placeholders — so the new error is the
correct outcome, but the precondition is new, is absent from this record's *Observable Behaviour
Change* section, and belongs in the public rustdoc and `docs/API_REFERENCE.md` contract for the
digest methods. `test_zip_wrapper_ae2_stream_by_id_without_password_errors` pins only the
wrapper-level `is_err()`, not the facade-level behaviour.

The error arrives classified `ArchiveError::Format { format: Some(Zip), message: "... Password
required to decrypt file" }` rather than `ArchiveError::Password`. That mislabel is **pre-existing**
on the ZIP no-password read path and is owned by TicGit `9bdf2c`; it is neither fixed nor endorsed
here.

With the password supplied the digest is genuinely content-sensitive again — which is what the gate
guarantees structurally, since real per-payload CRC32s replace the constant placeholder. The pass-1
review probe reported two different two-entry AE-2 ZIPs digesting `456d4668` and `170fa9d4`; those
are quoted as reported and were not recomputed for this amendment.

### 4. The prescribed 0.3.1 → 0.4.0 bump has NOT been performed

*Observable Behaviour Change* prescribes "minor bump 0.3.1 → 0.4.0 with a BREAKING `CHANGELOG.md`
entry". `Cargo.toml` still reads `version = "0.3.1"`. The owner has not authorised a version bump, so
the entire change set sits unreleased and both the version stamp and the CHANGELOG entry are
**deferred to the owner**. Treat that prescription as OPEN, not discharged. Consequently the
`content_digest_element` rustdoc heading "**Changed in 0.4.0 (DCR-012).**" names the version this is
intended to ship in, not one that exists.

### 5. Sequencing: OI-0001-009 / TicGit `82bf8fd4` did not land alongside this change

*Scope and Sequencing* requires this change to land **before** the single-traversal rewrite. That
ordering is satisfied — but only because the rewrite did not land at all. Its enabling API sits
outside the file ownership of the agent that attempted it: `PayloadTarget` and
`ReadBackend::visit_payloads_by_listing_id` in `src/backend.rs`, the shared-handle walk and a
borrowed entry reader in `src/ffi/libarchive_wrapper/reader.rs`, and the `HardCapReader`
exact/ceiling constructors (plus raising `HardCapReader` to `pub(crate)`) in `src/streaming.rs`.
Nothing of it is half-landed; the tree is coherent, and the shipped rustdoc deliberately describes
the quadratic behaviour and states that OI-0001-009 has not landed. The acceptance criterion this
section adds to `82bf8fd4` — preserve the ordinal-free encoding and unconditional order-independence
— remains outstanding.

## Amendment (2026-08-17, pass-3 record reconciliation — sections 2 and 5 of the pass-2 amendment are superseded by the tree; the gate quoted above is one conjunct short)

Nothing above is rewritten. The 2026-08-16 body stands as authored, and so does the pass-2 amendment
appended earlier today — including the two sections this one supersedes. They are left intact because
they are an accurate snapshot of the tree *at the moment they were written*, and because the reason
they are wrong is itself worth preserving.

**Why the earlier amendment reports a tree that no longer exists.** Pass 2 ran its code agent and its
documentation agents concurrently. The documentation agents observed, reproduced and truthfully
recorded a defect that the code agent was closing in the same wall-clock window. Nothing in section 2
or section 5 was careless; both cite real reproductions. Both were simply overtaken. Read them as a
snapshot of a mid-flight tree, never as a claim about the shipped one — and note the general lesson,
which is that a record's verification stamp is only as strong as the exclusion between the agent
writing it and the agents editing the code it describes.

Everything below was re-verified directly against the working tree of branch `001-unified-archive` on
2026-08-17, where it remains **uncommitted**. Values quoted as digests were recomputed here rather
than copied from any earlier report.

### A. Section 2 is superseded: the ZIP `ReadBackend` forward IS wired

Section 2 states that `ZipArchive::extract_to_stream_by_listing_id` "was never wired into
`impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive` in `src/backend.rs`", that "Only the
libarchive impl overrides the trait default", that `cargo check --all-features` emits
``warning: method `extract_to_stream_by_listing_id` is never used``, and — in its *To discharge*
paragraph — that the fix "is outside this amendment's ownership too, so this is reported, not fixed".

It was fixed. `impl ReadBackend for crate::ffi::zip_wrapper::ZipArchive` in `src/backend.rs` now
carries the forward, under a rustdoc that names this record and the defect it closes:

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

Consequences, each checked:

* The dispatch chain section 2 traces no longer ends at the trait default. `entry_crc32_for_digest` →
  `ValidatedSource::extract_to_stream_by_id` → `ReadBackend::extract_to_stream_by_listing_id` now
  reaches `ZipArchive`'s own inherent seek, so it never falls back to `extract_to_stream(&entry.path)`
  and never meets `security::validate_single_entry` / `reject_if_duplicate`.
* The "never used" warning is gone: `cargo build --all-features --all-targets` completes with **zero**
  rustc warnings.
* The pass-1 reproduction — `calculate_content_multiset_digest_and_size()` returning
  `Err(OperationBlocked { operation: "extract_to_stream", reason: "Multiple entries match ..." })` on
  a duplicate-after-normalization AE-2 ZIP — does not reproduce. That archive digests.

Section 2's two *corrections of substance* survive the fix and are worth restating, because they are
what motivated it: the trace's conclusion (the path failed **closed**, so no false positive was ever
introduced) still holds, and the trace's premise (that the `NotImplemented` arm is unreachable in
practice and therefore not worth a test) was indeed falsified by the AE-2 change. The remedy for the
falsified premise is section B.

### B. The missing regression test exists, at the facade, and its value is reproduced here

Section 2 prescribed "a regression test at the `calculate_content_multiset_digest_and_size` level over
a duplicate-after-normalization AE-2 ZIP". `tests/integration/digest_ae2_duplicate_path.rs` is that
test, and it holds three cases, all asserting through `Archive` rather than through the wrapper — a
deliberate choice, since an inherent-method test cannot detect a missing `ReadBackend` forward at all:

* `ae2_duplicate_after_normalization_zip_digests_each_payload` — the headline case.
* `ae2_duplicate_path_digest_tracks_the_shadowed_payload` — two archives differing only in the
  shadowed occurrence's payload must digest differently.
* `ae2_digest_without_password_is_an_error_not_a_placeholder_digest` — which closes the gap section 3
  named, that `test_zip_wrapper_ae2_stream_by_id_without_password_errors` pinned only the
  wrapper-level `is_err()` and nothing at the facade. It deliberately does not assert the error
  *variant*, leaving the `Format`-versus-`Password` mislabel owned by TicGit `9bdf2c` exactly where
  section 3 left it.

The fixture builds one AE-2 AES ZIP whose central directory holds the raw names `sub/a.txt` and
`sub\a.txt` — two records that normalize to the single path `sub/a.txt` (R0076-0048) — with payloads
`b"alpha"` and `b"bravo"`. The test first asserts the fixture really has that shape: `list_files`
returns exactly two entries at `sub/a.txt`, both `is_encrypted`, both listing `crc32 == None`. Without
those guards the digest assertions could pass without touching the dispatch under test.

Recomputed for this amendment rather than quoted: `crc32("alpha") = d0e0396a`,
`crc32("bravo") = 099bb889`; sorted and joined the elements read `099bb889,d0e0396a`; the CRC32 of
that join is **`6a4fd559`**, and the declared total is **10** bytes. The facade returns exactly
`Ok(("6a4fd559", 10))`. The load-bearing assertion is the `assert_ne!` beside it, against
`expected_digest([first, first])`: a resolver that re-read the first occurrence for both ids would
produce that other value, so the equality alone would not prove non-aliasing and the test does not
rest on it.

Section 2's *To discharge* paragraph is therefore **discharged in full** — override and test both.

### C. The gate has a THIRD conjunct, and it is load-bearing

The pass-2 amendment's verification preamble quotes the shipped predicate as

> `fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool { zip_file.encrypted() && zip_file.crc32() == 0 }`

and section 1 restates it as "it is `encrypted() && crc == 0`". Both are one conjunct short of the
tree. `src/ffi/zip_wrapper.rs` reads:

```rust
fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool {
    zip_file.encrypted()
        && zip_file.crc32() == 0
        && aes_vendor_version(zip_file) == Some(AES_VENDOR_VERSION_AE2)
}
```

with `AES_EXTRA_FIELD_ID = 0x9901` and `AES_VENDOR_VERSION_AE2 = 0x0002`, and `aes_vendor_version`
walking the entry's extra-field chain (`id:u16 | len:u16 | body[len]`, little-endian) out of
`ZipFile::extra_data()` — the `zip` crate parses the WinZip-AES field into a private `aes_mode` tuple
and exposes no accessor, but leaves the raw bytes in place. It returns `None` for an entry with no AES
field and `None` for a truncated or malformed chain rather than guessing.

**Why the third conjunct is not decoration.** A two-conjunct gate sweeps in any *encrypted empty
file*, because `CRC32(b"") == 0`: a ZipCrypto or AE-1 entry with no bytes satisfies
`encrypted() && crc32() == 0` while carrying a stored CRC32 that is a genuine checksum. Exempting it
would discard a checksum the archive really recorded and — post-DCR-012 — would list it `crc32: None`
and force a pointless decrypt-and-stream during the digest walk. The vendor-version read is what
distinguishes the specification's placeholder from a real zero, so the predicate now means what its
name says. This is precisely the AD 0012 seam ("Treat CRC32 value zero as valid, not absent"), and it
is pinned by `test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32`; the field reader
itself is pinned by `test_zip_wrapper_aes_vendor_version_reads_ae1_and_ae2`, which also fixes that an
AE-1 entry (vendor version 1) is **not** exempt and keeps its real stored CRC.

One residual is stated in the shipped rustdoc rather than hidden: an AE-2 entry whose central record
omits the `0x9901` field is not recognised by this gate. Such an entry is unreadable anyway — the
`zip` crate rejects "AES encryption without AES extra data field" while parsing the central directory
— so it cannot reach a CRC comparison in the first place.

Nothing section 1 *concludes* changes: AD 0012 survives intact, a plaintext empty file still lists
`Some(0)`, and the AE-2 fix is still bundled into this record's change set as TicGit `04ba4897`. Only
the conjunct count was understated, and it is corrected here because AD-0047's own 2026-08-17
amendment quotes the same two-conjunct body as a *verified code block*.

### D. Section 5 is superseded: OI-0001-009 / TicGit `82bf8fd4` DID land

Section 5 states that the single-traversal rewrite "did not land at all", that "Nothing of it is
half-landed", and that "the shipped rustdoc deliberately describes the quadratic behaviour and states
that OI-0001-009 has not landed". All three are false against the tree. The rewrite landed whole, in
the four places section 5 correctly predicted it would have to touch:

* `src/backend.rs` — `pub(crate) struct PayloadTarget<'a>` (`id`, `validated_path`, `declared_size`),
  `pub(crate) type PayloadVisitor<'v> = dyn FnMut(&PayloadTarget<'_>, &mut dyn std::io::Read) -> Result<()> + 'v`,
  and `ReadBackend::visit_payloads_by_listing_id` with a `NotImplemented` trait default that the
  libarchive impl overrides.
* `src/ffi/libarchive_wrapper/reader.rs` — `LibarchiveArchive::visit_payloads_by_listing_id` opens
  **one** read handle under `ReadHandleGuard`, sorts the targets by id, walks the headers once in
  ascending order, `data_skip`s past non-targets, and hands each target a `BorrowedEntryReader` over
  the shared handle. The R0079-0022 index rule is preserved verbatim (an entry with a NULL pointer or
  NULL pathname is skipped *without* consuming an index), so `id` keeps meaning what
  `list_files_metadata_only` makes it mean, and the OI-0076-002 drift guard still refuses a header
  whose name disagrees with the listing name resolved for that id. No size bound is applied inside the
  walk — DCR-011's exact/ceiling contract stays with the caller, which is the only party that knows
  whether the listing declared a size at all.
* `src/streaming.rs` — `HardCapReader` is `pub(crate)` and offers `exact(inner, declared)` and
  `ceiling(inner, cap)`.
* `src/inspection.rs` — `resolve_crc32_single_pass` collects targets from
  `entry_type == EntryType::File && crc32.is_none()`, abandons the one-pass route entirely if the ids
  are not distinct (rather than letting one map slot absorb two payloads), dispatches through
  `visit_payloads_by_listing_id`, and treats `NotImplemented` as "no one-pass walk on this backend" by
  returning an empty map so the per-entry resolver produces the identical digest more slowly. Both
  routes call the same `crc32_of_bounded_payload` helper, so DCR-011 cannot mean two different things
  on the two paths.

The rustdoc claim is inverted too: `src/inspection.rs` now reads "Since OI-0001-009 (ticgit
`82bf8fd4`) the libarchive backend resolves all of them in a single traversal", in both the
`calculate_manifest_digest` and `calculate_content_multiset_digest_and_size` Performance sections.

**The acceptance criterion this record's *Scope and Sequencing* added to `82bf8fd4` is met**, and by
construction rather than by luck: the one-pass walk resolves per-id CRC32s into a
`HashMap<usize, u32>` and never touches the encoding, so `content_digest_element` is still
`format!("{crc32:08x}")`, multiplicity is still carried by repetition, and order-independence is still
unconditional. The prescribed ordering also held — DCR-012 landed first, in the same change set, so
the rewrite baked in no grouping constraint and pinned no pre-DCR-012 digest values in its own
regressions.

**Cost.** On the libarchive backend the digest walk no longer reopens the archive per CRC-less entry,
so a compressed TAR no longer re-decompresses its whole prefix once per member: the quadratic term
this record's *Known Risks* and AD-0047's amendment (b) both describe is gone from that path. It
survives only on the fallback route, which now runs when a backend offers no one-pass walk or when a
listing presents non-unique ids. What is **not** fixed: N is still bounded only by the archive — the
digest still hands `ExtractionLimits::unlimited()` down (R0076-0091) — so AD-0047's third open
question, whether the CRC-less walk should carry a budget or a documented ceiling, stays **open**.

### E. What still stands from the earlier amendment

* **Section 1** stands, subject to section C's conjunct correction.
* **Section 3** stands unchanged: digesting an AE-2 ZIP genuinely requires the password, the new `Err`
  is the correct outcome, and the precondition is still absent from the public rustdoc and
  `docs/API_REFERENCE.md`. It is now pinned at the facade by
  `ae2_digest_without_password_is_an_error_not_a_placeholder_digest`. The two AE-2 digest values
  section 3 quotes as reported (`456d4668`, `170fa9d4`) were not recomputed then and are not
  recomputed now; `6a4fd559` in section B above is the one AE-2 value this record vouches for.
* **Section 4** stands unchanged and remains **OPEN**: `Cargo.toml` still reads `version = "0.3.1"`,
  the owner has not authorised the prescribed 0.3.1 → 0.4.0 minor bump, and the
  `content_digest_element` rustdoc heading "**Changed in 0.4.0 (DCR-012).**" still names a version
  that does not exist.

### F. Gate evidence for this amendment

Measured on 2026-08-17 against the uncommitted tree, by running the commands rather than by quoting
an earlier report:

* `cargo build --all-features --all-targets` — exit 0, **zero rustc warnings** and no "never used"
  diagnostic. (The nine `warning:` lines it does emit are all `unified-archive@0.3.1:`-prefixed
  build-script output from the vendored unrar C++ sources, one `-Wnontrivial-memcall` in
  `unpack50mt.cpp`. That is pre-existing and unrelated.)
* `cargo test --workspace --all-features --no-fail-fast -- --test-threads=4` — exit 0, **1585
  passed / 0 failed / 5 ignored across 36 test binaries**.

The empirical sentinel `integration::manifest_digest_perf::manifest_digest_does_not_reopen_archive_per_entry`
passes **in the default lane**: the `#[ignore]` it carried while the walk really was quadratic has
been lifted, and its module doc now records that a debug build on Apple silicon measured 2.6× (639 µs
listing, 1.65 ms digest) against a 50× bound, where a 1000-entry quadratic walk produced ~150×. That
is the independent shape check on section D's claim.

**A caution on those two counts, and it is the same caution this whole amendment exists to teach.**
A first run of the same suite, twenty minutes earlier in this session, reported 693 lib unit tests and
failed two integration cases; the run quoted above reported 696 and passed everything. The three extra
tests, the two failures, and the sentinel's `#[ignore]` all moved *while this amendment was being
written*, because other agents were editing the tree concurrently. The two failures were
shared-scratch-directory races on `/Volumes/Temp/claude` between concurrent suite runs
(`AlreadyExists` on a nanosecond-named `split_7z_*.7z`, and an "archive file identity changed while
opening" abort on `extract-*.tar`), not defects in anything this record describes. Treat the exact
counts as a timestamped observation, never as a constant.

## Amendment (2026-08-17, owner ruled the version — bumped to 0.4.0, discharging the "Observable Behaviour Change" prescription)

The "Observable Behaviour Change" section required a minor bump with a BREAKING changelog entry, and
the pass-2 amendment recorded that the bump had **not** been performed because the owner had not
authorised one. They have now ruled, with the test stated explicitly: bump to `0.3.2` if there is no
interface change, to `0.4.0` if there is a breaking change.

Those two clauses pull in opposite directions here, so the ruling was applied on the second, and the
reason is worth recording because it is the interesting case.

**There is no interface change.** A public-API surface diff of the working tree against the committed
`0.3.1` — every `pub` item's declaration line, across all of `src/` — shows **no added, removed or
altered `pub` item**. All fourteen items this change set introduced are `pub(crate)`:
`PayloadTarget`, `PayloadVisitor`, `visit_payloads_by_listing_id`, `HardCapReader` and its
`exact`/`ceiling` constructors, and the libarchive externs that merely moved declaration file. Code
written against 0.3.1 still compiles unmodified.

**There is nonetheless a breaking change**, on types callers already hold rather than on any
signature:

* `ArchiveEntry::crc32` is a `pub` field. AE-2 AES ZIP entries now report `None` where they reported
  `Some(0)`.
* `ArchiveEntry::permissions` is a `pub` field. RAR5 Unix-host entries now report `Some(0o644)` where
  they reported `Some(0)`.
* `calculate_content_multiset_digest_and_size`, `calculate_manifest_digest` and
  `calculate_manifest_summary` return **different values** for duplicate-path archives, and return
  `Err` for a password-protected AES ZIP opened without its password where they returned `Ok`.
* Payload faults now surface before the aggregation loop's `total_size` overflow check, so a caller
  matching on the error can see a different variant for the same archive.

The distinction that decides it: code that *compiled* against 0.3.1 still compiles, but code that
*compared* against results recorded under 0.3.1 no longer agrees. A digest is a value contract, and
silently changing a value contract under a patch number is exactly the failure this record's
"Consumer asymmetry" section warned about for the named downstream consumer. So `0.3.2` was rejected
and `Cargo.toml` now reads `0.4.0`, with the `[Unreleased]` changelog section stamped
`## [0.4.0] - 2026-08-17` carrying that reasoning in its scope note.

Known Risk 4 of this record — a cross-repo check of the AdvancedDeduplicator digest store before
0.4.0 lands — remains **undischarged**. It is a check in another repository and outside what this
change set can perform. It should happen before anyone relies on 0.4.0 digests matching stored ones.

No tag was created. Tagging is a release action distinct from the bump, and the owner asked only for
the version decision and the commits.

## Amendment (2026-08-17, Known Risk 4 discharged — the cross-repo check was performed, and it came back the other way)

Known Risk 4 asked for a cross-repo check of the AdvancedDeduplicator digest store before 0.4.0 is
relied upon, on the premise that this record's encoding change invalidates digests that project has
stored. The earlier amendment recorded the check as undischarged and outside what this change set
could perform. That was wrong on the second half: the repository is present on this machine, so the
check is read-only and nothing blocked it. It has now been performed (ticgit `26c1a983`), without
writing anything to that repository.

**The premise does not hold. Nothing there was invalidated.**

1. **That project never calls this crate's digest API.** A repo-wide search for
   `calculate_manifest_digest`, `calculate_content_multiset_digest_and_size` and
   `calculate_manifest_summary` returns no hit in any `.rs` file. Its `unified_archive::` uses — 24
   of them, concentrated in `src/add-scanner/src/cover_test.rs` — name only `Archive`,
   `ArchiveEntry`, `EntryType`, `ArchiveFormat`, `ExtractionOptions`, `ZipCompressionOptions` and
   `StreamBound`. So no digest it holds was produced here.

2. **Its stored digest is its own.** `manifest_index.manifest_digest TEXT PRIMARY KEY` (its
   `001_initial.sql`) is filled by its own indexer pipeline, and
   `src/add-dedup-daemon/src/effective_manifest.rs::compute_effective_manifest_digest` implements the
   algorithm: filter to non-ignored file entries, hex-encode each fingerprint, **sort, join with
   `,`, CRC32 the joined string**, empty string as the no-key sentinel. There is **no per-path
   occurrence ordinal anywhere in it**.

3. **Which inverts the risk.** That is precisely the encoding this record just adopted. Before this
   change the two implementations *disagreed* on duplicate-path archives — the crate appended
   `#<ordinal>`, that project never did. This change did not break the consumer; it **converged the
   crate onto what the consumer already computed**, and closed a divergence neither side had
   recorded.

4. **Neither changed public field reaches it.** The `ArchiveEntry` fields read in `cover_test.rs` are
   `path`, `size` and `entry_type` — never `crc32`, never `permissions`. Its CRC32 comparison uses an
   `observed_crc32` it computes by decompressing, checked against its own pinned `req.crc32`. So the
   AE-2 `crc32 = None` change and the RAR5 permission change, which are the other two breaks 0.4.0
   carries, are both invisible to it.

**One genuine residual, and it is not the one the risk named.** That project depends on this crate by
**path**: `unified-archive = { path = "../Unified-Archiver/Unified-Archiver" }`. It consumes the
working tree, not a published version, so *no* semver signal gates any behavioural change reaching
it — the 0.4.0 bump this record prescribed buys it nothing. Today that costs nothing because it
touches none of the changed surfaces. It is worth knowing that the protection this record reached for
does not exist for the one consumer it was reached for.

Known Risk 4 is therefore closed as **checked and not applicable**, rather than as satisfied by
someone having recomputed a store.
