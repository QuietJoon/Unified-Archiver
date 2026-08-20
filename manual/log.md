# Manual Log

## 2026-08-17

* **Sync (version stamp — 6 English pages)**: The owner ruled the version for this change set
  (`0.3.2` if no interface change, `0.4.0` if breaking) and it resolved to **0.4.0**, so every page
  that names the crate's current version was retargeted from `0.3.1`: `open-password-protected-archives`
  ("so in 0.4.0 the answer is still…"), `install-native-dependencies` (the links-two-native-pieces
  sentence and the Windows-not-release-verified note), `build-and-test-from-source` (the sample
  `Compiling unified-archive v0.4.0` build line), `cargo-features` (the `version` row and the
  "additive in 0.4.0" status), `format-support-matrix` ("complete as of crate version 0.4.0"), and
  `public-api-surface` (both "version 0.4.0" statements). Body edits only — no page was added,
  removed or retitled and no `description` changed, so `index.md` and the sub-indexes are untouched.
  `synced_hash` was recomputed with the canonical trimmed-body command on exactly these six pages and
  verified to settle; `generated.at` was left alone, because no page's *sources* were re-read — this
  was a version-string retarget, not a re-derivation from the tree.
* **One page fixed but deliberately NOT rehashed**: `tutorials/user/getting-started` carried a
  `unified-archive = "0.3.1"` dependency snippet, now `0.4.0`. That page is one of the 11 already
  listed below as *pre-existing drift* — its canonical body hash disagreed with its `synced_hash`
  before this session touched anything. Recomputing the hash here would have erased the drift signal
  for body changes nobody in this session read or reviewed, so the version string is corrected and
  the stale `synced_hash` is left standing. It stays in the drift set until a real sync run reads the
  page in full. The five pages still reporting drift are `decision-records-and-reviews`,
  `streaming-and-memory`, `stream-a-large-entry`, `options-and-defaults` and this one; `git status`
  confirms none of the first four was modified in this change set.
* **Correction to the AE-2 predicate quoted in this log's first entry**: it was quoted one conjunct
  short. See the inline correction there. The same omission had propagated into
  `checksums-and-integrity`, whose gate paragraph is now rewritten to state all three conjuncts and
  why each earns its place; that page's `synced_hash` was recomputed too. The lesson is the one this
  log already records about hand-edited pages: this file is what a later `write-diataxis-manual` run
  reads to decide what the manual already reflects, so an inexact quote here becomes a page defect
  later.

* **Sync (targeted — 3 English pages)**: Re-synced `checksums-and-integrity`,
  `verify-archive-integrity` and `errors-and-warnings` against the uncommitted working tree
  of `001-unified-archive`. Three source changes drove it: DCR-012 (the content-multiset
  digest dropped its per-path occurrence ordinal), TicGit `04ba4897` (AE-2 AES ZIP entries
  stopped listing `crc32 = Some(0)`), and — landing in the tree while this sync was being
  written — OI-0001-009 / TicGit `82bf8fd4` (the libarchive digest walk became a single
  traversal, and the ZIP `extract_to_stream_by_listing_id` forward was wired into
  `impl ReadBackend for ZipArchive`). No page was added, removed or retitled and no
  `description` changed, so `index.md` and the four sub-indexes are untouched. `generated.at`
  and `synced_hash` were refreshed on exactly these three pages and on no others.
* **What was false — the ordinal**: `checksums-and-integrity` documented the occurrence
  ordinal as part of the digest contract (`<crc32-hex>#<ordinal>` from the second occurrence
  of a repeated path on, review item R0079-0028) and said under *What none of them detect*
  that re-ordering same-path occurrences does move the digest. `src/inspection.rs` now reads
  `fn content_digest_element(crc32: u32) -> String { format!("{crc32:08x}") }` with no
  `path_occurrences` map, and `test_content_digest_grouped_and_spread_duplicates_agree` and
  `test_content_digest_is_order_independent_for_duplicate_paths` assert the opposite.
  `verify-archive-integrity` carried the same claim twice — "with the single exception of
  the duplicate-path ordinal" in the section opener, and a trade-off bullet saying
  order-independence holds only for unique-path archives. Both pages now state
  order-independence unconditionally, and the how-to states the one real break a reader must
  act on: stored digest *values* for duplicate-path archives changed once, while unique-path
  digests are byte-identical because ordinal 0 already emitted the bare hex.
* **What was false — AE-2 AES ZIP**: the ZIP listing walk is now gated on
  `!crc32_check_exempt(&zip_file)`, with
  `fn crc32_check_exempt(zip_file: &zip::read::ZipFile) -> bool { zip_file.encrypted() && zip_file.crc32() == 0 && aes_vendor_version(zip_file) == Some(AES_VENDOR_VERSION_AE2) }`,
  so AE-2 entries list `None` and the digest streams their decrypted payloads.
  *(Correction, 2026-08-17: this entry originally quoted the predicate with only its first two
  conjuncts. The third — the WinZip-AES vendor-version check — is load-bearing, not decoration:
  without it an* encrypted *empty file, whose stored `CRC32(b"")` of `0` is a genuine checksum,
  would also be read as absent, which is exactly the AD 0012 seam
  `test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32` pins. The two-conjunct
  form was the shipped gate only briefly, before it was narrowed; quoting it here as verified
  source is what propagated the same omission into `checksums-and-integrity`, since that page is
  written from this log.)*
  `checksums-and-integrity`'s "the listing reports the stored `0`, so every AE-2 entry
  contributes the CRC32 of nothing" paragraph described a live blind spot that is now closed,
  and is rewritten as a closed one with its two new costs. The how-to gained the precondition
  a caller actually meets: digesting an AES ZIP now requires `Archive::open_encrypted`, and on
  a handle opened without a password the call fails
  `ArchiveError::Format { format: Some(Zip), … "Password required to decrypt file" }` —
  described as the known mislabel it is (a missing password should raise
  `ArchiveError::Password`; TicGit `9bdf2c` owns that, and this page neither fixes nor
  endorses it). The archive-CRC section's "true for ZIP, 7z, and RAR" now names AE-2 as its
  exception, since those entries no longer contribute to the sum.
* **Third page, found while verifying**: `errors-and-warnings`' `NotImplemented` section said
  the crate has a single raiser, that libarchive is its only overrider, and that the digest
  walk "short-circuits on `ArchiveEntry::crc32` before reaching that call for every
  non-libarchive backend". AE-2 entries falsified the last clause, and the OI-0001-009 work
  added a second raiser. The section now names both defaults
  (`extract_to_stream_by_listing_id` and `visit_payloads_by_listing_id`), both `reason`
  strings, the operation label they share, and which backend overrides which.
* **Verified by running, not only by reading**: `cargo check --lib --all-features` and a
  throwaway probe crate outside the repo. Early in the run the probe reproduced the pass-1
  finding — the ZIP id-addressed stream was an inherent method never wired into the trait, so
  `cargo check` printed ``warning: method `extract_to_stream_by_listing_id` is never used``.
  Re-run after the concurrent backend change it no longer reproduces: a two-entry AE-2 ZIP
  digests only with its password, two AE-2 ZIPs of different contents digest differently
  (`16328381` vs `2a08eebb`), and an AE-2 ZIP holding `sub/a.txt` and `sub\a.txt` digests
  successfully to the same value as the same payloads under distinct names — the
  duplicate-after-normalisation `OperationBlocked` that pass 1 reproduced is gone. Both
  pages describe the fixed behaviour, not the defect.
* **Not a full generation run**: this was a hand-applied, page-targeted sync, not a
  `write-diataxis-manual` pass over all 27 pages. Only the three pages above were read in
  full and re-verified against the tree; the other 24 keep whatever frontmatter they already
  had. Read `generated.at: 2026-08-16T…Z` on those three as "this page was verified against
  the tree then", which is exactly what it now means.
* **Pre-existing drift NOT repaired here**: 11 of the 24 pages this run did not touch report
  *hand-edited* under the profile's drift rule — current canonical body hash ≠ `synced_hash`.
  They are `decision-records-and-reviews`, `streaming-and-memory`,
  `install-native-dependencies`, `open-password-protected-archives`, `stream-a-large-entry`,
  `cargo-features`, `format-support-matrix`, `options-and-defaults`, `public-api-surface`,
  `build-and-test-from-source` and `getting-started`. The cause is commit `86277a9`, which
  edited page bodies without recomputing their hashes; two more pages
  (`checksums-and-integrity`, `errors-and-warnings`) were in the same state and are back in
  step only because this run re-verified them. Re-hashing the remaining 11 without re-reading
  each body against the tree would erase the only signal that they were hand-edited — the
  same mistake in the opposite direction — so they are left as they are and reported. They
  need a real sync run.
* **Late entry — 2026-08-16, `explanation/developer/en/sfx-detection-pipeline.md`**: an
  earlier agent rewrote that page's stage-1 paragraph (R0001-0066 / R0001-0067: the ELF and
  Mach-O checks widened past the magic bytes to the whole fixed header), recomputed its
  `synced_hash` and moved `generated.at` to `2026-08-16T00:00:00Z`, without adding a log
  entry — so this file's newest entry read 2026-08-07 while a page asserted a generation run
  nine days later, and the bundle's only hand-edit signal was consumed to hide a hand edit.
  The page's body and stored hash do agree, so it is left exactly as it stands; this bullet
  is the missing declaration, recorded a day late. That edit is still uncommitted.
* **Findings**: three, none filed as tickets from this run — TicGit is outside its file
  ownership, which was `manual/**/en/**.md` and this log. (1) The ZIP
  `extract_to_stream_by_listing_id` forward missing from `src/backend.rs`, recorded in
  DCR-012's 2026-08-17 amendment and fixed in the working tree during this run — worth
  confirming it stays fixed before the change set is committed. (2) The
  `Format`-instead-of-`Password` classification on the ZIP no-password read path, owned by
  TicGit `9bdf2c` and now user-visible on a digest call, which raises its priority. (3) The
  bundle has no gate that fails when a page's body hash and its `synced_hash` disagree, which
  is why 13 pages drifted unnoticed from 2026-08-12 to today; `tests/manual_conformance.rs`
  (uncommitted) appears to close this, and it is adjacent to TicGit `c8d771eb`, which owns
  "nothing detects when the manual's quoted claims go stale".

## 2026-08-07

* **Sync**: Reviewed all 27 English pages against the source tree and updated 14 of them. No
  page was added, removed, or retitled; `index.md` and the four sub-indexes were already
  correct and are unchanged. Eleven pages had body edits and are re-hashed; three more
  (`decision-records-and-reviews`, `install-native-dependencies`, `build-environment`) had
  been edited by the 2026-08-06 hygiene batch without a re-hash and are now back in step, so
  the bundle no longer reports four pages as spuriously hand-edited. `language: en` was added
  to all 27 pages, which the profile expects and the bootstrap run omitted; it is frontmatter,
  so no body hash moved.
* **Cause**: the bundle was generated 2026-08-05 and the documentation-hygiene batch landed
  2026-08-06, changing several of the exact things these pages quote. Ten claims were
  falsified: the `OperationBlocked` reason for encrypted creation (quoted verbatim on two
  reference pages, and still naming the retired record id `AD 0027`), the `Cancelled` label
  list (which advertised an `"extract_file"` label the code cannot emit, contradicting the
  progress how-to), the manifest `description` and the note explaining why it lagged, the
  DEF-004 buffered-backend count, and four observations of the form "`<file>` still says X"
  whose subjects had just been fixed — in `src/options.rs`, `src/creation.rs`,
  `docs/USER_MANUAL.md`, and the source tree's record ids.
* **Record ids**: `AD 0027` was cited on five pages and resolves to nothing; the ruling is
  `MADR-0027`. `sfx-detection-pipeline` cited "AD 0014" twice for the SFX confidence clamp,
  which resolves to a real but unrelated record — the clamp is `MADR-0014-r0001` — and that
  page now says so explicitly, because the same number is correctly cited elsewhere in the
  bundle for the record it does name.
* **Code samples**: all 65 fenced `rust` blocks were type-checked against the built crate.
  Two failed: both `let cb = Box::new(…)` progress-callback snippets on
  `report-progress-and-cancel`, including the page's opening example, gave
  `error[E0282]: type annotations needed` because `ControlFlow`'s break type was unpinned.
  Both now annotate the binding as `Box<dyn ProgressCallback>`, with one sentence saying why.
  The remaining 33 non-compiling blocks are deliberate fragments and signature listings.
* **Also verified, unchanged**: 0 broken links or anchors across 33 files; `type` and
  `audience` match their directories on all 27 pages; every index entry matches its page's
  `description` verbatim; the public-API reference names all 62 public `Archive` methods and
  every v2 handle method with nothing invented; the capability matrix, detection probe order,
  extension and suffix tables, error variants and `Display` strings, limits and defaults,
  `build.rs` behaviour, and the test-suite counts all still match the code. No `docs/` file
  and no translation mirror was touched.
* **Findings**: three tickets — `d8262f20` (`docs/API_REFERENCE.md` still cites the retired
  `AD 0027`), `0477055a` (`module-map.md` lists a `pub(crate)` module as an sfx re-export),
  `c8d771eb` (nothing detects when the manual's quoted strings and repo-state claims go
  stale — the gap that let all ten claims above ship). Both id instances were also appended to
  the existing ticket `dd119253`, which owns the record-id ↔ subject check.

## 2026-08-05

* **Bootstrap**: Created the `manual/` bundle — 27 pages (3 tutorials, 11 how-to guides,
  6 reference, 7 explanation) across the `user`, `developer` and `operator` audiences, plus
  `index.md` and 4 sub-indexes (`how-to/user/en`, `reference/user/en`, `explanation/user/en`,
  `explanation/developer/en`). Derived one-way from the `docs/` bundle and the 0.3.0 source tree;
  `manual/` is its own OKF v0.2 bundle and indexes none of `docs/`, as `docs/` indexes none of it.
  English (`en/`) pages only.
* **Verification**: Every page was audited against the code it documents by independent reviewers
  (accuracy, quadrant purity, frontmatter conformance, body hashes, link resolution). 45 defects
  were found and all 45 fixed in the same run — the load-bearing ones being two limit fields
  documented as enforced when no code path reads them, a licence restatement that was stricter
  than the vendored licence, a test-only constructor listed as public API, and several code
  samples that would not have compiled. Bundle conformance now passes: 27 of 27 pages with
  matching `type`/`audience`, existing `sources` paths, recomputed `synced_hash`, resolving links,
  and no line-number citations.
* **Findings**: 113 raw observations about the repository (never fixed here) were deduplicated into
  23 tickets — `1dfb92d6`, `0d98ed8c`, `6277386e`, `d3cfceed`, `04ba4897`, `208978e9`, `ed3647a7`,
  `1e44087c`, `6d74af4c` at priority 2; `98abe283`, `44d81e5f`, `f5e91841`, `58b58363`, `11bf1524`,
  `6221c83a`, `854caa5a`, `9909d449`, `f393870a`, `9bdf2ca1`, `d4aa8c11`, `ca4c1047`, `86a73a95` at
  priority 3; `aa37c64f` at priority 4 — with dedupe notes appended to the existing tickets
  `3e5a6ecd` (compressed-tar rustdoc residue), `da4fedc0` (further decision-record drift) and
  `2fe44544` (OKF migration items).
