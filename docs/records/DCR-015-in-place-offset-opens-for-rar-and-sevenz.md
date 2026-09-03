---
type: DCR
title: "In-place offset opens extended to RAR and 7z; libarchive deferred on gate strength, not on FFI cost"
description: "Archive::try_open_in_place grew RAR and 7z arms, so a self-extracting RAR or 7z payload is read where it lies instead of being copied to a tempfile first. The three arms do not share a mechanism: ZIP and RAR self-relocate (the second inside UnRAR's own SDK, needing no new FFI), while 7z is reached through a new pub(crate) PayloadWindow because sevenz-rust2 deliberately requires its signature at stream position 0. AD-0040's 16 GiB ceiling was a consequence of the copy, not of any format limit, and DEF-001 was closed in 2026-04-18 on an implementation that copied. libarchive stays staged because its formats carry no strong magic at the payload offset. This record also enumerates what the staged copy was quietly buying and is now trading, none of which is visible in the diff."
tags: [change, project-control, DCR-015]
status: active
---

# DCR-015: In-place offset opens extended to RAR and 7z; libarchive deferred on gate strength, not on FFI cost

- **Date:** 2026-09-01 (landed); recorded 2026-09-02
- **Source:** DEF-001 (`docs/project/stub-manifest.md`); TicGit `5858e17b`; owner ruling 2026-09-01
  ("pursue it — RAR and 7z now, libarchive documented and filed"). ZIP had already landed
  2026-08-21 under TicGit `1ddc37ec`.
- **Related:** AD-0040 (the SFX payload ceiling, and the staging copy that forced it), AD-0052
  (lazy backend-open validation — nothing here parses at open), AD-0065 (memoised listings, live
  payload re-reads), AD-0015 and AD-0006 (the detection scan window, which is why detection-driven
  offsets always fall inside UnRAR's reach), DCR-014 (the open-time identity binding that in-place
  handles inherit), DCR-007 (the residual stat-to-open window this change does not widen),
  MADR-0023 (the record that first governed `open_at_offset`'s error surface, whose amendment noted
  the method was tempfile-backed), TicGit `05e2dea4` (the libarchive follow-up, design-complete)

## Why this needs a record at all

Two reasons, and both are about a reader arriving later and drawing the wrong conclusion.

**A closed deferral had left the thing it was about unbuilt.** DEF-001 promised that
`Archive::open_at_offset(path, offset)` would work for every backend. It was marked **Closed
2026-04-18**, and that closure was honest on its own terms: the method did work, for every backend,
because it copied the payload tail out to a tempfile and opened *that*. What it never did was teach
any backend to read at an offset. The distinction is the substance of this record. A future reader
auditing "which deferrals are still open" would have found DEF-001 struck through and moved on;
the deferral register itself said so for four months while a multi-gigabyte self-extracting archive
still had to be written to disk in full before a single entry could be listed.

**Three arms, three mechanisms.** The natural reading of "in-place opens now cover ZIP, RAR and 7z"
is that one mechanism was applied three times. It was not, and a reader who believes it was will
mis-plan the fourth. Section *Three routes* below is the correction.

## What the AD-0040 ceiling actually was

`DEFAULT_MAX_SFX_PAYLOAD_SIZE` is 16 GiB, and it was never a format limit. It bounded **the blast
radius of a disk write**: an SFX payload of `file_len - offset` bytes was copied into the caller's
temp volume before the backend saw anything, so the ceiling existed to stop an attacker-supplied or
merely malformed stub from filling that volume. AD-0040's own consequences section says as much —
"a blast-radius bound, not an integrity check" — and its revisit trigger already named the exit:
replace the temp-file hand-off and the decision becomes obsolete.

So the ceiling is a property of the *copy*, and where the copy goes away the ceiling goes away with
it, which is exactly what `PayloadAccess` was introduced to disclose. Since the 2026-07-22 amendment
the 16 GiB figure has been the **default** of `ExtractionLimits::max_sfx_payload_size`, a
caller-settable `Cap`, not a hard constant — which matters for the trade recorded further down.

## What changed

**`src/payload_window.rs` (new, `pub(crate)`)** — `PayloadWindow<R: Read + Seek>`, a `Read + Seek`
view of `[start, start + len)`. Top-level rather than under `src/ffi/` because both safe Rust (the
7z wrapper) and FFI-adjacent code (the UnRAR recovery-block walk) consume it. Nothing in the crate
did this already: `HardCapReader` is `Read`-only and polices byte *totals* rather than translating
positions, and the libarchive stream readers are `Read`-only pulls over an FFI handle. Two
properties are load-bearing and are argued in the module's own docs:

- **`len` is frozen at construction**, so bytes appended to the underlying file after the window is
  built are invisible. This is the in-place analogue of the `Read::take` bound the staging copy
  enforced; without it a concurrently-growing SFX could feed a backend bytes past the length the
  safety gate admitted.
- **`End`-relative seeks resolve against the window, not the file.** Not a nicety:
  `sevenz_rust2::Archive::read` seeks `End(0)` to learn the archive's length before it reads
  anything, so a window that answered with the file's end would report a length that includes the
  stub and everything after the payload.

Twelve unit tests cover the boundaries, the `End` translation, seek-past-end, negative-seek
`InvalidInput`, the frozen length against a growing file, cursor-drift under interleaved seeks and
reads, and offset-zero passthrough equivalence.

**`src/archive.rs`** — `try_open_in_place` was a ZIP-only body; it is now a dispatcher over three
arms (`try_open_zip_in_place`, `try_open_rar_in_place`, `try_open_sevenz_in_place`) with the shared
executable-extension gate hoisted above them, a shared `magic_at::<N>` probe, a shared
`in_place_handle` constructor so `PayloadSource::InPlace` cannot be forgotten on a fourth arm, and
`first_rar_signature_before` for the RAR agreement scan. Arm order is irrelevant by construction:
the three signatures are mutually exclusive. The RAR arm is `#[cfg(feature = "rar-support")]`. Plus
the staging space pre-flight, described in its own section below.

**`src/ffi/wrapper.rs`** — `UnrarArchive` gains `payload_offset: u64` (zero for every existing
constructor), `open_at_offset`, and `UNRAR_MAX_SFX_SCAN`. `fresh_handle` propagates the offset to
the re-opened handle. The one read in this type that does *not* go through the SDK handle —
`parse_recovery_percentage`, which opens the path itself and walks raw block bytes — is wrapped in a
`PayloadWindow`; both recovery parsers were already generic over `Read + Seek`, so their `Start(..)`
signature skips and `End(0)` length probe became payload-relative for free.

**`src/ffi/sevenz_wrapper.rs`** — `SevenZArchive` gains `payload_offset: u64`, `open_at_offset`, and
one private `at_offset` constructor so a new field cannot be initialised two different ways.
`open_reader` now returns `ArchiveReader<PayloadWindow<File>>`; all six internal callers recompile
unchanged because the type is internal.

## Three routes, and why they are not one mechanism applied three times

- **ZIP relocates itself.** The `zip` crate derives the archive's start from the end-of-central-
  directory record and folds that offset into every local-header position, and this crate's wrapper
  already reads its raw central-directory index from the absolute central-directory start. Handing
  the wrapper the *outer* file is the whole implementation.
- **RAR relocates itself inside the SDK, and this cost no new FFI at all.** UnRAR's
  `Archive::IsArchive` reads seven bytes at position 0 and, when they are not a marker, reads a
  `MAXSFXSIZE`-sized buffer and scans it for the **first** RAR signature, setting its internal
  `SFXSize` to that offset and proceeding from there. `RAROpenArchiveDataEx` has no offset or SFX
  field and needs none: listing, extraction and integrity testing all go through the handle opened
  on the outer pathname. **The SDK could always read a RAR SFX in place; the crate simply never
  routed offset opens to it.** `open_at_offset` is therefore `open_with_mode` on the outer path plus
  one field assignment, and the interesting work is entirely in the gate that must establish, before
  the call, that the SDK will land where the caller asked.
- **7z does not relocate, deliberately.** `sevenz_rust2::Archive::read` seeks `End(0)` to learn the
  length, seeks back to `Start(0)`, and requires the six-byte `7z\xBC\xAF\x27\x1C` signature **at
  stream position 0**. It searches for nothing. That is why 7z needs an adapter where RAR needs
  none — and why `PayloadWindow`'s `End`-relative translation is load-bearing rather than
  decorative.

The asymmetry is not an accident of the crates involved; it is the reason the fourth backend is a
different problem, and it is what the gate section below turns into per-arm policy.

## The gates, per arm, and why the asymmetry is real

**Common to every arm — SFX-shaped outer name.** The outer path must carry an executable extension
(`extension_suggests_executable`). This is not cosmetic and it is not a heuristic about
trustworthiness. When the caller supplies a password, extraction reopens the archive through
`Archive::open_encrypted` against `source_path_for_reopen()` — which for an in-place handle is the
**outer** file — and `open_encrypted` resolves an embedded payload only through its SFX fallback,
which admits a file on executable extension. Without this gate,
`open_at_offset("blob.bin", n)` followed by a password-bearing extraction would fail where staging
succeeded. Lifting it needs an offset-aware encrypted reopen; that is tracked as its own deferred
item in `docs/architecture/mvp-scope.md` and is **not** part of the libarchive ticket, a conflation
that has caused confusion before.

**Common to every arm — magic at `offset`.** The staged tempfile's suffix used to drive format
re-detection. In place there is nothing to re-detect, so the bytes at `offset` are the only
authority and each arm demands its own signature there. This is precisely the requirement libarchive
cannot meet, below.

**ZIP — offset agreement.** The `zip` crate is asked, through the same default `ArchiveOffset`
resolution the wrapper uses, where *it* thinks the archive starts; disagreement declines to staging,
which slices at exactly the caller's offset. Costs one throwaway central-directory parse (metadata
only, no payload decode) against a copy of up to the whole payload.

**RAR — offset agreement, needed more than ZIP needs it, plus reach at both ends.** Agreement
matters more here because the SDK binds to the **first** marker it finds: an earlier
`Rar!\x1A\x07\x00` byte run in the stub — a string constant, another embedded archive — would
silently redirect the open to a different archive than `offset` addresses, and the SDK does not
report its `SFXSize` back through the DLL API, so the crate cannot ask afterwards where the library
actually landed. The check must therefore happen on our side, before the open:
`first_rar_signature_before` scans `[0, offset)` in chunks with an eight-byte overlap and declines
on any earlier RAR-family hit. An unreadable span is also a decline, not a "no earlier signature" —
an unreadable stub is exactly when guessing is wrong.

Reach is a gate at **both** ends, and the near end is the non-obvious one. The SDK's SFX scan starts
at byte 7, because it reads the marker at position 0 first and only then scans onward, so **a
payload at offset 1..=6 is unreachable to UnRAR however well-formed it is** and is declined to
staging. At the far end the scan stops at `MAXSFXSIZE` (4 MiB in the vendored `rardefs.hpp`), so
`UNRAR_MAX_SFX_SCAN` is expressed as `MAXSFXSIZE - 16` — a few bytes *stricter* than the C++ loop
rather than looser, an asymmetry that is free: an offset this bound wrongly declines is staged the
way it always was, while an offset it wrongly admitted would hand the SDK an outer file whose
payload it cannot find. Detection-driven offsets always qualify, because the detection scan window
is far smaller; only a raw `open_at_offset` caller can name a larger number.

**7z — no agreement gate at all, and none is owed.** The window's base *is* the archive start,
because `Archive::read` requires the signature at stream position 0 and searches for nothing. A
wrong offset fails the signature check and falls back to staging; there is no "some other archive"
for it to find. Recording that this arm is *deliberately* one gate lighter matters: a later reader
comparing the arms should not add an agreement scan here in the belief that its absence was an
oversight.

**Every arm declines rather than propagates on a constructor failure.** The contract of
`try_open_in_place` is that `Ok(None)` means "could not prove it, let staging try", and staging then
owns the error. The concrete reason this had to be enforced on the new arms — and the reason the `?`
that used to sit on the ZIP wrapper's own open was changed to a decline — is that UnRAR's
`IsSignature` also accepts RARFMT14's `RE~^` and the RAR 2..4 future markers, which this crate's
`scan_for_signatures` does not know. A stub carrying a marker the crate's signature table cannot
name must fail **closed into a copy**, not into a hard error the caller never used to get.

## Why libarchive is deferred, and it is not the FFI

The FFI is about thirteen mechanical declarations — the `archive_read_set_*_callback` family,
`archive_read_set_callback_data`, `archive_read_open1`, five callback typedefs and one scalar alias
— plus one RAII holder for the client data. The exact C signatures are verified and carried in
TicGit `05e2dea4`. That is not the blocker, and saying "too much FFI" would misdescribe the
deferral.

**The blocker is the gate.** Every shipped arm rests on strong magic at the payload offset. The
libarchive family has no such anchor: plain tar's `ustar` sits at **+257**, gzip's magic is **two
bytes**, and the raw filter accepts nearly anything. A gate cheap enough to run at open time would
admit garbage. The alternative — commit to the in-place open and let libarchive's format bidding
decide — needs a **fall-back-after-failed-open** path, and `try_open_in_place` deliberately does not
have one: every existing arm's failures happen *before* backend construction. There is a known
answer (run the AD-0052 eager first-header probe inside the gate and decline to `Ok(None)` on
failure), but that is new policy about when a backend may be constructed speculatively, and it
deserves its own review rather than riding along on a ticket whose other two halves were a day's
work each.

**The counterweight, stated so the deferral is honest.** makeself `.run` / `.sh` installers are a
stub plus a **tar.gz** payload; both extensions are already admitted by
`extension_suggests_executable`; and they are routinely multi-GB. They are the class that most feels
the staging cost, and they are exactly the class still paying it. The deferral is therefore a
triggered follow-up with a named constituency, not a "someday": TicGit `05e2dea4` carries the
design, and the `docs/architecture/mvp-scope.md` rows point at it.

Secondary, but real: the blast radius is eight libarchive reopen sites plus an ownership rework. The
stream reader currently disarms a `ReadHandleGuard` with `std::mem::forget` on its success path and
moves a bare `*mut Archive` into the returned reader; porting that shape naively to callbacks is
precisely how the boxed client data gets dropped while libarchive still holds a pointer to it. That
rework is behaviour-preserving on its own and can be landed ahead of the FFI to de-risk it.

## What staging quietly bought, and is now traded

None of this is visible in the diff, which is why it is the section that matters most.

1. **The staged copy was immutable under concurrent rewrite; an in-place handle reads the live
   file.** Three things bound the new exposure. The open-time identity binding (DCR-014) applies to
   in-place opens, so a handle whose file was replaced is refused rather than read. `PayloadWindow`
   freezes `len`, so post-open appends are invisible. And in-place content rewrites land exactly
   where AD-0065 already stands for **ordinary offset-zero opens**: listing snapshots are memoised,
   payload re-reads are live, and per-entry CRC verification catches mid-read tampering at
   extraction. The net position, and the honest way to state the trade: an in-place SFX open is
   *exactly* as exposed as `Archive::open("file.zip")` is today — **the staged path was stronger
   than a normal open, and that extra strength is what is being traded away.** It is not a new class
   of exposure; it is also not the copy's guarantee. Said in the `PayloadAccess::InPlace` docs, not
   only here.
2. **The AD-0040 ceiling doubled as a size refusal, and those callers silently lose it.** A caller
   who set `max_sfx_payload_size` as a *disk-budget* gate gets what they asked for: no disk is
   consumed, so no budget is spent. A caller who used the same field as a *size* gate — "refuse
   payloads bigger than N" — loses the refusal entirely on the in-place path, because there is no
   copy for the cap to bound. Nothing in the type system signals this. The remedy documented for
   them is to check the payload size themselves; `payload_access()` tells them which regime a handle
   is in.
3. **`SfxStagingProgress` is a staging observer, not an open observer.** It fires during the copy.
   An in-place open emits nothing, because nothing is staged. This was already true for the ZIP arm,
   but with RAR and 7z the fraction of SFX opens the callback can see anything at all has shrunk
   enough that the `open_with_sfx_progress` docs must say what the callback observes rather than
   leaving "no emissions" to be read as an error. Callers driving a progress bar off it should treat
   "no emissions, `Ok` returned" as instant completion.
4. **A password-bearing extraction still re-stages, and that is why the executable-extension gate
   stays on every arm.** `reopen_with_password_if_set` reopens the outer path through
   `Archive::open_encrypted`, whose SFX fallback stages under the **default** cap rather than the
   caller's. So for an in-place handle plus `ExtractionOptions.password`, the temp-space saving
   evaporates at extraction time. The result is still correct; the saving is not. This is
   pre-existing for ZIP and is not worsened here, and it is the reason gate policy could not be
   relaxed on the new arms.
5. **An in-place handle holds the outer file open for the backend's lifetime.** ZIP keeps a cached
   `File`, UnRAR keeps its SDK handle; 7z's exposure is per-operation, since `open_reader` is
   transient. On Windows this blocks deleting or replacing the SFX while the `Archive` lives, where
   a staged handle released the outer file as soon as the copy finished. Again identical to how an
   ordinary `Archive::open` behaves on the same OS. There is deliberately no "force staging" knob; if
   one is demanded it is a one-field `ExtractionLimits` follow-up, not a retrofit here.
6. **Structurally unchanged: staging is not deleted.** In-place is an optimisation layer that must
   prove itself; anything that cannot returns `Ok(None)` and stages exactly as before, and staging
   remains the error-reporting authority.

**Gained, and it could not have been gained any other way: multi-volume RAR SFX chains now
resolve.** Staging copied only volume 1 into an otherwise empty temp directory, so the volume chain
dead-ended there — a structural break, not a tuning problem. In place, UnRAR is left sitting next to
the sibling volumes it needs. This is a capability the copy could not have had.

## The staging space pre-flight, advisory in both directions

Staging survives — for the libarchive family, for every gate-declined ZIP/RAR/7z open, and inside
the encrypted-reopen path — and its worst failure mode is writing multiple GiB and only then hitting
ENOSPC, costing the caller the whole copy and telling them nothing until it is spent. One
`fs4::available_space` call at the single shared staging choke point (`stage_sfx_payload`, after the
`Cap` check and before the tempfile is created) answers it up front, measuring
`std::env::temp_dir()` — what `tempfile::Builder` picks by default, so it measures the volume the
copy will actually land on. `fs4` was already an unconditional dependency (used for advisory locking
in modify mode) and `available_space` is `#[cfg(any(unix, windows))]`, **not** feature-gated, so this
added no dependency and no platform code; the pre-flight carries the same `cfg` and other targets
simply keep today's behaviour.

**It is advisory in both directions, and that is the decision.** It refuses up front when the temp
volume *observably* cannot hold the copy. It **proceeds when the question cannot be answered** — an
exotic filesystem, a sandbox, a permission problem — because refusing to stage because the *check*
failed would turn working setups into regressions. TOCTOU is accepted for the same reason: space
vanishing between the check and the write still surfaces as today's `ArchiveError::io("copy", ..)`
with the `NamedTempFile` cleaning itself up on drop. This is a courtesy, not a gate.

**Error variant: `OperationBlocked`, not a synthesised `Io` with `ErrorKind::StorageFull`.** There is
no OS error here to wrap. Faking one would attribute a refusal the crate made to a syscall that
never failed, which is worse for a caller matching on error kinds than an honest refusal is. There
is no interaction with `Cap::Unlimited`: the pre-flight compares against `payload_size`, which is
known exactly as `file_len - offset` regardless of the ceiling.

## Test impact

The suite went from 1941 to 1969 passing tests across 41 suites (0 failed, 13 ignored), with
`cargo check --all-targets` warning-free, `fmt` clean and `clippy -D warnings` clean.

New coverage sits at **two altitudes, and deliberately not at a third**:

- **`PayloadWindow` unit tests** (twelve) pin the adapter's contract directly, including the two
  properties the design depends on — the frozen length against a file that grows, and `End`-relative
  seeks resolving against the window rather than the file.
- **Backend-constructor tests** pin each backend's half. On 7z: offset-zero equivalence with a plain
  open, listing and extraction of an embedded payload, a wrong offset failing the signature check, an
  embedded payload being invisible at offset zero, an offset past EOF and a missing archive both
  surfacing as typed `Io` rather than as a format error, deferred validation (AD-0052) preserved, and
  the identity binding still captured on an offset handle. On RAR: an in-place handle parsing recovery
  metadata *from the payload* where an offset-zero read of the same file cannot see the record at
  all, a fresh handle inheriting the payload offset, offset zero behaving as an ordinary open, and
  `UNRAR_MAX_SFX_SCAN` checked against the vendored `MAXSFXSIZE` so a vendored-source bump trips a
  test rather than silently loosening a gate.
- **Not added: facade-level integration coverage of the new arms.** `tests/integration/sfx_in_place_open.rs`
  still exercises `payload_access()` end to end for ZIP only. The RAR and 7z *gates* —
  the agreement scan, the two-ended reach gate, the magic probe — are therefore argued in prose and
  covered only through their constituent parts, not through a facade open that observes
  `PayloadAccess::InPlace` for a RAR or 7z SFX. Named here as a real hole rather than left for a
  reader to discover from a passing gate.

## Known risks

1. **The size-refusal loss (trade 2 above) is silent.** A caller using `max_sfx_payload_size` as a
   size gate gets no error, no warning and no type-level signal — the open simply succeeds where it
   used to be refused. `payload_access()` is the only way to notice, and a caller has to already
   suspect the problem to consult it.
2. **The RAR agreement scan is only as good as `scan_for_signatures`.** It knows the RAR4 and RAR5
   markers; UnRAR's `IsSignature` additionally accepts RARFMT14's `RE~^` and the RAR 2..4 future
   markers. A stub containing one of *those* earlier byte runs would pass the agreement scan and the
   SDK would still bind to it first. The decline-on-constructor-failure rule turns that into a copy
   rather than a wrong read in the cases where the open then fails, but a stub whose earlier marker
   opens *successfully* would be served instead of the caller's offset. The scan's signature set is
   the place to widen if this is ever observed.
3. **The offset-1..=6 hole is a permanent decline, not a fix.** Those payloads stage forever, or
   until UnRAR changes where its scan starts.
4. **The 7z arm has no agreement gate by design**, which is correct against `sevenz-rust2` 0.19.3's
   signature-at-position-0 behaviour. That behaviour is the load-bearing upstream assumption of the
   whole arm and must be re-verified on any `sevenz-rust2` version bump; if a future version gained
   signature searching, the arm would silently need a gate it does not have.
5. **The pre-flight can be defeated by an un-answerable `statvfs`**, by design. On a filesystem where
   the space question always fails, the multi-GiB-then-ENOSPC failure mode is exactly as it was
   before this change.
6. **libarchive's constituency keeps paying.** Until TicGit `05e2dea4` lands, multi-GB makeself
   installers stage in full under the AD-0040 ceiling. The deferral is honest but it is not free.

## Amendment (2026-09-02, backlog re-triage — the multi-volume claim is untested)

The "Gained" paragraph above states that multi-volume RAR SFX chains now resolve, and calls it a
capability the copy could not have had. **The reasoning holds; the claim is not tested, and it was
written as though it were.** Correcting that here rather than leaving it, because this repository
has a specific history of exactly this failure — a 2026-08-21 reconciliation found three closed
tickets whose described fixes were not in the tree — and a record asserting an unverified capability
is how that history repeats.

What is actually established: staging copies only the first volume into an otherwise empty temporary
directory, so the SDK's volume-change callback has no sibling to find and the chain dead-ends. That
is verifiable from `stage_sfx_payload`, which copies a single byte range, and it is why the failure
is structural rather than a tuning problem. An in-place open leaves UnRAR reading the outer file,
which sits wherever the caller's volumes sit. Both halves are sound.

What is NOT established: that an actual multi-volume RAR SFX opens and extracts end to end. No test
pairs a multi-volume set with an SFX stub — `tests/rar_multivolume_test.rs` covers multi-volume sets
without a stub, and `tests/integration/sfx_in_place_open.rs` covers stubs without a multi-volume set.
Two things would have to be true beyond the offset arithmetic, and neither is checked: that the
volume-naming derivation still finds the siblings when the *outer* filename carries an executable
extension (`installer.exe` rather than `archive.part1.rar`), and that this interacts correctly with
the executable-extension gate every in-place arm requires.

Treat the paragraph above as a plausible consequence of the design, not as a shipped guarantee, until
a test pins it. It is not a reason to doubt the RAR arm itself, which is tested at both the backend
and the facade.

## Amendment (2026-09-03, the "Not added: facade-level integration coverage" bullet is discharged)

The Test-impact section names a real hole — that `tests/integration/sfx_in_place_open.rs`
"still exercises `payload_access()` end to end for ZIP only", leaving the RAR and 7z gates argued in
prose. **That hole is now closed**, and the record is amended rather than edited so the sequence
stays visible.

The file carries facade-level coverage for both new arms:

- `sevenz_sfx_opens_above_the_staging_ceiling` and `open_sfx_reads_a_sevenz_payload_in_place` —
  the 7z arm, which is the one that needed the `PayloadWindow` adapter, observed through
  `Archive::payload_access()` rather than through its parts.
- `rar_sfx_opens_above_the_staging_ceiling` and `an_earlier_rar_marker_in_the_stub_declines_to_staging`
  — the RAR arm, including the decline path, gated on `rar-support` and serialised on the
  process-wide UnRAR lock.

The second RAR test is the load-bearing one: it pins the *decline*, so an over-eager signature scan
that latched onto a marker inside the stub would fail the suite rather than silently open the wrong
bytes. The ZIP decoy case (`decoy_header_at_the_offset_declines_in_place`) does the same for ZIP.
