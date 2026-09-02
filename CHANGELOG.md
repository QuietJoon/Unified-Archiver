# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Release status.** Versions are listed newest-first. Three git tags exist — `v0.1.0`,
> `v0.1.1`, and `v0.4.0` — and **no version of this crate has ever been published to a public
> registry.** (`v0.3.1` was tagged and then deleted; `0.3.1` keeps its section below because
> the changes it names are all in history.) Two of the headings below therefore do not mean what a reader would assume:
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

## [Unreleased]

> **This will be 0.5.0, and the bump is forced.** Two independent reasons: seven public structs
> became `#[non_exhaustive]`, and several operations changed which `ArchiveError` variant they
> return.
> 0.5.0 is not a number chosen here — every deprecation-timeline block already in the tree names
> 0.5.0 as the next stop, and the three `#[deprecated(since = …)]` attributes below are set to it.
>
> `Cargo.toml` still reads `0.4.0` and is deliberately left alone. Bumping it before the tag is
> what produced the phantom versions the header above has to apologise for, and one item is still
> owed before 0.5.0 can honestly be cut — see **Still owed**.
>
> **How to read the Breaking list.** Three kinds of break, and they cost a caller different amounts
> of work. **One signature change:** `recovery_percentage()` returns `Option<u16>` where it returned
> `Option<u8>`, so a `u8` binding has to widen. **Construction breaks:** the `#[non_exhaustive]`
> types can no longer be built with a struct literal from outside the crate — including the
> `..Default::default()` form, which is not an escape hatch. **The rest are behavioural** — calls
> that returned `Ok` now return `Err`, or return a different `ArchiveError` variant, so a caller
> matching on variants is the one who has to act.
>
> *(This paragraph used to claim "nothing here changes a signature". That was true when written and
> stopped being true as the window filled; it is corrected rather than quietly dropped, because a
> reader who trusted it would have skipped the one break that does not announce itself at the call
> site.)*

### Breaking

- **The exact-content-total APIs return a typed `SizedContentTotal`, not a bare `u64`**
  (OI-0001-007). `Archive::calculate_content_multiset_digest_and_size`, its
  `calculate_manifest_summary` shim, and both `ReadArchive` mirrors now return
  `Result<(String, SizedContentTotal)>`.

  The `u64` was never an exact total. It accumulated only from entries whose listing declares a
  size, and an entry that declares none contributed **zero with nothing in the return to say so**.
  Two real populations declare no size: raw single-file `.gz` / `.bz2` / `.xz` members, and
  libarchive entries whose `archive_entry_size_is_set` is false. Digesting a raw `.gz` handle
  therefore returned a confident `0` — indistinguishable from an empty archive's honest `0`.

  `SizedContentTotal` carries the partial sum *and* its coverage: `sized_bytes()` is the old number
  (unknown counted as zero, now something a caller asks for on purpose), `exact()` returns
  `Some(total)` only when every file entry was sized, and `is_complete()` / `sized_entries()` /
  `unsized_entries()` / `file_entries()` make "sized 3 of 5" expressible. A struct rather than
  `Option<u64>` so the partial total survives — a caller who can size three of five entries can
  still act on it — and rather than a tuple so the two numbers are named. `Display` spells out an
  incomplete total, so printing one cannot pass it off as complete.

  *Migration is mechanical and the compiler finds every site:* `.sized_bytes()` restores the
  previous value byte for byte; `.exact()` is the one to reach for wherever the number was treated
  as the archive's true uncompressed size. `calculate_manifest_summary` moved with the primary
  rather than keeping `(String, u64)` for source-compat, and the reason is recorded on the shim: the
  only `u64` it could return is the partial sum, so source-compat there would have preserved a
  silently short number inside a method that promises a summary. A compile error is found once by a
  developer; a wrong figure is found by a user.

- **`Archive::modify()` on a RAR archive reports a different reason string.** Was
  `"RAR archives are read-only"`; now the shared gate's `"Rar archives do not support
  modification"`. The dedicated RAR arm was unreachable — `ArchiveFormat::can_modify()` refuses RAR
  earlier — so the two messages could only ever disagree in wording. Callers matching the variant or
  the `operation: "modify"` label are unaffected; only a match on the reason text breaks.

### Added

- **`Archive::volume_set_report()` and `ReadArchive::volume_set_report()`** — the typed volume-set
  report, including what is *wrong* with the set (OI-0080-004). `detect_multipart` and
  `multipart_layout` both reduce a set to a list of paths, and a list cannot express "and a fourth
  volume is missing between these three": a hole arrives as a shorter list, indistinguishable from a
  smaller set. Holes, duplicated volume numbers and foreign siblings arrive here as typed
  `VolumeSetDefect`s.

  Nothing is recomputed to provide it. The parser already produced a report on every
  `detect_multipart` call and the tuple discarded it, which is why this is purely additive.
  `MultipartLayout` is deliberately left alone — widening it would break a type callers match on,
  and a caller who does not care about defects should not have to.

### Removed

- Two unreachable dispatch arms, deleted after a per-site review rather than a sweep (ticgit
  9909d449): the RAR arm in `Archive::modify`'s backend selection, and the `Lzma` arm in the
  internal `promote_to_compound_tar` helper — `detect_from_bytes` has no LZMA probe and cannot
  return `Lzma`, while `.tar.lzma` / `.tlz` are resolved by the extension fallback and never routed
  through that arm. No detection or extraction behaviour changes.

  **Two other unreachable sites were deliberately KEPT**, which is the point of ruling per site: the
  `MAX_STUB_SIZE` guard in `extract_stub` and the ZIP arms in the libarchive writer. Unreachable
  code in an FFI wrapper is not automatically waste — deleting a defensive branch and deleting a
  genuine invariant look identical from outside. Both now document why they stay and what would make
  them reachable again. `unreachable!()` was rejected for the same reason: it trades a typed refusal
  for a panic.


- **`Archive::recovery_percentage()` returns `Option<u16>`, not `Option<u8>`.** The number does not
  fit a byte: RAR 6.10 raised the maximum recovery record from 99% to 1000% and changed the RAR5
  encoding to a vint, so `rar -rr256p` through `-rr1000p` produce ordinary, readable archives. What
  the old signature did with them was neither truncate nor error — the `u8::try_from` failed and the
  caller was handed `Ok(None)`, *"the record is there, its percentage cannot be determined"*, for a
  value sitting in plain sight in the header. Returning an `Err` above 255 was considered and
  rejected for the same reason: it turns a sound archive into a damage report and still does not
  tell you the number. Taken now because the 0.5.0 window is open and the alternative is a major
  bump of its own later. *Migration:* widen the binding — `let pct: u8 = …` becomes `u16`; a `match`
  on `Some(pct)` that only prints it needs nothing. `ReadArchive::recovery_percentage` moves with
  it.

  *What it does not buy.* Nothing on the RAR4 path: RAR4 stores no percentage field at all — the
  value is derived from recovery/total block counts and capped at 100 — so a RAR4 archive still
  cannot report more than 100, and the wider type is representational there. And the range is
  bounded rather than open: the extra-area value is a `u64` vint, so anything past `u16::MAX` is a
  malformed record and stays on the `None` branch, exactly where 256 used to sit. The
  header-encrypted case is unchanged and is now stated in the contract rather than only in a
  comment: a `-hp` archive still answers `has_recovery_record() == true` with
  `recovery_percentage() == None`, because the password-less byte walk may not read the block the
  percentage lives in.

- **`ArchiveError::Password` now says what it cannot tell you.** Not a code change — a contract that
  was always true and never written down. On an encrypted **7z** entry the variant means *wrong
  password **or** damaged payload*: the backend rewrites a decoder read error or a CRC/size mismatch
  to `Password` whenever the entry is encrypted and a password was supplied, so matching on the
  typed error cannot separate a bad passphrase from bad media. That is 7z AES-256's doing rather
  than the crate's — the format carries neither an authentication tag nor a password-verification
  value, so a wrong key decrypts to plausible garbage that fails the downstream LZMA/CRC checks
  exactly as damaged ciphertext does. Nothing is being withheld; at that point the distinction does
  not exist.

  The documentation now scopes it per backend rather than crate-wide, which matters because the
  answer differs: RAR5's per-file password check value and ZIP's AES/ZipCrypto verification bytes do
  keep the two apart, with legacy RAR3 called out as the format that does not. It also says what a
  caller can actually do — retry with the password believed correct and treat a second failure as
  most likely damage — and warns that `validate_integrity` is **not** a tiebreaker: it decodes the
  same ciphertext with the same key, so it tells you *scope* (unencrypted entries validate
  regardless of the password) and nothing more, and on an all-encrypted archive not even that.

  **There is deliberately no typed ambiguity marker** (OI-0080-007 item 1). No `PasswordOrCorruption`
  variant, no `Password { ambiguous: true }`: either would advertise a distinction the format cannot
  supply, implying the backend could do better with more work. `ArchiveError` is `#[non_exhaustive]`,
  so adding such a shape later is not a breaking change if a format-level signal ever makes the
  distinction real — which is exactly why documenting it first costs nothing.

- **`ArchiveEntry` is `#[non_exhaustive]`.** External code can no longer build it with a struct
  literal or destructure/match it exhaustively; both now fail to compile. The fields stay `pub`
  and readable, so field *access* is unaffected. *Migration:* construct through
  `ArchiveEntry::file(path, id)` / `dir_at` / `symlink_at` / `hardlink_at` and finish with
  `.build()` — or `.build_checked()`, which refuses an empty path and any combination that
  contradicts the entry's own kind, such as a directory carrying a size — and add `..` to any
  pattern that matched every field. The struct is a *listing* type whose field set tracks what archive formats
  carry, so every metadata addition so far has been a silent break for anyone holding a literal;
  the attribute converts that into a compile error at the one place it can be fixed.

- **Five more public structs are `#[non_exhaustive]`: `CompressionOptions`,
  `ModificationOptions`, `ValidationReport`, `ResultWithWarnings<T>` and `StreamChecksum`.**
  External code can no longer build any of them with a struct literal, and can no longer match one
  with an exhaustive pattern. The compile errors are `E0639` ("cannot create non-exhaustive struct
  using struct expression") for the literal and `E0638` ("`..` required with struct marked as
  non-exhaustive") for the pattern. **`..Default::default()` is not a workaround** — verified by
  a two-crate `rustc` experiment, the functional-update form raises the same `E0639`, so
  `CompressionOptions { level: Fast, ..Default::default() }` stops compiling exactly like the
  spelled-out literal. A bare `Default::default()` call, with no literal around it, is unaffected;
  `CompressionOptions` and `ModificationOptions` have a `Default` impl, the other three do not.
  Field *reads* and field *assignment* (`opts.level = …`) still compile: the fields stay `pub`.

  *Migration, per type:*
  - `CompressionOptions` — `CompressionOptions::for_writable(WritableFormat::ZIP)` when the format
    is a literal, `::try_new(format)` when it is computed, then assign the rest
    (`opts.level = CompressionLevel::Fast`). Or move to a typed builder —
    `ZipCompressionOptions` / `SevenZCompressionOptions` / `LibarchiveCompressionOptions` with
    `Archive::create_zip` / `create_seven_zip` / `create_libarchive`.
  - `ModificationOptions` — `ModificationOptions::new()` (or `Default::default()`), then
    `.with_backup(suffix)` / `.without_metadata_preservation()`, or assign the field.
  - `ResultWithWarnings<T>` — `ResultWithWarnings::ok(value)` or `::with_warnings(value, warnings)`;
    between them they set both fields, so nothing is unreachable.
  - `ValidationReport` and `StreamChecksum` — no constructor, and none is owed: they are output
    types, returned by `Archive::validate_integrity` and by `extract_stream_checksum` and its
    per-codec siblings. If you were fabricating one in a test, hold a real one instead.

  *What this does and does not buy.* It buys exactly one thing: a field can be added to any of the
  five without a major bump. It enforces **no invariant** — the fields are still `pub` and still
  assignable, so a value can still be mutated into a state the crate rejects.
  `CompressionOptions::validate_for_format()` remains the check that catches that, and
  `Archive::create` and the `commit_changes` path still call it. What is given up is *type-state*
  (failing at construction instead of at the call), not safety. `ExtractionLimits` — the type whose
  numeric invariants were the dangerous ones — has had private fields and a builder since
  Innovation I1 and is unaffected.

  *Not in this change:* `ExtractionOptions` was deliberately left out of these five
  (OI-0076-005), and `CompressionOptions` keeps its `pub` fields — demoting them needs a
  `level`/`progress` setter pair that does not exist yet, and demoting without it would leave both
  unreachable from outside the crate. **Superseded on the `ExtractionOptions` half:** the entry
  immediately below closes that deferral, so read the two together — "not in this change" is a
  statement about the commit that landed the five, not a promise about 0.5.0. The
  `CompressionOptions` half stands: the setter pair still does not exist.

- **`ExtractionOptions` is `#[non_exhaustive]` too — the seventh struct in this release, and the
  one the entry above deferred.** External code can no longer build one with a struct literal; the
  error is the same `E0639` ("cannot create non-exhaustive struct using struct expression").
  **`..Default::default()` is not an escape hatch here either, and this is the type where that
  hurts.** The functional-update
  form is refused by the *syntax*, whatever the base expression is, so both
  `ExtractionOptions { destination: dest, overwrite: true, ..Default::default() }` and
  `ExtractionOptions { overwrite: true, ..some_base() }` stop compiling. `..Default::default()` was
  the idiomatic in-tree spelling for this type, which is exactly why it was split out of the group
  of five and given a migration of its own rather than a line in a shared list. Field *reads*
  (`opts.overwrite`) and field *assignment* (`opts.overwrite = true`) still compile: the fields stay
  `pub`. `Default` is untouched — a bare `ExtractionOptions::default()`, with no literal around it,
  still compiles outside the crate, and its `destination` is still `PathBuf::from(".")`.

  *Migration.* Unlike the other five, this one did get a constructor, because it needed one.
  `ExtractionOptions::new(destination)` yields the defaults with the destination filled in, and
  every remaining field has a consuming setter that chains off it, in the style of the pre-existing
  `.password(…)`: `.overwrite(bool)`, `.preserve_permissions(bool)`, `.preserve_times(bool)`,
  `.verify_crc32(bool)`, `.limits(ExtractionLimits)`, `.filter(…)` and `.progress(…)`. The last two
  box internally — pass the closure or the callback itself, with no `Some` and no `Box::new` around
  it. *The common shape, before and after:*
  `ExtractionOptions { destination: dest, overwrite: true, ..Default::default() }` becomes
  `ExtractionOptions::new(&dest).overwrite(true)`.

  *Two cases stay field assignment, and no setter is owed for either.* An **already-boxed**
  `Box<dyn ProgressCallback>` does not satisfy `.progress(impl ProgressCallback + 'static)` —
  there is no impl of the trait for the box — so hold a `let mut` binding and write
  `opts.progress = Some(the_box);`. And **clearing an `Option` back to `None`** has no setter
  spelling at all: `opts.progress = None;`, `opts.filter = None;`. A *plain closure* needs neither
  workaround and should go through `.progress(…)`: `ProgressCallback` has a blanket impl for
  `FnMut(u64, Option<u64>) -> ControlFlow<()> + Send`.

  *What it buys is the same thing the other five bought* — a field can be added to
  `ExtractionOptions` without a major bump — and it enforces no invariant, since the nine fields
  remain `pub` and assignable. `ExtractionLimits`, reachable through `.limits(…)`, is still the
  place the numeric invariants are enforced, and it has had private fields and a builder since
  Innovation I1.

- **Creating or modifying an archive now refuses entry names it used to accept.** This is the
  break most likely to bite, and it has a case with no escape hatch, so it is worth reading in
  full. The archive-internal path validator delegated to `Path::components()`, and std eats
  segments the validator was written to catch — verified by experiment: `a/./b` and `a/b/.` both
  come back as `[a, b]`, so an interior or trailing `.` was invisible, and on Unix `C:/x` comes
  back as `[C:, x]`, so a drive prefix passed as an ordinary segment. The check now splits on `/`
  and judges each segment itself.
  - Refused **regardless of policy**: `.` and `..` in *any* position, absolute prefixes, NUL
    bytes, empty strings and empty segments.
  - Refused additionally under the new default `ArchivePathPolicy::Portable`: Windows reserved
    device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, case-insensitive, with
    or without an extension), segments ending in `.` or a space, drive-letter prefixes, and `:`
    anywhere in a segment.

  It applies on the **write side only** — `Archive::create*` and the `commit_changes` path — not
  to extraction. And it applies to *retained* entries too: `commit_changes` re-validates every
  pre-existing entry it is about to re-emit and refuses the whole commit with `OperationBlocked`
  if any fails, before creating a temp file. So **an archive written by another tool that contains
  such a name can no longer be modified at all.** *Migration:* `set_archive_path_policy(
  ArchivePathPolicy::Host)` relaxes the portable-only rules for callers who deliberately want host
  grammar — but it does **not** relax `.` and `..`, so an archive carrying `a/./b` cannot be
  modified under either policy. For that case, rename the entry through the modification API
  instead of retaining it, or extract and recreate.

- **`ExtractionLimits::max_sfx_payload_size` is enforced now, where before it was ignored.** In
  0.4.0 the field was stored, defaulted and readable, and no staging code consulted it: the
  SFX/offset staging copy was bounded by `sfx::limits::MAX_SFX_PAYLOAD_SIZE`, a hardwired alias of
  the 16 GiB default, whatever the caller configured. The caller's value now governs, with
  consequences in both directions — set **below** 16 GiB it was silently ignored and is now
  honoured, so a staging open that used to succeed can fail; set **above** 16 GiB it was silently
  capped and now is not.

- **That ceiling does not bind on an in-place open, by design.** It bounds a *copy*; where the
  backend reads the archive straight out of the file the caller named there is no copy and nothing
  for the ceiling to bound (AD 0040, amended). It is therefore not a file-size guard and must not
  be used as one. *To tell the two apart:* read the new
  `Archive::payload_access() -> PayloadAccess`. `Staged` is exactly the case the ceiling applied
  to — and the case that consumed temp-volume space — and `InPlace` is exactly the case it did
  not. Budget off that, not off the limit value.

- **A declared-versus-actual length mismatch is `ArchiveError::Corruption` on all three commit
  routes.** It promised `Corruption` and delivered two other variants depending on which backend
  committed: ZIP raised `Format` ("over-produced" / "under-produced"), libarchive raised `Io`
  ("Stream length mismatch"). Detection was never broken, classification was — so
  `match err { Corruption { .. } => … }` fired on no backend at all. All three routes now call one
  `ArchiveError::declared_length_mismatch(path, declared, actual)`, which carries both numbers and
  says which direction it went. *Migration:* stop matching on `Format`/`Io` for this condition, and
  stop matching on the `"Stream length mismatch"` string — the message is gone.

- **An encrypted ZIP entry opened without a password is `ArchiveError::Password`, not `Format`.**
  RAR already yielded `Password` (via `ERAR_MISSING_PASSWORD`) and 7z already did (via
  `Error::PasswordRequired`), and `docs/API_REFERENCE.md` promised `Password`, so ZIP was the lone
  outlier. Only the missing-credential condition moved: a *wrong* password and a malformed entry
  keep the variants they had.

- **A libarchive build without the zstd / lz4 / lzma write filter now raises
  `ArchiveError::CodecUnavailable`, not a generic `Format`.** The variant, its constructor and a
  per-platform install-instruction table all existed and no production path constructed them. The
  write-filter registration site raises it now, naming the codec, so `codec_install_instructions`
  finally reaches the caller it was written for. The lookup also stopped missing: it matches on an
  upper-cased key, so libarchive's lower-case filter names (`xz`, `lzma`, `bzip2`) reach their real
  per-platform arm instead of the generic fallback.

- **Truncation on a short compressed stream is `Format`, not `Io`.** Concretely: a bzip2 stream
  under 4 bytes or an xz stream under 12 bytes, the latter reachable through
  `extract_stream_checksum`. The per-format helpers mapped every read failure including
  `UnexpectedEof` to `Io` while the auto-detect probe already reported the same truncation as
  `Format`. The asymmetry is resolved toward `Format` rather than `Io`, and the reasoning is the
  point: `Io` is the class callers treat as operational and retryable, so labelling a permanently
  short file `Io` invites a retry that can never succeed. One `framing_read_error` now decides for
  all six production call sites — two each in `gzip` and `bzip2`, one each in `xz` and `detect`;
  every other `io::ErrorKind` keeps
  `Io` with its operation label and source. AD-0010, *Unified Stream Checksum Extraction*,
  carries the dated amendment under the heading "truncation is a `Format` error at every layer".

- **Not a break, contrary to how it was recorded.** `ArchiveWarning` gained the
  `SkippedUnsupportedEntry` variant, and `b778c2a`'s footer called that breaking. It is not:
  `ArchiveWarning` and `ArchiveError` are both already `#[non_exhaustive]`, so a downstream match
  has an unreachable arm and cannot be broken by a new variant. Noted here so nobody plans a
  migration around it.

### Added

- **`ArchiveWarning::BackendAdvisory { backend, operation, message }`** and
  **`Archive::take_backend_warnings()`** — the channel for text a backend recovered from but
  wanted to report. Adding the variant is not a break: `ArchiveWarning` is already
  `#[non_exhaustive]`, so external matches carry a `_` arm.

  Unlike every other variant this one carries unstructured third-party text, deliberately.
  libarchive's `ARCHIVE_WARN` is a recoverable status whose only detail is a free-form English
  string from the linked library — not a vendored one, so its wording is version- and
  format-dependent and cannot be parsed into fields without inventing a taxonomy libarchive does
  not have. Do not match on `message`; show it.

  `extract_all` appends these to the `Vec<ArchiveWarning>` it already returns, so the common path
  needs no new call. The other reads — `list_files`, `extract_file`, `extract_to_memory`,
  `validate_integrity` — return through signatures with no warning channel, several of them shared
  `ArchiveBackend` trait methods that the ZIP, 7z and UnRAR backends also implement and that never
  produce this condition; `take_backend_warnings()` reaches those without forcing a channel onto
  three backends that would never use it. It returns an empty vector for those backends, and
  `ReadArchive` forwards it under `v2-api`.

- **`scripts/generate-rar-fixtures.sh`** — the RAR fixtures this crate cannot produce itself.
  RAR creation exists only behind `cfg(all(target_os = "windows", feature =
  "external-rar-create"))`, so a recovery-record fixture, an encrypted-plus-recovery fixture and
  a multi-volume set all have to come from the proprietary RARLAB `rar` CLI. The script is
  deliberately outside the build and must stay there: `rar` is not a build dependency, and on
  macOS every invocation is stalled by `syspolicyd`. Run it by hand, commit the bytes, and let
  the suite consume the committed bytes. It skips fixtures that already exist unless given
  `--force`, and verifies each result with `unrar t`.

  New fixtures: `test_recovery.rar` (regenerated with a genuine 5% record),
  `test_encrypted_data_recovery.rar`, `test_encrypted_recovery.rar`, and
  `test_multivol.part{1,2,3}.rar`. `tests/fixtures/README.md` documents each one, its generation
  command, and what it is for.

- **`tests/rar_multivolume_test.rs`** — the end-to-end half of ticgit d3cfce's acceptance
  criteria, which could not be written until a genuine volume set existed. It pins that a set
  missing its middle volume fails *bounded* rather than retrying the absent name forever under
  the process-wide UnRAR lock, and records why the typed `MissingVolume` diagnostic does not
  reach the caller on that path: the vendored SDK's `DllVolChange` maps both "our callback
  aborted" and "no callback installed" to `ERAR_EOPEN`, and this crate registers its callback per
  operation, so nothing is registered when UnRAR asks. Boundedness there comes from the SDK's own
  no-callback branch (ticgit 03ddc6).

- **`Archive::validate()`** — asks whether the handle refers to a usable archive, for the cost of
  the first parse and no more. `Archive::open` parses nothing on most backends, so `Ok` from
  `open()` never meant the input was a valid archive; a corrupt ZIP opened fine and failed later,
  at whatever call first needed the contents. `validate()` forces that parse and reports the
  verdict, and because AD 0065's frozen listing cache keeps the result, a later `list_files` is
  served from it rather than parsed again — that is what makes this cheap rather than a second
  pass, and it is pinned by a test asserting three pointer identities (same `Vec`, same `Arc`, and
  both cache layers empty after `open`). Returns the format error for a corrupt, truncated,
  misdetected or missing input, and `ArchiveError::WriteModeOnly` for a write-mode handle, where
  there is nothing to validate until `finish` has run. This makes first-operation validation a
  stated contract instead of an accident of when parsing happens; libarchive stays the eager
  exception it has been since R0001-0012. Distinct from `validate_integrity()`, which is a full
  per-entry walk — a different cost class and a different question.
- **`security`: an archive-path policy** — `ArchivePathPolicy::{Portable, Host}`,
  `set_archive_path_policy`, `archive_path_policy`, `with_archive_path_policy` (scoped, for tests)
  and `validate_archive_internal_path_as`, which checks against an explicit policy rather than the
  ambient one. See the Breaking entry above for what each policy refuses.
- **`options`: `WritableFormat`** — a newtype that can only hold a format this crate can create,
  with checked construction and per-format constants (`ZIP`, `SEVEN_ZIP`, `TAR`, `TAR_GZIP`,
  `TAR_BZIP2`, `TAR_XZ`, `TAR_ZST`, `TAR_LZ4`, `TAR_LZMA`, `ALL`). `CompressionOptions` and
  `LibarchiveCompressionOptions` both gained `for_writable(WritableFormat)` — unrejectable by
  construction — and `try_new(ArchiveFormat)` for a format known only at runtime, so a
  non-creatable format is refused where it is written rather than carried to the create call. Also
  `password_ref`, `split_size`, `has_progress`.
- **`format::multipart`** — a typed parser for split/multi-volume sets: `VolumeSet`, `Volume`,
  `VolumeName`, `VolumeScheme`, `VolumeSetDefect`, `VolumeSetReport`, with `parse_volume_name`,
  `parse_volume_set` and `parse_volume_set_for`. The queries split across two types, which is worth
  stating because an earlier draft of this entry ran them together: `VolumeSet` answers `paths`,
  `to_layout` and `expected_name`; `VolumeSetReport` — what both parse functions actually return —
  answers `set`, `is_complete` and `defects`. `defects` returns a list rather than a bool on purpose:
  a caller who cannot open a set needs to know *which* volume is missing, and may need to know about
  more than one gap.
- **`entry`**: `build_checked` on the builder — the fallible finish that enforces the entry-kind
  invariants `build` cannot — plus `symlink_at` / `try_symlink_at` / `hardlink_at` /
  `try_hardlink_at`, and the `entry_type`, `id`, `path`, `is_encrypted` accessors — the read path
  that `#[non_exhaustive]` makes the supported one.
- **`error`**: `EntrySkipReason` and `UnsupportedEntryKind` (with `from_unix_mode`, which returns
  `None` for a mode carrying no format bits rather than guessing), the
  `ArchiveError::declared_length_mismatch` constructor, the
  `ArchiveWarning::{skipped_unsupported_entry, dropped_unsupported_entry}` constructors, and
  `codec_install_instructions`.
- **`archive`**: `open_sfx_with_limits`, `open_with_sfx_progress_and_limits` and
  `open_at_offset_with_limits` — the limits-taking forms of the three SFX/offset opens, which is
  how a caller's `max_sfx_payload_size` reaches the staging copy. The three limits-free entry
  points keep the previous default, so they are unaffected. Plus `PayloadAccess` and
  `payload_access`.
- **Crate-root re-exports**: `ArchiveWarning`, `ResultWithWarnings`, `RateLimiter` and
  `WritableFormat`. The first two appear in public `Archive` method signatures, so callers were
  reaching into `unified_archive::error` for types the facade hands them; the last was the argument
  type of a root-level constructor that lived one module down.
- **`external::rar` is now seven child modules** — `argv`, `discovery`, `error`, `exit`, `runner`,
  `session`, `version` — where it was one file with everything private. **Read the gate before
  reading the list:** `pub mod external` is behind
  `#[cfg(all(target_os = "windows", feature = "external-rar-create"))]`, and
  `external-rar-create` is *not* a default feature, so for every default-feature consumer and
  every non-Windows consumer none of this is public API. Where it does exist it offers the
  `CommandRunner` trait and `SystemRunner`, so a caller can supply their own process launcher (job
  objects, sandboxes, custom timeouts); `redact_argv` and `REDACTED_PASSWORD_ARG`, so a password
  never reaches a log or an error message; `preview_argv`, so the exact argv can be displayed
  before it runs, with no un-redacted accessor by design; `RarExit`; `RarCliError` with
  `remediation`, `is_binary_unavailable`, `is_binary_unusable` and `is_password_failure` — binary
  absent and binary too old are distinct, because a caller handling the first installs something
  and the second upgrades it; `RarVersion` / `RarFlavor` / `parse_banner`; and the discovery
  surface (`find_rar_binary`, `vet_program`, `external_rar_supported`,
  `SHELL_INTERPRETED_EXTENSIONS`).
  Three of those types are `#[non_exhaustive]`, so adding a field to one later is not a break:
  `CommandOutcome` and `RarVersion` already had total constructors, and `RarBanner` gained one
  (`RarBanner::new`) alongside the attribute rather than being closed without a way in.
  `AddArgv` — the request type of `argv::build`, `session::create_archive` and
  `session::preview_argv` alike — is deliberately left open and says so in its own doc: it is a
  caller-constructed input with no builder, so closing it would leave an external caller unable to
  build one at all. (Earlier in this same cycle `session` also carried a field-for-field identical
  `CreateRequest`; it was collapsed into `AddArgv` before release — see **Changed** — so it is not
  part of this surface and never shipped.)
  `StubRunner`, `StubResponse` and `StubCall` are **not** part of
  that surface — they are `#[cfg(test)]`, "a seam, not public surface" as the module puts it. The
  contract tests reach them only by compiling the source files into their own binary with
  `#[path]`, which turns `cfg(test)` back on locally; a downstream crate cannot do that and should
  write its own `CommandRunner` double.

### Changed

- **`Archive::detect_multipart` no longer keeps its own idea of what a volume name is, and
  opening a continuation volume now finds the set.** The method carried ~270 lines of hand-rolled
  matching — four boundary closures, four hoisted source predicates and a bespoke numeric sort —
  a few hundred lines away from `format::multipart`, which has done the same job as a typed
  parser since it landed as additive API with no in-crate consumer. Two implementations of "is
  this a volume name" is one too many, and they were already drifting. The matcher is gone: the
  method collects the sibling names exactly as before and hands them to `parse_volume_set_for`,
  anchored on the archive that was opened.

  **One behavioural consequence, and it is the point of the change.** Old-style RAR sibling
  matching was gated on the *source* name ending in `.rar`, so opening `archive.r00` reported
  `(false, [archive.r00])` while opening `archive.rar` beside it discovered the whole series —
  the same set on disk, a different answer, decided by nothing but which member the caller
  happened to hold. A `.rNN`/`.sNN` source now derives its base like any other member, so
  `archive.rar`, `archive.r00` and `archive.r01` all report
  `[archive.rar, archive.r00, archive.r01]` in that order. A caller that read "opened a `.r00`,
  got `is_multipart == false`" as "this is a standalone archive" is now told it is volume 2 of a
  set, which is what it is. This also holds for a series whose `.rar` main is absent: two
  `.rNN` files are still one set.

  Anchoring is load-bearing, not incidental. `parse_volume_set_for` falls back to an *unanchored*
  largest-group rule when the source parses as no volume name at all, and in a directory holding
  two sets that fallback would bind the handle to whichever set is bigger. The migrated body
  refuses to reach it: a source name that parses as no volume returns the documented
  `(false, [self.path])` shape without consulting the parser. That check sits *after* the
  directory scan, so an unreadable parent still surfaces as `ArchiveError::Io` whatever the
  archive is named. The rest is unchanged by construction — write mode still errors, the
  capability gate still short-circuits non-multipart formats before any scan (7z `.001` included;
  that routing decision stays open), the deliberate single-element `(false, [self.path])` return
  shape is preserved, matching stays ASCII-case-insensitive over original-case paths, and volume
  order is still main-first then ascending with `.sNN` after `.rNN`.

  Three degenerate name classes move with the parser's rules, none of them a name WinRAR or
  WinZip writes. Volume *zero* siblings (`x.z00`, `x.000`, `x.0`, `x.part0.rar`) used to be swept
  into a set and are now rejected, because 1-based continuity arithmetic cannot say what volume 0
  is. A source carrying an all-digit extension wider than nine digits used to group `.NNN`
  siblings even though the sort key could not represent it — the source predicate had no width
  bound although the sibling predicate did — and is now rejected on both sides. And two files
  claiming the same volume number (`x.part1.rar` with `x.part01.rar`) are still both returned,
  but now in a deterministic `(number, path)` order instead of whatever `read_dir` yielded.

- **`external::rar::session::CreateRequest` is gone; the invocation sequence takes
  `argv::AddArgv` directly.** The two structs were field-for-field identical — the same five
  fields, the same types, the same doc text — and `CreateRequest::as_argv` bridged them by copying
  each field by hand. That protected exactly one direction. A field added to `AddArgv` broke the
  conversion with E0063, loud and correct; a field added to `CreateRequest` compiled clean and the
  new field simply never reached the argument vector, so a switch a caller set would have been
  dropped with nothing said about it.

  `create_archive` and `preview_argv` now take `&AddArgv<'_>`, the type that lives beside
  `argv::build`, where the argv contract is enforced. There is one place to add a field and one
  place — `build` — that decides what it emits. **No deprecated alias was left behind.**
  `pub use argv::AddArgv as CreateRequest;` would restore the second name for the one type, which
  is the thing being removed, and there is no source compatibility to keep: `pub mod external` is
  gated on `all(target_os = "windows", feature = "external-rar-create")`, a non-default feature,
  and no version of this crate has ever been published to a registry, so no build outside this
  tree can name `CreateRequest` today.

  `AddArgv` stays open rather than joining its `#[non_exhaustive]` siblings `CommandOutcome`,
  `RarVersion` and `RarBanner`. The attribute obliges a total constructor, and the only one
  available is a five-argument `new` — exactly the positional call site the struct exists to
  prevent. Adding a field remains a breaking change; what the collapse buys is that the break is
  now *loud in every direction*, since every construction site is a struct literal that fails to
  compile until the field is supplied. No runtime assertion replaces the removed drift, because
  with one type there is nothing left to compare — the guarantee is the compiler's. What
  `tests/external_rar_cli_contract.rs` gains instead is `the_preview_is_the_vector_that_is_run`,
  which hands *one* `AddArgv` to both `preview_argv` and `create_archive` and asserts the redacted
  argv the runner received equals the preview element for element. That is only expressible
  because there is one type, so re-introducing a parallel request struct in `session` stops it
  compiling.

- **The eight libarchive read walks agree on `ARCHIVE_WARN`, and no longer throw its message
  away.** Three of them refused the status and five accepted it, so the same archive could pass
  `validate_integrity()` and be rejected by `list_files()` — same bytes, opposite answers,
  decided by nothing more than which method the caller reached for. All eight accept it now:
  libarchive returns `ARCHIVE_WARN` when it *recovered* and the header is usable, and refusing a
  read the library completed makes this crate stricter than the library it wraps.

  Calls that returned `Err` for a recovered header now return `Ok`. The text libarchive attached
  used to be discarded at every accepting site, so a caller was told the archive was fine with no
  way to learn what had been objected to; it now arrives as an `ArchiveWarning::BackendAdvisory`.

  The policy lives in one helper, `next_header_status`, and
  `tests/libarchive_header_policy_test.rs` reads the source to keep it that way — a ninth call
  site with its own inline status check would reintroduce exactly the divergence while every
  behavioural test still passed. Two of the newly-unified sites also stopped bypassing the error
  classifier, so an encrypted-header failure there now surfaces as `ArchiveError::Password`
  instead of a generic `Format`.

- **`test_integrity` tells a failing disk from a damaged archive.** libarchive raises
  "Error reading '…'" through the same `archive_read_data_block` return as "Truncated input
  file", and the storage case was being recorded as a damaged entry — sending the caller to fix
  the wrong thing. A new message discriminator, `is_libarchive_operational_failure`, splits them,
  and an operational fault now propagates as `ArchiveError::Io` instead of landing in
  `failed_files`.

  Everything else stays a damaged entry on purpose. Promoting every non-checksum `Format` to a
  typed abort would turn a truncated tar from a listed failed file into a hard error and lose the
  rest of the walk — the regression this was filed to avoid. The discriminator is deliberately
  narrow, case-sensitive, and fails toward "not operational", which preserves prior behaviour; a
  test asserts the two message discriminators cannot both claim one message.

- **`add_directory_recursive` on a filesystem root no longer drops the root entry.** `/` has no
  parent, so the parent-relative rule produced an empty archive path for the root itself, the
  walker's guard dropped it, and its children were archived unprefixed as `etc/`, `usr/`, … The
  root's own mtime and mode were lost without a word.

  A root now gets a synthesised name that behaves like any other source directory's: it is the
  top-level prefix for the whole tree, so `/` yields `rootfs/`, `rootfs/etc/`, … Nesting is what
  makes it coherent — naming the root entry without nesting its children would emit an empty
  `rootfs/` beside `etc/`, worse than the drop — and it makes a collision impossible, since a
  real `/rootfs` becomes `rootfs/rootfs`. The name derives from the root's own spelling, so it is
  stable and distinguishes volumes: `C:\` gives `C`, `\\srv\share\` gives `srv_share`. Unix `/`
  has no alphanumerics to draw on and falls back to `rootfs`.

- **`ArchiveFormat::Rar` and `Rar5` report `multipart_read: Support::Partial`, not `Full`.** The
  `Full` claim was false, and a real three-volume set now in the fixtures proves it. RAR does get
  further than ZIP — UnRAR opens the set and lists it, rather than ZIP's name-level enumeration —
  but extraction across volumes is not implemented end-to-end either, so the same `Partial` that
  ZIP carries applies for the same reason.

  What a caller actually gets for a complete, `unrar`-verified volume set: the split file is
  listed as one entry *per volume*, all sharing one path, so `extract_all` trips the
  duplicate-output-path guard, `extract_file` and `extract_to_memory` refuse to disambiguate and
  recommend `extract_by_ids`, and `extract_by_ids` — the recommended call — fails with a
  listing-drift error. `validate_integrity()` meanwhile reports the set as fully healthy. Every
  outcome is pinned in `tests/rar_multivolume_test.rs`; the gap is ticgit 3b4d15.

- **`ExtractionLimits::reject_unsafe_paths` enforces something.** It recorded intent and gated
  nothing. Set `true`, the pre-extraction gate now blocks an archive carrying an unsafe entry name
  with `OperationBlocked`, whose reason names the entry and what is wrong with it — "unsafe path
  components in entry '…': … (ExtractionLimits::reject_unsafe_paths is set)" — instead of letting
  the lossy sanitiser rewrite it. The default stays `false`, which keeps AD 0066's
  lossy-repair baseline byte-for-byte — so this changes nothing for a caller who does not opt in.
- **Losing an entry is no longer silent.** A modify round-trip *deleted* symlinks, hard links and
  special entries: `commit_changes` replayed retained entries through a loop that skipped anything
  the rewrite could not represent, and said nothing. Extraction had the same hole in the other
  direction — the libarchive and 7z kind allowlists skipped FIFOs, sockets and device nodes
  silently, because `ArchiveWarning` had `SkippedSymlink` and `SkippedHardLink` and nothing for a
  kind. Both now emit `ArchiveWarning::SkippedUnsupportedEntry { path, kind, reason }`, with the
  reason field keeping "dropped during modify" and "unsupported kind on extract" distinguishable at
  a match site. The ruling was warn — not refuse, and not re-emit.
- **`ExtractionOptions::progress` is honoured by `extract_file`.** It was the one disk-writing
  entry point that dropped the callback, and unlike its in-memory and streaming siblings it did not
  document the omission.
- **A zstd file with a leading skippable frame opens.** Detection matched only the bare frame
  magic, while RFC 8878 permits skippable frames (`0x184D2A50`–`0x184D2A5F`) anywhere including
  first, and `Zst` is absent from the extension-fallback set — so such a file ended as "Unknown
  archive format" on every path. Detection now skips leading skippable frames using their declared
  length, bounded so a crafted file cannot loop. `Zst` was deliberately *not* added to the
  extension fallback: content-based detection is the policy and that set is small on purpose.
- **Recursive creation observes the filesystem once, and directory metadata survives on both
  writers.** It builds a manifest in a single walk and writes from the manifest instead of
  re-deriving the file set while writing. Every directory the walk observed now gets an entry
  carrying its mtime and unix mode — not only the empty leaves, which is all the old
  `is_leaf_dir` emit produced — and that now includes ZIP, whose arm was handing the writer's
  default `FileOptions` and dropping both. *This changes what a recursively created archive
  contains:* non-empty directories contribute entries where they contributed none, and a ZIP
  directory record now carries a real DOS timestamp and, on Unix, external attributes with the
  source mode, where before it carried the DOS epoch — 1980-01-01. **A checksum stored over a
  recursively created archive will not match a freshly created one and must be recomputed.**
  Archives built entry-by-entry through `add_file_from_data` / `add_file_from_path` are
  unaffected. `Archive::add_directory(path)` stays metadata-free by construction: its caller
  names an archive path, so there is no filesystem entry to read a mode or mtime from. DCR-013
  is the covering record.
- **Test lanes can no longer pass without testing.** A lane that returned early when a fixture was
  missing reported success having tested nothing — worse than a failure, because it is invisible.
  Every such lane now either runs and can fail, or is `#[ignore]`d with the reason *and* the exact
  command to run it; lanes that iterate a fixture set gained vacuity floors, so an empty glob fails
  rather than passes. The ignored count rose from 6 to 13, which is the honest direction: those are
  lanes that used to pass while skipping. The perf sentinel went the other way and now runs in the
  default lane.

### Deprecated

Six items now carry `since = "0.5.0"`. Three of them were written as `since = "0.4.0"`, which named
a release that does not contain them — `v0.4.0` is tagged at `2c7f349`, before either change landed,
so a caller on 0.4.0 would never see the warning the attribute promised.

- **`CompressionOptions::new`** → `for_writable` for a literal format, `try_new` for a computed one.
  It accepts a read-only format and defers the rejection to `Archive::create`. Removed in 0.6.0.
- **`ArchiveEntry::new`** → `ArchiveEntry::file(path, id).build()`. Removed in 0.6.0. *One migration
  note, because it bites:* `new` took a concrete `String`, so `"x".into()` inferred its target from
  the parameter; `file` takes `impl Into<String>`, which makes that `.into()` ambiguous. Drop it —
  `"x"` is what the builder wants.
- **`ArchiveEntry::directory`** → `ArchiveEntry::dir_at(path, id).build()`. Removed in 0.6.0.

Attaching those three is what the 0.4.x doc blocks deferred, on the stated grounds that the crate
still called them itself and the gate is warning-free. The 187 in-crate call sites moved first: 132
literal formats to `CompressionOptions::for_writable`, 7 computed ones to `try_new`, and 48 entry
constructions to `ArchiveEntry::file(..).build()`. Along the way the checked constructors stopped
routing through the loose one — `for_writable` called `new`, which is the dependency the wrong way
round — and `CompressionOptions::default()` now says `for_writable(WritableFormat::ZIP)`, which is
exact since ZIP is creatable. Seven test files stopped naming `ArchiveFormat` altogether, which is
the newtype doing its job.

And the three that were mis-dated:

- **`ArchiveEntry::symlink`** → `ArchiveEntry::symlink_at(path, id, target).build()`, which returns
  a builder so the remaining metadata can be set in the same expression instead of by field
  assignment — the mutation path `#[non_exhaustive]` closes. Removed in 0.6.0.
- **`LibarchiveCompressionOptions::new`** → `for_writable` or `try_new`. It accepts a format
  libarchive cannot create and defers the rejection to `create_libarchive`. Removed in 0.6.0.
- **`CompressionOptions::password`** — *suspended, not sunset.* Encrypted creation is not supported
  (MADR-0027), so every value this produces is rejected by `Archive::create`; there is nothing to
  migrate to and no deadline. The attribute comes off, not the method, when the opt-in of
  OI-0081-006 ships.

### Fixed

- **The external RAR lane's destination check now really is the last step before the spawn.** Its
  comment claimed it ran "immediately before the run"; `probe_binary`, which spawns `rar` and waits
  for its banner, sat between the check and the run — the widest possible version of the window the
  check exists to narrow.

  The check now runs twice. The first, before the probe, keeps the common "the file is already
  there" case free of any child process at all, which a test pins deliberately. The second runs
  after the probe, so nothing spawns between it and the create. A second `stat` buys the narrowing
  that moving the probe would have bought, without spawning on every occupied destination.

  This narrows the window; it does not close it. Closing it means creating under an exclusive
  temporary name and installing the result with `rename_noclobber` — which already exists in
  `ffi::common` with unix, Windows and fallback arms — and remains the open half of
  OI-0076-006 / ticgit 642488, along with the missing `-ep1` switch that would stop absolute source
  paths being stored verbatim.

- **A header-encrypted RAR reported none of its main-header flags.** `UnrarArchive` took its flags
  word from `RAROpenArchiveEx` and called `RARSetPassword` afterwards, so for a `-hp` archive — whose
  main header lives inside a HEAD_CRYPT block — the SDK could not decrypt that header in time and
  the word came back empty. `has_recovery_record()` therefore answered `false` for an archive
  `unrar lt -p<pw>` describes as "RAR 5, recovery record, encrypted headers", and `ROADF_SOLID`,
  `ROADF_COMMENT`, `ROADF_VOLUME` and the rest were lost the same way. The password now reaches
  `RAROpenArchiveEx` itself, through the SDK's `UCM_NEEDPASSWORD` request.

  Registering that callback is conditional on actually having a password, and that is not a
  detail: a header-encrypted archive must still **open** without one, because reporting that a
  file is encrypted is how a caller learns which password to ask for. A callback that declines the
  request makes the SDK proceed as though an empty password had been supplied, which fails the
  open — so when there is no password the open is left exactly as it was, and the callback is
  installed immediately after instead.

  Fixing the flag exposed a second fault behind it. `recovery_percentage()` re-opens the path and
  walks the block structure with no password, and that walk had never been reachable for a `-hp`
  archive before — it promptly read HEAD_CRYPT ciphertext as a header and reported
  `Corruption: "RAR5 header extends past end of archive"` for an archive `unrar t` calls sound. It
  now short-circuits on `ROADF_ENCHEADERS` and returns the documented "percentage cannot be
  determined". A header this parser is not permitted to read is not damage.

- **A missing RAR volume now says so.** The typed `MissingVolume` diagnostic added for the
  unbounded-retry fix could not reach a caller. The extract path installed its callback for the
  duration of one `RARProcessFile` and cleared it after, so a volume request during the *listing*
  walk — skipping past an entry that continues into the next volume — found no callback at all.
  The vendored SDK's `DllVolChange` maps both "the callback aborted" and "no callback was
  installed" to `ERAR_EOPEN`, and never calls back in the second case, so what a caller saw was
  `Io` / "cannot open archive" naming the *first* volume. Boundedness came from the SDK's own
  no-callback branch rather than from the fix.

  The callback now lives for the handle's lifetime and is restored after any operation that
  installs its own, so every walk can answer. Extracting a set with its middle volume removed
  reports the record it was given: which set, that the next volume is unavailable, and that
  `parse_volume_set` / `VolumeSetReport::defects()` names the gap.

- **`recovery_percentage()` returned the same number for every archive.** It reported `Some(2)`
  for a RAR built with `-rr1p`, `-rr3p`, `-rr5p`, `-rr10p` and `-rr30p` alike, and could never
  report more than 15. The cause: it searched the 32 bytes after the literal `"RR"` for the first
  byte in `1..=15` and returned it — and that byte is the extra-area record's own *size* field,
  which is `0x02` for every recovery record. It now decodes the field the format actually defines:
  the `FHEXTRA_SUBDATA` (`0x07`) record of the `"RR"` service header, read as a vint, exactly as
  the vendored UnRAR does in `arcread.cpp`. Measured against fixtures built at five different
  percentages, each now reports its own value.

  This was invisible because no fixture in the repository had a recovery record at all —
  `tests/fixtures/test_recovery.rar` was byte-identical to `test.rar` despite the name, so every
  assertion in the suite could only ever check the `false` / `None` branch. A parser that always
  answered `Some(2)` passed. The fixture is real now, and `FIXTURE_RECOVERY` in
  `tests/recovery_percentage_edge_cases.rs` holds a per-fixture expected value that a
  regeneration has to update.

  Two limits are recorded rather than fixed, each with a test that pins today's answer so a fix
  breaks it: a record above 255% cannot be carried by the `Option<u8>` return type and reports
  `None` (ticgit 7ca208), and a header-encrypted (`-hp`) archive reports no recovery record at all
  because `UnrarArchive` captures its flags at `RAROpenArchiveEx` and calls `RARSetPassword`
  afterwards (ticgit 3f8790).

- **`cargo doc` is warning-free again.** Three rustdoc warnings shipped in `4f521e0` because that
  change's gate counted clippy warnings and never ran `cargo doc`. Two were public docs linking to
  private items (`SourceManifest`, `RawCentralDirectory`), which render as dead links for anyone
  reading the published docs; the third was a redundant explicit link target.
- **A RAR set with a missing volume errors instead of spinning under the process-wide lock.**
  The UnRAR data-callback trampoline answered every non-`UCM_PROCESSDATA` message with `1`,
  described as keeping the library's default behaviour. For `UCM_CHANGEVOLUME` /
  `UCM_CHANGEVOLUMEW` that is not what a non-abort answer means. The vendored SDK's own
  `DllVolChange` quits only on an abort return, or when no callback is registered at all, and its
  comment says returning an unchanged volume name is a legitimate way to say "waiting for a
  volume that does not exist yet" — so a registered callback returning `1` without rewriting the
  name buffer asks for the same volume again, indefinitely. Every UnRAR call holds a
  process-wide lock (AD 0019), so that spin stalls every RAR operation in the process, not just
  the one that started it.
  The message is now handled on its mode, which is the part that cannot be simplified: with
  `RAR_VOL_ASK` the volume is missing and the trampoline aborts, surfacing
  `ArchiveError::Corruption` that names the archive and points at
  `format::multipart::parse_volume_set` / `VolumeSetReport::defects()` for *which* volume; with
  `RAR_VOL_NOTIFY` the next volume was opened successfully and the answer stays non-negative,
  because `-1` there makes the SDK give up on a perfectly good multi-volume read. Supplying the
  next volume path instead of aborting is the multi-volume continuation feature, tracked
  separately (OI-0001-006). Five tests pin both arms; reverting either direction fails a
  different one.
- **One assertion that could never fail is gone.** `assert!(!cfg!(windows))` inside a match arm was
  `assert!(true)` off Windows and unreachable on it, so it documented an expectation rather than
  checking one. The arm is `#[cfg(not(windows))]` instead, which states the same thing where it
  cannot rot.
- `build.rs` stops emitting its Windows libarchive warning on builds where the link succeeded.
- `examples/detect_sfx.rs` drops a "Confirmed" line that could only ever print "No", because the
  production detector never returns `SfxConfidence::Confirmed`.
- `LICENSE` names the actual rights holder; the "7zip-RBinding Contributors" line was left over
  from a rename `CHANGELOG.md` records as complete, not a deliberate retention.

### Internal

- `src/stream_crc.rs` split into `digest` (values and the I/O-free bit scan), `codec` plus
  `codec/{gzip,bzip2,xz}` (framing) and `detect` (the six-byte probe and routing), with the root
  holding the module doc and re-exports and no logic. Every child is private and every public item
  keeps its historic `stream_crc::…` path, so no public surface moved.
- AD-0057's test move-out: 94 tests left `src/ffi/{wrapper,zip_wrapper,sevenz_wrapper}.rs` for four
  new child modules — `ffi::wrapper` 37, `ffi::zip_wrapper` 34, `ffi::sevenz_wrapper` 23 — and the
  126-line inline block that had just been added to `src/archive.rs`, itself a named large-file
  target, followed it into `src/archive/in_place_payload_tests.rs`. Nothing was widened from
  private to `pub(crate)` to make a move compile.

### Nothing owed before 0.5.0 can be tagged

The one item that was outstanding here — `CompressionOptions::new` and `ArchiveEntry::new` still
un-attributed, against doc blocks that promised `#[deprecated]` in 0.5.0 — is delivered, so the
timeline did not have to slip. It is described under **Deprecated** above; the summary is that the
187 in-crate call sites moved first (132 literal formats to `for_writable`, 7 computed to `try_new`,
48 entry constructions to `file(..).build()`), and `cargo clippy --all-targets --all-features -D
warnings` is green with the attributes attached.

Three `#[allow(deprecated)]` sites remain, all deliberate and all commented: two assert that
`CompressionOptions::new` still accepts a non-creatable format and defers the rejection — which is
the back-compat guarantee, and neither replacement can express a non-creatable format at all — and
they are the reason `new` is deprecated rather than removed. `ArchiveEntry::new` needed none.

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
