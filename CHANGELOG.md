# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Release status.** Versions are listed newest-first. Three git tags exist — `v0.1.0`,
> `v0.1.1`, and `v0.3.1` — and **no version of this crate has ever been published to a public
> registry.** Two of the headings below therefore do not mean what a reader would assume:
>
> - **`0.2.0`** was a `Cargo.toml`-only bump (`8de7c77`, 2026-04-27). It was never tagged and
>   never shipped.
> - **`0.1.2`** never existed as a `Cargo.toml` version at all — the crate went 0.1.1 → 0.2.0
>   directly. Its section records a real 2026-04-23 hardening pass whose changes were carried
>   by the 0.2.0 bump.
>
> `0.3.0` (`e5c1ce2`, 2026-04-28) was likewise an untagged, unpublished bump, and has no
> section of its own: its changes are recorded under 0.3.1 below, which is why that section
> spans everything since 0.2.0.

## [0.4.0] - 2026-08-17

> **Why the minor bump.** The *signature* surface is unchanged — a public-API diff against
> 0.3.1 shows no added, removed or altered `pub` item, and every new item introduced here is
> `pub(crate)`. The break is behavioural, on types callers already hold: the public fields
> `ArchiveEntry::crc32` and `ArchiveEntry::permissions` report different values than before,
> the digest methods return different values for duplicate-path archives, and one of them now
> returns `Err` where it returned `Ok`. Code that compiled against 0.3.1 still compiles; code
> that *compared* against stored results does not still agree. Under semver that is a break,
> so this is 0.4.0 rather than 0.3.2.

### Breaking

- **The content-multiset digest changed value on duplicate-path archives** (DCR-012).
  `calculate_content_multiset_digest_and_size` — and therefore `calculate_manifest_digest`
  and `calculate_manifest_summary`, which are shims over it — used to distinguish repeated
  occurrences of the same archive-internal path by appending a per-path occurrence ordinal
  to the digest element from the second occurrence onward (`<crc32-hex>#<n>`). The ordinal is
  gone: every entry now contributes the bare `{crc32:08x}` of its own payload, and
  multiplicity is carried by the element simply appearing that many times in the sorted
  multiset. The digest is consequently independent of the *relative listing order* of
  same-path occurrences, which the ordinal encoding made it depend on. *Migration:* a digest
  **stored** for an archive that repeats a path (TAR append/update, shadowed ZIP/7z
  central-directory entries) will not match a freshly computed one and must be recomputed;
  the known downstream consumer of stored digests is **AdvancedDeduplicator**. Archives whose
  paths are all distinct are unaffected — a first occurrence always contributed the bare hex,
  so their digests are byte-for-byte what they were. The `_and_size` total is unchanged.
- **AE-2 AES ZIP entries list `crc32 = None` instead of `Some(0)`, and the digest APIs now
  need the password on those archives** (ti-04ba4897, R0079-0007). AE-2 stores `0` in the
  central-directory CRC32 field *by specification*, so the listed `Some(0)` was a placeholder
  and never a checksum. The `zip`-crate listing path now gates on `crc32_check_exempt` and
  leaves those entries `None`. That gate is a **three-way** conjunction —
  `encrypted() && crc32() == 0 && aes_vendor_version(zip_file) == Some(AE2)`, the third
  conjunct reading the vendor version out of the entry's `0x9901` WinZip-AES extra field
  (`0x0001` = AE-1, which stores a real CRC32; `0x0002` = AE-2, which stores none). The
  vendor-version conjunct is load-bearing, not belt-and-braces: `encrypted() && crc32() == 0`
  alone is a *superset* of AE-2 and would also sweep in an encrypted entry whose payload is
  genuinely **empty** (a ZipCrypto empty file, or an AE-1 empty file), whose stored
  `CRC32(b"") == 0` is a real checksum per AD 0012 — sweeping it in would discard a checksum
  the archive carried and force a needless decrypt-and-stream in the digest walk. Those
  entries therefore keep `Some(0)`, as does any plaintext empty file, and
  `test_zip_wrapper_encrypted_empty_file_without_aes_field_keeps_crc32` pins the seam.
  *Consequence, and the reason this is
  breaking twice over:* the digest walk no longer folds a constant placeholder for AE-2
  entries, so it is genuinely content-sensitive on them — two AE-2 ZIPs with identical paths
  and sizes but different contents now digest differently, where they previously collided —
  but to see the content it must **decrypt the payload**. A digest call that previously
  returned `Ok` for a password-protected ZIP opened *without* a usable password now returns
  `Err`. *Migration:* open such archives through `Archive::open_encrypted()` before calling
  `calculate_content_multiset_digest_and_size` / `calculate_manifest_digest` /
  `calculate_manifest_summary`. Note that the refusal currently arrives as
  `ArchiveError::Format` with the message `"… Password required to decrypt file"`, **not** as
  `ArchiveError::Password` — that mis-classification predates this change, is tracked
  separately as ti-9bdf2c, and is deliberately not fixed here; callers matching on the variant
  must expect `Format` today and should not treat it as the settled classification.

### Changed

- **The digest walk resolves every CRC-less entry in a single traversal instead of re-opening
  the archive once per entry** (OI-0001-009, ticgit `82bf8fd4`). On the libarchive-backed
  formats (TAR and its compression wrappers, ISO) `calculate_content_multiset_digest_and_size`
  — and therefore `calculate_manifest_digest` / `calculate_manifest_summary` — used to call
  `extract_to_stream_by_listing_id` per CRC-less entry, and each such call opened a fresh
  handle and re-walked from the first header. On a **compressed** TAR that re-ran the
  decompressor from byte zero for every member, so the cost was quadratic in the entry count.
  A new backend hook (`ReadBackend::visit_payloads_by_listing_id`, overridden only by
  libarchive) now opens one handle, walks the headers once in ascending listing-id order, and
  hands each target a reader borrowed from that handle; a compressed TAR is decompressed once.
  The cost is linear in the archive's bytes, and the difference is large on TARs with many
  members. *No API changed and no digest value changed:* the same `crc32_of_bounded_payload`
  helper enforces the same DCR-011 exact/ceiling bound on both routes, resolution stays keyed
  on the stable listing id (never the path, so duplicate-path occurrences stay distinct), and
  the per-entry resolver remains as the fallback for any backend without a one-pass walk —
  including a bail-out to it if a backend were ever to hand out non-unique listing ids.
- **Payload faults now surface before the aggregation loop's `total_size` overflow check.** A
  side effect of the single traversal above: CRC-less payloads are resolved up front rather
  than interleaved with the sum. An archive that would trip *both* a truncated-member
  `ArchiveError::Corruption` and a `total_size` overflow now reports the corruption first,
  where it previously could report the overflow. Both remain errors and both still name their
  cause; only which one wins the race changed. Callers matching on the error of a
  doubly-broken archive may observe the different variant.
- **A repeated libarchive finalization now replays libarchive's own failure text** (R0001-0018).
  After a failed `close_write`, every later finalization attempt returned an
  `ArchiveError::OperationBlocked` that named only the path. The rendered reason of the
  original failure is now recorded and appended to that message (`"… must be discarded and
  rewritten: <original reason>"`), so the second and later attempts say the same thing the
  first one did — e.g. `"Write error: No space left on device"` — instead of degrading to a
  path-only complaint. The variant and the `finish` operation label are unchanged; only the
  message text grew. This state is now independent of the entry-write `write_poisoned` flag,
  which the two used to share: a writer poisoned by a mid-entry write failure can still
  finalize the entries that succeeded, and a repeated finish after a *clean* close still
  returns `Ok(())`.

### Fixed

- **RAR5 Unix-host entries report their real permissions instead of `Some(0)`** (ticgit
  `b75cafb4`). `ArchiveEntry::permissions` is a public field, so this is an observable change.
  The RAR backend applied the RAR4 packing unconditionally — RAR4 stores `st_mode` in the
  upper 16 bits of a 32-bit attribute word, so the decode was `file_attr >> 16`. RAR5 stores
  the mode **unshifted** in the file header's Attributes vint, so that shift flattened every
  RAR5 Unix entry to `Some(0)` — a positive claim of "readable by nobody" for a file that
  actually recorded `0o644`. The packing is now selected by the header's `unp_ver`
  (`>= 50` = the RAR5 family, since the UnRAR DLL API exposes no archive-format field and
  normalises both formats' host OS through one line), and an attribute field carrying no Unix
  file-type bits at all now yields `None` rather than `Some(0)`. *Migration:* code that
  special-cased RAR entries as "permissions always zero" should drop that workaround; code
  matching `Some(0)` as a sentinel for "unknown mode" must match `None` instead.
  `tests/integration/permissions_contract.rs` pins the decoded `0o644` on both committed RAR5
  fixtures, alongside the ZIP/TAR/7z lanes.

## [0.3.1] - 2026-08-13

> **Scope note.** Tagged `v0.3.1`. `Cargo.toml` was bumped to `0.3.0` on 2026-04-28, but that
> version was never tagged, never published, and never got a CHANGELOG section, so **0.3.1
> carries every change since 0.2.0** — the 2026-08-04 batch below as well as the 2026-08-12/13
> streaming work. Two `[Unreleased]` sections held that work separately; they are consolidated
> here.

> Every change is additive or an error-classification / diagnostic tightening; no public API
> changed. `StreamBound` remains `{ DeclaredSize, Cap(u64), Unbounded }`, and `ArchiveError` /
> `Operation` gained no variants.

### Changed

- **Stream-path refusals are labelled `extract_to_stream`** (R5 / ti-581bcda4, DCR-006 Amendment 4).
  `Archive::extract_to_stream[_with_options]` refused by a staging backend (ZIP/7z/RAR) reported
  `operation: "extract_to_memory"`, because the capped memory helper those paths reuse internally
  hard-coded the constant — contradicting the convention that the label names the *public* operation
  the caller invoked. *Migration:* callers matching `operation == "extract_to_memory"` on errors from
  the stream path must match `"extract_to_stream"` instead. The memory entry points are unchanged.
- **A RAR `Cap(n)` refusal now fires at call time on the declared size** (R2, DCR-006 Amendment 4).
  UnRAR previously aborted only once *decoded* bytes crossed the budget, mid-decode, so a RAR entry
  whose header over-declared but decoded small still succeeded where ZIP and 7z would have refused
  it, and an honestly oversized entry paid the decode work up to the cap first. All three staging
  backends now refuse on the header's declared size before any decode, with the same "Entry '…'
  declares N bytes; exceeds the configured per-entry limit of M bytes" phrasing. *Migration:* the
  newly refused population is specifically **a RAR entry streamed under a `Cap(n)` below its declared
  size** — a tight `ExtractionLimits` alone changes nothing, because the facade's single-entry gate
  already refused declared-over-limit entries at call time on every backend and every path
  (`check_single_entry_safe`, ceiling `min(max_file_size, max_total_size)`, R0081-0025). The error
  names both the declaration and the cap, so raise the budget. Entries whose header declares no size
  are unaffected (no declaration is invented) and stay covered by the mid-decode abort. The
  incremental libarchive path still serves a prefix under `Cap(n)`.
- **Over-production is sticky across retries** (R3 / ti-4ba1ff1a, DCR-006 Amendment 4). Only
  truncation latched before: a caller retrying after `io::ErrorKind::InvalidData` consumed one probe
  byte per retry and, once the inner stream drained, received a clean `Ok(0)` — an integrity error
  decaying into an ordinary EOF. Both violation classes now latch, and a retry replays the verdict
  without consuming another byte. *Migration:* a read loop that retried past `InvalidData` expecting
  eventual EOF will now loop on the error; treat it as terminal (which it always was).
- **RAR staged payloads are length-checked against the listing declaration** (R1, DCR-006
  Amendment 4). The staged file was previously compared only with its own metadata, so RAR truncation
  and over-production surfaced only at read time and only under `StreamBound::DeclaredSize`. A
  mismatch is now `ArchiveError::Corruption` from the extract call, matching ZIP's and 7z's staging.
  *Migration:* a damaged RAR entry that previously produced a short buffer now errors; entries whose
  header declares no size keep the read-time `UnexpectedEof` fallback.
- **Digest methods detect truncated CRC-less entries** (R6 / ti-c0d6fad6, DCR-011).
  `calculate_content_multiset_digest_and_size`, `calculate_manifest_digest` and
  `calculate_manifest_summary` bounded the per-entry CRC hasher with a *ceiling*, so a truncated
  member of a format without a per-entry checksum (TAR family, CPIO, ISO) digested its short payload
  and the call returned `Ok`. The bound is now exact where the listing declares a size, and a
  violation returns `ArchiveError::Corruption` naming the entry. The digest **value** for a healthy
  archive is unchanged (pinned by a regression test). *Migration:* callers digesting possibly-damaged
  archives must handle `Corruption` where they previously always received a digest; entries with no
  declared size (raw gzip/bzip2/xz) are unaffected.
- **`StreamBound::DeclaredSize` is an exact-length contract** (R0001-0011 / OI-0001-001, DCR-006
  Amendment 3). When the preflight listing declares a size, a stream that ends *before* that size now
  fails the read with `io::ErrorKind::UnexpectedEof` (sticky across retries) instead of returning a
  short buffer and a clean EOF; over-production keeps DCR-006's `io::ErrorKind::InvalidData`. No
  `StreamBound` variant was added or removed. *Migration:* callers that deliberately tolerated short
  entries should use `StreamBound::Cap(n)` or `StreamBound::Unbounded` (both remain ceiling-only), or
  handle `UnexpectedEof`. Entries that declare no size (raw gzip/bzip2/xz readers, libarchive entries
  with an unset size field) are unaffected — no declaration is invented for them.
- **`StreamBound` now shapes what the backend may materialise** (R0001-0011, DEF-004 first step). The
  backend receives `min(bound, max_file_size, max_total_size)` instead of `max_file_size` alone, and
  ZIP/7z gained `extract_to_stream_with_limit` overrides that reject an over-budget declared size
  before buffering (RAR already had one). *Migration:* a `Cap(n)` below the entry's declared size now
  fails at the extract call with `ArchiveError::OperationBlocked` on ZIP/7z/RAR, where it previously
  returned a reader over an already-materialised entry; the incremental libarchive path still serves a
  prefix. To read a window of a larger entry portably, wrap a `DeclaredSize` stream in `Read::take`.
- **`max_total_size` participates in the streaming budget** (R0001-0011). The stream path handed the
  backend `max_file_size` alone, so a caller whose total budget was tighter than its per-file budget
  got no runtime protection on unknown-size entries. It now uses the effective entry ceiling, matching
  the memory paths (R0001-0006 / R0001-0010). Callers with `max_total_size < max_file_size` will see
  streams capped tighter than before.
- **`total_size()` under `DeclaredSize` reports the listing's declaration** rather than the
  materialised buffer length on the staging backends (R0001-0008 direction), so `progress()` reaches
  `1.0` exactly when the entry completes.

### Earlier in this release — the 2026-08-04 to 2026-08-06 batch

#### Added (2026-08-04)

- **TAR.ZST / TAR.LZ4 / TAR.LZMA creation.** `Archive::create` now accepts
  `TarZst`, `TarLz4`, and `TarLzma`: the libarchive
  `archive_write_add_filter_zstd` / `_lz4` / `_lzma` bindings are wired,
  `can_create()` / `capabilities().compression_write` flipped to full, and
  compression levels plumb through the shared `compression-level` filter
  option (with `Store` mapped to zstd level 1, since zstd reads level 0 as
  "library default" rather than "no compression"). A libarchive built
  without the matching codec fails loudly at writer construction instead of
  degrading. Round-trip integration coverage in
  `tests/integration/creation.rs`. Closes the R0075-0031 creation deferral.

#### Changed (2026-08-04)

- **Advisory file locking migrated from `fs2` 0.4 to `fs4` 1.x** (maintained
  successor; `fs2` has been unreleased since 2019). Modify-mode's exclusive
  lock keeps the identical contract: fs4's `try_lock` reports contention and
  I/O failure as `Err` exactly like the fs2 call it replaces, so every
  non-acquisition still surfaces as the same
  `OperationBlocked("Another Modify session holds the advisory lock …")`.
  A new `test_second_modify_blocked_by_advisory_lock` unit test pins that
  contention path, which had no regression coverage under `fs2` either.
  (MADR-0009 amendment; decision-review-2026-07-19 §B "wrong-primitive"
  group.)

---

Review 0068 closure pass. The reviewer worked under static-only inspection
(their own preface notes that `cargo clippy --all-targets --all-features`
did not complete), so a large majority of the Critical/High findings were
already addressed in head-of-branch code. The accepted residual fixes
below land alongside a closure ADR (AD 0051) that catalogs the discarded
stale findings so they don't get re-raised. Group D (architectural pass —
backend traits, Archive god-object split, large-file refactors) is
sequenced as forward work in the implementation plan rather than landing
in this pass.

#### Changed

- **`extract_to_memory_with_options` / `extract_to_stream_with_options` now
  honor `options.password`** by reopening the archive through
  `Archive::open_encrypted` when a password is set, mirroring the
  password-aware behavior of `extract_all` / `extract_file` /
  `extract_some`. Other fields (`destination`, `overwrite`, `verify_crc32`,
  `preserve_*`, `filter`, `progress`) remain inapplicable to single-entry
  in-memory / stream reads and stay documented as ignored. **Supersedes
  AD 0050** (R0068-0003 / R0068-0004; tracked by AD 0051).
- **`ZipWriter::add_directory_recursive` now preserves the source
  directory's name in archive paths** (`add_directory_recursive("foo/bar")`
  emits `bar/<...>` entries, not bare `<...>`). Matches the libarchive
  backend's recursive-create convention and standard tools (`tar`,
  `zip -r`, `7z a -r`). Existing layouts are no longer compatible —
  callers depending on the old strip-`dir_path` behavior must rewrite
  their assertions. (R0068-0023)
- **`AtomicOutputFile::commit` on Windows now uses
  `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`** via
  a shared `crate::ffi::common::rename_with_overwrite` helper (also reused
  by `modification.rs::commit_changes`). Removes the
  delete-then-`persist` window that could lose the original on Windows
  if `persist` failed. (R0068-0065)
- **`Archive::modify` encryption probe error remap.** Header-encrypted
  formats whose `Archive::open` fails with `Password` /
  `"encrypt"`-flavored `Format` / `Corruption` errors now uniformly map
  to `OperationBlocked(MODIFY, "Encrypted archives cannot be modified
  ...")`, instead of leaking the underlying password error. (R0068-0009)
- **`check_overwrite_conflicts` now resolves output paths via
  `sanitize_entry_path`**, applying the same component-normalisation +
  symlink-ancestor escape policy the real extractors use. The preflight
  and the actual extract no longer disagree under symlinked ancestors.
  (R0068-0020)
- **`open_encrypted` returns format-aware "encryption not supported"
  reasons** for TAR variants, Gzip, Bzip2, Xz, ISO instead of the
  catch-all `"Format not yet supported"`. (R0068-0070)
- **`is_encrypted` rustdoc tightened** to specify "best-effort metadata
  probe; returns `false` for header-encrypted archives that refuse
  listing without a password — use `open_encrypted` then check for
  password errors for guaranteed validation." (R0068-0069)
- **`ZipArchive::list_files` switched to `by_index_raw`** so encrypted
  ZIP archives can list metadata without holding the password. Required
  for the password-aware extract-to-memory/stream path above to work
  end-to-end. (Discovered while wiring R0068-0080's tests.)

#### Added

- **ISO 9660 magic recognition in `detect_from_bytes`.** When the supplied
  buffer is `≥ 32774` bytes the detector checks for `CD001` at offset
  `32769` (sector 16 PVD) and returns `ArchiveFormat::Iso`. Typical 1 MiB
  SFX scan buffers cover this; smaller buffers fall through cleanly.
  (R0068-0007)
- **`StreamingExtractor::take_bounded(self, fallback: u64) ->
  std::io::Take<Self>`** clamps reads to the entry's declared `total_size`
  (or the supplied fallback when size is unknown). The bare `Read` impl
  is unchanged. (R0068-0061)
- **`tests/review_0068_test.rs`** — 19 new tests covering per-format
  recursive create layout, password propagation through the
  `*_with_options` paths, offset-open path identity, multipart
  unreadable-directory I/O surfacing (Unix-only), SFX raw-signature
  negative cases, libarchive empty-directory round-trip, recursive
  symlink rejection parity, dup-path commit rejection, backup noclobber,
  `strip_progress` callback semantics, and the
  `compression_ratio == compression_fraction` deprecation contract.
  (R0068-0078..0089)
- **`docs/USER_MANUAL.md` "Locating `rar.exe`" and "Password handling
  caveat" sections** for the optional `external-rar-create` Windows
  feature. The CLI mode passes `-hp{password}` as a process argument;
  the password is therefore visible in the OS process listing while
  `rar.exe` runs. `SecStr` only protects in-process memory.
  (R0068-0066, R0068-0067, R0068-0068)
- **`AD 0051`** — Review 0068 closure record (catalogs discarded stale
  findings, accepted residual fixes, supersession of AD 0050, and the
  forward-looking Group D architectural sequencing).

#### Internal

- **`src/sfx/signatures.rs` is now `pub(crate)`.** `Signature`, `SIGNATURES`,
  `scan_for_signatures` are no longer part of the public API; they were
  internal SFX scanner primitives that should never have been exposed.
  `find_first_signature` is `#[cfg(test)]`-gated. The high-level public
  SFX surface (`detect_sfx`, `SfxDetectionResult`, `StubType`) is
  unchanged. (R0068-0047)
- **`AtomicOutputFile::inner` is now plain `NamedTempFile`** rather than
  `Option<NamedTempFile>`. The two `expect("file present until commit()")`
  calls are gone; `commit` consumes `self` and destructures it directly.
  (R0068-0064)
- **Public `Operation` enum** in `unified_archive::error` (re-exported as
  `unified_archive::Operation`). Construction sites can now pass typed
  variants (`Operation::ExtractAll`, etc.) to `ArchiveError::operation_blocked`
  and friends, instead of relying on the kebab/snake-case `&str`
  constants. The legacy `error::ops::*` constants are kept as `const`
  views over the enum's `as_str()` so existing call sites are unaffected
  during the migration. (Group D D7 / R0068-0045)
- **AD 0052** codifies lazy-validation semantics for backend `open()`
  across Piz / ZipReader / SevenZ (no parse until first use) and
  documents libarchive's eager-then-discard variant. Per-backend rustdoc
  on each `open` constructor now states the validation timing
  explicitly. (Group D D8 / R0068-0024..0026, R0068-0035 partial)
- **`pub(crate) trait ReadBackend`** in `src/backend.rs` (Group D D1 /
  R0068-0029). Five-method scaffold (`list_files`, `extract_to_memory`,
  `extract_to_stream`, `extract_file`, `test_integrity`) with per-backend
  forwarding impls. The `inspection.rs::ArchiveBackend::list_entries`,
  `inspection.rs::Archive::validate_integrity`, and
  `extraction.rs::dispatch_read_backend` dispatch sites now route
  through the trait instead of the prior `ReadBackendRef` enum + manual
  match ladder. `extract_file` stays `#[allow(dead_code)]` until D2
  unifies the disk-write surface.
- **AD 0053** baselines the design for the remaining Group D
  sub-phases (D2 Archive god-object split, D3 ValidatedSource token,
  D4 per-backend handle reuse, D9 LibarchiveArchive r/w split, D10
  large-file refactors).
- **D3 / `ValidatedSource` token landed** (AD 0055): a `pub(crate)`
  newtype around `&Archive` whose existence is the type-level proof
  that the caller has a fresh listing. Replaces the comment-only
  invariant on `extract_to_*_unchecked` at the
  `commit_changes` and `calculate_manifest_digest` call sites.
  Public extract-with-options paths now also route through it. The
  legacy `_unchecked` methods stay during the migration window.
  (R0068-0039 / R0068-0040)
- **D4 / Piz + ZIP handle caching landed** (AD 0054): each
  long-lived `Archive` now amortises file open + central-directory
  parse / mmap setup over the operation count instead of paying it
  per call. `PizArchive` memoises the env-default mmap via
  `OnceCell`; `ZipArchive` memoises the `RawZipArchive<File>` via
  `Mutex<Option<...>>`. 7z, UnRAR, and libarchive caching remain
  forward work — each needs upstream-rewind-semantics validation
  before landing. (R0068-0032 / R0068-0033)
- **AD 0056** records the deferral of D9 (LibarchiveArchive
  read/write struct split) until D2 lands the mode-specific backend
  enums — sequencing the split after D2 avoids re-touching
  dispatch sites.
- **AD 0057** records the deferral of D10 (large-file refactors and
  `#[cfg(test)]` move-out) until D2/D3/D4/D9 land — sequencing
  ensures the splits reflect the post-refactor layout instead of
  the pre-refactor one.

#### Decision records (Review 0068)

- AD 0051 — Review 0068 closure (bulk-reject stale findings, accept
  residuals, supersede AD 0050)
- AD 0052 — Codify lazy-validation semantics for backend `open()` (D8)
- AD 0053 — Group D architectural pass design baseline (D2/D3/D4/D9/D10)
- AD 0054 — Cache Piz mmap + ZIP `RawZipArchive` handle (D4 first cut)
- AD 0055 — `ValidatedSource` token replaces `_unchecked` extraction (D3)
- AD 0056 — Defer LibarchiveArchive read/write struct split (D9 deferred)
- AD 0057 — Defer large-file refactors and `#[cfg(test)]` move-out (D10 deferred)

---

## [0.2.0] - 2026-04-24

> **Never tagged, never published.** `Cargo.toml` carried `0.2.0` from 2026-04-27 (`8de7c77`)
> until the 0.3.0 bump the following day. The heading's date is when the changes landed, not
> when the version was stamped.

Review 0066/0067 pass. **This is a breaking release.** Several public methods
change their signatures to surface misuse instead of silently no-oping, the
deliberate-drop `Clone` impl on `CompressionOptions` has been removed to stop
silent progress-callback loss, and a number of long-standing bugs in path
handling, SFX detection, and archive modification were fixed.

### Breaking

- **`Archive::pending_operations(&self) -> Result<usize>`** now errors when
  called on a non-Modify handle instead of silently returning `0` (R0066-0053).
- **`Archive::clear_operations(&mut self) -> Result<()>`** now errors when
  called on a non-Modify handle instead of silently no-oping (R0066-0054).
- **`Archive::remove_entry(&mut self, path) -> Result<usize>`** now returns the
  number of source entries queued for removal; `0` means the path did not
  appear in the source listing (R0066-0052).
- **`CompressionOptions` no longer implements `Clone`.** The prior impl
  silently dropped the progress callback. `strip_progress(&self) -> Self`
  makes the intent explicit for callers that genuinely want a progress-less
  copy (R0066-0049). `ModificationOptions` also loses `Clone` for the same
  reason.
- **`CompressionOptions::builder(format)` removed** — it was an alias for
  `CompressionOptions::new(format)` that implied validation semantics that
  didn't exist (R0066-0048).
- **`validate_archive_internal_path` rejects `./file`.** Archive-internal
  paths may no longer include `Component::CurDir`; names must round-trip
  cleanly through `sanitize_entry_path` (R0066-0051).
- **`sfx::signatures::scan_for_signatures(buffer)` /
  `find_first_signature(buffer)`** drop the unused `chunk_size` parameter
  (R0066-0060). `src/sfx/detection.rs::CHUNK_SIZE` removed.
- **`AtomicOutputFile::create(path, overwrite, op)`** gains an operation
  label argument instead of hardcoding `"extract"`, so non-extraction
  callers no longer get mislabeled error diagnostics (R0066-0063).
- **`Archive::commit_changes` rejects duplicate output paths.** Retained-vs-
  added and added-vs-added collisions now fail fast with
  `OperationBlocked` before the temp archive is written (R0066-0011/0012).
- **Backup creation is noclobber.** `ModificationOptions::create_backup`
  refuses to overwrite an existing backup file instead of silently
  destroying it (R0066-0013).
- **`ArchiveEntry::compression_ratio()` is deprecated.** It returns
  `compressed / uncompressed` (a shrinkage fraction), which is the
  **inverse** of `ExtractionLimits::max_compression_ratio`. Use the new
  `compression_fraction` (shrinkage) or `expansion_ratio` (zip-bomb gauge)
  methods instead. The deprecated alias will be removed in a future
  release (R0066-0050).
- **Recursive libarchive creation rejects symlinks and special files.**
  Previously the libarchive path silently skipped non-files, producing
  archives that differed by backend. ZIP already rejected symlinks; the
  behavior is now consistent (R0066-0022).

### Added

- **`Archive::open_at_offset()` preserves the caller-facing path.** The
  staged tempfile is no longer visible through `Archive::path()`, multipart
  sibling discovery anchors on the original source directory, and the
  tempfile stays alive via `_backing_tempfile` until drop (R0066-0001/0002).
- **`stage_suffix_for` preserves the full source extension.** Embedded ISO,
  `.zip`, `.7z`, `.rar` (and other single-extension formats) are no longer
  mis-routed to the generic backend when opened via offset staging
  (R0066-0008).
- **Recursive libarchive creation preserves empty directories** — parity
  with ZIP recursive-create behavior (R0066-0021).
- **`add_directory_recursive` validates the input path.** Missing or
  non-directory inputs fail fast with `InvalidPath` rather than mid-walk
  I/O errors.
- **`creation::validate_file_path` / `validate_directory_path` helpers** so
  the `external-rar-create` feature compiles on Windows again (R0066-0006).
- **SFX raw-compression probes.** Stage 3 of SFX detection now parses a
  minimal structural header for gzip, bzip2, and xz instead of the
  prior `len() >= 100` catch-all, suppressing false positives on ordinary
  executables that contain incidental magic bytes (R0066-0005).
- **Numeric split-volume boundary check.** `.001`-style multipart
  detection requires `<stem>.<digits>` and no longer sweeps in unrelated
  siblings with shared prefixes (R0066-0017).
- **Single-file extraction runs the archive-level ratio guard.**
  `extract_file` now calls `check_extraction_safe_with_archive`, closing
  the zip-bomb bypass that affected CRC-less compressed formats
  (R0066-0018).
- **Multipart detection propagates directory-read failures** instead of
  treating a permission error or missing directory as "not multipart"
  (R0066-0016).
- **Decision records AD 0050 + AD 0051** (review 0067 duplicate reject +
  R0066-0003/0004 documented-options reject).

### Changed

- **`SfxDetectionResult::probable()` clamps confidence to `[0.5, 0.99]`**
  — the probable band documented on the `confidence` field. A flagged
  SFX result can no longer coexist with zero confidence; genuine
  negative detections must use `not_sfx()` (R0066-0059).
- **Modify-mode temp paths append rather than replace the extension**,
  keeping leftover temps recognizable to humans and recovery tooling
  (R0066-0055).
- **`ArchiveMode` drops its stale `#[allow(dead_code)]`** and outdated
  "planned for Phase 5-6" comment now that Write + Modify are shipped
  (R0066-0041).

### Removed

- **Dead `archive_error_to_io()` helper** (R0066-0062).

### Decision records

- AD 0050 — REJECT R0066-0003/0004: `extract_to_{memory,stream}_with_options`
  options-ignored is documented in rustdoc; narrowing the parameter to
  `&ExtractionLimits` is deferred pending the v0.2+ god-object split.
- AD 0051 — REJECT R0067 as byte-identical duplicate of R0066 (same issues
  re-numbered), mirroring AD 0025's handling of R0058/R0057.

### Backend dispatch consolidation (partial Cat C1)

- **`ArchiveBackend` variants are now `Box<T>`.** Every variant wraps its
  backend in a heap allocation so the enum itself is single-pointer sized.
  The `#[allow(clippy::large_enum_variant)]` suppression is gone. Pattern
  matches continue to auto-deref the boxed value, so match arms that called
  `writer.add_file_from_data(...)` compile unchanged (R0066-0042).
- **Internal CRC-walking listing variant removed.** `LibarchiveArchive`
  now exposes only `list_files_metadata_only`; the prior `list_files_internal(compute_crc: bool)`
  that could re-introduce decompression during listing has been deleted.
  CRC verification belongs to `validate_integrity` /
  `calculate_manifest_digest` exclusively (R0066-0058).
- **Listing dispatch consolidated.** `Archive::list_files` (cached) and
  `Archive::list_files_for_limits` (uncached) now share one
  `ArchiveBackend::list_entries(op)` method instead of duplicating the
  per-backend match (R0066-0029 partial, R0066-0038).
- **Finalization dispatch consolidated.** `Archive::finish` and the `Drop`
  impl share a private `finalize_write_backend` helper so the error-
  propagating and error-ignoring paths stay in lockstep (R0066-0036).
- **Creation-side write dispatch consolidated.** A `WriteBackend<'a>`
  borrow enum plus `ArchiveBackend::as_write(op)` replace the 4-way
  `ArchiveBackend` match that was duplicated across every creation
  method; the `Unrar | Piz | SevenZ | ZipReader => read_only_backend`
  arm now lives in one place (R0066-0037).
- **Read-side dispatch consolidated.** A `ReadBackendRef<'a>` borrow
  enum plus `dispatch_read_backend` helper back `extract_to_memory_unchecked`
  and `extract_to_stream_unchecked` with a single place that surfaces
  `write_mode_only` errors for the `ZipWriter` variant (R0066-0029 partial).

### Module reorganization (Cat B partial)

- **`src/archive.rs`** `1077 → 711` LOC — test block moved to `src/archive/tests.rs`.
- **`src/extraction.rs`** `1040 → 675` LOC — test block moved to `src/extraction/tests.rs`.
- **`src/inspection.rs`** `971 → 551` LOC — test block moved to `src/inspection/tests.rs`.
- **`src/modification.rs`** `1204 → 761` LOC — test block moved to `src/modification/tests.rs`.
- **`src/ffi/libarchive_wrapper.rs`** (1665 LOC) and **`src/ffi/wrapper.rs`** (1098 LOC)
  intentionally left intact: both are FFI-boundary files without embedded test
  blocks, and a meaningful split (read vs write in libarchive's case, parse
  vs extract in UnRAR's) depends on the LibarchiveArchive read/write split
  (R0066-0057) and the backend-trait refactor (R0066-0027/0028/0029) — part of
  the deferred architectural cluster below.

### Known limitations / follow-up

Review 0066 surfaced a significant architectural cluster (god-object split,
trait-based backend dispatch, modify-mode via native backends, LibarchiveArchive
read/write split, UnRAR worker model, backend metadata caching, the remaining
two file splits) that is too large for a single release and has been deferred.
These items are explicitly *not* tracked as individual `OI-*` entries — the
user elected to treat them as scope for a future architectural pass rather
than fragment them into ledger entries. Concretely unaddressed in v0.2.0:

- R0066-0005 (partial — raw-sig parser probes added, full archive-open
  validation still deferred)
- R0066-0009/0010 (modify encryption probe, streaming add API)
- R0066-0014/0056/0057 (modify via native backends, drop ZIP sidecar,
  split LibarchiveArchive)
- R0066-0019/0020 (bomb-detection estimation for selective extraction,
  conflict-check vs extraction path policy alignment)
- R0066-0023 (recursive add path semantics parity)
- R0066-0024/0025/0026 (eager-vs-lazy open normalization)
- R0066-0027/0028 (god-object split, ArchiveBackend mode-split — require
  full API redesign beyond the partial consolidation landed here)
- R0066-0029 (backend routing duplication — partially closed via
  `WriteBackend` + `ReadBackendRef` helpers; the full trait-based
  extraction/creation dispatch is still pending)
- R0066-0030/0031 (UnRAR worker model)
- R0066-0032/0033/0034/0035 (backend metadata caching)
- R0066-0039/0040 (`_unchecked` session-type hardening)
- R0066-0043/0044/0046/0047 (module reorg beyond the four test-block splits)
- R0066-0064/0065 (AtomicOutputFile typed state + Windows `ReplaceFileW`)
- R0066-0066/0067/0068 (external-RAR discovery + CLI password leak)
- R0066-0069/0070 (`is_encrypted` / `open_encrypted` contract refinements)
- R0066-0075/0076 (remaining `libarchive_wrapper.rs` / `wrapper.rs` splits)
- R0066-0078..0090 (test coverage gaps)

---

## [0.1.2] - 2026-04-23

> **Not a version of this crate.** `version = "0.1.2"` never appeared in `Cargo.toml`; the
> crate went 0.1.1 → 0.2.0 directly. The changes below are real and are kept under their own
> heading for traceability, but they were carried by the 0.2.0 bump.

Post-v0.1.1 hardening pass, primarily driven by Review 0064. No public API break;
every change is a bug fix, rustdoc alignment, or decision record.

### Added

- **Content-identity `calculate_manifest_digest` on CRC-less formats.** TAR, TAR+gz/bz2/xz, and ISO now stream each entry through CRC32 via the new `entry_crc32_for_digest` helper; the `"{path}:{size}"` fallback is gone, so two archives with identical paths/sizes but different contents produce different digests (AD 0047). The Performance rustdoc now warns about the O(N²) cost on compressed TAR and points callers needing a summary to `calculate_archive_crc`.
- **`ExtractionOptions.filter` and `ExtractionLimits.max_mmap_size` honored.** Previously-dead public fields are now wired to the extraction path (R0064-0022..0025).
- **`ModificationOptions` fields honored.** `compression`, `preserve_metadata`, `create_backup`, and `backup_suffix` now affect `commit_changes` behavior end-to-end.

### Changed

- **Single-pass extraction per AD 0029.** `extract_all` dispatches through `extract_some` when a filter is supplied, removing the duplicate entry-iteration path (R0064-0004, R0064-0005).
- **`extract_file` rejects symlinks and hard-links per FR-022.** Single-file extraction now surfaces `SkippedSymlink` / `SkippedHardLink` warnings and returns without writing — consistent with `extract_all` policy (R0064-0009..0012).
- **`extract_file` materializes directory entries.** A directory entry in single-file mode now creates the target path via `mkdir`, matching the archive's manifest (R0064-0013..0015).
- **Selective extraction propagates warnings.** `extract_some` composes `ResultWithWarnings` from the backend's `extract_all` warnings instead of silently discarding them (R0064-0006..0008).
- **Rustdoc examples updated to consume `ResultWithWarnings<()>`.** `README.md`, `docs/USER_MANUAL.md`, `docs/GETTING_STARTED.md`, `docs/API_REFERENCE.md`, `CONTRIBUTING.md`, `src/lib.rs`, `src/extraction.rs`, and `examples/extract_archive.rs` now bind `extract_all`'s result and iterate warnings (R0064-0121..0130).
- **Doc sweep: `v0.1.0` → `v0.1.1`.** Version strings across the public doc tree realigned with the shipped release (R0064-0083..0120).

### Fixed

- **`open_at_offset()` preserves compound TAR extensions.** Embedded `.tar.gz` / `.tar.bz2` / `.tar.xz` / `.tar.zst` payloads staged during SFX / offset opening now carry the compound suffix through tempfile creation, so `ArchiveFormat::detect()` routes them to the TAR reader instead of the standalone compressor backend (R0064-0001..0003).
- **Size-sum overflow, calendar validation, and minor doc drift** tightened (R0064-0016..0021, various).
- **Install-guidance warnings converted to panics** where a missing dependency would cause a silent runtime failure; routine status messages silenced (pre-Review-0064 cleanup).

### Removed

- Dead `examples/test_extract.rs` and empty bench placeholders (R0064 cluster 1, 219a416).

### Decision records

- AD 0046 — REJECT R0064-0034..0039 (Windows support messaging downgrade).
- AD 0047 — Content-based `manifest_digest` on CRC-less formats.
- AD 0048 — Close OI-0057-007 against v0.1.x; SevenZ row handed off to DEF-004 / AD 0035. (archived under `docs/records/`)

### Notes

- OI-0057-007 (`commit_changes` retained-entry buffering) closed against v0.1.x. Remaining SevenZ source-streaming is upstream-blocked (sevenz-rust2 0.19.4 lacks an owned entry-level `Read`) and tracked as DEF-004 in `docs/project/stub-manifest.md`.

### Planned for v0.2.0
- Multi-part archive creation support (T058)
- Additional inline documentation examples (T112)
- Contract tests for API stability (T113-T115)
- Performance benchmarks for all operations (T116-T120, T127)
- Improved streaming extraction (true RAR streaming via callbacks)
- Windows platform comprehensive testing and fixes

### Under Consideration
- ISO format creation (currently read-only)
- Additional archive formats (ARJ, LZH, CAB)
- Archive comment support on non-ZIP formats (ZIP-level comment round-trips in v0.1.0)
- Extended attributes preservation
- Sparse file support

---

## [0.1.1] - 2026-04-19

Post-release hardening pass. No public API break; every change is either a
tightened internal invariant, a docs-alignment fix, or a reviewer-driven
safety gate.

### Added

- **Archive-internal path validation at the write-side facade.** `Archive::add_file_from_data`, `add_file_from_path_as`, `add_directory`, `add_entry`, `add_directory_entry`, and `remove_entry` now reject empty, NUL-containing, traversal (`..`), and absolute names up front with `ArchiveError::InvalidPath` instead of forwarding them to the backend and letting the extractor silently rewrite them on read-back (AD 0044, closes R0063-0001..0006).
- **Non-UTF-8 password rejection at the FFI boundary.** `CompressionOptions::password_as_str` now returns `Result<&str, ArchiveError::InvalidArgument>`; password-capable backends propagate the error instead of silently dropping or corrupting non-UTF-8 bytes (AD 0042).
- **SFX payload size ceiling.** `Archive::open_sfx` now caps embedded payload extraction at 16 GiB (AD 0040) to bound tempfile growth on hostile or malformed SFX binaries.

### Changed

- **Hermetic UnRAR build.** `build.rs` now stages UnRAR sources into `OUT_DIR` rather than compiling from the source tree, eliminating cross-crate-build working-tree contamination (AD 0039).
- **Dropped hardcoded `/Volumes/Temp/claude` preference from the library.** Tempfile selection now uses the system defaults exclusively; scratch-path preference is a test-harness concern, not a library concern (AD 0041).
- **Inspection contract asserts semantic stability, not pointer identity.** `contract_list_files_caching_same_pointer` → `contract_list_files_caching_stable`: repeated `list_files()` calls must return the same entries (same path / size / crc32), not necessarily the same backing slice.
- **SFX stage-3 rustdoc narrowed** and the misleading "CD signature" test-assertion message refreshed (Review 0062 closure).
- **Examples migrated to `tempfile::tempdir()`** and dropped stale SFX prose (Review 0062).
- **Doc alignment** across `Limitations.md`, `README.md`, `docs/GETTING_STARTED.md`, `docs/README.md`, `src/lib.rs`, and `src/inspection.rs` to match the shipped v0.1.0 capability surface (split-volume is RAR-only end-to-end, encrypted ZIP is read-only, `Archive::create()` rejects passwords, standalone `.gz/.bz2/.xz` are read-only).

### Fixed

- **DEF-001 closure recorded in the impact report.** The tempfile-backed `open_at_offset()` / `open_sfx()` implementation that shipped in v0.1.0 is now reflected in `docs/project/implementation-impact-report.md` so future readers don't re-open the gap.

### Decision records

- AD 0039 — Hermetic UnRAR build + staged artifact purge
- AD 0040 — SFX payload size ceiling (16 GiB)
- AD 0041 — Remove hardcoded temp path from library
- AD 0042 — `password_as_str` returns `Result` on non-UTF-8
- AD 0043 — Reject R0062-0007 (tempdir-security concern already resolved) (archived under `docs/records/`)
- AD 0044 — Validate archive-internal paths at the creation/modification facade boundary
- AD 0045 — Reject R0063 low-cluster doc-drift sweep; already covered by OI-0057-009 + post-v0.1.0 banners (archived under `docs/records/`)

---

## [0.1.0] - 2026-04-18

### Pre-release gap closure (2026-04-18)

Four targeted gaps closed before tagging v0.1.0:

- **`Archive::open_at_offset(path, offset)`** is now fully implemented for every backend (ZIP via an `OffsetReader` adapter; 7z / TAR / ISO / RAR via a tempfile slice). Closes DEF-001 and unblocks `Archive::open_sfx` end-to-end.
- **Options-aware memory/stream extraction**: `Archive::extract_to_memory_with_options` and `Archive::extract_to_stream_with_options` let callers pass `ExtractionLimits` through to non-libarchive backends (Piz, ZipReader, SevenZ, UnRAR). Closes OI-0057-005.
- **ZIP modification metadata preservation**: `commit_changes` now preserves the archive-level EOCD comment and each entry's compression method (Stored vs Deflated) when rewriting a ZIP source. Closes DEF-005 (quick-wins scope); remaining ZIP-specific caveats (ZIP64 >4 GiB, encrypted re-encryption, crash-recovery journaling) are documented in `Limitations.md`.
- **SevenZ source-side streaming**: investigated and deferred — sevenz-rust2 0.19.4 exposes no owned entry-level `Read`, so the buffered `extract_to_stream` adapter stays. Recorded as AD 0035; OI-0057-007 closes as partial with the SevenZ row deferred to DEF-004 pending upstream support.

### Documentation polish (2026-04-18)

- Added a public [user manual](./docs/USER_MANUAL.md) for `v0.1.0`
- Aligned `README.md`, `docs/README.md`, `docs/API_REFERENCE.md`, `docs/GETTING_STARTED.md`, and `Limitations.md` around the same capability story
- Corrected outdated claims around encrypted creation, standalone `.gz/.bz2/.xz` support, SFX opening, and split-volume support

### Added

#### Core Functionality
- **Archive Operations**
  - `Archive::open()` - Open archives with automatic format detection
  - `Archive::open_encrypted()` - Open password-protected archives
  - `Archive::list_files()` - List all files with rich metadata
  - `Archive::find_entry()` - Find specific files by path
  - `Archive::is_encrypted()` - Check for password protection
  - `Archive::validate_integrity()` - Validate CRC32 checksums
  - **NEW** `Archive::create()` - Create archives in ZIP, 7z, and TAR variants with configurable compression
  - **NEW** `Archive::modify()` - Modify existing archives (add/remove/replace files)
  - **NEW** `Archive::detect_sfx()` - Detect self-extracting archives across platforms
  - **NEW** `Archive::open_sfx()` / `Archive::open_at_offset()` - Open embedded archive payloads discovered inside self-extracting binaries

#### Extraction Features
- **Multiple Extraction Methods**
  - `extract_all()` - Extract all files with options
  - `extract_file()` - Extract single file by path
  - `extract_filtered()` - Extract files matching predicate with parallel execution
  - `extract_to_memory()` - Extract directly to memory
  - `extract_to_stream()` - Stream-based extraction for large files

- **Progress Tracking**
  - `ProgressCallback` trait for monitoring extraction
  - Cancellable extraction via `ControlFlow::Break`
  - Rate-limited callbacks to avoid overhead

- **Parallel Extraction**
  - Automatic parallelization for 4+ files using rayon
  - Thread-safe archive handles per worker
  - Efficient work stealing for balanced CPU utilization

#### Format Support
- **RAR/RAR5** (via UnRAR SDK)
  - Full read/extract support
  - Password-protected archives
  - Multi-part archives (automatic)
  - Direct CRC32 access from metadata
  - Encrypted header detection

- **ZIP**
  - Full read/extract support
  - Native ZIP creation
  - Encrypted reads via the `zip` crate backend

- **7z**
  - Full read/extract support
  - Native 7z inspection/extraction
  - 7z creation through the libarchive-backed creation path

- **TAR family / ISO / raw compressed formats**
  - TAR, TAR.GZ, TAR.BZ2, TAR.XZ, and ISO support via libarchive
  - Standalone `.gz`, `.bz2`, and `.xz` read/extract support via libarchive `format_raw`
  - TAR-family creation through libarchive

#### Metadata Access
- **Rich Entry Metadata**
  - File sizes (uncompressed and compressed)
  - CRC32 checksums (all formats)
  - Timestamps (modified, created, accessed)
  - File permissions (Unix mode bits)
  - Encryption status
  - Compression ratios
  - Entry types (File, Directory, Symlink, Other)

#### Performance Features
- **Memory Efficiency**
  - Streaming extraction <100MB memory for GB+ files
  - Chunk-based reading with configurable buffer sizes
  - Avoids full-file loads on libarchive-backed streaming paths; some native backends still buffer entries

- **SIMD Acceleration**
  - CRC32 computation using `crc32fast` (~300MB/s)
  - Optimized for modern CPU architectures

- **Thread Safety**
  - Parallel extraction with per-thread archive handles
  - Unique timestamp-based temporary directories
  - No global state or race conditions in callers; UnRAR's internal global state is mediated by an internal `UNRAR_LOCK` mutex so concurrent callers on different RAR archives are safe. Per-archive extraction still runs sequentially due to the SDK's iterator shape.

#### Error Handling
- **Comprehensive Error Types**
  - `ArchiveError::Io` - Missing files
  - `ArchiveError::Unsupported` - Unknown formats
  - `ArchiveError::Password` - Password issues
  - `ArchiveError::Corruption` - CRC32 failures
  - `ArchiveError::Io` - I/O errors with context
  - `ArchiveError::Format` - Format-specific errors

- **Automatic CRC32 Verification**
  - Enabled by default during extraction
  - Detects corruption immediately
  - Clear error messages with file paths

#### Developer Experience
- **Examples**
  - `inspect_archive.rs` - Archive inspection demo
  - `extract_archive.rs` - Extraction with progress
  - `streaming_extract.rs` - Memory-efficient extraction
  - `test_extract.rs` - Simple extraction example
  - `detect_sfx.rs` - SFX detection and embedded payload opening

- **Documentation**
  - Comprehensive README with usage examples
  - API reference documentation
  - Inline code documentation
  - Known limitations documented

- **Testing**
  - Comprehensive test suite covering unit, integration, contract, and doc tests
  - Performance benchmarks
  - Property-based tests with proptest
  - Multi-format test fixtures

### Fixed

#### Thread Safety Issues
- Fixed parallel test failures caused by `chdir()` usage
- Implemented absolute path handling for extraction
- Added nanosecond timestamps to temporary directories
- Eliminated race conditions in cleanup operations

#### CRC32 Computation
- Implemented CRC32 computation for ZIP/7z/TAR formats
- Fixed libarchive backend to read and hash file data during listing
- Ensured consistent CRC32 availability across all formats

#### Documentation
- Updated README to reflect current implementation status
- Corrected format support table
- Added missing error handling examples
- Documented all limitations and workarounds
- Added a consolidated public user manual

### Changed

#### Project Naming
- Renamed from "7zip-RBinding" to "unified-archive"
- Updated to reflect unified interface philosophy
- Consistent branding across documentation

#### API Design
- `ExtractionOptions` uses builder pattern with `..Default::default()`
- Progress callbacks use `std::ops::ControlFlow` for cancellation
- Consistent `Result<T>` return types across all methods

#### Backend Architecture
- Piz (ZIP read), zip crate (encrypted ZIP read + ZIP creation), SevenZ (7z read), libarchive (TAR family creation/read, ISO, standalone `.gz`/`.bz2`/`.xz` read), UnRAR (RAR/RAR5)
- Automatic backend selection based on format detection
- Thread-local archive handles for parallel operations

### Performance

- **CRC32**: ~300MB/s using SIMD-accelerated hashing
- **Parallel Extraction**: Linear scaling up to available CPU cores
- **Memory Usage**: <100MB for multi-GB files with streaming (Note: This bound currently applies to libarchive-backed formats (TAR family). ZIP, 7z, and RAR backends buffer full entries in memory during streaming extraction.)
- **Format Detection**: O(1) single header read

### Technical Details

#### Dependencies
- `crc32fast` 1.4 - SIMD-accelerated CRC32
- `rayon` 1.8 - Parallel extraction
- `secstr` 0.5 - Secure password storage
- `once_cell` 1.20 - Entry caching
- `libarchive` (system) - Multi-format support
- `UnRAR SDK` (bundled) - RAR/RAR5 support

#### Platform Support
- ✅ macOS (tested on Darwin 24.6.0)
- ✅ Linux (Ubuntu/Debian, Fedora/RHEL)
- ⏳ Windows (untested, may need adjustments)

#### Rust Version
- Minimum: Rust 1.85+ (stable)
- Edition: 2024

### Security

- Passwords are stored as `Option<SecStr>` (via the `secstr` crate). Password bytes are zeroed on drop; decoding to `&str` happens only at the FFI boundary.
- No password leakage in error messages or logs
- Secure temporary file creation with unique names
- Permission preservation for extracted files

### Known Limitations

See [Limitations.md](./Limitations.md) for complete details.

**Key Limitations in v0.1.0:**
- `Archive::create()` rejects password-based encrypted creation for every format
- Split-volume support is limited to RAR / RAR5
- Progress callbacks during creation report per-entry events with `total = None` (file count not pre-computed)
- RAR format is read-only through the main `Archive` facade
- Windows support is not release-verified yet
- Some workflows intentionally use temporary files (`open_at_offset`, UnRAR `extract_to_memory`)
- SFX detection has substantial unit and integration test coverage including synthetic PE/ELF/Mach-O/script stubs, signature scanning, and false-positive tests

### Migration Notes

This is the initial release (v0.1.0), no migration needed.

Future breaking changes will follow semantic versioning:
- Major version (2.0.0) for breaking API changes
- Minor version (0.2.0) for new features
- Patch version (0.1.1) for bug fixes

---

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md with release date, and add the new section at the *top* of the version
   list so the file stays newest-first
3. Run full test suite: `cargo test --release`
4. Tag release: `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
5. Push tag: `git push origin vX.Y.Z`
6. Publish to crates.io: `cargo publish`

Steps 1–2 have run ahead of steps 4–6 before: `0.2.0` and `0.3.0` were stamped in `Cargo.toml`
without ever being tagged, and step 6 has never been performed for any version. Bump the
version only as part of a release that completes the list, or the record stops describing
what exists.

---

**Legend:**
- Added: New features
- Changed: Changes in existing functionality
- Deprecated: Soon-to-be removed features
- Removed: Removed features
- Fixed: Bug fixes
- Security: Security improvements
